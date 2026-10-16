//! Window-specific, exact-modifier bindings. Disabled dispatch still consumes a
//! reserved key; composing/text-owned keys remain with the input control.
use crate::command::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowKind {
    Main,
    TemplateEditor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub key: String,
    pub modifiers: u8,
}
impl KeyChord {
    /// Bits: Ctrl=1, Shift=2, Alt=4, Meta=8. No loose modifier matching.
    pub fn new(key: &str, modifiers: u8) -> Self {
        Self {
            key: key.to_lowercase(),
            modifiers,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InputContext {
    pub text_input: bool,
    pub composing: bool,
    pub modal: bool,
    pub repeat: bool,
    pub editor_blocked: bool,
    pub editor_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutOutcome {
    Invoke(Command),
    Consume,
    Ignore,
    Escape,
    CloseEditor,
    DismissEditorError,
}

const MAIN: &[(&str, u8, Command)] = &[
    ("o", 1, Command::Open),
    ("w", 1, Command::CloseFile),
    ("s", 3, Command::SaveAs),
    ("e", 3, Command::Export),
    ("f4", 4, Command::Exit),
    ("f2", 0, Command::EditSelected),
    ("e", 5, Command::ToggleEdit),
    ("z", 1, Command::Undo),
    ("g", 1, Command::Goto),
    ("f", 1, Command::Search),
    ("f3", 0, Command::SearchNext),
    ("f3", 2, Command::SearchPrevious),
    ("t", 3, Command::TemplateEditor),
    ("enter", 1, Command::ApplyTemplate),
    ("o", 5, Command::LoadTemplate),
    ("u", 5, Command::UnloadTemplate),
    ("t", 5, Command::ThemeToggle),
    ("1", 1, Command::Row16),
    ("2", 1, Command::Row32),
];
const EDITOR: &[(&str, u8, Command)] = &[
    ("s", 1, Command::SaveTemplate),
    ("s", 3, Command::SaveTemplateAs),
    ("o", 5, Command::LoadTemplate),
    ("u", 5, Command::UnloadTemplate),
    ("enter", 1, Command::ApplyTemplate),
];

pub fn binding_for(
    window: WindowKind,
    key: KeyChord,
    input: &InputContext,
) -> Option<ShortcutOutcome> {
    if input.composing {
        return None;
    }
    if window == WindowKind::TemplateEditor {
        return editor_binding(&key, input);
    }
    if key.key == "s" && key.modifiers == 1 {
        return Some(ShortcutOutcome::Consume);
    }
    let command = MAIN
        .iter()
        .find(|(name, mods, _)| *name == key.key && *mods == key.modifiers)
        .map(|(_, _, command)| *command);
    let command = command.filter(|command| {
        !input.text_input || !matches!(command, Command::Undo | Command::EditSelected)
    });
    if input.modal {
        return command.map(|_| ShortcutOutcome::Consume);
    }
    if key.key == "escape" {
        return Some(ShortcutOutcome::Escape);
    }
    command.map(ShortcutOutcome::Invoke)
}

fn editor_binding(key: &KeyChord, input: &InputContext) -> Option<ShortcutOutcome> {
    let ctrl = key.modifiers & 1 != 0;
    let reserved_save = key.key == "s" && matches!(key.modifiers, 1 | 3);
    if key.key == "w" && key.modifiers == 1 {
        return Some(if input.modal || input.editor_blocked {
            ShortcutOutcome::Consume
        } else {
            ShortcutOutcome::CloseEditor
        });
    }
    if input.modal {
        return (reserved_save || (ctrl && matches!(key.key.as_str(), "s" | "o" | "u" | "enter")))
            .then_some(ShortcutOutcome::Consume);
    }
    if key.key == "escape" && input.editor_error {
        return Some(ShortcutOutcome::DismissEditorError);
    }
    if input.editor_blocked {
        // A browser leaves these disabled actions inert. Native text inputs
        // also recognize Ctrl+U as delete-line (even with Alt), so letting a
        // registered but busy template key fall through would erase the draft.
        // Consume only the exact application chords; IME and unbound native
        // editing keys retain their normal routes.
        return EDITOR
            .iter()
            .find(|(name, mods, _)| *name == key.key && *mods == key.modifiers)
            .map(|_| ShortcutOutcome::Consume);
    }
    EDITOR
        .iter()
        .find(|(name, mods, _)| *name == key.key && *mods == key.modifiers)
        .map(|(_, _, command)| ShortcutOutcome::Invoke(*command))
}

pub fn shortcut_label(window: WindowKind, command: Command) -> Option<String> {
    let table = if window == WindowKind::Main {
        MAIN
    } else {
        EDITOR
    };
    let (key, mods, _) = table
        .iter()
        .find(|(_, _, candidate)| *candidate == command)?;
    let mut parts = Vec::new();
    if mods & 1 != 0 {
        parts.push("Ctrl".into());
    }
    if mods & 4 != 0 {
        parts.push("Alt".into());
    }
    if mods & 2 != 0 {
        parts.push("Shift".into());
    }
    parts.push(if *key == "enter" {
        "Enter".into()
    } else {
        key.to_uppercase()
    });
    Some(parts.join("+"))
}
