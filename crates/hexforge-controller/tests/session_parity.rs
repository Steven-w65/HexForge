//! Source session assertions at the native controller/core boundary. Disk
//! workflows use the real core; async replies are held only to choose ordering.
#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    controller::{AppController, ControllerEvent, Effect, InitialState},
    identity::SourceIdentity,
    operation::OperationKind,
};
use hexforge_core::{
    dto::PageResponse,
    error::{AppError, ErrorCode},
    operations::{CoreReply, CoreRequest, CoreService},
    progress::{OperationPhase, OperationProgress},
    template::TemplateDefinition,
};
use support::*;

fn definition(name: &str) -> TemplateDefinition {
    serde_json::from_value(
        serde_json::json!({"name": name, "defaultEndianness": "little", "fields": [
            {"name": "id", "type": "u8", "placement": {"mode": "sequential"}}
        ]}),
    )
    .unwrap()
}

fn settle(c: &mut AppController, core: &CoreService, effects: Vec<Effect>) {
    let mut pending = std::collections::VecDeque::from(effects);
    while let Some(effect) = pending.pop_front() {
        if let Effect::RunCore { stamp, request } = effect {
            let reply = core.execute(request, &mut |_| {});
            pending.extend(c.handle(ControllerEvent::CoreCompleted { stamp, reply }));
        }
    }
    assert!(snapshot(c).error.is_none());
}
fn real_action(c: &mut AppController, core: &CoreService, action: Action) {
    let effects = c.handle(ControllerEvent::Action(action));
    settle(c, core, effects);
}

#[test]
fn effective_edits_undo_and_saved_copy_publish_exact_marker_deltas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.bin");
    let copy = dir.path().join("copy.bin");
    std::fs::write(&path, vec![0x41; 8192]).unwrap();
    let core = CoreService::default();
    let mut c = AppController::new(InitialState::default());
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::OpenFile {
            path: path.clone(),
            discard_unsaved: false,
        }),
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ReadPage {
            offset: 0,
            length: 32,
        }),
    );
    real_action(&mut c, &core, Action::Navigate { start: 7, end: 7 });
    assert!(snapshot(&c).last_edit_delta.is_none());
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::EditByte {
            offset: 7,
            value: 0xff,
        }),
    );
    let edited = snapshot(&c);
    assert_eq!(edited.last_edit_delta, Some((7, "1".into())));
    assert_eq!(edited.modified_overview.bin_count, 1024);
    assert_eq!(edited.modified_overview.bins, [0]);
    assert!(edited.file.unwrap().dirty);
    assert!(edited.can_undo);
    assert_eq!(edited.page.unwrap().bytes[7], 0xff);
    real_action(&mut c, &core, Action::Core(CoreRequest::UndoEdit));
    let undone = snapshot(&c);
    assert_eq!(undone.last_edit_delta, Some((7, "2".into())));
    assert!(undone.modified_overview.bins.is_empty());
    assert!(!undone.file.unwrap().dirty);
    assert!(!undone.can_undo);
    assert_eq!(undone.page.unwrap().bytes[7], 0x41);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::EditByte {
            offset: 7,
            value: 0xff,
        }),
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::SearchBytes {
            pattern: "41 FF".into(),
        }),
    );
    assert_eq!(snapshot(&c).matches, [6]);
    real_action(&mut c, &core, Action::TemplateChanged(definition("Header")));
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: definition("Header"),
        }),
    );
    assert_eq!(snapshot(&c).results.len(), 1);
    assert!(snapshot(&c).template_applied);
    real_action(&mut c, &core, Action::Navigate { start: 7, end: 7 });
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::SaveBinaryAs { path: copy.clone() }),
    );
    let saved = snapshot(&c);
    assert!(saved.last_edit_delta.is_none());
    assert!(saved.modified_overview.bins.is_empty());
    assert!(!saved.can_undo);
    let file = saved.file.unwrap();
    assert!(!file.dirty);
    assert_eq!(file.revision, "0");
    assert_eq!(
        std::path::Path::new(&file.path),
        std::fs::canonicalize(&copy).unwrap()
    );
    assert_eq!(saved.selection.unwrap().start, 7);
    assert_eq!(saved.viewport_offset, 7);
    assert!(saved.page.is_none());
    assert!(saved.matches.is_empty());
    assert!(saved.results.is_empty());
    assert!(!saved.template_applied);
    assert_eq!(saved.search.match_length, 1);
    assert!(!saved.search.truncated);
    assert_eq!(std::fs::read(path).unwrap()[7], 0x41);
    assert_eq!(std::fs::read(copy).unwrap()[7], 0xff);
}

