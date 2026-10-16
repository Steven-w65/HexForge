//! Controller actions are separate from worker replies and window input.
use crate::search::MatchDirection;
use hexforge_core::{operations::CoreRequest, template::TemplateDefinition};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Action {
    /// Trusted workflow continuation. Presenters invoke Commands or typed input
    /// actions; they never construct raw core requests or bypass command policy.
    Core(CoreRequest),
    /// Scrolling changes only the viewport, never byte/parsed-field selection.
    ScrollViewport {
        offset: u64,
    },
    /// Bounded viewport data request; not an unrestricted core command.
    ReadViewport {
        offset: u64,
        length: u32,
    },
    Select {
        start: u64,
        end: u64,
    },
    Navigate {
        start: u64,
        end: u64,
    },
    NavigateTemplate {
        start: u64,
        end: u64,
    },
    DropFiles(Vec<PathBuf>),
    SetSearchPattern(String),
    SubmitSearch {
        force: bool,
        direction: MatchDirection,
    },
    ClearSearch,
    CloseSearch,
    SetGotoValue(String),
    SubmitGoto,
    CloseGoto,
    EditSelectedByte(String),
    SetEditValue(String),
    CancelEdit,
    ClearSelection,
    ClearError,
    ExpireNotice {
        revision: u64,
    },
    TemplateChanged(TemplateDefinition),
    TemplateDraft {
        definition: TemplateDefinition,
        valid: bool,
    },
    PrepareClose,
    ReleaseCloseBarrier,
}
