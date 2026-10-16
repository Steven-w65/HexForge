//! Source-renderer contracts observed on native paint, not a browser-canvas mock.
use eframe::egui::{self, Color32, Pos2};
use hexforge_controller::{
    controller::{
        AppController, ByteSelection, InitialState, MainSnapshot, RowWidth, ViewportPage,
        WindowSnapshot,
    },
    hex::layout::{HexLayout, HexMetrics},
    shortcuts::WindowKind,
};
use hexforge_core::dto::{FileInfoDto, PageResponse};
use hexforge_native::{
    drawing::hex::{paint_hex, HexPaintStats, HexScene},
    style::{fonts, HexForgeStyle},
};

fn snapshot(size: u64) -> MainSnapshot {
    let WindowSnapshot::Main(mut s) =
        AppController::new(InitialState::default()).snapshot(WindowKind::Main)
    else {
        unreachable!()
    };
    s.file = Some(FileInfoDto {
        name: "fixture.bin".into(),
        path: "fixture.bin".into(),
        size: size.to_string(),
        revision: "1".into(),
        dirty: false,
    });
    s.page = Some(ViewportPage {
        generation: 1,
        data: PageResponse {
            offset: "0".into(),
            bytes: vec![0x41; 64],
            modified_offsets: vec![],
            revision: "1".into(),
        },
    });
    s
}
fn paint(s: &MainSnapshot, top: u64, height: f32) -> egui::FullOutput {
    paint_with_stats(s, top, height).0
}
fn paint_with_stats(s: &MainSnapshot, top: u64, height: f32) -> (egui::FullOutput, HexPaintStats) {
    paint_at_dpr(s, top, height, 1.0)
}
fn paint_at_dpr(
    s: &MainSnapshot,
    top: u64,
    height: f32,
    dpr: f32,
) -> (egui::FullOutput, HexPaintStats) {
    let ctx = egui::Context::default();
    fonts::install(&ctx);
    let layout = HexLayout::new(
        HexMetrics {
            width: 1200.0,
            char_width: 8.0,
        },
        s.row_width,
    );
    let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, height));
    let style = HexForgeStyle::for_theme(s.theme, dpr);
    let mut stats = HexPaintStats::default();
    let mut input = egui::RawInput {
        screen_rect: Some(rect),
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(dpr);
    let mut output = ctx.run_ui(input, |ui| {
        stats = paint_hex(
            ui.painter(),
            &HexScene {
                rect,
                layout: &layout,
                snapshot: s,
                top_row: top,
                horizontal_px: 0.0,
            },
            &style,
        );
    });
    output.textures_delta.clear();
    (output, stats)
}
fn rects(output: &egui::FullOutput, color: Color32) -> Vec<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
}
fn search_color() -> Color32 {
    Color32::from_rgba_unmultiplied(139, 111, 42, 107)
}

#[test]
fn native_dpi_retains_background_content_overlay_composition_and_bounded_clips() {
    let mut s = snapshot(64);
    s.selection = Some(ByteSelection { start: 0, end: 0 });
    for dpr in [1.0, 1.25, 1.5, 2.0] {
        let (output, _) = paint_at_dpr(&s, 0, 100.0, dpr);
        assert_eq!(output.pixels_per_point, dpr);
        let background = output.shapes.iter().position(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.fill == Color32::from_rgb(17,20,24) && r.rect.size() == egui::vec2(1200.0,100.0))).unwrap();
        let content = output
            .shapes
            .iter()
            .position(|shape| {
                matches!(&shape.shape,
            egui::Shape::Text(t) if t.galley.text() == "41")
            })
            .unwrap();
        let overlay = output
            .shapes
            .iter()
            .position(|shape| {
                matches!(&shape.shape,
            egui::Shape::Rect(r) if r.fill == Color32::from_rgba_unmultiplied(31,111,235,122))
            })
            .unwrap();
        assert!(background < content && content < overlay);
        for shape in &output.shapes[background..] {
            assert!(shape.clip_rect.left() >= 0.0 && shape.clip_rect.top() >= 0.0);
            assert!(shape.clip_rect.right() <= 1200.0 && shape.clip_rect.bottom() <= 100.0);
        }
        assert_eq!(
            rects(&output, Color32::from_rgba_unmultiplied(31, 111, 235, 122)).len(),
            2
        );
    }
}

#[test]
fn hex_headings_dividers_and_uppercase_bytes_keep_source_origins() {
    let mut s = snapshot(0x1_0000_0000);
    s.page.as_mut().unwrap().data.bytes = vec![0x41, 0x00, 0xaf];
    let output = paint(&s, 0, 200.0);
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts
            .iter()
            .take(3)
            .map(|t| (t.galley.text(), t.pos.x))
            .collect::<Vec<_>>(),
        vec![("OFFSET", 16.0), ("HEX BYTES", 160.0), ("ASCII", 576.0)]
    );
    for label in ["00000000", "41", "00", "AF", "A"] {
        assert!(
            texts.iter().any(|t| t.galley.text() == label),
            "Missing {label}"
        );
    }
    assert_eq!(texts.iter().filter(|t| t.galley.text() == ".").count(), 2);
    let dividers: Vec<_> = rects(&output, Color32::from_rgb(48, 54, 61))
        .into_iter()
        .filter(|r| r.width() == 1.0 && r.height() == 200.0)
        .collect();
    assert_eq!(
        dividers.iter().map(|r| r.left()).collect::<Vec<_>>(),
        vec![152.0, 564.0]
    );
}