#[test]
fn no_op_edit_does_not_publish_a_delta_or_create_undo_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.bin");
    std::fs::write(&path, [0x41; 16]).unwrap();
    let core = CoreService::default();
    let mut c = AppController::new(InitialState::default());
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::OpenFile {
            path,
            discard_unsaved: false,
        }),
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::EditByte {
            offset: 3,
            value: 0x41,
        }),
    );
    let s = snapshot(&c);
    assert!(s.last_edit_delta.is_none());
    assert!(!s.can_undo);
    assert!(s.modified_overview.bins.is_empty());
    assert_eq!(s.file.as_ref().unwrap().revision, "0");
    assert!(!s.file.unwrap().dirty);
}

#[test]
fn same_metadata_reopens_advance_source_and_leave_reads_to_view_measurement() {
    let mut c = AppController::new(InitialState::default());
    assert_eq!(snapshot(&c).source, None);
    for identity in [1, 2] {
        let (stamp, _) = run(c.handle(ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: "same.bin".into(),
                discard_unsaved: false,
            },
        ))));
        let effects = finish(
            &mut c,
            stamp,
            CoreReply::FileInfo(hexforge_core::dto::FileInfoDto {
                name: "same.bin".into(),
                path: "same.bin".into(),
                size: "64".into(),
                revision: "0".into(),
                dirty: false,
            }),
        );
        assert_eq!(snapshot(&c).source, Some(SourceIdentity(identity)));
        assert!(snapshot(&c).page.is_none());
        assert!(!effects.iter().any(|e| matches!(
            e,
            Effect::RunCore {
                request: CoreRequest::ReadPage { .. },
                ..
            }
        )));
        assert!(!snapshot(&c)
            .busy
            .get(&OperationKind::Open)
            .copied()
            .unwrap_or(false));
    }
    let (failed, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::OpenFile {
            path: "secret.bin".into(),
            discard_unsaved: false,
        },
    ))));
    let effects = c.handle(ControllerEvent::CoreCompleted {
        stamp: failed,
        reply: Err(AppError::new(
            ErrorCode::PermissionDenied,
            "Permission denied.",
            None,
        )),
    });
    let retained = snapshot(&c);
    assert_eq!(retained.source, Some(SourceIdentity(2)));
    assert_eq!(retained.file.as_ref().unwrap().path, "same.bin");
    assert!(retained.page.is_none());
    assert!(!retained
        .busy
        .get(&OperationKind::Open)
        .copied()
        .unwrap_or(false));
    assert_eq!(
        retained.error.as_ref().unwrap().message,
        "Permission denied."
    );
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::RunCore {
            request: CoreRequest::ReadPage { .. },
            ..
        }
    )));
}

#[test]
fn newer_page_success_owns_generation_even_when_old_success_arrives_last() {
    let mut c = opened(8192, false);
    let (old, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 0,
            length: 16,
        },
    ))));
    let (new, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 4096,
            length: 16,
        },
    ))));
    let page = |offset: &str, byte| {
        CoreReply::Page(PageResponse {
            offset: offset.into(),
            bytes: vec![byte],
            modified_offsets: vec![],
            revision: "0".into(),
        })
    };
    finish(&mut c, new, page("4096", 0x42));
    assert_eq!(snapshot(&c).page.as_ref().unwrap().generation, 2);
    finish(&mut c, old, page("0", 0x41));
    let s = snapshot(&c);
    let accepted = s.page.unwrap();
    assert_eq!(accepted.offset, "4096");
    assert_eq!(accepted.generation, 2);
    assert_eq!(accepted.bytes, [0x42]);
    assert!(s.error.is_none());
    assert!(!s.busy.get(&OperationKind::Page).copied().unwrap_or(false));
}

