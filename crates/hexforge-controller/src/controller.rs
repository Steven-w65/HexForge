//! Authoritative, GPU/OS-independent event reducer. Worker completions carry
//! unchanged stamps; domain checks happen here, not in response callbacks.
use crate::{
    action::Action,
    binary_workflow::{BinaryDirective, BinaryEvent, BinaryState, BinaryWorkflow},
    command::Command,
    dialog::{ControllerError, DialogDecision, DialogRequest, PickerRequest},
    goto::{parse_goto, GotoState},
    identity::{increment, CheckedSequence, RequestId, RequestStamp, SourceIdentity},
    operation::{is_mutation, OperationActivity, OperationKind, OperationTracker},
    policy::{command_enabled, PolicyState},
    search::{parse_edit_byte, parse_search_pattern, MatchDirection, SearchState},
    shortcuts::WindowKind,
    template::coordinator::{
        EditorCoordinator, EditorDirective, EditorMessage, EditorSessionId, FlushPurpose,
    },
    template::workspace::{
        TemplateAction, TemplateDirective, TemplateEvent, TemplateReply, TemplateSnapshot,
        TemplateWorkspace,
    },
};
use hexforge_core::{
    dto::*,
    error::{AppError, ErrorCode},
    operations::{CoreReply, CoreRequest},
    progress::OperationProgress,
    template::{ParsedNode, TemplateDefinition},
};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowWidth {
    Sixteen,
    ThirtyTwo,
}
impl RowWidth {
    pub fn bytes(self) -> u64 {
        match self {
            Self::Sixteen => 16,
            Self::ThirtyTwo => 32,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimapMode {
    Fit,
    Proportional,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimapRender {
    Characters,
    Blocks,
}
#[derive(Debug, Clone)]
pub struct MinimapSettings {
    pub enabled: bool,
    pub mode: MinimapMode,
    pub render: MinimapRender,
    pub scale: u8,
}
impl Default for MinimapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: MinimapMode::Fit,
            render: MinimapRender::Characters,
            scale: 1,
        }
    }
}
#[derive(Debug, Clone)]
pub struct InitialState {
    pub theme: Theme,
    pub row_width: RowWidth,
    pub minimap: MinimapSettings,
}
impl Default for InitialState {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            row_width: RowWidth::Sixteen,
            minimap: MinimapSettings::default(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteSelection {
    pub start: u64,
    pub end: u64,
}
impl ByteSelection {
    pub fn count(&self) -> u128 {
        u128::from(self.end) - u128::from(self.start) + 1
    }
}
#[derive(Debug, Clone)]
pub struct ViewportPage {
    pub data: PageResponse,
    pub generation: u64,
}
#[derive(Debug, Clone)]
pub struct ByteEditState {
    pub offset: u64,
    pub value: String,
    pub error: Option<String>,
    pub pending: bool,
}
impl std::ops::Deref for ViewportPage {
    type Target = PageResponse;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}
#[derive(Debug, Clone)]
pub struct MainSnapshot {
    pub file: Option<FileInfoDto>,
    pub source: Option<SourceIdentity>,
    pub page: Option<ViewportPage>,
    pub selection: Option<ByteSelection>,
    pub viewport_offset: u64,
    pub content_revision: u64,
    pub template: TemplateDefinition,
    pub template_state: TemplateSnapshot,
    pub template_range: Option<ByteSelection>,
    pub completion_notice: Option<String>,
    pub notice_revision: u64,
    pub editor_session: Option<EditorSessionId>,
    pub editor_close_prompt: bool,
    pub results: Vec<ParsedNode>,
    pub template_applied: bool,
    pub results_need_refresh: bool,
    pub matches: Vec<u64>,
    pub activity: Option<OperationActivity>,
    pub busy: BTreeMap<OperationKind, bool>,
    pub error: Option<ControllerError>,
    pub search: SearchState,
    pub goto: GotoState,
    pub dialog: Option<(RequestId, DialogRequest)>,
    pub edit_dialog: Option<ByteEditState>,
    pub edit_mode: bool,
    pub can_undo: bool,
    pub theme: Theme,
    pub row_width: RowWidth,
    pub minimap: MinimapSettings,
    pub modified_overview: ModifiedOverviewDto,
    pub last_edit_delta: Option<(u64, String)>,
    pub policy: PolicyState,
}
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub template: TemplateDefinition,
    pub template_state: TemplateSnapshot,
    pub results: Vec<ParsedNode>,
    pub theme: Theme,
    pub workspace_revision: u64,
    pub persistence_revision: u64,
    pub busy: bool,
    pub session: Option<EditorSessionId>,
    pub snapshot_revision: u64,
    pub acknowledged_sequence: u64,
    pub close_prompt: bool,
    pub policy: PolicyState,
    pub notice: Option<String>,
}
#[derive(Debug, Clone)]
pub enum WindowSnapshot {
    Main(MainSnapshot),
    Editor(EditorSnapshot),
}
#[derive(Debug, Clone, Copy, Default)]
pub struct InvokeContext {
    pub window: Option<WindowKind>,
    pub modal: bool,
    pub composing: bool,
}
#[derive(Debug)]
pub enum ControllerEvent {
    /// A platform window-close request is not a menu/keyboard command. Like
    /// the reference close listener, it waits for started byte mutations even
    /// when their edit dialog owns ordinary command input.
    MainCloseRequested,
    /// Auxiliary reads share the source/revision error ownership of viewport
    /// reads, without discarding accepted bytes or changing the workspace.
    SourceReadFailed {
        source: SourceIdentity,
        revision: String,
        error: AppError,
    },
    /// An actual OS preference transition, not a frame/theme polling result.
    /// Bypasses user command availability without touching file/draft state.
    SystemThemeChanged(Theme),
    Editor(EditorMessage),
    Invoke {
        command: Command,
        context: InvokeContext,
    },
    Action(Action),
    CoreCompleted {
        stamp: RequestStamp,
        reply: Result<CoreReply, AppError>,
    },
    Progress {
        stamp: RequestStamp,
        progress: OperationProgress,
    },
    PickerCompleted {
        request_id: RequestId,
        path: Option<PathBuf>,
    },
    DialogCompleted {
        request_id: RequestId,
        decision: DialogDecision,
    },
    PickerFailed {
        request_id: RequestId,
        error: AppError,
    },
}
#[derive(Debug)]
pub enum Effect {
    RunCore {
        stamp: RequestStamp,
        request: CoreRequest,
    },
    OpenOrFocusEditor {
        session: EditorSessionId,
    },
    FlushEditorDraft {
        request_id: RequestId,
        session: EditorSessionId,
    },
    CloseWindow {
        window: WindowKind,
        session: Option<EditorSessionId>,
    },
    FocusWindow {
        window: WindowKind,
        session: Option<EditorSessionId>,
    },
    EditorReply {
        session: EditorSessionId,
        request_id: RequestId,
        success: bool,
    },
    EditorError {
        session: EditorSessionId,
        message: String,
    },
    OpenPicker {
        request_id: RequestId,
        picker: PickerRequest,
    },
    ShowDialog {
        request_id: RequestId,
        dialog: DialogRequest,
    },
    ExitPrepared,
    RequestRepaint,
}
#[derive(Debug)]
struct Pending {
    stamp: RequestStamp,
    request: CoreRequest,
    selection_revision: u64,
    page_generation: u64,
    workspace_binding: u64,
}

#[derive(Debug, Clone, Copy)]
enum TemplateDialogEnd {
    Operation,
    Decision(RequestId),
}
#[derive(Debug, Clone, Copy)]
struct TemplateDialogReturn {
    session: EditorSessionId,
    end: TemplateDialogEnd,
}

pub struct AppController {
    initial: InitialState,
    tracker: OperationTracker,
    source_ids: CheckedSequence,
    source: Option<SourceIdentity>,
    file: Option<FileInfoDto>,
    page: Option<ViewportPage>,
    selection: Option<ByteSelection>,
    selection_revision: u64,
    viewport_offset: u64,
    content_revision: u64,
    viewport_intent: u64,
    workspace: TemplateWorkspace,
    template_core: Option<RequestId>,
    template_ui: Option<RequestId>,
    template_owner: WindowKind,
    template_dialog_return: Option<TemplateDialogReturn>,
    template_range: Option<ByteSelection>,
    completion_notice: Option<String>,
    notice_revision: u64,
    edit_dialog: Option<ByteEditState>,
    edit_dialog_request: Option<RequestId>,
    csv_ui: Option<RequestId>,
    editor: EditorCoordinator,
    exit_dialog: Option<RequestId>,
    search: SearchState,
    goto: GotoState,
    error: Option<ControllerError>,
    binary_workflow: BinaryWorkflow,
    binary_core: Option<RequestId>,
    binary_ui: Option<RequestId>,
    dialog: Option<(RequestId, DialogRequest)>,
    latest_error_intent: u64,
    pending: HashMap<RequestId, Pending>,
    latest: HashMap<OperationKind, RequestId>,
    mutation_queue: VecDeque<Pending>,
    mutation_running: Option<RequestId>,
    close_barrier: bool,
    close_query_pending: bool,
    edit_mode: bool,
    undo_depth: u64,
    modified_overview: ModifiedOverviewDto,
    last_edit_delta: Option<(u64, String)>,
    refresh_requested: Option<(SourceIdentity, u64)>,
}
impl AppController {
    pub fn new(initial: InitialState) -> Self {
        Self {
            initial,
            tracker: OperationTracker::default(),
            source_ids: CheckedSequence::default(),
            source: None,
            file: None,
            page: None,
            selection: None,
            selection_revision: 0,
            viewport_offset: 0,
            content_revision: 0,
            viewport_intent: 0,
            workspace: TemplateWorkspace::default(),
            template_core: None,
            template_ui: None,
            template_owner: WindowKind::Main,
            template_dialog_return: None,
            template_range: None,
            completion_notice: None,
            notice_revision: 0,
            edit_dialog: None,
            edit_dialog_request: None,
            csv_ui: None,
            editor: EditorCoordinator::default(),
            exit_dialog: None,
            search: SearchState::default(),
            goto: GotoState::default(),
            binary_workflow: BinaryWorkflow::default(),
            binary_core: None,
            binary_ui: None,
            dialog: None,
            error: None,
            latest_error_intent: 0,
            pending: HashMap::new(),
            latest: HashMap::new(),
            mutation_queue: VecDeque::new(),
            mutation_running: None,
            close_barrier: false,
            close_query_pending: false,
            edit_mode: false,
            undo_depth: 0,
            modified_overview: ModifiedOverviewDto {
                bin_count: 1024,
                bins: vec![],
            },
            last_edit_delta: None,
            refresh_requested: None,
        }
    }
    pub fn snapshot(&self, window: WindowKind) -> WindowSnapshot {
        match window {
            WindowKind::Main => WindowSnapshot::Main(MainSnapshot {
                file: self.file.clone(),
                source: self.source,
                page: self.page.clone(),
                selection: self.selection.clone(),
                viewport_offset: self.viewport_offset,
                content_revision: self.content_revision,
                template: self.workspace.definition.clone(),
                template_state: self.workspace.snapshot(),
                template_range: self.template_range.clone(),
                completion_notice: self.completion_notice.clone(),
                notice_revision: self.notice_revision,
                editor_session: self.editor.session(),
                editor_close_prompt: self.editor.close_prompt,
                results: self.workspace.results.clone(),
                template_applied: self.workspace.applied,
                results_need_refresh: self.workspace.needs_refresh,
                matches: self.search.matches.clone(),
                search: self.search.clone(),
                goto: self.goto.clone(),
                dialog: self.dialog.clone(),
                edit_dialog: self.edit_dialog.clone(),
                activity: self.tracker.visible_activity(),
                busy: self.tracker.busy_snapshot(),
                error: self.error.clone(),
                edit_mode: self.edit_mode,
                can_undo: self.undo_depth > 0,
                theme: self.initial.theme,
                row_width: self.initial.row_width,
                minimap: self.initial.minimap.clone(),
                modified_overview: self.modified_overview.clone(),
                last_edit_delta: self.last_edit_delta.clone(),
                policy: self.policy(),
            }),
            WindowKind::TemplateEditor => WindowSnapshot::Editor(EditorSnapshot {
                template: self.workspace.definition.clone(),
                template_state: self.workspace.snapshot(),
                results: self.workspace.results.clone(),
                theme: self.initial.theme,
                workspace_revision: self.workspace.workspace_revision,
                persistence_revision: self.workspace.persistence_revision,
                busy: self.policy().operation_busy || self.editor.editor_blocked(),
                session: self.editor.session(),
                snapshot_revision: self.editor.revision(),
                acknowledged_sequence: self.editor.acknowledged_sequence(),
                close_prompt: self.editor.close_prompt,
                policy: self.policy(),
                notice: self.completion_notice.clone(),
            }),
        }
    }
    pub fn handle(&mut self, event: ControllerEvent) -> Vec<Effect> {
        let mut effects = vec![];
        let result = match event {
            ControllerEvent::SourceReadFailed {
                source,
                revision,
                error,
            } => {
                if self.source == Some(source)
                    && self
                        .file
                        .as_ref()
                        .is_some_and(|file| file.revision == revision)
                {
                    self.error = Some(error.into());
                    effects.push(Effect::RequestRepaint);
                }
                Ok(())
            }
            ControllerEvent::SystemThemeChanged(theme) => {
                if self.initial.theme != theme {
                    self.initial.theme = theme;
                    effects.push(Effect::RequestRepaint);
                }
                Ok(())
            }
            ControllerEvent::Editor(message) => {
                if let EditorMessage::Closed { session }
                | EditorMessage::OpenFailed { session, .. } = &message
                {
                    if self
                        .template_dialog_return
                        .is_some_and(|r| r.session == *session)
                    {
                        self.template_dialog_return = None;
                    }
                }
                if matches!(
                    &message,
                    EditorMessage::Action {
                        action: crate::template::coordinator::EditorAction::Command(_),
                        ..
                    } | EditorMessage::CloseDecision {
                        decision: crate::template::coordinator::EditorCloseDecision::Save
                            | crate::template::coordinator::EditorCloseDecision::SaveAs,
                        ..
                    }
                ) {
                    self.template_owner = WindowKind::TemplateEditor;
                }
                let directives = self.editor.receive(message, &mut self.workspace);
                self.editor_directives(directives, &mut effects)
            }
            ControllerEvent::MainCloseRequested => self.request_main_close(&mut effects),
            ControllerEvent::Action(action) => self.action(action, &mut effects),
            ControllerEvent::Invoke { command, context } => {
                self.invoke(command, context, &mut effects)
            }
            ControllerEvent::CoreCompleted { stamp, reply } => {
                self.completed(stamp, reply, &mut effects)
            }
            ControllerEvent::Progress { stamp, progress } => {
                if self
                    .pending
                    .get(&stamp.request_id)
                    .is_some_and(|p| p.stamp == stamp && self.current(p))
                {
                    self.tracker.report(stamp.request_id, progress);
                }
                Ok(())
            }
            ControllerEvent::PickerCompleted { request_id, path } => {
                if self.csv_ui == Some(request_id) {
                    self.csv_ui = None;
                    if let Some(path) = path {
                        self.submit(
                            CoreRequest::ExportResultsCsv {
                                path,
                                template: self.workspace.definition.clone(),
                            },
                            &mut effects,
                        )
                    } else {
                        Ok(())
                    }
                } else if self.template_ui == Some(request_id) {
                    self.template_ui = None;
                    self.template_event(TemplateEvent::Picker(path), &mut effects)
                } else if self.binary_ui == Some(request_id) {
                    self.binary_ui = None;
                    self.binary(BinaryEvent::Picker(path), &mut effects)
                } else {
                    Ok(())
                }
            }
            ControllerEvent::DialogCompleted {
                request_id,
                decision,
            } => {
                if self.exit_dialog == Some(request_id) {
                    self.exit_dialog = None;
                    self.dialog = None;
                    let directives = self.editor.exit_decision(decision);
                    self.editor_directives(directives, &mut effects)
                } else if self.template_ui == Some(request_id) {
                    self.template_ui = None;
                    self.dialog = None;
                    self.finish_template_dialog(Some(request_id), &mut effects);
                    self.template_event(TemplateEvent::Decision(decision), &mut effects)
                } else if self.binary_ui == Some(request_id) {
                    self.binary_ui = None;
                    self.dialog = None;
                    self.binary(BinaryEvent::Dialog(decision), &mut effects)
                } else {
                    Ok(())
                }
            }
            ControllerEvent::PickerFailed { request_id, error } => {
                if self.csv_ui == Some(request_id) {
                    self.csv_ui = None;
                    self.error = Some(error.into());
                    Ok(())
                } else if self.template_ui == Some(request_id) {
                    self.template_ui = None;
                    // Preserve window ownership even when the OS picker fails
                    // before a core save starts (including close-save actions).
                    self.template_directives(vec![TemplateDirective::Error(error)], &mut effects)
                        .and_then(|_| {
                            self.template_event(TemplateEvent::Picker(None), &mut effects)
                        })
                } else if self.binary_ui == Some(request_id) {
                    self.binary_ui = None;
                    self.error = Some(error.into());
                    self.binary(BinaryEvent::Picker(None), &mut effects)
                } else {
                    Ok(())
                }
            }
        };
        if let Err(error) = result {
            self.error = Some(error.into());
        }
        if self.error.is_some() {
            self.completion_notice = None;
        }
        if let Err(error) = self.editor.publish() {
            self.error = Some(error.into());
        }
        effects.push(Effect::RequestRepaint);
        effects
    }
    pub fn policy(&self) -> PolicyState {
        PolicyState {
            has_file: self.file.is_some(),
            has_bytes: self.file.as_ref().is_some_and(|f| f.size != "0"),
            single_byte_selected: self.selection.as_ref().is_some_and(|s| s.count() == 1),
            edit_mode: self.edit_mode,
            can_undo: self.undo_depth > 0,
            template_valid: self.workspace.valid(),
            template_active: self.workspace.active,
            template_has_path: self.workspace.saved_path.is_some(),
            template_has_fields: !self.workspace.definition.fields.is_empty(),
            has_parsed_results: has_successful_leaves(&self.workspace.results)
                && !has_diagnostics(&self.workspace.results),
            operation_busy: self.tracker.visible_activity().is_some()
                || self.close_barrier
                || self.binary_workflow.busy()
                || self.workspace.busy()
                || self.csv_ui.is_some()
                || self.editor.main_blocked(),
            // Window-local modal/focus checks remain separate from this shared policy.
            has_search_matches: self.search.is_current() && !self.search.matches.is_empty(),
            template_issue_count: Some(
                u32::try_from(
                    crate::template::validation::collect_validation(
                        &self.workspace.definition,
                        &crate::template::validation::RawInputState::default(),
                    )
                    .len(),
                )
                .unwrap_or(u32::MAX),
            ),
        }
    }
    fn invoke(
        &mut self,
        command: Command,
        context: InvokeContext,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        if context.modal
            || context.composing
            || self.dialog.is_some()
            || self.edit_dialog.is_some()
            || !command_enabled(command, &self.policy())
        {
            return Ok(());
        }
        if command == Command::Exit && self.editor.main_closing {
            return Ok(());
        }
        if matches!(
            command,
            Command::ApplyTemplate
                | Command::LoadTemplate
                | Command::SaveTemplate
                | Command::SaveTemplateAs
                | Command::UnloadTemplate
        ) {
            self.template_owner = context.window.unwrap_or(WindowKind::Main);
        }
        if matches!(
            command,
            Command::ApplyTemplate
                | Command::LoadTemplate
                | Command::SaveTemplate
                | Command::SaveTemplateAs
                | Command::UnloadTemplate
                | Command::Exit
        ) {
            if command == Command::Exit {
                self.editor.begin_main_close();
            }
            if self.editor.session().is_some() {
                let id = self.tracker.reserve_id()?;
                let directives = self.editor.start_flush(
                    id,
                    FlushPurpose::MainCommand(command),
                    &mut self.workspace,
                );
                return self.editor_directives(directives, effects);
            }
        }
        self.invoke_after_flush(command, context, effects)
    }
    fn request_main_close(&mut self, effects: &mut Vec<Effect>) -> Result<(), AppError> {
        // Keep one close attempt and do not interrupt a binary picker/save or
        // a confirmation. Ordinary Exit commands retain their modal policy.
        if self.binary_workflow.busy()
            || self.editor.main_closing
            || self.editor.close_prompt
            || self.dialog.is_some()
        {
            return Ok(());
        }
        self.editor.begin_main_close();
        if self.editor.session().is_some() {
            let id = self.tracker.reserve_id()?;
            let directives =
                self.editor
                    .start_flush(id, FlushPurpose::MainClose, &mut self.workspace);
            self.editor_directives(directives, effects)
        } else {
            self.binary(BinaryEvent::Exit, effects)
        }
    }
    fn invoke_after_flush(
        &mut self,
        command: Command,
        context: InvokeContext,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        if context.modal
            || context.composing
            || self.dialog.is_some()
            || self.edit_dialog.is_some()
            || !command_enabled(command, &self.policy())
        {
            return Ok(());
        }
        use Command::*;
        match command {
            Open => self.binary(BinaryEvent::ChooseOpen, effects)?,
            SaveAs => self.binary(BinaryEvent::SaveCopy, effects)?,
            CloseFile => self.binary(BinaryEvent::CloseFile, effects)?,
            Exit => self.binary(BinaryEvent::Exit, effects)?,
            Export => {
                self.completion_notice = None;
                let request_id = self.tracker.reserve_id()?;
                self.csv_ui = Some(request_id);
                effects.push(Effect::OpenPicker {
                    request_id,
                    picker: PickerRequest {
                        purpose: crate::dialog::PickerPurpose::Csv,
                        title: "Export parsed results".into(),
                        default_path: Some(
                            format!(
                                "{}.csv",
                                if self.workspace.definition.name.is_empty() {
                                    "results"
                                } else {
                                    &self.workspace.definition.name
                                }
                            )
                            .into(),
                        ),
                        filters: vec![("CSV".into(), vec!["csv".into()])],
                        owner: WindowKind::Main,
                    },
                });
            }
            EditSelected => {
                if let Some(selection) = &self.selection {
                    self.edit_dialog = Some(ByteEditState {
                        offset: selection.start,
                        value: String::new(),
                        error: None,
                        pending: false,
                    });
                }
            }
            Search => {
                if !self.search.open {
                    self.search.raw = self.search.query.clone();
                    self.search.error = None;
                }
                self.goto.open = false;
                self.search.open = true;
                increment(&mut self.search.focus_key)?;
            }
            Goto => {
                if !self.goto.open {
                    self.goto.raw.clear();
                    self.goto.error = None;
                }
                self.search.open = false;
                self.goto.open = true;
                increment(&mut self.goto.focus_key)?;
            }
            SearchNext => self.navigate_search(MatchDirection::Next)?,
            SearchPrevious => self.navigate_search(MatchDirection::Previous)?,
            TemplateEditor => {
                let session = self.editor.open(&mut self.workspace)?;
                effects.push(Effect::OpenOrFocusEditor { session });
            }
            ToggleEdit => self.edit_mode = !self.edit_mode,
            Undo => self.submit(CoreRequest::UndoEdit, effects)?,
            ApplyTemplate => {
                self.template_event(TemplateEvent::Begin(TemplateAction::Apply), effects)?
            }
            LoadTemplate => {
                self.template_event(TemplateEvent::Begin(TemplateAction::Load), effects)?
            }
            SaveTemplate => {
                self.template_event(TemplateEvent::Begin(TemplateAction::Save), effects)?
            }
            SaveTemplateAs => {
                self.template_event(TemplateEvent::Begin(TemplateAction::SaveAs), effects)?
            }
            UnloadTemplate => {
                self.template_event(TemplateEvent::Begin(TemplateAction::Unload), effects)?
            }
            ThemeToggle => {
                self.initial.theme = if self.initial.theme == Theme::Dark {
                    Theme::Light
                } else {
                    Theme::Dark
                }
            }
            Row16 => self.initial.row_width = RowWidth::Sixteen,
            Row32 => self.initial.row_width = RowWidth::ThirtyTwo,
            MinimapToggle => self.initial.minimap.enabled = !self.initial.minimap.enabled,
            MinimapFit => self.initial.minimap.mode = MinimapMode::Fit,
            MinimapProportional => self.initial.minimap.mode = MinimapMode::Proportional,
            MinimapCharacters => self.initial.minimap.render = MinimapRender::Characters,
            MinimapBlocks => self.initial.minimap.render = MinimapRender::Blocks,
            MinimapScale1 => self.initial.minimap.scale = 1,
            MinimapScale2 => self.initial.minimap.scale = 2,
            MinimapScale3 => self.initial.minimap.scale = 3,
            _ => {} // Owning workflows are added in the following tested slices.
        }
        Ok(())
    }
    fn action(&mut self, action: Action, effects: &mut Vec<Effect>) -> Result<(), AppError> {
        // Window-local modals own their input, regardless of whether an event
        // came from a shortcut, pointer control, or a queued presenter action.
        // Accepted reads and editor draft updates are not background gestures.
        let modal = self.dialog.is_some()
            || self.edit_dialog.is_some()
            || self.error.is_some()
            || self.editor.close_prompt;
        if modal
            && matches!(
                &action,
                Action::Select { .. }
                    | Action::Navigate { .. }
                    | Action::NavigateTemplate { .. }
                    | Action::ScrollViewport { .. }
                    | Action::DropFiles(_)
                    | Action::SetSearchPattern(_)
                    | Action::SubmitSearch { .. }
                    | Action::ClearSearch
                    | Action::CloseSearch
                    | Action::SetGotoValue(_)
                    | Action::SubmitGoto
                    | Action::CloseGoto
                    | Action::ClearSelection
            )
        {
            return Ok(());
        }
        match action {
            Action::ScrollViewport { offset } => {
                if self.dialog.is_none() && !self.close_barrier {
                    if let Some(size) = self.file.as_ref().and_then(|f| f.size.parse::<u64>().ok())
                    {
                        let next = if size == 0 { 0 } else { offset.min(size - 1) };
                        if next != self.viewport_offset {
                            // Only a new read advances the request generation. A
                            // scroll inside an already requested page must still
                            // accept that covering read when it completes.
                            self.viewport_offset = next;
                        }
                    }
                }
            }
            Action::ReadViewport { offset, length } => {
                if let Some(size) = self.file.as_ref().and_then(|f| f.size.parse::<u64>().ok()) {
                    if offset < size
                        && length > 0
                        && u64::from(length) <= crate::hex::layout::MAX_READ_BYTES
                    {
                        let length = u64::from(length).min(size - offset) as u32;
                        self.submit(CoreRequest::ReadPage { offset, length }, effects)?;
                    }
                }
            }
            Action::Core(request) => {
                if let CoreRequest::SearchBytes { pattern } = &request {
                    self.search.set_pattern(pattern);
                }
                self.submit(request, effects)?;
            }
            Action::Navigate { start, end } => self.navigate(start, end)?,
            Action::NavigateTemplate { start, end } => {
                self.navigate(start, end)?;
                self.template_range = self.selection.clone();
            }
            Action::DropFiles(paths) => {
                if paths.len() != 1 {
                    self.error = Some(ControllerError::invalid_input(
                        "Drop exactly one file at a time.",
                    ));
                } else if command_enabled(Command::Open, &self.policy()) && self.dialog.is_none() {
                    self.binary(BinaryEvent::OpenPath(paths[0].clone()), effects)?;
                }
            }
            Action::SetSearchPattern(raw) => self.search.set_pattern(&raw),
            Action::SubmitSearch { force, direction } => {
                if !self.policy().operation_busy
                    && self.file.as_ref().is_some_and(|f| f.size != "0")
                    && self.dialog.is_none()
                {
                    self.search.error = None;
                    if !force && self.search.is_current() && !self.search.matches.is_empty() {
                        self.navigate_search(direction)?;
                    } else {
                        match parse_search_pattern(&self.search.raw) {
                            Ok(_) => self.submit(
                                CoreRequest::SearchBytes {
                                    pattern: self.search.raw.clone(),
                                },
                                effects,
                            )?,
                            Err(error) => {
                                self.search.error = Some(error.message.clone());
                                self.error = Some(error);
                            }
                        }
                    }
                }
            }
            Action::ClearSearch => {
                if !self.tracker.is_busy(OperationKind::Search) {
                    self.search.clear();
                }
            }
            Action::CloseSearch => self.search.open = false,
            Action::SetGotoValue(raw) => self.goto.raw = raw,
            Action::SubmitGoto => {
                // An already-open non-modal Go To bar is still navigation,
                // not a new operation. The reference permits it during a scan.
                if self.dialog.is_none() && self.edit_dialog.is_none() && !self.close_barrier {
                    let result = self.file.as_ref().map_or_else(
                        || {
                            Err(ControllerError::invalid_input(
                                "Open a file before navigating.",
                            ))
                        },
                        |f| parse_goto(&self.goto.raw, f.size.parse().unwrap_or(0)),
                    );
                    match result {
                        Ok(offset) => {
                            self.navigate(offset, offset)?;
                            self.goto.open = false;
                            self.goto.error = None;
                            self.error = None;
                        }
                        Err(error) => {
                            self.goto.error = Some(error.message);
                            self.error = None;
                        }
                    }
                }
            }
            Action::CloseGoto => self.goto.open = false,
            Action::EditSelectedByte(raw) => {
                if command_enabled(Command::EditSelected, &self.policy()) {
                    match parse_edit_byte(&raw) {
                        Ok(value) => {
                            if let Some(selected) = &self.selection {
                                self.submit(
                                    CoreRequest::EditByte {
                                        offset: selected.start,
                                        value,
                                    },
                                    effects,
                                )?;
                                if let Some(dialog) = &mut self.edit_dialog {
                                    dialog.pending = true;
                                    dialog.error = None;
                                    self.edit_dialog_request = effects.iter().rev().find_map(|e| {
                                        if let Effect::RunCore {
                                            stamp,
                                            request: CoreRequest::EditByte { .. },
                                        } = e
                                        {
                                            Some(stamp.request_id)
                                        } else {
                                            None
                                        }
                                    });
                                }
                            }
                        }
                        Err(error) => {
                            if let Some(dialog) = &mut self.edit_dialog {
                                dialog.error = Some(error.message);
                            } else {
                                self.error = Some(error)
                            }
                        }
                    }
                }
            }
            Action::SetEditValue(value) => {
                if let Some(dialog) = &mut self.edit_dialog {
                    if !dialog.pending {
                        dialog.value = value;
                    }
                }
            }
            Action::CancelEdit => {
                self.edit_dialog = None;
                self.edit_dialog_request = None;
                self.error = None;
            }
            Action::Select { start, end } => {
                if let Some(file) = &self.file {
                    let size: u64 = file.size.parse().map_err(|_| invalid_reply())?;
                    if start < size && end < size {
                        increment(&mut self.selection_revision)?;
                        self.template_range = None;
                        self.selection = Some(ByteSelection {
                            start: start.min(end),
                            end: start.max(end),
                        });
                    }
                }
            }
            Action::ClearSelection => {
                increment(&mut self.selection_revision)?;
                self.selection = None;
                self.template_range = None;
            }
            Action::ClearError => self.error = None,
            Action::ExpireNotice { revision } => {
                if revision == self.notice_revision {
                    self.completion_notice = None;
                }
            }
            Action::TemplateChanged(definition) => {
                self.template_event(
                    TemplateEvent::Draft {
                        definition,
                        valid: true,
                    },
                    effects,
                )?;
            }
            Action::TemplateDraft { definition, valid } => {
                self.template_event(TemplateEvent::Draft { definition, valid }, effects)?
            }
            Action::PrepareClose => {
                self.close_barrier = true;
                self.close_query_pending = true;
                self.maybe_query_close(effects)?;
            }
            Action::ReleaseCloseBarrier => {
                self.close_barrier = false;
                self.close_query_pending = false;
            }
        }
        Ok(())
    }
    fn submit(&mut self, request: CoreRequest, effects: &mut Vec<Effect>) -> Result<(), AppError> {
        self.submit_with_close(request, false, effects)
    }
    fn submit_with_close(
        &mut self,
        request: CoreRequest,
        for_close: bool,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        self.submit_operation(request, for_close, true, effects)
    }
    fn submit_operation(
        &mut self,
        request: CoreRequest,
        for_close: bool,
        viewport_request: bool,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        let approved_close_save = for_close
            && self.close_barrier
            && self.mutation_running.is_none()
            && self.mutation_queue.is_empty()
            && matches!(request, CoreRequest::SaveBinaryAs { .. });
        if (self.close_barrier && is_mutation(&request) && !approved_close_save)
            || (for_close && !approved_close_save)
        {
            return Err(AppError::new(
                ErrorCode::OperationFailed,
                "Window close is in progress.",
                None,
            ));
        }
        let kind = OperationKind::for_request(&request);
        // Check epochs before allocating or emitting work: no wrapping identity
        // can make an old completion look current.
        if kind == OperationKind::Page && viewport_request {
            increment(&mut self.viewport_intent)?;
        }
        if matches!(request, CoreRequest::OpenFile { .. }) {
            increment(&mut self.content_revision)?;
        }
        let id = self.tracker.begin(kind)?;
        let stamp = RequestStamp {
            request_id: id,
            source: self.source,
            content_revision: Some(self.content_revision),
            workspace_revision: self.workspace.workspace_revision,
            definition_revision: self.workspace.definition_revision,
            persistence_revision: self.workspace.persistence_revision,
            viewport_intent: self.viewport_intent,
        };
        self.latest.insert(kind, id);
        if kind.visible() {
            self.latest_error_intent = id.0;
            self.error = None;
        }
        let pending = Pending {
            stamp,
            request,
            selection_revision: self.selection_revision,
            page_generation: if viewport_request {
                self.viewport_intent
            } else {
                self.page
                    .as_ref()
                    .map_or(self.viewport_intent, |p| p.generation)
            },
            workspace_binding: self.workspace.workspace_revision,
        };
        if is_mutation(&pending.request) && self.mutation_running.is_some() {
            self.mutation_queue.push_back(pending);
        } else {
            self.start(pending, effects);
        }
        Ok(())
    }
    fn start(&mut self, mut pending: Pending, effects: &mut Vec<Effect>) {
        // A queued mutation executes in the state left by its predecessor.
        if is_mutation(&pending.request) {
            pending.stamp.source = self.source;
            pending.stamp.content_revision = Some(self.content_revision);
            self.mutation_running = Some(pending.stamp.request_id);
        }
        effects.push(Effect::RunCore {
            stamp: pending.stamp.clone(),
            request: pending.request.clone(),
        });
        self.pending.insert(pending.stamp.request_id, pending);
    }
    fn current(&self, p: &Pending) -> bool {
        let kind = OperationKind::for_request(&p.request);
        if matches!(kind, OperationKind::Open) {
            return true;
        }
        let source_current = p.stamp.source == self.source;
        if is_mutation(&p.request) && kind != OperationKind::Template {
            return source_current;
        }
        let newest = self.latest.get(&kind) == Some(&p.stamp.request_id);
        match kind {
            OperationKind::Search => {
                source_current && newest && p.stamp.content_revision == Some(self.content_revision)
            }
            OperationKind::Parse => {
                source_current
                    && newest
                    && p.stamp.content_revision == Some(self.content_revision)
                    && p.stamp.workspace_revision == self.workspace.workspace_revision
                    && p.stamp.definition_revision == self.workspace.definition_revision
            }
            OperationKind::Page => {
                source_current && newest && p.stamp.viewport_intent == self.viewport_intent
            }
            OperationKind::Overview | OperationKind::Minimap => {
                source_current && newest && p.stamp.content_revision == Some(self.content_revision)
            }
            OperationKind::Template => {
                newest && p.workspace_binding == self.workspace.workspace_revision
            }
            _ => source_current && newest,
        }
    }
    fn completed(
        &mut self,
        stamp: RequestStamp,
        reply: Result<CoreReply, AppError>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        // Unknown/duplicate/mismatched replies are not allowed to settle a
        // different operation or change its busy counters.
        let Some(pending) = self.pending.get(&stamp.request_id) else {
            return Ok(());
        };
        if pending.stamp != stamp {
            return Ok(());
        }
        let accepted = self.current(pending);
        let binary_owned = self.binary_core == Some(stamp.request_id);
        let template_owned = self.template_core == Some(stamp.request_id);
        let core_success = reply.is_ok();
        if binary_owned && accepted && matches!(pending.request, CoreRequest::GetDirtyState) {
            if let Ok(CoreReply::Dirty(state)) = &reply {
                self.editor.note_exit_dirty(state.dirty);
            }
        }
        let pending = self
            .pending
            .remove(&stamp.request_id)
            .expect("checked pending");
        self.tracker.finish(stamp.request_id);
        if self.mutation_running == Some(stamp.request_id) {
            self.mutation_running = None;
        }
        let mut result = Ok(());
        if template_owned {
            self.template_core = None;
            let template_reply = if !accepted {
                Ok(TemplateReply::Ignored)
            } else {
                reply.map(|reply| match reply {
                    CoreReply::Template(definition) => TemplateReply::Loaded(definition),
                    CoreReply::Parsed(nodes) => TemplateReply::Parsed(nodes),
                    CoreReply::Unit => TemplateReply::Unit,
                    _ => TemplateReply::Ignored,
                })
            };
            result = self.template_event(TemplateEvent::CoreFinished(template_reply), effects);
        } else if accepted {
            match reply {
                Ok(reply) => result = self.install(&pending, reply),
                Err(error) => {
                    if stamp.request_id.0 >= self.latest_error_intent {
                        if matches!(pending.request, CoreRequest::SearchBytes { .. }) {
                            self.search.error = Some(error.message.clone());
                        }
                        self.error = Some(error.into());
                    }
                    if matches!(pending.request, CoreRequest::GetDirtyState) {
                        self.close_barrier = false;
                    }
                }
            }
        }
        if accepted
            && core_success
            && result.is_ok()
            && matches!(
                pending.request,
                CoreRequest::EditByte { .. } | CoreRequest::UndoEdit
            )
        {
            self.refresh_requested = self
                .source
                .map(|source| (source, pending.stamp.viewport_intent));
        }
        if binary_owned {
            self.binary_core = None;
            self.binary(
                BinaryEvent::CoreFinished {
                    success: accepted && core_success && result.is_ok(),
                },
                effects,
            )?;
        }
        if self.edit_dialog_request == Some(stamp.request_id) {
            self.edit_dialog_request = None;
            if accepted && core_success && result.is_ok() {
                self.edit_dialog = None;
            } else if let Some(dialog) = &mut self.edit_dialog {
                dialog.pending = false;
                dialog.error = self.error.as_ref().map(|e| e.message.clone());
            }
        }
        if self.mutation_running.is_none() {
            if let Some(next) = self.mutation_queue.pop_front() {
                self.start(next, effects);
            }
        }
        self.maybe_query_close(effects)?;
        self.flush_edit_refresh(effects)?;
        result
    }
    fn install(&mut self, pending: &Pending, reply: CoreReply) -> Result<(), AppError> {
        match (&pending.request, reply) {
            (CoreRequest::OpenFile { .. }, CoreReply::FileInfo(file)) => {
                self.edit_dialog = None;
                let identity = SourceIdentity(self.source_ids.next_id()?);
                self.source = Some(identity);
                self.file = Some(file);
                self.page = None;
                self.selection = None;
                self.viewport_offset = 0;
                self.undo_depth = 0;
                self.last_edit_delta = None;
                self.modified_overview.bins.clear();
                self.invalidate_content()?;
                self.workspace.needs_refresh = false;
                self.template_range = None;
            }
            (CoreRequest::CloseFile { .. }, CoreReply::Unit) => {
                self.edit_dialog = None;
                self.source_ids.next_id()?;
                self.source = None;
                self.file = None;
                self.page = None;
                self.selection = None;
                self.viewport_offset = 0;
                self.undo_depth = 0;
                self.edit_mode = false;
                self.last_edit_delta = None;
                self.modified_overview.bins.clear();
                self.invalidate_content()?;
                self.workspace.needs_refresh = false;
                self.template_range = None;
            }
            (CoreRequest::ReadPage { .. }, CoreReply::Page(data)) => {
                self.page = Some(ViewportPage {
                    data,
                    generation: pending.page_generation,
                })
            }
            (CoreRequest::SearchBytes { pattern }, CoreReply::Search(found)) => {
                let matches = found
                    .matches
                    .iter()
                    .map(|s| s.parse::<u64>().map_err(|_| invalid_reply()))
                    .collect::<Result<_, _>>()?;
                let length = parse_search_pattern(pattern)
                    .map_err(|_| invalid_reply())?
                    .len() as u64;
                self.search
                    .install(pattern, matches, length, found.truncated);
                if self.selection_revision == pending.selection_revision {
                    if let Some(start) = self.search.matches.first().copied() {
                        let end = start.checked_add(length - 1).ok_or_else(invalid_reply)?;
                        self.navigate(start, end)?;
                    }
                }
            }
            (CoreRequest::ApplyTemplate { .. }, CoreReply::Parsed(nodes)) => {
                self.workspace.install_results(nodes);
            }
            (CoreRequest::EditByte { offset, .. }, CoreReply::Dirty(state)) => {
                if self
                    .file
                    .as_ref()
                    .is_some_and(|f| f.revision != state.revision)
                {
                    increment(&mut self.undo_depth)?;
                    self.last_edit_delta = Some((*offset, state.revision.clone()));
                }
                self.invalidate_content()?;
                self.update_dirty(state);
            }
            (CoreRequest::UndoEdit, CoreReply::Undo(undo)) => {
                if undo.undone {
                    self.undo_depth = self.undo_depth.saturating_sub(1);
                    self.invalidate_content()?;
                    if let Some(offset) = undo.offset {
                        self.last_edit_delta = Some((
                            offset.parse().map_err(|_| invalid_reply())?,
                            undo.state.revision.clone(),
                        ));
                    }
                }
                self.update_dirty(undo.state);
            }
            (CoreRequest::SaveBinaryAs { .. }, CoreReply::BinarySaved(saved)) => {
                self.source = Some(SourceIdentity(self.source_ids.next_id()?));
                self.file = Some(saved.file);
                self.page = None;
                self.undo_depth = 0;
                self.last_edit_delta = None;
                self.modified_overview.bins.clear();
                self.invalidate_content()?;
            }
            (CoreRequest::GetDirtyState, CoreReply::Dirty(state)) => self.update_dirty(state),
            (CoreRequest::GetFileInfo, CoreReply::FileInfo(info)) => self.file = Some(info),
            (CoreRequest::GetModifiedOverview, CoreReply::ModifiedOverview(overview)) => {
                self.modified_overview = overview
            }
            (CoreRequest::LoadTemplate { path }, CoreReply::Template(template)) => {
                // Trusted raw requests still settle through the same workspace.
                self.workspace
                    .handle(TemplateEvent::Begin(TemplateAction::Load));
                self.workspace
                    .handle(TemplateEvent::Picker(Some(path.clone())));
                self.workspace
                    .handle(TemplateEvent::Decision(DialogDecision::Confirm));
                self.workspace
                    .handle(TemplateEvent::CoreFinished(Ok(TemplateReply::Loaded(
                        template,
                    ))));
                self.clear_template_highlight()?;
            }
            (CoreRequest::UnloadTemplate, CoreReply::Unit) => {
                self.workspace
                    .handle(TemplateEvent::Begin(TemplateAction::Unload));
                self.workspace
                    .handle(TemplateEvent::Decision(DialogDecision::Confirm));
                self.workspace
                    .handle(TemplateEvent::CoreFinished(Ok(TemplateReply::Unit)));
                self.clear_template_highlight()?;
            }
            (
                CoreRequest::SaveTemplate { .. } | CoreRequest::SaveTemplateAs { .. },
                CoreReply::Unit,
            ) => {}
            (CoreRequest::ExportResultsCsv { path, .. }, CoreReply::Unit) => {
                self.notice(format!(
                    "CSV exported: {}.",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ))?;
            }
            (CoreRequest::ReadMinimapSamples { .. }, CoreReply::MinimapSamples(_)) => {}
            _ => return Err(invalid_reply()),
        }
        Ok(())
    }
    fn notice(&mut self, text: String) -> Result<(), AppError> {
        increment(&mut self.notice_revision)?;
        self.completion_notice = Some(text);
        Ok(())
    }
    fn update_dirty(&mut self, state: DirtyState) {
        if let Some(file) = &mut self.file {
            file.dirty = state.dirty;
            file.revision = state.revision;
        }
    }
    fn invalidate_content(&mut self) -> Result<(), AppError> {
        increment(&mut self.content_revision)?;
        self.search.invalidate();
        self.invalidate_template(false);
        Ok(())
    }
    fn invalidate_template(&mut self, reset_refresh: bool) {
        self.workspace.invalidate(reset_refresh);
    }
    fn maybe_query_close(&mut self, effects: &mut Vec<Effect>) -> Result<(), AppError> {
        if self.close_query_pending
            && self.mutation_running.is_none()
            && self.mutation_queue.is_empty()
        {
            self.close_query_pending = false;
            if self.file.is_none() {
                if self.binary_workflow.awaiting_query() {
                    self.binary(BinaryEvent::CoreFinished { success: true }, effects)?;
                }
                return Ok(());
            }
            self.submit(CoreRequest::GetDirtyState, effects)?;
            if self.binary_workflow.awaiting_query() {
                self.binary_core = effects.iter().rev().find_map(|e| {
                    if let Effect::RunCore {
                        stamp,
                        request: CoreRequest::GetDirtyState,
                    } = e
                    {
                        Some(stamp.request_id)
                    } else {
                        None
                    }
                });
            }
        }
        Ok(())
    }
    fn navigate(&mut self, start: u64, end: u64) -> Result<(), AppError> {
        increment(&mut self.selection_revision)?;
        self.template_range = None;
        self.selection = Some(ByteSelection {
            start: start.min(end),
            end: start.max(end),
        });
        self.viewport_offset = start;
        Ok(())
    }
    fn clear_template_highlight(&mut self) -> Result<(), AppError> {
        if self.template_range.is_some() && self.selection == self.template_range {
            increment(&mut self.selection_revision)?;
            self.selection = None;
        }
        self.template_range = None;
        Ok(())
    }
    fn template_event(
        &mut self,
        event: TemplateEvent,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        if matches!(event, TemplateEvent::Begin(_) | TemplateEvent::Draft { .. }) {
            self.completion_notice = None;
        }
        let directives = self.workspace.handle(event);
        self.template_directives(directives, effects)
    }
    fn template_directives(
        &mut self,
        directives: Vec<TemplateDirective>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        for directive in directives {
            match directive {
                TemplateDirective::Core(request) => {
                    self.submit(request, effects)?;
                    self.template_core = effects.iter().rev().find_map(|e| {
                        if let Effect::RunCore { stamp, .. } = e {
                            Some(stamp.request_id)
                        } else {
                            None
                        }
                    });
                }
                TemplateDirective::Picker(mut picker) => {
                    let request_id = self.tracker.reserve_id()?;
                    // Platform ownership and action/error origin are distinct.
                    // Companion requests use the main window's native dialogs.
                    picker.owner = WindowKind::Main;
                    self.begin_template_dialog(TemplateDialogEnd::Operation, effects);
                    self.template_ui = Some(request_id);
                    effects.push(Effect::OpenPicker { request_id, picker });
                }
                TemplateDirective::Dialog(mut dialog) => {
                    self.error = None;
                    let request_id = self.tracker.reserve_id()?;
                    dialog.owner = WindowKind::Main;
                    self.begin_template_dialog(TemplateDialogEnd::Decision(request_id), effects);
                    self.template_ui = Some(request_id);
                    self.dialog = Some((request_id, dialog.clone()));
                    effects.push(Effect::ShowDialog { request_id, dialog });
                }
                TemplateDirective::ClearHighlight => self.clear_template_highlight()?,
                TemplateDirective::Notice(notice) => self.notice(notice)?,
                TemplateDirective::Error(error) => {
                    if self.template_owner == WindowKind::TemplateEditor {
                        if let Some(session) = self.editor.session() {
                            effects.push(Effect::EditorError {
                                session,
                                message: error.message.clone(),
                            });
                        }
                    }
                    self.error = Some(error.into());
                }
                TemplateDirective::Completed { success } => {
                    self.finish_template_dialog(None, effects);
                    let directives = self.editor.action_completed(success, &self.workspace);
                    self.editor_directives(directives, effects)?;
                }
            }
        }
        Ok(())
    }
    fn begin_template_dialog(&mut self, end: TemplateDialogEnd, effects: &mut Vec<Effect>) {
        if self.template_owner != WindowKind::TemplateEditor
            || self.template_dialog_return.is_some()
        {
            return;
        }
        if let Some(session) = self.editor.session() {
            self.template_dialog_return = Some(TemplateDialogReturn { session, end });
            effects.push(Effect::FocusWindow {
                window: WindowKind::Main,
                session: None,
            });
        }
    }
    fn finish_template_dialog(&mut self, decision: Option<RequestId>, effects: &mut Vec<Effect>) {
        let Some(pending) = self.template_dialog_return else {
            return;
        };
        // Load / Save As wrap picker, confirmation and core work. Standalone
        // overwrite/unload confirmations return focus as soon as the decision
        // arrives, before continuing the disk operation, matching the source.
        if decision.is_some()
            && !matches!(pending.end, TemplateDialogEnd::Decision(id) if Some(id) == decision)
        {
            return;
        }
        self.template_dialog_return = None;
        if self.editor.session() == Some(pending.session) && !self.editor.main_closing {
            effects.push(Effect::FocusWindow {
                window: WindowKind::TemplateEditor,
                session: Some(pending.session),
            });
        }
    }
    fn flush_edit_refresh(&mut self, effects: &mut Vec<Effect>) -> Result<(), AppError> {
        if self.mutation_running.is_some() || self.close_barrier {
            return Ok(());
        }
        let Some((source, intent)) = self.refresh_requested.take() else {
            return Ok(());
        };
        if self.source != Some(source) {
            return Ok(());
        }
        self.submit(CoreRequest::GetModifiedOverview, effects)?;
        if intent == self.viewport_intent {
            if let Some(page) = &self.page {
                if !page.bytes.is_empty() {
                    let offset = page.offset.parse().map_err(|_| invalid_reply())?;
                    let length = u32::try_from(page.bytes.len()).map_err(|_| invalid_reply())?;
                    self.submit_operation(
                        CoreRequest::ReadPage { offset, length },
                        false,
                        false,
                        effects,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn navigate_search(&mut self, direction: MatchDirection) -> Result<(), AppError> {
        if self.tracker.is_busy(OperationKind::Search) {
            return Ok(());
        }
        self.search
            .set_anchor(self.selection.clone(), self.viewport_offset);
        if let Some(range) = self.search.next(direction) {
            self.navigate(range.start, range.end)?;
        }
        Ok(())
    }
    fn binary(&mut self, event: BinaryEvent, effects: &mut Vec<Effect>) -> Result<(), AppError> {
        let state = BinaryState {
            name: self.file.as_ref().map(|f| f.name.clone()),
            has_file: self.file.is_some(),
            dirty: self.file.as_ref().is_some_and(|f| f.dirty),
        };
        let directives = self.binary_workflow.handle(event, &state);
        for directive in directives {
            match directive {
                BinaryDirective::Picker(picker) => {
                    let request_id = self.tracker.reserve_id()?;
                    self.binary_ui = Some(request_id);
                    effects.push(Effect::OpenPicker { request_id, picker });
                }
                BinaryDirective::Dialog(dialog) => {
                    let request_id = self.tracker.reserve_id()?;
                    self.binary_ui = Some(request_id);
                    self.dialog = Some((request_id, dialog.clone()));
                    effects.push(Effect::ShowDialog { request_id, dialog });
                }
                BinaryDirective::Core { request, for_close } => {
                    self.submit_with_close(request, for_close, effects)?;
                    self.binary_core = effects.iter().rev().find_map(|e| {
                        if let Effect::RunCore { stamp, .. } = e {
                            Some(stamp.request_id)
                        } else {
                            None
                        }
                    });
                }
                BinaryDirective::PrepareClose => {
                    self.close_barrier = true;
                    self.close_query_pending = true;
                    self.maybe_query_close(effects)?;
                }
                BinaryDirective::ReleaseClose => {
                    self.close_barrier = false;
                    self.close_query_pending = false;
                    self.editor.abort_exit();
                }
                BinaryDirective::ExitPrepared => {
                    effects.push(Effect::ExitPrepared);
                    if self.editor.after_binary_close(&self.workspace)
                        && self.editor.session().is_some()
                    {
                        let id = self.tracker.reserve_id()?;
                        let directives = self.editor.start_flush(
                            id,
                            FlushPurpose::ExitAfterBinary,
                            &mut self.workspace,
                        );
                        self.editor_directives(directives, effects)?;
                    } else {
                        let directives = self.editor.check_exit_template(&self.workspace);
                        self.editor_directives(directives, effects)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn editor_directives(
        &mut self,
        directives: Vec<EditorDirective>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AppError> {
        for directive in directives {
            match directive {
                EditorDirective::Workspace(directive) => {
                    self.template_directives(vec![directive], effects)?
                }
                EditorDirective::Execute(command) => {
                    // Flush/action acceptance precedes the authoritative policy
                    // recheck. Disabled controls cannot bypass it through messages.
                    if !command_enabled(command, &self.policy()) {
                        let replies = self.editor.action_completed(false, &self.workspace);
                        self.editor_directives(replies, effects)?;
                    } else {
                        self.invoke_after_flush(command, InvokeContext::default(), effects)?;
                    }
                }
                EditorDirective::Navigate { start, end } => {
                    self.navigate(start, end)?;
                    self.template_range = self.selection.clone();
                }
                EditorDirective::Flush {
                    request_id,
                    session,
                } => effects.push(Effect::FlushEditorDraft {
                    request_id,
                    session,
                }),
                EditorDirective::RequestCloseFlush => {
                    let id = self.tracker.reserve_id()?;
                    let directives =
                        self.editor
                            .start_flush(id, FlushPurpose::EditorClose, &mut self.workspace);
                    self.editor_directives(directives, effects)?;
                }
                EditorDirective::ShowCloseDialog(dialog) => {
                    let request_id = self.tracker.reserve_id()?;
                    if dialog.owner == WindowKind::Main {
                        self.exit_dialog = Some(request_id);
                        self.dialog = Some((request_id, dialog.clone()));
                    }
                    effects.push(Effect::ShowDialog { request_id, dialog });
                }
                EditorDirective::CloseEditor(session) => effects.push(Effect::CloseWindow {
                    window: WindowKind::TemplateEditor,
                    session: Some(session),
                }),
                EditorDirective::CloseMain => effects.push(Effect::CloseWindow {
                    window: WindowKind::Main,
                    session: None,
                }),
                EditorDirective::ContinueMainClose => {
                    self.binary(BinaryEvent::Exit, effects)?;
                }
                EditorDirective::AbortExit => {
                    self.editor.abort_exit();
                    self.binary_workflow.reset();
                    self.close_barrier = false;
                    self.close_query_pending = false;
                }
                EditorDirective::Reply {
                    request_id,
                    success,
                } => {
                    if let Some(session) = self.editor.session() {
                        effects.push(Effect::EditorReply {
                            session,
                            request_id,
                            success,
                        });
                    }
                }
                EditorDirective::Error(error) => self.error = Some(error),
            }
        }
        Ok(())
    }
}
fn invalid_reply() -> AppError {
    AppError::new(
        ErrorCode::OperationFailed,
        "The operation could not be completed.",
        None,
    )
}
fn has_successful_leaves(nodes: &[ParsedNode]) -> bool {
    nodes.iter().any(|n| {
        (n.kind == "leaf" && n.diagnostics.is_empty()) || has_successful_leaves(&n.children)
    })
}
fn has_diagnostics(nodes: &[ParsedNode]) -> bool {
    nodes
        .iter()
        .any(|n| !n.diagnostics.is_empty() || has_diagnostics(&n.children))
}
