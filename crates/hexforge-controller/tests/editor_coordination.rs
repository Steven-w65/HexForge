#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{AppController, ControllerEvent, Effect, InitialState, WindowSnapshot},
    dialog::DialogDecision,
    identity::RequestId,
    shortcuts::WindowKind,
    template::{
        coordinator::{
            DraftUpdate, EditorAction, EditorBuffer, EditorCloseDecision, EditorEnvelope,
            EditorMessage, EditorSessionId,
        },
        workspace::TemplateWorkspace,
    },
};
use hexforge_core::{
    dto::{DirtyState, FileInfoDto, SaveResponse},
    operations::{CoreReply, CoreRequest},
    template::TemplateDefinition,
};
use support::*;

fn definition(name: &str) -> TemplateDefinition {
    serde_json::from_value(serde_json::json!({"name":name,"defaultEndianness":"little","fields":[{"name":"id","type":"u8","placement":{"mode":"sequential"}}]})).unwrap()
}

#[test]
fn editor_cancelled_save_as_and_disk_full_reply_do_not_complete_or_advance_the_draft() {
    use hexforge_core::error::{AppError, ErrorCode};
    for disk_full in [false, true] {
        let mut c = opened(16, true);
        let session = open_editor(&mut c);
        let changed = draft(&c, session, 1, "Keep this unsaved draft", true);
        c.handle(ControllerEvent::Editor(EditorMessage::Draft(
            changed.clone(),
        )));
        let before = snapshot(&c);
        let actions = c.handle(ControllerEvent::Editor(EditorMessage::Action {
            request_id: RequestId(900),
            draft: changed,
            action: EditorAction::Command(Command::SaveTemplateAs),
        }));
        let picker = actions
            .into_iter()
            .find_map(|e| match e {
                Effect::OpenPicker { request_id, .. } => Some(request_id),
                _ => None,
            })
            .unwrap();
        let effects = if disk_full {
            let (save, request) = run(c.handle(ControllerEvent::PickerCompleted {
                request_id: picker,
                path: Some("draft.json".into()),
            }));
            assert!(matches!(request, CoreRequest::SaveTemplateAs { .. }));
            // Fault injection at the typed worker-completion boundary avoids
            // filling a real user's disk just to exercise this failure path.
            c.handle(ControllerEvent::CoreCompleted {
                stamp: save,
                reply: Err(AppError::new(
                    ErrorCode::DiskFull,
                    "Not enough disk space to save this template.",
                    None,
                )),
            })
        } else {
            c.handle(ControllerEvent::PickerCompleted {
                request_id: picker,
                path: None,
            })
        };
        assert!(effects.iter().any(|e| matches!(e,
            Effect::EditorReply { session: owner, request_id: RequestId(900), success: false }
                if *owner == session
        )));
        assert!(!effects.iter().any(|e| matches!(
            e,
            Effect::CloseWindow { .. }
                | Effect::RunCore {
                    request: CoreRequest::ApplyTemplate { .. },
                    ..
                }
        )));
        if disk_full {
            assert!(effects.iter().any(|e| matches!(e,
                Effect::EditorError { session: owner, message }
                    if *owner == session && message.contains("Not enough disk space")
            )));
        }
        let after = snapshot(&c);
        assert_eq!(after.template, before.template);
        assert_eq!(
            after.template_state.baseline,
            before.template_state.baseline
        );
        assert_eq!(
            after.template_state.checkpoint_definition,
            before.template_state.checkpoint_definition
        );
        assert_eq!(
            after.template_state.persistence_revision,
            before.template_state.persistence_revision
        );
        assert_eq!(
            after.template_state.workspace_revision,
            before.template_state.workspace_revision
        );
        assert!(after.template_state.dirty);
        assert!(after.template_state.saved_path.is_none());
        assert!(!after.template_state.busy);
        assert!(!after.template_applied);
        assert_eq!(after.editor_session, Some(session));
        assert_eq!(after.source, before.source);
        assert!(after.file.unwrap().dirty);
    }
}

