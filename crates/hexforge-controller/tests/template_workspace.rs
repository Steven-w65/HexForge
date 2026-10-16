#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, Effect},
    dialog::DialogDecision,
    template::workspace::{
        TemplateAction, TemplateDirective, TemplateEvent, TemplateReply, TemplateWorkspace,
    },
};
use hexforge_core::{
    error::{AppError, ErrorCode},
    operations::{CoreReply, CoreRequest, CoreService},
    template::TemplateDefinition,
};
use support::*;

fn definition(name: &str) -> TemplateDefinition {
    serde_json::from_value(serde_json::json!({"name":name,"defaultEndianness":"little","fields":[{"name":"id","type":"u8","placement":{"mode":"sequential"}}]})).unwrap()
}
fn draft(w: &mut TemplateWorkspace, name: &str, valid: bool) {
    w.handle(TemplateEvent::Draft {
        definition: definition(name),
        valid,
    });
}
fn begin(w: &mut TemplateWorkspace, action: TemplateAction) -> Vec<TemplateDirective> {
    w.handle(TemplateEvent::Begin(action))
}
fn loaded(w: &mut TemplateWorkspace) {
    begin(w, TemplateAction::Load);
    w.handle(TemplateEvent::Picker(Some("saved.json".into())));
    w.handle(TemplateEvent::CoreFinished(Ok(TemplateReply::Loaded(
        definition("Loaded"),
    ))));
}
fn success(w: &mut TemplateWorkspace) -> Vec<TemplateDirective> {
    w.handle(TemplateEvent::CoreFinished(Ok(TemplateReply::Unit)))
}

#[test]
fn load_save_never_apply() {
    let mut w = TemplateWorkspace::default();
    loaded(&mut w);
    assert!(w.snapshot().active);
    assert!(!w.snapshot().dirty);
    assert!(!w.snapshot().applied);
    draft(&mut w, "Edited", true);
    let effects = begin(&mut w, TemplateAction::Save);
    assert!(effects.iter().any(|e| matches!(
        e,
        TemplateDirective::Core(CoreRequest::SaveTemplate {
            overwrite_external: false,
            ..
        })
    )));
    assert!(!effects.iter().any(|e| matches!(
        e,
        TemplateDirective::Core(CoreRequest::ApplyTemplate { .. })
    )));
    success(&mut w);
    assert!(!w.snapshot().dirty);
    assert!(!w.snapshot().applied);
    assert_eq!(
        w.snapshot().saved_path.unwrap().to_string_lossy(),
        "saved.json"
    );
}

#[test]
fn cancelled_picker_keeps_path_draft_results() {
    let mut w = TemplateWorkspace::default();
    loaded(&mut w);
    draft(&mut w, "Edited", true);
    let before = w.snapshot();
    begin(&mut w, TemplateAction::SaveAs);
    w.handle(TemplateEvent::Picker(None));
    let after = w.snapshot();
    assert_eq!(before.definition, after.definition);
    assert_eq!(before.saved_path, after.saved_path);
    assert_eq!(before.persistence_revision, after.persistence_revision);
    assert!(after.dirty);
}

#[test]
fn external_change_cancel_preserves_workspace() {
    let mut w = TemplateWorkspace::default();
    loaded(&mut w);
    draft(&mut w, "Edited", true);
    begin(&mut w, TemplateAction::Save);
    let effects = w.handle(TemplateEvent::CoreFinished(Err(AppError::new(
        ErrorCode::ExternalModification,
        "External change.",
        None,
    ))));
    assert!(effects.iter().any(|e| matches!(e, TemplateDirective::Dialog(d) if d.message == "The file has been modified by another program. Overwrite?")));
    w.handle(TemplateEvent::Decision(DialogDecision::Cancel));
    assert_eq!(w.snapshot().definition.name, "Edited");
    assert!(w.snapshot().dirty);
    assert_eq!(
        w.snapshot().saved_path.unwrap().to_string_lossy(),
        "saved.json"
    );
}

