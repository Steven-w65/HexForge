use crate::{
    drawing::cache::HexDrawCache,
    platform::{
        dialogs::PlatformDialogs,
        windows::{self, NativeWindowHandle, WindowRegistry},
    },
    runtime::Runtime,
    style::HexForgeStyle,
    views::hex_view::HexView,
};
use eframe::egui;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{
        AppController, ControllerEvent, EditorSnapshot, Effect, InitialState, InvokeContext,
        MainSnapshot, WindowSnapshot,
    },
    dialog::DialogRequest,
    identity::RequestId,
    shortcuts::WindowKind,
    template::{
        coordinator::{EditorBuffer, EditorEnvelope, EditorMessage, EditorSessionId},
        editing::EditorModel,
    },
};
use hexforge_core::operations::CoreService;
use std::{collections::VecDeque, sync::Arc};
pub struct LocalEditor {
    pub session: EditorSessionId,
    pub buffer: EditorBuffer,
    pub model: EditorModel,
    pub error: Option<String>,
    pub workspace_revision: u64,
    pub controls: crate::views::field_card::EditorControlState,
    action_ids: hexforge_controller::identity::CheckedSequence,
}
pub struct NativeApp {
    controller: AppController,
    pub runtime: Runtime,
    pub windows: WindowRegistry,
    pub editor: Option<LocalEditor>,
    pub dialogs: Vec<(RequestId, DialogRequest)>,
    pub picker: Option<(RequestId, hexforge_controller::dialog::PickerRequest)>,
    pub editor_handle: Option<NativeWindowHandle>,
    context: Option<egui::Context>,
    editor_presented: bool,
    focus_editor: bool,
    first_frame: bool,
    hex_view: HexView,
    draw_cache: HexDrawCache,
    main_controls: crate::views::main_window::MainControls,
    notice_clock: hexforge_controller::preferences::NoticeClock,
    system_theme: crate::platform::theme::SystemThemeSource,
    preference_store: Option<Arc<dyn crate::platform::preferences::PreferenceStore>>,
    preference_snapshot: crate::platform::preferences::ResultsPreferences,
    startup: crate::startup::diagnostics::StartupDiagnostics,
}
fn envelope(s: EditorSnapshot) -> EditorEnvelope {
    EditorEnvelope {
        session: s.session.expect("editor snapshot has session"),
        revision: s.snapshot_revision,
        acknowledged_sequence: s.acknowledged_sequence,
        workspace: s.template_state,
    }
}
impl NativeApp {
    pub fn new(initial: InitialState, core: Arc<CoreService>) -> Self {
        Self::with_startup_diagnostics(initial, core, Default::default())
    }
    pub fn with_startup_diagnostics(
        initial: InitialState,
        core: Arc<CoreService>,
        startup: crate::startup::diagnostics::StartupDiagnostics,
    ) -> Self {
        let system_theme = crate::platform::theme::SystemThemeSource::from_initial(initial.theme);
        let mut app = Self {
            controller: AppController::new(initial),
            runtime: Runtime::new(core),
            windows: WindowRegistry::default(),
            editor: None,
            dialogs: vec![],
            picker: None,
            editor_handle: None,
            context: None,
            editor_presented: false,
            focus_editor: false,
            first_frame: true,
            hex_view: HexView::default(),
            draw_cache: HexDrawCache::default(),
            main_controls: crate::views::main_window::MainControls::default(),
            notice_clock: hexforge_controller::preferences::NoticeClock::default(),
            system_theme,
            preference_store: None,
            preference_snapshot: Default::default(),
            startup,
        };
        app.startup
            .mark(crate::startup::trace::StartupPhase::ControllerReady);
        app
    }
    pub fn startup_report(&self) -> Option<crate::startup::diagnostics::NativeStartupReport> {
        self.startup.report()
    }
    pub fn attach(&mut self, ctx: &egui::Context) {
        crate::style::fonts::install(ctx);
        self.startup
            .mark(crate::startup::trace::StartupPhase::FontAssetsReady);
        crate::widgets::dialog::install_focus(ctx);
        HexForgeStyle::for_theme(self.main_snapshot().theme, ctx.pixels_per_point()).apply(ctx);
        self.context = Some(ctx.clone());
        let ctx = ctx.clone();
        self.runtime
            .set_waker(move || ctx.request_repaint_of(egui::ViewportId::ROOT));
    }
    /// Called before the first scene, so optional storage cannot cause a later
    /// panel jump. Failed storage is deliberately not an operation/error modal.
    pub fn configure_preferences(
        &mut self,
        store: Arc<dyn crate::platform::preferences::PreferenceStore>,
    ) {
        let values = store
            .read_results_preferences()
            .ok()
            .flatten()
            .unwrap_or_default()
            .normalized();
        self.main_controls.panel = hexforge_controller::preferences::PanelPreferences::restore(
            Some(values.height),
            values.collapsed,
        );
        self.preference_snapshot = values;
        self.preference_store = Some(store);
        self.startup
            .mark(crate::startup::trace::StartupPhase::PreferencesReady);
    }
    pub fn results_preferences(&self) -> crate::platform::preferences::ResultsPreferences {
        crate::platform::preferences::ResultsPreferences {
            height: self.main_controls.panel.preferred_height,
            collapsed: self.main_controls.panel.collapsed,
        }
    }
    fn persist_results_preferences(&mut self) {
        let values = self.results_preferences();
        if values != self.preference_snapshot {
            if let Some(store) = &self.preference_store {
                let _ = store.write_results_preferences(&values);
            }
            // Do not retry failed optional writes every frame. A later user
            // preference change can retry, just as the reference storage does.
            self.preference_snapshot = values;
        }
    }
    pub fn snapshot(&self, window: WindowKind) -> WindowSnapshot {
        self.controller.snapshot(window)
    }
    pub fn main_snapshot(&self) -> MainSnapshot {
        match self.snapshot(WindowKind::Main) {
            WindowSnapshot::Main(s) => s,
            _ => unreachable!(),
        }
    }
    /// Last measured viewport geometry for opt-in presentation diagnostics.
    /// File/selection authority remains in the controller, never this frame.
    pub fn hex_frame(&self) -> &crate::views::hex_view::HexFrame {
        self.hex_view.frame()
    }
    pub fn editor_snapshot(&self) -> EditorSnapshot {
        match self.snapshot(WindowKind::TemplateEditor) {
            WindowSnapshot::Editor(s) => s,
            _ => unreachable!(),
        }
    }
    fn dispatch(&mut self, event: ControllerEvent) {
        // Opt-in interaction proof is an actual accepted controller transition,
        // not a predicted policy outcome, arbitrary event, or painted button.
        // The independent observer must still witness the real visible window.
        let theme_before = (self.startup.enabled()
            && matches!(&event,
            ControllerEvent::Invoke { command: Command::ThemeToggle, context }
                if context.window != Some(WindowKind::TemplateEditor)))
        .then(|| self.main_snapshot().theme);
        let effects = self.controller.handle(event);
        if theme_before.is_some_and(|theme| self.main_snapshot().theme != theme) {
            self.startup
                .mark(crate::startup::trace::StartupPhase::MainDispatchAccepted);
        }
        for e in effects {
            self.runtime.submit(e);
        }
        self.platform_effects();
        self.synchronize_editor();
    }
    pub fn invoke(&mut self, command: Command, mut context: InvokeContext) {
        // Errors are a presenter-owned main-window modal, not a global worker
        // failure barrier: the separate editor still owns its own input.
        if context.window != Some(WindowKind::TemplateEditor) {
            let s = self.main_snapshot();
            context.modal |= s.error.is_some() || s.editor_close_prompt;
        }
        if command == Command::ToggleRightPanel {
            let s = self.main_snapshot();
            if !context.modal
                && !context.composing
                && s.dialog.is_none()
                && s.edit_dialog.is_none()
                && s.error.is_none()
            {
                self.main_controls.panel.collapsed = !self.main_controls.panel.collapsed;
            }
        }
        self.dispatch(ControllerEvent::Invoke { command, context });
        self.persist_results_preferences();
    }
    /// Presenters cannot call the trusted internal raw-Core continuation.
    pub fn gesture(&mut self, action: Action) -> bool {
        if matches!(action, Action::Core(_)) {
            return false;
        }
        self.dispatch(ControllerEvent::Action(action));
        true
    }
    pub fn editor_message(&mut self, message: EditorMessage) {
        if let EditorMessage::OpenFailed { session, .. } = &message {
            // A renderer that did not create the child cannot later deliver
            // Closed. Retire only that session's platform record, without
            // treating failure as an approved editor/main-window exit.
            if self.windows.observe_editor_closed(*session) {
                self.editor = None;
                self.editor_handle = None;
                self.editor_presented = false;
                self.focus_editor = false;
            }
        }
        self.dispatch(ControllerEvent::Editor(message));
    }
    /// Present the session-owned native child. Window creation/destruction is
    /// separate from its editor controls and shared workspace authority.
    pub fn paint_editor_viewport(&mut self, ctx: &egui::Context) {
        if let Some(session) = self.windows.editor() {
            let id = windows::editor_viewport(session);
            if self.windows.editor_closing() {
                ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Close);
                let vanished = !self.editor_presented
                    || self.editor_handle.is_some_and(|h| !windows::is_window(h));
                if vanished {
                    self.editor_destroyed(session);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(10));
                }
            } else {
                let presented = windows::show_editor(ctx, session, |ui| self.paint_editor(ui));
                if presented {
                    self.editor_presented = true;
                    self.editor_handle = windows::find_owned_window("HexForge — Template Editor");
                    if self.focus_editor {
                        windows::focus_editor(ctx, session);
                        self.focus_editor = false;
                    }
                } else {
                    self.editor_message(EditorMessage::OpenFailed {
                        session,
                        message: "The Template Editor window could not be opened.".into(),
                    });
                }
            }
        }
    }
    pub fn pump(&mut self) {
        for event in self.runtime.drain_events() {
            self.dispatch(event);
        }
        self.platform_effects();
        for (ticket, result) in self.runtime.take_minimap_events() {
            if let Some(event) = self.hex_view.complete_minimap(ticket, result) {
                self.dispatch(event);
            }
        }
    }
    pub fn paint_main(&mut self, ui: &mut egui::Ui) {
        crate::platform::input::begin_frame(ui.ctx(), WindowKind::Main);
        // The OS supplies paths, not an in-memory copy of the dropped file.
        // The reducer owns one-file, busy, modal and unsaved-change protection.
        let paths = ui
            .input_mut(|i| std::mem::take(&mut i.raw.dropped_files))
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            self.gesture(Action::DropFiles(paths));
        }
        if let Some(theme) = ui.input(|i| i.raw.system_theme) {
            let theme = match theme {
                egui::Theme::Dark => hexforge_controller::controller::Theme::Dark,
                egui::Theme::Light => hexforge_controller::controller::Theme::Light,
            };
            if let Some(theme) = self.system_theme.observe(theme) {
                self.dispatch(ControllerEvent::SystemThemeChanged(theme));
            }
        }
        self.main_shortcuts(ui.ctx());
        let mut snapshot = self.main_snapshot();
        let now = ui.input(|i| i.time);
        if let Some(revision) = self.notice_clock.update(
            snapshot.notice_revision,
            snapshot.completion_notice.is_some(),
            now,
        ) {
            self.gesture(Action::ExpireNotice { revision });
            snapshot = self.main_snapshot();
        } else if let Some(delay) = self.notice_clock.remaining(now) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(delay));
        }
        let style = HexForgeStyle::for_theme(snapshot.theme, ui.ctx().pixels_per_point());
        style.apply(ui.ctx());
        let events = crate::views::main_window::show(
            ui,
            &style,
            &snapshot,
            &mut self.hex_view,
            &mut self.draw_cache,
            &mut self.main_controls,
        );
        for event in events {
            if let ControllerEvent::Invoke { command, context } = event {
                self.invoke(command, context);
            } else {
                self.dispatch(event);
            }
        }
        self.persist_results_preferences();
        // Main-page reads are submitted first. Only one generation-stamped
        // minimap sample job is allowed in flight; stale replies cannot paint.
        if let Some((ticket, request)) = self.hex_view.next_minimap_request() {
            self.runtime.submit_minimap(ticket, request);
        }
        // Shapes now contain the real controls. They have not yet been
        // tessellated/submitted/swapped/presented; no paint time is set here.
        self.startup
            .mark(crate::startup::trace::StartupPhase::MainSceneBuilt);
        self.startup
            .paint_submission_probe(ui.painter(), ui.max_rect());
    }
    fn main_shortcuts(&mut self, ctx: &egui::Context) {
        use hexforge_controller::shortcuts::{
            binding_for, InputContext, KeyChord, ShortcutOutcome,
        };
        let s = self.main_snapshot();
        let modal = s.dialog.is_some()
            || s.edit_dialog.is_some()
            || s.error.is_some()
            || s.editor_close_prompt;
        let input = InputContext {
            modal,
            text_input: ctx
                .memory(|m| m.focused())
                .is_some_and(|id| ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false))),
            ..Default::default()
        };
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                repeat,
                ..
            } = event
            {
                if key == egui::Key::Escape && self.main_controls.menu.is_open() {
                    continue;
                }
                let name = match key {
                    egui::Key::Num1 => "1".into(),
                    egui::Key::Num2 => "2".into(),
                    _ => format!("{key:?}").to_lowercase(),
                };
                let bits = u8::from(modifiers.ctrl)
                    | u8::from(modifiers.shift) * 2
                    | u8::from(modifiers.alt) * 4
                    | u8::from(modifiers.mac_cmd) * 8;
                let context = InputContext {
                    repeat,
                    composing: crate::platform::input::key_is_composing(
                        ctx,
                        WindowKind::Main,
                        key,
                        modifiers,
                    ),
                    ..input
                };
                if let Some(outcome) =
                    binding_for(WindowKind::Main, KeyChord::new(&name, bits), &context)
                {
                    ctx.input_mut(|i| i.consume_key(modifiers, key));
                    match outcome {
                        ShortcutOutcome::Invoke(command) => self.invoke(
                            command,
                            InvokeContext {
                                modal,
                                ..Default::default()
                            },
                        ),
                        ShortcutOutcome::Escape => {
                            let action = if s.goto.open {
                                Action::CloseGoto
                            } else if s.search.open {
                                Action::CloseSearch
                            } else {
                                Action::ClearSelection
                            };
                            self.gesture(action);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    fn synchronize_editor(&mut self) {
        let snapshot = self.editor_snapshot();
        if snapshot.session.is_none() {
            return;
        }
        if let Some(local) = self.editor.as_mut() {
            let next = envelope(snapshot);
            let revision = next.workspace.workspace_revision;
            let before = local.buffer.draft();
            let pending_controls = local.model.definition() != before.definition
                || local.model.raw.pending()
                || local.controls.pending_inputs();
            local.buffer.apply_snapshot(next);
            let draft = local.buffer.draft();
            if revision != local.workspace_revision {
                local.model.replace(draft.definition);
                local.controls = Default::default();
                local.workspace_revision = revision;
            } else if !pending_controls && local.model.definition() != draft.definition {
                // A focus/theme/worker acknowledgement must not replace an
                // editor control changed since its last published draft. An
                // approved workspace replacement above still resets it, and
                // the normal next editor publish flushes these newer inputs.
                local.model.replace(draft.definition);
            }
        }
    }
    pub fn publish_editor(&mut self) -> Result<(), hexforge_core::error::AppError> {
        if let Some(local) = self.editor.as_mut() {
            let draft = local.buffer.draft();
            let def = local.model.definition();
            let valid = local.model.valid();
            let pending = local.model.raw.pending() || local.controls.pending_inputs();
            if def != draft.definition || valid != draft.valid || pending != draft.pending_inputs {
                local.buffer.edit(def, valid, pending)?;
                let draft = local.buffer.draft();
                self.dispatch(ControllerEvent::Editor(EditorMessage::Draft(draft)));
            }
        }
        Ok(())
    }
    fn flush_editor_inputs(&mut self) {
        if let Some(local) = self.editor.as_mut() {
            for input in local.controls.take_buffered_inputs(&local.model) {
                if let crate::views::field_card::FieldEvent::Input {
                    id,
                    path,
                    control,
                    index,
                    text,
                } = input
                {
                    let _ = crate::views::field_card::apply_input(
                        &mut local.model,
                        id,
                        path,
                        &control,
                        index,
                        &text,
                    );
                }
            }
        }
    }
    /// Widget edits are applied and published before any save/apply/close
    /// event. Both windows therefore observe the same latest accepted draft.
    pub fn paint_editor(&mut self, ui: &mut egui::Ui) {
        crate::platform::input::begin_frame(ui.ctx(), WindowKind::TemplateEditor);
        use crate::views::{
            field_card::{apply_input, FieldEvent},
            template_editor::{self, EditorUiEvent},
        };
        use hexforge_controller::{
            policy::command_enabled,
            shortcuts::{binding_for, InputContext, KeyChord, ShortcutOutcome},
            template::coordinator::EditorAction,
        };
        let snapshot = self.editor_snapshot();
        let Some(session) = snapshot.session else {
            return;
        };
        let Some(local) = self.editor.as_mut().filter(|e| e.session == session) else {
            return;
        };
        local.controls.reconcile(&local.model);
        let style = HexForgeStyle::for_theme(snapshot.theme, ui.ctx().pixels_per_point());
        style.apply(ui.ctx());
        let workflow = self
            .main_snapshot()
            .dialog
            .filter(|(_, d)| d.owner == WindowKind::TemplateEditor);
        let local = self.editor.as_mut().unwrap();
        let input = InputContext {
            modal: snapshot.close_prompt
                || local.controls.confirmation.is_some()
                || workflow.is_some()
                || local.error.is_some(),
            editor_blocked: snapshot.busy || self.windows.editor_closing(),
            editor_error: local.error.is_some(),
            ..Default::default()
        };
        let mut shortcuts = vec![];
        let mut close = false;
        for event in ui.input(|i| i.events.clone()) {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                repeat,
                ..
            } = event
            {
                let bits = u8::from(modifiers.ctrl)
                    | u8::from(modifiers.shift) * 2
                    | u8::from(modifiers.alt) * 4
                    | u8::from(modifiers.mac_cmd) * 8;
                let context = InputContext {
                    repeat,
                    composing: crate::platform::input::key_is_composing(
                        ui.ctx(),
                        WindowKind::TemplateEditor,
                        key,
                        modifiers,
                    ),
                    ..input
                };
                if let Some(outcome) = binding_for(
                    WindowKind::TemplateEditor,
                    KeyChord::new(&format!("{key:?}").to_lowercase(), bits),
                    &context,
                ) {
                    ui.input_mut(|i| i.consume_key(modifiers, key));
                    match outcome {
                        ShortcutOutcome::Invoke(command) => {
                            shortcuts.push(EditorUiEvent::Action(EditorAction::Command(command)))
                        }
                        ShortcutOutcome::CloseEditor => close = true,
                        ShortcutOutcome::DismissEditorError => {
                            shortcuts.push(EditorUiEvent::ClearError)
                        }
                        _ => {}
                    }
                }
            }
        }
        let local = self.editor.as_mut().unwrap();
        let mut events = template_editor::show_template_editor(
            ui,
            &style,
            &snapshot,
            &local.model,
            &mut local.controls,
            local.error.as_deref(),
            workflow.as_ref().map(|(id, d)| (*id, d)),
        );
        events.extend(shortcuts);
        for event in events {
            match event {
                EditorUiEvent::Name(name) => {
                    let local = self.editor.as_mut().unwrap();
                    local.model.cancel();
                    local.model.name = name;
                }
                EditorUiEvent::Endian(endian) => {
                    let local = self.editor.as_mut().unwrap();
                    local.model.cancel();
                    local.model.default_endianness = endian;
                }
                EditorUiEvent::Field(field) => {
                    let local = self.editor.as_mut().unwrap();
                    let result = match field {
                        FieldEvent::Edit(edit) => local.model.edit(edit),
                        FieldEvent::Input {
                            id,
                            path,
                            control,
                            index,
                            text,
                        } => apply_input(&mut local.model, id, path, &control, index, &text),
                        FieldEvent::Navigate { start, end } => {
                            self.editor_action(EditorAction::Navigate { start, end });
                            continue;
                        }
                    };
                    if let Ok(outcome) = result {
                        local.controls.confirmation = if outcome.confirmation.is_some() {
                            Some(outcome)
                        } else {
                            None
                        };
                    }
                }
                EditorUiEvent::Confirm => {
                    let local = self.editor.as_mut().unwrap();
                    match local.model.confirm() {
                        Ok(outcome) => {
                            local.controls.confirmation = if outcome.confirmation.is_some() {
                                Some(outcome)
                            } else {
                                None
                            }
                        }
                        Err(error) => {
                            local.controls.confirmation = None;
                            local.error = Some(error.0);
                        }
                    }
                }
                EditorUiEvent::CancelConfirm => {
                    let local = self.editor.as_mut().unwrap();
                    local.model.cancel();
                    local.controls.confirmation = None;
                }
                EditorUiEvent::Action(action) => {
                    self.flush_editor_inputs();
                    if let EditorAction::Command(command) = &action {
                        let local = self.editor.as_ref().unwrap();
                        if !command_enabled(
                            *command,
                            &template_editor::editor_policy(&self.editor_snapshot(), &local.model),
                        ) {
                            continue;
                        }
                    }
                    self.editor_action(action);
                }
                EditorUiEvent::CloseDecision(decision) => {
                    if self.publish_editor().is_ok() {
                        self.editor_message(EditorMessage::CloseDecision { session, decision });
                    }
                }
                EditorUiEvent::DialogDecision(request_id, decision) => {
                    self.dispatch(ControllerEvent::DialogCompleted {
                        request_id,
                        decision,
                    })
                }
                EditorUiEvent::ClearError => {
                    if let Some(local) = self.editor.as_mut() {
                        local.error = None;
                    }
                    self.gesture(Action::ClearError);
                }
            }
        }
        if let Err(error) = self.publish_editor() {
            if let Some(local) = self.editor.as_mut() {
                local.error = Some(error.message);
            }
        }
        if close || ui.input(|i| i.viewport().close_requested()) {
            self.flush_editor_inputs();
            if let Err(error) = self.publish_editor() {
                if let Some(local) = self.editor.as_mut() {
                    local.error = Some(error.message);
                }
                return;
            }
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.editor_message(EditorMessage::CloseRequested { session });
        }
    }
    fn editor_action(&mut self, action: hexforge_controller::template::coordinator::EditorAction) {
        self.flush_editor_inputs();
        if let Err(error) = self.publish_editor() {
            if let Some(local) = self.editor.as_mut() {
                local.error = Some(error.message);
            }
            return;
        }
        let Some(local) = self.editor.as_mut() else {
            return;
        };
        let id = match local.action_ids.next_id() {
            Ok(id) => RequestId(id),
            Err(error) => {
                local.error = Some(error.message);
                return;
            }
        };
        let discard = matches!(
            action,
            hexforge_controller::template::coordinator::EditorAction::Discard
        );
        local.buffer.begin_action(id, discard);
        let draft = local.buffer.draft();
        self.editor_message(EditorMessage::Action {
            request_id: id,
            draft,
            action,
        });
    }
    fn platform_effects(&mut self) {
        let mut work: VecDeque<_> = self.runtime.take_platform_effects().into();
        while let Some(effect) = work.pop_front() {
            match effect {
                Effect::OpenOrFocusEditor { session } => {
                    self.windows.open_or_focus(session);
                    self.focus_editor = true;
                    if self.editor.as_ref().is_none_or(|e| e.session != session) {
                        let s = self.editor_snapshot();
                        let model = EditorModel::new(s.template.clone());
                        let ws = s.workspace_revision;
                        self.editor = Some(LocalEditor {
                            session,
                            buffer: EditorBuffer::new(envelope(s)),
                            model,
                            error: None,
                            workspace_revision: ws,
                            controls: Default::default(),
                            action_ids: Default::default(),
                        });
                        let effects = self
                            .controller
                            .handle(ControllerEvent::Editor(EditorMessage::Ready { session }));
                        for e in effects {
                            self.runtime.submit(e);
                        }
                    }
                }
                Effect::FlushEditorDraft {
                    request_id,
                    session,
                } => {
                    self.flush_editor_inputs();
                    let published = self.publish_editor();
                    let message = match self.editor.as_ref().filter(|e| e.session == session) {
                        _ if published.is_err() => EditorMessage::FlushFailed {
                            request_id,
                            message: published.unwrap_err().message,
                        },
                        Some(e) => EditorMessage::FlushReply {
                            request_id,
                            draft: e.buffer.draft(),
                        },
                        None => EditorMessage::FlushFailed {
                            request_id,
                            message: "Template Editor is unavailable.".into(),
                        },
                    };
                    for e in self.controller.handle(ControllerEvent::Editor(message)) {
                        self.runtime.submit(e);
                    }
                }
                Effect::CloseWindow {
                    window: WindowKind::TemplateEditor,
                    session: Some(session),
                } => {
                    self.windows.prepare_editor_close(session);
                }
                Effect::CloseWindow {
                    window: WindowKind::Main,
                    ..
                } => self.windows.prepare_main_close(),
                Effect::FocusWindow {
                    window: WindowKind::Main,
                    ..
                } => {
                    if let Some(ctx) = &self.context {
                        ctx.send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Focus,
                        );
                    }
                }
                Effect::FocusWindow {
                    window: WindowKind::TemplateEditor,
                    session: Some(session),
                } if self.windows.editor() == Some(session)
                    && !self.windows.editor_closing()
                    && !self.windows.main_closing() =>
                {
                    if let Some(ctx) = &self.context {
                        windows::focus_editor(ctx, session);
                        self.focus_editor = false;
                    } else {
                        self.focus_editor = true;
                    }
                }
                Effect::FocusWindow { .. } => {}
                Effect::EditorReply {
                    session,
                    request_id,
                    success,
                } => {
                    let s = self.editor_snapshot();
                    let error = self.main_snapshot().error.map(|e| e.message);
                    if let Some(local) = self.editor.as_mut().filter(|e| e.session == session) {
                        let incoming = s.session.map(|_| envelope(s));
                        local.buffer.complete_action(request_id, success, incoming);
                        if !success {
                            // Cancelled pickers return false, not an error.
                            local.error = error;
                        }
                    }
                }
                Effect::EditorError { session, message } => {
                    if let Some(local) = self.editor.as_mut().filter(|e| e.session == session) {
                        local.error = Some(message);
                    }
                }
                Effect::OpenPicker { request_id, picker } => {
                    self.picker = Some((request_id, picker));
                }
                Effect::ShowDialog { request_id, dialog } => {
                    self.dialogs.retain(|(_, d)| d.owner != dialog.owner);
                    self.dialogs.push((request_id, dialog));
                }
                Effect::RequestRepaint => {
                    if let Some(ctx) = &self.context {
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                        if let Some(session) = self.windows.editor() {
                            ctx.request_repaint_of(windows::editor_viewport(session));
                        }
                    }
                }
                Effect::ExitPrepared => {}
                Effect::RunCore { .. } => unreachable!("runtime owns core effects"),
                Effect::CloseWindow { .. } => {}
            }
            work.extend(self.runtime.take_platform_effects());
        }
    }
    pub fn editor_destroyed(&mut self, session: EditorSessionId) {
        if self.windows.observe_editor_closed(session) {
            self.editor = None;
            self.editor_handle = None;
            self.editor_presented = false;
            self.editor_message(EditorMessage::Closed { session });
        }
    }
    fn start_picker(&mut self) {
        if let Some((id, request)) = self.picker.take() {
            let title = if request.owner == WindowKind::Main {
                "HexForge"
            } else {
                "HexForge — Template Editor"
            };
            let owner = windows::find_owned_window(title);
            let complete = self.runtime.picker_sender();
            if std::thread::Builder::new()
                .name("hexforge-picker".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        PlatformDialogs::open(request, owner)
                    }))
                    .unwrap_or_else(|_| Err(hexforge_core::operations::operation_failed()));
                    complete(id, result);
                })
                .is_err()
            {
                self.runtime
                    .picker_finished(id, Err(hexforge_core::operations::operation_failed()));
            }
        }
    }
}
impl eframe::App for NativeApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        self.start_picker();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pump();
        if ctx.input(|i| i.viewport().close_requested()) && !self.windows.main_closing() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dispatch(ControllerEvent::MainCloseRequested);
        }
        self.paint_main(ui);
        self.start_picker();
        self.paint_editor_viewport(&ctx);
        if self.windows.main_closing() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.first_frame {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            // The root is created hidden. On Windows showing that window does
            // not activate it. Eframe applies these viewport commands after the
            // real first scene has been painted and swapped; request focus once,
            // in show-then-focus order, never on later idle frames.
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.first_frame = false;
        }
    }
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        crate::style::palette::Palette::new(self.main_snapshot().theme)
            .bg
            .to_normalized_gamma_f32()
    }
}