#[test]
fn template_only_main_close_protects_both_valid_edits_and_unchanged_json_with_rejected_input() {
    for rejected_input in [false, true] {
        let mut c = AppController::new(InitialState::default());
        let session = open_editor(&mut c);
        let original = snapshot(&c).template;
        let latest = DraftUpdate {
            session,
            sequence: 1,
            workspace_revision: snapshot(&c).template_state.workspace_revision,
            definition: if rejected_input {
                original.clone()
            } else {
                definition("Unsaved template")
            },
            valid: !rejected_input,
            pending_inputs: rejected_input,
        };
        c.handle(ControllerEvent::Editor(EditorMessage::Draft(
            latest.clone(),
        )));
        let before = snapshot(&c);
        assert!(before.file.is_none());
        if rejected_input {
            assert_eq!(before.template, original);
        }
        for decision in [DialogDecision::Cancel, DialogDecision::Confirm] {
            let mut effects = c.handle(ControllerEvent::MainCloseRequested);
            // Native coordination has no IPC subscription window. Still honor
            // each editor flush and the separate post-binary-close checkpoint.
            for _ in 0..3 {
                let pending = effects.iter().find_map(|e| match e {
                    Effect::FlushEditorDraft {
                        request_id,
                        session: owner,
                    } => {
                        assert_eq!(*owner, session);
                        Some(*request_id)
                    }
                    _ => None,
                });
                let Some(id) = pending else { break };
                assert!(!effects
                    .iter()
                    .any(|e| matches!(e, Effect::RunCore { .. } | Effect::CloseWindow { .. })));
                effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
                    request_id: id,
                    draft: latest.clone(),
                }));
            }
            let confirmation = effects
                .iter()
                .find_map(|e| match e {
                    Effect::ShowDialog { request_id, dialog } => {
                        assert_eq!(dialog.owner, WindowKind::Main);
                        assert_eq!(
                            dialog.message,
                            "Unsaved template changes will be lost. Exit HexForge?"
                        );
                        Some(*request_id)
                    }
                    _ => None,
                })
                .expect("even unchanged valid JSON must not hide rejected unsaved text");
            assert!(!effects
                .iter()
                .any(|e| matches!(e, Effect::RunCore { .. } | Effect::CloseWindow { .. })));
            let approved = c.handle(ControllerEvent::DialogCompleted {
                request_id: confirmation,
                decision,
            });
            assert_eq!(snapshot(&c).template, before.template);
            assert_eq!(
                snapshot(&c).template_state.saved_path,
                before.template_state.saved_path
            );
            assert_eq!(snapshot(&c).editor_session, Some(session));
            if decision == DialogDecision::Cancel {
                assert!(!approved
                    .iter()
                    .any(|e| matches!(e, Effect::CloseWindow { .. })));
                assert!(!snapshot(&c).policy.operation_busy);
                assert_eq!(snapshot(&c).template_state.valid, !rejected_input);
            } else {
                assert!(approved.iter().any(|e| matches!(e,
                    Effect::CloseWindow { window: WindowKind::TemplateEditor, session: Some(owner) } if *owner == session
                )));
                assert!(!approved.iter().any(|e| matches!(
                    e,
                    Effect::CloseWindow {
                        window: WindowKind::Main,
                        ..
                    }
                )));
                let destroyed =
                    c.handle(ControllerEvent::Editor(EditorMessage::Closed { session }));
                assert!(destroyed.iter().any(|e| matches!(
                    e,
                    Effect::CloseWindow {
                        window: WindowKind::Main,
                        ..
                    }
                )));
            }
        }
    }
}
fn open_editor(c: &mut AppController) -> EditorSessionId {
    let session = invoke(c, Command::TemplateEditor)
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenOrFocusEditor { session } = e {
                Some(session)
            } else {
                None
            }
        })
        .expect("editor opens");
    c.handle(ControllerEvent::Editor(EditorMessage::Ready { session }));
    session
}
fn draft(
    c: &AppController,
    session: EditorSessionId,
    sequence: u64,
    name: &str,
    valid: bool,
) -> DraftUpdate {
    DraftUpdate {
        session,
        sequence,
        workspace_revision: snapshot(c).template_state.workspace_revision,
        definition: definition(name),
        valid,
        pending_inputs: !valid,
    }
}
fn flush(effects: Vec<Effect>) -> (RequestId, EditorSessionId) {
    effects
        .into_iter()
        .find_map(|e| {
            if let Effect::FlushEditorDraft {
                request_id,
                session,
            } = e
            {
                Some((request_id, session))
            } else {
                None
            }
        })
        .expect("flush barrier")
}
fn dialog_id(effects: Vec<Effect>) -> RequestId {
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

#[test]
fn open_or_focus_keeps_pending_draft() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    let changed = draft(&c, session, 1, "Local draft", true);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(changed)));
    let effects = invoke(&mut c, Command::TemplateEditor);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::OpenOrFocusEditor { session: s } if *s == session)));
    assert_eq!(snapshot(&c).template.name, "Local draft");
}

