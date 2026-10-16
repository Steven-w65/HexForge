//! Shared operation boundary. Native effects and retained Tauri commands call
//! the same guarded algorithms, so the presentation cannot bypass file safety.
use crate::dto::*;
use crate::error::{AppError, ErrorCode};
use crate::export;
use crate::minimap::{MinimapSampleRowDto, MinimapSamplesResponse};
use crate::progress::{OperationPhase, OperationProgress, Reporter};
use crate::search;
#[cfg(test)]
use crate::search::SearchResult;
#[cfg(test)]
use crate::session::{FileInfo, PageData};
use crate::session::{FileSession, OVERVIEW_BIN_COUNT};
use crate::template::{self, ParsedNode, TemplateDefinition};
use crate::template_file::TemplateFileSession;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const PAGE_SIZE: usize = 64 * 1024;
const MAX_PAGES: usize = 256;
const SEARCH_LIMIT: usize = 100_000;

#[derive(Default, Clone)]
pub struct CoreService {
    pub session: Arc<Mutex<Option<FileSession>>>,
    pub template_file: Arc<Mutex<TemplateFileSession>>,
}

#[derive(Debug, Clone)]
pub enum CoreRequest {
    OpenFile {
        path: PathBuf,
        discard_unsaved: bool,
    },
    CloseFile {
        discard_unsaved: bool,
    },
    GetFileInfo,
    ReadPage {
        offset: u64,
        length: u32,
    },
    ReadMinimapSamples {
        rows: Vec<u64>,
        bytes_per_row: u8,
        source_key: String,
        expected_revision: u64,
    },
    EditByte {
        offset: u64,
        value: u8,
    },
    UndoEdit,
    GetDirtyState,
    GetModifiedOverview,
    SaveBinaryAs {
        path: PathBuf,
    },
    SearchBytes {
        pattern: String,
    },
    ApplyTemplate {
        template: TemplateDefinition,
    },
    LoadTemplate {
        path: PathBuf,
    },
    SaveTemplate {
        template: TemplateDefinition,
        overwrite_external: bool,
    },
    SaveTemplateAs {
        path: PathBuf,
        template: TemplateDefinition,
        overwrite: bool,
    },
    UnloadTemplate,
    ExportResultsCsv {
        path: PathBuf,
        template: TemplateDefinition,
    },
}

#[derive(Debug)]
pub enum CoreReply {
    Unit,
    FileInfo(FileInfoDto),
    Page(PageResponse),
    MinimapSamples(MinimapSamplesResponse),
    Dirty(DirtyState),
    Undo(UndoResponse),
    ModifiedOverview(ModifiedOverviewDto),
    BinarySaved(SaveResponse),
    Search(SearchResponse),
    Parsed(Vec<ParsedNode>),
    Template(TemplateDefinition),
}

pub fn operation_failed() -> AppError {
    AppError::new(
        ErrorCode::OperationFailed,
        "The operation could not be completed.",
        None,
    )
}

pub fn parse_offset_arg(value: &str) -> Result<u64, AppError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::new(
            ErrorCode::InvalidOffset,
            "Offsets must be unsigned decimal integers.",
            None,
        ));
    }
    value.parse().map_err(|_| {
        AppError::new(
            ErrorCode::InvalidOffset,
            "The offset exceeds the supported range.",
            None,
        )
    })
}

fn guard_discard(dirty: bool, discard_unsaved: bool) -> Result<(), AppError> {
    if dirty && !discard_unsaved {
        return Err(AppError::new(
            ErrorCode::UnsavedChanges,
            "The current file has unsaved changes.",
            None,
        ));
    }
    Ok(())
}

fn active_session(session: &mut Option<FileSession>) -> Result<&mut FileSession, AppError> {
    session
        .as_mut()
        .ok_or_else(|| AppError::new(ErrorCode::NoActiveFile, "No file is open.", None))
}

fn open_core(
    session: &mut Option<FileSession>,
    path: PathBuf,
    discard_unsaved: bool,
) -> Result<FileInfoDto, AppError> {
    guard_discard(
        session.as_ref().is_some_and(FileSession::is_dirty),
        discard_unsaved,
    )?;
    let replacement = FileSession::open(path, PAGE_SIZE, MAX_PAGES)?;
    let info = replacement.info().into();
    *session = Some(replacement);
    Ok(info)
}

fn close_core(session: &mut Option<FileSession>, discard_unsaved: bool) -> Result<(), AppError> {
    guard_discard(active_session(session)?.is_dirty(), discard_unsaved)?;
    *session = None;
    Ok(())
}

