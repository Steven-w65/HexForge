//! Bounded operation metadata; delivery failure cannot invalidate safe I/O.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    pub(crate) fn new(
        operation_id: String,
        phase: OperationPhase,
        processed: u64,
        total: u64,
    ) -> Self {
        Self {
            operation_id,
            phase,
            processed: processed.to_string(),
            total: total.to_string(),
        }
    }
}

pub(crate) struct Reporter<'a> {
    id: String,
    sink: &'a mut dyn FnMut(OperationProgress),
}

impl<'a> Reporter<'a> {
    pub(crate) fn new(sink: &'a mut dyn FnMut(OperationProgress)) -> Self {
        static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_OPERATION.fetch_add(1, Ordering::Relaxed).to_string(),
            sink,
        }
    }
    pub(crate) fn send(&mut self, phase: OperationPhase, processed: u64, total: u64) {
        (self.sink)(OperationProgress::new(
            self.id.clone(),
            phase,
            processed,
            total,
        ));
    }
}
