//! Main-window template commands through the actual native controller/runtime,
//! core parser, persistence and result controls. All files are test-owned.
use eframe::egui::{self, Event, Pos2};
use hexforge_controller::{action::Action, command::Command, controller::InitialState};
use hexforge_core::{operations::CoreService, template::TemplateDefinition};
use hexforge_native::app::NativeApp;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn app() -> (NativeApp, egui::Context) {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    frame(&mut app, &ctx, vec![]);
    (app, ctx)
}
fn frame(app: &mut NativeApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Pos2)> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => {
                let center = t.pos + t.galley.size() / 2.0;
                s.clip_rect
                    .contains(center)
                    .then(|| (t.galley.text().into(), center))
            }
            _ => None,
        })
        .collect()
}
fn text(app: &mut NativeApp, ctx: &egui::Context, expected: &str) {
    assert!(
        frame(app, ctx, vec![]).iter().any(|(s, _)| s == expected),
        "missing real native text: {expected}"
    );
}
fn click(app: &mut NativeApp, ctx: &egui::Context, label: &str) {
    frame(app, ctx, vec![]); // Settle a new modal's native sizing/focus pass.
    let pos = frame(app, ctx, vec![])
        .into_iter()
        .find(|(s, _)| s == label)
        .unwrap_or_else(|| panic!("missing control: {label}"))
        .1;
    frame(app, ctx, vec![Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
}
fn settle(app: &mut NativeApp) {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        app.pump();
        if !app.main_snapshot().policy.operation_busy {
            return;
        }
        assert!(
            Instant::now() < until,
            "native workflow stalled: {:?}",
            app.main_snapshot()
        );
        std::thread::yield_now();
    }
}
fn pick(app: &mut NativeApp, command: Command, path: Option<PathBuf>) {
    app.invoke(command, Default::default());
    let (id, _) = app.picker.take().expect("enabled command owns its picker");
    app.runtime.picker_finished(id, Ok(path));
    app.pump();
}
fn simple(name: &str) -> TemplateDefinition {
    serde_json::from_value(serde_json::json!({
        "name": name, "defaultEndianness": "little", "fields": [
            {"name":"magic", "type":"u8", "placement":{"mode":"absolute", "offset":"0"}}
        ]
    }))
    .unwrap()
}

#[test]
fn main_template_commands_preserve_loaded_pending_applied_saved_and_unloaded_states() {
    for binary_first in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let template_path = directory.path().join("header.json");
        let binary_path = directory.path().join("firmware.bin");
        std::fs::write(
            &template_path,
            serde_json::to_vec_pretty(&simple("Internal title")).unwrap(),
        )
        .unwrap();
        std::fs::write(&binary_path, [0x4d; 64]).unwrap();
        let (mut app, ctx) = app();
        text(&mut app, &ctx, "No template loaded.");
        if binary_first {
            pick(&mut app, Command::Open, Some(binary_path.clone()));
            settle(&mut app);
            text(
                &mut app,
                &ctx,
                "No template loaded. Load or create template from Template menu.",
            );
        }
        pick(&mut app, Command::LoadTemplate, Some(template_path.clone()));
        settle(&mut app);
        let loaded = app.main_snapshot();
        assert_eq!(loaded.template.name, "Internal title");
        assert_eq!(
            loaded.template_state.saved_path.as_ref(),
            Some(&template_path)
        );
        assert!(loaded.template_state.active);
        assert!(!loaded.template_state.dirty);
        assert!(!loaded.template_applied);
        assert!(app.windows.editor().is_none());
        if !binary_first {
            assert!(loaded.file.is_none());
            text(
                &mut app,
                &ctx,
                "Template: \"header.json\" loaded. Open binary file to preview.",
            );
            app.invoke(Command::ApplyTemplate, Default::default());
            assert!(app.main_snapshot().results.is_empty());
            assert!(!app.main_snapshot().policy.operation_busy);
            pick(&mut app, Command::Open, Some(binary_path.clone()));
            settle(&mut app);
        }
        let source = app.main_snapshot().source;
        text(
            &mut app,
            &ctx,
            "Template: \"header.json\" loaded, pending apply.",
        );
        click(&mut app, &ctx, "Apply Template");
        settle(&mut app);
        let applied = app.main_snapshot();
        assert!(applied.template_applied);
        assert_eq!(applied.results.len(), 1);
        assert_eq!(applied.results[0].path, "magic");
        assert_eq!(applied.results[0].value.as_deref(), Some("77"));
        assert_eq!(
            applied.completion_notice.as_deref(),
            Some("Parsed 1 field.")
        );
        click(&mut app, &ctx, "magic");
        assert_eq!(
            app.main_snapshot().template_range.map(|r| (r.start, r.end)),
            Some((0, 0))
        );

        let mut edited = app.main_snapshot().template;
        edited.name = "Edited header".into();
        app.gesture(Action::TemplateChanged(edited));
        let changed = app.main_snapshot();
        assert!(changed.template_state.dirty);
        assert!(changed.results_need_refresh);
        assert!(!changed.template_applied);
        assert!(changed.results.is_empty());
        assert!(changed.template_range.is_none());
        text(
            &mut app,
            &ctx,
            "Template: \"header.json\" loaded, pending apply.",
        );
        app.invoke(Command::SaveTemplate, Default::default());
        settle(&mut app);
        let saved = app.main_snapshot();
        assert!(!saved.template_state.dirty);
        assert!(saved.results_need_refresh);
        assert!(!saved.template_applied);
        assert!(saved.results.is_empty());
        assert_eq!(
            saved.completion_notice.as_deref(),
            Some("Template saved: header.json.")
        );
        assert_eq!(saved.source, source);
        assert!(!saved.file.as_ref().unwrap().dirty);
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&template_path).unwrap()).unwrap();
        assert_eq!(json["name"], "Edited header");
        assert!(json.get("version").is_none());

        app.invoke(Command::ApplyTemplate, Default::default());
        settle(&mut app);
        click(&mut app, &ctx, "magic");
        app.invoke(Command::UnloadTemplate, Default::default());
        settle(&mut app);
        let unloaded = app.main_snapshot();
        assert_eq!(unloaded.source, source);
        assert_eq!(unloaded.file.as_ref().unwrap().name, "firmware.bin");
        assert!(unloaded.results.is_empty());
        assert!(unloaded.template_range.is_none());
        assert!(!unloaded.template_state.active);
        assert!(unloaded.template_state.saved_path.is_none());
        text(
            &mut app,
            &ctx,
            "No template loaded. Load or create template from Template menu.",
        );
        assert_eq!(std::fs::read(binary_path).unwrap(), [0x4d; 64]);
    }
}