#[test]
fn failed_save_keeps_dirty_baseline() {
    let mut w = TemplateWorkspace::default();
    loaded(&mut w);
    draft(&mut w, "Edited", true);
    let revision = w.snapshot().persistence_revision;
    begin(&mut w, TemplateAction::Save);
    w.handle(TemplateEvent::CoreFinished(Err(AppError::new(
        ErrorCode::DiskFull,
        "Disk full.",
        None,
    ))));
    assert!(w.snapshot().dirty);
    assert_eq!(w.snapshot().persistence_revision, revision);
    assert_eq!(w.snapshot().baseline.name, "Loaded");
    assert!(!w.snapshot().busy);
}

#[test]
fn save_ack_does_not_clean_newer_input() {
    let mut w = TemplateWorkspace::default();
    loaded(&mut w);
    draft(&mut w, "Saved revision", true);
    begin(&mut w, TemplateAction::Save);
    draft(&mut w, "Newer edit", true);
    success(&mut w);
    let s = w.snapshot();
    assert_eq!(s.definition.name, "Newer edit");
    assert!(s.dirty);
    assert_eq!(s.baseline.name, "Saved revision");
    assert_eq!(s.checkpoint_definition.name, "Saved revision");
    draft(&mut w, "Newer edit", false);
    begin(&mut w, TemplateAction::Save);
    assert!(!w.snapshot().busy);
    assert!(!w.snapshot().valid);
    assert!(w.snapshot().dirty);
}

#[test]
fn save_as_captures_after_picker_but_retry_reuses_captured_definition() {
    let mut w = TemplateWorkspace::default();
    draft(&mut w, "Before picker", true);
    begin(&mut w, TemplateAction::SaveAs);
    draft(&mut w, "During picker", true);
    let effects = w.handle(TemplateEvent::Picker(Some("chosen.json".into())));
    assert!(effects.iter().any(|e| matches!(e, TemplateDirective::Core(CoreRequest::SaveTemplateAs { template, overwrite: false, .. }) if template.name == "During picker")));
    draft(&mut w, "Newer edit", true);
    let effects = w.handle(TemplateEvent::CoreFinished(Err(AppError::new(
        ErrorCode::DestinationExists,
        "Exists.",
        None,
    ))));
    assert!(effects.iter().any(|e| matches!(e, TemplateDirective::Dialog(d) if d.message == "File already exists. Overwrite?")));
    let effects = w.handle(TemplateEvent::Decision(DialogDecision::Confirm));
    assert!(effects.iter().any(|e| matches!(e, TemplateDirective::Core(CoreRequest::SaveTemplateAs { template, overwrite: true, .. }) if template.name == "During picker")));
    success(&mut w);
    assert_eq!(w.snapshot().definition.name, "Newer edit");
    assert!(w.snapshot().dirty);
    assert_eq!(w.snapshot().baseline.name, "During picker");
}

#[test]
fn unload_clears_binding_results_and_field_highlight() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Draft"),
    )));
    c.handle(ControllerEvent::Action(Action::NavigateTemplate {
        start: 0,
        end: 0,
    }));
    let effects = invoke(&mut c, Command::UnloadTemplate);
    let id = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::ShowDialog { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let (stamp, request) = run(c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Confirm,
    }));
    assert!(matches!(request, CoreRequest::UnloadTemplate));
    finish(&mut c, stamp, CoreReply::Unit);
    let s = snapshot(&c);
    assert!(!s.template_state.active);
    assert!(s.results.is_empty());
    assert!(s.template_range.is_none());
    assert!(s.selection.is_none());
    assert!(s.file.is_some());
}

