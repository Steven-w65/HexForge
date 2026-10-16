use crate::{
    style::{fonts, HexForgeStyle},
    widgets::button::{ButtonRole, ButtonSpec, StyledButton},
};
use eframe::egui::{self, Align2};
use hexforge_core::dto::FileInfoDto;
#[derive(Default)]
pub struct FileStrip {
    pub expanded: bool,
    path: Option<String>,
}
pub fn format_size(bytes: u64, precise_kib: bool) -> String {
    // Match the source's nonnegative toFixed(2) labels (ties round up), rather
    // than Rust's ties-to-even formatting. u128 also keeps large byte counts
    // precise; this is a display calculation, not a logical file coordinate.
    let fixed = |unit: u64, suffix: &str| {
        let hundredths = (u128::from(bytes) * 100 + u128::from(unit) / 2) / u128::from(unit);
        format!("{}.{:02} {suffix}", hundredths / 100, hundredths % 100)
    };
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        if precise_kib {
            fixed(1024, "KiB")
        } else {
            format!("{} KiB", bytes as f64 / 1024.0)
        }
    } else if bytes < 1_073_741_824 {
        fixed(1_048_576, "MiB")
    } else {
        fixed(1_073_741_824, "GiB")
    }
}
impl FileStrip {
    pub fn height(&mut self, ui: &egui::Ui, file: Option<&FileInfoDto>) -> f32 {
        let path = file.map(|f| f.path.clone());
        if self.path != path {
            self.path = path;
            self.expanded = false;
        }
        if !self.expanded {
            return 42.0;
        }
        let width = (ui.max_rect().width() - 103.0).max(80.0);
        let path = file.map_or("", |f| f.path.as_str());
        let galley = ui.painter().layout(
            path.into(),
            fonts::regular(11.0),
            egui::Color32::WHITE,
            width,
        );
        42.0 + galley.size().y.max(16.0) + 16.0 + 4.0 + 9.0
    }
    pub fn show(&mut self, ui: &mut egui::Ui, style: &HexForgeStyle, file: Option<&FileInfoDto>) {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.rect_filled(r, 0, style.palette.surface);
        p.hline(
            r.x_range(),
            r.bottom() - 0.5,
            egui::Stroke::new(1.0, style.palette.border),
        );
        let at = r.min + egui::vec2(10.0, 13.0);
        let svg = |x: f32, y: f32| at + egui::vec2(x * 15.0 / 24.0, y * 15.0 / 24.0);
        let stroke = egui::Stroke::new(1.0, style.palette.address);
        p.add(egui::Shape::line(
            vec![
                svg(6.0, 3.0),
                svg(14.0, 3.0),
                svg(18.0, 7.0),
                svg(18.0, 21.0),
                svg(6.0, 21.0),
                svg(6.0, 3.0),
            ],
            stroke,
        ));
        p.add(egui::Shape::line(
            vec![svg(14.0, 3.0), svg(14.0, 8.0), svg(19.0, 8.0)],
            stroke,
        ));
        let Some(file) = file else {
            p.text(
                r.min + egui::vec2(32.0, 20.5),
                Align2::LEFT_CENTER,
                "No file open",
                fonts::regular(11.0),
                style.palette.muted,
            );
            return;
        };
        let compact = r.width() <= 720.0;
        let name_limit = if compact {
            r.width() * 0.42
        } else {
            (r.width() * 0.32).min(320.0)
        };
        let name_width = p
            .layout_no_wrap(file.name.clone(), fonts::bold(12.0), style.palette.text)
            .size()
            .x
            .min(name_limit);
        let name_rect =
            egui::Rect::from_min_size(r.min + egui::vec2(32.0, 0.0), egui::vec2(name_width, 41.0));
        super::results::cell(
            &p,
            name_rect,
            &file.name,
            fonts::bold(12.0),
            style.palette.text,
        );
        // The reference's title attributes reveal the full clipped value.
        // Hover-only regions do not introduce buttons or change keyboard focus.
        ui.interact(
            name_rect,
            ui.id().with(("file-name", &file.path)),
            egui::Sense::hover(),
        )
        .on_hover_text(&file.name);
        let mut x = r.left() + 32.0 + name_width + 7.0;
        for (text, color) in [
            ("·".to_owned(), style.palette.border_strong),
            (
                format_size(file.size.parse().unwrap_or(0), true),
                style.palette.muted,
            ),
            ("·".to_owned(), style.palette.border_strong),
        ] {
            if compact && text == "·" {
                continue;
            }
            let galley = p.layout_no_wrap(text, fonts::regular(11.0), color);
            p.galley(
                egui::pos2(x, r.top() + 20.5 - galley.size().y / 2.0),
                galley.clone(),
                color,
            );
            x += galley.size().x + 7.0;
        }
        let badge = if file.dirty {
            18.0 + p
                .layout_no_wrap(
                    "Modified".into(),
                    fonts::regular(11.0),
                    style.palette.modified,
                )
                .size()
                .x
        } else {
            0.0
        };
        if !compact {
            // The source path has an intrinsic flex width, not flex-grow:1.
            // Modified follows the visible path; only the toggle is right-aligned.
            let path_width = p
                .layout_no_wrap(file.path.clone(), fonts::regular(11.0), style.palette.muted)
                .size()
                .x
                .min((r.right() - 43.0 - badge - x).max(0.0));
            let path_rect =
                egui::Rect::from_min_size(egui::pos2(x, r.top()), egui::vec2(path_width, 41.0));
            super::results::cell(
                &p,
                path_rect,
                &file.path,
                fonts::regular(11.0),
                style.palette.muted,
            );
            ui.interact(
                path_rect,
                ui.id().with(("file-path", &file.path)),
                egui::Sense::hover(),
            )
            .on_hover_text(&file.path);
            x += path_width + 7.0;
        }
        if file.dirty {
            p.circle_filled(
                egui::pos2(x + 3.0, r.top() + 20.5),
                3.0,
                style.palette.modified,
            );
            p.text(
                egui::pos2(x + 11.0, r.top() + 20.5),
                Align2::LEFT_CENTER,
                "Modified",
                fonts::regular(11.0),
                style.palette.modified,
            );
        }
        let toggle = egui::Rect::from_min_size(
            egui::pos2(r.right() - 36.0, r.top() + 7.5),
            egui::vec2(26.0, 26.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(toggle), |ui| {
            let response = StyledButton::show(
                ui,
                style,
                &ButtonSpec {
                    width: Some(26.0),
                    height: Some(26.0),
                    tooltip: Some(if self.expanded {
                        "Hide full file details"
                    } else {
                        "Show full file details"
                    }),
                    accessible_name: Some(if self.expanded {
                        "Hide full file details"
                    } else {
                        "Show full file details"
                    }),
                    ..ButtonSpec::new("").role(ButtonRole::Transparent)
                },
            );
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_expanded(self.expanded));
            let color = if response.hovered() {
                style.palette.text
            } else {
                style.palette.muted
            };
            let origin = response.rect.center() - egui::vec2(7.0, 7.0);
            let icon = |x: f32, y: f32| origin + egui::vec2(x * 14.0 / 24.0, y * 14.0 / 24.0);
            let points = if self.expanded {
                vec![icon(7.0, 14.0), icon(12.0, 9.0), icon(17.0, 14.0)]
            } else {
                vec![icon(7.0, 10.0), icon(12.0, 15.0), icon(17.0, 10.0)]
            };
            ui.painter().add(egui::Shape::line(
                points,
                egui::Stroke::new(1.7 * 14.0 / 24.0, color),
            ));
            if response.clicked() {
                self.expanded = !self.expanded;
                ui.ctx().request_repaint();
            }
        });
        if self.expanded {
            let rect = egui::Rect::from_min_max(
                r.min + egui::vec2(32.0, 42.0),
                r.max - egui::vec2(32.0, 9.0),
            );
            let text = p.layout(
                file.path.clone(),
                fonts::regular(11.0),
                style.palette.text,
                (rect.width() - 42.0).max(1.0),
            );
            p.text(
                rect.min,
                Align2::LEFT_TOP,
                "PATH",
                fonts::regular(11.0),
                style.palette.address,
            );
            p.galley(
                rect.min + egui::vec2(42.0, 0.0),
                text.clone(),
                style.palette.text,
            );
            let y = text.size().y.max(16.0) + 4.0;
            p.text(
                rect.min + egui::vec2(0.0, y),
                Align2::LEFT_TOP,
                "SIZE",
                fonts::regular(11.0),
                style.palette.address,
            );
            p.text(
                rect.min + egui::vec2(42.0, y),
                Align2::LEFT_TOP,
                format!("{} bytes", file.size),
                fonts::regular(11.0),
                style.palette.muted,
            );
        }
    }
}