#[test]
fn native_main_close_flushes_the_editor_then_waits_for_a_started_byte_edit() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    c.handle(ControllerEvent::Action(Action::Select { start: 0, end: 0 }));
    invoke(&mut c, Command::ToggleEdit);
    invoke(&mut c, Command::EditSelected);
    let (edit, request) = run(c.handle(ControllerEvent::Action(Action::EditSelectedByte(
        "FF".into(),
    ))));
    assert!(matches!(
        request,
        CoreRequest::EditByte {
            offset: 0,
            value: 255
        }
    ));
    assert!(snapshot(&c).edit_dialog.as_ref().unwrap().pending);
    assert!(
        !invoke(&mut c, Command::Exit)
            .iter()
            .any(|e| matches!(e, Effect::RunCore { .. } | Effect::FlushEditorDraft { .. })),
        "ordinary Exit command must still obey the byte-dialog modal rule"
    );
    let (flush_id, owner) = flush(c.handle(ControllerEvent::MainCloseRequested));
    assert_eq!(owner, session);
    assert!(
        !c.handle(ControllerEvent::MainCloseRequested)
            .iter()
            .any(|e| matches!(e, Effect::RunCore { .. } | Effect::FlushEditorDraft { .. })),
        "a second native X must not create a second flush/close attempt"
    );
    let current = snapshot(&c).template;
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: flush_id,
        draft: DraftUpdate {
            session,
            sequence: 1,
            workspace_revision: snapshot(&c).template_state.workspace_revision,
            definition: current.clone(),
            valid: false,
            pending_inputs: true,
        },
    }));
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::RunCore { .. })),
        "authoritative dirty query must wait for the already-started byte edit"
    );
    let (query, request) = run(finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    ));
    assert!(matches!(request, CoreRequest::GetDirtyState));
    let id = dialog_id(finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    ));
    c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Cancel,
    });
    let state = snapshot(&c);
    assert_eq!(
        state.template, current,
        "rejected raw editor input must not rewrite valid JSON"
    );
    assert!(state.template_state.dirty);
    assert!(!state.template_state.valid);
    assert_eq!(state.editor_session, Some(session));
    assert!(state.file.as_ref().unwrap().dirty);
    assert!(!state.policy.operation_busy);
}

#[test]
fn failed_native_close_flush_keeps_both_windows_and_the_unsaved_template() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(draft(
        &c,
        session,
        1,
        "Keep this draft",
        true,
    ))));
    let (id, _) = flush(c.handle(ControllerEvent::MainCloseRequested));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushFailed {
        request_id: id,
        message: "Template Editor did not respond. Its draft is still open.".into(),
    }));
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::CloseWindow { .. } | Effect::RunCore { .. })));
    let state = snapshot(&c);
    assert_eq!(state.editor_session, Some(session));
    assert_eq!(state.template.name, "Keep this draft");
    assert!(state.template_state.dirty);
    assert!(state.file.is_some());
    assert!(!state.policy.operation_busy);
    assert_eq!(
        state.error.unwrap().message,
        "Template Editor did not respond. Its draft is still open."
    );
    c.handle(ControllerEvent::Action(Action::ClearError));
    assert!(
        c.handle(ControllerEvent::MainCloseRequested)
            .iter()
            .any(|e| matches!(e, Effect::FlushEditorDraft { .. })),
        "failure must release the close guard so the user can retry safely"
    );
}

