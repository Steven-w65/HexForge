use hexforge_controller::{
    controller::RowWidth,
    hex::{
        layout::{visible_range, HexLayout, HexMetrics, HexViewState, LocalPoint},
        scroll::{
            clamp_top_row, offset_to_overview_pixel, row_to_thumb, thumb_to_row, wheel_row,
            Scrollbar, WheelPolicy,
        },
        selection::{contains_offset, normalize_selection, Selection, SelectionInput},
    },
};
fn layout(width: RowWidth) -> HexLayout {
    HexLayout::new(
        HexMetrics {
            width: 900.0,
            char_width: 8.0,
        },
        width,
    )
}
fn view(size: Option<u64>, top: u64, height: f64) -> HexViewState {
    HexViewState {
        file_size: size,
        top_row: top,
        height,
        horizontal_px: 0.0,
    }
}
#[test]
fn gutter_stays_visible_after_horizontal_scroll() {
    let l = layout(RowWidth::ThirtyTwo);
    let mut v = view(Some(1024), 0, 400.0);
    v.horizontal_px = 200.0;
    assert_eq!(l.address_x, 16.0);
    assert!(l.hit_test(LocalPoint { x: 20.0, y: 40.0 }, &v).is_none());
    assert_eq!(
        l.hit_test(
            LocalPoint {
                x: l.hex_x + 8.0 * l.byte_stride + 8.0 - 200.0 + 1.0,
                y: 40.0
            },
            &v
        ),
        Some(8)
    );
}
#[test]
fn group_gaps_and_eof_are_not_bytes() {
    let l = layout(RowWidth::Sixteen);
    let v = view(Some(20), 1, 400.0);
    assert_eq!(l.row_height, 22.0);
    assert_eq!(l.header_height, 32.0);
    assert_eq!(l.group_gap, 8.0);
    assert!(l
        .hit_test(
            LocalPoint {
                x: l.hex_x + 7.0 * l.byte_stride + l.byte_cell_width + 1.0,
                y: 36.0
            },
            &v
        )
        .is_none());
    assert_eq!(
        l.hit_test(
            LocalPoint {
                x: l.hex_x + 3.0 * l.byte_stride + 2.0,
                y: 36.0
            },
            &v
        ),
        Some(19)
    );
    assert!(l
        .hit_test(
            LocalPoint {
                x: l.hex_x + 4.0 * l.byte_stride + 2.0,
                y: 36.0
            },
            &v
        )
        .is_none());
    assert!(l
        .hit_test(
            LocalPoint {
                x: l.hex_x,
                y: 31.0
            },
            &v
        )
        .is_none());
}
#[test]
fn hex_ascii_mapping_and_measured_gaps_match() {
    for width in [RowWidth::Sixteen, RowWidth::ThirtyTwo] {
        let l = HexLayout::new(
            HexMetrics {
                width: 900.0,
                char_width: 10.0,
            },
            width,
        );
        let v = view(Some(1024), 2, 400.0);
        for i in 0..width.bytes() {
            let x = l.hex_cell_x(i as usize) + 1.0;
            let a = l.ascii_cell_x(i as usize) + 1.0;
            assert_eq!(
                l.hit_test(LocalPoint { x, y: 40.0 }, &v),
                Some(2 * width.bytes() + i)
            );
            assert_eq!(
                l.hit_test(LocalPoint { x: a, y: 40.0 }, &v),
                Some(2 * width.bytes() + i)
            );
        }
        assert_eq!(l.group_gap, 8.0);
    }
}
#[test]
fn selection_is_inclusive_and_reversible() {
    let mut s = Selection::default();
    s.handle(SelectionInput::Begin(12), 20);
    let update = s.handle(SelectionInput::Move(8), 20);
    let selected = update.selection.unwrap();
    assert_eq!((selected.start, selected.end, selected.count()), (8, 12, 5));
    s.handle(SelectionInput::End, 20);
    assert!(!s.captured());
    let start = 9_007_199_254_740_993;
    s.handle(SelectionInput::Begin(start), u64::MAX);
    assert_eq!(
        s.handle(SelectionInput::Move(start + 2), u64::MAX)
            .selection
            .unwrap()
            .count(),
        3
    );
}

