use hexforge_controller::{controller::MinimapMode, hex::minimap_geometry::*};
fn g(mode: MinimapMode, n: u64, h: u32, r: u32, top: u64, v: u64) -> MinimapGeometry {
    MinimapGeometry::new(MinimapConfig {
        mode,
        total_rows: n,
        height_px: h,
        row_px: r,
        top_row: top,
        visible_main_rows: Some(v),
    })
    .unwrap()
}
#[test]
fn proportional_numerical_oracle() {
    let g = g(MinimapMode::Proportional, 1000, 100, 2, 0, 10);
    assert_eq!((g.viewport_height_px, g.viewport_travel_px), (20, 80));
    for (row, px, origin) in [(0, 0.0, 0), (495, 40.0, 950), (990, 80.0, 1900)] {
        let p = project_editor(&g, row);
        assert_eq!(p.box_top, px);
        assert_eq!(p.editor_row, row);
        assert_eq!(p.virtual_origin, LogicalY(origin * 1024));
        assert!(g.canvas_height_px <= 100);
        if row == 990 {
            let plan = sample_plan(&g, &p);
            assert_eq!(plan.last().unwrap().row, 999);
            assert_eq!(plan.last().unwrap().y, 98.0);
        }
    }
}
#[test]
fn fit_short_file_never_stretches() {
    let g = g(MinimapMode::Fit, 2, 100, 4, 0, 10);
    assert_eq!(g.canvas_height_px, 8);
    assert_eq!(g.viewport_travel_px, 0);
    assert_eq!(row_edge_y(&g, 0).unwrap(), LogicalY(0));
    assert_eq!(row_edge_y(&g, 2).unwrap(), LogicalY(8 * 1024));
    assert_eq!(row_at_y(&g, LogicalY(7 * 1024)), 1);
    assert_eq!(row_at_y(&g, LogicalY(99 * 1024)), 1);
    let plan = sample_plan(&g, &project_editor(&g, 0));
    assert_eq!(
        plan.iter()
            .map(|p| (p.row, p.y, p.height))
            .collect::<Vec<_>>(),
        vec![(0, 0.0, 4.0), (1, 4.0, 4.0)]
    );
}
#[test]
fn fit_long_file_retains_floor_midpoint_endpoints() {
    let n = 1u64 << 56;
    let g = g(MinimapMode::Fit, n, 500, 2, 0, 10);
    assert_eq!(row_edge_y(&g, n).unwrap(), LogicalY(500 * 1024));
    assert_eq!(row_at_y(&g, LogicalY(0)), 0);
    assert_eq!(row_at_y(&g, LogicalY(499 * 1024)), n - 1);
    assert_eq!(
        row_at_y(&g, LogicalY(250 * 1024)),
        ((501u128 * n as u128) / 1000) as u64
    );
    assert_eq!(sample_plan(&g, &project_editor(&g, 0)).len(), 500);
    let one = g_one_pixel();
    assert_eq!(row_at_y(&one, LogicalY(0)), 0);
    let plan = sample_plan(&one, &one.projection());
    assert_eq!(plan.len(), 1);
    assert_eq!((plan[0].row, plan[0].y, plan[0].height), (0, 0.0, 1.0));
}
fn g_one_pixel() -> MinimapGeometry {
    g(MinimapMode::Fit, 10, 1, 2, 0, 1)
}
#[test]
fn raw_edges_and_inverse_remain_exact_above_safe_integer() {
    let n = (1u64 << 56) + 1000;
    let g = g(MinimapMode::Proportional, n, 100, 2, n - 10, 10);
    let row = n - 1;
    let edge = row_edge_y(&g, row).unwrap();
    assert_eq!(edge, LogicalY(row as i128 * 2 * 1024));
    assert_eq!(row_at_y(&g, edge), row);
    assert_eq!(
        project_editor(&g, n - 10).virtual_origin,
        LogicalY((n as i128 * 2 - 100) * 1024)
    );
    assert_eq!(
        sample_plan(&g, &project_editor(&g, n - 10))
            .last()
            .unwrap()
            .row,
        n - 1
    );
}
#[test]
fn zero_travel_still_tracks_editor() {
    let map = g(MinimapMode::Proportional, 100, 20, 2, 90, 10);
    assert_eq!(map.viewport_travel_px, 0);
    assert_eq!(
        project_editor(&map, 90).virtual_origin,
        LogicalY(180 * 1024)
    );
    let empty = g(MinimapMode::Fit, 0, 100, 2, 0, 10);
    assert_eq!(empty.canvas_height_px, 0);
    assert!(sample_plan(&empty, &project_editor(&empty, 0)).is_empty());
    assert!(viewport_box(&empty, &project_editor(&empty, 0), 0, 0).is_none());
}
#[test]
fn partial_edge_rows_and_end_exclusive_box_are_preserved() {
    let map = g(MinimapMode::Proportional, 1000, 101, 2, 495, 10);
    let p = drag_projection(&map, 40.25, 0.0);
    let plan = sample_plan(&map, &p);
    assert_eq!(plan.len(), 52);
    assert_eq!(plan[0].y, 0.0);
    assert!(plan[0].height > 0.0 && plan[0].height < 2.0);
    assert_eq!(plan.last().unwrap().y + plan.last().unwrap().height, 101.0);
    let fit = g(MinimapMode::Fit, 10, 100, 4, 0, 3);
    let b = viewport_box(&fit, &project_editor(&fit, 2), 2, 5).unwrap();
    assert_eq!((b.top, b.height), (8.0, 12.0));
    assert_eq!(row_edge_y(&fit, 5).unwrap(), LogicalY(20 * 1024));
}
#[test]
fn invalid_height_is_rejected_and_wide_ratios_use_checked_exact_arithmetic() {
    assert!(MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Fit,
        total_rows: 2,
        height_px: 100,
        row_px: 0,
        top_row: 0,
        visible_main_rows: None
    })
    .is_err());
    let g = g(
        MinimapMode::Proportional,
        u64::MAX,
        u32::MAX,
        u32::MAX,
        u64::MAX - 1,
        1,
    );
    let p = project_editor(&g, u64::MAX - 1);
    assert!(p.virtual_origin.0 > 0);
    assert_eq!(
        row_at_y(&g, row_edge_y(&g, u64::MAX - 1).unwrap()),
        u64::MAX - 1
    );
}
#[test]
fn default_proportional_window_uses_raw_edges_and_integer_ceiling() {
    let map = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: 100,
        height_px: 101,
        row_px: 3,
        top_row: 10,
        visible_main_rows: None,
    })
    .unwrap();
    assert_eq!(map.visible_mini_rows, 34);
    assert_eq!((map.canvas_height_px, map.logical_height_px), (101, 300));
    for (row, px) in [(9, 27), (10, 30), (45, 135)] {
        assert_eq!(row_edge_y(&map, row).unwrap(), LogicalY(px * 1024));
    }
    assert_eq!(row_at_y(&map, LogicalY(35 * 1024)), 11);
    let p = map.projection();
    assert_eq!(viewport_box(&map, &p, 12, 20).unwrap().height, 101.0);
    assert_eq!(sample_plan(&map, &p)[0].row, p.first_sample_row);
    let top = (1u64 << 56) + 17;
    let huge = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: top + 100,
        height_px: 100,
        row_px: 2,
        top_row: top,
        visible_main_rows: None,
    })
    .unwrap();
    assert_eq!(huge.logical_height_px, i128::from(top + 100) * 2);
    assert_eq!(
        row_at_y(&huge, LogicalY(i128::from(top + 3) * 2 * 1024)),
        top + 3
    );
    assert_eq!(
        viewport_box(&huge, &huge.projection(), top + 3, top + 13)
            .unwrap()
            .height,
        100.0
    );
}
#[test]
fn short_and_empty_proportional_files_keep_natural_canvas_and_no_virtual_scroll() {
    let short = g(MinimapMode::Proportional, 2, 100, 4, 0, 10);
    assert_eq!((short.logical_height_px, short.canvas_height_px), (8, 8));
    let plan = sample_plan(&short, &short.projection());
    assert_eq!(
        plan.iter()
            .map(|p| (p.row, p.y, p.height))
            .collect::<Vec<_>>(),
        [(0, 0.0, 4.0), (1, 4.0, 4.0)]
    );
    let twenty = g(MinimapMode::Proportional, 20, 100, 2, 10, 10);
    assert_eq!(twenty.canvas_height_px, 40);
    assert_eq!(twenty.projection().box_top_sub, LogicalY(20 * 1024));
    assert_eq!(twenty.projection().virtual_origin, LogicalY(0));
    for mode in [MinimapMode::Fit, MinimapMode::Proportional] {
        let empty = g(mode, 0, 100, 2, 0, 10);
        assert_eq!(empty.canvas_height_px, 0);
        assert_eq!(row_at_y(&empty, LogicalY(50 * 1024)), 0);
        assert!(sample_plan(&empty, &empty.projection()).is_empty());
        assert!(viewport_box(&empty, &empty.projection(), 0, 0).is_none());
    }
}