#[test]
fn rejected_legacy_load_and_cancelled_or_failed_save_as_preserve_the_native_draft() {
    let directory = tempfile::tempdir().unwrap();
    let legacy_path = directory.path().join("old.json");
    let legacy = br#"{"version":1,"name":"Old","defaultEndianness":"little","fields":[]}"#;
    std::fs::write(&legacy_path, legacy).unwrap();
    let (mut app, ctx) = app();
    app.gesture(Action::TemplateChanged(simple("Keep draft")));
    let before = app.main_snapshot();
    assert!(before.template_state.dirty);
    assert!(before.completion_notice.is_none());
    pick(&mut app, Command::SaveTemplateAs, None);
    settle(&mut app);
    assert!(app.main_snapshot().completion_notice.is_none());
    assert_eq!(app.main_snapshot().template, before.template);
    assert!(app.main_snapshot().template_state.saved_path.is_none());

    pick(
        &mut app,
        Command::SaveTemplateAs,
        Some(directory.path().join("missing").join("draft.json")),
    );
    settle(&mut app);
    let failed = app.main_snapshot();
    assert!(failed.error.is_some());
    assert!(failed.completion_notice.is_none());
    assert_eq!(failed.template, before.template);
    assert_eq!(
        failed.template_state.workspace_revision,
        before.template_state.workspace_revision
    );
    assert_eq!(
        failed.template_state.persistence_revision,
        before.template_state.persistence_revision
    );
    assert!(failed.template_state.dirty);
    assert!(failed.template_state.saved_path.is_none());
    text(&mut app, &ctx, "Operation failed");
    app.gesture(Action::ClearError);

    pick(&mut app, Command::LoadTemplate, Some(legacy_path.clone()));
    assert!(app
        .main_snapshot()
        .dialog
        .as_ref()
        .unwrap()
        .1
        .message
        .contains("unsaved changes"));
    click(&mut app, &ctx, "OK");
    settle(&mut app);
    let rejected = app.main_snapshot();
    assert!(rejected
        .error
        .as_ref()
        .unwrap()
        .message
        .contains("unknown field `version`"));
    assert_eq!(rejected.template, before.template);
    assert_eq!(
        rejected.template_state.baseline,
        before.template_state.baseline
    );
    assert_eq!(
        rejected.template_state.workspace_revision,
        before.template_state.workspace_revision
    );
    assert_eq!(
        rejected.template_state.persistence_revision,
        before.template_state.persistence_revision
    );
    assert!(rejected.template_state.dirty);
    assert!(rejected.template_state.saved_path.is_none());
    assert!(rejected.results.is_empty());
    assert_eq!(std::fs::read(legacy_path).unwrap(), legacy);
}