#[test]
fn ready_snapshots_and_draft_acknowledgements_share_one_authoritative_workspace() {
    let mut c = AppController::new(InitialState::default());
    let session = open_editor(&mut c);
    let envelope = |c: &AppController| {
        let WindowSnapshot::Editor(s) = c.snapshot(WindowKind::TemplateEditor) else {
            unreachable!()
        };
        EditorEnvelope {
            session: s.session.unwrap(),
            revision: s.snapshot_revision,
            acknowledged_sequence: s.acknowledged_sequence,
            workspace: s.template_state,
        }
    };
    let initial = envelope(&c);
    assert_eq!(initial.workspace.definition.name, "Untitled");
    assert_eq!(initial.acknowledged_sequence, 0);
    let mut buffer = EditorBuffer::new(initial);
    buffer.edit(definition("Header"), true, false).unwrap();
    assert!(!buffer.acknowledged());
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(
        buffer.draft(),
    )));
    let published = envelope(&c);
    assert_eq!(published.workspace.definition.name, "Header");
    assert_eq!(published.acknowledged_sequence, 1);
    assert!(published.workspace.dirty);
    assert!(published.workspace.saved_path.is_none());
    assert!(!published.workspace.applied);
    assert!(buffer.apply_snapshot(published));
    assert!(buffer.acknowledged());
    buffer
        .edit(definition("Newer local input"), true, false)
        .unwrap();
    assert!(!buffer.acknowledged());
    // Repeated ready/publication is not a draft reset or an acknowledgement of
    // local input that has not reached the main controller yet.
    c.handle(ControllerEvent::Editor(EditorMessage::Ready { session }));
    assert!(buffer.apply_snapshot(envelope(&c)));
    assert_eq!(buffer.draft().definition.name, "Newer local input");
    assert!(!buffer.acknowledged());
    assert_eq!(snapshot(&c).template.name, "Header");
}

#[test]
fn discarded_workspaces_reject_late_editor_drafts_and_file_actions() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Before replacement"),
    )));
    let session = open_editor(&mut c);
    let old = draft(&c, session, 1, "Discarded local input", true);
    let previous = old.workspace_revision;
    let (unload, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::UnloadTemplate,
    ))));
    finish(&mut c, unload, CoreReply::Unit);
    let fresh = snapshot(&c);
    assert!(fresh.template_state.workspace_revision > previous);
    assert_eq!(fresh.template.name, "Untitled");
    assert!(fresh.template.fields.is_empty());
    assert!(!fresh.template_state.active);
    assert!(fresh.template_state.saved_path.is_none());
    assert!(fresh.results.is_empty());
    assert!(fresh.template_range.is_none());
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(old.clone())));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::Action {
        request_id: RequestId(77),
        draft: old,
        action: EditorAction::Command(Command::SaveTemplateAs),
    }));
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::EditorReply {
            request_id: RequestId(77),
            success: false,
            ..
        }
    )));
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::RunCore { .. }
            | Effect::OpenPicker { .. }
            | Effect::FlushEditorDraft { .. }
            | Effect::CloseWindow { .. }
    )));
    let after = snapshot(&c);
    assert_eq!(after.template.name, "Untitled");
    assert_eq!(
        after.template_state.workspace_revision,
        fresh.template_state.workspace_revision
    );
    assert_eq!(after.editor_session, Some(session));
    assert!(!after.template_state.dirty);
    assert!(after.results.is_empty());
}

#[test]
fn flush_precedes_apply_load_close() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Before"),
    )));
    let session = open_editor(&mut c);
    let effects = invoke(&mut c, Command::ApplyTemplate);
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let (id, _) = flush(effects);
    let updated = draft(&c, session, 1, "After flush", true);
    let (parse, request) = run(c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: updated,
    })));
    assert!(
        matches!(request, CoreRequest::ApplyTemplate { template } if template.name == "After flush")
    );
    finish(&mut c, parse, CoreReply::Parsed(vec![]));
    let effects = invoke(&mut c, Command::LoadTemplate);
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::OpenPicker { .. })));
    let (id, _) = flush(effects);
    let updated = draft(&c, session, 1, "After flush", true);
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: updated,
    }));
    let picker = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    c.handle(ControllerEvent::PickerCompleted {
        request_id: picker,
        path: None,
    });
    let (id, _) = flush(
        c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
            session,
        })),
    );
    let updated = draft(&c, session, 1, "After flush", true);
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: updated,
    }));
    assert!(effects.iter().any(|e| matches!(e, Effect::ShowDialog { dialog, .. } if dialog.title == "Unsaved template edits")));
}

