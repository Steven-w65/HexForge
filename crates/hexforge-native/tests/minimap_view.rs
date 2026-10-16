use eframe::egui::Color32;
use hexforge_controller::{
    controller::{MinimapMode, MinimapRender, RowWidth, Theme},
    hex::{minimap_data::MinimapRow, minimap_geometry::*},
};
use hexforge_native::drawing::{
    glyphs::GlyphCache,
    minimap::{MinimapDrawCache, MinimapOptions, MinimapScene},
};
use std::collections::BTreeMap;

fn geometry(mode: MinimapMode, rows: u64, height: u32, row_px: u32) -> MinimapGeometry {
    MinimapGeometry::new(MinimapConfig {
        mode,
        total_rows: rows,
        height_px: height,
        row_px,
        top_row: 0,
        visible_main_rows: Some(1),
    })
    .unwrap()
}
fn row(row: u64, bytes: &[u8], modified: &[u64]) -> MinimapRow {
    MinimapRow {
        row,
        bytes: bytes.to_vec(),
        modified_offsets: modified.to_vec(),
    }
}
fn options() -> MinimapOptions {
    MinimapOptions {
        row_width: RowWidth::Sixteen,
        render: MinimapRender::Characters,
        theme: Theme::Dark,
    }
}
fn raster(cache: &MinimapDrawCache, x: usize, width: usize, height: usize) -> Vec<String> {
    (0..height)
        .map(|y| {
            (x..x + width)
                .map(|x| {
                    if cache.image().pixels[y * cache.image().size[0] + x]
                        == Color32::from_rgb(22, 27, 32)
                    {
                        '0'
                    } else {
                        '1'
                    }
                })
                .collect()
        })
        .collect()
}
#[test]
fn every_nonspace_glyph_survives_compression() {
    let mut cache = GlyphCache::default();
    for (width, height) in [(3, 1), (3, 2), (3, 5), (4, 6)] {
        for byte in 0..=255 {
            let runs = cache.get(byte, width, height);
            assert_eq!(runs.is_empty(), byte == 0x20, "byte {byte}");
            assert!(runs.iter().all(|r| r.x + r.width <= width && r.y < height));
        }
    }
}
#[test]
fn glyph_rasters_reuse_normalized_bytes_and_evict_then_clear_under_pressure() {
    use hexforge_native::drawing::glyphs::GlyphRun;
    use std::sync::Arc;
    let mut cache = GlyphCache::default();
    let first = cache.get(0x41, 3, 5);
    assert!(Arc::ptr_eq(&first, &cache.get(0x41, 3, 5)));
    assert!(Arc::ptr_eq(&cache.get(0, 3, 5), &cache.get(0xff, 3, 5)));
    assert_eq!(
        &*cache.get(0xff, 3, 5),
        &[GlyphRun {
            x: 1,
            y: 4,
            width: 1
        }]
    );
    for byte in 0x20..=0x7e {
        for width in 1..=3 {
            for height in 1..=5 {
                cache.get(byte, width, height);
            }
        }
    }
    let later = cache.get(0x41, 3, 5);
    assert!(!Arc::ptr_eq(&first, &later));
    assert_eq!(first, later);
    cache.clear();
    assert!(!Arc::ptr_eq(&later, &cache.get(0x41, 3, 5)));
}
#[test]
fn glyphs_are_distinct_sharp_outlines_not_character_bitmasks() {
    for row_width in [RowWidth::Sixteen, RowWidth::ThirtyTwo] {
        let g = geometry(MinimapMode::Proportional, 1, 20, 2);
        let rows = BTreeMap::from([(0, row(0, &[65, 66, 32, 0], &[]))]);
        let width = if row_width == RowWidth::Sixteen {
            84
        } else {
            136
        };
        let mut cache = MinimapDrawCache::default();
        cache.render(&MinimapScene {
            geometry: &g,
            projection: &g.projection(),
            rows: &rows,
            width_px: width,
            dpr: 1.0,
            options: MinimapOptions {
                row_width,
                ..options()
            },
        });
        let stride = (width - 8) as f64 / row_width.bytes() as f64;
        assert_eq!(raster(&cache, 4, 3, 2), ["111", "101"]);
        assert_eq!(
            raster(&cache, (4.0 + stride).round() as usize, 3, 2),
            ["110", "110"]
        );
        assert_eq!(
            raster(&cache, (4.0 + 2.0 * stride).round() as usize, 3, 2),
            ["000", "000"]
        );
        assert_eq!(
            raster(&cache, (4.0 + 3.0 * stride).round() as usize, 3, 2),
            ["000", "010"]
        );
        assert!(cache.image().size[1] <= 20);
    }
}
#[test]
fn edited_space_is_orange_while_ordinary_space_is_blank() {
    let g = geometry(MinimapMode::Fit, 1, 20, 4);
    let rows = BTreeMap::from([(0, row(0, &[32, 32], &[1]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert_eq!(raster(&cache, 4, 3, 4), ["000", "000", "000", "000"]);
    assert!(cache
        .image()
        .pixels
        .contains(&Color32::from_rgb(240, 136, 62)));
}
#[test]
fn changed_row_clears_on_device_boundaries_without_ghosts() {
    let g = geometry(MinimapMode::Proportional, 4, 8, 2);
    let p = g.projection();
    let mut rows = BTreeMap::from([(0, row(0, &[65], &[])), (1, row(1, &[66], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.25,
        options: options(),
    });
    rows.insert(0, row(0, &[32], &[]));
    let update = cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.25,
        options: options(),
    });
    assert_eq!(update.rects, vec![[0, 0, 105, 3]]);
    assert!(cache.image().pixels[..315]
        .iter()
        .all(|c| *c == Color32::from_rgb(22, 27, 32)));
}
#[test]
fn integer_scroll_reuses_overlap_and_fractional_scroll_repaints_bounded_tile() {
    let g = geometry(MinimapMode::Proportional, 100, 8, 2);
    let rows = (0..5).map(|n| (n, row(n, &[65 + n as u8], &[]))).collect();
    let mut cache = MinimapDrawCache::default();
    let mut p = g.projection();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    p.virtual_origin = LogicalY(2 * SUBPIXELS);
    p.first_sample_row = 1;
    let update = cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert_eq!(update.copied_device_rows, 6);
    assert!(!update.full);
    p.virtual_origin = LogicalY(2 * SUBPIXELS + 1);
    assert!(
        cache
            .render(&MinimapScene {
                geometry: &g,
                projection: &p,
                rows: &rows,
                width_px: 84,
                dpr: 1.0,
                options: options()
            })
            .full
    );
}
#[test]
fn huge_origin_is_subtracted_before_raster_conversion() {
    let n = (1u64 << 56) + 117;
    let g = geometry(MinimapMode::Proportional, n, 8, 2);
    let p = project_editor(&g, g.max_main_row);
    let rows = BTreeMap::from([(n - 1, row(n - 1, &[65], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 2.0,
        options: options(),
    });
    assert_eq!(cache.image().size, [168, 16]);
    assert!(cache.image().pixels[12 * 168..]
        .iter()
        .any(|c| *c == Color32::from_rgb(174, 192, 208)));
}
#[test]
fn zero_and_empty_samples_keep_a_bounded_background() {
    let mut cache = MinimapDrawCache::default();
    let g = geometry(MinimapMode::Fit, 0, 100, 2);
    let update = cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &BTreeMap::new(),
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert!(update.full);
    assert_eq!(cache.image().size, [84, 1]);
}
#[test]
fn compact_dots_survive_fractional_dpi_in_both_modes() {
    for mode in [MinimapMode::Fit, MinimapMode::Proportional] {
        let g = geometry(mode, if mode == MinimapMode::Fit { 100 } else { 4 }, 8, 2);
        let rows = (0..100).map(|n| (n, row(n, &[0], &[]))).collect();
        let mut cache = MinimapDrawCache::default();
        cache.render(&MinimapScene {
            geometry: &g,
            projection: &g.projection(),
            rows: &rows,
            width_px: 84,
            dpr: 1.25,
            options: options(),
        });
        let ink = cache
            .image()
            .pixels
            .iter()
            .filter(|&&c| c == Color32::from_rgb(95, 116, 133))
            .count();
        assert_eq!(ink, if mode == MinimapMode::Fit { 8 } else { 4 });
    }
}
#[test]
fn only_changed_rows_upload_when_viewport_box_moves() {
    let g = geometry(MinimapMode::Fit, 2, 20, 4);
    let rows = BTreeMap::from([(0, row(0, &[65], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    let update = cache.render(&MinimapScene {
        geometry: &g,
        projection: &project_editor(&g, 1),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert!(!update.full);
    assert!(update.rects.is_empty());
}

#[test]
fn larger_native_glyphs_keep_the_source_a_and_dot_outlines() {
    let g = geometry(MinimapMode::Fit, 1, 20, 6);
    let rows = BTreeMap::from([(0, row(0, &[0x41, 0], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert_eq!(
        raster(&cache, 4, 3, 6),
        ["010", "101", "111", "101", "101", "000"]
    );
    assert_eq!(
        raster(&cache, 9, 3, 6),
        ["000", "000", "000", "000", "010", "000"]
    );
}

#[test]
fn printable_glyphs_dots_and_modified_color_keep_their_exact_byte_cells() {
    let g = geometry(MinimapMode::Fit, 2, 20, 4);
    let rows = BTreeMap::from([(0, row(0, &[0x41, 0, 0x42], &[2]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert_eq!(raster(&cache, 4, 3, 4), ["010", "101", "101", "101"]);
    assert_eq!(raster(&cache, 9, 3, 4), ["000", "000", "000", "010"]);
    let orange: Vec<_> = cache
        .image()
        .pixels
        .iter()
        .enumerate()
        .filter_map(|(index, &c)| (c == Color32::from_rgb(240, 136, 62)).then_some(index % 84))
        .collect();
    assert!(orange.len() > 1);
    assert!(orange.iter().all(|&x| (14..17).contains(&x)));
}

#[test]
fn block_colors_have_the_source_contrast_in_both_themes() {
    fn luminance(color: Color32) -> f64 {
        let channels = [color.r(), color.g(), color.b()].map(|value| {
            let v = f64::from(value) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
    }
    for (theme, bg, inks) in [
        (
            Theme::Dark,
            Color32::from_rgb(22, 27, 32),
            [
                Color32::from_rgb(95, 116, 133),
                Color32::from_rgb(174, 192, 208),
                Color32::from_rgb(140, 162, 181),
            ],
        ),
        (
            Theme::Light,
            Color32::from_rgb(243, 245, 247),
            [
                Color32::from_rgb(127, 142, 155),
                Color32::from_rgb(66, 95, 119),
                Color32::from_rgb(99, 125, 146),
            ],
        ),
    ] {
        let g = geometry(MinimapMode::Fit, 2, 20, 4);
        let rows = BTreeMap::from([(0, row(0, &[0, 0x41, 0xff], &[]))]);
        let mut cache = MinimapDrawCache::default();
        cache.render(&MinimapScene {
            geometry: &g,
            projection: &g.projection(),
            rows: &rows,
            width_px: 84,
            dpr: 1.0,
            options: MinimapOptions {
                theme,
                render: MinimapRender::Blocks,
                ..options()
            },
        });
        assert!(cache.image().pixels.contains(&bg));
        for color in inks {
            assert!(
                cache.image().pixels.contains(&color),
                "Missing byte-category color {color:?}"
            );
            let (a, b) = (luminance(color), luminance(bg));
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 3.0);
        }
    }
}

#[test]
fn fractional_tile_edge_glyphs_are_clipped_without_losing_either_partial_row() {
    let g = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: 6,
        height_px: 10,
        row_px: 2,
        top_row: 1,
        visible_main_rows: Some(1),
    })
    .unwrap();
    let p = drag_projection(&g, 2.0, 0.0);
    assert_eq!(p.virtual_origin, LogicalY(512));
    let rows = BTreeMap::from([(0, row(0, &[0x41], &[])), (5, row(5, &[0x42], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 2.0,
        options: options(),
    });
    assert_eq!(cache.image().size, [168, 20]);
    let bg = Color32::from_rgb(22, 27, 32);
    let occupied: Vec<_> = cache
        .image()
        .pixels
        .chunks(168)
        .enumerate()
        .filter_map(|(y, pixels)| pixels.iter().any(|&c| c != bg).then_some(y))
        .collect();
    assert!(occupied.iter().any(|&y| y < 3));
    assert!(occupied.contains(&19));
    assert!(occupied.iter().all(|&y| y < 3 || y == 19));
}

#[test]
fn compressed_columns_keep_even_bytes_and_separate_adjacent_glyphs() {
    for byte in [0x42, 0x47] {
        let g = geometry(MinimapMode::Fit, 100, 1, 2);
        let rows = BTreeMap::from([(0, row(0, &vec![byte; 32], &[]))]);
        let mut cache = MinimapDrawCache::default();
        cache.render(&MinimapScene {
            geometry: &g,
            projection: &g.projection(),
            rows: &rows,
            width_px: 84,
            dpr: 1.0,
            options: MinimapOptions {
                row_width: RowWidth::ThirtyTwo,
                ..options()
            },
        });
        let ink: Vec<_> = cache
            .image()
            .pixels
            .iter()
            .enumerate()
            .filter_map(|(x, &c)| (c == Color32::from_rgb(174, 192, 208)).then_some(x))
            .collect();
        // One compact glyph run may be 1 or 2 physical pixels wide. Count
        // separated runs, not pixels, just as the source records draw calls.
        let separated = usize::from(!ink.is_empty())
            + ink.windows(2).filter(|pair| pair[1] > pair[0] + 1).count();
        assert_eq!(separated, 32);
    }
}

#[test]
fn configured_row_width_truncates_extra_bytes_and_marks_the_second_half_of_32() {
    let g = geometry(MinimapMode::Fit, 2, 20, 4);
    let rows = BTreeMap::from([(1, row(1, &vec![0x41; 32], &[48]))]);
    let mut cache = MinimapDrawCache::default();
    for (width, count, edited) in [
        (RowWidth::Sixteen, 16, false),
        (RowWidth::ThirtyTwo, 32, true),
    ] {
        cache.render(&MinimapScene {
            geometry: &g,
            projection: &g.projection(),
            rows: &rows,
            width_px: 84,
            dpr: 1.0,
            options: MinimapOptions {
                row_width: width,
                ..options()
            },
        });
        let occupied: std::collections::BTreeSet<_> = cache
            .image()
            .pixels
            .iter()
            .enumerate()
            .filter_map(|(index, &c)| (c != Color32::from_rgb(22, 27, 32)).then_some(index % 84))
            .collect();
        let occupied: Vec<_> = occupied.into_iter().collect();
        let separated = usize::from(!occupied.is_empty())
            + occupied
                .windows(2)
                .filter(|pair| pair[1] > pair[0] + 1)
                .count();
        assert_eq!(separated, count);
        assert_eq!(
            cache
                .image()
                .pixels
                .contains(&Color32::from_rgb(240, 136, 62)),
            edited
        );
        if edited {
            let orange: Vec<_> = cache
                .image()
                .pixels
                .iter()
                .enumerate()
                .filter_map(|(index, &c)| {
                    (c == Color32::from_rgb(240, 136, 62)).then_some(index % 84)
                })
                .collect();
            assert!(!orange.is_empty());
            assert!(orange.iter().all(|&x| (42..44).contains(&x)));
        }
    }
}

#[test]
fn larger_block_rows_keep_thicker_ink_and_a_blank_separating_row() {
    let g = geometry(MinimapMode::Proportional, 1, 20, 6);
    let rows = BTreeMap::from([(0, row(0, &[0x41], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: MinimapOptions {
            render: MinimapRender::Blocks,
            ..options()
        },
    });
    let raster = raster(&cache, 4, 3, 6);
    assert!(raster.iter().filter(|row| row.contains('1')).count() >= 2);
    assert_eq!(raster.last().unwrap(), "000");
}

#[test]
fn compact_block_rows_keep_separate_ink_and_sixteen_distinct_byte_columns() {
    let g = geometry(MinimapMode::Fit, 2, 4, 2);
    let rows = BTreeMap::from([(0, row(0, &[0x41; 16], &[])), (1, row(1, &[0x42; 16], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &g.projection(),
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: MinimapOptions {
            render: MinimapRender::Blocks,
            ..options()
        },
    });
    assert_eq!(cache.image().size, [84, 4]);
    let bg = Color32::from_rgb(22, 27, 32);
    let occupied: Vec<_> = cache
        .image()
        .pixels
        .chunks(84)
        .enumerate()
        .filter_map(|(y, pixels)| pixels.iter().any(|&c| c != bg).then_some(y))
        .collect();
    // The source compact block is centered within its native two-pixel row.
    assert_eq!(occupied, [1, 3]);
    for y in occupied {
        let pixels = &cache.image().pixels[y * 84..(y + 1) * 84];
        let starts = pixels
            .iter()
            .enumerate()
            .filter(|&(x, c)| *c != bg && (x == 0 || pixels[x - 1] == bg))
            .count();
        assert_eq!(starts, 16);
    }
}

#[test]
fn removed_rows_clear_their_exact_tile_and_theme_recolors_only_bounded_pixels() {
    let g = geometry(MinimapMode::Fit, 2, 20, 4);
    let p = g.projection();
    let mut rows = BTreeMap::from([(0, row(0, &[0x41], &[])), (1, row(1, &[0x42], &[]))]);
    let mut cache = MinimapDrawCache::default();
    cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    rows.remove(&1);
    let removed = cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert!(!removed.full);
    assert_eq!(removed.rects, [[0, 4, 84, 4]]);
    assert!(cache.image().pixels[84 * 4..]
        .iter()
        .all(|&c| c == Color32::from_rgb(22, 27, 32)));
    let equivalent = geometry(MinimapMode::Fit, 2, 20, 4);
    let stable = cache.render(&MinimapScene {
        geometry: &equivalent,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: options(),
    });
    assert!(!stable.full);
    assert!(stable.rects.is_empty());
    let theme = cache.render(&MinimapScene {
        geometry: &g,
        projection: &p,
        rows: &rows,
        width_px: 84,
        dpr: 1.0,
        options: MinimapOptions {
            theme: Theme::Light,
            ..options()
        },
    });
    assert!(theme.full);
    assert_eq!(theme.rects, [[0, 0, 84, 8]]);
    assert_eq!(cache.image().size, [84, 8]);
    assert!(cache
        .image()
        .pixels
        .contains(&Color32::from_rgb(243, 245, 247)));
    assert!(cache
        .image()
        .pixels
        .contains(&Color32::from_rgb(66, 95, 119)));
}

#[test]
fn fractional_display_scales_have_bounded_physical_upload_rectangles() {
    for (dpr, width, height, row_height) in [(1.25, 105, 10, 5), (2.0, 168, 16, 8)] {
        let g = geometry(MinimapMode::Fit, 2, 20, 4);
        let p = g.projection();
        let mut rows = BTreeMap::from([(0, row(0, &[0x41], &[])), (1, row(1, &[0x42], &[]))]);
        let mut cache = MinimapDrawCache::default();
        cache.render(&MinimapScene {
            geometry: &g,
            projection: &p,
            rows: &rows,
            width_px: 84,
            dpr,
            options: options(),
        });
        assert_eq!(cache.image().size, [width, height]);
        rows.insert(1, row(1, &[0x20], &[]));
        let update = cache.render(&MinimapScene {
            geometry: &g,
            projection: &p,
            rows: &rows,
            width_px: 84,
            dpr,
            options: options(),
        });
        assert_eq!(update.rects, [[0, row_height, width, row_height]]);
        assert!(cache.image().pixels[width * row_height..]
            .iter()
            .all(|&c| c == Color32::from_rgb(22, 27, 32)));
    }
}