#[test]
fn native_structured_results_navigate_exact_ranges_and_export_large_integer_text() {
    let directory = tempfile::tempdir().unwrap();
    let binary_path = directory.path().join("firmware.bin");
    let template_path = directory.path().join("wide.json");
    let csv_path = directory.path().join("wide.csv");
    let mut binary = [0; 64];
    binary[16..24].copy_from_slice(&9_007_199_254_740_993u64.to_le_bytes());
    std::fs::write(&binary_path, binary).unwrap();
    std::fs::write(&template_path, br#"{"name":"Wide","defaultEndianness":"little","fields":[{"name":"header","type":"struct","fields":[{"name":"value","type":"u64","placement":{"mode":"absolute","offset":"16"}}]}]}"#).unwrap();
    let (mut app, ctx) = app();
    pick(&mut app, Command::Open, Some(binary_path.clone()));
    settle(&mut app);
    pick(&mut app, Command::LoadTemplate, Some(template_path.clone()));
    settle(&mut app);
    assert!(!app.main_snapshot().template_applied);
    assert!(app.main_snapshot().results.is_empty());
    app.invoke(Command::ApplyTemplate, Default::default());
    settle(&mut app);
    let s = app.main_snapshot();
    let leaf = &s.results[0].children[0];
    assert_eq!(leaf.path, "header.value");
    assert_eq!(leaf.offset.as_deref(), Some("16"));
    assert_eq!(leaf.length.as_deref(), Some("8"));
    assert_eq!(leaf.value.as_deref(), Some("9007199254740993"));
    assert!(leaf.diagnostics.is_empty());
    click(&mut app, &ctx, "value");
    let selected = app.main_snapshot();
    assert_eq!(
        selected
            .template_range
            .as_ref()
            .map(|r| (r.start, r.end, r.count())),
        Some((16, 23, 8))
    );
    assert_eq!(
        selected.selection.as_ref().map(|r| (r.start, r.end)),
        Some((16, 23))
    );
    pick(&mut app, Command::Export, Some(csv_path.clone()));
    settle(&mut app);
    let csv = std::fs::read_to_string(csv_path).unwrap();
    assert!(csv.lines().next().unwrap().starts_with("path,"));
    assert!(csv.contains("header.value"));
    assert!(csv.contains("9007199254740993"));
    assert_eq!(
        app.main_snapshot()
            .template_range
            .as_ref()
            .map(|r| (r.start, r.end)),
        Some((16, 23))
    );
    assert_eq!(std::fs::read(binary_path).unwrap(), binary);
    assert!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(template_path).unwrap())
            .unwrap()
            .get("version")
            .is_none()
    );
}

