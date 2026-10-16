#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, Effect},
    dialog::{DialogDecision, PickerPurpose},
};
use hexforge_core::{
    dto::{DirtyState, FileInfoDto, SaveResponse},
    error::{AppError, ErrorCode},
    operations::{CoreReply, CoreRequest},
};
use support::*;

#[test]
fn cancelled_binary_copy_keeps_dirty_bytes() {
    let mut c = opened(3, true);
    let effects = invoke(&mut c, Command::SaveAs);
    let id = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, picker } = e {
                assert_eq!(picker.purpose, PickerPurpose::BinarySave);
                assert_eq!(picker.title, "Save binary as");
                assert_eq!(
                    picker.default_path.unwrap().to_string_lossy(),
                    "source.bin.copy"
                );
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    assert!(snapshot(&c).policy.operation_busy);
    let effects = c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: None,
    });
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert!(snapshot(&c).file.unwrap().dirty);
    assert!(!snapshot(&c).policy.operation_busy);
}

#[test]
fn copy_reopen_failure_keeps_source_active() {
    let mut c = opened(3, true);
    let id = picker(invoke(&mut c, Command::SaveAs));
    let (stamp, request) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("copy.bin".into()),
    }));
    assert!(
        matches!(request, CoreRequest::SaveBinaryAs { path } if path.to_string_lossy() == "copy.bin")
    );
    c.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: Err(AppError::new(
            ErrorCode::IoError,
            "Copy could not be reopened.",
            None,
        )),
    });
    assert_eq!(snapshot(&c).file.as_ref().unwrap().path, "source.bin");
    assert!(snapshot(&c).file.unwrap().dirty);
    assert_eq!(
        snapshot(&c).error.unwrap().message,
        "Copy could not be reopened."
    );
    assert!(!snapshot(&c).policy.operation_busy);
}

#[test]
fn close_barrier_releases_after_cancel() {
    let mut c = opened(3, true);
    let (query, request) = run(invoke(&mut c, Command::Exit));
    assert!(matches!(request, CoreRequest::GetDirtyState));
    let effects = finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "0".into(),
        }),
    );
    let id = effects.into_iter().find_map(|e| if let Effect::ShowDialog { request_id, dialog } = e {
        assert_eq!(dialog.message, "Save unsaved byte edits to \"source.bin\" before exiting HexForge? The original binary will not be overwritten."); Some(request_id)
    } else { None }).unwrap();
    c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Cancel,
    });
    assert!(!snapshot(&c).policy.operation_busy);
    assert!(snapshot(&c).file.unwrap().dirty);
    let effects = invoke(&mut c, Command::ToggleEdit);
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert!(snapshot(&c).edit_mode);
}

#[test]
fn drop_uses_guarded_open() {
    let mut c = opened(3, true);
    let effects = c.handle(ControllerEvent::Action(Action::DropFiles(vec![
        "other.bin".into(),
    ])));
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let id = dialog(effects);
    let (stamp, request) = run(c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Discard,
    }));
    assert!(matches!(
        request,
        CoreRequest::OpenFile {
            discard_unsaved: true,
            ..
        }
    ));
    finish(
        &mut c,
        stamp,
        CoreReply::FileInfo(FileInfoDto {
            name: "other.bin".into(),
            path: "other.bin".into(),
            size: "3".into(),
            revision: "0".into(),
            dirty: false,
        }),
    );
    assert_eq!(snapshot(&c).file.unwrap().name, "other.bin");
    c.handle(ControllerEvent::Action(Action::DropFiles(vec![
        "one.bin".into(),
        "two.bin".into(),
    ])));
    assert_eq!(
        snapshot(&c).error.unwrap().message,
        "Drop exactly one file at a time."
    );
}

#[test]
fn successful_copy_keeps_selection_and_viewport_and_resets_undo() {
    let mut c = opened(64, true);
    c.handle(ControllerEvent::Action(Action::Navigate {
        start: 16,
        end: 17,
    }));
    let source = snapshot(&c).source;
    let id = picker(invoke(&mut c, Command::SaveAs));
    let (stamp, _) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("copy.bin".into()),
    }));
    finish(
        &mut c,
        stamp,
        CoreReply::BinarySaved(SaveResponse {
            bytes_written: "64".into(),
            destination: "copy.bin".into(),
            file: FileInfoDto {
                name: "copy.bin".into(),
                path: "copy.bin".into(),
                size: "64".into(),
                revision: "0".into(),
                dirty: false,
            },
            state: DirtyState {
                dirty: false,
                revision: "0".into(),
            },
        }),
    );
    assert_ne!(source, snapshot(&c).source);
    assert_eq!(snapshot(&c).viewport_offset, 16);
    assert_eq!(snapshot(&c).selection.unwrap().end, 17);
    assert!(!snapshot(&c).can_undo);
    assert!(snapshot(&c).page.is_none());
}

