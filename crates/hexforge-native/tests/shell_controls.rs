mod support;

use eframe::egui::{self, Event, PointerButton};
use hexforge_controller::{action::Action, command::Command, controller::InitialState};
use hexforge_core::operations::CoreService;
use hexforge_native::app::NativeApp;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use support::ui_harness::{CapturedFrame, NativeUiHarness};

fn new_app(h: &NativeUiHarness) -> NativeApp {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.attach(&h.context);
    app
}

#[test]
fn minimap_source_change_reaches_the_normal_error_without_dropping_accepted_bytes() {
    use std::io::Write;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source-change.bin");
    std::fs::write(&path, vec![0x41; 16_000]).unwrap();
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    load_binary(&mut app, &path);
    app.gesture(Action::ReadViewport {
        offset: 0,
        length: 4096,
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().page.is_none() {
        app.pump();
        assert!(
            Instant::now() < deadline,
            "Initial real page did not arrive"
        );
        std::thread::yield_now();
    }
    let before = app.main_snapshot();
    // No fake backend error: a genuine external file change is detected by the
    // unchanged core's first sparse minimap read. Visible bytes are already cached.
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writer.write_all(&[0xbb]).unwrap();
    writer.flush().unwrap();
    drop(writer);
    let deadline = Instant::now() + Duration::from_secs(3);
    let observed = loop {
        app.pump();
        let f = frame(&h, &mut app, 800.0, vec![]);
        if app.main_snapshot().error.is_some()
            || f.widgets.iter().any(|w| w.label == "Retry minimap preview")
        {
            break f;
        }
        assert!(
            Instant::now() < deadline,
            "The core source-change completion did not reach a presenter"
        );
        std::thread::yield_now();
    };
    let after = app.main_snapshot();
    assert_eq!(
        after.error.as_ref().map(|e| e.code.as_str()),
        Some("source_changed")
    );
    assert!(texts(&observed)
        .iter()
        .any(|(t, _)| t == "Operation failed"));
    assert!(!observed
        .widgets
        .iter()
        .any(|w| w.label == "Retry minimap preview"));
    assert_eq!(after.source, before.source);
    assert_eq!(after.viewport_offset, before.viewport_offset);
    assert_eq!(
        after.page.as_ref().unwrap().bytes,
        before.page.as_ref().unwrap().bytes
    );
    assert!(!after.file.unwrap().dirty);
}

fn frame(
    h: &NativeUiHarness,
    app: &mut NativeApp,
    height: f32,
    events: Vec<Event>,
) -> CapturedFrame {
    h.main_frame(
        app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, height),
            )),
            events,
            ..Default::default()
        },
    )
}

fn texts(f: &CapturedFrame) -> Vec<(String, egui::Pos2)> {
    f.output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => {
                Some((t.galley.job.text.clone(), t.pos + t.galley.size() / 2.0))
            }
            _ => None,
        })
        .collect()
}

fn hex_gutter(f: &CapturedFrame) -> egui::Rect {
    f.output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == "OFFSET" => Some(s.clip_rect),
            _ => None,
        })
        .expect("The real hex address header must be painted")
}

fn click(h: &NativeUiHarness, app: &mut NativeApp, label: &str) {
    let f = frame(h, app, 800.0, vec![]);
    let position = f
        .widgets
        .iter()
        .filter(|w| w.label == label)
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap_or_else(|| panic!("Missing control {label}"))
        .rect
        .center();
    frame(h, app, 800.0, vec![Event::PointerMoved(position)]);
    for pressed in [true, false] {
        frame(
            h,
            app,
            800.0,
            vec![Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
}

fn settle(app: &mut NativeApp) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(Instant::now() < deadline, "Native operation did not finish");
        std::thread::yield_now();
    }
}

fn picker(app: &mut NativeApp) -> hexforge_controller::identity::RequestId {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.pump();
        if let Some((id, _)) = app.picker.take() {
            return id;
        }
        assert!(Instant::now() < deadline, "Expected the owned file picker");
        std::thread::yield_now();
    }
}

