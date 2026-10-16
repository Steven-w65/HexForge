//! Synchronization identities are never filenames or template-format versions.
use hexforge_core::error::{AppError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceIdentity(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestStamp {
    pub request_id: RequestId,
    pub source: Option<SourceIdentity>,
    /// Controller content epoch, not a pathname or a lossy numeric DTO revision.
    pub content_revision: Option<u64>,
    pub workspace_revision: u64,
    /// Accepted-definition changes invalidate parsing without replacing the
    /// file workspace or discarding local editor input buffers.
    pub definition_revision: u64,
    pub persistence_revision: u64,
    pub viewport_intent: u64,
}

#[derive(Debug, Default)]
pub struct CheckedSequence {
    last: u64,
}
impl CheckedSequence {
    pub const fn new(last: u64) -> Self {
        Self { last }
    }
    pub fn next_id(&mut self) -> Result<u64, AppError> {
        let next = self.last.checked_add(1).ok_or_else(exhausted)?;
        self.last = next;
        Ok(next)
    }
}
pub(crate) fn increment(value: &mut u64) -> Result<(), AppError> {
    *value = value.checked_add(1).ok_or_else(exhausted)?;
    Ok(())
}
fn exhausted() -> AppError {
    AppError::new(
        ErrorCode::OperationFailed,
        "Synchronization identifiers have been exhausted.",
        None,
    )
}
