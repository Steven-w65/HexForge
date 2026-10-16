//! Real native bootstrap contracts replacing asynchronous frontend registration.
mod support;
use eframe::egui;
use hexforge_controller::{command::Command, controller::InitialState};
use hexforge_core::operations::CoreService;
use hexforge_native::app::NativeApp;
use std::sync::Arc;
use support::ui_harness::NativeUiHarness;

#[test]
fn first_real_main_frame_reveals_then_focuses_once_without_stealing_later_focus() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let harness = NativeUiHarness::new();
    app.attach(&harness.context);
    let mut platform_frame = eframe::Frame::_new_kittest();
    for index in 0..2 {
        let frame = harness.frame(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut app, ui, &mut platform_frame),
        );
        for label in ["File", "Edit", "Navigate", "Template", "View", "Open File"] {
            assert!(frame
                .widgets
                .iter()
                .any(|w| w.label == label && w.enabled && w.rect.is_positive()));
        }
        let reveal_commands: Vec<_> = frame.output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    egui::ViewportCommand::Visible(_) | egui::ViewportCommand::Focus
                )
            })
            .collect();
        if index == 0 {
            assert_eq!(
                reveal_commands,
                [
                    &egui::ViewportCommand::Visible(true),
                    &egui::ViewportCommand::Focus
                ]
            );
        } else {
            assert!(
                reveal_commands.is_empty(),
                "Idle frames must not steal focus back"
            );
        }
    }
}

#[derive(Debug)]
struct DroppedPath(std::path::PathBuf);
impl egui::DroppedFile for DroppedPath {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        panic!("the presenter must not read dropped bytes")
    }
}

#[test]
fn first_main_frame_has_real_enabled_controls_without_a_file_or_editor() {
    let core = Arc::new(CoreService::default());
    let mut app = NativeApp::new(InitialState::default(), core.clone());
    let harness = NativeUiHarness::new();
    app.attach(&harness.context);
    // Initial rendering must not wait on an optional file read or worker.
    let held = core.session.lock().unwrap();
    let frame = harness.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    );
    for label in [
        "File",
        "Edit",
        "Navigate",
        "Template",
        "View",
        "Open File",
        "16",
        "32",
    ] {
        let control = frame
            .widgets
            .iter()
            .find(|w| w.label == label)
            .unwrap_or_else(|| panic!("missing real {label}"));
        assert!(control.enabled && control.rect.is_positive());
        assert!(control.rect.left() >= 0.0 && control.rect.right() <= 1280.0);
        assert!(control.rect.top() >= 0.0 && control.rect.bottom() <= 800.0);
    }
    assert!(frame
        .widgets
        .iter()
        .all(|w| w.label != "+ Field" && w.label != "Template Editor"));
    assert!(app.windows.editor().is_none());
    assert!(app.main_snapshot().file.is_none());
    assert!(app.startup_report().is_none());
    assert!(held.is_none());
}

#[test]
fn child_frame_paints_only_the_editor_without_main_controls_or_binary_io() {
    let core = Arc::new(CoreService::default());
    let mut app = NativeApp::new(InitialState::default(), core.clone());
    let harness = NativeUiHarness::new();
    app.attach(&harness.context);
    app.invoke(Command::TemplateEditor, Default::default());
    let session = app.windows.editor().unwrap();
    let held = core.session.lock().unwrap();
    let frame = harness.editor_frame(&mut app, egui::RawInput::default());
    for label in ["Name", "+ Field", "Load…", "Save As…"] {
        assert!(
            frame.widgets.iter().any(|w| w.label == label),
            "missing editor {label}"
        );
    }
    assert!(frame
        .widgets
        .iter()
        .all(|w| !["File", "Navigate", "Open File", "32"].contains(&w.label.as_str())));
    assert_eq!(app.windows.editor(), Some(session));
    assert!(app.main_snapshot().file.is_none());
    assert!(held.is_none());
}

#[test]
fn native_close_is_protected_before_same_frame_drop_can_open_a_file() {
    let source = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(source.path(), [0x41; 16]).unwrap();
    let core = Arc::new(CoreService::default());
    let mut app = NativeApp::new(InitialState::default(), core.clone());
    let harness = NativeUiHarness::new();
    app.attach(&harness.context);
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        dropped_files: vec![Arc::new(DroppedPath(source.path().into()))],
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut frame = eframe::Frame::_new_kittest();
    let mut output = harness
        .context
        .run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
    output.textures_delta.clear();
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !app.windows.main_closing() {
        app.pump();
        assert!(
            std::time::Instant::now() < deadline,
            "clean guarded exit did not complete"
        );
        std::thread::yield_now();
    }
    assert!(app.main_snapshot().file.is_none());
    assert!(core.session.lock().unwrap().is_none());
    assert_eq!(std::fs::read(source.path()).unwrap(), [0x41; 16]);
}