#[test]
fn pending_edit_refresh_cannot_overwrite_a_newer_viewport_page() {
    let mut c = opened(8192, false);
    let (page, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 0,
            length: 16,
        },
    ))));
    let reply = |offset: &str, revision: &str| {
        CoreReply::Page(PageResponse {
            offset: offset.into(),
            bytes: vec![0x41; 16],
            modified_offsets: vec![],
            revision: revision.into(),
        })
    };
    finish(&mut c, page, reply("0", "0"));
    let (edit, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::EditByte {
            offset: 0,
            value: 0xff,
        },
    ))));
    let effects = finish(
        &mut c,
        edit,
        CoreReply::Dirty(hexforge_core::dto::DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    let refresh = effects
        .into_iter()
        .find_map(|e| match e {
            Effect::RunCore {
                stamp,
                request: CoreRequest::ReadPage { offset: 0, .. },
            } => Some(stamp),
            _ => None,
        })
        .expect("the original edit starts a refresh before the user moves");
    let (new, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 4096,
            length: 16,
        },
    ))));
    finish(&mut c, new, reply("4096", "1"));
    finish(&mut c, refresh, reply("0", "1"));
    let s = snapshot(&c);
    assert_eq!(s.page.unwrap().offset, "4096");
    assert_eq!(s.file.unwrap().revision, "1");
    assert_eq!(s.last_edit_delta.unwrap().0, 0);
    assert!(s.error.is_none());
}

#[test]
fn overlapping_template_loads_install_only_the_latest_file_and_definition() {
    let mut c = opened(64, false);
    let (old, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::LoadTemplate {
            path: "old.json".into(),
        },
    ))));
    let queued = c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::LoadTemplate {
            path: "new.json".into(),
        },
    )));
    assert!(!queued.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let definition = |name| {
        serde_json::from_value::<TemplateDefinition>(serde_json::json!({
            "name": name, "defaultEndianness": "little", "fields": [],
        }))
        .unwrap()
    };
    let (new, request) = run(finish(&mut c, old, CoreReply::Template(definition("Old"))));
    assert!(
        matches!(request, CoreRequest::LoadTemplate { path } if path.to_str() == Some("new.json"))
    );
    assert_ne!(snapshot(&c).template.name, "Old");
    assert!(snapshot(&c).busy[&OperationKind::Template]);
    finish(&mut c, new, CoreReply::Template(definition("New")));
    let s = snapshot(&c);
    assert_eq!(s.template.name, "New");
    assert_eq!(
        s.template_state.saved_path.unwrap().to_str(),
        Some("new.json")
    );
    assert!(!s.template_applied);
    assert!(s.results.is_empty());
    assert!(!s
        .busy
        .get(&OperationKind::Template)
        .copied()
        .unwrap_or(false));
    assert!(s.error.is_none());
}

#[test]
fn template_unload_or_replacement_rejects_old_parse_results_and_progress() {
    for unload in [true, false] {
        let mut c = opened(64, false);
        c.handle(ControllerEvent::Action(Action::TemplateChanged(
            definition("A"),
        )));
        let (old, _) = run(c.handle(ControllerEvent::Action(Action::Core(
            CoreRequest::ApplyTemplate {
                template: definition("A"),
            },
        ))));
        let request = if unload {
            CoreRequest::UnloadTemplate
        } else {
            CoreRequest::LoadTemplate {
                path: "b.json".into(),
            }
        };
        let (replace, _) = run(c.handle(ControllerEvent::Action(Action::Core(request))));
        finish(
            &mut c,
            replace,
            if unload {
                CoreReply::Unit
            } else {
                CoreReply::Template(definition("B"))
            },
        );
        c.handle(ControllerEvent::Progress {
            stamp: old.clone(),
            progress: OperationProgress {
                operation_id: "obsolete-parse".into(),
                phase: OperationPhase::Parse,
                processed: "1".into(),
                total: "1".into(),
            },
        });
        assert!(snapshot(&c).activity.unwrap().progress.is_none());
        finish(&mut c, old, CoreReply::Parsed(vec![leaf("stale")]));
        let s = snapshot(&c);
        assert!(s.results.is_empty());
        assert!(!s.template_applied);
        assert!(s.activity.is_none());
        assert!(s.error.is_none());
        assert_eq!(s.template.name, if unload { "Untitled" } else { "B" });
        if unload {
            assert!(s.template.fields.is_empty());
            assert!(s.template_state.saved_path.is_none());
            assert!(!s.template_state.active);
        } else {
            assert_eq!(
                s.template_state.saved_path.unwrap().to_str(),
                Some("b.json")
            );
        }
    }
}

