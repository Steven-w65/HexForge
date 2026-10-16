//! A modal's background is inert, not policy-disabled. Preserve its ordinary
//! CSS colors while denying input; the modal backdrop supplies the shading.
use eframe::egui;

const INERT: &str = "HexForge inert background";

pub fn builder(builder: egui::UiBuilder, inert: bool) -> egui::UiBuilder {
    if inert {
        builder.ui_stack_info(egui::UiStackInfo::default().with_tag(INERT))
    } else {
        builder
    }
}

pub fn is_inert(ui: &egui::Ui) -> bool {
    ui.stack()
        .iter()
        .any(|stack| stack.info.tags.contains(INERT))
}

/// Call before painting the scope, so standard egui inputs and all nested
/// custom controls remain non-interactive without inheriting disabled alpha.
pub fn block(ui: &mut egui::Ui) {
    if is_inert(ui) {
        let opacity = ui.opacity();
        ui.disable();
        ui.set_opacity(opacity);
    }
}

pub fn visually_enabled(ui: &egui::Ui) -> bool {
    ui.is_enabled() || is_inert(ui)
}
