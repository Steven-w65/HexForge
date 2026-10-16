//! One bounded, generation-stamped tile. Main viewport reads always win;
//! sparse work is requested by the runtime only after a presented frame.
use super::minimap_geometry::{sample_plan, MinimapGeometry, MinimapProjection};
use crate::{
    controller::{MinimapMode, RowWidth},
    identity::SourceIdentity,
};
use hexforge_core::{
    dto::PageResponse,
    error::{AppError, ErrorCode},
    operations::{CoreReply, CoreRequest},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone)]
pub struct MinimapDataInput {
    pub enabled: bool,
    pub source: SourceIdentity,
    pub source_key: String,
    pub revision: String,
    pub file_size: u64,
    pub row_width: RowWidth,
    pub geometry: MinimapGeometry,
    pub projection: Option<MinimapProjection>,
    pub accepted_page: Option<PageResponse>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimapRow {
    pub row: u64,
    pub bytes: Vec<u8>,
    pub modified_offsets: Vec<u64>,
}
#[derive(Debug, Default)]
pub struct DataUpdate {
    pub changed: Vec<u64>,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapTicket {
    generation: u64,
    serial: u64,
}
struct Pending {
    ticket: MinimapTicket,
    input: MinimapDataInput,
    rows: Vec<u64>,
}
#[derive(Default)]
pub struct MinimapDataManager {
    current: Option<MinimapDataInput>,
    cache: BTreeMap<u64, MinimapRow>,
    generation: u64,
    serial: u64,
    pending: Option<Pending>,
    pending_delta: Option<(u64, String)>,
    failed_generation: Option<u64>,
    disposed: bool,
}
fn width(i: &MinimapDataInput) -> u64 {
    match i.row_width {
        RowWidth::Sixteen => 16,
        RowWidth::ThirtyTwo => 32,
    }
}
fn rows(i: &MinimapDataInput) -> BTreeSet<u64> {
    let p = i
        .projection
        .clone()
        .unwrap_or_else(|| i.geometry.projection());
    sample_plan(&i.geometry, &p)
        .into_iter()
        .map(|r| r.row)
        .collect()
}
fn failure(message: &str) -> AppError {
    AppError::new(ErrorCode::OperationFailed, message, None)
}
impl MinimapDataManager {
    fn advance(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("minimap generation exhausted");
        self.failed_generation = None;
    }
    pub fn get_row(&self, row: u64) -> Option<&MinimapRow> {
        self.cache.get(&row)
    }
    pub fn cached_rows(&self) -> usize {
        self.cache.len()
    }
    fn add_page(&mut self, page: &PageResponse, input: &MinimapDataInput) -> Vec<u64> {
        if page.revision != input.revision {
            return vec![];
        }
        let Ok(start) = page.offset.parse::<u64>() else {
            return vec![];
        };
        let end = u128::from(start) + page.bytes.len() as u128;
        let w = width(input);
        let modified: BTreeSet<_> = page
            .modified_offsets
            .iter()
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();
        let mut changed = vec![];
        for row in rows(input) {
            let offset = u128::from(row) * u128::from(w);
            if offset < u128::from(start) || offset >= u128::from(input.file_size) {
                continue;
            }
            let offset = offset as u64;
            let needed = (input.file_size - offset).min(w);
            if u128::from(offset) + u128::from(needed) > end {
                continue;
            }
            let at = (offset - start) as usize;
            let next = MinimapRow {
                row,
                bytes: page.bytes[at..at + needed as usize].to_vec(),
                modified_offsets: (0..needed)
                    .map(|n| offset + n)
                    .filter(|n| modified.contains(n))
                    .collect(),
            };
            if self.cache.get(&row) != Some(&next) {
                self.cache.insert(row, next);
                changed.push(row);
            }
        }
        changed
    }
    pub fn update(&mut self, input: MinimapDataInput) -> DataUpdate {
        if self.disposed {
            return DataUpdate::default();
        }
        if !input.enabled {
            self.advance();
            self.current = None;
            self.cache.clear();
            return DataUpdate::default();
        }
        let (identity, revision, window) = if let Some(old) = self.current.as_ref() {
            let a = &old.geometry;
            let b = &input.geometry;
            let op = old.projection.clone().unwrap_or_else(|| a.projection());
            let np = input.projection.clone().unwrap_or_else(|| b.projection());
            (
                old.source != input.source
                    || old.source_key != input.source_key
                    || old.row_width != input.row_width
                    || a.mode != b.mode
                    || a.total_rows != b.total_rows
                    || a.height_px != b.height_px
                    || a.row_px != b.row_px,
                old.revision != input.revision,
                op.first_sample_row != np.first_sample_row
                    || op.virtual_origin != np.virtual_origin,
            )
        } else {
            (true, true, true)
        };
        let mut changed = vec![];
        if identity || revision || window {
            self.advance();
            if identity
                || (revision
                    && self
                        .pending_delta
                        .as_ref()
                        .is_none_or(|(_, r)| r != &input.revision))
            {
                self.cache.clear();
            } else if revision {
                if let Some((offset, _)) = self.pending_delta.as_ref() {
                    let row = *offset / width(&input);
                    self.cache.remove(&row);
                    changed.push(row);
                }
            }
        }
        if self
            .pending_delta
            .as_ref()
            .is_some_and(|(_, r)| r == &input.revision)
        {
            self.pending_delta = None;
        }
        let needed = rows(&input);
        self.cache.retain(|r, _| needed.contains(r));
        if let Some(page) = input.accepted_page.as_ref() {
            changed.extend(self.add_page(page, &input));
        }
        self.current = Some(input);
        DataUpdate {
            changed,
            error: None,
        }
    }
    pub fn next_request(&mut self) -> Option<(MinimapTicket, CoreRequest)> {
        if self.disposed
            || self.pending.is_some()
            || self.failed_generation == Some(self.generation)
        {
            return None;
        }
        let i = self.current.as_ref()?;
        if !i.enabled
            || i.accepted_page
                .as_ref()
                .is_none_or(|p| p.revision != i.revision)
        {
            return None;
        }
        let missing: Vec<_> = rows(i)
            .into_iter()
            .filter(|r| !self.cache.contains_key(r))
            .collect();
        if missing.is_empty() {
            return None;
        }
        let (requested, request) = if i.geometry.mode == MinimapMode::Fit {
            let requested: Vec<_> = missing.into_iter().take(32).collect();
            let revision = i.revision.parse::<u64>().ok()?;
            (
                requested.clone(),
                CoreRequest::ReadMinimapSamples {
                    rows: requested,
                    bytes_per_row: width(i) as u8,
                    source_key: i.source_key.clone(),
                    expected_revision: revision,
                },
            )
        } else {
            let offset = u128::from(missing[0]) * u128::from(width(i));
            if offset >= u128::from(i.file_size) {
                return None;
            }
            let length = (i.geometry.visible_mini_rows.saturating_mul(width(i)))
                .clamp(width(i), 65536)
                .min(i.file_size - offset as u64) as u32;
            (
                vec![missing[0]],
                CoreRequest::ReadPage {
                    offset: offset as u64,
                    length,
                },
            )
        };
        self.serial = self
            .serial
            .checked_add(1)
            .expect("minimap ticket exhausted");
        let ticket = MinimapTicket {
            generation: self.generation,
            serial: self.serial,
        };
        self.pending = Some(Pending {
            ticket,
            input: i.clone(),
            rows: requested,
        });
        Some((ticket, request))
    }
    pub fn complete(
        &mut self,
        ticket: MinimapTicket,
        result: Result<CoreReply, AppError>,
    ) -> DataUpdate {
        if self.pending.as_ref().is_none_or(|p| p.ticket != ticket) {
            return DataUpdate::default();
        }
        let pending = self.pending.take().unwrap();
        if self.disposed || ticket.generation != self.generation {
            return DataUpdate::default();
        }
        let mut changed = vec![];
        let result = result.and_then(|reply| match reply {
            CoreReply::MinimapSamples(response)
                if pending.input.geometry.mode == MinimapMode::Fit =>
            {
                if response.revision != pending.input.revision {
                    return Err(failure("The minimap response is no longer current."));
                }
                for s in response.samples {
                    let Ok(row) = s.row.parse::<u64>() else {
                        continue;
                    };
                    if !pending.rows.contains(&row) {
                        continue;
                    }
                    self.cache.insert(
                        row,
                        MinimapRow {
                            row,
                            bytes: s.bytes,
                            modified_offsets: s
                                .modified_offsets
                                .iter()
                                .filter_map(|s| s.parse().ok())
                                .collect(),
                        },
                    );
                    changed.push(row);
                }
                if changed.len() < pending.rows.len() {
                    Err(failure("The minimap sample response was incomplete."))
                } else {
                    Ok(())
                }
            }
            CoreReply::Page(page) if pending.input.geometry.mode == MinimapMode::Proportional => {
                if page.revision != pending.input.revision {
                    return Err(failure("The minimap response is no longer current."));
                }
                changed.extend(self.add_page(&page, &pending.input));
                // A main page may have won this race; cache growth is NOT success.
                if self.cache.contains_key(&pending.rows[0]) {
                    Ok(())
                } else {
                    Err(failure(
                        "The minimap page did not contain the requested row.",
                    ))
                }
            }
            _ => Err(failure("The minimap response was invalid.")),
        });
        let error = result
            .err()
            .filter(|_| self.failed_generation != Some(self.generation));
        if error.is_some() {
            self.failed_generation = Some(self.generation);
        }
        DataUpdate { changed, error }
    }
    pub fn apply_edit_delta(&mut self, offset: u64, revision: &str) -> DataUpdate {
        if self.disposed {
            return DataUpdate::default();
        }
        self.pending_delta = Some((offset, revision.into()));
        if let Some(i) = self.current.as_ref().filter(|i| i.revision == revision) {
            let row = offset / width(i);
            self.cache.remove(&row);
            self.pending_delta = None;
            DataUpdate {
                changed: vec![row],
                error: None,
            }
        } else {
            DataUpdate::default()
        }
    }
    pub fn retry(&mut self) {
        if !self.disposed && self.current.is_some() {
            self.failed_generation = None;
        }
    }
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.advance();
        self.current = None;
        self.cache.clear();
    }
}