fn leaf(name: &str) -> hexforge_core::template::ParsedNode {
    hexforge_core::template::ParsedNode {
        kind: "leaf".into(),
        name: name.into(),
        path: name.into(),
        field_type: hexforge_core::template::FieldType::U8,
        offset: Some("0".into()),
        length: Some("1".into()),
        value: Some("41".into()),
        endianness: Some(hexforge_core::template::Endian::Little),
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    }
}

#[test]
fn completed_edit_invalidates_search_success_failure_and_progress() {
    for success in [true, false] {
        let mut c = opened(64, false);
        let (old, _) = run(c.handle(ControllerEvent::Action(Action::Core(
            CoreRequest::SearchBytes {
                pattern: "41".into(),
            },
        ))));
        let (edit, _) = run(c.handle(ControllerEvent::Action(Action::Core(
            CoreRequest::EditByte {
                offset: 0,
                value: 0x42,
            },
        ))));
        finish(
            &mut c,
            edit,
            CoreReply::Dirty(hexforge_core::dto::DirtyState {
                dirty: true,
                revision: "1".into(),
            }),
        );
        c.handle(ControllerEvent::Progress {
            stamp: old.clone(),
            progress: OperationProgress {
                operation_id: "obsolete-search".into(),
                phase: OperationPhase::Search,
                processed: "1".into(),
                total: "1".into(),
            },
        });
        assert!(snapshot(&c).activity.unwrap().progress.is_none());
        let reply = if success {
            Ok(CoreReply::Search(hexforge_core::dto::SearchResponse {
                matches: vec!["0".into()],
                truncated: false,
            }))
        } else {
            Err(AppError::new(
                ErrorCode::IoError,
                "Old search failed.",
                None,
            ))
        };
        c.handle(ControllerEvent::CoreCompleted { stamp: old, reply });
        let s = snapshot(&c);
        assert!(s.matches.is_empty());
        assert!(s.error.is_none());
        assert!(s.activity.is_none());
        assert_eq!(s.file.unwrap().revision, "1");
    }
}

#[test]
fn earlier_successful_edit_invalidates_parse_even_if_queued_undo_is_a_no_op() {
    let mut c = opened(64, false);
    let (parse, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ApplyTemplate {
            template: definition("Header"),
        },
    ))));
    let (edit, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::EditByte {
            offset: 0,
            value: 0x42,
        },
    ))));
    let queued = c.handle(ControllerEvent::Action(Action::Core(CoreRequest::UndoEdit)));
    assert!(!queued.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let (undo, request) = run(finish(
        &mut c,
        edit,
        CoreReply::Dirty(hexforge_core::dto::DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    ));
    assert!(matches!(request, CoreRequest::UndoEdit));
    finish(
        &mut c,
        undo,
        CoreReply::Undo(hexforge_core::dto::UndoResponse {
            undone: false,
            offset: None,
            state: hexforge_core::dto::DirtyState {
                dirty: true,
                revision: "1".into(),
            },
        }),
    );
    finish(&mut c, parse, CoreReply::Parsed(vec![leaf("old")]));
    let s = snapshot(&c);
    assert!(s.results.is_empty());
    assert!(!s.template_applied);
    assert_eq!(s.file.unwrap().revision, "1");
    assert!(s.error.is_none());
}

#[test]
fn queued_opens_keep_the_core_and_controller_at_the_latest_successful_file() {
    for newer_succeeds in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("old.bin");
        let new_path = dir.path().join("new.bin");
        std::fs::write(&old_path, [0x41]).unwrap();
        if newer_succeeds {
            std::fs::write(&new_path, [0x42]).unwrap();
        }
        let core = CoreService::default();
        let mut c = AppController::new(InitialState::default());
        let (old, request) = run(c.handle(ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: old_path.clone(),
                discard_unsaved: false,
            },
        ))));
        let queued = c.handle(ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: new_path.clone(),
                discard_unsaved: false,
            },
        )));
        assert!(!queued.iter().any(|e| matches!(e, Effect::RunCore { .. })));
        let (new, request) = run(c.handle(ControllerEvent::CoreCompleted {
            stamp: old,
            reply: core.execute(request, &mut |_| {}),
        }));
        assert_eq!(snapshot(&c).file.as_ref().unwrap().name, "old.bin");
        assert!(snapshot(&c).busy[&OperationKind::Open]);
        let reply = core.execute(request, &mut |_| {});
        assert_eq!(reply.is_ok(), newer_succeeds);
        let effects = c.handle(ControllerEvent::CoreCompleted { stamp: new, reply });
        assert!(!effects.iter().any(|e| matches!(
            e,
            Effect::RunCore {
                request: CoreRequest::ReadPage { .. },
                ..
            }
        )));
        let s = snapshot(&c);
        assert_eq!(
            s.file.unwrap().name,
            if newer_succeeds { "new.bin" } else { "old.bin" }
        );
        assert_eq!(s.error.is_none(), newer_succeeds);
        assert!(s.page.is_none());
        assert!(!s.busy.get(&OperationKind::Open).copied().unwrap_or(false));
        match core
            .execute(
                CoreRequest::ReadPage {
                    offset: 0,
                    length: 1,
                },
                &mut |_| {},
            )
            .unwrap()
        {
            CoreReply::Page(page) => {
                assert_eq!(page.bytes, [if newer_succeeds { 0x42 } else { 0x41 }])
            }
            _ => panic!("read from the actual active core session"),
        }
    }
}

