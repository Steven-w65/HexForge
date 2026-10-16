//! Pending counts and visible progress have the same owner. Finishing a newer
//! operation reveals an older activity, not a stale label paired with new progress.
use crate::identity::{CheckedSequence, RequestId};
use hexforge_core::{error::AppError, operations::CoreRequest, progress::OperationProgress};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperationKind {
    Open,
    Close,
    Page,
    Search,
    Parse,
    Edit,
    Undo,
    Save,
    Template,
    Export,
    Overview,
    Info,
    Minimap,
}
impl OperationKind {
    pub fn for_request(request: &CoreRequest) -> Self {
        use CoreRequest::*;
        match request {
            OpenFile { .. } => Self::Open,
            CloseFile { .. } => Self::Close,
            ReadPage { .. } => Self::Page,
            SearchBytes { .. } => Self::Search,
            ApplyTemplate { .. } => Self::Parse,
            EditByte { .. } => Self::Edit,
            UndoEdit => Self::Undo,
            SaveBinaryAs { .. } => Self::Save,
            LoadTemplate { .. } | SaveTemplate { .. } | SaveTemplateAs { .. } | UnloadTemplate => {
                Self::Template
            }
            ExportResultsCsv { .. } => Self::Export,
            GetModifiedOverview => Self::Overview,
            GetFileInfo | GetDirtyState => Self::Info,
            ReadMinimapSamples { .. } => Self::Minimap,
        }
    }
    pub fn visible(self) -> bool {
        !matches!(
            self,
            Self::Page | Self::Overview | Self::Info | Self::Minimap
        )
    }
}
#[derive(Debug, Clone)]
pub struct OperationActivity {
    pub operation: OperationKind,
    pub progress: Option<OperationProgress>,
}
#[derive(Debug, Default)]
pub struct OperationTracker {
    ids: CheckedSequence,
    pending: BTreeMap<RequestId, OperationActivity>,
}
impl OperationTracker {
    pub fn reserve_id(&mut self) -> Result<RequestId, AppError> {
        Ok(RequestId(self.ids.next_id()?))
    }
    pub fn begin(&mut self, kind: OperationKind) -> Result<RequestId, AppError> {
        let id = self.reserve_id()?;
        self.pending.insert(
            id,
            OperationActivity {
                operation: kind,
                progress: None,
            },
        );
        Ok(id)
    }
    pub fn finish(&mut self, id: RequestId) {
        self.pending.remove(&id);
    }
    pub fn report(&mut self, id: RequestId, progress: OperationProgress) {
        if let Some(activity) = self.pending.get_mut(&id) {
            activity.progress = Some(progress);
        }
    }
    pub fn is_busy(&self, kind: OperationKind) -> bool {
        self.pending.values().any(|a| a.operation == kind)
    }
    pub fn visible_activity(&self) -> Option<OperationActivity> {
        self.pending
            .values()
            .rev()
            .find(|a| a.operation.visible())
            .cloned()
    }
    pub fn busy_snapshot(&self) -> BTreeMap<OperationKind, bool> {
        self.pending.values().map(|a| (a.operation, true)).collect()
    }
}
/// Only operations that mutate guarded core state are serialized. Reads retain
/// the existing core locks; this is not the stopped concurrent-read redesign.
pub(crate) fn is_mutation(request: &CoreRequest) -> bool {
    use CoreRequest::*;
    matches!(
        request,
        OpenFile { .. }
            | CloseFile { .. }
            | EditByte { .. }
            | UndoEdit
            | SaveBinaryAs { .. }
            | LoadTemplate { .. }
            | SaveTemplate { .. }
            | SaveTemplateAs { .. }
            | UnloadTemplate
    )
}
