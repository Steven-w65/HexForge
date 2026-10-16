//! The minimap is a bounded tile in a potentially enormous logical canvas.
//! Rows and virtual origins remain exact; only tile-relative pixels become f64.
use crate::controller::MinimapMode;
pub const SUBPIXELS: i128 = 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogicalY(pub i128);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryError(pub &'static str);
impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for GeometryError {}
#[derive(Debug, Clone)]
pub struct MinimapConfig {
    pub mode: MinimapMode,
    pub total_rows: u64,
    pub height_px: u32,
    pub row_px: u32,
    pub top_row: u64,
    pub visible_main_rows: Option<u64>,
}
#[derive(Debug, Clone)]
pub struct MinimapGeometry {
    pub mode: MinimapMode,
    pub total_rows: u64,
    pub height_px: u32,
    pub row_px: u32,
    pub visible_mini_rows: u64,
    pub logical_height_px: i128,
    pub canvas_height_px: u32,
    pub fits_naturally: bool,
    pub main_top_row: u64,
    pub visible_main_rows: u64,
    pub max_main_row: u64,
    pub viewport_height_px: u32,
    pub viewport_travel_px: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MinimapProjection {
    pub editor_row: u64,
    pub virtual_origin: LogicalY,
    pub box_top: f64,
    pub box_top_sub: LogicalY,
    pub box_height: f64,
    pub first_sample_row: u64,
}
impl MinimapGeometry {
    pub fn new(config: MinimapConfig) -> Result<Self, GeometryError> {
        if config.height_px == 0 {
            return Err(GeometryError("Invalid minimap height"));
        }
        if config.row_px == 0 {
            return Err(GeometryError("Minimap row height must be positive"));
        }
        let visible_mini_rows =
            (u64::from(config.height_px) + u64::from(config.row_px) - 1) / u64::from(config.row_px);
        let visible_main_rows = config.visible_main_rows.unwrap_or(visible_mini_rows);
        if visible_main_rows == 0 {
            return Err(GeometryError("Visible editor rows must be positive"));
        }
        let logical_height_px = i128::from(config.total_rows) * i128::from(config.row_px);
        let fits_naturally = logical_height_px <= i128::from(config.height_px);
        let canvas_height_px = if fits_naturally {
            logical_height_px as u32
        } else {
            config.height_px
        };
        let max_main_row = config.total_rows.saturating_sub(visible_main_rows);
        let viewport_height_px = if config.mode == MinimapMode::Proportional {
            logical_height_px
                .min(i128::from(canvas_height_px))
                .min(i128::from(visible_main_rows) * i128::from(config.row_px)) as u32
        } else {
            0
        };
        let viewport_travel_px = if config.mode == MinimapMode::Proportional {
            canvas_height_px - viewport_height_px
        } else {
            0
        };
        Ok(Self {
            mode: config.mode,
            total_rows: config.total_rows,
            height_px: config.height_px,
            row_px: config.row_px,
            visible_mini_rows,
            logical_height_px,
            canvas_height_px,
            fits_naturally,
            main_top_row: config.top_row.min(max_main_row),
            visible_main_rows,
            max_main_row,
            viewport_height_px,
            viewport_travel_px,
        })
    }
    pub fn projection(&self) -> MinimapProjection {
        project_editor(self, self.main_top_row)
    }
}
/// The TOP edge of a row interval; total_rows denotes the bottom edge.
/// Fit compression uses integer FLOOR division before fixed-point conversion.
/// Proportional edges are raw logical coordinates, never offscreen sentinels.
pub fn row_edge_y(g: &MinimapGeometry, row: u64) -> Result<LogicalY, GeometryError> {
    let bounded = i128::from(row.min(g.total_rows));
    let px = if g.total_rows == 0 {
        0
    } else if g.mode == MinimapMode::Fit && !g.fits_naturally {
        bounded * i128::from(g.height_px) / i128::from(g.total_rows)
    } else {
        bounded * i128::from(g.row_px)
    };
    px.checked_mul(SUBPIXELS)
        .map(LogicalY)
        .ok_or(GeometryError("Minimap coordinate overflow"))
}
pub fn row_at_y(g: &MinimapGeometry, y: LogicalY) -> u64 {
    if g.total_rows == 0 || g.canvas_height_px == 0 {
        return 0;
    }
    if g.mode == MinimapMode::Proportional {
        return (y.0 / (i128::from(g.row_px) * SUBPIXELS)).clamp(0, i128::from(g.total_rows - 1))
            as u64;
    }
    let py = (y.0 / SUBPIXELS).clamp(0, i128::from(g.canvas_height_px - 1));
    let candidate = if g.fits_naturally {
        py / i128::from(g.row_px)
    } else if py == 0 {
        0
    } else if py == i128::from(g.canvas_height_px - 1) {
        i128::from(g.total_rows - 1)
    } else {
        ((2 * py + 1) * i128::from(g.total_rows)) / (2 * i128::from(g.canvas_height_px))
    };
    candidate.clamp(0, i128::from(g.total_rows - 1)) as u64
}
/// Exact floor(a*b/d). Decompose before multiplying: even maximum u64 rows
/// and u32 viewport/row dimensions cannot overflow an intermediate bitmap
/// coordinate. Remainders use u128 (two u64 factors can exceed signed i128).
fn mul_div(a: u128, b: u128, d: u128) -> u128 {
    debug_assert!(d > 0);
    (a / d)
        .checked_mul(b)
        .and_then(|whole| {
            (a % d)
                .checked_mul(b)
                .map(|remainder| whole + remainder / d)
        })
        .expect("proven bounded geometry ratio")
}
fn projection(
    g: &MinimapGeometry,
    row: u64,
    top_sub: i128,
    visual_top: Option<f64>,
) -> MinimapProjection {
    let row = row.min(g.max_main_row);
    let scrollable = g.logical_height_px - i128::from(g.canvas_height_px);
    let origin = if g.mode != MinimapMode::Proportional || g.max_main_row == 0 {
        0
    } else if g.viewport_travel_px > 0 {
        mul_div(
            scrollable as u128,
            top_sub as u128,
            u128::from(g.viewport_travel_px),
        ) as i128
    } else {
        mul_div(
            (scrollable * SUBPIXELS) as u128,
            u128::from(row),
            u128::from(g.max_main_row),
        ) as i128
    };
    let first_sample_row = (origin / (i128::from(g.row_px) * SUBPIXELS)) as u64;
    MinimapProjection {
        editor_row: row,
        virtual_origin: LogicalY(origin),
        box_top: visual_top.unwrap_or(top_sub as f64 / SUBPIXELS as f64),
        box_top_sub: LogicalY(top_sub),
        box_height: g.viewport_height_px as f64,
        first_sample_row: first_sample_row.min(g.total_rows.saturating_sub(g.visible_mini_rows)),
    }
}
pub fn project_editor(g: &MinimapGeometry, editor_row: u64) -> MinimapProjection {
    let row = editor_row.min(g.max_main_row);
    let top =
        if g.mode == MinimapMode::Proportional && g.max_main_row > 0 && g.viewport_travel_px > 0 {
            i128::from(row) * i128::from(g.viewport_travel_px) * SUBPIXELS
                / i128::from(g.max_main_row)
        } else {
            0
        };
    projection(g, row, top, None)
}
/// Dragging uses FULL slider travel, not rowAtY. Retain the exact local grab
/// offset and continuous visual top; quantize only the actual editor row.
pub fn drag_projection(g: &MinimapGeometry, pointer_y: f64, grab_offset: f64) -> MinimapProjection {
    let top = (pointer_y - grab_offset).clamp(0.0, g.viewport_travel_px as f64);
    if !top.is_finite() {
        return g.projection();
    }
    let sub = (top * SUBPIXELS as f64).round() as i128;
    let den = i128::from(g.viewport_travel_px) * SUBPIXELS;
    let row = if den == 0 {
        g.main_top_row
    } else {
        ((i128::from(g.max_main_row) * sub + den / 2) / den) as u64
    };
    projection(g, row, sub, Some(top))
}
pub fn canvas_offset_y(g: &MinimapGeometry, p: &MinimapProjection) -> LogicalY {
    if g.mode == MinimapMode::Proportional {
        LogicalY(-p.virtual_origin.0)
    } else {
        LogicalY(0)
    }
}
pub fn container_to_logical(g: &MinimapGeometry, p: &MinimapProjection, y: f64) -> LogicalY {
    let sub = if y.is_finite() {
        (y * SUBPIXELS as f64).floor() as i128
    } else {
        0
    };
    LogicalY(sub.saturating_add(if g.mode == MinimapMode::Proportional {
        p.virtual_origin.0
    } else {
        0
    }))
}
#[derive(Debug, Clone, PartialEq)]
pub struct MinimapBox {
    pub top: f64,
    pub height: f64,
    pub canvas_top: LogicalY,
}
pub fn viewport_box(
    g: &MinimapGeometry,
    p: &MinimapProjection,
    start: u64,
    end_exclusive: u64,
) -> Option<MinimapBox> {
    if g.total_rows == 0 || g.canvas_height_px == 0 {
        return None;
    }
    if g.mode == MinimapMode::Proportional {
        return Some(MinimapBox {
            top: p.box_top,
            height: p.box_height,
            canvas_top: LogicalY(p.virtual_origin.0 + p.box_top_sub.0),
        });
    }
    let top =
        (row_edge_y(g, start).ok()?.0 / SUBPIXELS).clamp(0, i128::from(g.canvas_height_px)) as f64;
    let bottom = (row_edge_y(g, end_exclusive).ok()?.0 / SUBPIXELS)
        .clamp(top as i128, i128::from(g.canvas_height_px)) as f64;
    let min_height = if g.fits_naturally {
        0.0
    } else {
        2.0_f64.min(g.canvas_height_px as f64)
    };
    let shown = top.min(g.canvas_height_px as f64 - min_height);
    Some(MinimapBox {
        top: shown,
        height: min_height.max(bottom - shown),
        canvas_top: LogicalY((shown * SUBPIXELS as f64) as i128),
    })
}
#[derive(Debug, Clone, PartialEq)]
pub struct MinimapRange {
    pub row: u64,
    pub y: f64,
    pub height: f64,
}
pub fn sample_plan(g: &MinimapGeometry, p: &MinimapProjection) -> Vec<MinimapRange> {
    if g.total_rows == 0 || g.canvas_height_px == 0 {
        return vec![];
    }
    if g.mode == MinimapMode::Fit && !g.fits_naturally {
        return (0..g.canvas_height_px)
            .map(|y| MinimapRange {
                row: row_at_y(g, LogicalY(i128::from(y) * SUBPIXELS)),
                y: y as f64,
                height: 1.0,
            })
            .collect();
    }
    let stride = i128::from(g.row_px) * SUBPIXELS;
    let canvas_end = i128::from(g.canvas_height_px) * SUBPIXELS;
    let origin = if g.mode == MinimapMode::Proportional {
        p.virtual_origin.0
    } else {
        0
    };
    let first = if g.mode == MinimapMode::Proportional {
        p.first_sample_row
    } else {
        0
    };
    let end_row =
        ((origin + canvas_end + stride - 1) / stride).min(i128::from(g.total_rows)) as u64;
    let mut out = vec![];
    for row in first..end_row {
        let top = i128::from(row) * stride - origin;
        let bottom = top + stride;
        let clipped = top.max(0);
        let end = bottom.min(canvas_end);
        if end > clipped {
            out.push(MinimapRange {
                row,
                y: clipped as f64 / SUBPIXELS as f64,
                height: (end - clipped) as f64 / SUBPIXELS as f64,
            });
        }
    }
    out
}