#[test]
fn queued_replacement_keeps_the_last_successful_edit_undo_or_save_and_retires_its_replies() {
    // The source session rejects late completions after a replacement. Native
    // mutations are serialized instead: assert the real earlier operation is
    // accepted before the replacement, then cannot affect the replacement twice.
    for mutation in ["edit", "undo", "save"] {
        for replacement_succeeds in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let original = dir.path().join("original.bin");
            let replacement = dir.path().join("replacement.bin");
            let copy = dir.path().join("copy.bin");
            std::fs::write(&original, vec![0x41; 8192]).unwrap();
            if replacement_succeeds {
                std::fs::write(&replacement, vec![0x42; 8192]).unwrap();
            }
            let core = CoreService::default();
            let mut c = AppController::new(InitialState::default());
            real_action(
                &mut c,
                &core,
                Action::Core(CoreRequest::OpenFile {
                    path: original.clone(),
                    discard_unsaved: false,
                }),
            );
            if mutation != "edit" {
                real_action(
                    &mut c,
                    &core,
                    Action::Core(CoreRequest::EditByte {
                        offset: 7,
                        value: 0xff,
                    }),
                );
            }
            real_action(
                &mut c,
                &core,
                Action::Core(CoreRequest::SearchBytes {
                    pattern: "41".into(),
                }),
            );
            real_action(&mut c, &core, Action::TemplateChanged(definition("Header")));
            real_action(
                &mut c,
                &core,
                Action::Core(CoreRequest::ApplyTemplate {
                    template: definition("Header"),
                }),
            );
            real_action(
                &mut c,
                &core,
                Action::Core(CoreRequest::ReadPage {
                    offset: 0,
                    length: 32,
                }),
            );
            real_action(&mut c, &core, Action::Navigate { start: 7, end: 7 });
            assert!(snapshot(&c).template_applied);
            assert!(!snapshot(&c).matches.is_empty());
            let before = snapshot(&c).source;
            let request = match mutation {
                "edit" => CoreRequest::EditByte {
                    offset: 7,
                    value: 0xff,
                },
                "undo" => CoreRequest::UndoEdit,
                "save" => CoreRequest::SaveBinaryAs { path: copy.clone() },
                _ => unreachable!(),
            };
            let (old_stamp, request) =
                run(c.handle(ControllerEvent::Action(Action::Core(request))));
            let queued = c.handle(ControllerEvent::Action(Action::Core(
                CoreRequest::OpenFile {
                    path: replacement.clone(),
                    discard_unsaved: true,
                },
            )));
            assert!(!queued.iter().any(|e| matches!(e, Effect::RunCore { .. })));
            assert!(snapshot(&c).busy[&OperationKind::Open]);
            let (open_stamp, open_request) = run(c.handle(ControllerEvent::CoreCompleted {
                stamp: old_stamp.clone(),
                reply: core.execute(request, &mut |_| {}),
            }));
            let intermediate = snapshot(&c);
            assert_eq!(
                intermediate.file.as_ref().unwrap().name,
                if mutation == "save" {
                    "copy.bin"
                } else {
                    "original.bin"
                }
            );
            assert_eq!(
                intermediate.file.as_ref().unwrap().dirty,
                mutation == "edit"
            );
            assert_eq!(intermediate.can_undo, mutation == "edit");
            assert_eq!(intermediate.source == before, mutation != "save");
            assert!(intermediate.matches.is_empty());
            assert!(intermediate.results.is_empty());
            assert!(!intermediate.template_applied);
            assert_eq!(intermediate.selection.unwrap().start, 7);
            let reply = core.execute(open_request, &mut |_| {});
            assert_eq!(reply.is_ok(), replacement_succeeds);
            let mut effects =
                std::collections::VecDeque::from(c.handle(ControllerEvent::CoreCompleted {
                    stamp: open_stamp,
                    reply,
                }));
            while let Some(effect) = effects.pop_front() {
                if let Effect::RunCore { stamp, request } = effect {
                    effects.extend(c.handle(ControllerEvent::CoreCompleted {
                        stamp,
                        reply: core.execute(request, &mut |_| {}),
                    }));
                }
            }
            let after = snapshot(&c);
            assert_eq!(
                after.file.as_ref().unwrap().name,
                if replacement_succeeds {
                    "replacement.bin"
                } else if mutation == "save" {
                    "copy.bin"
                } else {
                    "original.bin"
                }
            );
            assert_eq!(
                after.file.as_ref().unwrap().dirty,
                !replacement_succeeds && mutation == "edit"
            );
            assert_eq!(after.can_undo, !replacement_succeeds && mutation == "edit");
            assert_eq!(after.error.is_none(), replacement_succeeds);
            assert!(!after
                .busy
                .get(&OperationKind::Open)
                .copied()
                .unwrap_or(false));
            assert!(after.activity.is_none());
            if replacement_succeeds {
                assert!(after.page.is_none());
                assert!(after.selection.is_none());
                assert_ne!(after.source, intermediate.source);
            } else {
                assert_eq!(after.source, intermediate.source);
                assert_eq!(after.selection.as_ref().unwrap().start, 7);
                if mutation != "save" {
                    assert_eq!(
                        after.page.as_ref().unwrap().bytes[7],
                        if mutation == "edit" { 0xff } else { 0x41 }
                    );
                }
            }
            let CoreReply::FileInfo(core_file) =
                core.execute(CoreRequest::GetFileInfo, &mut |_| {}).unwrap()
            else {
                panic!("actual active metadata")
            };
            assert_eq!(core_file.path, after.file.as_ref().unwrap().path);
            assert_eq!(core_file.revision, after.file.as_ref().unwrap().revision);
            let CoreReply::Page(page) = core
                .execute(
                    CoreRequest::ReadPage {
                        offset: 0,
                        length: 32,
                    },
                    &mut |_| {},
                )
                .unwrap()
            else {
                panic!("actual active bytes")
            };
            assert_eq!(
                page.bytes[7],
                if replacement_succeeds {
                    0x42
                } else if mutation == "undo" {
                    0x41
                } else {
                    0xff
                }
            );
            assert!(c
                .handle(ControllerEvent::CoreCompleted {
                    stamp: old_stamp,
                    reply: Err(AppError::new(ErrorCode::IoError, "Late old failure.", None))
                })
                .iter()
                .all(|e| !matches!(e, Effect::RunCore { .. })));
            let unchanged = snapshot(&c);
            assert_eq!(unchanged.file.as_ref().unwrap().path, core_file.path);
            assert_eq!(
                unchanged.file.as_ref().unwrap().revision,
                core_file.revision
            );
            assert_eq!(unchanged.source, after.source);
            assert_eq!(
                unchanged.error.map(|e| e.message),
                after.error.map(|e| e.message)
            );
            assert_eq!(std::fs::read(&original).unwrap(), vec![0x41; 8192]);
            if mutation == "save" {
                assert_eq!(std::fs::read(&copy).unwrap()[7], 0xff);
            } else {
                assert!(!copy.exists());
            }
        }
    }
}