#[test]
fn an_unsaved_separate_editor_draft_applies_without_creating_or_binding_a_json_file() {
    use hexforge_controller::{
        identity::RequestId,
        template::coordinator::{EditorAction, EditorMessage},
    };
    let directory = tempfile::tempdir().unwrap();
    let binary_path = directory.path().join("firmware.bin");
    std::fs::write(&binary_path, [0x4d; 64]).unwrap();
    let (mut app, ctx) = app();
    pick(&mut app, Command::Open, Some(binary_path.clone()));
    settle(&mut app);
    app.invoke(Command::TemplateEditor, Default::default());
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(660.0, 760.0),
            )),
            ..Default::default()
        },
        |ui| app.paint_editor(ui),
    );
    output.textures_delta.clear();
    app.editor
        .as_mut()
        .unwrap()
        .model
        .replace(simple("My draft"));
    app.publish_editor().unwrap();
    let before = app.main_snapshot();
    assert!(before.template_state.dirty);
    assert!(before.template_state.saved_path.is_none());
    assert!(!before.template_applied);
    text(&mut app, &ctx, "Unsaved template draft: \"My draft\".");
    let draft = app.editor.as_ref().unwrap().buffer.draft();
    app.editor_message(EditorMessage::Action {
        request_id: RequestId(31),
        draft,
        action: EditorAction::Command(Command::ApplyTemplate),
    });
    settle(&mut app);
    let applied = app.main_snapshot();
    assert!(applied.template_applied);
    assert_eq!(applied.results.len(), 1);
    assert_eq!(applied.results[0].value.as_deref(), Some("77"));
    assert!(applied.template_state.saved_path.is_none());
    assert!(applied.template_state.dirty);
    assert!(app.windows.editor().is_some());
    assert!(!app.windows.editor_closing());
    let mut revised = app.main_snapshot().template;
    revised.name = "Revised draft".into();
    app.editor.as_mut().unwrap().model.replace(revised);
    app.publish_editor().unwrap();
    let changed = app.main_snapshot();
    assert!(!changed.template_applied);
    assert!(changed.results_need_refresh);
    assert!(changed.results.is_empty());
    assert!(changed.template_range.is_none());
    text(&mut app, &ctx, "Unsaved template draft: \"Revised draft\".");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    assert_eq!(std::fs::read(binary_path).unwrap(), [0x4d; 64]);
}

#[test]
fn real_template_save_as_save_and_unload_preserve_unsaved_binary_bytes_and_markers() {
    let directory = tempfile::tempdir().unwrap();
    let binary_path = directory.path().join("source.bin");
    let template_path = directory.path().join("draft.json");
    std::fs::write(&binary_path, [0x41; 64]).unwrap();
    let (mut app, ctx) = app();
    pick(&mut app, Command::Open, Some(binary_path.clone()));
    settle(&mut app);
    app.gesture(Action::Select { start: 7, end: 7 });
    app.invoke(Command::ToggleEdit, Default::default());
    app.invoke(Command::EditSelected, Default::default());
    app.gesture(Action::EditSelectedByte("ff".into()));
    settle(&mut app);
    app.gesture(Action::ReadViewport {
        offset: 0,
        length: 64,
    });
    // Viewport/overview reads intentionally do not block ordinary controls.
    // Wait for their actual completions, not the visible-operation busy flag.
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().page.is_none()
        || app.main_snapshot().modified_overview.bins.is_empty()
    {
        app.pump();
        assert!(
            Instant::now() < deadline,
            "effective page and modified overview did not arrive"
        );
        std::thread::yield_now();
    }
    let before = app.main_snapshot();
    assert!(before.file.as_ref().unwrap().dirty);
    assert_eq!(before.page.as_ref().unwrap().bytes[7], 255);
    assert_eq!(before.page.as_ref().unwrap().modified_offsets, ["7"]);
    app.gesture(Action::TemplateChanged(simple("Draft")));
    pick(
        &mut app,
        Command::SaveTemplateAs,
        Some(template_path.clone()),
    );
    settle(&mut app);
    assert!(!app.main_snapshot().template_state.dirty);
    assert!(!app.main_snapshot().template_applied);
    app.gesture(Action::TemplateChanged(simple("Saved revision")));
    app.invoke(Command::SaveTemplate, Default::default());
    settle(&mut app);
    app.invoke(Command::UnloadTemplate, Default::default());
    settle(&mut app);
    let after = app.main_snapshot();
    assert_eq!(after.source, before.source);
    assert_eq!(
        after.file.as_ref().unwrap().path,
        before.file.as_ref().unwrap().path
    );
    assert!(after.file.as_ref().unwrap().dirty);
    assert_eq!(after.selection, before.selection);
    assert!(after.can_undo);
    assert_eq!(after.page.as_ref().unwrap().bytes[7], 255);
    assert_eq!(after.page.as_ref().unwrap().modified_offsets, ["7"]);
    assert_eq!(after.modified_overview.bins, before.modified_overview.bins);
    assert!(!after.template_state.active);
    assert!(after.results.is_empty());
    assert!(!after.template_applied);
    text(
        &mut app,
        &ctx,
        "No template loaded. Load or create template from Template menu.",
    );
    assert_eq!(std::fs::read(binary_path).unwrap(), [0x41; 64]);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(template_path).unwrap()).unwrap();
    assert_eq!(saved["name"], "Saved revision");
    assert!(saved.get("version").is_none());
}