#[test]
fn invalid_version_json_does_not_replace_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.json");
    std::fs::write(
        &path,
        r#"{"version":2,"name":"Old","defaultEndianness":"little","fields":[]}"#,
    )
    .unwrap();
    let core = CoreService::default();
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Keep draft"),
    )));
    let before = snapshot(&c).template_state;
    let id = invoke(&mut c, Command::LoadTemplate)
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let id = c
        .handle(ControllerEvent::PickerCompleted {
            request_id: id,
            path: Some(path.clone()),
        })
        .into_iter()
        .find_map(|e| {
            if let Effect::ShowDialog { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let (stamp, request) = run(c.handle(ControllerEvent::DialogCompleted {
        request_id: id,
        decision: DialogDecision::Confirm,
    }));
    let reply = core.execute(request, &mut |_| {});
    assert!(reply.is_err());
    c.handle(ControllerEvent::CoreCompleted { stamp, reply });
    let after = snapshot(&c).template_state;
    assert_eq!(before.definition, after.definition);
    assert_eq!(before.saved_path, after.saved_path);
    assert_eq!(before.baseline, after.baseline);
    assert!(after.dirty);
    assert!(!after.busy);
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("\"version\":2"));
}

#[test]
fn controller_save_ack_keeps_newer_raw_input_dirty_without_applying() {
    let mut c = opened(16, true);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Saved draft"),
    )));
    let id = invoke(&mut c, Command::SaveTemplateAs)
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .unwrap();
    let (stamp, request) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("draft.json".into()),
    }));
    assert!(matches!(request, CoreRequest::SaveTemplateAs { .. }));
    c.handle(ControllerEvent::Action(Action::TemplateDraft {
        definition: definition("Newer input"),
        valid: false,
    }));
    finish(&mut c, stamp, CoreReply::Unit);
    let s = snapshot(&c);
    assert_eq!(s.template_state.definition.name, "Newer input");
    assert_eq!(s.template_state.baseline.name, "Saved draft");
    assert!(s.template_state.dirty);
    assert!(!s.template_state.valid);
    assert!(!s.template_applied);
    assert!(s.file.unwrap().dirty);
    assert!(!s.policy.template_valid);
    assert!(!s.policy.operation_busy);
    assert_eq!(
        s.template_state.saved_path.unwrap().to_string_lossy(),
        "draft.json"
    );
}

#[test]
fn invalid_input_during_save_as_picker_never_writes_or_changes_path() {
    let mut w = TemplateWorkspace::default();
    loaded(&mut w);
    draft(&mut w, "Edited", true);
    begin(&mut w, TemplateAction::SaveAs);
    draft(&mut w, "Edited", false);
    let effects = w.handle(TemplateEvent::Picker(Some("other.json".into())));
    assert!(!effects
        .iter()
        .any(|e| matches!(e, TemplateDirective::Core(_))));
    assert_eq!(
        w.snapshot().saved_path.unwrap().to_string_lossy(),
        "saved.json"
    );
    assert!(w.snapshot().dirty);
    assert!(!w.snapshot().busy);
}

#[test]
fn status_keeps_persistence_and_result_freshness_independent() {
    use hexforge_controller::template::status::{template_status, TemplateSource};
    let cases = [
        (
            TemplateSource::File,
            false,
            false,
            false,
            true,
            false,
            "Saved · Pending apply",
        ),
        (
            TemplateSource::File,
            true,
            true,
            false,
            true,
            false,
            "Modified · Applied",
        ),
        (
            TemplateSource::File,
            true,
            false,
            true,
            true,
            false,
            "Modified · Needs apply",
        ),
        (
            TemplateSource::File,
            false,
            false,
            true,
            true,
            false,
            "Saved · Needs apply",
        ),
        (
            TemplateSource::File,
            false,
            true,
            false,
            true,
            true,
            "Saved · Parse errors",
        ),
        (
            TemplateSource::File,
            false,
            false,
            false,
            false,
            false,
            "Saved · Loaded",
        ),
        (
            TemplateSource::Draft,
            false,
            true,
            false,
            true,
            false,
            "Unsaved draft · Applied",
        ),
        (
            TemplateSource::None,
            false,
            true,
            false,
            true,
            false,
            "No template",
        ),
    ];
    for (source, dirty, applied, refresh, file, errors, expected) in cases {
        assert_eq!(
            template_status(source, dirty, applied, refresh, file, errors),
            expected
        );
    }
}

#[test]
fn request_workspace_revision_matches_binding_without_counting_draft_edits() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Draft"),
    )));
    let workspace = snapshot(&c).template_state.workspace_revision;
    let (stamp, _) = run(invoke(&mut c, Command::ApplyTemplate));
    assert_eq!(stamp.workspace_revision, workspace);
    c.handle(ControllerEvent::Action(Action::TemplateChanged(
        definition("Newer"),
    )));
    finish(&mut c, stamp, CoreReply::Parsed(vec![]));
    assert!(!snapshot(&c).template_applied);
    assert_eq!(snapshot(&c).template_state.workspace_revision, workspace);
}