fn save_as_core(
    session: &mut Option<FileSession>,
    destination: PathBuf,
    progress: &mut dyn FnMut(u64),
) -> Result<SaveResponse, AppError> {
    let summary = export::save_session_as_with_progress(
        active_session(session)?,
        &destination,
        PAGE_SIZE,
        progress,
    )?;
    let replacement = FileSession::open(summary.destination.clone(), PAGE_SIZE, MAX_PAGES)?;
    let file = replacement.info().into();
    let state = DirtyState::from(&replacement);
    *session = Some(replacement);
    Ok(SaveResponse {
        bytes_written: summary.bytes_written.to_string(),
        destination: summary.destination.to_string_lossy().into_owned(),
        file,
        state,
    })
}

fn with_session_state<T>(
    shared: &Arc<Mutex<Option<FileSession>>>,
    operation: impl FnOnce(&mut Option<FileSession>) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let mut session = shared.lock().map_err(|_| operation_failed())?;
    operation(&mut session)
}

impl CoreService {
    pub fn execute(
        &self,
        request: CoreRequest,
        report: &mut dyn FnMut(OperationProgress),
    ) -> Result<CoreReply, AppError> {
        // Persistence keeps the original separate locks and ordering. The
        // controller serializes mutations; this boundary does not redesign them.
        match request {
            CoreRequest::LoadTemplate { path } => self
                .template_file
                .lock()
                .map_err(|_| operation_failed())?
                .load(&path)
                .map(CoreReply::Template),
            CoreRequest::SaveTemplate {
                template,
                overwrite_external,
            } => {
                let source = self.binary_source()?;
                self.template_file
                    .lock()
                    .map_err(|_| operation_failed())?
                    .save(&template, overwrite_external, source.as_deref())?;
                Ok(CoreReply::Unit)
            }
            CoreRequest::SaveTemplateAs {
                path,
                template,
                overwrite,
            } => {
                let source = self.binary_source()?;
                self.template_file
                    .lock()
                    .map_err(|_| operation_failed())?
                    .save_as(&path, &template, overwrite, source.as_deref())?;
                Ok(CoreReply::Unit)
            }
            CoreRequest::UnloadTemplate => {
                self.template_file
                    .lock()
                    .map_err(|_| operation_failed())?
                    .clear();
                Ok(CoreReply::Unit)
            }
            request => with_session_state(&self.session, |state| match request {
                CoreRequest::OpenFile {
                    path,
                    discard_unsaved,
                } => open_core(state, path, discard_unsaved).map(CoreReply::FileInfo),
                CoreRequest::CloseFile { discard_unsaved } => {
                    close_core(state, discard_unsaved)?;
                    Ok(CoreReply::Unit)
                }
                CoreRequest::GetFileInfo => {
                    Ok(CoreReply::FileInfo(active_session(state)?.info().into()))
                }
                CoreRequest::ReadPage { offset, length } => Ok(CoreReply::Page(
                    active_session(state)?
                        .read_range(offset, u64::from(length))?
                        .into(),
                )),
                CoreRequest::ReadMinimapSamples {
                    rows,
                    bytes_per_row,
                    source_key,
                    expected_revision,
                } => {
                    let session = active_session(state)?;
                    let pages = session.sample_rows(
                        &rows,
                        bytes_per_row,
                        &source_key,
                        expected_revision,
                    )?;
                    let samples = rows
                        .into_iter()
                        .zip(pages)
                        .map(|(row, page)| MinimapSampleRowDto::from_page(row, page))
                        .collect();
                    Ok(CoreReply::MinimapSamples(MinimapSamplesResponse {
                        revision: session.info().revision.to_string(),
                        samples,
                    }))
                }
                CoreRequest::EditByte { offset, value } => {
                    let session = active_session(state)?;
                    session.edit_byte(offset, value)?;
                    Ok(CoreReply::Dirty(DirtyState::from(&*session)))
                }
                CoreRequest::UndoEdit => {
                    let session = active_session(state)?;
                    let offset = session.undo_with_offset()?;
                    Ok(CoreReply::Undo(UndoResponse {
                        undone: offset.is_some(),
                        offset: offset.map(|value| value.to_string()),
                        state: DirtyState::from(&*session),
                    }))
                }
                CoreRequest::GetDirtyState => {
                    Ok(CoreReply::Dirty(DirtyState::from(&*active_session(state)?)))
                }
                CoreRequest::GetModifiedOverview => {
                    Ok(CoreReply::ModifiedOverview(ModifiedOverviewDto {
                        bin_count: OVERVIEW_BIN_COUNT,
                        bins: active_session(state)?.modified_overview_bins(),
                    }))
                }
                CoreRequest::SaveBinaryAs { path } => {
                    let mut reporter = Reporter::new(report);
                    let total = active_session(state)?.source_size();
                    reporter.send(OperationPhase::Save, 0, total);
                    let saved = save_as_core(state, path, &mut |processed| {
                        reporter.send(OperationPhase::Save, processed, total)
                    })?;
                    reporter.send(OperationPhase::Complete, total, total);
                    Ok(CoreReply::BinarySaved(saved))
                }
                CoreRequest::SearchBytes { pattern } => {
                    let session = active_session(state)?;
                    let pattern = search::parse_hex_pattern(&pattern)?;
                    let mut reporter = Reporter::new(report);
                    let total = session.source_size();
                    let mut processed = 0;
                    reporter.send(OperationPhase::Search, 0, total);
                    let result = search::search_session_with_progress(
                        session,
                        &pattern,
                        PAGE_SIZE,
                        SEARCH_LIMIT,
                        &mut |value| {
                            processed = value;
                            reporter.send(OperationPhase::Search, value, total);
                        },
                    )?;
                    reporter.send(OperationPhase::Complete, processed, total);
                    Ok(CoreReply::Search(result.into()))
                }
                CoreRequest::ApplyTemplate { template } => {
                    let mut reporter = Reporter::new(report);
                    let mut processed = 0;
                    reporter.send(OperationPhase::Parse, 0, 0);
                    let fields = template::parse_template_with_progress(
                        active_session(state)?,
                        &template,
                        &mut |value| {
                            processed = value;
                            reporter.send(OperationPhase::Parse, value, 0);
                        },
                    )?;
                    reporter.send(OperationPhase::Complete, processed, processed);
                    Ok(CoreReply::Parsed(fields))
                }
                CoreRequest::ExportResultsCsv { path, template } => {
                    let mut reporter = Reporter::new(report);
                    reporter.send(OperationPhase::Parse, 0, 0);
                    let fields = template::parse_template_with_progress(
                        active_session(state)?,
                        &template,
                        &mut |processed| reporter.send(OperationPhase::Parse, processed, 0),
                    )?;
                    fn leaf_count(nodes: &[ParsedNode]) -> u64 {
                        nodes
                            .iter()
                            .map(|node| u64::from(node.kind == "leaf") + leaf_count(&node.children))
                            .sum()
                    }
                    let total = leaf_count(&fields);
                    reporter.send(OperationPhase::Csv, 0, total);
                    export::export_results_csv_with_progress(&path, &fields, &mut |processed| {
                        reporter.send(OperationPhase::Csv, processed, total)
                    })?;
                    reporter.send(OperationPhase::Complete, total, total);
                    Ok(CoreReply::Unit)
                }
                _ => unreachable!("persistence requests dispatched before taking session lock"),
            }),
        }
    }

