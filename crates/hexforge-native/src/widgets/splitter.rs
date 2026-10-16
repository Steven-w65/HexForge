use crate::style::HexForgeStyle;
use eframe::egui::{self, Ui};
pub fn show(
    ui: &mut Ui,
    s: &HexForgeStyle,
    height: &mut f32,
    min: f32,
    max: f32,
    width: f32,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), egui::Sense::hover());
    let r = ui.interact(
        rect.expand2(egui::vec2(0.0, 3.0)),
        ui.id().with("results-splitter"),
        egui::Sense::click_and_drag(),
    );
    r.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Slider,
            ui.is_enabled(),
            "Resize parsed results",
        )
    });
    ui.painter().rect_filled(
        rect,
        0,
        if r.hovered() || r.dragged() || r.has_focus() {
            s.palette.selection
        } else {
            s.palette.bg
        },
    );
    let anchor_id = r.id.with("original-drag-anchor");
    if r.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
        if let Some(at) = ui.input(|i| i.pointer.interact_pos()) {
            ui.ctx()
                .data_mut(|d| d.insert_temp(anchor_id, (*height, at.y)));
        }
    }
    if r.dragged() {
        if let Some((height_at_start, y)) = ui.ctx().data(|d| d.get_temp::<(f32, f32)>(anchor_id)) {
            if let Some(at) = ui.input(|i| i.pointer.interact_pos()) {
                *height = (height_at_start + y - at.y)
                    .clamp(min, max.max(min))
                    .round();
            }
        }
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        ui.ctx().data_mut(|d| d.remove::<(f32, f32)>(anchor_id));
    }
    if r.has_focus() {
        let step = if ui.input(|i| i.modifiers.shift) {
            5.0
        } else {
            20.0
        };
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            // This separator owns arrows like the source's stopPropagation.
            // Cancel egui's pre-queued spatial move so Home/End and the next
            // resize key still reach the same keyboard-focused separator.
            ui.ctx()
                .memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            *height = (*height + step).clamp(min, max.max(min));
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            ui.ctx()
                .memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            *height = (*height - step).clamp(min, max.max(min));
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home)) {
            *height = min;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::End)) {
            *height = max.max(min);
        }
    }
    r.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}