#[test]
fn ascii_glyphs_and_selected_cells_align_in_both_source_row_widths() {
    for (width, ascii_x, selected_xs) in [
        (
            RowWidth::Sixteen,
            576.0,
            [608.0, 616.0, 624.0, 632.0, 648.0, 656.0],
        ),
        (
            RowWidth::ThirtyTwo,
            976.0,
            [1008.0, 1016.0, 1024.0, 1032.0, 1048.0, 1056.0],
        ),
    ] {
        let mut s = snapshot(64);
        s.row_width = width;
        s.page.as_mut().unwrap().data.bytes = (0..width.bytes()).map(|n| 0x41 + n as u8).collect();
        s.selection = Some(ByteSelection { start: 4, end: 9 });
        let output = paint(&s, 0, 100.0);
        let selected: Vec<_> = rects(&output, Color32::from_rgba_unmultiplied(31, 111, 235, 122))
            .into_iter()
            .filter(|r| r.width() == 8.0)
            .collect();
        assert_eq!(
            selected.iter().map(|r| r.left()).collect::<Vec<_>>(),
            selected_xs
        );
        for (index, x) in (4..=9).zip(selected_xs) {
            assert!(output.shapes.iter().any(|c| matches!(&c.shape, egui::Shape::Text(t) if t.galley.text() == char::from(0x41 + index).to_string() && t.pos.x == x && t.pos.x >= ascii_x)));
        }
    }
}

#[test]
fn every_multibyte_search_cell_and_before_viewport_intersection_are_painted() {
    let mut s = snapshot(1000);
    s.matches = vec![4];
    s.search.match_length = 3;
    let output = paint(&s, 0, 80.0);
    assert_eq!(rects(&output, search_color()).len(), 6);
    s.matches = vec![14];
    s.search.match_length = 4;
    let output = paint(&s, 1, 80.0);
    let found = rects(&output, search_color());
    assert_eq!(found.len(), 4);
    assert_eq!(
        found
            .iter()
            .map(|r| (r.left(), r.top()))
            .collect::<Vec<_>>(),
        vec![(160.0, 32.0), (576.0, 32.0), (184.0, 32.0), (584.0, 32.0)]
    );
}

#[test]
fn overlapping_search_matches_retain_each_source_overlay_in_order() {
    let mut s = snapshot(64);
    s.matches = vec![4, 5];
    s.search.match_length = 3;
    let output = paint(&s, 0, 80.0);
    let found = rects(&output, search_color());
    // Two three-byte matches, each painted in hex and ASCII. Intersections
    // deliberately retain both translucent passes, as in the source renderer.
    assert_eq!(found.len(), 12);
    assert_eq!(
        found.iter().map(|r| r.left()).collect::<Vec<_>>(),
        vec![256.0, 608.0, 280.0, 616.0, 304.0, 624.0, 280.0, 616.0, 304.0, 624.0, 328.0, 632.0]
    );
}

#[test]
fn hundred_thousand_search_starts_paint_only_the_three_intersecting_rows() {
    let mut s = snapshot(2_000_000);
    s.matches = (0..100_000).map(|n| n * 16).collect();
    s.search.match_length = 3;
    let (output, stats) = paint_with_stats(&s, 50_000, 80.0);
    assert!(stats.match_start_probes > 20 && stats.match_start_probes < 64,
        "Index actual partition predicate visits, not only the number of painted rectangles: {stats:?}");
    assert_eq!(stats.visible_match_starts, 3);
    let found = rects(&output, search_color());
    assert_eq!(found.len(), 18);
    assert!(found.iter().all(|r| matches!(r.top(), 32.0 | 54.0 | 76.0)));
    assert!(found.iter().all(|r| r.left() <= 592.0));
}

#[test]
fn search_highlights_near_the_u64_boundary_use_exact_offsets() {
    let mut s = snapshot(u64::MAX);
    let top = (u64::MAX - 15) / 16;
    s.page = None;
    s.matches = vec![u64::MAX - 2];
    s.search.match_length = 4;
    let output = paint(&s, top, 80.0);
    let found = rects(&output, search_color());
    assert_eq!(
        found.len(),
        4,
        "The last two real bytes, with no blank byte past EOF"
    );
    assert_eq!(
        found.iter().map(|r| r.left()).collect::<Vec<_>>(),
        vec![480.0, 688.0, 504.0, 696.0]
    );
}