    fn binary_source(&self) -> Result<Option<PathBuf>, AppError> {
        Ok(self
            .session
            .lock()
            .map_err(|_| operation_failed())?
            .as_ref()
            .map(|session| session.source_path().to_path_buf()))
    }
}

#[cfg(test)]
type AppState = CoreService;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_argument_accepts_u64_decimal_and_rejects_invalid_text() {
        assert_eq!(parse_offset_arg("18446744073709551615").unwrap(), u64::MAX);
        assert_eq!(parse_offset_arg("0").unwrap(), 0);
        for value in [
            "-1",
            "",
            " ",
            " 1",
            "1 ",
            "+1",
            "0xff",
            "1.0",
            "18446744073709551616",
        ] {
            assert_eq!(
                parse_offset_arg(value).unwrap_err().code(),
                "invalid_offset"
            );
        }
    }

    #[test]
    fn dto_conversions_preserve_full_width_decimal_values() {
        let info = FileInfoDto::from(FileInfo {
            name: "x".into(),
            path: "x".into(),
            size: u64::MAX,
            revision: 9007199254740993,
            dirty: true,
        });
        let json = serde_json::to_value(info).unwrap();
        assert_eq!(json["size"], "18446744073709551615");
        assert_eq!(json["revision"], "9007199254740993");
        let page = PageResponse::from(PageData {
            offset: u64::MAX,
            bytes: vec![1],
            modified_offsets: vec![9007199254740993],
            revision: 2,
        });
        let json = serde_json::to_value(page).unwrap();
        assert_eq!(json["offset"], "18446744073709551615");
        assert_eq!(
            json["modifiedOffsets"],
            serde_json::json!(["9007199254740993"])
        );
        assert_eq!(json["revision"], "2");
        let result = SearchResponse::from(SearchResult {
            matches: vec![u64::MAX],
            truncated: true,
        });
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({"matches":["18446744073709551615"],"truncated":true})
        );
    }

    #[test]
    fn undo_response_includes_only_an_actual_edit_offset() {
        let undone = serde_json::to_value(UndoResponse {
            undone: true,
            offset: Some(u64::MAX.to_string()),
            state: DirtyState {
                dirty: false,
                revision: "2".into(),
            },
        })
        .unwrap();
        assert_eq!(undone["offset"], "18446744073709551615");
        let no_op = serde_json::to_value(UndoResponse {
            undone: false,
            offset: None,
            state: DirtyState {
                dirty: false,
                revision: "2".into(),
            },
        })
        .unwrap();
        assert!(no_op.get("offset").is_none());
    }

    #[test]
    fn dirty_guard_requires_explicit_discard() {
        assert!(guard_discard(false, false).is_ok());
        assert!(guard_discard(false, true).is_ok());
        assert!(guard_discard(true, true).is_ok());
        assert_eq!(
            guard_discard(true, false).unwrap_err().code(),
            "unsaved_changes"
        );
    }

    #[test]
    fn missing_session_has_structured_error() {
        assert_eq!(
            active_session(&mut None).unwrap_err().code(),
            "no_active_file"
        );
    }

    #[test]
    fn operation_progress_serializes_only_bounded_metadata() {
        let progress = OperationProgress::new("7".into(), OperationPhase::Search, 9, 10);
        assert_eq!(
            serde_json::to_value(progress).unwrap(),
            serde_json::json!({"operationId":"7","phase":"search","processed":"9","total":"10"})
        );
    }

    #[test]
    fn replacement_and_close_preserve_dirty_session_until_discarded() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [1, 2]).unwrap();
        let mut current = None;
        open_core(&mut current, file.path().to_path_buf(), false).unwrap();
        active_session(&mut current)
            .unwrap()
            .edit_byte(0, 9)
            .unwrap();
        assert_eq!(
            open_core(&mut current, file.path().to_path_buf(), false)
                .unwrap_err()
                .code(),
            "unsaved_changes"
        );
        assert_eq!(
            close_core(&mut current, false).unwrap_err().code(),
            "unsaved_changes"
        );
        assert_eq!(
            active_session(&mut current)
                .unwrap()
                .read_range(0, 1)
                .unwrap()
                .bytes,
            [9]
        );
        assert!(open_core(&mut current, file.path().with_extension("missing"), true).is_err());
        assert!(active_session(&mut current).unwrap().is_dirty());
        open_core(&mut current, file.path().to_path_buf(), true).unwrap();
        assert!(!active_session(&mut current).unwrap().is_dirty());
        close_core(&mut current, false).unwrap();
        assert!(current.is_none());
        assert_eq!(
            close_core(&mut current, false).unwrap_err().code(),
            "no_active_file"
        );
    }

    #[test]
    fn save_as_switches_to_the_new_read_only_copy_without_touching_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.bin");
        let copy = dir.path().join("copy.bin");
        std::fs::write(&source, [1, 2, 3]).unwrap();
        let mut current = None;
        open_core(&mut current, source.clone(), false).unwrap();
        active_session(&mut current)
            .unwrap()
            .edit_byte(1, 9)
            .unwrap();

        let saved = save_as_core(&mut current, copy.clone(), &mut |_| {}).unwrap();

        assert_eq!(saved.bytes_written, "3");
        assert_eq!(
            saved.file.path,
            std::fs::canonicalize(&copy).unwrap().to_string_lossy()
        );
        assert_eq!(
            active_session(&mut current)
                .unwrap()
                .read_range(0, 3)
                .unwrap()
                .bytes,
            [1, 9, 3]
        );
        assert!(!active_session(&mut current).unwrap().is_dirty());
        assert_eq!(std::fs::read(&source).unwrap(), [1, 2, 3]);
    }

    #[test]
    fn poisoned_state_returns_operation_failed() {
        let state = AppState::default();
        let shared = state.session.clone();
        let _ = std::thread::spawn(move || {
            let _guard = shared.lock().unwrap();
            panic!("poison fixture");
        })
        .join();
        assert_eq!(
            with_session_state(&state.session, |_| Ok(()))
                .unwrap_err()
                .code(),
            "operation_failed"
        );
    }
}
