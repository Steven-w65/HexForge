use super::glyphs::GlyphCache;
use eframe::egui::{self, Color32, ColorImage};
use hexforge_controller::{
    controller::{MinimapMode, MinimapRender, RowWidth, Theme},
    hex::{minimap_data::MinimapRow, minimap_geometry::*},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapOptions {
    pub row_width: RowWidth,
    pub render: MinimapRender,
    pub theme: Theme,
}
pub struct MinimapScene<'a> {
    pub geometry: &'a MinimapGeometry,
    pub projection: &'a MinimapProjection,
    pub rows: &'a BTreeMap<u64, MinimapRow>,
    pub width_px: u32,
    pub dpr: f64,
    pub options: MinimapOptions,
}
#[derive(Debug, Default)]
pub struct RasterUpdate {
    pub full: bool,
    pub copied_device_rows: usize,
    pub rects: Vec<[usize; 4]>,
}
struct Previous {
    width: u32,
    dpr: f64,
    mode: MinimapMode,
    total: u64,
    height: u32,
    row_px: u32,
    origin: LogicalY,
    options: MinimapOptions,
    rows: BTreeMap<u64, MinimapRow>,
}
pub struct MinimapDrawCache {
    image: ColorImage,
    previous: Option<Previous>,
    glyphs: GlyphCache,
    texture: Option<egui::TextureHandle>,
}
impl Default for MinimapDrawCache {
    fn default() -> Self {
        Self {
            image: ColorImage::filled([1, 1], Color32::TRANSPARENT),
            previous: None,
            glyphs: GlyphCache::default(),
            texture: None,
        }
    }
}
pub fn background(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => Color32::from_rgb(22, 27, 32),
        Theme::Light => Color32::from_rgb(243, 245, 247),
    }
}
fn byte_color(theme: Theme, byte: u8, modified: bool) -> Color32 {
    if modified {
        return Color32::from_rgb(240, 136, 62);
    }
    match (theme, byte) {
        (Theme::Dark, 0) => Color32::from_rgb(95, 116, 133),
        (Theme::Dark, 32..=126) => Color32::from_rgb(174, 192, 208),
        (Theme::Dark, _) => Color32::from_rgb(140, 162, 181),
        (Theme::Light, 0) => Color32::from_rgb(127, 142, 155),
        (Theme::Light, 32..=126) => Color32::from_rgb(66, 95, 119),
        (Theme::Light, _) => Color32::from_rgb(99, 125, 146),
    }
}
impl MinimapDrawCache {
    pub fn image(&self) -> &ColorImage {
        &self.image
    }
    pub fn clear(&mut self) {
        self.previous = None;
        self.texture = None;
        self.glyphs.clear();
        self.image = ColorImage::filled([1, 1], Color32::TRANSPARENT);
    }
    fn fill_rows(&mut self, top: usize, bottom: usize, color: Color32) {
        let width = self.image.size[0];
        let height = self.image.size[1];
        self.image.pixels[top.min(height) * width..bottom.min(height) * width].fill(color);
    }
    fn paint_row(&mut self, s: &MinimapScene<'_>, pos: &MinimapRange, dpr: f64) {
        let Some(row) = s.rows.get(&pos.row) else {
            return;
        };
        let g = s.geometry;
        // Subtract the exact global origin BEFORE converting to local pixels.
        let row_top = if g.mode == MinimapMode::Proportional {
            (i128::from(pos.row) * i128::from(g.row_px) * SUBPIXELS - s.projection.virtual_origin.0)
                as f64
                / SUBPIXELS as f64
        } else {
            pos.y
        };
        let glyph_top = (row_top * dpr).round() as i64;
        let compressed = g.mode == MinimapMode::Fit && !g.fits_naturally;
        let glyph_height = (((row_top
            + if compressed {
                1.0
            } else {
                g.row_px.min(5) as f64
            })
            * dpr)
            .round() as i64
            - glyph_top)
            .max(1) as usize;
        let clip_top = (pos.y * dpr).round().max(0.0) as i64;
        let clip_bottom = ((pos.y + pos.height) * dpr)
            .round()
            .min(self.image.size[1] as f64) as i64;
        let width = s.options.row_width.bytes() as usize;
        let cell = (s.width_px as f64 - 8.0) / width as f64;
        let base = u128::from(pos.row) * width as u128;
        for (column, &byte) in row.bytes.iter().take(width).enumerate() {
            let modified = row
                .modified_offsets
                .iter()
                .any(|&o| u128::from(o) == base + column as u128);
            let color = byte_color(s.options.theme, byte, modified);
            if s.options.render == MinimapRender::Characters {
                let x = ((4.0 + column as f64 * cell) * dpr).round() as i64;
                let right = ((4.0 + (column + 1) as f64 * cell) * dpr).round() as i64
                    - dpr.round().max(1.0) as i64;
                let glyph_width =
                    (right - x).max(1).min((3.0 * dpr).round().max(1.0) as i64) as usize;
                let glyph = self.glyphs.get(
                    if modified && byte == 32 { 95 } else { byte },
                    glyph_width,
                    glyph_height,
                );
                for run in glyph.iter() {
                    let y = glyph_top + run.y as i64;
                    if y < clip_top || y >= clip_bottom {
                        continue;
                    }
                    for px in x + run.x as i64..x + (run.x + run.width) as i64 {
                        if px >= 0 && px < self.image.size[0] as i64 {
                            self.image.pixels[y as usize * self.image.size[0] + px as usize] =
                                color;
                        }
                    }
                }
            } else {
                let x = (4.0 + column as f64 * cell).floor();
                let ink_width = ((4.0 + (column + 1) as f64 * cell).floor() - x - 1.0).max(1.0);
                let h = pos.height.min((pos.height / 2.0).floor().max(1.0));
                let y = pos.y + ((pos.height - h) / 2.0).ceil().min(pos.height - h);
                let left = (x * dpr).round().max(0.0) as usize;
                let right = ((x + ink_width) * dpr)
                    .round()
                    .max(0.0)
                    .min(self.image.size[0] as f64) as usize;
                for py in (y * dpr).round().max(0.0) as usize
                    ..((y + h) * dpr)
                        .round()
                        .max(0.0)
                        .min(self.image.size[1] as f64) as usize
                {
                    for px in left..right {
                        self.image.pixels[py * self.image.size[0] + px] = color;
                    }
                }
            }
        }
    }
    pub fn render(&mut self, s: &MinimapScene<'_>) -> RasterUpdate {
        let g = s.geometry;
        let dpr = if s.dpr.is_finite() && s.dpr > 0.0 {
            s.dpr
        } else {
            1.0
        };
        let size = [
            (s.width_px as f64 * dpr).round().max(1.0) as usize,
            (g.canvas_height_px.max(1) as f64 * dpr).round().max(1.0) as usize,
        ];
        let bg = background(s.options.theme);
        let positions = sample_plan(g, s.projection);
        let mut update = RasterUpdate::default();
        let mut dirty = BTreeSet::new();
        let mut shift = 0i64;
        if self.image.size != size {
            self.image = ColorImage::filled(size, bg);
            update.full = true;
        }
        if let Some(old) = &self.previous {
            update.full |= old.width != s.width_px
                || old.dpr != dpr
                || old.mode != g.mode
                || old.total != g.total_rows
                || old.height != g.canvas_height_px
                || old.row_px != g.row_px
                || old.options != s.options;
            if old.origin != s.projection.virtual_origin {
                let delta = s.projection.virtual_origin.0 - old.origin.0;
                if delta % SUBPIXELS != 0
                    || delta.unsigned_abs() >= u128::from(g.canvas_height_px) * SUBPIXELS as u128
                {
                    update.full = true;
                } else {
                    let device = delta as f64 / SUBPIXELS as f64 * dpr;
                    if device.fract() != 0.0 {
                        update.full = true;
                    } else {
                        shift = device as i64;
                    }
                }
            }
            for pos in &positions {
                if old.rows.get(&pos.row) != s.rows.get(&pos.row) {
                    dirty.insert(pos.row);
                }
            }
        } else {
            update.full = true;
        }
        if update.full {
            self.image.pixels.fill(bg);
            for p in &positions {
                self.paint_row(s, p, dpr);
            }
            update.rects.push([0, 0, size[0], size[1]]);
        } else {
            if shift != 0 {
                let n = shift.unsigned_abs() as usize;
                let h = size[1];
                let w = size[0];
                if n < h {
                    if shift > 0 {
                        self.image.pixels.copy_within(n * w..h * w, 0);
                        self.fill_rows(h - n, h, bg);
                    } else {
                        self.image.pixels.copy_within(0..(h - n) * w, n * w);
                        self.fill_rows(0, n, bg);
                    }
                    update.copied_device_rows = h - n;
                    for p in &positions {
                        let a = (p.y * dpr).round() as usize;
                        let b = ((p.y + p.height) * dpr).round() as usize;
                        if shift > 0 && b > h - n || shift < 0 && a < n {
                            dirty.insert(p.row);
                        }
                    }
                } else {
                    self.image.pixels.fill(bg);
                    for p in &positions {
                        dirty.insert(p.row);
                    }
                }
                update.rects.push([0, 0, w, h]);
            }
            for p in &positions {
                if !dirty.contains(&p.row) {
                    continue;
                }
                let top = (p.y * dpr).round().max(0.0) as usize;
                let bottom = ((p.y + p.height) * dpr)
                    .round()
                    .max(0.0)
                    .min(size[1] as f64) as usize;
                self.fill_rows(top, bottom, bg);
                self.paint_row(s, p, dpr);
                if shift == 0 && bottom > top {
                    update.rects.push([0, top, size[0], bottom - top]);
                }
            }
        }
        // Retain only this bounded tile, even if the caller owns a larger map.
        self.previous = Some(Previous {
            width: s.width_px,
            dpr,
            mode: g.mode,
            total: g.total_rows,
            height: g.canvas_height_px,
            row_px: g.row_px,
            origin: s.projection.virtual_origin,
            options: s.options,
            rows: positions
                .iter()
                .filter_map(|p| s.rows.get(&p.row).map(|r| (p.row, r.clone())))
                .collect(),
        });
        update
    }
    pub fn paint(&mut self, painter: &egui::Painter, rect: egui::Rect, s: &MinimapScene<'_>) {
        let update = self.render(s);
        if self.texture.is_none() {
            self.texture = Some(painter.ctx().load_texture(
                "HexForge minimap tile",
                self.image.clone(),
                egui::TextureOptions::NEAREST,
            ));
        } else {
            let texture = self.texture.as_mut().unwrap();
            if texture.size() != self.image.size {
                texture.set(self.image.clone(), egui::TextureOptions::NEAREST);
            } else {
                for [x, y, w, h] in update.rects {
                    let pixels = (y..y + h)
                        .flat_map(|py| {
                            self.image.pixels
                                [py * self.image.size[0] + x..py * self.image.size[0] + x + w]
                                .iter()
                                .copied()
                        })
                        .collect();
                    texture.set_partial(
                        [x, y],
                        ColorImage::new([w, h], pixels),
                        egui::TextureOptions::NEAREST,
                    );
                }
            }
        }
        let image_rect = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(
                self.image.size[0] as f32 / s.dpr as f32,
                self.image.size[1] as f32 / s.dpr as f32,
            ),
        );
        painter.with_clip_rect(rect).image(
            self.texture.as_ref().unwrap().id(),
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}
