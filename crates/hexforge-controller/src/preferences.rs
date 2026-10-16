//! Optional pane preference; tiny/resize frames retain the source bounds.
#[derive(Debug, Clone)]
pub struct PanelPreferences {
    pub preferred_height: f32,
    pub collapsed: bool,
}
impl Default for PanelPreferences {
    fn default() -> Self {
        Self {
            preferred_height: 220.0,
            collapsed: false,
        }
    }
}
impl PanelPreferences {
    pub fn restore(height: Option<f32>, collapsed: bool) -> Self {
        Self {
            preferred_height: height
                .filter(|h| h.is_finite() && *h >= 28.0)
                .unwrap_or(220.0),
            collapsed,
        }
    }
    pub fn bounds(&self, workspace_height: f32) -> (f32, f32) {
        let max = (workspace_height - 164.0).floor().max(28.0);
        (84.0f32.min(max), max)
    }
    pub fn effective_height(&self, workspace_height: f32) -> f32 {
        if self.collapsed {
            28.0
        } else {
            let (min, max) = self.bounds(workspace_height);
            self.preferred_height.clamp(min, max)
        }
    }
}

/// Monotonic presentation clock for transient notices, unrelated to startup
/// measurement or template synchronization revisions.
#[derive(Default)]
pub struct NoticeClock {
    seen: Option<(u64, f64)>,
}
impl NoticeClock {
    pub fn update(&mut self, revision: u64, present: bool, now: f64) -> Option<u64> {
        if !present {
            self.seen = None;
            return None;
        }
        if !now.is_finite() {
            return None;
        }
        if self.seen.is_none_or(|(r, _)| r != revision) {
            self.seen = Some((revision, now));
        }
        self.seen
            .filter(|(_, start)| now - *start >= 5.0)
            .map(|(r, _)| r)
    }
    pub fn remaining(&self, now: f64) -> Option<f64> {
        self.seen
            .map(|(_, start)| (5.0 - (now - start)).clamp(0.0, 5.0))
    }
}
