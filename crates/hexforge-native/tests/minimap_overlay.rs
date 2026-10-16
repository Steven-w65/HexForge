//! Actual native overlay paint: logical coordinates are transformed before
//! clipping, never converted to f32 while they still contain a huge origin.
use eframe::egui::{self, Color32, Pos2, Rect};
use hexforge_controller::{
    controller::Theme,
    hex::minimap_geometry::{LogicalY, SUBPIXELS},
};
use hexforge_native::drawing::minimap_overlay::{paint_viewport, viewport_rect};

fn container() -> Rect {
    Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(84.0, 8.0))
}
fn paint(canvas_top: LogicalY, offset: LogicalY, height: f64, residual: f64) -> egui::FullOutput {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 40.0))),
            ..Default::default()
        },
        |ui| {
            if let Some(rect) = viewport_rect(container(), canvas_top, offset, height, residual) {
                paint_viewport(ui.painter(), container(), rect, Theme::Dark, false, false);
            }
        },
    );
    output.textures_delta.clear();
    output
}
fn fills(output: &egui::FullOutput) -> Vec<Rect> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.fill == Color32::from_rgba_unmultiplied(145, 165, 185, 41) =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn shaded_box_transforms_and_clips_canvas_local_coordinates_at_the_tile_edge() {
    let output = paint(
        LogicalY(24 * SUBPIXELS),
        LogicalY(-20 * SUBPIXELS),
        5.0,
        0.0,
    );
    assert_eq!(
        fills(&output),
        [Rect::from_min_size(
            Pos2::new(10.0, 24.0),
            egui::vec2(84.0, 4.0)
        )]
    );
    assert!(output
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.stroke.width == 1.0))
        .all(|s| s.clip_rect == container()));
}

#[test]
fn plain_canvas_local_shaded_box_has_the_same_offset_transform() {
    let output = paint(
        LogicalY(24 * SUBPIXELS),
        LogicalY(-20 * SUBPIXELS),
        2.0,
        0.0,
    );
    assert_eq!(
        fills(&output),
        [Rect::from_min_size(
            Pos2::new(10.0, 24.0),
            egui::vec2(84.0, 2.0)
        )]
    );
}

#[test]
fn huge_origin_is_subtracted_exactly_before_shaded_box_paint() {
    let origin = (((1i128 << 56) + 17) * 2) * SUBPIXELS;
    let output = paint(
        LogicalY(origin + 6 * SUBPIXELS),
        LogicalY(-origin),
        2.0,
        0.0,
    );
    assert_eq!(
        fills(&output),
        [Rect::from_min_size(
            Pos2::new(10.0, 26.0),
            egui::vec2(84.0, 2.0)
        )]
    );
}

#[test]
fn overlay_preserves_the_continuous_drag_residual_and_rejects_invisible_ranges() {
    let origin = (1i128 << 100) + 19;
    let r = viewport_rect(
        container(),
        LogicalY(origin + 4 * SUBPIXELS),
        LogicalY(-origin),
        2.0,
        0.0003,
    )
    .unwrap();
    assert!((f64::from(r.top()) - 24.0003).abs() < 0.00001);
    assert!((f64::from(r.height()) - 2.0).abs() < 0.00001);
    assert!(viewport_rect(container(), LogicalY(0), LogicalY(-origin), 2.0, 0.0).is_none());
    assert!(viewport_rect(container(), LogicalY(origin), LogicalY(0), 2.0, 0.0).is_none());
    assert!(viewport_rect(container(), LogicalY(i128::MAX), LogicalY(1), 2.0, 0.0).is_none());
    assert!(viewport_rect(container(), LogicalY(0), LogicalY(0), 0.0, 0.0).is_none());
    assert!(viewport_rect(container(), LogicalY(0), LogicalY(0), f64::NAN, 0.0).is_none());
}
