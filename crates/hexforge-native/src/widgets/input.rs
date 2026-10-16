use crate::style::{fonts, HexForgeStyle};
use eframe::egui::{self, Response, Ui};
/// Native text editing/IME/undo with the existing input frame and metrics.
pub fn text(
    ui: &mut Ui,
    s: &HexForgeStyle,
    id: egui::Id,
    value: &mut String,
    width: f32,
    hint: &str,
) -> Response {
    ui.ctx().data_mut(|d| d.insert_temp(id, true));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.max(1.0), s.metrics.input_height),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, s.metrics.input_radius, s.palette.input);
    let inner = rect.shrink2(egui::vec2(7.0, 5.0));
    // `Ui::put` advances its parent's cursor to the inner text bounds. Keep it
    // in a child so the already allocated outer input owns its full flex width.
    let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let r = text_ui.put(
        inner,
        egui::TextEdit::singleline(value)
            .id(id)
            .font(fonts::regular(12.0))
            .text_color(s.palette.text)
            .hint_text(hint)
            .frame(egui::Frame::NONE)
            .margin(0)
            .desired_width(inner.width()),
    );
    let color = if r.has_focus() {
        s.palette.selection
    } else {
        s.palette.border
    };
    ui.painter().rect_stroke(
        rect,
        s.metrics.input_radius,
        egui::Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    super::dialog::register_focus(ui, &r, ui.is_enabled());
    r
}
pub fn combo<T: Clone + PartialEq>(
    ui: &mut Ui,
    s: &HexForgeStyle,
    id: egui::Id,
    value: &mut T,
    options: &[(T, &str)],
    width: f32,
) -> bool {
    // The caller's ID is the actual focus owner, not a salt hashed with a
    // transient card UI. Validation reveal and retained keyboard focus use
    // this same identity even after field reordering.
    ui.ctx().data_mut(|d| d.insert_temp(id, true));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.max(1.0), s.metrics.input_height),
        egui::Sense::hover(),
    );
    let response = ui.interact(rect, id, egui::Sense::click());
    let popup_id = id.with("popup");
    let enabled = ui.is_enabled() && !options.is_empty();
    super::dialog::register_focus(ui, &response, enabled);
    let focused = ui.memory(|m| m.has_focus(id));
    let mut changed = false;
    if focused && enabled {
        // Egui may already have queued directional focus at pass start.
        // Cancel that traversal before applying this select's own arrow key.
        if ui.input(|i| {
            !i.pointer.any_pressed()
                && [egui::Key::ArrowUp, egui::Key::ArrowDown]
                    .iter()
                    .any(|k| i.key_pressed(*k))
        }) {
            ui.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
        for key in [
            egui::Key::ArrowUp,
            egui::Key::ArrowDown,
            egui::Key::Home,
            egui::Key::End,
        ] {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                let current = options.iter().position(|(v, _)| v == value).unwrap_or(0);
                let next = match key {
                    egui::Key::ArrowUp => current.saturating_sub(1),
                    egui::Key::ArrowDown => (current + 1).min(options.len() - 1),
                    egui::Key::Home => 0,
                    _ => options.len() - 1,
                };
                if *value != options[next].0 {
                    *value = options[next].0.clone();
                    changed = true;
                }
            }
        }
    }
    let text = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or("", |(_, label)| *label);
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::new(egui::WidgetType::ComboBox);
        info.enabled = enabled;
        info.current_text_value = Some(text.to_owned());
        info
    });
    let opacity = if super::inert::visually_enabled(ui) && !options.is_empty() {
        1.0
    } else {
        0.4
    };
    ui.painter().rect_filled(
        rect,
        s.metrics.input_radius,
        s.palette.input.gamma_multiply(opacity),
    );
    let border = if focused || egui::Popup::is_id_open(ui.ctx(), popup_id) {
        s.palette.selection
    } else {
        s.palette.border
    };
    ui.painter().rect_stroke(
        rect,
        s.metrics.input_radius,
        egui::Stroke::new(1.0, border.gamma_multiply(opacity)),
        egui::StrokeKind::Inside,
    );
    let galley = ui.painter().layout_no_wrap(
        text.into(),
        fonts::regular(12.0),
        s.palette.text.gamma_multiply(opacity),
    );
    ui.painter()
        .with_clip_rect(
            rect.shrink2(egui::vec2(7.0, 0.0))
                .intersect(egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.right() - 24.0, rect.bottom()),
                )),
        )
        .galley(
            egui::pos2(rect.left() + 7.0, rect.center().y - galley.size().y / 2.0),
            galley,
            s.palette.text.gamma_multiply(opacity),
        );
    let center = egui::pos2(rect.right() - 12.0, rect.center().y);
    ui.painter().add(egui::Shape::line(
        vec![
            center + egui::vec2(-4.0, -2.0),
            center + egui::vec2(0.0, 2.0),
            center + egui::vec2(4.0, -2.0),
        ],
        egui::Stroke::new(1.2, s.palette.muted.gamma_multiply(opacity)),
    ));
    if enabled {
        egui::Popup::from_toggle_button_response(&response)
            .width(rect.width())
            .frame(super::menu::frame(s))
            .show(|ui| {
                ui.set_min_width((rect.width() - 10.0).max(1.0));
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (option, label) in options {
                            let spec = super::button::ButtonSpec {
                                width: Some(ui.available_width()),
                                height: Some(s.metrics.input_height),
                                checked: Some(option == value),
                                ..super::button::ButtonSpec::new(label)
                                    .role(super::button::ButtonRole::Menu)
                            };
                            let r = super::button::StyledButton::show(ui, s, &spec);
                            if option == value {
                                ui.painter().text(
                                    egui::pos2(r.rect.left() + 9.0, r.rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "✓",
                                    fonts::regular(12.0),
                                    s.palette.text,
                                );
                            }
                            if r.clicked() {
                                changed |= *value != *option;
                                *value = option.clone();
                                ui.memory_mut(|m| m.request_focus(id));
                                ui.close();
                            }
                        }
                    });
            });
    } else {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    changed
}
