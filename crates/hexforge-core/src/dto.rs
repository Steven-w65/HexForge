//! Existing exact, JSON-safe boundary DTOs, shared by reference and native adapters.
use crate::search::SearchResult;
use crate::session::{FileInfo, FileSession, PageData};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirtyState {
    pub dirty: bool,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifiedOverviewDto {
    pub bin_count: u32,
    pub bins: Vec<u32>,
}

impl From<&FileSession> for DirtyState {
    fn from(session: &FileSession) -> Self {
        Self {
            dirty: session.is_dirty(),
            revision: session.info().revision.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResponse {
    pub undone: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<String>,
    #[serde(flatten)]
    pub state: DirtyState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResponse {
    pub bytes_written: String,
    pub destination: String,
    pub file: FileInfoDto,
    #[serde(flatten)]
    pub state: DirtyState,
}
