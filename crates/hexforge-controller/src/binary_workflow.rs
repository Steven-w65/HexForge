//! Binary copy/open/close decisions. Directives are interpreted by the single
//! controller so all effects receive globally unique IDs and domain stamps.
use crate::{
    dialog::{DialogDecision, DialogKind, DialogRequest, PickerRequest},
    shortcuts::WindowKind,
};
use hexforge_core::operations::CoreRequest;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct BinaryState {
    pub name: Option<String>,
    pub has_file: bool,
    pub dirty: bool,
}
#[derive(Debug, Clone)]
pub enum BinaryEvent {
    ChooseOpen,
    OpenPath(PathBuf),
    SaveCopy,
    CloseFile,
    Exit,
    Picker(Option<PathBuf>),
    Dialog(DialogDecision),
    CoreFinished { success: bool },
}
#[derive(Debug, Clone)]
pub enum BinaryDirective {
    Picker(PickerRequest),
    Dialog(DialogRequest),
    Core {
        request: CoreRequest,
        for_close: bool,
    },
    PrepareClose,
    ReleaseClose,
    ExitPrepared,
}
#[derive(Debug, Clone)]
enum Continuation {
    Open(PathBuf),
    Close,
    Exit,
}
impl Continuation {
    fn reason(&self) -> &'static str {
        match self {
            Self::Open(_) => "opening another binary",
            Self::Close => "closing this binary",
            Self::Exit => "exiting HexForge",
        }
    }
}
#[derive(Debug, Clone, Default)]
enum Stage {
    #[default]
    Idle,
    PickingOpen,
    Confirm(Continuation),
    PickingSave(Option<Continuation>),
    Saving(Option<Continuation>),
    Executing,
    QueryExit,
    PreparedExit,
}
#[derive(Debug, Default)]
pub struct BinaryWorkflow {
    stage: Stage,
}
impl BinaryWorkflow {
    pub(crate) fn awaiting_query(&self) -> bool {
        matches!(self.stage, Stage::QueryExit)
    }
    pub fn busy(&self) -> bool {
        !matches!(self.stage, Stage::Idle)
    }
    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
    }
    pub fn handle(&mut self, event: BinaryEvent, state: &BinaryState) -> Vec<BinaryDirective> {
        let mut out = vec![];
        match (self.stage.clone(), event) {
            (Stage::Idle, BinaryEvent::ChooseOpen) => {
                self.stage = Stage::PickingOpen;
                out.push(BinaryDirective::Picker(PickerRequest::binary_open()));
            }
            (Stage::Idle, BinaryEvent::OpenPath(path)) => {
                self.continue_or_confirm(Continuation::Open(path), state, &mut out)
            }
            (Stage::Idle, BinaryEvent::CloseFile) if state.has_file => {
                self.continue_or_confirm(Continuation::Close, state, &mut out)
            }
            (Stage::Idle, BinaryEvent::SaveCopy) if state.has_file => {
                self.stage = Stage::PickingSave(None);
                out.push(BinaryDirective::Picker(PickerRequest::binary_save(
                    state.name.as_deref(),
                )));
            }
            (Stage::Idle, BinaryEvent::Exit) => {
                self.stage = Stage::QueryExit;
                out.push(BinaryDirective::PrepareClose);
            }
            (Stage::PickingOpen, BinaryEvent::Picker(Some(path))) => {
                self.continue_or_confirm(Continuation::Open(path), state, &mut out)
            }
            (Stage::PickingOpen, BinaryEvent::Picker(None)) => self.stage = Stage::Idle,
            (Stage::Confirm(continuation), BinaryEvent::Dialog(DialogDecision::Discard)) => {
                self.execute(continuation, true, &mut out)
            }
            (Stage::Confirm(continuation), BinaryEvent::Dialog(DialogDecision::Save)) => {
                self.stage = Stage::PickingSave(Some(continuation));
                out.push(BinaryDirective::Picker(PickerRequest::binary_save(
                    state.name.as_deref(),
                )));
            }
            (Stage::Confirm(continuation), BinaryEvent::Dialog(_)) => {
                self.cancel(Some(continuation), &mut out)
            }
            (Stage::PickingSave(continuation), BinaryEvent::Picker(Some(path))) => {
                let for_close = matches!(continuation, Some(Continuation::Exit));
                self.stage = Stage::Saving(continuation);
                out.push(BinaryDirective::Core {
                    request: CoreRequest::SaveBinaryAs { path },
                    for_close,
                });
            }
            (Stage::PickingSave(continuation), BinaryEvent::Picker(None)) => {
                self.cancel(continuation, &mut out)
            }
            (Stage::Saving(Some(continuation)), BinaryEvent::CoreFinished { success: true }) => {
                self.execute(continuation, false, &mut out)
            }
            (Stage::Saving(continuation), BinaryEvent::CoreFinished { success: false }) => {
                self.cancel(continuation, &mut out)
            }
            (Stage::Saving(None) | Stage::Executing, BinaryEvent::CoreFinished { .. }) => {
                self.stage = Stage::Idle
            }
            (Stage::QueryExit, BinaryEvent::CoreFinished { success: true }) => {
                self.continue_or_confirm(Continuation::Exit, state, &mut out)
            }
            (Stage::QueryExit, BinaryEvent::CoreFinished { success: false }) => {
                self.stage = Stage::Idle;
                out.push(BinaryDirective::ReleaseClose);
            }
            _ => {} // Duplicate/obsolete input is not a second workflow.
        }
        out
    }
    fn continue_or_confirm(
        &mut self,
        continuation: Continuation,
        state: &BinaryState,
        out: &mut Vec<BinaryDirective>,
    ) {
        if state.dirty {
            out.push(BinaryDirective::Dialog(DialogRequest { kind: DialogKind::BinaryUnsaved, title: "Unsaved byte edits".into(),
                message: format!("Save unsaved byte edits to \"{}\" before {}? The original binary will not be overwritten.", state.name.as_deref().unwrap_or("this binary"), continuation.reason()), owner: WindowKind::Main }));
            self.stage = Stage::Confirm(continuation);
        } else {
            self.execute(continuation, false, out);
        }
    }
    fn execute(
        &mut self,
        continuation: Continuation,
        discard: bool,
        out: &mut Vec<BinaryDirective>,
    ) {
        match continuation {
            Continuation::Open(path) => {
                self.stage = Stage::Executing;
                out.push(BinaryDirective::Core {
                    request: CoreRequest::OpenFile {
                        path,
                        discard_unsaved: discard,
                    },
                    for_close: false,
                });
            }
            Continuation::Close => {
                self.stage = Stage::Executing;
                out.push(BinaryDirective::Core {
                    request: CoreRequest::CloseFile {
                        discard_unsaved: discard,
                    },
                    for_close: false,
                });
            }
            Continuation::Exit => {
                self.stage = Stage::PreparedExit;
                out.push(BinaryDirective::ExitPrepared);
            }
        }
    }
    fn cancel(&mut self, continuation: Option<Continuation>, out: &mut Vec<BinaryDirective>) {
        if matches!(continuation, Some(Continuation::Exit)) {
            out.push(BinaryDirective::ReleaseClose);
        }
        self.stage = Stage::Idle;
    }
}
