//! Test-only binary workflow input/paint through the real native presenter. These
//! in-memory frames do not launch Windows dialogs or claim presented startup.
use eframe::egui::{self, Event, Pos2};
use hexforge_controller::controller::InitialState;
use hexforge_core::operations::CoreService;
use hexforge_native::app::NativeApp;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub fn native() -> (NativeApp, egui::Context) {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    paint(&mut app, &ctx, vec![]);
    (app, ctx)
}
pub fn paint(app: &mut NativeApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Pos2)> {
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
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => {
                let center = text.pos + text.galley.size() / 2.0;
                shape
                    .clip_rect
                    .contains(center)
                    .then(|| (text.galley.text().into(), center))
            }
            _ => None,
        })
        .collect()
}
pub fn click(app: &mut NativeApp, ctx: &egui::Context, label: &str) {
    // Native modal measurement has a sizing pass before accepting input.
    paint(app, ctx, vec![]);
    let pos = paint(app, ctx, vec![])
        .into_iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("missing real native control: {label}"))
        .1;
    paint(app, ctx, vec![Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        paint(
            app,
            ctx,
            vec![Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: Default::default(),
            }],
        );
    }
}
pub fn wait_until(app: &mut NativeApp, mut complete: impl FnMut(&NativeApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.pump();
        if complete(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native workflow stalled: {:#?}",
            app.main_snapshot()
        );
        std::thread::yield_now();
    }
}
pub fn settle(app: &mut NativeApp) {
    wait_until(app, |app| !app.main_snapshot().policy.operation_busy);
}
