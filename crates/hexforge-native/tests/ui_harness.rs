mod support;
use eframe::egui;
use support::ui_harness::NativeUiHarness;

#[test]
fn harness_keeps_requested_client_size_and_native_dpi() {
    let harness = NativeUiHarness::new();
    for dpi in [1.0, 1.25, 1.5, 2.0] {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.native_pixels_per_point = Some(dpi);
        viewport.inner_rect = Some(egui::Rect::from_min_size(
            egui::pos2(-1920.0, -400.0),
            egui::vec2(1280.0, 800.0),
        ));
        let mut actual = egui::Rect::NOTHING;
        let frame = harness.frame(input, |ui| actual = ui.max_rect());
        assert_eq!(actual.size(), egui::vec2(1280.0, 800.0));
        assert_eq!(frame.output.pixels_per_point, dpi);
    }
}

#[test]
fn captured_main_and_editor_frames_record_actual_controls_and_authority() {
    use hexforge_controller::{
        command::Command,
        controller::{InitialState, RowWidth, Theme, WindowSnapshot},
    };
    use hexforge_core::operations::CoreService;
    use hexforge_native::app::NativeApp;
    let harness = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(CoreService::default()),
    );
    app.attach(&harness.context);
    app.invoke(Command::ThemeToggle, Default::default());
    app.invoke(Command::Row32, Default::default());
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
    let Some(WindowSnapshot::Main(snapshot)) = &frame.snapshot else {
        panic!("missing main authority");
    };
    assert_eq!(snapshot.theme, Theme::Light);
    assert_eq!(snapshot.row_width, RowWidth::ThirtyTwo);
    let button = frame
        .widgets
        .iter()
        .find(|w| w.label == "32")
        .expect("actual row control");
    assert!(button.enabled);
    assert_eq!(button.toggled, Some(egui::accesskit::Toggled::True));
    assert!(button.rect.bottom() <= 800.0);
    assert!(button.rect.top() >= 776.0);
    assert!(frame
        .widgets
        .iter()
        .any(|w| w.label == "File" && w.role == egui::accesskit::Role::MenuItem));

    app.invoke(Command::TemplateEditor, Default::default());
    let frame = harness.editor_frame(&mut app, egui::RawInput::default());
    let Some(WindowSnapshot::Editor(snapshot)) = &frame.snapshot else {
        panic!("missing editor authority");
    };
    assert_eq!(snapshot.theme, Theme::Light);
    assert_eq!(snapshot.template.name, "Untitled");
    assert!(frame
        .widgets
        .iter()
        .any(|w| w.label == "+ Field" && w.enabled));
}
