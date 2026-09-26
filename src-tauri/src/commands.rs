use crate::error::{AppError, ErrorCode};
use crate::export;
use crate::search::{self, SearchResult};
use crate::session::{FileInfo, FileSession, PageData};
use crate::template::{self, ParsedField, TemplateDefinition, TemplateFileSession};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{ipc::Channel, State};

const PAGE_SIZE: usize = 64 * 1024;
const MAX_PAGES: usize = 256;
const SEARCH_LIMIT: usize = 100_000;

#[derive(Default)]
pub struct AppState {
    pub session: Arc<Mutex<Option<FileSession>>>,
    pub template_file: Arc<Mutex<TemplateFileSession>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoDto {
    pub name: String,
    pub path: String,
    pub size: String,
    pub revision: String,
    pub dirty: bool,
}

impl From<FileInfo> for FileInfoDto {
    fn from(info: FileInfo) -> Self {
        Self {
            name: info.name,
            path: info.path,
            size: info.size.to_string(),
            revision: info.revision.to_string(),
            dirty: info.dirty,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResponse {
    pub offset: String,
    pub bytes: Vec<u8>,
    pub modified_offsets: Vec<String>,
    pub revision: String,
}

impl From<PageData> for PageResponse {
    fn from(page: PageData) -> Self {
        Self {
            offset: page.offset.to_string(),
            bytes: page.bytes,
            modified_offsets: page
                .modified_offsets
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
            revision: page.revision.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub matches: Vec<String>,
    pub truncated: bool,
}

impl From<SearchResult> for SearchResponse {
    fn from(result: SearchResult) -> Self {
        Self {
            matches: result
                .matches
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
            truncated: result.truncated,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirtyState {
    pub dirty: bool,
    pub revision: String,
}

impl From<&FileSession> for DirtyState {
    fn from(session: &FileSession) -> Self {
        Self {
            dirty: session.is_dirty(),
            revision: session.info().revision.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResponse {
    pub undone: bool,
    #[serde(flatten)]
    pub state: DirtyState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResponse {
    pub bytes_written: String,
    pub destination: String,
    pub file: FileInfoDto,
    #[serde(flatten)]
    pub state: DirtyState,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Search,
    Parse,
    Save,
    Csv,
    Complete,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationProgress {
    pub operation_id: String,
    pub phase: OperationPhase,
    pub processed: String,
    pub total: String,
}

impl OperationProgress {
    fn new(operation_id: String, phase: OperationPhase, processed: u64, total: u64) -> Self {
        Self {
            operation_id,
            phase,
            processed: processed.to_string(),
            total: total.to_string(),
        }
    }
}

// IDs are generated here rather than accepting arbitrary frontend text in progress events.
struct Reporter {
    id: String,
    channel: Channel<OperationProgress>,
}

impl Reporter {
    fn new(channel: Channel<OperationProgress>) -> Self {
        static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_OPERATION.fetch_add(1, Ordering::Relaxed).to_string(),
            channel,
        }
    }

    fn send(&self, phase: OperationPhase, processed: u64, total: u64) {
        // A detached frontend must not abort or invalidate an otherwise safe output operation.
        let _ = self.channel.send(OperationProgress::new(
            self.id.clone(),
            phase,
            processed,
            total,
        ));
    }
}

fn operation_failed() -> AppError {
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
    // Construct first so a failed replacement leaves the old session and edits intact.
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
    // Keep the source session and its edits intact until the new copy is readable.
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

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| operation_failed())?
}

async fn session_operation<T: Send + 'static>(
    state: &AppState,
    operation: impl FnOnce(&mut Option<FileSession>) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let shared = Arc::clone(&state.session);
    blocking(move || with_session_state(&shared, operation)).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn open_file(
    state: State<'_, AppState>,
    path: String,
    discard_unsaved: bool,
) -> Result<FileInfoDto, AppError> {
    session_operation(&state, move |session| {
        open_core(session, path.into(), discard_unsaved)
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn close_file(state: State<'_, AppState>, discard_unsaved: bool) -> Result<(), AppError> {
    session_operation(&state, move |session| close_core(session, discard_unsaved)).await
}

#[tauri::command]
pub async fn get_file_info(state: State<'_, AppState>) -> Result<FileInfoDto, AppError> {
    session_operation(&state, |session| Ok(active_session(session)?.info().into())).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn read_page(
    state: State<'_, AppState>,
    offset: String,
    length: u32,
) -> Result<PageResponse, AppError> {
    let offset = parse_offset_arg(&offset)?;
    session_operation(&state, move |session| {
        Ok(active_session(session)?
            .read_range(offset, u64::from(length))?
            .into())
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn edit_byte(
    state: State<'_, AppState>,
    offset: String,
    value: u8,
) -> Result<DirtyState, AppError> {
    let offset = parse_offset_arg(&offset)?;
    session_operation(&state, move |session| {
        let session = active_session(session)?;
        session.edit_byte(offset, value)?;
        Ok(DirtyState::from(&*session))
    })
    .await
}

#[tauri::command]
pub async fn undo_edit(state: State<'_, AppState>) -> Result<UndoResponse, AppError> {
    session_operation(&state, |session| {
        let session = active_session(session)?;
        let undone = session.undo()?;
        Ok(UndoResponse {
            undone,
            state: DirtyState::from(&*session),
        })
    })
    .await
}

#[tauri::command]
pub async fn get_dirty_state(state: State<'_, AppState>) -> Result<DirtyState, AppError> {
    session_operation(&state, |session| {
        Ok(DirtyState::from(&*active_session(session)?))
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_as(
    state: State<'_, AppState>,
    path: String,
    on_progress: Channel<OperationProgress>,
) -> Result<SaveResponse, AppError> {
    session_operation(&state, move |session| {
        let reporter = Reporter::new(on_progress);
        let total = active_session(session)?.source_size();
        reporter.send(OperationPhase::Save, 0, total);
        let saved = save_as_core(session, PathBuf::from(path), &mut |processed| {
            reporter.send(OperationPhase::Save, processed, total)
        })?;
        reporter.send(OperationPhase::Complete, total, total);
        Ok(saved)
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_bytes(
    state: State<'_, AppState>,
    pattern: String,
    on_progress: Channel<OperationProgress>,
) -> Result<SearchResponse, AppError> {
    session_operation(&state, move |session| {
        let session = active_session(session)?;
        let pattern = search::parse_hex_pattern(&pattern)?;
        let reporter = Reporter::new(on_progress);
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
        Ok(result.into())
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn apply_template(
    state: State<'_, AppState>,
    template: TemplateDefinition,
    on_progress: Channel<OperationProgress>,
) -> Result<Vec<ParsedField>, AppError> {
    session_operation(&state, move |session| {
        let reporter = Reporter::new(on_progress);
        let total = template.fields.len() as u64;
        reporter.send(OperationPhase::Parse, 0, total);
        let fields = template::parse_template_with_progress(
            active_session(session)?,
            &template,
            &mut |processed| reporter.send(OperationPhase::Parse, processed, total),
        )?;
        reporter.send(OperationPhase::Complete, total, total);
        Ok(fields)
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn load_template(
    state: State<'_, AppState>,
    path: String,
) -> Result<TemplateDefinition, AppError> {
    let files = Arc::clone(&state.template_file);
    blocking(move || {
        files
            .lock()
            .map_err(|_| operation_failed())?
            .load(&PathBuf::from(path))
    })
    .await
}

fn active_binary_source(
    shared: &Arc<Mutex<Option<FileSession>>>,
) -> Result<Option<PathBuf>, AppError> {
    let guard = shared.lock().map_err(|_| operation_failed())?;
    Ok(guard
        .as_ref()
        .map(|session| session.source_path().to_path_buf()))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_template(
    state: State<'_, AppState>,
    template: TemplateDefinition,
    overwrite_external: bool,
) -> Result<(), AppError> {
    let files = Arc::clone(&state.template_file);
    let session = Arc::clone(&state.session);
    blocking(move || {
        let source = active_binary_source(&session)?;
        files.lock().map_err(|_| operation_failed())?.save(
            &template,
            overwrite_external,
            source.as_deref(),
        )
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_template_as(
    state: State<'_, AppState>,
    path: String,
    template: TemplateDefinition,
    overwrite: bool,
) -> Result<(), AppError> {
    let files = Arc::clone(&state.template_file);
    let session = Arc::clone(&state.session);
    blocking(move || {
        let source = active_binary_source(&session)?;
        files.lock().map_err(|_| operation_failed())?.save_as(
            &PathBuf::from(path),
            &template,
            overwrite,
            source.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn unload_template_file(state: State<'_, AppState>) -> Result<(), AppError> {
    let files = Arc::clone(&state.template_file);
    blocking(move || {
        files.lock().map_err(|_| operation_failed())?.clear();
        Ok(())
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn export_results_csv(
    state: State<'_, AppState>,
    path: String,
    template: TemplateDefinition,
    on_progress: Channel<OperationProgress>,
) -> Result<(), AppError> {
    session_operation(&state, move |session| {
        let reporter = Reporter::new(on_progress);
        let total = template.fields.len() as u64;
        reporter.send(OperationPhase::Parse, 0, total);
        // Reparse the effective session under the same lock: exported values cannot be stale.
        let fields = template::parse_template_with_progress(
            active_session(session)?,
            &template,
            &mut |processed| reporter.send(OperationPhase::Parse, processed, total),
        )?;
        reporter.send(OperationPhase::Csv, 0, total);
        export::export_csv_create_new_with_progress(
            &PathBuf::from(path),
            &fields,
            &mut |processed| reporter.send(OperationPhase::Csv, processed, total),
        )?;
        reporter.send(OperationPhase::Complete, total, total);
        Ok(())
    })
    .await
}

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
