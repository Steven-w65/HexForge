use hexforge_controller::{
    controller::{MinimapMode, RowWidth},
    hex::{minimap_data::*, minimap_geometry::*},
    identity::SourceIdentity,
};
use hexforge_core::{
    dto::PageResponse,
    error::{AppError, ErrorCode},
    minimap::{MinimapSampleRowDto, MinimapSamplesResponse},
    operations::{CoreReply, CoreRequest},
};
fn input(mode: MinimapMode, top: u64) -> MinimapDataInput {
    MinimapDataInput {
        enabled: true,
        source: SourceIdentity(1),
        source_key: "a.bin".into(),
        revision: "1".into(),
        file_size: 16000,
        row_width: RowWidth::Sixteen,
        geometry: MinimapGeometry::new(MinimapConfig {
            mode,
            total_rows: 1000,
            height_px: 70,
            row_px: 2,
            top_row: top,
            visible_main_rows: None,
        })
        .unwrap(),
        projection: None,
        accepted_page: Some(PageResponse {
            offset: "0".into(),
            bytes: vec![0x41; 16],
            modified_offsets: vec![],
            revision: "1".into(),
        }),
    }
}
fn samples(rows: &[u64], revision: &str) -> CoreReply {
    CoreReply::MinimapSamples(MinimapSamplesResponse {
        revision: revision.into(),
        samples: rows
            .iter()
            .map(|r| MinimapSampleRowDto {
                row: r.to_string(),
                bytes: vec![(*r % 256) as u8],
                modified_offsets: vec![],
            })
            .collect(),
    })
}
#[test]
fn fit_batches_are_sequential_bounded_and_main_page_is_reused() {
    let mut m = MinimapDataManager::default();
    m.update(input(MinimapMode::Fit, 0));
    assert_eq!(m.get_row(0).unwrap().bytes, vec![0x41; 16]);
    let mut calls = 0;
    while let Some((ticket, request)) = m.next_request() {
        assert!(m.next_request().is_none());
        let CoreRequest::ReadMinimapSamples { rows, .. } = request else {
            panic!("fit samples");
        };
        assert!(rows.len() <= 32);
        let u = m.complete(ticket, Ok(samples(&rows, "1")));
        assert!(u.error.is_none());
        calls += 1;
    }
    assert!(calls > 1);
    assert_eq!(m.get_row(999).unwrap().bytes, vec![231]);
    assert!(m.cached_rows() <= 70);
}
#[test]
fn stale_tile_response_is_ignored() {
    let mut m = MinimapDataManager::default();
    m.update(input(MinimapMode::Proportional, 0));
    let (ticket, _) = m.next_request().unwrap();
    m.update(input(MinimapMode::Proportional, 100));
    let u = m.complete(
        ticket,
        Ok(CoreReply::Page(PageResponse {
            offset: "16".into(),
            bytes: vec![0xaa; 560],
            modified_offsets: vec![],
            revision: "1".into(),
        })),
    );
    assert!(u.error.is_none());
    assert!(m.get_row(1).is_none());
    let (_, request) = m.next_request().unwrap();
    assert!(matches!(request,CoreRequest::ReadPage{offset,..}if offset>=1600));
}
#[test]
fn accepted_main_page_winning_the_race_is_not_a_missing_row_error() {
    let mut m = MinimapDataManager::default();
    let first = input(MinimapMode::Proportional, 0);
    m.update(first.clone());
    let (ticket, _) = m.next_request().unwrap();
    let mut filled = first;
    filled.accepted_page.as_mut().unwrap().bytes = vec![0x42; 560];
    m.update(filled);
    let u = m.complete(
        ticket,
        Ok(CoreReply::Page(PageResponse {
            offset: "16".into(),
            bytes: vec![0x42; 544],
            modified_offsets: vec![],
            revision: "1".into(),
        })),
    );
    assert!(u.error.is_none());
    assert_eq!(m.get_row(34).unwrap().bytes, vec![0x42; 16]);
}
#[test]
fn known_edit_retains_other_rows_and_unknown_revision_clears_them() {
    let mut m = MinimapDataManager::default();
    let mut i = input(MinimapMode::Proportional, 0);
    i.accepted_page.as_mut().unwrap().bytes = vec![0x41; 560];
    m.update(i.clone());
    let unaffected = m.get_row(1).unwrap().clone();
    m.apply_edit_delta(32, "2");
    i.revision = "2".into();
    i.accepted_page = None;
    m.update(i.clone());
    assert_eq!(*m.get_row(1).unwrap(), unaffected);
    assert!(m.get_row(2).is_none());
    i.revision = "3".into();
    m.update(i);
    assert_eq!(m.cached_rows(), 0);
}
#[test]
fn current_failure_is_reported_once_and_explicit_retry_releases_it() {
    let mut m = MinimapDataManager::default();
    m.update(input(MinimapMode::Fit, 0));
    let (ticket, _) = m.next_request().unwrap();
    let u = m.complete(
        ticket,
        Err(AppError::new(
            ErrorCode::SourceChanged,
            "Source changed.",
            None,
        )),
    );
    assert_eq!(u.error.unwrap().code(), "source_changed");
    assert!(m.next_request().is_none());
    m.update(input(MinimapMode::Fit, 0));
    assert!(m.next_request().is_none());
    m.retry();
    assert!(m.next_request().is_some());
}
#[test]
fn stale_revision_page_gates_work_and_disposal_invalidates_pending_work() {
    let mut m = MinimapDataManager::default();
    let mut i = input(MinimapMode::Fit, 0);
    i.revision = "2".into();
    m.update(i);
    assert!(m.next_request().is_none());
    m.update(input(MinimapMode::Fit, 0));
    let (ticket, request) = m.next_request().unwrap();
    let CoreRequest::ReadMinimapSamples { rows, .. } = request else {
        panic!()
    };
    m.dispose();
    assert!(m.complete(ticket, Ok(samples(&rows, "1"))).error.is_none());
    assert_eq!(m.cached_rows(), 0);
    assert!(m.next_request().is_none());
}

