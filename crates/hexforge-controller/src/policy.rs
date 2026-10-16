//! The reference command policy, enforced by rendering AND authoritative dispatch.
use crate::command::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PolicyState {
    pub has_file: bool,
    pub has_bytes: bool,
    pub single_byte_selected: bool,
    pub edit_mode: bool,
    pub can_undo: bool,
    pub template_valid: bool,
    pub template_active: bool,
    pub template_has_path: bool,
    pub template_has_fields: bool,
    pub has_parsed_results: bool,
    pub operation_busy: bool,
    pub has_search_matches: bool,
    pub template_issue_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAvailability {
    pub enabled: bool,
    pub reason: Option<String>,
}

pub fn command_availability(command: Command, state: &PolicyState) -> CommandAvailability {
    let reason = command_unavailable_reason(command, state);
    CommandAvailability {
        enabled: reason.is_none(),
        reason,
    }
}

pub fn command_enabled(command: Command, state: &PolicyState) -> bool {
    command_unavailable_reason(command, state).is_none()
}

pub fn command_unavailable_reason(command: Command, s: &PolicyState) -> Option<String> {
    use Command::*;
    if matches!(
        command,
        Exit | ThemeToggle
            | Row16
            | Row32
            | ToggleRightPanel
            | MinimapToggle
            | MinimapFit
            | MinimapProportional
            | MinimapCharacters
            | MinimapBlocks
            | MinimapScale1
            | MinimapScale2
            | MinimapScale3
    ) {
        return None;
    }
    if s.operation_busy {
        return Some("Wait for the current operation to finish.".into());
    }
    let invalid = || match s.template_issue_count.filter(|count| *count > 0) {
        Some(count) => format!(
            "Correct {count} template {} first.",
            if count == 1 { "error" } else { "errors" }
        ),
        None => "Correct the highlighted template errors first.".into(),
    };
    let reason = match command {
        Open | TemplateEditor | LoadTemplate => None,
        SaveTemplate => {
            if !s.template_active {
                Some("Load or create a template first.".into())
            } else if !s.template_has_path {
                Some("Use Save As first to choose a template file.".into())
            } else if !s.template_valid {
                Some(invalid())
            } else {
                None
            }
        }
        SaveTemplateAs => {
            if !s.template_active {
                Some("Load or create a template first.".into())
            } else if !s.template_valid {
                Some(invalid())
            } else {
                None
            }
        }
        UnloadTemplate => (!s.template_active).then(|| "No template is loaded.".into()),
        CloseFile | SaveAs | ToggleEdit => {
            (!s.has_file).then(|| "Open a binary file first.".into())
        }
        Goto | Search => (!s.has_bytes).then(|| "Open a nonempty binary file first.".into()),
        SearchNext | SearchPrevious => (!s.has_bytes || !s.has_search_matches)
            .then(|| "Search the current byte pattern first.".into()),
        EditSelected => {
            if !s.has_bytes {
                Some("Open a nonempty binary file first.".into())
            } else if !s.edit_mode {
                Some("Enable Edit Mode first.".into())
            } else if !s.single_byte_selected {
                Some("Select one byte to edit.".into())
            } else {
                None
            }
        }
        Undo => (!s.has_file || !s.can_undo).then(|| "No byte edit to undo.".into()),
        Export => (!s.has_file || !s.has_parsed_results)
            .then(|| "Apply a valid template before exporting results.".into()),
        ApplyTemplate => {
            if !s.has_file {
                Some("Open a binary file to apply the template.".into())
            } else if !s.template_valid {
                Some(invalid())
            } else if !s.template_has_fields {
                Some("Add a template field first.".into())
            } else {
                None
            }
        }
        _ => None,
    };
    reason
}