#[test]
fn empty_parse_failed_reparse_and_template_replacement_keep_independent_refresh_state() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("source.bin");
    let template_path = dir.path().join("replacement.json");
    std::fs::write(&binary, [0x41; 32]).unwrap();
    let empty: TemplateDefinition = serde_json::from_value(
        serde_json::json!({"name":"Empty","defaultEndianness":"little","fields":[]}),
    )
    .unwrap();
    std::fs::write(
        &template_path,
        serde_json::to_vec(&definition("Loaded header")).unwrap(),
    )
    .unwrap();
    let core = CoreService::default();
    let mut c = AppController::new(InitialState::default());
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::OpenFile {
            path: binary.clone(),
            discard_unsaved: false,
        }),
    );
    real_action(&mut c, &core, Action::TemplateChanged(empty.clone()));
    assert!(!snapshot(&c).template_applied);
    assert!(!snapshot(&c).results_need_refresh);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: empty.clone(),
        }),
    );
    assert!(
        snapshot(&c).template_applied,
        "An empty successful parse is still applied"
    );
    assert!(snapshot(&c).results.is_empty());
    let mut invalid = definition("Bad parse input");
    invalid.fields[0].name = Some(String::new());
    let (stamp, request) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ApplyTemplate { template: invalid },
    ))));
    let reply = core.execute(request, &mut |_| {});
    assert!(reply.is_err());
    c.handle(ControllerEvent::CoreCompleted { stamp, reply });
    assert!(
        snapshot(&c).template_applied,
        "A failed parse cannot become a successful replacement or invalidate the earlier parse"
    );
    assert_eq!(snapshot(&c).template.name, "Empty");
    assert!(!snapshot(&c).results_need_refresh);
    c.handle(ControllerEvent::Action(Action::ClearError));
    real_action(
        &mut c,
        &core,
        Action::TemplateChanged(definition("Edited header")),
    );
    assert!(!snapshot(&c).template_applied);
    assert!(
        !snapshot(&c).results_need_refresh,
        "The source only requests refresh when previous result nodes existed"
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: definition("Edited header"),
        }),
    );
    assert!(snapshot(&c).template_applied);
    assert!(!snapshot(&c).results_need_refresh);
    assert_eq!(snapshot(&c).results[0].value.as_deref(), Some("65"));
    real_action(
        &mut c,
        &core,
        Action::TemplateChanged(definition("Edited again")),
    );
    assert!(snapshot(&c).results_need_refresh);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::LoadTemplate {
            path: template_path.clone(),
        }),
    );
    let loaded = snapshot(&c);
    assert_eq!(loaded.template.name, "Loaded header");
    assert_eq!(
        loaded.template_state.saved_path.as_deref(),
        Some(template_path.as_path())
    );
    assert!(loaded.results.is_empty());
    assert!(!loaded.template_applied);
    assert!(!loaded.results_need_refresh);
    assert!(!loaded.template_state.dirty);
    assert!(loaded.template_range.is_none());
    assert!(!loaded.file.unwrap().dirty);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: definition("Loaded header"),
        }),
    );
    assert!(snapshot(&c).template_applied);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::EditByte {
            offset: 0,
            value: 0x42,
        }),
    );
    assert!(!snapshot(&c).template_applied);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: definition("Loaded header"),
        }),
    );
    assert_eq!(snapshot(&c).results[0].value.as_deref(), Some("66"));
    let copy = dir.path().join("copy.bin");
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::SaveBinaryAs { path: copy.clone() }),
    );
    assert!(!snapshot(&c).template_applied);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: definition("Loaded header"),
        }),
    );
    assert!(snapshot(&c).template_applied);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::CloseFile {
            discard_unsaved: false,
        }),
    );
    assert!(!snapshot(&c).template_applied);
    assert!(snapshot(&c).file.is_none());
    assert!(snapshot(&c).results.is_empty());
    assert!(snapshot(&c).template_state.active);
    assert_eq!(
        snapshot(&c).template_state.saved_path.as_deref(),
        Some(template_path.as_path())
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::OpenFile {
            path: copy.clone(),
            discard_unsaved: false,
        }),
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::ApplyTemplate {
            template: definition("Loaded header"),
        }),
    );
    assert!(snapshot(&c).template_applied);
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::OpenFile {
            path: binary.clone(),
            discard_unsaved: false,
        }),
    );
    assert!(!snapshot(&c).template_applied);
    assert_eq!(
        std::fs::read(copy).unwrap(),
        [0x42].into_iter().chain([0x41; 31]).collect::<Vec<_>>()
    );
    assert_eq!(std::fs::read(binary).unwrap(), [0x41; 32]);
}