fn load_binary(app: &mut NativeApp, path: &Path) {
    app.gesture(Action::DropFiles(vec![path.to_owned()]));
    settle(app);
    assert!(app.main_snapshot().file.is_some());
}

fn load_template(app: &mut NativeApp, path: &Path) {
    app.invoke(Command::LoadTemplate, Default::default());
    let id = picker(app);
    app.runtime.picker_finished(id, Ok(Some(path.to_owned())));
    settle(app);
    assert!(app.main_snapshot().policy.template_active);
}

#[test]
fn actual_view_minimap_menu_updates_the_preview_without_mutating_the_binary_or_template() {
    use hexforge_controller::controller::{MinimapMode, MinimapRender};
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("menu.bin");
    std::fs::write(&path, vec![0x41; 16_000]).unwrap();
    load_binary(&mut app, &path);
    app.gesture(Action::Select { start: 18, end: 20 });
    let before = app.main_snapshot();
    assert!(before.minimap.enabled);
    assert_eq!(before.minimap.mode, MinimapMode::Fit);
    assert_eq!(before.minimap.render, MinimapRender::Characters);
    assert_eq!(before.minimap.scale, 1);
    for label in ["Proportional", "Color Blocks", "Scale 3×", "Show Minimap"] {
        click(&h, &mut app, "View");
        click(&h, &mut app, "Minimap");
        // Use the production accessibility/menu output, not an Invoke stub.
        let menu = frame(&h, &mut app, 800.0, vec![]);
        let entry = menu.widgets.iter().find(|w| w.label == label).unwrap();
        assert!(entry.enabled);
        click(&h, &mut app, label);
    }
    let after = app.main_snapshot();
    assert!(!after.minimap.enabled);
    assert_eq!(after.minimap.mode, MinimapMode::Proportional);
    assert_eq!(after.minimap.render, MinimapRender::Blocks);
    assert_eq!(after.minimap.scale, 3);
    assert_eq!(
        after.file.as_ref().unwrap().path,
        before.file.as_ref().unwrap().path
    );
    assert_eq!(
        after.file.as_ref().unwrap().size,
        before.file.as_ref().unwrap().size
    );
    assert_eq!(
        after.file.as_ref().unwrap().dirty,
        before.file.as_ref().unwrap().dirty
    );
    assert_eq!(after.source, before.source);
    assert_eq!(after.selection, before.selection);
    assert_eq!(after.template, before.template);
    assert_eq!(
        after.template_state.saved_path,
        before.template_state.saved_path
    );
    assert_eq!(after.template_state.dirty, before.template_state.dirty);
    assert_eq!(
        after.template_state.baseline,
        before.template_state.baseline
    );
    assert_eq!(
        after.template_state.workspace_revision,
        before.template_state.workspace_revision
    );
    assert_eq!(
        after.template_state.persistence_revision,
        before.template_state.persistence_revision
    );
    assert_eq!(after.results, before.results);
    assert!(after.error.is_none());
    assert!(app.picker.is_none());
    // Disabling removes the preview column; the same hex grid stays usable.
    let hidden = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&hidden).iter().any(|(t, _)| t == "OFFSET"));
    assert!(!hidden
        .widgets
        .iter()
        .any(|w| w.label == "Retry minimap preview"));
    click(&h, &mut app, "View");
    click(&h, &mut app, "Minimap");
    click(&h, &mut app, "Show Minimap");
    assert!(app.main_snapshot().minimap.enabled);
    assert_eq!(app.main_snapshot().minimap.scale, 3);
    assert_eq!(std::fs::read(path).unwrap(), vec![0x41; 16_000]);
}

