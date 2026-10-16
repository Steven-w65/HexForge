//! Exact logical rows, continuous viewport-local drawing coordinates. The
//! address gutter is a separate fixed layer, never horizontally scrolled.
use crate::controller::RowWidth;
pub const ROW_HEIGHT: f64 = 22.0;
pub const HEADER_HEIGHT: f64 = 32.0;
pub const MAX_READ_BYTES: u64 = 1_048_576;
pub const DEFAULT_CHAR_WIDTH: f64 = 8.0;
pub const GROUP_GAP: f64 = 8.0;
const MAX_LOCAL_INTEGER: f64 = 9_007_199_254_740_991.0;
#[derive(Debug, Clone, Copy)]
pub struct HexMetrics {
    pub width: f64,
    pub char_width: f64,
}
#[derive(Debug, Clone, Copy)]
pub struct LocalPoint {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone)]
pub struct HexViewState {
    pub file_size: Option<u64>,
    pub top_row: u64,
    pub height: f64,
    pub horizontal_px: f64,
}
#[derive(Debug, Clone)]
pub struct HexLayout {
    pub width: f64,
    pub row_width: RowWidth,
    pub row_height: f64,
    pub header_height: f64,
    pub char_width: f64,
    pub address_x: f64,
    pub address_width: f64,
    pub hex_x: f64,
    pub hex_width: f64,
    pub ascii_x: f64,
    pub ascii_width: f64,
    pub byte_stride: f64,
    pub byte_cell_width: f64,
    pub group_gap: f64,
}
impl HexLayout {
    pub fn new(metrics: HexMetrics, row_width: RowWidth) -> Self {
        assert!(
            metrics.width.is_finite() && metrics.width >= 0.0,
            "Layout width must be a non-negative finite number"
        );
        let char_width = if metrics.char_width.is_finite() && metrics.char_width > 0.0 {
            metrics.char_width
        } else {
            DEFAULT_CHAR_WIDTH
        };
        let address_x = char_width * 2.0;
        let address_width = char_width * 16.0;
        let hex_x = address_x + address_width + char_width * 2.0;
        let byte_stride = char_width * 3.0;
        let groups = (row_width.bytes() - 1) / 8;
        let hex_width = byte_stride * row_width.bytes() as f64 + groups as f64 * GROUP_GAP;
        let ascii_x = hex_x + hex_width + char_width * 3.0;
        let ascii_width = char_width * row_width.bytes() as f64 + groups as f64 * GROUP_GAP;
        Self {
            width: metrics.width,
            row_width,
            row_height: ROW_HEIGHT,
            header_height: HEADER_HEIGHT,
            char_width,
            address_x,
            address_width,
            hex_x,
            hex_width,
            ascii_x,
            ascii_width,
            byte_stride,
            byte_cell_width: char_width * 2.0,
            group_gap: GROUP_GAP,
        }
    }
    pub fn content_width(&self) -> f64 {
        (self.ascii_x + self.ascii_width + self.char_width * 2.0).ceil()
    }
    pub fn gutter_width(&self) -> f64 {
        self.hex_x - self.char_width + 1.0
    }
    pub fn hex_cell_x(&self, index: usize) -> f64 {
        self.hex_x + index as f64 * self.byte_stride + (index / 8) as f64 * self.group_gap
    }
    pub fn ascii_cell_x(&self, index: usize) -> f64 {
        self.ascii_x + index as f64 * self.char_width + (index / 8) as f64 * self.group_gap
    }
    pub fn total_rows(&self, file_size: u64) -> u64 {
        if file_size == 0 {
            0
        } else {
            (file_size - 1) / self.row_width.bytes() + 1
        }
    }
    pub fn fully_visible_rows(&self, height: f64) -> u64 {
        ((height - self.header_height).max(0.0) / self.row_height)
            .floor()
            .max(1.0) as u64
    }
    pub fn visible_rows(&self, height: f64) -> u64 {
        ((height - self.header_height).max(0.0) / self.row_height)
            .ceil()
            .max(1.0) as u64
    }
    pub fn row_y(&self, row: u64, top: u64) -> Option<f64> {
        let delta = i128::from(row) - i128::from(top);
        if delta.unsigned_abs() > MAX_LOCAL_INTEGER as u128 {
            return None;
        }
        Some(self.header_height + delta as f64 * self.row_height)
    }
    pub fn hit_test(&self, point: LocalPoint, view: &HexViewState) -> Option<u64> {
        // Pixels under the fixed gutter do not belong to scrolled data cells.
        if point.x < self.gutter_width() {
            return None;
        }
        let row_offset = view.top_row.checked_mul(self.row_width.bytes())?;
        self.hit_at(
            point.x + view.horizontal_px,
            point.y,
            row_offset,
            view.file_size?,
        )
    }
    pub fn hit_at(&self, x: f64, y: f64, row_offset: u64, file_size: u64) -> Option<u64> {
        if !x.is_finite() || !y.is_finite() || y < self.header_height {
            return None;
        }
        let row = ((y - self.header_height) / self.row_height).floor();
        if row > MAX_LOCAL_INTEGER {
            return None;
        }
        let index = (0..self.row_width.bytes() as usize).find(|&i| {
            let a = self.hex_cell_x(i);
            let b = self.ascii_cell_x(i);
            x >= a && x < a + self.byte_cell_width || x >= b && x < b + self.char_width
        })?;
        let offset = u128::from(row_offset)
            + (row as u128) * u128::from(self.row_width.bytes())
            + index as u128;
        (offset < u128::from(file_size)).then_some(offset as u64)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteReadPlan {
    pub first_row: u64,
    pub row_count: u32,
    pub byte_start: u64,
    pub byte_length: u32,
}
/// Matches the reference nearby-context plan. Bounds are applied BEFORE any
/// potentially large height/count is converted to an integer read length.
pub fn visible_range(
    layout: &HexLayout,
    view: &HexViewState,
    nearby_rows: f64,
) -> Option<ByteReadPlan> {
    let size = view.file_size?;
    if size == 0 || !view.height.is_finite() || view.height < 0.0 {
        return None;
    }
    let total = layout.total_rows(size);
    let max_read_rows = MAX_READ_BYTES / layout.row_width.bytes();
    let max_visible = max_read_rows - 4;
    let visible = if view.height >= max_visible as f64 * layout.row_height {
        max_visible
    } else {
        (view.height / layout.row_height).ceil().max(1.0) as u64
    };
    let nearby = if nearby_rows.is_finite() {
        nearby_rows.floor().max(2.0).min(max_read_rows as f64) as u64
    } else {
        2
    };
    let prefetch = nearby.min((max_read_rows - visible) / 2);
    let requested = max_read_rows.min(visible + prefetch * 2);
    let top = view.top_row.min(total - 1);
    let first = top.saturating_sub(prefetch);
    let count = requested.min(total - first);
    let byte_start = first * layout.row_width.bytes();
    let length = (count * layout.row_width.bytes())
        .min(size - byte_start)
        .min(MAX_READ_BYTES);
    Some(ByteReadPlan {
        first_row: first,
        row_count: count as u32,
        byte_start,
        byte_length: length as u32,
    })
}