#[test]
fn stale_snapshot_cannot_restore_discarded_draft() {
    let workspace = TemplateWorkspace::default();
    let session = EditorSessionId(1);
    let initial = EditorEnvelope {
        session,
        revision: 1,
        acknowledged_sequence: 0,
        workspace: workspace.snapshot(),
    };
    let mut buffer = EditorBuffer::new(initial);
    buffer
        .edit(definition("Typed locally"), true, false)
        .unwrap();
    let mut theme_update = EditorEnvelope {
        session,
        revision: 2,
        acknowledged_sequence: 0,
        workspace: workspace.snapshot(),
    };
    assert!(buffer.apply_snapshot(theme_update.clone()));
    assert_eq!(buffer.draft().definition.name, "Typed locally");
    theme_update.workspace.workspace_revision = 1;
    theme_update.revision = 3;
    theme_update.acknowledged_sequence = 1;
    assert!(buffer.apply_snapshot(theme_update));
    assert_eq!(buffer.draft().definition.name, "Untitled");
    let mut obsolete = EditorEnvelope {
        session,
        revision: 4,
        acknowledged_sequence: 1,
        workspace: workspace.snapshot(),
    };
    obsolete.workspace.definition = definition("Old resurrected draft");
    assert!(!buffer.apply_snapshot(obsolete));
    assert_eq!(buffer.draft().definition.name, "Untitled");
}

#[test]
fn obsolete_close_cannot_destroy_reopened_editor() {
    let mut c = AppController::new(InitialState::default());
    let old = open_editor(&mut c);
    c.handle(ControllerEvent::Editor(EditorMessage::Closed {
        session: old,
    }));
    let new = open_editor(&mut c);
    assert_ne!(old, new);
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
        session: old,
    }));
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow { .. } | Effect::FlushEditorDraft { .. }
    )));
    c.handle(ControllerEvent::Editor(EditorMessage::Closed {
        session: old,
    }));
    assert_eq!(snapshot(&c).editor_session, Some(new));
}

#[test]
fn dont_save_restores_checkpoint_then_closes() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Already modified"),
    )));
    let session = open_editor(&mut c);
    let changed = draft(&c, session, 1, "Edited in editor", true);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(
        changed.clone(),
    )));
    let (id, _) = flush(
        c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
            session,
        })),
    );
    c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: changed,
    }));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::CloseDecision {
        session,
        decision: EditorCloseDecision::Discard,
    }));
    assert!(effects.iter().any(|e| matches!(e, Effect::CloseWindow { window: WindowKind::TemplateEditor, session: Some(s) } if *s == session)));
    assert_eq!(snapshot(&c).template.name, "Already modified");
    assert!(snapshot(&c).template_state.dirty);
    c.handle(ControllerEvent::Editor(EditorMessage::Closed { session }));
    assert!(snapshot(&c).editor_session.is_none());
}

#[test]
fn main_close_waits_for_editor_outcome() {
    let mut c = AppController::new(InitialState::default());
    let session = open_editor(&mut c);
    let (id, _) = flush(invoke(&mut c, Command::Exit));
    let d = DraftUpdate {
        session,
        sequence: 0,
        workspace_revision: 0,
        definition: snapshot(&c).template,
        valid: true,
        pending_inputs: false,
    };
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: d,
    }));
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::TemplateEditor,
            ..
        }
    )));
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::Main,
            ..
        }
    )));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::Closed { session }));
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::Main,
            session: None
        }
    )));
}

#[test]
fn save_close_ack_does_not_close_over_newer_raw_input() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    let changed = draft(&c, session, 1, "Save this", true);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(
        changed.clone(),
    )));
    let (id, _) = flush(
        c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
            session,
        })),
    );
    c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: changed,
    }));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::CloseDecision {
        session,
        decision: EditorCloseDecision::SaveAs,
    }));
    let id = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let (save, _) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("draft.json".into()),
    }));
    let newer = draft(&c, session, 2, "Newer raw edit", false);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(newer)));
    let effects = finish(&mut c, save, CoreReply::Unit);
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::CloseWindow { .. })));
    assert_eq!(snapshot(&c).editor_session, Some(session));
    assert!(snapshot(&c).editor_close_prompt);
    assert_eq!(snapshot(&c).template.name, "Newer raw edit");
    assert!(snapshot(&c).template_state.dirty);
}