#[test]
fn inclusive_selection_contains_both_endpoints_and_preserves_a_single_byte() {
    let selection = normalize_selection(12, 8);
    assert!(contains_offset(Some(&selection), 8));
    assert!(contains_offset(Some(&selection), 12));
    assert!(!contains_offset(Some(&selection), 13));
    assert!(!contains_offset(None, 8));
    let single = normalize_selection(7, 7);
    assert_eq!((single.start, single.end, single.count()), (7, 7, 1));
    let large = normalize_selection(9_007_199_254_740_993, 9_007_199_254_740_995);
    assert_eq!(
        (large.start, large.end, large.count()),
        (9_007_199_254_740_993, 9_007_199_254_740_995, 3)
    );
}

#[test]
fn reference_horizontal_extent_and_read_cap_are_preserved() {
    let sixteen = layout(RowWidth::Sixteen);
    let thirty_two = layout(RowWidth::ThirtyTwo);
    assert_eq!(sixteen.content_width(), 728.0);
    assert_eq!(thirty_two.content_width(), 1272.0);
    let top = 9_007_199_254_740_993;
    let range = visible_range(
        &thirty_two,
        &view(Some((top + 100_000) * 32), top, f64::MAX),
        2.0,
    )
    .unwrap();
    assert_eq!(
        (range.first_row, range.row_count, range.byte_length),
        (9_007_199_254_740_991, 32768, 1_048_576)
    );
    assert!(sixteen
        .hit_at(162.0, 9_007_199_254_740_991.0 * 44.0 + 32.0, 0, u64::MAX)
        .is_none());
}