#[test]
fn old_fit_identity_cannot_restore_samples_over_the_new_accepted_page() {
    let mut m = MinimapDataManager::default();
    m.update(input(MinimapMode::Fit, 0));
    let (old, request) = m.next_request().unwrap();
    let CoreRequest::ReadMinimapSamples { rows, .. } = request else {
        panic!("Fit samples");
    };
    let mut replacement = input(MinimapMode::Fit, 0);
    replacement.source = SourceIdentity(2);
    replacement.source_key = "b.bin".into();
    replacement.revision = "2".into();
    let page = replacement.accepted_page.as_mut().unwrap();
    page.revision = "2".into();
    page.bytes = vec![0x42; 16];
    m.update(replacement);
    assert!(m.complete(old, Ok(samples(&rows, "1"))).error.is_none());
    assert_eq!(m.get_row(0).unwrap().bytes, vec![0x42; 16]);
    assert!(rows.iter().all(|row| m.get_row(*row).is_none()));
    let (fresh, request) = m.next_request().unwrap();
    let CoreRequest::ReadMinimapSamples {
        rows,
        source_key,
        expected_revision,
        ..
    } = request
    else {
        panic!("Fit samples");
    };
    assert_eq!(source_key, "b.bin");
    assert_eq!(expected_revision, 2);
    let reply = CoreReply::MinimapSamples(MinimapSamplesResponse {
        revision: "2".into(),
        samples: rows
            .iter()
            .map(|r| MinimapSampleRowDto {
                row: r.to_string(),
                bytes: vec![0xbb],
                modified_offsets: vec![],
            })
            .collect(),
    });
    assert!(!m.complete(fresh, Ok(reply)).changed.is_empty());
    assert_eq!(m.get_row(rows[0]).unwrap().bytes, [0xbb]);
}

