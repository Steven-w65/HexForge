//! Native effects never execute core work on the presentation thread. The
//! controller retains original mutation/lock policy; this is only an adapter.
use hexforge_controller::{
    controller::{ControllerEvent, Effect},
    hex::minimap_data::MinimapTicket,
    identity::{RequestId, RequestStamp},
};
use hexforge_core::{
    error::AppError,
    operations::{operation_failed, CoreReply, CoreRequest, CoreService},
};
use std::{
    collections::VecDeque,
    sync::{
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
};
enum Job {
    Core {
        stamp: RequestStamp,
        request: CoreRequest,
    },
    Minimap {
        ticket: MinimapTicket,
        request: CoreRequest,
    },
}
enum Message {
    Controller(ControllerEvent),
    Minimap {
        ticket: MinimapTicket,
        result: Result<CoreReply, AppError>,
    },
}
pub struct Runtime {
    core: Arc<CoreService>,
    jobs: Option<SyncSender<Job>>,
    sender: SyncSender<Message>,
    receiver: Receiver<Message>,
    ready: VecDeque<Message>,
    platform: VecDeque<Effect>,
    minimap: Vec<(MinimapTicket, Result<CoreReply, AppError>)>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl Runtime {
    pub fn new(core: Arc<CoreService>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(256);
        Self {
            core,
            jobs: None,
            sender,
            receiver,
            ready: VecDeque::new(),
            platform: VecDeque::new(),
            minimap: vec![],
            wake: Arc::new(|| {}),
        }
    }
    pub fn set_waker(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake = Arc::new(wake);
    }
    fn workers(&mut self) -> Result<&SyncSender<Job>, AppError> {
        if self.jobs.is_none() {
            let (tx, rx) = mpsc::sync_channel::<Job>(64);
            let queue = Arc::new(Mutex::new(rx));
            for index in 0..3 {
                let queue = queue.clone();
                let core = self.core.clone();
                let sender = self.sender.clone();
                let wake = self.wake.clone();
                thread::Builder::new()
                    .name(format!("hexforge-core-{index}"))
                    .spawn(move || loop {
                        let job = {
                            let Ok(q) = queue.lock() else {
                                break;
                            };
                            q.recv()
                        };
                        let Ok(job) = job else {
                            break;
                        };
                        let message = match job {
                            Job::Core { stamp, request } => {
                                let progress_stamp = stamp.clone();
                                let progress_tx = sender.clone();
                                let progress_wake = wake.clone();
                                let result =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        core.execute(request, &mut |progress| {
                                            if progress_tx
                                                .try_send(Message::Controller(
                                                    ControllerEvent::Progress {
                                                        stamp: progress_stamp.clone(),
                                                        progress,
                                                    },
                                                ))
                                                .is_ok()
                                            {
                                                progress_wake();
                                            }
                                        })
                                    }))
                                    .unwrap_or_else(|_| Err(operation_failed()));
                                Message::Controller(ControllerEvent::CoreCompleted {
                                    stamp,
                                    reply: result,
                                })
                            }
                            Job::Minimap { ticket, request } => {
                                let result =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        core.execute(request, &mut |_| {})
                                    }))
                                    .unwrap_or_else(|_| Err(operation_failed()));
                                Message::Minimap { ticket, result }
                            }
                        };
                        if sender.send(message).is_err() {
                            break;
                        }
                        wake();
                    })
                    .map_err(|_| operation_failed())?;
            }
            self.jobs = Some(tx);
        }
        Ok(self.jobs.as_ref().unwrap())
    }
    // A UI-thread completion must not be sent into the same bounded channel
    // that only that thread drains: blocking deadlocks, try_send loses it.
    // Only lossy progress uses try_send; terminal outcomes always settle.
    fn failed_job(&mut self, job: Job) {
        let event = match job {
            Job::Core { stamp, .. } => Message::Controller(ControllerEvent::CoreCompleted {
                stamp,
                reply: Err(operation_failed()),
            }),
            Job::Minimap { ticket, .. } => Message::Minimap {
                ticket,
                result: Err(operation_failed()),
            },
        };
        self.ready.push_back(event);
        (self.wake)();
    }
    fn queue(&mut self, job: Job) {
        let tx = match self.workers() {
            Ok(tx) => tx.clone(),
            Err(_) => {
                self.failed_job(job);
                return;
            }
        };
        if let Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) = tx.try_send(job) {
            self.failed_job(job);
        }
    }
    pub fn submit(&mut self, effect: Effect) {
        match effect {
            Effect::RunCore { stamp, request } => self.queue(Job::Core { stamp, request }),
            other => self.platform.push_back(other),
        }
    }
    pub fn submit_minimap(&mut self, ticket: MinimapTicket, request: CoreRequest) {
        self.queue(Job::Minimap { ticket, request });
    }
    pub fn drain_events(&mut self) -> Vec<ControllerEvent> {
        let mut events = vec![];
        for message in self
            .ready
            .drain(..)
            .chain(self.receiver.try_iter().take(256))
        {
            match message {
                Message::Controller(e) => events.push(e),
                Message::Minimap { ticket, result } => self.minimap.push((ticket, result)),
            }
        }
        events
    }
    pub fn take_minimap_events(&mut self) -> Vec<(MinimapTicket, Result<CoreReply, AppError>)> {
        std::mem::take(&mut self.minimap)
    }
    pub fn take_platform_effects(&mut self) -> Vec<Effect> {
        self.platform.drain(..).collect()
    }
    pub fn picker_finished(
        &mut self,
        request_id: RequestId,
        path: Result<Option<std::path::PathBuf>, AppError>,
    ) {
        let event = match path {
            Ok(path) => ControllerEvent::PickerCompleted { request_id, path },
            Err(error) => ControllerEvent::PickerFailed { request_id, error },
        };
        self.ready.push_back(Message::Controller(event));
        (self.wake)();
    }
    pub fn picker_sender(
        &self,
    ) -> impl Fn(RequestId, Result<Option<std::path::PathBuf>, AppError>) + Send + 'static {
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        move |request_id, result| {
            let event = match result {
                Ok(path) => ControllerEvent::PickerCompleted { request_id, path },
                Err(error) => ControllerEvent::PickerFailed { request_id, error },
            };
            let _ = sender.send(Message::Controller(event));
            wake();
        }
    }
}
