use hexforge_controller::{
    controller::{MinimapMode, RowWidth},
    hex::{minimap_geometry::*, minimap_markers::*},
};
use hexforge_core::dto::ModifiedOverviewDto;
fn markers(
    matches: &[u64],
    len: u64,
    fields: &[(u64, u64)],
    size: u64,
    bins: &[u32],
) -> Vec<MinimapMarker> {
    collect_markers(
        matches,
        len,
        fields,
        &ModifiedOverviewDto {
            bin_count: 1024,
            bins: bins.to_vec(),
        },
        size,
        RowWidth::Sixteen,
    )
}
fn geometry(n: u64, h: u32, r: u32, top: u64) -> MinimapGeometry {
    MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: n,
        height_px: h,
        row_px: r,
        top_row: top,
        visible_main_rows: None,
    })
    .unwrap()
}
#[test]
fn row_crossings_and_large_offsets_are_exact() {
    assert_eq!(
        markers(&[15], 3, &[(31, 3)], 64, &[]),
        vec![
            MinimapMarker {
                kind: MarkerKind::Search,
                row: 0,
                end_exclusive: 2
            },
            MinimapMarker {
                kind: MarkerKind::Template,
                row: 1,
                end_exclusive: 3
            }
        ]
    );
    let offset = (1u64 << 56) + 16;
    assert_eq!(
        markers(&[offset], 1, &[], offset + 16, &[])[0].row,
        offset / 16
    );
    assert_eq!(
        markers(&[offset], 1, &[], offset + 16, &[])[0].end_exclusive,
        offset / 16 + 1
    );
}
#[test]
fn overview_last_row_is_pinned_to_last_pixel() {
    let m = markers(&[], 1, &[], 160, &[1023]);
    assert_eq!(
        m,
        [MinimapMarker {
            kind: MarkerKind::Modified,
            row: 9,
            end_exclusive: 10
        }]
    );
    assert_eq!(
        project_overview(&m, 10, 100),
        vec![PixelRun {
            kind: MarkerKind::Modified,
            top: 99,
            height: 1
        }]
    );
    assert_eq!(
        project_overview(&markers(&[99], 1, &[], 100, &[]), 7, 500)[0].top,
        499
    );
    assert_eq!(
        project_overview(&markers(&[99], 1, &[], 100, &[]), 7, 500),
        [PixelRun {
            kind: MarkerKind::Search,
            top: 499,
            height: 1
        }]
    );
}
#[test]
fn dense_runs_are_coalesced_and_priority_is_preserved() {
    let g = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Fit,
        total_rows: 10000,
        height_px: 500,
        row_px: 2,
        top_row: 0,
        visible_main_rows: None,
    })
    .unwrap();
    let m = markers(
        &(0..10000).map(|i| i * 16).collect::<Vec<_>>(),
        1,
        &[],
        160000,
        &[],
    );
    let runs = project_minimap(&m, &g, &g.projection());
    assert!(runs.len() <= 500);
    assert_eq!(runs[0].top, 0);
    let g = geometry(10, 10, 1, 0);
    let m = markers(&[0], 16, &[(0, 16)], 160, &[0]);
    assert_eq!(
        project_minimap(&m, &g, &g.projection())[0],
        PixelRun {
            kind: MarkerKind::Modified,
            top: 0,
            height: 1
        }
    );
    assert_eq!(
        project_overview(&m, 10, 10)[0],
        PixelRun {
            kind: MarkerKind::Modified,
            top: 0,
            height: 1
        }
    );
}
#[test]
fn off_tile_markers_remain_in_the_shared_overview() {
    let g = geometry(10000, 100, 2, 995);
    let m = markers(&[0, 16000], 1, &[], 160000, &[]);
    assert_eq!(
        project_minimap(&m, &g, &g.projection()),
        vec![PixelRun {
            kind: MarkerKind::Search,
            top: 10,
            height: 2
        }]
    );
    assert!(project_overview(&m, 10000, 100).iter().any(|r| r.top == 0));
    assert!(project_overview(&m, 10000, 100).iter().any(|r| r.top > 0));
    let top = (1u64 << 56) + 17;
    let g = geometry(top + 100, 100, 2, top);
    assert_eq!(
        project_minimap(
            &[MinimapMarker {
                kind: MarkerKind::Search,
                row: top + 3,
                end_exclusive: top + 4
            }],
            &g,
            &g.projection()
        )[0],
        PixelRun {
            kind: MarkerKind::Search,
            top: 6,
            height: 2
        }
    );
}
#[test]
fn fractional_tile_markers_clip_before_float_conversion() {
    let g = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: 1000,
        height_px: 101,
        row_px: 2,
        top_row: 495,
        visible_main_rows: Some(10),
    })
    .unwrap();
    let p = drag_projection(&g, 40.25, 0.0);
    let first = p.first_sample_row;
    let a = project_minimap(
        &[MinimapMarker {
            kind: MarkerKind::Search,
            row: first,
            end_exclusive: first + 1,
        }],
        &g,
        &p,
    );
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].top, 0);
    assert!(a[0].height < 2);
    assert_eq!(
        project_minimap(
            &[MinimapMarker {
                kind: MarkerKind::Search,
                row: first + 1,
                end_exclusive: first + 2
            }],
            &g,
            &p
        )[0],
        PixelRun {
            kind: MarkerKind::Search,
            top: 0,
            height: 3
        }
    );
}
#[test]
fn no_selected_template_field_creates_no_template_marker() {
    let m = markers(&[], 0, &[], 132, &[1023]);
    assert!(m.iter().all(|m| m.kind != MarkerKind::Template));
    assert!(m.iter().any(|m| m.kind == MarkerKind::Modified));
}
