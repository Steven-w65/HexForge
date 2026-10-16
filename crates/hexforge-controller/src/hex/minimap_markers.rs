//! One marker dataset for the minimap and overview, independently projected.
use super::minimap_geometry::{row_edge_y, MinimapGeometry, MinimapProjection, SUBPIXELS};
use crate::controller::{MinimapMode, RowWidth};
use hexforge_core::dto::ModifiedOverviewDto;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkerKind {
    Search,
    Template,
    Modified,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimapMarker {
    pub kind: MarkerKind,
    pub row: u64,
    pub end_exclusive: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelRun {
    pub kind: MarkerKind,
    pub top: u32,
    pub height: u32,
}
pub fn collect_markers(
    matches: &[u64],
    match_length: u64,
    selected_template_ranges: &[(u64, u64)],
    modified: &ModifiedOverviewDto,
    file_size: u64,
    row_width: RowWidth,
) -> Vec<MinimapMarker> {
    if file_size == 0 {
        return vec![];
    }
    let w = match row_width {
        RowWidth::Sixteen => 16u128,
        RowWidth::ThirtyTwo => 32u128,
    };
    let total = u128::from(file_size).div_ceil(w);
    let mut markers = vec![];
    let mut append = |kind, start: u64, len: u64| {
        let start = u128::from(start).min(u128::from(file_size));
        let end = (start + u128::from(len)).min(u128::from(file_size));
        if len > 0 && start < end {
            markers.push(MinimapMarker {
                kind,
                row: (start / w) as u64,
                end_exclusive: end.div_ceil(w).min(total) as u64,
            });
        }
    };
    if match_length > 0 {
        for &offset in matches {
            append(MarkerKind::Search, offset, match_length);
        }
    }
    // Only the active parsed-field selection belongs here, never all results.
    for &(offset, length) in selected_template_ranges {
        append(MarkerKind::Template, offset, length);
    }
    if modified.bin_count > 0 {
        for &bin in &modified.bins {
            if bin >= modified.bin_count {
                continue;
            }
            let at = if modified.bin_count == 1 {
                0
            } else {
                (u128::from(bin) * u128::from(file_size - 1) / u128::from(modified.bin_count - 1))
                    as u64
            };
            append(MarkerKind::Modified, at, 1);
        }
    }
    markers.sort_by_key(|m| (m.kind, m.row));
    let mut merged: Vec<MinimapMarker> = vec![];
    for m in markers {
        if let Some(last) = merged
            .last_mut()
            .filter(|l| l.kind == m.kind && m.row <= l.end_exclusive)
        {
            last.end_exclusive = last.end_exclusive.max(m.end_exclusive);
        } else {
            merged.push(m);
        }
    }
    merged.sort_by_key(|m| (m.row, m.kind));
    merged
}
fn project_runs(
    markers: &[MinimapMarker],
    height: u32,
    project: impl Fn(&MinimapMarker) -> Option<(f64, f64)>,
) -> Vec<PixelRun> {
    if height == 0 {
        return vec![];
    }
    let mut diff = [
        vec![0i32; height as usize + 1],
        vec![0; height as usize + 1],
        vec![0; height as usize + 1],
    ];
    for m in markers {
        let Some((top, bottom)) = project(m) else {
            continue;
        };
        let top = top.floor().clamp(0.0, (height - 1) as f64) as usize;
        let bottom = bottom.ceil().clamp((top + 1) as f64, height as f64) as usize;
        let d = &mut diff[m.kind as usize];
        d[top] += 1;
        d[bottom] -= 1;
    }
    let mut active = [0; 3];
    let mut runs: Vec<PixelRun> = vec![];
    for y in 0..height {
        for k in 0..3 {
            active[k] += diff[k][y as usize];
        }
        let kind = if active[2] > 0 {
            Some(MarkerKind::Modified)
        } else if active[1] > 0 {
            Some(MarkerKind::Template)
        } else if active[0] > 0 {
            Some(MarkerKind::Search)
        } else {
            None
        };
        if let Some(kind) = kind {
            if let Some(last) = runs
                .last_mut()
                .filter(|r| r.kind == kind && r.top + r.height == y)
            {
                last.height += 1;
            } else {
                runs.push(PixelRun {
                    kind,
                    top: y,
                    height: 1,
                });
            }
        }
    }
    runs
}
pub fn project_minimap(
    markers: &[MinimapMarker],
    g: &MinimapGeometry,
    p: &MinimapProjection,
) -> Vec<PixelRun> {
    project_runs(markers, g.canvas_height_px, |m| {
        if g.mode == MinimapMode::Proportional {
            let stride = i128::from(g.row_px) * SUBPIXELS;
            let end = p.virtual_origin.0 + i128::from(g.canvas_height_px) * SUBPIXELS;
            let start = i128::from(m.row) * stride;
            let bottom = i128::from(m.end_exclusive) * stride;
            if bottom <= p.virtual_origin.0 || start >= end {
                return None;
            }
            // Clip exact global coordinates, THEN convert their bounded difference.
            Some((
                (start.max(p.virtual_origin.0) - p.virtual_origin.0) as f64 / SUBPIXELS as f64,
                (bottom.min(end) - p.virtual_origin.0) as f64 / SUBPIXELS as f64,
            ))
        } else {
            Some((
                row_edge_y(g, m.row).ok()?.0 as f64 / SUBPIXELS as f64,
                row_edge_y(g, m.end_exclusive).ok()?.0 as f64 / SUBPIXELS as f64,
            ))
        }
    })
}
pub fn project_overview(markers: &[MinimapMarker], total_rows: u64, height: u32) -> Vec<PixelRun> {
    project_runs(markers, height, |m| {
        if total_rows == 0 {
            return None;
        }
        let last = total_rows - 1;
        let project = |row: u64| {
            if last == 0 {
                0.0
            } else {
                (u128::from(row) * u128::from(height - 1) / u128::from(last)) as f64
            }
        };
        Some((
            project(m.row),
            project(m.end_exclusive.saturating_sub(1)) + 1.0,
        ))
    })
}
