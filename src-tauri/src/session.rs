use crate::edit_buffer::EditBuffer;
use crate::error::{AppError, ErrorCode};
use crate::page_cache::PageCache;
use same_file::Handle;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const MAX_READ_RANGE: u64 = 1_048_576;
pub const OVERVIEW_BIN_COUNT: u32 = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub revision: u64,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageData {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub modified_offsets: Vec<u64>,
    pub revision: u64,
}

#[derive(Debug)]
pub struct FileSession {
    file: File,
    info: FileInfo,
    source_modified: Option<SystemTime>,
    source_identity: Handle,
    cache: PageCache,
    edits: EditBuffer,
    #[cfg(test)]
    read_count: usize,
}

impl FileSession {
    pub fn open(path: PathBuf, page_size: usize, max_pages: usize) -> Result<Self, AppError> {
        if page_size == 0 || max_pages == 0 || page_size as u64 > MAX_READ_RANGE {
            return Err(AppError::new(
                ErrorCode::InvalidLength,
                "The page cache settings are invalid.",
                None,
            ));
        }

        let canonical_path =
            std::fs::canonicalize(&path).map_err(|error| AppError::from_io(error, Some(&path)))?;
        let file = OpenOptions::new()
            .read(true)
            .open(&canonical_path)
            .map_err(|error| AppError::from_io(error, Some(&canonical_path)))?;
        let metadata = file
            .metadata()
            .map_err(|error| AppError::from_io(error, Some(&canonical_path)))?;
        if !metadata.is_file() {
            return Err(AppError::new(
                ErrorCode::InvalidPath,
                "The selected path is not a file.",
                Some(canonical_path.to_string_lossy().into_owned()),
            ));
        }

        let name = canonical_path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::InvalidPath,
                    "The selected path does not name a file.",
                    Some(canonical_path.to_string_lossy().into_owned()),
                )
            })?;

        let identity_file = file
            .try_clone()
            .map_err(|error| AppError::from_io(error, Some(&canonical_path)))?;
        let source_identity = Handle::from_file(identity_file)
            .map_err(|error| AppError::from_io(error, Some(&canonical_path)))?;

        Ok(Self {
            file,
            info: FileInfo {
                name,
                path: canonical_path.to_string_lossy().into_owned(),
                size: metadata.len(),
                revision: 0,
                dirty: false,
            },
            source_modified: metadata.modified().ok(),
            source_identity,
            cache: PageCache::new(page_size, max_pages),
            edits: EditBuffer::default(),
            #[cfg(test)]
            read_count: 0,
        })
    }

    pub fn info(&self) -> FileInfo {
        let mut info = self.info.clone();
        info.dirty = self.is_dirty();
        info
    }

    /// Returns the canonical, read-only source path for operations that create a separate output.
    pub fn source_path(&self) -> &Path {
        Path::new(&self.info.path)
    }

    /// Returns the source size captured when the read-only session was opened.
    pub fn source_size(&self) -> u64 {
        self.info.size
    }

    /// Streams the effective source through a caller-owned output without exposing source writes.
    pub(crate) fn copy_effective_into<W: Write>(
        &mut self,
        output: &mut W,
        chunk_size: usize,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, AppError> {
        self.ensure_source_unchanged()?;
        let written = crate::export::copy_effective(
            &mut self.file,
            output,
            self.info.size,
            chunk_size,
            &self.edits,
            progress,
        )?;
        self.ensure_source_unchanged()?;
        Ok(written)
    }

    pub fn read_range(&mut self, start: u64, len: u64) -> Result<PageData, AppError> {
        self.ensure_source_unchanged()?;
        if len > MAX_READ_RANGE {
            return Err(invalid_length("The requested range is too large."));
        }
        if start > self.info.size {
            return Err(invalid_offset("The requested offset is outside the file."));
        }
        let requested_end = start
            .checked_add(len)
            .ok_or_else(|| invalid_length("The requested range is invalid."))?;
        if len == 0 || start == self.info.size {
            return Ok(self.page_data(start, Vec::new()));
        }

        let end = requested_end.min(self.info.size);
        let mut bytes = self.read_source_range(start, end)?;
        self.ensure_source_unchanged()?;
        self.edits.overlay(start, &mut bytes);
        Ok(self.page_data(start, bytes))
    }

    /// Reads at most `buffer.len()` effective bytes without loading data beyond the buffer.
    pub fn read_effective_chunk(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, AppError> {
        self.ensure_source_unchanged()?;
        #[cfg(test)]
        {
            self.read_count += 1;
        }
        if buffer.len() as u64 > MAX_READ_RANGE {
            return Err(invalid_length("The requested chunk is too large."));
        }
        if offset > self.info.size {
            return Err(invalid_offset("The requested offset is outside the file."));
        }
        if buffer.is_empty() || offset == self.info.size {
            return Ok(0);
        }

        let len = (self.info.size - offset).min(buffer.len() as u64) as usize;
        let path = Path::new(&self.info.path);
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|error| AppError::from_io(error, Some(path)))?;
        self.file
            .read_exact(&mut buffer[..len])
            .map_err(|error| AppError::from_io(error, Some(path)))?;
        self.ensure_source_unchanged()?;
        self.edits.overlay(offset, &mut buffer[..len]);
        Ok(len)
    }

    #[cfg(test)]
    pub(crate) fn test_read_count(&self) -> usize {
        self.read_count
    }

    pub fn edit_byte(&mut self, offset: u64, value: u8) -> Result<(), AppError> {
        self.ensure_source_unchanged()?;
        if offset >= self.info.size {
            return Err(invalid_offset("The edit offset is outside the file."));
        }
        let source = self.source_byte(offset)?;
        self.ensure_source_unchanged()?;
        if self.edits.apply(offset, source, value) {
            self.info.revision = self.info.revision.saturating_add(1);
        }
        Ok(())
    }

    /// Reverts the most recent effective edit. Returns false when there is no edit to undo.
    pub fn undo(&mut self) -> Result<bool, AppError> {
        self.ensure_source_unchanged()?;
        let Some(offset) = self.edits.last_offset() else {
            return Ok(false);
        };
        let source = self.source_byte(offset)?;
        self.ensure_source_unchanged()?;
        if self.edits.undo(source).is_some() {
            self.info.revision = self.info.revision.saturating_add(1);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn clear_edits(&mut self) {
        if self.edits.clear() {
            self.info.revision = self.info.revision.saturating_add(1);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.edits.is_dirty()
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Returns the exact number of source bytes currently retained by the page cache.
    pub fn cached_bytes(&self) -> usize {
        self.cache.cached_bytes()
    }

    pub fn modified_overview_bins(&self) -> Vec<u32> {
        self.edits
            .modified_overview_bins(self.info.size, OVERVIEW_BIN_COUNT)
    }

    fn page_data(&self, offset: u64, bytes: Vec<u8>) -> PageData {
        let modified_offsets = self.edits.modified_offsets(offset, bytes.len() as u64);
        PageData {
            offset,
            bytes,
            modified_offsets,
            revision: self.info.revision,
        }
    }

    pub(crate) fn ensure_source_unchanged(&self) -> Result<(), AppError> {
        let path = self.source_path();
        let changed = || {
            AppError::new(
            ErrorCode::SourceChanged,
            "The source file changed outside HexForge. Reopen it to continue; in-memory edits are still retained.",
            Some(path.to_string_lossy().into_owned()),
        )
        };
        let current = self.file.metadata().map_err(|_| changed())?;
        let at_path = std::fs::metadata(path).map_err(|_| changed())?;
        let path_identity = Handle::from_path(path).map_err(|_| changed())?;
        if path_identity != self.source_identity {
            return Err(changed());
        }
        if [current, at_path].iter().any(|metadata| {
            metadata.len() != self.info.size || metadata.modified().ok() != self.source_modified
        }) {
            return Err(changed());
        }
        Ok(())
    }

    fn source_byte(&mut self, offset: u64) -> Result<u8, AppError> {
        let end = offset
            .checked_add(1)
            .ok_or_else(|| invalid_length("The requested range is invalid."))?;
        let bytes = self.read_source_range(offset, end)?;
        bytes.first().copied().ok_or_else(|| {
            AppError::new(
                ErrorCode::OperationFailed,
                "The source byte could not be read.",
                None,
            )
        })
    }

    fn read_source_range(&mut self, start: u64, end: u64) -> Result<Vec<u8>, AppError> {
        let capacity = (end - start) as usize;
        let mut result = Vec::with_capacity(capacity);
        let page_size = self.cache.page_size() as u64;
        let mut cursor = start;

        while cursor < end {
            let page_offset = cursor - (cursor % page_size);
            let page_end = page_offset
                .checked_add(page_size)
                .ok_or_else(|| invalid_length("The requested range is invalid."))?
                .min(self.info.size);
            let segment_end = end.min(page_end);
            let page = self.source_page(page_offset)?;
            let page_index = (cursor - page_offset) as usize;
            let segment_len = (segment_end - cursor) as usize;
            let segment_end_index = page_index.checked_add(segment_len).ok_or_else(|| {
                AppError::new(
                    ErrorCode::OperationFailed,
                    "The source page could not be read.",
                    None,
                )
            })?;
            let segment = page.get(page_index..segment_end_index).ok_or_else(|| {
                AppError::new(
                    ErrorCode::OperationFailed,
                    "The source page could not be read.",
                    None,
                )
            })?;
            result.extend_from_slice(segment);
            cursor = segment_end;
        }

        Ok(result)
    }

    fn source_page(&mut self, offset: u64) -> Result<Vec<u8>, AppError> {
        if let Some(bytes) = self.cache.get(offset) {
            return Ok(bytes.to_vec());
        }

        let page_len = (self.info.size - offset).min(self.cache.page_size() as u64) as usize;
        let mut bytes = vec![0; page_len];
        let path = Path::new(&self.info.path);
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|error| AppError::from_io(error, Some(path)))?;
        self.file
            .read_exact(&mut bytes)
            .map_err(|error| AppError::from_io(error, Some(path)))?;
        self.cache.insert(offset, bytes.clone());
        Ok(bytes)
    }
}

fn invalid_offset(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidOffset, message, None)
}

fn invalid_length(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidLength, message, None)
}

#[cfg(test)]
mod tests {
    use super::FileSession;
    use crate::error::ErrorCode;

    fn fixture(bytes: Vec<u8>) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), bytes).unwrap();
        file
    }

    #[test]
    fn session_reads_only_requested_range_and_overlays_edits() {
        let file = fixture((0u8..32).collect());
        let mut session = FileSession::open(file.path().to_path_buf(), 8, 2).unwrap();
        session.edit_byte(9, 0xfe).unwrap();
        let page = session.read_range(8, 4).unwrap();
        assert_eq!(page.bytes, vec![8, 0xfe, 10, 11]);
        assert_eq!(page.modified_offsets, vec![9]);
    }

    #[test]
    fn read_effective_chunk_is_bounded_and_overlays_edits() {
        let file = fixture((0u8..32).collect());
        let mut session = FileSession::open(file.path().to_path_buf(), 8, 2).unwrap();
        session.edit_byte(9, 0xfe).unwrap();
        let mut buffer = [0u8; 4];

        let read = session.read_effective_chunk(8, &mut buffer).unwrap();

        assert_eq!(read, 4);
        assert_eq!(buffer, [8, 0xfe, 10, 11]);
    }

    #[test]
    fn read_range_clamps_at_eof_and_marks_the_current_revision() {
        let file = fixture(vec![1, 2, 3]);
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(2, 9).unwrap();

        let page = session.read_range(1, 10).unwrap();

        assert_eq!(page.offset, 1);
        assert_eq!(page.bytes, vec![2, 9]);
        assert_eq!(page.modified_offsets, vec![2]);
        assert_eq!(page.revision, 1);
    }

    #[test]
    fn read_range_rejects_requests_larger_than_one_mebibyte() {
        let file = fixture(vec![0]);
        let mut session = FileSession::open(file.path().to_path_buf(), 1, 1).unwrap();

        let error = session.read_range(0, 1_048_577).unwrap_err();

        assert_eq!(error.code(), "invalid_length");
    }

    #[test]
    fn read_and_edit_reject_offsets_past_eof() {
        let file = fixture(vec![1, 2, 3]);
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 1).unwrap();

        assert_eq!(
            session.read_range(4, 1).unwrap_err().code,
            ErrorCode::InvalidOffset
        );
        assert_eq!(
            session.edit_byte(3, 0xff).unwrap_err().code,
            ErrorCode::InvalidOffset
        );
    }

    #[test]
    fn overlay_does_not_mutate_cached_source_pages() {
        let file = fixture(vec![4, 5, 6, 7]);
        let mut session = FileSession::open(file.path().to_path_buf(), 4, 1).unwrap();
        assert_eq!(session.read_range(0, 4).unwrap().bytes, vec![4, 5, 6, 7]);
        session.edit_byte(1, 0xfe).unwrap();
        assert_eq!(session.read_range(0, 4).unwrap().bytes, vec![4, 0xfe, 6, 7]);
        session.clear_edits();
        assert_eq!(session.read_range(0, 4).unwrap().bytes, vec![4, 5, 6, 7]);
    }

    #[test]
    fn externally_changed_source_rejects_cached_reads_without_discarding_edits() {
        let file = fixture(vec![4, 5, 6, 7]);
        let mut session = FileSession::open(file.path().to_path_buf(), 4, 1).unwrap();
        assert_eq!(session.read_range(0, 4).unwrap().bytes, vec![4, 5, 6, 7]);
        session.edit_byte(1, 0xfe).unwrap();

        std::fs::write(file.path(), [9, 5, 6, 7]).unwrap();
        let changed = std::fs::OpenOptions::new()
            .write(true)
            .open(file.path())
            .unwrap();
        changed
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
            .unwrap();

        assert_eq!(
            session.read_range(0, 4).unwrap_err().code(),
            "source_changed"
        );
        assert!(session.is_dirty());
    }

    #[test]
    fn externally_changed_source_rejects_streaming_reads() {
        let file = fixture(vec![1, 2, 3, 4]);
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        std::fs::write(file.path(), [1, 2, 3, 4, 5]).unwrap();
        let mut bytes = [0; 2];

        assert_eq!(
            session
                .read_effective_chunk(0, &mut bytes)
                .unwrap_err()
                .code(),
            "source_changed"
        );
    }

    #[test]
    fn replaced_source_path_is_rejected_even_when_size_and_mtime_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.bin");
        let moved = dir.path().join("original.bin");
        let fixed_time =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::write(&path, [1, 2, 3]).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(fixed_time)
            .unwrap();
        let mut session = FileSession::open(path.clone(), 2, 2).unwrap();

        std::fs::rename(&path, &moved).unwrap();
        std::fs::write(&path, [9, 8, 7]).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(fixed_time)
            .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            fixed_time
        );

        assert_eq!(
            session.read_range(0, 3).unwrap_err().code(),
            "source_changed"
        );
    }

    #[test]
    fn external_source_change_rejects_new_byte_edits() {
        let file = fixture(vec![1, 2]);
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        std::fs::write(file.path(), [1, 2, 3]).unwrap();

        assert_eq!(
            session.edit_byte(0, 9).unwrap_err().code(),
            "source_changed"
        );
        assert!(!session.is_dirty());
    }

    #[test]
    fn external_source_change_preserves_undo_history() {
        let file = fixture(vec![1, 2]);
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(0, 9).unwrap();
        std::fs::write(file.path(), [1, 2, 3]).unwrap();

        assert_eq!(session.undo().unwrap_err().code(), "source_changed");
        assert!(session.is_dirty());
    }

    #[test]
    fn no_op_edit_does_not_advance_revision() {
        let file = fixture(vec![4]);
        let mut session = FileSession::open(file.path().to_path_buf(), 1, 1).unwrap();

        session.edit_byte(0, 4).unwrap();

        assert_eq!(session.info().revision, 0);
        assert!(!session.is_dirty());
    }

    #[test]
    fn undo_restores_source_state_and_advances_revision() {
        let file = fixture(vec![4]);
        let mut session = FileSession::open(file.path().to_path_buf(), 1, 1).unwrap();
        session.edit_byte(0, 9).unwrap();

        assert!(session.undo().unwrap());
        assert_eq!(session.read_range(0, 1).unwrap().bytes, vec![4]);
        assert!(!session.is_dirty());
        assert_eq!(session.info().revision, 2);
    }

    #[test]
    fn modified_overview_tracks_edit_and_undo() {
        let file = fixture(vec![1, 2, 3, 4]);
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(3, 9).unwrap();
        assert_eq!(session.modified_overview_bins(), vec![1023]);
        session.undo().unwrap();
        assert!(session.modified_overview_bins().is_empty());
    }

    #[test]
    fn clear_edits_resets_dirty_state_and_advances_revision_once() {
        let file = fixture(vec![4]);
        let mut session = FileSession::open(file.path().to_path_buf(), 1, 1).unwrap();
        session.edit_byte(0, 9).unwrap();
        session.clear_edits();
        session.clear_edits();

        assert_eq!(session.info().revision, 2);
        assert!(!session.is_dirty());
    }

    #[test]
    fn session_info_uses_metadata_without_dirty_edits() {
        let file = fixture(vec![4, 5, 6]);
        let session = FileSession::open(file.path().to_path_buf(), 2, 1).unwrap();
        let info = session.info();

        assert_eq!(
            info.name,
            file.path().file_name().unwrap().to_string_lossy()
        );
        assert_eq!(
            info.path,
            std::fs::canonicalize(file.path())
                .unwrap()
                .to_string_lossy()
        );
        assert_eq!(info.size, 3);
        assert_eq!(info.revision, 0);
        assert!(!info.dirty);
    }
}