#[test]
fn real_search_scan_shows_pending_activity_and_keeps_the_limit_notice_after_the_bar_closes() {
    use hexforge_controller::operation::OperationKind;
    let h = NativeUiHarness::new();
    let core = Arc::new(CoreService::default());
    let mut app = NativeApp::new(InitialState::default(), core.clone());
    app.attach(&h.context);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("many-matches.bin");
    std::fs::write(&path, vec![0x41; 100_001]).unwrap();
    load_binary(&mut app, &path);
    app.invoke(Command::Search, Default::default());
    frame(&h, &mut app, 800.0, vec![]);
    frame(&h, &mut app, 800.0, vec![]);
    app.gesture(Action::SetSearchPattern("41".into()));
    // A held real file-session lock delays the actual scan. The pending UI is
    // asserted before releasing it, and the unchanged bounded search runs next.
    let held = core.session.lock().unwrap();
    app.gesture(Action::SubmitSearch {
        force: false,
        direction: hexforge_controller::search::MatchDirection::Next,
    });
    let pending = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&pending).iter().any(|(t, _)| t == "Searching bytes"));
    assert!(app.main_snapshot().busy[&OperationKind::Search]);
    assert!(!pending
        .widgets
        .iter()
        .any(|w| w.role == egui::accesskit::Role::Dialog));
    drop(held);
    settle(&mut app);
    let complete = frame(&h, &mut app, 800.0, vec![]);
    let state = app.main_snapshot();
    assert_eq!(state.matches.len(), 100_000);
    assert_eq!(state.matches.first(), Some(&0));
    assert_eq!(state.matches.last(), Some(&99_999));
    assert!(state.search.truncated);
    assert!(state.activity.is_none());
    assert!(texts(&complete).iter().any(|(t, _)| t.contains("limited")));
    assert!(!texts(&complete)
        .iter()
        .any(|(t, _)| t.starts_with("Searching bytes")));
    // Closing is not clearing. The warning moves out of the floating bar into
    // the real hex stage, and reopening retains the query and accepted matches.
    app.gesture(Action::CloseSearch);
    let closed = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&closed)
        .iter()
        .any(|(t, _)| t == "Search results were limited; refine the byte pattern."));
    assert_eq!(app.main_snapshot().matches.len(), 100_000);
    app.invoke(Command::Search, Default::default());
    assert_eq!(app.main_snapshot().search.raw, "41");
    frame(&h, &mut app, 800.0, vec![]);
    click(&h, &mut app, "Clear");
    let cleared = app.main_snapshot();
    assert!(cleared.matches.is_empty());
    assert!(!cleared.search.truncated);
    assert_eq!(cleared.search.raw, "");
    assert_eq!(cleared.selection.unwrap().start, 0);
}

#[test]
fn native_empty_prompt_uses_one_owned_open_workflow_and_recovers_after_cancel() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let first = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&first)
        .iter()
        .any(|(text, _)| text == "Drop a binary file"));
    assert!(texts(&first).iter().any(|(text, _)| text == "No file open"));
    let mut headings: Vec<_> = first
        .widgets
        .iter()
        .filter(|w| w.rect.top() < 34.0)
        .collect();
    headings.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
    let headings: Vec<_> = headings.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(headings, ["File", "Edit", "Navigate", "Template", "View"]);
    let open_node = first
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Open File"))
        .unwrap();
    assert!(open_node.1.description().unwrap().contains("Ctrl+O"));
    click(&h, &mut app, "Open File");
    let request = picker(&mut app);
    let busy = frame(&h, &mut app, 800.0, vec![]);
    assert!(
        !busy
            .widgets
            .iter()
            .find(|w| w.label == "Open File")
            .unwrap()
            .enabled
    );
    assert!(busy
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, n)| n.label() == Some("Open File")
            && n.description()
                .is_some_and(|d| d.contains("Wait for the current operation"))));
    click(&h, &mut app, "Open File");
    assert!(
        app.picker.is_none(),
        "Disabled click cannot create a second picker"
    );
    app.runtime.picker_finished(request, Ok(None));
    settle(&mut app);
    let idle = frame(&h, &mut app, 800.0, vec![]);
    assert!(
        idle.widgets
            .iter()
            .find(|w| w.label == "Open File")
            .unwrap()
            .enabled
    );
    assert!(app.main_snapshot().file.is_none());
    assert!(app.main_snapshot().error.is_none());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opened.bin");
    std::fs::write(&path, [0x41; 32]).unwrap();
    click(&h, &mut app, "Open File");
    let request = picker(&mut app);
    app.runtime.picker_finished(request, Ok(Some(path)));
    settle(&mut app);
    let opened = frame(&h, &mut app, 800.0, vec![]);
    assert!(!opened.widgets.iter().any(|w| w.label == "Open File"));
    assert!(!texts(&opened)
        .iter()
        .any(|(text, _)| text == "Drop a binary file"));
    assert!(texts(&opened).iter().any(|(text, _)| text == "opened.bin"));
    assert!(texts(&opened).iter().any(|(text, _)| text == "OFFSET"));
    assert!(app.windows.editor().is_none());
}

