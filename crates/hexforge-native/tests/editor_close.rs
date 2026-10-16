use eframe::egui::{self, Event};
use hexforge_controller::{command::Command, controller::InitialState};
use hexforge_native::app::NativeApp;
use std::sync::Arc;
#[test]
fn dont_save_button_prepares_editor_destruction_after_restoring_checkpoint() {
    let mut app = NativeApp::new(
        InitialState::default(),
        Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.invoke(Command::TemplateEditor, Default::default());
    let session = app.windows.editor().unwrap();
    app.editor.as_mut().unwrap().model.name = "Changed".into();
    app.publish_editor().unwrap();
    app.editor_message(
        hexforge_controller::template::coordinator::EditorMessage::CloseRequested { session },
    );
    assert!(app.editor_snapshot().close_prompt);
    let frame = |app: &mut NativeApp, events: Vec<Event>| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(660.0, 760.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.paint_editor(ui),
        );
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) if t.galley.job.text == "Don't Save" => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .next()
    };
    frame(&mut app, vec![]);
    let p = frame(&mut app, vec![]).expect("native close dialog");
    frame(&mut app, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
    assert_eq!(app.main_snapshot().template.name, "Untitled");
    assert!(app.windows.editor_closing());
    // The native adapter still waits for the actual child-window destruction.
    assert!(app.windows.editor().is_some());
    app.editor_destroyed(session);
    assert!(app.windows.editor().is_none());
}
