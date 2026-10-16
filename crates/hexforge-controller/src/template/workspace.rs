//! One authoritative draft, file binding and saved/checkpoint baseline. No
//! template-format versions, conversion, or hidden per-format workspaces.
use crate::{
    dialog::{DialogDecision, DialogKind, DialogRequest, PickerPurpose, PickerRequest},
    identity::increment,
    shortcuts::WindowKind,
    template::status::TemplateSource,
};
use hexforge_core::{
    error::{AppError, ErrorCode},
    operations::CoreRequest,
    template::{validate_template, Endian, ParsedNode, TemplateDefinition},
};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateAction {
    Load,
    Save,
    SaveAs,
    Unload,
    Apply,
}
#[derive(Debug)]
pub enum TemplateReply {
    Unit,
    Loaded(TemplateDefinition),
    Parsed(Vec<ParsedNode>),
    Ignored,
}
#[derive(Debug)]
pub enum TemplateEvent {
    Begin(TemplateAction),
    Draft {
        definition: TemplateDefinition,
        valid: bool,
    },
    Picker(Option<PathBuf>),
    Decision(DialogDecision),
    CoreFinished(Result<TemplateReply, AppError>),
}
#[derive(Debug, Clone)]
pub enum TemplateDirective {
    Core(CoreRequest),
    Picker(PickerRequest),
    Dialog(DialogRequest),
    ClearHighlight,
    Notice(String),
    Error(AppError),
    Completed { success: bool },
}
#[derive(Debug, Clone)]
pub struct TemplateCheckpoint {
    pub definition: TemplateDefinition,
    pub baseline: TemplateDefinition,
    pub active: bool,
    pub saved_path: Option<PathBuf>,
}
#[derive(Debug, Clone)]
pub struct TemplateSnapshot {
    pub definition: TemplateDefinition,
    pub active: bool,
    pub saved_path: Option<PathBuf>,
    pub baseline: TemplateDefinition,
    pub dirty: bool,
    pub valid: bool,
    pub busy: bool,
    pub applied: bool,
    pub needs_refresh: bool,
    pub results: Vec<ParsedNode>,
    pub workspace_revision: u64,
    pub persistence_revision: u64,
    pub definition_revision: u64,
    pub checkpoint_definition: TemplateDefinition,
    pub source: TemplateSource,
    pub display_name: String,
}
#[derive(Debug, Clone)]
struct CapturedSave {
    definition: TemplateDefinition,
    path: PathBuf,
    save_as: bool,
}
#[derive(Debug, Clone, Default)]
enum Stage {
    #[default]
    Idle,
    PickingLoad,
    ConfirmLoad(PathBuf),
    Loading {
        path: PathBuf,
        definition_revision: u64,
    },
    PickingSaveAs,
    Saving(CapturedSave),
    ConfirmOverwrite(CapturedSave),
    ConfirmUnload,
    Unloading,
    Parsing,
}
#[derive(Debug)]
pub struct TemplateWorkspace {
    pub definition: TemplateDefinition,
    pub results: Vec<ParsedNode>,
    pub applied: bool,
    pub needs_refresh: bool,
    pub workspace_revision: u64,
    pub persistence_revision: u64,
    pub definition_revision: u64,
    pub active: bool,
    pub saved_path: Option<PathBuf>,
    pub raw_valid: bool,
    baseline: TemplateDefinition,
    checkpoint: Option<TemplateCheckpoint>,
    stage: Stage,
}
impl Default for TemplateWorkspace {
    fn default() -> Self {
        let definition = empty();
        Self {
            baseline: definition.clone(),
            definition,
            results: vec![],
            applied: false,
            needs_refresh: false,
            workspace_revision: 0,
            persistence_revision: 0,
            definition_revision: 0,
            active: false,
            saved_path: None,
            raw_valid: true,
            checkpoint: None,
            stage: Stage::Idle,
        }
    }
}
impl TemplateWorkspace {
    pub fn busy(&self) -> bool {
        !matches!(self.stage, Stage::Idle)
    }
    pub fn valid(&self) -> bool {
        self.raw_valid && validate_template(&self.definition).is_ok()
    }
    pub fn dirty(&self) -> bool {
        !self.raw_valid || self.definition != self.baseline
    }
    pub fn snapshot(&self) -> TemplateSnapshot {
        TemplateSnapshot {
            definition: self.definition.clone(),
            active: self.active,
            saved_path: self.saved_path.clone(),
            baseline: self.baseline.clone(),
            dirty: self.dirty(),
            valid: self.valid(),
            busy: self.busy(),
            applied: self.applied,
            needs_refresh: self.needs_refresh,
            results: self.results.clone(),
            workspace_revision: self.workspace_revision,
            persistence_revision: self.persistence_revision,
            definition_revision: self.definition_revision,
            checkpoint_definition: self
                .checkpoint
                .as_ref()
                .map_or_else(|| self.definition.clone(), |c| c.definition.clone()),
            source: if !self.active {
                TemplateSource::None
            } else if self.saved_path.is_some() {
                TemplateSource::File
            } else {
                TemplateSource::Draft
            },
            display_name: self
                .saved_path
                .as_ref()
                .map_or_else(|| self.definition.name.clone(), filename),
        }
    }
    pub fn handle(&mut self, event: TemplateEvent) -> Vec<TemplateDirective> {
        let mut out = vec![];
        if let Err(error) = self.process(event, &mut out) {
            self.stage = Stage::Idle;
            out.push(TemplateDirective::Error(error));
            out.push(TemplateDirective::Completed { success: false });
        }
        out
    }
    fn process(
        &mut self,
        event: TemplateEvent,
        out: &mut Vec<TemplateDirective>,
    ) -> Result<(), AppError> {
        match event {
            TemplateEvent::Draft { definition, valid } => {
                if self.definition != definition {
                    increment(&mut self.definition_revision)?;
                    self.definition = definition;
                    self.active = true;
                    self.invalidate(false);
                    out.push(TemplateDirective::ClearHighlight);
                }
                self.raw_valid = valid;
            }
            TemplateEvent::Begin(action) if !self.busy() => match action {
                TemplateAction::Load => {
                    self.stage = Stage::PickingLoad;
                    out.push(TemplateDirective::Picker(PickerRequest {
                        purpose: PickerPurpose::TemplateLoad,
                        title: "Load parsing template".into(),
                        default_path: None,
                        filters: vec![("JSON template".into(), vec!["json".into()])],
                        owner: WindowKind::Main,
                    }));
                }
                TemplateAction::Save if self.active && self.saved_path.is_some() => {
                    self.require_valid("saving")?;
                    let captured = CapturedSave {
                        definition: self.definition.clone(),
                        path: self.saved_path.clone().expect("checked path"),
                        save_as: false,
                    };
                    self.start_save(captured, false, out);
                }
                TemplateAction::SaveAs if self.active => {
                    self.require_valid("saving")?;
                    self.stage = Stage::PickingSaveAs;
                    out.push(TemplateDirective::Picker(PickerRequest {
                        purpose: PickerPurpose::TemplateSave,
                        title: "Save parsing template".into(),
                        default_path: Some(
                            format!(
                                "{}.json",
                                if self.definition.name.is_empty() {
                                    "template"
                                } else {
                                    &self.definition.name
                                }
                            )
                            .into(),
                        ),
                        filters: vec![("JSON template".into(), vec!["json".into()])],
                        owner: WindowKind::Main,
                    }));
                }
                TemplateAction::Unload if self.active => {
                    if self.dirty() {
                        self.stage = Stage::ConfirmUnload;
                        out.push(confirm(
                            "This template has unsaved changes. Discard them and unload it?",
                        ));
                    } else {
                        self.stage = Stage::Unloading;
                        out.push(TemplateDirective::Core(CoreRequest::UnloadTemplate));
                    }
                }
                TemplateAction::Apply => {
                    self.require_valid("applying")?;
                    self.stage = Stage::Parsing;
                    out.push(TemplateDirective::Core(CoreRequest::ApplyTemplate {
                        template: self.definition.clone(),
                    }));
                }
                _ => out.push(TemplateDirective::Completed { success: false }),
            },
            TemplateEvent::Picker(path) => match (self.stage.clone(), path) {
                (Stage::PickingLoad, Some(path)) => {
                    if self.dirty() {
                        self.stage = Stage::ConfirmLoad(path);
                        out.push(confirm("This template has unsaved changes. Discard them and load another template?"));
                    } else {
                        self.start_load(path, out);
                    }
                }
                (Stage::PickingSaveAs, Some(path)) => {
                    // Capture after the picker, before any overwrite retry.
                    self.require_valid("saving")?;
                    let captured = CapturedSave {
                        definition: self.definition.clone(),
                        path,
                        save_as: true,
                    };
                    self.start_save(captured, false, out);
                }
                (Stage::PickingLoad | Stage::PickingSaveAs, None) => {
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::Completed { success: false });
                }
                _ => {}
            },
            TemplateEvent::Decision(decision) => match self.stage.clone() {
                Stage::ConfirmLoad(path) if decision == DialogDecision::Confirm => {
                    self.start_load(path, out)
                }
                Stage::ConfirmUnload if decision == DialogDecision::Confirm => {
                    self.stage = Stage::Unloading;
                    out.push(TemplateDirective::Core(CoreRequest::UnloadTemplate));
                }
                Stage::ConfirmOverwrite(captured) if decision == DialogDecision::Confirm => {
                    self.start_save(captured, true, out)
                }
                Stage::ConfirmLoad(_) | Stage::ConfirmUnload | Stage::ConfirmOverwrite(_) => {
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::Completed { success: false });
                }
                _ => {}
            },
            TemplateEvent::CoreFinished(reply) => match (self.stage.clone(), reply) {
                (
                    Stage::Loading {
                        path,
                        definition_revision,
                    },
                    Ok(TemplateReply::Loaded(definition)),
                ) => {
                    self.advance_replacement()?;
                    if Some(self.definition_revision) == definition_revision.checked_add(1) {
                        self.definition = definition.clone();
                        self.raw_valid = true;
                    }
                    self.baseline = definition;
                    self.saved_path = Some(path);
                    self.active = true;
                    self.invalidate(true);
                    self.checkpoint_current();
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::ClearHighlight);
                    out.push(TemplateDirective::Completed { success: true });
                }
                (Stage::Saving(captured), Ok(TemplateReply::Unit)) => {
                    increment(&mut self.persistence_revision)?;
                    self.baseline = captured.definition.clone();
                    self.saved_path = Some(captured.path.clone());
                    self.checkpoint = Some(TemplateCheckpoint {
                        definition: captured.definition,
                        baseline: self.baseline.clone(),
                        active: self.active,
                        saved_path: self.saved_path.clone(),
                    });
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::Notice(if captured.save_as {
                        format!("Template saved as {}.", filename(&captured.path))
                    } else {
                        format!("Template saved: {}.", filename(&captured.path))
                    }));
                    out.push(TemplateDirective::Completed { success: true });
                }
                (Stage::Saving(captured), Err(error))
                    if (!captured.save_as && error.code == ErrorCode::ExternalModification)
                        || (captured.save_as && error.code == ErrorCode::DestinationExists) =>
                {
                    out.push(confirm(if captured.save_as {
                        "File already exists. Overwrite?"
                    } else {
                        "The file has been modified by another program. Overwrite?"
                    }));
                    self.stage = Stage::ConfirmOverwrite(captured);
                }
                (Stage::Unloading, Ok(TemplateReply::Unit)) => {
                    self.advance_replacement()?;
                    self.definition = empty();
                    self.baseline = self.definition.clone();
                    self.active = false;
                    self.saved_path = None;
                    self.raw_valid = true;
                    self.invalidate(true);
                    self.checkpoint_current();
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::ClearHighlight);
                    out.push(TemplateDirective::Completed { success: true });
                }
                (Stage::Parsing, Ok(TemplateReply::Parsed(nodes))) => {
                    self.install_results(nodes);
                    self.stage = Stage::Idle;
                    let count = count_leaves(&self.results);
                    out.push(TemplateDirective::Notice(
                        if has_diagnostics(&self.results) {
                            "Parsing completed with field errors. Inspect parsed results.".into()
                        } else {
                            format!(
                                "Parsed {count} {}.",
                                if count == 1 { "field" } else { "fields" }
                            )
                        },
                    ));
                    out.push(TemplateDirective::Completed { success: true });
                }
                (_, Ok(TemplateReply::Ignored)) => {
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::Completed { success: false });
                }
                (_, Err(error)) => {
                    self.stage = Stage::Idle;
                    out.push(TemplateDirective::Error(error));
                    out.push(TemplateDirective::Completed { success: false });
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }
    fn require_valid(&self, action: &str) -> Result<(), AppError> {
        if self.valid() {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::InvalidTemplate,
                format!("Correct the highlighted template field before {action}."),
                None,
            ))
        }
    }
    fn start_save(
        &mut self,
        captured: CapturedSave,
        overwrite: bool,
        out: &mut Vec<TemplateDirective>,
    ) {
        let request = if captured.save_as {
            CoreRequest::SaveTemplateAs {
                path: captured.path.clone(),
                template: captured.definition.clone(),
                overwrite,
            }
        } else {
            CoreRequest::SaveTemplate {
                template: captured.definition.clone(),
                overwrite_external: overwrite,
            }
        };
        self.stage = Stage::Saving(captured);
        out.push(TemplateDirective::Core(request));
    }
    fn start_load(&mut self, path: PathBuf, out: &mut Vec<TemplateDirective>) {
        self.stage = Stage::Loading {
            path: path.clone(),
            definition_revision: self.definition_revision,
        };
        out.push(TemplateDirective::Core(CoreRequest::LoadTemplate { path }));
    }
    fn advance_replacement(&mut self) -> Result<(), AppError> {
        let mut workspace = self.workspace_revision;
        let mut persistence = self.persistence_revision;
        let mut definition = self.definition_revision;
        increment(&mut workspace)?;
        increment(&mut persistence)?;
        increment(&mut definition)?;
        self.workspace_revision = workspace;
        self.persistence_revision = persistence;
        self.definition_revision = definition;
        Ok(())
    }
    pub fn invalidate(&mut self, reset_refresh: bool) {
        self.applied = false;
        if reset_refresh {
            self.needs_refresh = false;
        } else if !self.results.is_empty() {
            self.needs_refresh = true;
        }
        self.results.clear();
    }
    pub fn install_results(&mut self, nodes: Vec<ParsedNode>) {
        self.results = nodes;
        self.applied = true;
        self.needs_refresh = false;
    }
    pub fn checkpoint_current(&mut self) {
        self.checkpoint = Some(TemplateCheckpoint {
            definition: self.definition.clone(),
            baseline: self.baseline.clone(),
            active: self.active,
            saved_path: self.saved_path.clone(),
        });
    }
    pub fn restore_checkpoint(&mut self) -> Result<(), AppError> {
        let checkpoint = self.checkpoint.clone().ok_or_else(|| {
            AppError::new(
                ErrorCode::OperationFailed,
                "The Template Editor has no draft to restore.",
                None,
            )
        })?;
        self.advance_replacement()?;
        self.definition = checkpoint.definition;
        self.baseline = checkpoint.baseline;
        self.active = checkpoint.active;
        self.saved_path = checkpoint.saved_path;
        self.raw_valid = true;
        self.invalidate(false);
        self.checkpoint_current();
        Ok(())
    }
}
pub fn empty() -> TemplateDefinition {
    TemplateDefinition {
        name: "Untitled".into(),
        default_endianness: Endian::Little,
        fields: vec![],
    }
}
pub fn filename(path: &PathBuf) -> String {
    path.to_string_lossy()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned()
}
fn confirm(message: &str) -> TemplateDirective {
    TemplateDirective::Dialog(DialogRequest {
        kind: DialogKind::Confirmation,
        title: "HexForge".into(),
        message: message.into(),
        owner: WindowKind::Main,
    })
}
pub fn has_diagnostics(nodes: &[ParsedNode]) -> bool {
    nodes
        .iter()
        .any(|n| !n.diagnostics.is_empty() || has_diagnostics(&n.children))
}
pub fn count_leaves(nodes: &[ParsedNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            if n.kind == "leaf" {
                1
            } else {
                count_leaves(&n.children)
            }
        })
        .sum()
}