#[test]
fn native_empty_binary_keeps_its_gutter_without_an_open_overlay() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.bin");
    std::fs::write(&path, []).unwrap();
    load_binary(&mut app, &path);
    let f = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&f)
        .iter()
        .any(|(text, _)| text == "This binary file is empty."));
    assert!(texts(&f).iter().any(|(text, _)| text == "OFFSET"));
    assert!(!f.widgets.iter().any(|w| w.label == "Open File"));
    assert!(app.main_snapshot().selection.is_none());
}

#[test]
fn native_shell_resize_uses_the_current_viewport_without_mutating_the_workspace() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    // 100px of menu/file/status leaves exactly the source 300/350px
    // workspaces. Their result limits are hand-derived: workspace - 164px.
    for (height, result_height) in [
        (400.0, 136.0),
        (450.0, 186.0),
        (800.0, 220.0),
        (400.0, 136.0),
    ] {
        let f = frame(&h, &mut app, height, vec![]);
        let collapse = f
            .widgets
            .iter()
            .find(|w| w.label == "Collapse parsed results")
            .unwrap();
        assert!((collapse.rect.top() - (height - 24.0 - result_height + 3.5)).abs() < 0.1);
        assert!(texts(&f)
            .iter()
            .any(|(text, pos)| text == "Size 0 B" && pos.y > height - 24.0));
        assert!(app.main_snapshot().file.is_none());
        assert!(app.main_snapshot().selection.is_none());
        assert!(app.windows.editor().is_none());
    }
    assert_eq!(
        app.results_preferences().height,
        220.0,
        "Resizing must not replace the user's preferred height"
    );
    // Layout has no browser observer/frame callback: destroying this native
    // presenter cannot enqueue an asynchronous layout mutation on its owner.
    drop(app);
    let mut reopened = new_app(&h);
    frame(&h, &mut reopened, 800.0, vec![]);
    assert_eq!(reopened.results_preferences().height, 220.0);
}

#[test]
fn native_file_details_stay_above_hex_results_and_preserve_selection() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("details.bin");
    std::fs::write(&path, [0x41; 32]).unwrap();
    load_binary(&mut app, &path);
    app.gesture(Action::Select { start: 5, end: 5 });
    let f = frame(&h, &mut app, 800.0, vec![]);
    assert_eq!(hex_gutter(&f).top(), 76.0);
    click(&h, &mut app, "Show full file details");
    let f = frame(&h, &mut app, 800.0, vec![]);
    let values = texts(&f);
    let path_label = values.iter().find(|(text, _)| text == "PATH").unwrap().1;
    let size = values
        .iter()
        .find(|(text, _)| text == "32 bytes")
        .unwrap()
        .1;
    let results = values
        .iter()
        .find(|(text, _)| text == "PARSED RESULTS")
        .unwrap()
        .1;
    assert!(path_label.y >= 76.0 && path_label.y < size.y);
    assert!(size.y < hex_gutter(&f).top());
    assert!(hex_gutter(&f).bottom() < results.y);
    assert!(values
        .iter()
        .any(|(text, pos)| text == "Offset 0x5" && pos.y > 776.0));
    assert!(!values.iter().any(|(text, _)| text == "Template Editor"));
    assert!(
        f.output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some("Hide full file details")
                && n.is_expanded() == Some(true))
    );
    click(&h, &mut app, "Hide full file details");
    let f = frame(&h, &mut app, 800.0, vec![]);
    assert_eq!(hex_gutter(&f).top(), 76.0);
    assert_eq!(app.main_snapshot().selection.unwrap().start, 5);
}

