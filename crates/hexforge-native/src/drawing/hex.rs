use crate::style::{fonts, HexForgeStyle};
use eframe::egui::{self, Color32};
use hexforge_controller::{controller::MainSnapshot, hex::layout::HexLayout};
use std::collections::BTreeSet;
pub struct HexScene<'a> {
    pub rect: egui::Rect,
    pub layout: &'a HexLayout,
    pub snapshot: &'a MainSnapshot,
    pub top_row: u64,
    pub horizontal_px: f64,
}
/// Bounded renderer diagnostics from real predicate visits. Returning these
/// does not sample a clock or change rendering/cache state on ordinary frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HexPaintStats {
    pub match_start_probes: usize,
    pub visible_match_starts: usize,
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}
/// Search starts are sorted. Include a match beginning before the viewport if
/// its tail intersects it; do not walk the full-file match list for each byte.
fn visible_matches(matches: &[u64], first: u128, end: u128, length: u64) -> (&[u64], usize) {
    let earliest = first.saturating_sub(u128::from(length.saturating_sub(1)));
    let mut probes = 0;
    let low = matches.partition_point(|&start| {
        probes += 1;
        u128::from(start) < earliest
    });
    let high = matches.partition_point(|&start| {
        probes += 1;
        u128::from(start) < end
    });
    (&matches[low.min(high)..high], probes)
}
pub fn paint_hex(
    painter: &egui::Painter,
    scene: &HexScene<'_>,
    style: &HexForgeStyle,
) -> HexPaintStats {
    let s = scene.snapshot;
    let l = scene.layout;
    let rect = scene.rect;
    let p = painter.with_clip_rect(rect);
    p.rect_filled(rect, 0, style.palette.hex_background);
    let data_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + l.gutter_width() as f32, rect.top()),
        rect.max,
    );
    let data = p.with_clip_rect(data_rect);
    let gutter = p.with_clip_rect(egui::Rect::from_min_max(
        rect.min,
        egui::pos2(data_rect.left(), rect.bottom()),
    ));
    let font = fonts::regular((l.char_width * 1.5) as f32);
    let text = |p: &egui::Painter, x: f64, y: f64, value: &str, color: Color32| {
        p.text(
            rect.min + egui::vec2(x as f32, y as f32),
            egui::Align2::LEFT_CENTER,
            value,
            font.clone(),
            color,
        );
    };
    text(
        &gutter,
        l.address_x,
        l.header_height / 2.0,
        "OFFSET",
        style.palette.address,
    );
    text(
        &data,
        l.hex_x - scene.horizontal_px,
        l.header_height / 2.0,
        "HEX BYTES",
        style.palette.address,
    );
    text(
        &data,
        l.ascii_x - scene.horizontal_px,
        l.header_height / 2.0,
        "ASCII",
        style.palette.address,
    );
    let divider = style.palette.hex_divider;
    p.rect_filled(
        egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, l.header_height as f32 - 1.0),
            egui::vec2(rect.width(), 1.0),
        ),
        0,
        divider,
    );
    p.rect_filled(
        egui::Rect::from_min_size(
            rect.min + egui::vec2((l.hex_x - l.char_width) as f32, 0.0),
            egui::vec2(1.0, rect.height()),
        ),
        0,
        divider,
    );
    data.rect_filled(
        egui::Rect::from_min_size(
            rect.min
                + egui::vec2(
                    (l.ascii_x - l.char_width * 1.5 - scene.horizontal_px) as f32,
                    0.0,
                ),
            egui::vec2(1.0, rect.height()),
        ),
        0,
        divider,
    );
    let Some(file) = &s.file else {
        return HexPaintStats::default();
    };
    let size = file.size.parse::<u64>().unwrap_or(0);
    if size == 0 {
        return HexPaintStats::default();
    }
    let page = s.page.as_ref().filter(|p| p.revision == file.revision);
    let page_start = page.and_then(|p| p.offset.parse::<u64>().ok()).unwrap_or(0);
    let modified: BTreeSet<u64> = page
        .map(|p| {
            p.modified_offsets
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let count = l
        .visible_rows(rect.height() as f64)
        .min(l.total_rows(size).saturating_sub(scene.top_row));
    let digits = format!("{:X}", size - 1).len().max(8);
    let width = l.row_width.bytes();
    for relative in 0..count {
        let row = scene.top_row + relative;
        let start = row * width;
        let y = l.header_height + relative as f64 * l.row_height;
        let row_end = (u128::from(start) + u128::from(width)).min(u128::from(size));
        let has_bytes = page.is_some_and(|p| {
            u128::from(page_start) < row_end
                && u128::from(page_start) + p.bytes.len() as u128 > u128::from(start)
        });
        if has_bytes {
            text(
                &gutter,
                l.address_x,
                y + l.row_height / 2.0,
                &format!("{start:0digits$X}"),
                style.palette.address,
            );
        }
        for col in 0..width {
            let at = u128::from(start) + u128::from(col);
            if at >= u128::from(size) {
                break;
            }
            let offset = at as u64;
            let hex_x = l.hex_cell_x(col as usize) - scene.horizontal_px;
            let ascii_x = l.ascii_cell_x(col as usize) - scene.horizontal_px;
            if let Some(byte) = page.and_then(|p| {
                offset
                    .checked_sub(page_start)
                    .and_then(|n| p.bytes.get(n as usize))
            }) {
                text(
                    &data,
                    hex_x,
                    y + l.row_height / 2.0,
                    &format!("{byte:02X}"),
                    style.palette.hex_text,
                );
                let printable = if (32..=126).contains(byte) {
                    *byte as char
                } else {
                    '.'
                };
                text(
                    &data,
                    ascii_x,
                    y + l.row_height / 2.0,
                    &printable.to_string(),
                    style.palette.hex_text,
                );
            }
        }
    }
    // Native composition preserves the source order: content, modified tint,
    // each search match (including overlaps), selection, template outline,
    // then the opaque modified-byte underline. Only local cells are painted.
    let first = u128::from(scene.top_row) * u128::from(width);
    let end = (first + u128::from(count) * u128::from(width)).min(u128::from(size));
    let paint_byte = |offset: u64, color: Color32, stroke: bool, underline: bool| {
        let col = (offset % width) as usize;
        let relative = offset / width - scene.top_row;
        let y = l.header_height + relative as f64 * l.row_height;
        let hex_x = l.hex_cell_x(col) - scene.horizontal_px;
        let ascii_x = l.ascii_cell_x(col) - scene.horizontal_px;
        let cells = [
            egui::Rect::from_min_size(
                rect.min + egui::vec2(hex_x as f32, y as f32),
                egui::vec2(l.byte_cell_width as f32, l.row_height as f32),
            ),
            egui::Rect::from_min_size(
                rect.min + egui::vec2(ascii_x as f32, y as f32),
                egui::vec2(l.char_width as f32, l.row_height as f32),
            ),
        ];
        for cell in cells {
            if stroke {
                data.rect_stroke(
                    cell,
                    0,
                    egui::Stroke::new(1.0, color),
                    egui::StrokeKind::Middle,
                );
            } else if underline {
                data.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(cell.left(), cell.bottom() - 2.0),
                        cell.max,
                    ),
                    0,
                    color,
                );
            } else {
                data.rect_filled(cell, 0, color);
            }
        }
    };
    let paint_range = |start: u128, finish: u128, color: Color32, stroke: bool| {
        for offset in start.max(first)..finish.min(end) {
            paint_byte(offset as u64, color, stroke, false);
        }
    };
    if first >= end {
        return HexPaintStats::default();
    }
    for &offset in modified.range(first as u64..end as u64) {
        paint_byte(offset, rgba(240, 136, 62, 56), false, false);
    }
    let match_length = s.search.match_length.max(1);
    let (visible, probes) = visible_matches(&s.matches, first, end, match_length);
    for &start in visible {
        paint_range(
            u128::from(start),
            u128::from(start) + u128::from(match_length),
            rgba(139, 111, 42, 107),
            false,
        );
    }
    if let Some(range) = &s.selection {
        paint_range(
            u128::from(range.start),
            u128::from(range.end) + 1,
            rgba(31, 111, 235, 122),
            false,
        );
    }
    if let Some(range) = &s.template_range {
        paint_range(
            u128::from(range.start),
            u128::from(range.end) + 1,
            Color32::from_rgb(57, 197, 207),
            true,
        );
    }
    for &offset in modified.range(first as u64..end as u64) {
        paint_byte(offset, Color32::from_rgb(240, 136, 62), false, true);
    }
    HexPaintStats {
        match_start_probes: probes,
        visible_match_starts: visible.len(),
    }
}
