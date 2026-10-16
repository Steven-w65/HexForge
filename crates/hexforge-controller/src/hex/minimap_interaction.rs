use super::{
    minimap_geometry::*,
    scroll::{wheel_row, WheelPolicy},
};
use crate::controller::MinimapMode;
#[derive(Debug, Clone)]
enum Drag {
    Proportional {
        grab: f64,
        start_y: f64,
        start_projection: MinimapProjection,
    },
    Fit {
        row_grab: i128,
    },
}
#[derive(Debug, Clone)]
pub struct MinimapInteraction {
    geometry: MinimapGeometry,
    projection: MinimapProjection,
    drag: Option<Drag>,
}
impl MinimapInteraction {
    pub fn new(geometry: MinimapGeometry) -> Self {
        let projection = geometry.projection();
        Self {
            geometry,
            projection,
            drag: None,
        }
    }
    pub fn projection(&self) -> MinimapProjection {
        self.projection.clone()
    }
    pub fn geometry(&self) -> &MinimapGeometry {
        &self.geometry
    }
    pub fn captured(&self) -> bool {
        self.drag.is_some()
    }
    pub fn set_editor_row(&mut self, row: u64) -> MinimapProjection {
        self.geometry.main_top_row = row.min(self.geometry.max_main_row);
        self.projection = project_editor(&self.geometry, row);
        self.projection()
    }
    pub fn replace_geometry(&mut self, geometry: MinimapGeometry) {
        // Resizing the same logical file is not a new pointer gesture. The
        // presenter rebases the held pointer in the new local coordinates.
        // A different mode/data density is deliberately a fresh interaction.
        if self.geometry.mode != geometry.mode
            || self.geometry.total_rows != geometry.total_rows
            || self.geometry.row_px != geometry.row_px
        {
            self.drag = None;
        }
        self.projection = geometry.projection();
        self.geometry = geometry;
    }
    pub fn click(&mut self, y: f64) -> MinimapProjection {
        let target = row_at_y(
            &self.geometry,
            container_to_logical(&self.geometry, &self.projection, y),
        );
        let target = if self.geometry.mode == MinimapMode::Fit {
            target.saturating_sub(self.geometry.visible_main_rows / 2)
        } else {
            target
        };
        self.set_editor_row(target)
    }
    pub fn pointer_down(&mut self, y: f64) -> MinimapProjection {
        if self.geometry.total_rows == 0 {
            return self.projection();
        }
        let p = &self.projection;
        let g = &self.geometry;
        let b = viewport_box(
            g,
            p,
            p.editor_row,
            p.editor_row.saturating_add(g.visible_main_rows),
        );
        let in_box = b.is_some_and(|b| y >= b.top && y < b.top + b.height);
        if in_box {
            self.begin_drag(y);
        } else {
            self.click(y);
            if self.geometry.mode == MinimapMode::Fit {
                self.begin_drag(y);
            }
        }
        self.projection()
    }
    pub fn begin_drag(&mut self, y: f64) {
        if !y.is_finite() || self.geometry.total_rows == 0 {
            return;
        }
        self.drag = Some(if self.geometry.mode == MinimapMode::Proportional {
            Drag::Proportional {
                grab: y - self.projection.box_top,
                start_y: y,
                start_projection: self.projection(),
            }
        } else {
            Drag::Fit {
                row_grab: i128::from(
                    row_at_y(
                        &self.geometry,
                        container_to_logical(&self.geometry, &self.projection, y),
                    )
                    .saturating_sub(self.projection.editor_row),
                ),
            }
        });
    }
    pub fn drag(&mut self, y: f64) -> MinimapProjection {
        if !y.is_finite() {
            return self.projection();
        }
        match self.drag.clone() {
            Some(Drag::Proportional {
                grab,
                start_y,
                start_projection,
            }) => {
                // The slider's bounded fixed-point pixels cannot encode every
                // row of a huge file. A stationary/rebased pointer must retain
                // its exact integer anchor rather than round-trip through it.
                self.projection = if y == start_y {
                    start_projection
                } else {
                    drag_projection(&self.geometry, y, grab)
                };
                self.geometry.main_top_row = self.projection.editor_row;
            }
            Some(Drag::Fit { row_grab }) => {
                let row = i128::from(row_at_y(
                    &self.geometry,
                    container_to_logical(&self.geometry, &self.projection, y),
                )) - row_grab;
                self.set_editor_row(row.clamp(0, i128::from(self.geometry.max_main_row)) as u64);
            }
            None => {}
        }
        self.projection()
    }
    /// Re-anchor a held pointer after a DPI coordinate-system change, without
    /// quantizing or replacing the continuous visual projection. Only nearby
    /// container pixels change; the exact logical origin and editor row do not.
    pub fn rebase_drag(&mut self, y: f64) {
        if !y.is_finite() || self.drag.is_none() {
            return;
        }
        if self.geometry.mode == MinimapMode::Proportional {
            self.begin_drag(y);
        } else {
            // After a coordinate-system change the pointer can be above the
            // current fit box. Retain a signed relative row offset so merely
            // remapping the pointer cannot move the editor. Ordinary fit drag
            // start and its natural/compressed sampling remain unchanged.
            self.drag = Some(Drag::Fit {
                row_grab: i128::from(row_at_y(
                    &self.geometry,
                    container_to_logical(&self.geometry, &self.projection, y),
                )) - i128::from(self.projection.editor_row),
            });
        }
    }
    pub fn drag_anchor(&self) -> Option<(f64, u64)> {
        match &self.drag {
            Some(Drag::Proportional {
                start_y,
                start_projection,
                ..
            }) => Some((*start_y, start_projection.editor_row)),
            _ => None,
        }
    }
    pub fn wheel(&mut self, delta: f64) -> MinimapProjection {
        let policy = if self.geometry.mode == MinimapMode::Proportional {
            WheelPolicy::Main
        } else {
            WheelPolicy::FitMinimap
        };
        let row = wheel_row(
            self.projection.editor_row,
            self.geometry.total_rows,
            self.geometry.visible_main_rows,
            delta,
            policy,
        );
        self.set_editor_row(row)
    }
    pub fn end_drag(&mut self) {
        self.drag = None;
        self.projection = project_editor(&self.geometry, self.projection.editor_row);
    }
    pub fn leave(&mut self) {
        if self.geometry.mode == MinimapMode::Fit {
            self.end_drag();
        }
    }
}
