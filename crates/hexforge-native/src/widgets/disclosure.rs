//! The source's CSS disclosure marker is a triangle, not a fallback font glyph.
//! Its 11px summary text has a measured 11.640625px marker/text advance.
use crate::style::fonts;
use eframe::egui::{self, Align2};

pub fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    open: bool,
    color: egui::Color32,
) {
    let left = rect.left();
    let center = rect.center().y;
    let points = if open {
        vec![
            egui::pos2(left, center - 3.0),
            egui::pos2(left + 8.0, center - 3.0),
            egui::pos2(left + 4.0, center + 4.0),
        ]
    } else {
        vec![
            egui::pos2(left, center - 4.0),
            egui::pos2(left, center + 4.0),
            egui::pos2(left + 7.0, center),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
    painter.text(
        egui::pos2(left + 11.640625, center),
        Align2::LEFT_CENTER,
        label,
        fonts::regular(11.0),
        color,
    );
}
