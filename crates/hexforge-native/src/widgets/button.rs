use crate::style::{fonts, HexForgeStyle};
use eframe::egui::{self, Color32, Response, Ui};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonRole {
    Normal,
    Primary,
    Menu,
    Transparent,
    DialogClose,
}
#[derive(Debug, Clone, Copy)]
pub enum VisualState {
    Normal,
    Hover,
    Pressed,
    Focused,
    Disabled,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonVisual {
    pub fill: Color32,
    pub text: Color32,
    pub focus: Option<Color32>,
    pub opacity: f32,
}
#[derive(Debug, Clone)]
pub struct ButtonSpec<'a> {
    pub label: &'a str,
    /// Semantic name for an icon-only control; does not change its paint.
    pub accessible_name: Option<&'a str>,
    pub role: ButtonRole,
    pub disabled_reason: Option<&'a str>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub font_size: Option<f32>,
    /// Icon-only text has no baseline to align with surrounding words. Center
    /// its painted glyph bounds, not the font's asymmetric line box.
    pub icon_only: bool,
    /// CSS grid's content inset for rows without a checkmark column.
    pub label_left_inset: Option<f32>,
    pub checked: Option<bool>,
    pub tooltip: Option<&'a str>,
}
impl<'a> ButtonSpec<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            accessible_name: None,
            role: ButtonRole::Normal,
            disabled_reason: None,
            width: None,
            height: None,
            font_size: None,
            icon_only: false,
            label_left_inset: None,
            checked: None,
            tooltip: None,
        }
    }
    pub fn role(mut self, role: ButtonRole) -> Self {
        self.role = role;
        self
    }
    pub fn disabled(mut self, reason: &'a str) -> Self {
        self.disabled_reason = Some(reason);
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
    pub fn visual(&self, s: &HexForgeStyle, state: VisualState) -> ButtonVisual {
        let p = &s.palette;
        let active = matches!(state, VisualState::Hover | VisualState::Pressed)
            || matches!(self.role, ButtonRole::Menu | ButtonRole::DialogClose)
                && matches!(state, VisualState::Focused);
        let (fill, text) = match self.role {
            ButtonRole::Primary => (
                if active {
                    Color32::from_rgb(
                        (p.selection.r() as f32 * 1.12).min(255.0) as u8,
                        (p.selection.g() as f32 * 1.12).min(255.0) as u8,
                        (p.selection.b() as f32 * 1.12).min(255.0) as u8,
                    )
                } else {
                    p.selection
                },
                p.selection_text,
            ),
            ButtonRole::Normal => (if active { p.hover } else { p.button }, p.text),
            ButtonRole::Menu | ButtonRole::Transparent => (
                if active {
                    p.hover
                } else {
                    Color32::TRANSPARENT
                },
                p.text,
            ),
            ButtonRole::DialogClose => (
                if active {
                    p.hover
                } else {
                    Color32::TRANSPARENT
                },
                if active { p.text } else { p.muted },
            ),
        };
        ButtonVisual {
            fill,
            text,
            focus: (!matches!(self.role, ButtonRole::Menu | ButtonRole::DialogClose)
                && matches!(state, VisualState::Focused))
            .then_some(p.selection),
            opacity: if matches!(state, VisualState::Disabled) {
                if self.role == ButtonRole::Menu {
                    0.38
                } else {
                    0.4
                }
            } else {
                1.0
            },
        }
    }
}
pub struct StyledButton;
impl StyledButton {
    pub fn show(ui: &mut Ui, s: &HexForgeStyle, spec: &ButtonSpec<'_>) -> Response {
        let enabled = ui.is_enabled() && spec.disabled_reason.is_none();
        let font = fonts::regular(spec.font_size.unwrap_or(s.metrics.font_body));
        let galley = ui
            .painter()
            .layout_no_wrap(spec.label.into(), font, s.palette.text);
        let height = spec.height.unwrap_or(if spec.role == ButtonRole::Menu {
            33.0
        } else {
            s.metrics.button_height
        });
        let width = spec.width.unwrap_or(galley.size().x + 18.0);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(width, height),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let state = if !super::inert::visually_enabled(ui) || spec.disabled_reason.is_some() {
            VisualState::Disabled
        } else if response.is_pointer_button_down_on() {
            VisualState::Pressed
        } else if response.hovered() {
            VisualState::Hover
        } else if response.has_focus() && crate::platform::input::focus_visible(ui.ctx()) {
            VisualState::Focused
        } else {
            VisualState::Normal
        };
        let visual = spec.visual(s, state);
        ui.painter().rect_filled(
            rect,
            if spec.role == ButtonRole::DialogClose {
                4
            } else {
                s.metrics.button_radius
            },
            visual.fill.gamma_multiply(visual.opacity),
        );
        let at = if spec.icon_only && galley.mesh_bounds.is_positive() {
            rect.center() - galley.mesh_bounds.center().to_vec2()
        } else {
            egui::pos2(
                if spec.role == ButtonRole::Menu {
                    rect.left() + spec.label_left_inset.unwrap_or(30.0)
                } else {
                    rect.center().x - galley.size().x / 2.0
                },
                rect.center().y - galley.size().y / 2.0,
            )
        };
        ui.painter().galley_with_override_text_color(
            at,
            galley,
            visual.text.gamma_multiply(visual.opacity),
        );
        if let Some(color) = visual.focus {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                s.metrics.button_radius,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Outside,
            );
        }
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                enabled,
                spec.accessible_name.unwrap_or(spec.label),
            );
            info.selected = spec.checked;
            info
        });
        ui.ctx().accesskit_node_builder(response.id, |node| {
            if spec.role == ButtonRole::Menu {
                node.set_role(if spec.checked.is_some() {
                    egui::accesskit::Role::MenuItemCheckBox
                } else {
                    egui::accesskit::Role::MenuItem
                });
            }
            if let Some(reason) = spec.disabled_reason.or(spec.tooltip) {
                node.set_description(reason);
            }
        });
        super::dialog::register_focus(ui, &response, enabled);
        if let Some(reason) = spec.disabled_reason.or(spec.tooltip) {
            response.on_hover_text(reason)
        } else {
            response
        }
    }
}
