use crate::{style::HexForgeStyle, widgets::input};
use eframe::egui;
use hexforge_controller::{
    action::Action,
    controller::{ControllerEvent, MainSnapshot},
};
#[derive(Default)]
pub struct GotoBar {
    focus: Option<u64>,
}
impl GotoBar {
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        style: &HexForgeStyle,
        s: &MainSnapshot,
        stage: egui::Rect,
    ) -> Vec<ControllerEvent> {
        let mut events = vec![];
        if !s.goto.open || s.file.is_none() {
            self.focus = None;
            return events;
        }
        let mut raw = s.goto.raw.clone();
        let id = egui::Id::new("goto-offset");
        let request_focus = self.focus != Some(s.goto.focus_key);
        self.focus = Some(s.goto.focus_key);
        let ctx = ui.ctx().clone();
        let modal = s.dialog.is_some()
            || s.edit_dialog.is_some()
            || s.error.is_some()
            || s.editor_close_prompt;
        let mut submit = false;
        if !modal && ctx.memory(|m| m.has_focus(id)) {
            let mods = ctx.input(|i| i.modifiers);
            let composing = crate::platform::input::key_is_composing(
                &ctx,
                hexforge_controller::shortcuts::WindowKind::Main,
                egui::Key::Enter,
                mods,
            );
            if !composing
                && !mods.ctrl
                && !mods.alt
                && !mods.mac_cmd
                && ctx.input_mut(|i| i.consume_key(mods, egui::Key::Enter))
            {
                submit = true;
            }
        }
        super::search_bar::frame(
            &ctx,
            style,
            "HexForge Go To bar",
            stage,
            if s.minimap.enabled { 96.0 } else { 12.0 },
            |ui| {
                if modal {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 63.2).max(50.0);
                    let mut input_style = style.clone();
                    input_style.metrics.input_height = 27.0;
                    input_style.metrics.input_radius = 3;
                    let r = input::text(ui, &input_style, id, &mut raw, width, "0x100 or 256");
                    if request_focus || submit {
                        r.request_focus();
                    }
                    submit |=
                        super::search_bar::button(ui, style, "Go", Some(27.2), None, None, false);
                    if super::search_bar::button(
                        ui,
                        style,
                        "×",
                        Some(26.0),
                        None,
                        Some("Close Go To"),
                        true,
                    ) {
                        events.push(ControllerEvent::Action(Action::CloseGoto));
                    }
                });
                super::search_bar::detail_label(
                    ui,
                    style,
                    s.goto
                        .error
                        .as_deref()
                        .unwrap_or("Enter a decimal or 0x-prefixed hexadecimal offset."),
                    s.goto.error.is_some(),
                );
            },
        );
        if raw != s.goto.raw {
            events.insert(0, ControllerEvent::Action(Action::SetGotoValue(raw)));
        }
        if submit {
            events.push(ControllerEvent::Action(Action::SubmitGoto));
        }
        events
    }
}