#[test]
fn main_close_flushes_again_after_picker() {
    let mut c = opened(16, true);
    let session = open_editor(&mut c);
    let (id, _) = flush(invoke(&mut c, Command::Exit));
    let first = DraftUpdate {
        session,
        sequence: 0,
        workspace_revision: 0,
        definition: snapshot(&c).template,
        valid: true,
        pending_inputs: false,
    };
    let (query, _) = run(c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: first,
    })));
    let id = dialog_id(finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "0".into(),
        }),
    ));
    let effects = c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Save,
    });
    let id = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let (save, _) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("copy.bin".into()),
    }));
    let changed = draft(&c, session, 1, "Edited during picker", true);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(
        changed.clone(),
    )));
    let (id, _) = flush(finish(
        &mut c,
        save,
        CoreReply::BinarySaved(SaveResponse {
            bytes_written: "16".into(),
            destination: "copy.bin".into(),
            file: FileInfoDto {
                name: "copy.bin".into(),
                path: "copy.bin".into(),
                size: "16".into(),
                revision: "0".into(),
                dirty: false,
            },
            state: DirtyState {
                dirty: false,
                revision: "0".into(),
            },
        }),
    ));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: changed,
    }));
    let id = dialog_id(effects);
    c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Cancel,
    });
    assert_eq!(snapshot(&c).editor_session, Some(session));
    assert!(snapshot(&c).template_state.dirty);
    assert!(!snapshot(&c).policy.operation_busy);
}

#[test]
fn flush_failure_releases_action_but_keeps_editor_and_draft() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Keep this"),
    )));
    let session = open_editor(&mut c);
    let (id, _) = flush(invoke(&mut c, Command::SaveTemplateAs));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushFailed {
        request_id: id,
        message: "Template Editor did not respond. Its draft is still open.".into(),
    }));
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::OpenPicker { .. } | Effect::CloseWindow { .. })));
    assert_eq!(snapshot(&c).editor_session, Some(session));
    assert_eq!(snapshot(&c).template.name, "Keep this");
    assert!(!snapshot(&c).policy.operation_busy);
    assert_eq!(
        snapshot(&c).error.unwrap().message,
        "Template Editor did not respond. Its draft is still open."
    );
}

#[test]
fn child_close_failure_keeps_main_alive_and_unlocks_the_workspace() {
    let mut c = AppController::new(InitialState::default());
    let session = open_editor(&mut c);
    let (id, _) = flush(invoke(&mut c, Command::Exit));
    let d = DraftUpdate {
        session,
        sequence: 0,
        workspace_revision: 0,
        definition: snapshot(&c).template,
        valid: true,
        pending_inputs: false,
    };
    c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: d,
    }));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::CloseFailed {
        session,
        message: "Could not close companion.".into(),
    }));
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::Main,
            ..
        }
    )));
    assert_eq!(snapshot(&c).editor_session, Some(session));
    assert!(!snapshot(&c).policy.operation_busy);
}

#[test]
fn invalid_raw_close_draft_cannot_save_and_cancel_keeps_it() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    let changed = draft(&c, session, 1, "Invalid raw", false);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(
        changed.clone(),
    )));
    let (id, _) = flush(
        c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
            session,
        })),
    );
    c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: changed,
    }));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::CloseDecision {
        session,
        decision: EditorCloseDecision::SaveAs,
    }));
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::RunCore { .. } | Effect::OpenPicker { .. })));
    c.handle(ControllerEvent::Editor(EditorMessage::CloseDecision {
        session,
        decision: EditorCloseDecision::Cancel,
    }));
    assert_eq!(snapshot(&c).editor_session, Some(session));
    assert!(!snapshot(&c).editor_close_prompt);
    assert!(!snapshot(&c).template_state.valid);
    assert!(snapshot(&c).template_state.dirty);
}

#[test]
fn repeated_close_save_does_not_cancel_the_first_close_attempt() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    let changed = draft(&c, session, 1, "Save this", true);
    c.handle(ControllerEvent::Editor(EditorMessage::Draft(
        changed.clone(),
    )));
    let (id, _) = flush(
        c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
            session,
        })),
    );
    c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: changed,
    }));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::CloseDecision {
        session,
        decision: EditorCloseDecision::SaveAs,
    }));
    let id = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let repeated = c.handle(ControllerEvent::Editor(EditorMessage::CloseDecision {
        session,
        decision: EditorCloseDecision::SaveAs,
    }));
    assert!(!repeated
        .iter()
        .any(|e| matches!(e, Effect::OpenPicker { .. })));
    let (save, _) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("draft.json".into()),
    }));
    let effects = finish(&mut c, save, CoreReply::Unit);
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::TemplateEditor,
            ..
        }
    )));
}

