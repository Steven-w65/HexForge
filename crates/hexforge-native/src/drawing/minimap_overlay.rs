//! Shaded viewport composition on a bounded minimap tile. Logical coordinates
//! stay exact until the virtual origin is removed; clipping happens locally.
use eframe::egui::{self, Color32};
use hexforge_controller::{
    controller::Theme,
    hex::minimap_geometry::{LogicalY, SUBPIXELS},
};

/// `continuous_adjustment` retains the drag's sub-fixed-point pointer position.
/// It is local (less than one pixel), never a floating-point global row origin.
pub fn viewport_rect(
    container: egui::Rect,
    canvas_top: LogicalY,
    canvas_offset: LogicalY,
    height: f64,
    continuous_adjustment: f64,
) -> Option<egui::Rect> {
    if !container.min.is_finite()
        || !container.max.is_finite()
        || container.width() <= 0.0
        || container.height() <= 0.0
        || !height.is_finite()
        || height <= 0.0
        || height > u32::MAX as f64
        || !continuous_adjustment.is_finite()
        || continuous_adjustment.abs() > 1.0
    {
        return None;
    }
    let local_sub = canvas_top.0.checked_add(canvas_offset.0)?;
    // Reject far-away ranges in exact coordinates before any float conversion.
    // The extra pixel admits continuous residuals and partially visible edges.
    let lower = -((height.ceil() as i128 + 1) * SUBPIXELS);
    let upper = (f64::from(container.height()).ceil() as i128 + 1) * SUBPIXELS;
    if local_sub < lower || local_sub > upper {
        return None;
    }
    let local_y = local_sub as f64 / SUBPIXELS as f64 + continuous_adjustment;
    let rect = egui::Rect::from_min_size(
        container.min + egui::vec2(0.0, local_y as f32),
        egui::vec2(container.width(), height as f32),
    )
    .intersect(container);
    (rect.width() > 0.0 && rect.height() > 0.0).then_some(rect)
}

pub fn paint_viewport(
    painter: &egui::Painter,
    container: egui::Rect,
    rect: egui::Rect,
    theme: Theme,
    hovered: bool,
    active: bool,
) {
    let dark = theme == Theme::Dark;
    let alpha = if dark {
        if active {
            64
        } else if hovered {
            54
        } else {
            41
        }
    } else if active {
        51
    } else if hovered {
        38
    } else {
        26
    };
    let fill = if dark {
        Color32::from_rgba_unmultiplied(145, 165, 185, alpha)
    } else {
        Color32::from_rgba_unmultiplied(70, 100, 135, alpha)
    };
    let border = if active {
        if dark {
            Color32::from_rgb(31, 111, 235)
        } else {
            Color32::from_rgb(9, 105, 218)
        }
    } else if hovered {
        if dark {
            Color32::from_rgb(184, 212, 239)
        } else {
            Color32::from_rgb(87, 110, 138)
        }
    } else if dark {
        Color32::from_rgb(160, 185, 210)
    } else {
        Color32::from_rgb(70, 100, 135)
    };
    let p = painter.with_clip_rect(container);
    p.rect_filled(rect, 0, fill);
    p.rect_stroke(
        rect,
        0,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
}
