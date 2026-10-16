#![allow(dead_code)]
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{
        AppController, ControllerEvent, Effect, InitialState, InvokeContext, MainSnapshot,
        WindowSnapshot,
    },
    identity::RequestStamp,
    shortcuts::WindowKind,
};
use hexforge_core::{
    dto::FileInfoDto,
    operations::{CoreReply, CoreRequest},
};

pub fn run(effects: Vec<Effect>) -> (RequestStamp, CoreRequest) {
    effects
        .into_iter()
        .find_map(|e| {
            if let Effect::RunCore { stamp, request } = e {
                Some((stamp, request))
            } else {
                None
            }
        })
        .expect("core effect")
}
pub fn invoke(c: &mut AppController, command: Command) -> Vec<Effect> {
    c.handle(ControllerEvent::Invoke {
        command,
        context: InvokeContext::default(),
    })
}
pub fn finish(c: &mut AppController, stamp: RequestStamp, reply: CoreReply) -> Vec<Effect> {
    c.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: Ok(reply),
    })
}
pub fn snapshot(c: &AppController) -> MainSnapshot {
    if let WindowSnapshot::Main(s) = c.snapshot(WindowKind::Main) {
        s
    } else {
        panic!("main")
    }
}
pub fn opened(size: u64, dirty: bool) -> AppController {
    let mut c = AppController::new(InitialState::default());
    let (stamp, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::OpenFile {
            path: "source.bin".into(),
            discard_unsaved: false,
        },
    ))));
    finish(
        &mut c,
        stamp,
        CoreReply::FileInfo(FileInfoDto {
            name: "source.bin".into(),
            path: "source.bin".into(),
            size: size.to_string(),
            revision: "0".into(),
            dirty,
        }),
    );
    c
}