#[test]
fn view_only_close_preserves_preexisting_dirty_template() {
    let mut c = AppController::new(InitialState::default());
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Already dirty"),
    )));
    let session = open_editor(&mut c);
    let (id, _) = flush(
        c.handle(ControllerEvent::Editor(EditorMessage::CloseRequested {
            session,
        })),
    );
    let unchanged = draft(&c, session, 0, "Already dirty", true);
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::FlushReply {
        request_id: id,
        draft: unchanged,
    }));
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::TemplateEditor,
            ..
        }
    )));
    assert!(!snapshot(&c).editor_close_prompt);
    assert!(snapshot(&c).template_state.dirty);
}

#[test]
fn flush_blocks_concurrent_main_template_actions_and_creation_failure_can_retry() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Draft"),
    )));
    let session = open_editor(&mut c);
    let (id, _) = flush(invoke(&mut c, Command::SaveTemplateAs));
    assert!(snapshot(&c).policy.operation_busy);
    assert!(!invoke(&mut c, Command::LoadTemplate)
        .iter()
        .any(|e| matches!(
            e,
            Effect::OpenPicker { .. } | Effect::FlushEditorDraft { .. }
        )));
    c.handle(ControllerEvent::Editor(EditorMessage::FlushFailed {
        request_id: id,
        message: "Disconnected".into(),
    }));
    c.handle(ControllerEvent::Editor(EditorMessage::OpenFailed {
        session,
        message: "Creation failed".into(),
    }));
    let next = open_editor(&mut c);
    assert_ne!(next, session);
    assert_eq!(snapshot(&c).template.name, "Draft");
}

#[test]
fn late_buffer_action_replies_and_acknowledgements_do_not_resurrect_old_workspaces() {
    let mut workspace = TemplateWorkspace::default();
    let session = EditorSessionId(10);
    let envelope = |workspace: &TemplateWorkspace, revision| EditorEnvelope {
        session,
        revision,
        acknowledged_sequence: 0,
        workspace: workspace.snapshot(),
    };
    let mut b = EditorBuffer::new(envelope(&workspace, 1));
    b.edit(definition("Old local"), true, false).unwrap();
    b.begin_action(RequestId(20), false);
    b.begin_action(RequestId(21), true);
    let old = envelope(&workspace, 999);
    workspace.workspace_revision += 1;
    assert!(b.apply_snapshot(envelope(&workspace, 2)));
    b.acknowledge(session, 0, 100);
    b.edit(definition("New local"), true, false).unwrap();
    assert!(!b.acknowledged());
    assert_eq!(
        b.complete_action(RequestId(20), true, Some(old.clone())),
        Some(false)
    );
    assert_eq!(
        b.complete_action(RequestId(21), true, Some(old)),
        Some(false)
    );
    assert_eq!(b.draft().definition.name, "New local");
    b.begin_action(RequestId(22), true);
    workspace.workspace_revision += 1;
    assert_eq!(
        b.complete_action(RequestId(22), true, Some(envelope(&workspace, 3))),
        Some(true)
    );
}

#[test]
fn unexpected_editor_disconnect_aborts_pending_main_exit() {
    let mut c = opened(16, false);
    let session = open_editor(&mut c);
    let (id, _) = flush(invoke(&mut c, Command::Exit));
    let effects = c.handle(ControllerEvent::Editor(EditorMessage::Closed { session }));
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::CloseWindow {
            window: WindowKind::Main,
            ..
        }
    )));
    assert!(!snapshot(&c).policy.operation_busy);
    assert!(snapshot(&c).error.is_some());
    let next = open_editor(&mut c);
    assert_ne!(next, session);
    let old = draft(&c, session, 0, "Obsolete", true);
    assert!(!c
        .handle(ControllerEvent::Editor(EditorMessage::FlushReply {
            request_id: id,
            draft: old
        }))
        .iter()
        .any(|e| matches!(e, Effect::RunCore { .. })));
}
