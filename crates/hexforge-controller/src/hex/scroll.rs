//! Integer-ratio overview and scrollbar mappings; only bounded pixels become
//! f64. EOF preserves the last FULL editor page, not a page of blank space.
pub fn clamp_top_row(total: u64, visible: u64, requested: u64) -> u64 {
    requested.min(total.saturating_sub(visible.max(1)))
}
fn track_pixels(px: f64) -> u64 {
    if px.is_finite() && px > 0.0 {
        px.floor().min(9_007_199_254_740_991.0) as u64
    } else {
        0
    }
}
pub fn offset_to_overview_pixel(offset: u64, size: u64, height: f64) -> f64 {
    let h = track_pixels(height);
    if h <= 1 || size <= 1 {
        return 0.0;
    }
    ((u128::from(offset.min(size - 1)) * u128::from(h - 1)) / u128::from(size - 1)) as f64
}
pub fn row_to_thumb(row: u64, total: u64, track: f64) -> f64 {
    let t = track_pixels(track);
    if t == 0 || total <= 1 {
        return 0.0;
    }
    ((u128::from(row.min(total - 1)) * u128::from(t)) / u128::from(total - 1)) as f64
}
pub fn thumb_to_row(px: f64, total: u64, track: f64) -> u64 {
    let t = track_pixels(track);
    if t == 0 || total <= 1 || !px.is_finite() {
        return 0;
    }
    let p = px.floor().clamp(0.0, t as f64) as u64;
    ((u128::from(p) * u128::from(total - 1)) / u128::from(t)) as u64
}
#[derive(Debug, Clone, Copy)]
pub enum WheelPolicy {
    Main,
    FitMinimap,
}
pub fn wheel_row(row: u64, total: u64, visible: u64, delta: f64, policy: WheelPolicy) -> u64 {
    if !delta.is_finite() || delta == 0.0 {
        return row;
    }
    let steps = match policy {
        WheelPolicy::Main => 1,
        WheelPolicy::FitMinimap => {
            (delta.abs() / 22.0).round().max(1.0).min(u64::MAX as f64) as u64
        }
    };
    clamp_top_row(
        total,
        visible,
        if delta > 0.0 {
            row.saturating_add(steps)
        } else {
            row.saturating_sub(steps)
        },
    )
}
#[derive(Debug, Clone)]
pub struct Scrollbar {
    pub height: f64,
    pub thumb_height: f64,
    pub row: u64,
    total: u64,
    visible: u64,
    grab: Option<f64>,
}
impl Scrollbar {
    pub fn new(total: u64, visible: u64, height: f64, row: u64) -> Self {
        let visible = visible.max(1);
        let h = track_pixels(height).max(1);
        let thumb = if total <= visible {
            h
        } else {
            ((u128::from(h) * u128::from(visible)) / u128::from(total)) as u64
        };
        Self {
            height: h as f64,
            thumb_height: if total <= visible {
                h as f64
            } else {
                thumb.max(24).min(h) as f64
            },
            row: clamp_top_row(total, visible, row),
            total,
            visible,
            grab: None,
        }
    }
    pub fn top(&self) -> f64 {
        row_to_thumb(
            self.row,
            self.total.saturating_sub(self.visible) + 1,
            self.height - self.thumb_height,
        )
    }
    pub fn begin(&mut self, y: f64) {
        let top = self.top();
        self.grab = Some(if y >= top && y <= top + self.thumb_height {
            y - top
        } else {
            self.thumb_height / 2.0
        });
        self.drag(y);
    }
    pub fn drag(&mut self, y: f64) -> u64 {
        if let Some(grab) = self.grab {
            self.row = thumb_to_row(
                y - grab,
                self.total.saturating_sub(self.visible) + 1,
                self.height - self.thumb_height,
            );
        }
        self.row
    }
    pub fn end(&mut self) {
        self.grab = None;
    }
    pub fn captured(&self) -> bool {
        self.grab.is_some()
    }
}
