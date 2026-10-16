mod support;
use eframe::egui::{self, Event};
use hexforge_controller::{command::Command, controller::InitialState};
use hexforge_core::operations::CoreService;
use hexforge_native::{
    app::NativeApp,
    platform::preferences::{PreferenceError, PreferenceStore, ResultsPreferences},
};
use std::sync::{Arc, Mutex};
use support::ui_harness::{CapturedFrame, NativeUiHarness};

#[derive(Default)]
struct MemoryPreferences(Mutex<Option<ResultsPreferences>>);
impl PreferenceStore for MemoryPreferences {
    fn read_results_preferences(&self) -> Result<Option<ResultsPreferences>, PreferenceError> {
        Ok(*self.0.lock().unwrap())
    }
    fn write_results_preferences(&self, v: &ResultsPreferences) -> Result<(), PreferenceError> {
        *self.0.lock().unwrap() = Some(*v);
        Ok(())
    }
}
fn make_app(h: &NativeUiHarness, store: Arc<MemoryPreferences>) -> NativeApp {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.attach(&h.context);
    app.configure_preferences(store);
    app
}
fn frame(h: &NativeUiHarness, app: &mut NativeApp, events: Vec<Event>) -> CapturedFrame {
    h.main_frame(
        app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 700.0),
            )),
            events,
            ..Default::default()
        },
    )
}
fn splitter(f: &CapturedFrame) -> Option<egui::Rect> {
    f.widgets
        .iter()
        .find(|w| w.label == "Resize parsed results")
        .map(|w| w.rect)
}
fn focus_splitter(h: &NativeUiHarness, app: &mut NativeApp) {
    // The source splitter's prevented pointerdown preserves existing focus.
    // Its keyboard path is entered through Tab, not by assuming a click focuses.
    for _ in 0..32 {
        let f = frame(
            h,
            app,
            vec![Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        if let Some(focused) = h.context.memory(|m| m.focused()) {
            if f.output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(id, node)| {
                    *id == focused.accesskit_id() && node.label() == Some("Resize parsed results")
                })
            {
                return;
            }
        }
    }
    panic!("Keyboard Tab must reach the actual results splitter");
}
fn key(h: &NativeUiHarness, app: &mut NativeApp, key: egui::Key) {
    frame(
        h,
        app,
        vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

#[test]
fn real_pane_keyboard_bounds_persist_and_restore_after_reopening() {
    let h = NativeUiHarness::new();
    let store = Arc::new(MemoryPreferences::default());
    let mut app = make_app(&h, store.clone());
    frame(&h, &mut app, vec![]);
    focus_splitter(&h, &mut app);
    key(&h, &mut app, egui::Key::ArrowUp);
    assert_eq!(app.results_preferences().height, 240.0);
    key(&h, &mut app, egui::Key::Home);
    assert_eq!(app.results_preferences().height, 84.0);
    key(&h, &mut app, egui::Key::End);
    assert_eq!(app.results_preferences().height, 436.0);
    drop(app);
    let second_h = NativeUiHarness::new();
    let mut second = make_app(&second_h, store);
    let f = frame(&second_h, &mut second, vec![]);
    assert_eq!(second.results_preferences().height, 436.0);
    assert!(splitter(&f).is_some());
    assert!(second.main_snapshot().error.is_none());
}
#[test]
fn pane_drag_keeps_its_anchor_outside_the_viewport_and_stops_after_release() {
    let h = NativeUiHarness::new();
    let mut app = make_app(&h, Arc::new(MemoryPreferences::default()));
    let f = frame(&h, &mut app, vec![]);
    let p = splitter(&f).unwrap().center();
    frame(&h, &mut app, vec![Event::PointerMoved(p)]);
    frame(
        &h,
        &mut app,
        vec![Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    for (y, wanted) in [
        (p.y - 40.0, 260.0),
        (-1000.0, 436.0),
        (p.y - 40.0, 260.0),
        (1300.0, 84.0),
    ] {
        frame(&h, &mut app, vec![Event::PointerMoved(egui::pos2(p.x, y))]);
        assert_eq!(app.results_preferences().height, wanted);
    }
    frame(
        &h,
        &mut app,
        vec![Event::PointerButton {
            pos: egui::pos2(p.x, 1300.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    frame(&h, &mut app, vec![Event::PointerMoved(p)]);
    assert_eq!(app.results_preferences().height, 84.0);
    assert!(app.main_snapshot().file.is_none());
    assert!(app.main_snapshot().selection.is_none());
}
#[test]
fn collapsed_results_remove_the_splitter_without_forgetting_the_expanded_height() {
    let h = NativeUiHarness::new();
    let mut app = make_app(&h, Arc::new(MemoryPreferences::default()));
    frame(&h, &mut app, vec![]);
    focus_splitter(&h, &mut app);
    key(&h, &mut app, egui::Key::ArrowUp);
    assert_eq!(app.results_preferences().height, 240.0);
    app.invoke(Command::ToggleRightPanel, Default::default());
    let f = frame(&h, &mut app, vec![]);
    assert!(splitter(&f).is_none());
    assert!(app.results_preferences().collapsed);
    assert!(f.widgets.iter().any(|w| w.label == "Expand parsed results"));
    app.invoke(Command::ToggleRightPanel, Default::default());
    let f = frame(&h, &mut app, vec![]);
    assert!(splitter(&f).is_some());
    assert_eq!(app.results_preferences().height, 240.0);
}