#[test]
fn empty_workspace_exit_needs_no_core_file_query() {
    use hexforge_controller::controller::{AppController, InitialState};
    let mut c = AppController::new(InitialState::default());
    let effects = invoke(&mut c, Command::Exit);
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert!(effects.iter().any(|e| matches!(e, Effect::ExitPrepared)));
}
#[test]
fn authoritative_close_query_failure_keeps_file_and_releases_the_barrier() {
    let mut c = opened(3, true);
    let (query, request) = run(invoke(&mut c, Command::Exit));
    assert!(matches!(request, CoreRequest::GetDirtyState));
    let effects = c.handle(ControllerEvent::CoreCompleted {
        stamp: query,
        reply: Err(AppError::new(ErrorCode::IoError, "state unavailable", None)),
    });
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::ExitPrepared | Effect::CloseWindow { .. })));
    let state = snapshot(&c);
    assert_eq!(state.file.as_ref().unwrap().path, "source.bin");
    assert!(state.file.as_ref().unwrap().dirty);
    assert!(!state.policy.operation_busy);
    assert_eq!(state.error.as_ref().unwrap().message, "state unavailable");
    c.handle(ControllerEvent::Action(Action::ClearError));
    invoke(&mut c, Command::ToggleEdit);
    assert!(
        snapshot(&c).edit_mode,
        "failed close must release the mutation barrier"
    );
}
#[test]
fn repeated_close_requests_wait_for_one_authoritative_query_and_confirmation() {
    let mut c = opened(3, true);
    let (query, _) = run(invoke(&mut c, Command::Exit));
    let duplicate = invoke(&mut c, Command::Exit);
    assert!(
        duplicate
            .iter()
            .all(|effect| matches!(effect, Effect::RequestRepaint)),
        "a duplicate may repaint, but must not start another close workflow: {duplicate:#?}"
    );
    assert!(snapshot(&c).policy.operation_busy);
    assert!(snapshot(&c).dialog.is_none());
    let id = dialog(finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "0".into(),
        }),
    ));
    let duplicate = invoke(&mut c, Command::Exit);
    assert!(
        duplicate
            .iter()
            .all(|effect| matches!(effect, Effect::RequestRepaint)),
        "a duplicate must keep the existing confirmation: {duplicate:#?}"
    );
    assert_eq!(snapshot(&c).dialog.as_ref().unwrap().0, id);
    let effects = c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Discard,
    });
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::ExitPrepared))
            .count(),
        1
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(
                effect,
                Effect::CloseWindow {
                    window: hexforge_controller::shortcuts::WindowKind::Main,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(
        snapshot(&c).file.as_ref().unwrap().dirty,
        "discard permits exit; it does not overwrite source bytes"
    );
}
#[test]
fn close_uses_the_authoritative_reply_not_the_cached_clean_dirty_flag() {
    let mut c = opened(3, false);
    let (query, _) = run(invoke(&mut c, Command::Exit));
    assert!(snapshot(&c).dialog.is_none());
    let effects = finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::ExitPrepared | Effect::CloseWindow { .. })));
    assert!(snapshot(&c).dialog.is_some());
    assert!(snapshot(&c).file.as_ref().unwrap().dirty);
    assert_eq!(snapshot(&c).file.as_ref().unwrap().revision, "1");
}
#[test]
fn clean_authoritative_close_waits_then_approves_exactly_once() {
    let mut c = opened(3, false);
    let effects = invoke(&mut c, Command::Exit);
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::ExitPrepared | Effect::CloseWindow { .. })));
    let (query, request) = run(effects);
    assert!(matches!(request, CoreRequest::GetDirtyState));
    let effects = finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: false,
            revision: "0".into(),
        }),
    );
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::ShowDialog { .. })));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::ExitPrepared))
            .count(),
        1
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(
                effect,
                Effect::CloseWindow {
                    window: hexforge_controller::shortcuts::WindowKind::Main,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(snapshot(&c).dialog.is_none());
}

#[test]
fn save_then_close_waits_for_success_and_cancelled_picker_keeps_file() {
    let mut c = opened(3, true);
    let id = dialog(invoke(&mut c, Command::CloseFile));
    let id = picker(c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Save,
    }));
    let effects = c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: None,
    });
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert!(snapshot(&c).file.unwrap().dirty);
    let id = dialog(invoke(&mut c, Command::CloseFile));
    let id = picker(c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Save,
    }));
    let (save, _) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("copy.bin".into()),
    }));
    let (close, request) = run(finish(
        &mut c,
        save,
        CoreReply::BinarySaved(SaveResponse {
            bytes_written: "3".into(),
            destination: "copy.bin".into(),
            file: FileInfoDto {
                name: "copy.bin".into(),
                path: "copy.bin".into(),
                size: "3".into(),
                revision: "0".into(),
                dirty: false,
            },
            state: DirtyState {
                dirty: false,
                revision: "0".into(),
            },
        }),
    ));
    assert!(matches!(
        request,
        CoreRequest::CloseFile {
            discard_unsaved: false
        }
    ));
    finish(&mut c, close, CoreReply::Unit);
    assert!(snapshot(&c).file.is_none());
    assert!(!snapshot(&c).policy.operation_busy);
}
fn picker(effects: Vec<Effect>) -> hexforge_controller::identity::RequestId {
    effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .expect("picker")
}
fn dialog(effects: Vec<Effect>) -> hexforge_controller::identity::RequestId {
    effects
        .into_iter()
        .find_map(|e| {
            if let Effect::ShowDialog { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .expect("dialog")
}