#[test]
fn native_main_secondary_click_has_no_web_context_menu_or_state_change() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let position = egui::pos2(240.0, 54.0);
    for pressed in [true, false] {
        let f = frame(
            &h,
            &mut app,
            800.0,
            vec![Event::PointerButton {
                pos: position,
                button: PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!f
            .widgets
            .iter()
            .any(|w| w.role == egui::accesskit::Role::Menu));
    }
    assert!(app.main_snapshot().file.is_none());
    assert!(app.picker.is_none());
    assert!(app.main_snapshot().error.is_none());
}

#[test]
fn native_main_menu_and_pending_panel_apply_the_same_template_without_opening_an_editor() {
    for through_menu in [false, true] {
        let h = NativeUiHarness::new();
        let mut app = new_app(&h);
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("sample.bin");
        let template = directory.path().join("header.json");
        let mut bytes = [0; 32];
        bytes[4] = 123;
        std::fs::write(&binary, bytes).unwrap();
        std::fs::write(&template, br#"{"name":"Internal title","defaultEndianness":"little","fields":[{"name":"x","type":"u16","placement":{"mode":"absolute","offset":"4"}}]}"#).unwrap();
        load_binary(&mut app, &binary);
        load_template(&mut app, &template);
        let pending = frame(&h, &mut app, 800.0, vec![]);
        assert!(texts(&pending)
            .iter()
            .any(|(text, _)| text == "Template: \"header.json\" loaded, pending apply."));
        assert!(!app.main_snapshot().template_applied);
        assert!(app.windows.editor().is_none());
        if through_menu {
            click(&h, &mut app, "Template");
            frame(&h, &mut app, 800.0, vec![]);
        }
        click(&h, &mut app, "Apply Template");
        settle(&mut app);
        let applied = frame(&h, &mut app, 800.0, vec![]);
        assert!(app.main_snapshot().template_applied);
        assert_eq!(app.main_snapshot().results[0].value.as_deref(), Some("123"));
        assert!(app.windows.editor().is_none());
        assert!(!applied
            .widgets
            .iter()
            .any(|w| w.label == "Apply Template" && w.rect.top() > 556.0));
        let range = app.main_snapshot().results[0].clone();
        assert_eq!(range.offset.as_deref(), Some("4"));
        assert_eq!(range.length.as_deref(), Some("2"));
        click(&h, &mut app, "Navigate");
        frame(&h, &mut app, 800.0, vec![]);
        click(&h, &mut app, "Parsed Results");
        frame(&h, &mut app, 800.0, vec![]);
        click(&h, &mut app, "x");
        let selected = app.main_snapshot();
        assert_eq!(
            selected.selection.as_ref().map(|s| (s.start, s.end)),
            Some((4, 5))
        );
        assert_eq!(
            selected.template_range.as_ref().map(|s| (s.start, s.end)),
            Some((4, 5))
        );
    }
}

#[test]
fn native_invalid_template_disables_menu_and_panel_apply_without_dispatching() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("sample.bin");
    let template = directory.path().join("header.json");
    std::fs::write(&binary, [0; 32]).unwrap();
    std::fs::write(
        &template,
        br#"{"name":"Header","defaultEndianness":"little","fields":[{"name":"x","type":"u8"}]}"#,
    )
    .unwrap();
    load_binary(&mut app, &binary);
    load_template(&mut app, &template);
    app.gesture(Action::TemplateDraft {
        definition: app.main_snapshot().template,
        valid: false,
    });
    click(&h, &mut app, "Template");
    let f = frame(&h, &mut app, 800.0, vec![]);
    for label in ["Apply Template", "Save Template", "Save Template As…"] {
        let node = f
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label))
            .unwrap();
        assert!(node.1.is_disabled());
        assert!(node.1.description().is_some_and(|d| d.contains("Correct")));
        click(&h, &mut app, label);
        assert!(app.picker.is_none());
        assert!(!app.main_snapshot().template_applied);
    }
    for command in [
        Command::ApplyTemplate,
        Command::SaveTemplate,
        Command::SaveTemplateAs,
    ] {
        app.invoke(command, Default::default());
        assert!(app.picker.is_none());
        assert!(!app.main_snapshot().template_applied);
    }
    assert!(app.windows.editor().is_none());
}

