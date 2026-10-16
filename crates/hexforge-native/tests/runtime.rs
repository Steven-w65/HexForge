mod support;
use hexforge_controller::{
    action::Action,
    controller::{AppController, ControllerEvent, Effect, InitialState},
    identity::RequestStamp,
};
use hexforge_core::operations::{CoreRequest, CoreService};
use hexforge_native::runtime::Runtime;
use std::{
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};
fn request() -> (RequestStamp, Effect) {
    let mut c = AppController::new(InitialState::default());
    let e = c
        .handle(ControllerEvent::Action(Action::Core(
            CoreRequest::GetFileInfo,
        )))
        .into_iter()
        .find(|e| matches!(e, Effect::RunCore { .. }))
        .unwrap();
    let stamp = match &e {
        Effect::RunCore { stamp, .. } => stamp.clone(),
        _ => unreachable!(),
    };
    (stamp, e)
}
#[test]
fn worker_result_preserves_stamp() {
    let core = Arc::new(CoreService::default());
    let mut r = Runtime::new(core);
    let (stamp, e) = request();
    r.submit(e);
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(ControllerEvent::CoreCompleted { stamp: s, reply }) = r
            .drain_events()
            .into_iter()
            .find(|e| matches!(e, ControllerEvent::CoreCompleted { .. }))
        {
            assert_eq!(s, stamp);
            assert_eq!(reply.unwrap_err().code(), "no_active_file");
            break;
        }
        assert!(Instant::now() < until);
        thread::yield_now();
    }
}
#[test]
fn long_operation_does_not_block_presenter() {
    let core = Arc::new(CoreService::default());
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = core.clone();
    let guard_thread = thread::spawn(move || {
        let _guard = worker.session.lock().unwrap();
        held_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_millis(700));
    });
    held_rx.recv().unwrap();
    let mut r = Runtime::new(core);
    let (_, e) = request();
    let start = Instant::now();
    r.submit(e);
    let h = support::ui_harness::NativeUiHarness::new();
    let mut frames = 0;
    for _ in 0..4 {
        h.frame(Default::default(), |ui| {
            ui.label("HexForge");
            frames += 1;
        });
        r.drain_events();
    }
    assert_eq!(frames, 4);
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "worker blocked native widget processing"
    );
    release_tx.send(()).unwrap();
    guard_thread.join().unwrap();
}
#[test]
fn picker_cancel_is_a_completion() {
    use hexforge_controller::{dialog::PickerRequest, identity::RequestId};
    let mut r = Runtime::new(Arc::new(CoreService::default()));
    r.submit(Effect::OpenPicker {
        request_id: RequestId(9),
        picker: PickerRequest::binary_open(),
    });
    assert_eq!(r.take_platform_effects().len(), 1);
    r.picker_finished(RequestId(9), Ok(None));
    assert!(matches!(
        r.drain_events().as_slice(),
        [ControllerEvent::PickerCompleted {
            request_id: RequestId(9),
            path: None
        }]
    ));
}
#[test]
fn presentation_thread_completions_never_disappear_when_channel_is_full() {
    use hexforge_controller::identity::RequestId;
    let mut runtime = Runtime::new(Arc::new(CoreService::default()));
    for id in 1..=400 {
        runtime.picker_finished(RequestId(id), Ok(None));
    }
    let mut ids: Vec<_> = runtime
        .drain_events()
        .into_iter()
        .map(|e| match e {
            ControllerEvent::PickerCompleted {
                request_id,
                path: None,
            } => request_id.0,
            _ => panic!("unexpected completion"),
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (1..=400).collect::<Vec<_>>());
}
#[test]
fn saturated_worker_queue_settles_every_request_without_blocking_ui() {
    let core = Arc::new(CoreService::default());
    let guard = core.session.lock().unwrap();
    let mut runtime = Runtime::new(core.clone());
    let (base, _) = request();
    let start = Instant::now();
    for id in 1..=400 {
        let mut stamp = base.clone();
        stamp.request_id = hexforge_controller::identity::RequestId(id);
        runtime.submit(Effect::RunCore {
            stamp,
            request: CoreRequest::GetFileInfo,
        });
    }
    assert!(start.elapsed() < Duration::from_millis(200));
    drop(guard);
    let mut ids = std::collections::BTreeSet::new();
    let until = Instant::now() + Duration::from_secs(3);
    while ids.len() < 400 && Instant::now() < until {
        for event in runtime.drain_events() {
            if let ControllerEvent::CoreCompleted { stamp, .. } = event {
                assert!(ids.insert(stamp.request_id.0));
            }
        }
        thread::yield_now();
    }
    assert_eq!(
        ids.len(),
        400,
        "a failed queue submission left an operation hanging"
    );
}
