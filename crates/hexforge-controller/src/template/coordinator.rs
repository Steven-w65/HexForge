//! Native cross-window protocol: one authority, explicit flush barriers and
//! session-scoped close acknowledgements. Local control buffers are not a
//! second file workspace and never own persistence.
use crate::{
    command::Command,
    dialog::{ControllerError, DialogDecision, DialogKind, DialogRequest},
    identity::{increment, CheckedSequence, RequestId},
    shortcuts::WindowKind,
    template::workspace::{TemplateDirective, TemplateEvent, TemplateSnapshot, TemplateWorkspace},
};
use hexforge_core::{error::AppError, template::TemplateDefinition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorSessionId(pub u64);
#[derive(Debug, Clone)]
pub struct DraftUpdate {
    pub session: EditorSessionId,
    pub sequence: u64,
    pub workspace_revision: u64,
    pub definition: TemplateDefinition,
    pub valid: bool,
    pub pending_inputs: bool,
}
#[derive(Debug, Clone)]
pub struct EditorEnvelope {
    pub session: EditorSessionId,
    pub revision: u64,
    pub acknowledged_sequence: u64,
    pub workspace: TemplateSnapshot,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCloseDecision {
    Save,
    SaveAs,
    Discard,
    Cancel,
}
#[derive(Debug, Clone)]
pub enum EditorAction {
    Command(Command),
    Navigate { start: u64, end: u64 },
    Close,
    Discard,
}
#[derive(Debug, Clone)]
pub enum EditorMessage {
    Ready {
        session: EditorSessionId,
    },
    Draft(DraftUpdate),
    Action {
        request_id: RequestId,
        draft: DraftUpdate,
        action: EditorAction,
    },
    FlushReply {
        request_id: RequestId,
        draft: DraftUpdate,
    },
    FlushFailed {
        request_id: RequestId,
        message: String,
    },
    CloseRequested {
        session: EditorSessionId,
    },
    CloseDecision {
        session: EditorSessionId,
        decision: EditorCloseDecision,
    },
    Closed {
        session: EditorSessionId,
    },
    CloseFailed {
        session: EditorSessionId,
        message: String,
    },
    OpenFailed {
        session: EditorSessionId,
        message: String,
    },
}
#[derive(Debug, Clone, Copy)]
pub enum FlushPurpose {
    MainCommand(Command),
    MainClose,
    EditorClose,
    ExitAfterBinary,
}
#[derive(Debug, Clone)]
pub enum EditorDirective {
    Workspace(TemplateDirective),
    Execute(Command),
    Navigate {
        start: u64,
        end: u64,
    },
    Flush {
        request_id: RequestId,
        session: EditorSessionId,
    },
    RequestCloseFlush,
    ShowCloseDialog(DialogRequest),
    CloseEditor(EditorSessionId),
    CloseMain,
    ContinueMainClose,
    AbortExit,
    Reply {
        request_id: RequestId,
        success: bool,
    },
    Error(ControllerError),
}
#[derive(Debug)]
struct FlushPending {
    id: RequestId,
    session: EditorSessionId,
    workspace: u64,
    purpose: FlushPurpose,
}
#[derive(Debug)]
struct PendingAction {
    id: RequestId,
    workspace: u64,
}
#[derive(Debug, Default)]
pub struct EditorCoordinator {
    ids: CheckedSequence,
    session: Option<EditorSessionId>,
    ready: bool,
    last_sequence: u64,
    snapshot_revision: u64,
    pending_inputs: bool,
    flush: Option<FlushPending>,
    pending_action: Option<PendingAction>,
    pub close_prompt: bool,
    closing: bool,
    close_save_sequence: Option<u64>,
    pub main_closing: bool,
    exit_template_check: bool,
    main_waiting_editor: bool,
    exit_was_dirty: bool,
}
impl EditorCoordinator {
    pub fn session(&self) -> Option<EditorSessionId> {
        self.session
    }
    pub fn acknowledged_sequence(&self) -> u64 {
        self.last_sequence
    }
    pub fn revision(&self) -> u64 {
        self.snapshot_revision
    }
    pub fn blocking(&self) -> bool {
        self.flush.is_some() || self.closing || self.main_closing
    }
    pub fn main_blocked(&self) -> bool {
        self.main_closing
            || self
                .flush
                .as_ref()
                .is_some_and(|f| matches!(f.purpose, FlushPurpose::MainCommand(_)))
    }
    pub fn editor_blocked(&self) -> bool {
        self.blocking() || self.pending_action.is_some() || self.close_save_sequence.is_some()
    }
    pub fn open(
        &mut self,
        _workspace: &mut TemplateWorkspace,
    ) -> Result<EditorSessionId, AppError> {
        if let Some(session) = self.session {
            return Ok(session);
        }
        let session = EditorSessionId(self.ids.next_id()?);
        self.session = Some(session);
        self.ready = false;
        self.last_sequence = 0;
        self.snapshot_revision = 0;
        self.pending_inputs = false;
        self.close_prompt = false;
        self.closing = false;
        Ok(session)
    }
    pub fn publish(&mut self) -> Result<(), AppError> {
        if self.session.is_some() {
            increment(&mut self.snapshot_revision)?;
        }
        Ok(())
    }
    pub fn start_flush(
        &mut self,
        id: RequestId,
        purpose: FlushPurpose,
        workspace: &mut TemplateWorkspace,
    ) -> Vec<EditorDirective> {
        if self.flush.is_some() {
            return vec![];
        }
        let Some(session) = self.session else {
            return self.after_flush(purpose, workspace);
        };
        self.flush = Some(FlushPending {
            id,
            session,
            workspace: workspace.workspace_revision,
            purpose,
        });
        vec![EditorDirective::Flush {
            request_id: id,
            session,
        }]
    }
    pub fn receive(
        &mut self,
        message: EditorMessage,
        workspace: &mut TemplateWorkspace,
    ) -> Vec<EditorDirective> {
        match message {
            EditorMessage::Ready { session } if self.session == Some(session) => {
                if !self.ready {
                    self.ready = true;
                    workspace.checkpoint_current();
                }
                vec![]
            }
            EditorMessage::Draft(draft) => self.accept(draft, workspace),
            EditorMessage::Action {
                request_id,
                draft,
                action,
            } => {
                if !self.current_draft(&draft, workspace) {
                    return vec![EditorDirective::Reply {
                        request_id,
                        success: false,
                    }];
                }
                if self.pending_action.is_some() || self.closing {
                    return vec![EditorDirective::Reply {
                        request_id,
                        success: false,
                    }];
                }
                let mut out = self.accept(draft, workspace);
                self.pending_action = Some(PendingAction {
                    id: request_id,
                    workspace: workspace.workspace_revision,
                });
                match action {
                    EditorAction::Command(command) => out.push(EditorDirective::Execute(command)),
                    EditorAction::Navigate { start, end } => {
                        out.push(EditorDirective::Navigate { start, end });
                        out.extend(self.action_completed(true, workspace));
                    }
                    EditorAction::Close => {
                        self.closing = true;
                        out.push(EditorDirective::CloseEditor(
                            self.session.expect("validated session"),
                        ));
                    }
                    EditorAction::Discard => out.extend(self.discard(workspace)),
                }
                out
            }
            EditorMessage::FlushReply { request_id, draft } => {
                let Some(pending) = &self.flush else {
                    return vec![];
                };
                if pending.id != request_id {
                    return vec![];
                }
                if pending.session != draft.session
                    || pending.workspace != draft.workspace_revision
                    || !self.current_draft(&draft, workspace)
                {
                    self.flush = None;
                    return vec![EditorDirective::Error(ControllerError::invalid_input("The template workspace changed. Try the action again with the current draft.")), EditorDirective::AbortExit];
                }
                let purpose = self.flush.take().expect("checked flush").purpose;
                let mut out = self.accept(draft, workspace);
                out.extend(self.after_flush(purpose, workspace));
                out
            }
            EditorMessage::FlushFailed {
                request_id,
                message,
            } if self.flush.as_ref().is_some_and(|p| p.id == request_id) => {
                self.flush = None;
                vec![
                    EditorDirective::Error(ControllerError::invalid_input(message)),
                    EditorDirective::AbortExit,
                ]
            }
            EditorMessage::CloseRequested { session }
                if self.session == Some(session) && !self.editor_blocked() =>
            {
                vec![EditorDirective::RequestCloseFlush]
            }
            EditorMessage::CloseDecision { session, decision }
                if self.session == Some(session)
                    && self.close_prompt
                    && !self.closing
                    && self.close_save_sequence.is_none() =>
            {
                match decision {
                    EditorCloseDecision::Cancel => {
                        self.close_prompt = false;
                        vec![]
                    }
                    EditorCloseDecision::Discard => self.discard(workspace),
                    EditorCloseDecision::Save | EditorCloseDecision::SaveAs => {
                        self.close_save_sequence = Some(self.last_sequence);
                        vec![EditorDirective::Execute(
                            if decision == EditorCloseDecision::Save {
                                Command::SaveTemplate
                            } else {
                                Command::SaveTemplateAs
                            },
                        )]
                    }
                }
            }
            EditorMessage::Closed { session } if self.session == Some(session) => {
                let interrupted = self.flush.is_some()
                    || self.pending_action.is_some()
                    || self.close_save_sequence.is_some();
                self.session = None;
                self.ready = false;
                self.close_prompt = false;
                self.closing = false;
                self.flush = None;
                self.pending_action = None;
                self.pending_inputs = false;
                self.close_save_sequence = None;
                if self.main_waiting_editor {
                    self.main_waiting_editor = false;
                    vec![EditorDirective::CloseMain]
                } else if interrupted {
                    vec![EditorDirective::Error(ControllerError::invalid_input("Template Editor disconnected. Its draft remains in the main workspace.")), EditorDirective::AbortExit]
                } else {
                    vec![]
                }
            }
            EditorMessage::CloseFailed { session, message } if self.session == Some(session) => {
                self.closing = false;
                self.main_waiting_editor = false;
                vec![
                    EditorDirective::Error(ControllerError::invalid_input(message)),
                    EditorDirective::AbortExit,
                ]
            }
            EditorMessage::OpenFailed { session, message } if self.session == Some(session) => {
                self.session = None;
                self.ready = false;
                self.flush = None;
                vec![
                    EditorDirective::Error(ControllerError::invalid_input(message)),
                    EditorDirective::AbortExit,
                ]
            }
            _ => vec![],
        }
    }
    fn current_draft(&self, draft: &DraftUpdate, workspace: &TemplateWorkspace) -> bool {
        self.session == Some(draft.session)
            && draft.workspace_revision == workspace.workspace_revision
    }
    fn accept(
        &mut self,
        draft: DraftUpdate,
        workspace: &mut TemplateWorkspace,
    ) -> Vec<EditorDirective> {
        if !self.current_draft(&draft, workspace) || draft.sequence <= self.last_sequence {
            return vec![];
        }
        self.last_sequence = draft.sequence;
        self.pending_inputs = draft.pending_inputs;
        workspace
            .handle(TemplateEvent::Draft {
                definition: draft.definition,
                valid: draft.valid,
            })
            .into_iter()
            .map(EditorDirective::Workspace)
            .collect()
    }
    fn changed(&self, workspace: &TemplateWorkspace) -> bool {
        self.pending_inputs
            || !workspace.raw_valid
            || workspace.definition != workspace.snapshot().checkpoint_definition
    }
    fn after_flush(
        &mut self,
        purpose: FlushPurpose,
        workspace: &mut TemplateWorkspace,
    ) -> Vec<EditorDirective> {
        match purpose {
            FlushPurpose::MainCommand(command) => vec![EditorDirective::Execute(command)],
            FlushPurpose::MainClose => vec![EditorDirective::ContinueMainClose],
            FlushPurpose::EditorClose => {
                if self.changed(workspace) {
                    self.close_prompt = true;
                    vec![EditorDirective::ShowCloseDialog(DialogRequest {
                        kind: DialogKind::EditorUnsaved,
                        title: "Unsaved template edits".into(),
                        message: "Save changes made in this Template Editor before closing?".into(),
                        owner: WindowKind::TemplateEditor,
                    })]
                } else {
                    self.closing = true;
                    self.session
                        .map(|s| vec![EditorDirective::CloseEditor(s)])
                        .unwrap_or_default()
                }
            }
            FlushPurpose::ExitAfterBinary => self.check_exit_template(workspace),
        }
    }
    pub fn action_completed(
        &mut self,
        success: bool,
        workspace: &TemplateWorkspace,
    ) -> Vec<EditorDirective> {
        let mut out = vec![];
        if let Some(pending) = self.pending_action.take() {
            out.push(EditorDirective::Reply {
                request_id: pending.id,
                success: success && pending.workspace == workspace.workspace_revision,
            });
        }
        if let Some(sequence) = self.close_save_sequence.take() {
            if success && sequence == self.last_sequence && !self.changed(workspace) {
                self.closing = true;
                if let Some(session) = self.session {
                    out.push(EditorDirective::CloseEditor(session));
                }
            }
        }
        out
    }
    fn discard(&mut self, workspace: &mut TemplateWorkspace) -> Vec<EditorDirective> {
        match workspace.restore_checkpoint() {
            Ok(()) => {
                self.pending_inputs = false;
                self.closing = true;
                let mut out = vec![EditorDirective::Workspace(
                    TemplateDirective::ClearHighlight,
                )];
                if let Some(pending) = self.pending_action.take() {
                    out.push(EditorDirective::Reply {
                        request_id: pending.id,
                        success: pending.workspace.checked_add(1)
                            == Some(workspace.workspace_revision),
                    });
                }
                if let Some(session) = self.session {
                    out.push(EditorDirective::CloseEditor(session));
                }
                out
            }
            Err(error) => vec![EditorDirective::Error(error.into())],
        }
    }
    pub fn begin_main_close(&mut self) {
        self.main_closing = true;
        self.exit_was_dirty = false;
    }
    pub fn note_exit_dirty(&mut self, dirty: bool) {
        if self.main_closing {
            self.exit_was_dirty |= dirty;
        }
    }
    pub fn after_binary_close(&mut self, workspace: &TemplateWorkspace) -> bool {
        self.main_closing && (self.exit_was_dirty || workspace.dirty() || self.pending_inputs)
    }
    pub fn check_exit_template(&mut self, workspace: &TemplateWorkspace) -> Vec<EditorDirective> {
        if workspace.dirty() || self.pending_inputs {
            self.exit_template_check = true;
            vec![EditorDirective::ShowCloseDialog(DialogRequest {
                kind: DialogKind::Confirmation,
                title: "HexForge".into(),
                message: "Unsaved template changes will be lost. Exit HexForge?".into(),
                owner: WindowKind::Main,
            })]
        } else {
            self.finish_main_close()
        }
    }
    pub fn exit_decision(&mut self, decision: DialogDecision) -> Vec<EditorDirective> {
        if !self.exit_template_check {
            return vec![];
        }
        self.exit_template_check = false;
        self.exit_was_dirty = false;
        if decision == DialogDecision::Confirm {
            self.finish_main_close()
        } else {
            vec![EditorDirective::AbortExit]
        }
    }
    fn finish_main_close(&mut self) -> Vec<EditorDirective> {
        if let Some(session) = self.session {
            self.main_waiting_editor = true;
            self.closing = true;
            vec![EditorDirective::CloseEditor(session)]
        } else {
            vec![EditorDirective::CloseMain]
        }
    }
    pub fn abort_exit(&mut self) {
        self.main_closing = false;
        self.main_waiting_editor = false;
        self.exit_template_check = false;
    }
}

/// Pending text survives acknowledgements for older input. A genuinely newer
/// workspace explicitly replaces it; stale replies cannot resurrect it.
pub struct EditorBuffer {
    current: EditorEnvelope,
    sequence: u64,
    acknowledged: u64,
    valid: bool,
    pending_inputs: bool,
    pending_actions: std::collections::HashMap<RequestId, (u64, bool)>,
}
impl EditorBuffer {
    pub fn new(current: EditorEnvelope) -> Self {
        let sequence = current.acknowledged_sequence;
        Self {
            current,
            sequence,
            acknowledged: sequence,
            valid: true,
            pending_inputs: false,
            pending_actions: Default::default(),
        }
    }
    pub fn edit(
        &mut self,
        definition: TemplateDefinition,
        valid: bool,
        pending_inputs: bool,
    ) -> Result<(), AppError> {
        increment(&mut self.sequence)?;
        self.current.workspace.definition = definition;
        self.valid = valid;
        self.pending_inputs = pending_inputs;
        Ok(())
    }
    pub fn draft(&self) -> DraftUpdate {
        DraftUpdate {
            session: self.current.session,
            sequence: self.sequence,
            workspace_revision: self.current.workspace.workspace_revision,
            definition: self.current.workspace.definition.clone(),
            valid: self.valid,
            pending_inputs: self.pending_inputs,
        }
    }
    pub fn acknowledged(&self) -> bool {
        self.acknowledged >= self.sequence
    }
    pub fn acknowledge(&mut self, session: EditorSessionId, workspace: u64, sequence: u64) {
        if session == self.current.session && workspace == self.current.workspace.workspace_revision
        {
            self.acknowledged = self.acknowledged.max(sequence);
        }
    }
    pub fn begin_action(&mut self, id: RequestId, discard: bool) {
        self.pending_actions
            .insert(id, (self.current.workspace.workspace_revision, discard));
    }
    /// Replies have no picker timeout. Only a reply for this action and its
    /// own authoritative workspace may complete it; discard alone advances it.
    pub fn complete_action(
        &mut self,
        id: RequestId,
        success: bool,
        incoming: Option<EditorEnvelope>,
    ) -> Option<bool> {
        let (workspace, discard) = self.pending_actions.remove(&id)?;
        let reply_workspace = incoming.as_ref().map(|s| s.workspace.workspace_revision);
        if let Some(snapshot) = incoming {
            self.apply_snapshot(snapshot);
        }
        let current = self.current.workspace.workspace_revision;
        Some(
            success
                && (workspace == current
                    || (discard
                        && workspace.checked_add(1) == Some(current)
                        && reply_workspace == Some(current))),
        )
    }
    pub fn apply_snapshot(&mut self, mut incoming: EditorEnvelope) -> bool {
        if incoming.session != self.current.session
            || incoming.revision < self.current.revision
            || incoming.workspace.workspace_revision < self.current.workspace.workspace_revision
        {
            return false;
        }
        if incoming.workspace.workspace_revision > self.current.workspace.workspace_revision {
            self.sequence = incoming.acknowledged_sequence;
            self.acknowledged = incoming.acknowledged_sequence;
            self.valid = true;
            self.pending_inputs = false;
        } else {
            self.acknowledged = self.acknowledged.max(incoming.acknowledged_sequence);
            if incoming.acknowledged_sequence < self.sequence {
                incoming.workspace.definition = self.current.workspace.definition.clone();
            }
        }
        self.current = incoming;
        true
    }
}