#[test]
fn proportional_page_reuse_keeps_overlapping_rows_without_redundant_reads() {
    let mut m = MinimapDataManager::default();
    let mut first = input(MinimapMode::Proportional, 0);
    first.geometry = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: 1000,
        height_px: 8,
        row_px: 1,
        top_row: 0,
        visible_main_rows: None,
    })
    .unwrap();
    first.accepted_page.as_mut().unwrap().bytes = vec![0x42; 16 * 8];
    m.update(first.clone());
    assert_eq!(m.get_row(7).unwrap().bytes, vec![0x42; 16]);
    assert!(m.next_request().is_none());
    let overlap = m.get_row(1).unwrap().bytes.as_ptr();
    first.geometry.main_top_row = 1;
    assert!(m.update(first.clone()).changed.is_empty());
    assert_eq!(
        m.get_row(1).unwrap().bytes.as_ptr(),
        overlap,
        "Unchanged overlap retains its cached allocation"
    );
    let (tail, request) = m.next_request().unwrap();
    assert!(matches!(
        request,
        CoreRequest::ReadPage {
            offset: 128,
            length: 128
        }
    ));
    m.complete(
        tail,
        Ok(CoreReply::Page(PageResponse {
            offset: "128".into(),
            bytes: vec![0x43; 128],
            modified_offsets: vec![],
            revision: "1".into(),
        })),
    );
    assert_eq!(m.get_row(8).unwrap().bytes, vec![0x43; 16]);
    first.geometry.main_top_row = 100;
    m.update(first);
    let (later, request) = m.next_request().unwrap();
    assert!(matches!(
        request,
        CoreRequest::ReadPage {
            offset: 1600,
            length: 128
        }
    ));
    assert!(m
        .complete(
            later,
            Ok(CoreReply::Page(PageResponse {
                offset: "1600".into(),
                bytes: vec![0x43; 128],
                modified_offsets: vec![],
                revision: "1".into(),
            }))
        )
        .error
        .is_none());
    assert_eq!(m.get_row(100).unwrap().bytes, vec![0x43; 16]);
    assert!(m.next_request().is_none());
}

#[test]
fn fractional_proportional_origin_rejects_stale_pages_even_when_the_first_row_matches() {
    let mut m = MinimapDataManager::default();
    let mut first = input(MinimapMode::Proportional, 495);
    first.geometry = MinimapGeometry::new(MinimapConfig {
        mode: MinimapMode::Proportional,
        total_rows: 1000,
        height_px: 101,
        row_px: 2,
        top_row: 495,
        visible_main_rows: Some(10),
    })
    .unwrap();
    let before = drag_projection(&first.geometry, 40.1, 0.0);
    let after = drag_projection(&first.geometry, 40.11, 0.0);
    assert_eq!(before.first_sample_row, after.first_sample_row);
    assert!(after.virtual_origin > before.virtual_origin);
    first.projection = Some(before.clone());
    m.update(first.clone());
    let (old, _) = m.next_request().unwrap();
    first.projection = Some(after.clone());
    m.update(first);
    assert!(m
        .complete(
            old,
            Ok(CoreReply::Page(PageResponse {
                offset: (before.first_sample_row * 16).to_string(),
                bytes: vec![0xaa; 16 * 52],
                modified_offsets: vec![],
                revision: "1".into(),
            }))
        )
        .error
        .is_none());
    assert!(m.get_row(before.first_sample_row).is_none());
    let (fresh, _) = m.next_request().unwrap();
    assert!(m
        .complete(
            fresh,
            Ok(CoreReply::Page(PageResponse {
                offset: (after.first_sample_row * 16).to_string(),
                bytes: vec![0xbb; 16 * 52],
                modified_offsets: vec![],
                revision: "1".into(),
            }))
        )
        .error
        .is_none());
    assert_eq!(
        m.get_row(after.first_sample_row).unwrap().bytes,
        vec![0xbb; 16]
    );
}
