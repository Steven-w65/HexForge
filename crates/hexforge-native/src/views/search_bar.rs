use crate::{
    style::{fonts, HexForgeStyle},
    widgets::{
        button::{ButtonRole, ButtonSpec, StyledButton},
        input,
    },
};
use eframe::egui;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, InvokeContext, MainSnapshot},
    operation::OperationKind,
    policy::command_unavailable_reason,
    search::MatchDirection,
};
#[derive(Default)]
pub struct SearchBar {
    focus: Option<u64>,
}
pub(crate) fn frame<T>(
    ctx: &egui::Context,
    style: &HexForgeStyle,
    id: &str,
    stage: egui::Rect,
    inset: f32,
    body: impl FnOnce(&mut egui::Ui) -> T,
) -> T {
    let width = 390.0f32.min((stage.width() - inset - 20.0).max(240.0));
    egui::Area::new(egui::Id::new(id))
        .fade_in(false)
        .order(egui::Order::Middle)
        .fixed_pos(egui::pos2(
            stage.right() - inset - 10.0 - width,
            stage.top() + 7.0,
        ))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(style.palette.panel)
                .stroke(egui::Stroke::new(1.0, style.palette.border_strong))
                .corner_radius(5)
                .inner_margin(7)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 5],
                    blur: 14,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(38),
                })
                .show(ui, |ui| {
                    ui.set_width(width - 16.0);
                    ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                    body(ui)
                })
                .inner
        })
        .inner
}
pub(crate) fn button(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    label: &str,
    width: Option<f32>,
    reason: Option<&str>,
    tooltip: Option<&str>,
    close: bool,
) -> bool {
    let mut button_style = style.clone();
    if close {
        button_style.palette.text = style.palette.muted;
    }
    StyledButton::show(
        ui,
        &button_style,
        &ButtonSpec {
            width,
            height: Some(27.0),
            font_size: Some(if close { 19.0 } else { 11.0 }),
            disabled_reason: reason,
            tooltip,
            ..ButtonSpec::new(label).role(if close {
                ButtonRole::Transparent
            } else {
                ButtonRole::Normal
            })
        },
    )
    .clicked()
}
impl SearchBar {
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        style: &HexForgeStyle,
        s: &MainSnapshot,
        stage: egui::Rect,
    ) -> Vec<ControllerEvent> {
        let mut events = vec![];
        if !s.search.open || s.file.is_none() {
            self.focus = None;
            return events;
        }
        let mut raw = s.search.raw.clone();
        let id = egui::Id::new("search-bytes");
        let request_focus = self.focus != Some(s.search.focus_key);
        self.focus = Some(s.search.focus_key);
        let busy = s.busy.get(&OperationKind::Search).copied().unwrap_or(false);
        let mut submit = None;
        let ctx = ui.ctx().clone();
        let modal = s.dialog.is_some()
            || s.edit_dialog.is_some()
            || s.error.is_some()
            || s.editor_close_prompt;
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
                submit = Some((
                    false,
                    if mods.shift {
                        MatchDirection::Previous
                    } else {
                        MatchDirection::Next
                    },
                ));
            }
        }
        let inset = if s.minimap.enabled { 96.0 } else { 12.0 };
        frame(&ctx, style, "HexForge search bar", stage, inset, |ui| {
            if modal {
                ui.disable();
            }
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 199.6).max(50.0);
                let mut input_style = style.clone();
                input_style.metrics.input_height = 27.0;
                input_style.metrics.input_radius = 3;
                let r = input::text(ui, &input_style, id, &mut raw, width, "41 42 43");
                if request_focus || submit.is_some() {
                    r.request_focus();
                }
                let reason = s
                    .policy
                    .operation_busy
                    .then_some("Wait for the current operation to finish.");
                if button(ui, style, "Search", Some(53.6), reason, None, false) {
                    submit = Some((true, MatchDirection::Next));
                }
                for (command, label, tip) in [
                    (Command::SearchPrevious, "↑", "Previous match (Shift+F3)"),
                    (Command::SearchNext, "↓", "Next match (F3)"),
                ] {
                    let reason = command_unavailable_reason(command, &s.policy);
                    if button(
                        ui,
                        style,
                        label,
                        Some(24.0),
                        reason.as_deref(),
                        Some(tip),
                        false,
                    ) {
                        events.push(ControllerEvent::Invoke {
                            command,
                            context: InvokeContext::default(),
                        });
                    }
                }
                let reason = if busy {
                    Some("Wait for the current operation to finish.")
                } else if raw.is_empty() && s.search.query.is_empty() {
                    Some("No search to clear.")
                } else {
                    None
                };
                if button(ui, style, "Clear", Some(47.0), reason, None, false) {
                    events.push(ControllerEvent::Action(Action::ClearSearch));
                    ui.memory_mut(|m| m.request_focus(id));
                }
                if button(ui, style, "×", Some(26.0), None, Some("Close search"), true) {
                    events.push(ControllerEvent::Action(Action::CloseSearch));
                }
            });
            let detail = s.search.error.clone().unwrap_or_else(|| {
                if busy {
                    "Searching…".into()
                } else if s.search.query.is_empty() {
                    "Enter hexadecimal bytes, then press Enter.".into()
                } else {
                    let count = s.search.matches.len();
                    let index = s.search.active_index(s.selection.as_ref());
                    let text = index.map_or_else(
                        || format!("{count} {}", if count == 1 { "match" } else { "matches" }),
                        |i| format!("{} of {count}", i + 1),
                    );
                    format!(
                        "{text}{}{}",
                        if s.search.truncated { " (limited)" } else { "" },
                        if !s.search.is_current() {
                            " · Search to update results."
                        } else {
                            ""
                        }
                    )
                }
            });
            detail_label(ui, style, &detail, s.search.error.is_some());
        });
        if raw != s.search.raw {
            events.insert(0, ControllerEvent::Action(Action::SetSearchPattern(raw)));
        }
        if let Some((force, direction)) = submit {
            events.push(ControllerEvent::Action(Action::SubmitSearch {
                force,
                direction,
            }));
        }
        events
    }
}

pub(crate) fn detail_label(ui: &mut egui::Ui, style: &HexForgeStyle, text: &str, error: bool) {
    // Source .floating-bar__detail is text, not an interactive control. A
    // horizontal Ui inherits the 27px widget minimum and adds 12px to this
    // 15px line. Keep the source line box (and wrapping) explicit instead.
    // Its 5px top gap is already owned by the containing frame's spacing.
    let width = ui.available_width();
    let color = if error {
        style.palette.modified
    } else {
        style.palette.muted
    };
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        fonts::regular(11.0),
        color,
        (width - 4.0).max(1.0),
    );
    job.sections[0].format.line_height = Some(15.0);
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, galley.size().y.max(15.0)),
        egui::Sense::hover(),
    );
    ui.painter()
        .galley(rect.min + egui::vec2(2.0, 0.0), galley, color);
}
