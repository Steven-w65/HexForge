use hexforge_controller::{
    controller::MinimapMode,
    hex::{minimap_geometry::*, minimap_interaction::MinimapInteraction},
};
fn interaction(mode: MinimapMode, n: u64, top: u64) -> MinimapInteraction {
    MinimapInteraction::new(
        MinimapGeometry::new(MinimapConfig {
            mode,
            total_rows: n,
            height_px: 100,
            row_px: 2,
            top_row: top,
            visible_main_rows: Some(10),
        })
        .unwrap(),
    )
}
#[test]
fn large_origin_noncentral_drag_is_continuous() {
    let mut i = interaction(MinimapMode::Proportional, (1u64 << 56) + 1000, 0);
    i.begin_drag(7.25);
    let p = i.drag(47.5);
    assert_eq!(p.box_top, 40.25);
    assert!(i.captured());
    let old = p.virtual_origin;
    assert!(i.drag(47.51).virtual_origin >= old);
    let p = i.drag(87.25);
    assert_eq!(p.editor_row, (1u64 << 56) + 990);
    assert_eq!(p.box_top, 80.0);
    i.end_drag();
    assert!(!i.captured());
}
#[test]
fn outside_drag_clamps_without_ending() {
    let mut i = interaction(MinimapMode::Proportional, 1000, 0);
    i.pointer_down(7.0);
    assert_eq!(i.drag(200.0).editor_row, 990);
    assert!(i.captured());
    assert_eq!(i.drag(-100.0).editor_row, 0);
    assert!(i.captured());
    i.end_drag();
}
#[test]
fn upward_drag_preserves_noncentral_grab() {
    let mut i = interaction(MinimapMode::Proportional, 1000, 990);
    i.begin_drag(85.0);
    let p = i.drag(45.0);
    assert_eq!((p.editor_row, p.box_top), (495, 40.0));
    assert_eq!(i.drag(5.0).editor_row, 0);
}
#[test]
fn preview_click_does_not_drag_box() {
    let mut i = interaction(MinimapMode::Proportional, 1000, 0);
    let p = i.pointer_down(60.0);
    assert_eq!(p.editor_row, 30);
    assert!(!i.captured());
    assert_eq!(i.drag(80.0).editor_row, 30);
}
#[test]
fn wheel_is_applied_once() {
    let mut i = interaction(MinimapMode::Proportional, 1000, 35);
    let initial = i.projection().virtual_origin;
    let p = i.wheel(44.0);
    assert_eq!(p.editor_row, 36);
    assert!(p.virtual_origin > initial);
    assert_eq!(i.wheel(-44.0).editor_row, 35);
    assert_eq!(i.projection().virtual_origin, initial);
    let mut fit = interaction(MinimapMode::Fit, 100, 10);
    assert_eq!(fit.wheel(44.0).editor_row, 12);
}
#[test]
fn fit_click_centering_grab_offset_and_leave_are_unchanged() {
    let mut i = interaction(MinimapMode::Fit, 1000, 0);
    assert_eq!(i.click(99.0).editor_row, 990);
    assert_eq!(i.click(0.0).editor_row, 0);
    let g = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Fit,
        total_rows: 100,
        height_px: 100,
        row_px: 1,
        top_row: 30,
        visible_main_rows: Some(10),
    })
    .unwrap();
    let mut i = MinimapInteraction::new(g);
    i.begin_drag(35.0);
    assert_eq!(i.drag(40.0).editor_row, 35);
    i.leave();
    assert!(!i.captured());
    assert_eq!(i.drag(80.0).editor_row, 35);
}
#[test]
fn full_range_noncentral_drag_and_huge_fit_navigation_match_reference() {
    let mut i = interaction(MinimapMode::Proportional, 1000, 0);
    i.begin_drag(7.0);
    let p = i.drag(47.0);
    assert_eq!(
        (p.box_top, p.editor_row, p.virtual_origin),
        (40.0, 495, LogicalY(950 * 1024))
    );
    let p = i.drag(87.0);
    assert_eq!(
        (p.box_top, p.editor_row, p.virtual_origin),
        (80.0, 990, LogicalY(1900 * 1024))
    );
    assert_eq!(i.drag(200.0).editor_row, 990);
    assert_eq!((i.drag(-20.0).box_top, i.projection().editor_row), (0.0, 0));
    i.end_drag();
    assert!(!i.captured());
    assert_eq!(i.drag(80.0).editor_row, 0);
    let mut fit = interaction(MinimapMode::Fit, 1u64 << 56, 0);
    assert_eq!(fit.click(99.0).editor_row, (1u64 << 56) - 10);
}

#[test]
fn resizing_a_held_huge_file_keeps_exact_rows_and_rebases_the_pointer() {
    let n = (1u64 << 56) + 1000;
    let held_row = (1u64 << 56) + 333;
    let mut i = interaction(MinimapMode::Proportional, n, held_row);
    i.begin_drag(i.projection().box_top + 7.25);
    i.replace_geometry(
        MinimapGeometry::new(MinimapConfig {
            mode: MinimapMode::Proportional,
            total_rows: n,
            height_px: 200,
            row_px: 2,
            top_row: held_row,
            visible_main_rows: Some(15),
        })
        .unwrap(),
    );
    assert!(
        i.captured(),
        "A compatible resize must not end the held gesture"
    );
    i.rebase_drag(130.25);
    assert_eq!(
        i.drag(130.25).editor_row,
        held_row,
        "A stationary remapped pointer must retain the exact row above 2^53"
    );
    assert_eq!(i.drag(1000.0).editor_row, n - 15);
    assert_eq!(
        i.projection().virtual_origin,
        LogicalY((i128::from(n) * 2 - 200) * 1024)
    );
}