#[test]
fn native_clipped_file_name_and_path_keep_the_source_hover_tooltips() {
    use hexforge_core::dto::FileInfoDto;
    use hexforge_native::{style::HexForgeStyle, views::file_strip::FileStrip};
    let file = FileInfoDto {
        name: "a-long-binary-file-name-that-does-not-fit-the-title-region.bin".into(),
        path: "C:/projects/very/long/path/that-does-not-fit/the-file-information-row/a-long-binary-file-name-that-does-not-fit-the-title-region.bin".into(),
        size: "32".into(), revision: "1".into(), dirty: false,
    };
    for expected in [&file.name, &file.path] {
        let h = NativeUiHarness::new();
        let mut strip = FileStrip::default();
        let style = HexForgeStyle::for_theme(hexforge_controller::controller::Theme::Dark, 1.0);
        let mut time = 0.0;
        let mut draw = |events| {
            time += 1.0;
            h.frame(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 100.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 42.0),
                        )),
                        |ui| strip.show(ui, &style, Some(&file)),
                    );
                },
            )
        };
        let initial = draw(vec![]);
        let shape = initial
            .output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == *expected => {
                    Some((t.pos, s.clip_rect))
                }
                _ => None,
            })
            .unwrap();
        let pos = egui::pos2(shape.0.x + 5.0, 20.5);
        draw(vec![Event::PointerMoved(pos)]);
        draw(vec![]);
        let hovered = draw(vec![]);
        assert!(texts(&hovered).iter().any(|(text, pos)| text == expected && pos.y > 42.0),
            "Hovering clipped file information must reveal its complete value, not only the clipped paint");
        assert!(!strip.expanded, "Hovering must not expand details");
    }
}

#[test]
fn native_navigation_reaches_the_actual_hex_viewport_and_keeps_status_when_scrolled_away() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("navigation.bin");
    std::fs::write(&binary, [0x41; 4096]).unwrap();
    load_binary(&mut app, &binary);
    app.gesture(Action::Navigate {
        start: 160,
        end: 160,
    });
    frame(&h, &mut app, 800.0, vec![]);
    settle(&mut app);
    // Viewport reads intentionally do not make the application busy. Await
    // their actual bounded bytes, not the foreground-operation busy flag.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.main_snapshot().page.as_ref().is_some_and(|page| {
        page.offset
            .parse::<u64>()
            .ok()
            .is_some_and(|start| start <= 160 && 160 - start < page.bytes.len() as u64)
    }) {
        app.pump();
        assert!(
            Instant::now() < deadline,
            "Expected the navigated viewport bytes"
        );
        std::thread::yield_now();
    }
    let target = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&target).iter().any(|(text, _)| text == "000000A0"));
    let before = app.main_snapshot();
    assert_eq!(before.viewport_offset, 160);
    assert_eq!(
        before.selection.as_ref().map(|s| (s.start, s.end)),
        Some((160, 160))
    );
    assert_eq!(before.file.as_ref().unwrap().name, "navigation.bin");
    assert_eq!(before.file.as_ref().unwrap().revision, "0");
    assert!(before.source.is_some());
    app.gesture(Action::ScrollViewport { offset: 2048 });
    frame(&h, &mut app, 800.0, vec![]);
    settle(&mut app);
    let away = frame(&h, &mut app, 800.0, vec![]);
    assert!(texts(&away).iter().any(|(text, _)| text == "00000800"));
    assert!(texts(&away)
        .iter()
        .any(|(text, pos)| text == "Offset 0xA0" && pos.y > 776.0));
    assert_eq!(app.main_snapshot().source, before.source);
    assert_eq!(app.main_snapshot().selection.unwrap().start, 160);
}

