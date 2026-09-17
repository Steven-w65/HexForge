use crate::edit_buffer::EditBuffer;
use crate::error::{AppError, ErrorCode};
use crate::page_cache::PageCache;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const MAX_READ_RANGE: u64 = 1_048_576;

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
    cache: PageCache,
    edits: EditBuffer,
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

        Ok(Self {
            file,
            info: FileInfo {
                name,
                path: canonical_path.to_string_lossy().into_owned(),
                size: metadata.len(),
                revision: 0,
                dirty: false,
            },
            cache: PageCache::new(page_size, max_pages),
            edits: EditBuffer::default(),
        })
    }

    pub fn info(&self) -> FileInfo {
        let mut info = self.info.clone();
        info.dirty = self.is_dirty();
        info
    }

    pub fn read_range(&mut self, start: u64, len: u64) -> Result<PageData, AppError> {
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
        self.edits.overlay(start, &mut bytes);
        Ok(self.page_data(start, bytes))
    }

    pub fn edit_byte(&mut self, offset: u64, value: u8) -> Result<(), AppError> {
        if offset >= self.info.size {
            return Err(invalid_offset("The edit offset is outside the file."));
        }
        let source = self.source_byte(offset)?;
        if self.edits.apply(offset, source, value) {
            self.info.revision = self.info.revision.saturating_add(1);
        }
        Ok(())
    }

    /// Reverts the most recent effective edit. Returns false when there is no edit to undo.
    pub fn undo(&mut self) -> Result<bool, AppError> {
        let Some(offset) = self.edits.last_offset() else {
            return Ok(false);
        };
        let source = self.source_byte(offset)?;
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

    fn page_data(&self, offset: u64, bytes: Vec<u8>) -> PageData {
        let modified_offsets = self.edits.modified_offsets(offset, bytes.len() as u64);
        PageData {
            offset,
            bytes,
            modified_offsets,
            revision: self.info.revision,
        }
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

        assert_eq!(info.name, file.file_name().to_string_lossy());
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
