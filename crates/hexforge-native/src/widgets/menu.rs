use super::button::{ButtonRole, ButtonSpec, StyledButton};
use crate::style::HexForgeStyle;
use eframe::egui::{self, Response, Ui};
pub fn item(
    ui: &mut Ui,
    s: &HexForgeStyle,
    label: &str,
    shortcut: Option<&str>,
    reason: Option<&str>,
    checked: Option<bool>,
) -> Response {
    paint_item(ui, s, label, shortcut, reason, checked, false)
}
/// The retained parsed-result flyout has two columns, and its scoped kbd
/// styling uses the original 11px webfont rather than the command OS font.
pub fn result_item(ui: &mut Ui, s: &HexForgeStyle, path: &str, offset: &str) -> Response {
    paint_item(ui, s, path, Some(offset), None, None, true)
}
fn paint_item(
    ui: &mut Ui,
    s: &HexForgeStyle,
    label: &str,
    shortcut: Option<&str>,
    reason: Option<&str>,
    checked: Option<bool>,
    result_row: bool,
) -> Response {
    let spec = ButtonSpec {
        disabled_reason: reason,
        width: Some(ui.available_width().max(1.0)),
        label_left_inset: result_row.then_some(9.0),
        checked,
        ..ButtonSpec::new(label).role(ButtonRole::Menu)
    };
    let r = StyledButton::show(ui, s, &spec);
    if let Some(shortcut) = shortcut {
        ui.painter().text(
            egui::pos2(r.rect.right() - 9.0, r.rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            if result_row {
                crate::style::fonts::regular(11.0)
            } else {
                crate::style::fonts::generic_monospace(12.0)
            },
            (if result_row {
                s.palette.muted
            } else {
                s.palette.text
            })
            .gamma_multiply(if reason.is_some() { 0.38 } else { 1.0 }),
        );
    }
    if checked == Some(true) {
        ui.painter().text(
            egui::pos2(r.rect.left() + 9.0, r.rect.center().y),
            egui::Align2::LEFT_CENTER,
            "✓",
            crate::style::fonts::regular(12.0),
            s.palette.text,
        );
    }
    r
}
pub fn frame(s: &HexForgeStyle) -> egui::Frame {
    egui::Frame::new()
        .fill(s.palette.panel)
        .stroke(egui::Stroke::new(1.0, s.palette.border_strong))
        .corner_radius(4)
        .inner_margin(4)
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 20,
            spread: 0,
            color: egui::Color32::from_black_alpha(61),
        })
}