#[test]
fn native_parsed_navigation_status_uses_the_accepted_effective_byte() {
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("parsed-status.bin");
    let bytes: Vec<_> = (0..16000).map(|i| (i % 256) as u8).collect();
    std::fs::write(&binary, bytes).unwrap();
    load_binary(&mut app, &binary);
    app.gesture(Action::NavigateTemplate { start: 32, end: 39 });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.pump();
        let painted = frame(&h, &mut app, 800.0, vec![]);
        if app.main_snapshot().page.is_some() {
            assert!(
                texts(&painted)
                    .iter()
                    .any(|(text, _)| text == "Byte 20 ' '"),
                "Navigation must display its effective accepted byte, not a placeholder"
            );
            assert!(texts(&painted).iter().any(|(text, _)| text == "Selected 8"));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn native_shell_progress_stays_in_status_and_search_limit_notice_stays_in_the_hex_stage() {
    use hexforge_controller::{
        controller::{AppController, WindowSnapshot},
        operation::{OperationActivity, OperationKind},
        shortcuts::WindowKind,
    };
    use hexforge_core::progress::{OperationPhase, OperationProgress};
    use hexforge_native::{
        drawing::cache::HexDrawCache,
        style::HexForgeStyle,
        views::{
            hex_view::HexView,
            main_window::{self, MainControls},
        },
    };
    let h = NativeUiHarness::new();
    let controller = AppController::new(InitialState::default());
    let WindowSnapshot::Main(mut snapshot) = controller.snapshot(WindowKind::Main) else {
        unreachable!()
    };
    snapshot.activity = Some(OperationActivity {
        operation: OperationKind::Search,
        progress: Some(OperationProgress {
            operation_id: "1".into(),
            phase: OperationPhase::Search,
            processed: "512".into(),
            total: "1024".into(),
        }),
    });
    snapshot.search.truncated = true;
    let mut hex = HexView::default();
    let mut cache = HexDrawCache::default();
    let mut controls = MainControls::default();
    let style = HexForgeStyle::for_theme(snapshot.theme, 1.0);
    let f = h.frame(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| {
            main_window::show(ui, &style, &snapshot, &mut hex, &mut cache, &mut controls);
        },
    );
    let values = texts(&f);
    assert!(values
        .iter()
        .any(|(text, pos)| text == "Searching bytes · 512 / 1024" && pos.y > 776.0));
    assert!(values.iter().any(|(text, pos)| text
        == "Search results were limited; refine the byte pattern."
        && pos.y > 76.0
        && pos.y < 552.0));
    assert!(!f
        .widgets
        .iter()
        .any(|w| w.role == egui::accesskit::Role::Dialog));
}

#[test]
fn native_row_buttons_fit_16_bytes_and_scroll_32_without_overlapping_the_minimap() {
    use hexforge_controller::controller::RowWidth;
    let h = NativeUiHarness::new();
    let mut app = new_app(&h);
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("rows.bin");
    std::fs::write(&binary, [0x41; 4096]).unwrap();
    load_binary(&mut app, &binary);
    for (label, width, bottom, right) in [
        ("16", RowWidth::Sixteen, 552.0, 1182.0),
        ("32", RowWidth::ThirtyTwo, 542.0, 1130.0),
        ("16", RowWidth::Sixteen, 552.0, 1182.0),
    ] {
        click(&h, &mut app, label);
        let f = frame(&h, &mut app, 800.0, vec![]);
        assert_eq!(app.main_snapshot().row_width, width);
        assert_eq!(
            hex_gutter(&f).bottom(),
            bottom,
            "Only an overflowing 32-byte grid reserves the 10px horizontal scrollbar"
        );
        let data_clip = f
            .output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.job.text == "HEX BYTES" => Some(shape.clip_rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            data_clip.right(),
            right,
            "The minimap and overview are separate right-hand columns"
        );
        assert_eq!(data_clip.top(), 76.0);
        assert!(app.main_snapshot().selection.is_none());
    }
}