#[test]
fn main_template_menu_rechecks_validity_and_enables_in_place_save_only_after_save_as_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("firmware.bin");
    let json = directory.path().join("template.json");
    std::fs::write(&binary, [0x41; 64]).unwrap();
    let (mut app, ctx) = app();
    pick(&mut app, Command::Open, Some(binary.clone()));
    settle(&mut app);
    app.gesture(Action::TemplateDraft {
        definition: simple("Invalid raw draft"),
        valid: false,
    });
    click(&mut app, &ctx, "Template");
    for label in ["Save Template", "Save Template As…", "Apply Template"] {
        click(&mut app, &ctx, label);
        assert!(
            app.picker.is_none(),
            "disabled {label} must not launch a picker"
        );
        assert!(!app.main_snapshot().policy.operation_busy);
        assert!(!app.main_snapshot().template_applied);
        assert!(app.main_snapshot().error.is_none());
    }
    assert!(!json.exists());
    // Same workspace, now validated: Save still needs a path, while Save As
    // becomes a real menu action. No version conversion or auto-apply occurs.
    app.gesture(Action::TemplateDraft {
        definition: simple("Fixed field"),
        valid: true,
    });
    click(&mut app, &ctx, "Save Template");
    assert!(app.picker.is_none());
    assert!(app.main_snapshot().template_state.dirty);
    click(&mut app, &ctx, "Save Template As…");
    let (id, picker) = app
        .picker
        .take()
        .expect("valid menu Save As owns one picker");
    assert_eq!(
        picker.purpose,
        hexforge_controller::dialog::PickerPurpose::TemplateSave
    );
    app.runtime.picker_finished(id, Ok(Some(json.clone())));
    settle(&mut app);
    assert!(!app.main_snapshot().template_state.dirty);
    assert!(app.main_snapshot().template_state.saved_path.is_some());
    assert!(!app.main_snapshot().template_applied);
    assert!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&json).unwrap())
            .unwrap()
            .get("version")
            .is_none()
    );
    app.gesture(Action::TemplateChanged(simple("Saved in place")));
    click(&mut app, &ctx, "Template");
    click(&mut app, &ctx, "Save Template");
    assert!(
        app.picker.is_none(),
        "in-place Save does not ask for a second path"
    );
    settle(&mut app);
    assert!(!app.main_snapshot().template_state.dirty);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(json).unwrap()).unwrap()["name"],
        "Saved in place"
    );
    assert!(!app.main_snapshot().template_applied);
    click(&mut app, &ctx, "Template");
    click(&mut app, &ctx, "Apply Template");
    settle(&mut app);
    assert_eq!(app.main_snapshot().results[0].value.as_deref(), Some("65"));
    assert!(app.windows.editor().is_none());
    assert_eq!(std::fs::read(binary).unwrap(), vec![0x41; 64]);
}