#[test]
fn overview_and_scrollbar_clamps_and_effective_travel_match_reference() {
    assert_eq!(offset_to_overview_pixel(0, 1 << 60, 500.0), 0.0);
    assert_eq!(
        offset_to_overview_pixel((1 << 60) + 1, 1 << 60, 500.0),
        499.0
    );
    assert_eq!(row_to_thumb(99, 10, 1000.0), 1000.0);
    assert_eq!(thumb_to_row(-5.0, 10, 1000.0), 0);
    assert_eq!(thumb_to_row(2000.0, 10, 1000.0), 9);
    for (row, total, track) in [(0, 0, 1000.0), (0, 1, 1000.0), (2, 10, 0.0)] {
        assert_eq!(row_to_thumb(row, total, track), 0.0);
        assert_eq!(thumb_to_row(row as f64, total, track), 0);
    }
    for (row, px) in [(0, 0.0), (238, 238.0), (476, 476.0)] {
        assert_eq!(row_to_thumb(row, 477, 476.0), px);
        assert_eq!(thumb_to_row(px, 477, 476.0), row);
    }
    let before = row_to_thumb(9_007_199_254_740_864, 18_014_398_509_481_731, 1000.0);
    let after = row_to_thumb(9_007_199_254_740_865, 18_014_398_509_481_731, 1000.0);
    assert!(after >= before);
    assert!(thumb_to_row(500.0, 18_014_398_509_481_731, 1000.0) < 18_014_398_509_481_731);
}
#[test]
fn last_full_page_contains_final_row() {
    for (size, width) in [
        (1, RowWidth::Sixteen),
        (100, RowWidth::Sixteen),
        (32000, RowWidth::ThirtyTwo),
        (u64::MAX, RowWidth::ThirtyTwo),
    ] {
        let l = layout(width);
        let total = l.total_rows(size);
        let visible = l.fully_visible_rows(252.0);
        let top = clamp_top_row(total, visible, u64::MAX);
        assert!(top + visible >= total);
        assert_eq!(top, total.saturating_sub(visible));
        let range = visible_range(&l, &view(Some(size), top, 252.0), 2.0).unwrap();
        assert_eq!(
            range.byte_start as u128 + range.byte_length as u128,
            size as u128
        );
    }
}
#[test]
fn huge_offsets_are_exact_before_local_conversion() {
    let l = layout(RowWidth::Sixteen);
    let top = 9_007_199_254_740_993;
    let v = view(Some(u64::MAX), top, 400.0);
    assert_eq!(
        l.hit_test(
            LocalPoint {
                x: l.hex_x + 3.0 * l.byte_stride + 2.0,
                y: l.header_height + 3.0 * l.row_height + 4.0
            },
            &v
        ),
        Some(top * 16 + 51)
    );
    let r = visible_range(&l, &v, 2.0).unwrap();
    assert_eq!(r.first_row, top - 2);
    assert_eq!(r.byte_start, (top - 2) * 16);
    assert_eq!(l.row_y(top + 3, top), Some(98.0));
}
#[test]
fn prefetched_ranges_are_bounded_and_empty_scenes_do_not_read() {
    let l = layout(RowWidth::Sixteen);
    assert!(visible_range(&l, &view(None, 0, 400.0), 2.0).is_none());
    assert!(visible_range(&l, &view(Some(0), 0, 400.0), 2.0).is_none());
    let r = visible_range(&l, &view(Some(100), 2, 45.0), 2.0).unwrap();
    assert_eq!(
        (r.first_row, r.row_count, r.byte_start, r.byte_length),
        (0, 7, 0, 100)
    );
    let r = visible_range(&l, &view(Some(1_000_000), 100, 500.0), 80.0).unwrap();
    assert_eq!(
        (r.first_row, r.row_count, r.byte_start, r.byte_length),
        (20, 183, 320, 2928)
    );
    let r = visible_range(&l, &view(Some(u64::MAX), 100000, f64::MAX), 80.0).unwrap();
    assert_eq!(r.first_row, 99998);
    assert_eq!(r.byte_length, 1048576);
}
#[test]
fn integer_scroll_mapping_preserves_both_endpoints_and_large_midpoint() {
    let size = 1u64 << 60;
    assert_eq!(offset_to_overview_pixel(size / 2, size, 500.0), 249.0);
    assert_eq!(offset_to_overview_pixel(size - 1, size, 500.0), 499.0);
    assert_eq!(row_to_thumb(9_000_000_000, 9_000_000_001, 1000.0), 1000.0);
    assert_eq!(thumb_to_row(1000.0, 9_000_000_001, 1000.0), 9_000_000_000);
    assert_eq!(row_to_thumb(238, 477, 476.0), 238.0);
    assert_eq!(thumb_to_row(238.0, 477, 476.0), 238);
}
#[test]
fn main_wheel_and_scrollbar_retain_native_pointer_grab_policy() {
    assert_eq!(wheel_row(50, 100, 10, 1000.0, WheelPolicy::Main), 51);
    assert_eq!(wheel_row(50, 100, 10, -1.0, WheelPolicy::Main), 49);
    assert_eq!(wheel_row(0, 100, 10, -10.0, WheelPolicy::Main), 0);
    let mut bar = Scrollbar::new(1000, 10, 100.0, 495);
    let grab = bar.top() + 3.0;
    bar.begin(grab);
    assert_eq!(bar.drag(1000.0), 990);
    assert_eq!(bar.drag(-100.0), 0);
    bar.end();
    assert!(!bar.captured());
}

#[test]
fn zero_height_and_zero_requested_visible_count_cannot_scroll_beyond_eof() {
    let mut bar = Scrollbar::new(u64::MAX, 0, 0.0, u64::MAX);
    bar.begin(0.0);
    bar.drag(100.0);
    assert!(bar.row < u64::MAX);
    assert_eq!(clamp_top_row(1, 0, u64::MAX), 0);
}
