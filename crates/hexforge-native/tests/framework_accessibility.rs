//! Characterize our real native-window boundary, not the framework's source.
//! Physical provider registration is verified separately in Windows. This
//! catches showing a child before its accessibility provider can be installed.
use eframe::egui;
use hexforge_controller::{command::Command, controller::InitialState};
use hexforge_core::operations::CoreService;
use hexforge_native::{app::NativeApp, platform::windows::editor_viewport};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn editor_is_created_hidden_and_shown_only_after_its_real_controls_are_painted() {
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let returned = observed.clone();
    // Substitute only the native-window creation/render boundary. Run the
    // actual child presenter, including its accessibility controls and theme.
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let initially_hidden = viewport.builder.visible == Some(false);
        let mut input = egui::RawInput {
            viewport_id: viewport.ids.this,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(660.0, 760.0),
            )),
            ..Default::default()
        };
        input
            .viewports
            .insert(viewport.ids.this, Default::default());
        let mut frame = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
        let has_real_controls = ["Template Editor", "Name", "+ Field"].iter().all(|label| {
            frame.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                egui::epaint::Shape::Text(text) if text.galley.text() == *label)
            })
        });
        frame.textures_delta.clear();
        eframe::viewport_presentation::swap(ctx, viewport.ids.this, || Ok::<(), &str>(())).unwrap();
        returned
            .borrow_mut()
            .push((initially_hidden, has_real_controls));
    });
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.attach(&ctx);
    app.invoke(Command::TemplateEditor, Default::default());
    let session = app.windows.editor().unwrap();
    let mut frame = ctx.run_ui(Default::default(), |ui| app.paint_editor_viewport(ui.ctx()));
    let commands = frame.viewport_output[&editor_viewport(session)]
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                egui::ViewportCommand::Visible(_)
                    | egui::ViewportCommand::Minimized(_)
                    | egui::ViewportCommand::Focus
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    frame.textures_delta.clear();
    assert_eq!(
        *observed.borrow(),
        vec![(true, true)],
        "AccessKit requires a hidden new window; the real child frame must precede revealing it"
    );
    assert!(
        matches!(
            commands.as_slice(),
            [
                egui::ViewportCommand::Visible(true),
                egui::ViewportCommand::Minimized(false),
                egui::ViewportCommand::Focus,
            ]
        ),
        "Unexpected child visibility/focus commands: {commands:?}"
    );
    assert!(!app.windows.editor_closing());
}
