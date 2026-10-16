//! CSS values translated without introducing an egui design language.
pub mod fonts;
pub mod metrics;
pub mod palette;
use eframe::egui;
use hexforge_controller::controller::Theme;
use metrics::Metrics;
use palette::Palette;
#[derive(Debug, Clone)]
pub struct HexForgeStyle {
    pub palette: Palette,
    pub metrics: Metrics,
    pub pixels_per_point: f32,
}
impl HexForgeStyle {
    pub fn for_theme(theme: Theme, pixels_per_point: f32) -> Self {
        Self {
            palette: Palette::new(theme),
            metrics: Metrics::default(),
            pixels_per_point: if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
                pixels_per_point
            } else {
                1.0
            },
        }
    }
    pub fn apply(&self, ctx: &egui::Context) {
        let p = &self.palette;
        let mut s = egui::Style::default();
        // The source has no CSS transitions for disclosure or scrollbar
        // allocation. Egui's default fade changes card widths over several
        // frames; explicit smooth scroll/reveal uses its separate policy.
        s.animation_time = 0.0;
        s.text_styles = [
            (egui::TextStyle::Small, fonts::regular(11.0)),
            (egui::TextStyle::Body, fonts::regular(12.0)),
            (egui::TextStyle::Button, fonts::regular(12.0)),
            (egui::TextStyle::Monospace, fonts::regular(13.0)),
            (egui::TextStyle::Heading, fonts::semibold(13.0)),
        ]
        .into_iter()
        .collect();
        s.spacing.item_spacing = egui::vec2(6.0, 3.0);
        s.spacing.button_padding = egui::vec2(9.0, 0.0);
        s.spacing.interact_size = egui::vec2(25.0, 27.0);
        s.spacing.indent = 9.0;
        s.spacing.scroll.bar_width = 6.0;
        s.visuals = if p.theme == Theme::Dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        s.visuals.override_text_color = Some(p.text);
        s.visuals.panel_fill = p.bg;
        s.visuals.window_fill = p.panel;
        s.visuals.extreme_bg_color = p.input;
        s.visuals.faint_bg_color = p.card;
        s.visuals.window_stroke = egui::Stroke::new(1.0, p.border_strong);
        s.visuals.window_corner_radius = egui::CornerRadius::same(7);
        s.visuals.selection.bg_fill = p.selection;
        s.visuals.selection.stroke = egui::Stroke::new(1.0, p.selection_text);
        s.visuals.hyperlink_color = p.address;
        s.visuals.warn_fg_color = p.modified;
        s.visuals.error_fg_color = p.modified;
        s.visuals.disabled_alpha = 0.4;
        for v in [
            &mut s.visuals.widgets.noninteractive,
            &mut s.visuals.widgets.inactive,
        ] {
            v.bg_fill = p.input;
            v.weak_bg_fill = p.button;
            v.bg_stroke = egui::Stroke::new(1.0, p.border);
            v.fg_stroke = egui::Stroke::new(1.0, p.text);
            v.corner_radius = egui::CornerRadius::same(4);
        }
        for v in [
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
            &mut s.visuals.widgets.open,
        ] {
            v.bg_fill = p.hover;
            v.weak_bg_fill = p.hover;
            v.bg_stroke = egui::Stroke::new(1.0, p.selection);
            v.fg_stroke = egui::Stroke::new(1.0, p.text);
            v.corner_radius = egui::CornerRadius::same(4);
            v.expansion = 0.0;
        }
        let theme = if p.theme == Theme::Dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(theme);
        ctx.set_style_of(theme, s);
    }
}