#[test]
fn exact_decimal_placement_survives_real_load_save_apply_and_csv_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("small.bin");
    let template_path = dir.path().join("exact.json");
    let csv = dir.path().join("out.csv");
    std::fs::write(&binary, [0x41; 32]).unwrap();
    let definition: TemplateDefinition = serde_json::from_value(serde_json::json!({
        "name":"Exact", "defaultEndianness":"big", "fields":[
            {"name":"magic", "type":"u16", "placement":{"mode":"absolute","offset":"9007199254740993"}}
        ]
    })).unwrap();
    std::fs::write(&template_path, serde_json::to_vec(&definition).unwrap()).unwrap();
    let exact = |value: &TemplateDefinition| {
        assert_eq!(
            serde_json::to_value(value).unwrap()["fields"][0]["placement"]["offset"],
            "9007199254740993"
        );
    };
    let core = CoreService::default();
    let mut c = AppController::new(InitialState::default());
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::OpenFile {
            path: binary.clone(),
            discard_unsaved: false,
        }),
    );
    real_action(
        &mut c,
        &core,
        Action::Core(CoreRequest::LoadTemplate {
            path: template_path.clone(),
        }),
    );
    exact(&snapshot(&c).template);
    let mut changed = definition;
    changed.name = "Exact saved".into();
    real_action(&mut c, &core, Action::TemplateChanged(changed.clone()));
    let (stamp, request) = run(invoke(
        &mut c,
        hexforge_controller::command::Command::SaveTemplate,
    ));
    let CoreRequest::SaveTemplate { template, .. } = &request else {
        panic!("in-place template save")
    };
    exact(template);
    let effects = c.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: core.execute(request, &mut |_| {}),
    });
    settle(&mut c, &core, effects);
    let persisted: TemplateDefinition =
        serde_json::from_slice(&std::fs::read(&template_path).unwrap()).unwrap();
    exact(&persisted);
    assert_eq!(persisted.name, "Exact saved");
    assert!(!snapshot(&c).template_state.dirty);
    assert!(!snapshot(&c).template_applied);
    let (stamp, request) = run(invoke(
        &mut c,
        hexforge_controller::command::Command::ApplyTemplate,
    ));
    let CoreRequest::ApplyTemplate { template } = &request else {
        panic!("explicit template apply")
    };
    exact(template);
    let effects = c.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: core.execute(request, &mut |_| {}),
    });
    settle(&mut c, &core, effects);
    let result = &snapshot(&c).results[0];
    assert_eq!(result.path, "magic");
    assert!(result.value.is_none());
    assert_eq!(result.diagnostics[0].code, "template_out_of_bounds");
    assert!(
        !snapshot(&c).policy.has_parsed_results,
        "Diagnostics are not successful CSV leaves"
    );
    let request = CoreRequest::ExportResultsCsv {
        path: csv.clone(),
        template: snapshot(&c).template,
    };
    let CoreRequest::ExportResultsCsv { template, .. } = &request else {
        unreachable!()
    };
    exact(template);
    assert!(core.execute(request, &mut |_| {}).is_err());
    assert!(!csv.exists());
    assert_eq!(std::fs::read(binary).unwrap(), [0x41; 32]);
    assert!(serde_json::to_value(persisted)
        .unwrap()
        .get("version")
        .is_none());
}
