use hexforge_controller::command::Command;
use hexforge_controller::shortcuts::{
    binding_for, shortcut_label, InputContext, KeyChord, ShortcutOutcome, WindowKind,
};

fn chord(key: &str, modifiers: u8) -> KeyChord {
    KeyChord::new(key, modifiers)
}

#[test]
fn main_ctrl_s_consumed_without_save() {
    let input = InputContext::default();
    assert_eq!(
        binding_for(WindowKind::Main, chord("s", 1), &input),
        Some(ShortcutOutcome::Consume)
    );
    assert_eq!(
        binding_for(WindowKind::Main, chord("s", 3), &input),
        Some(ShortcutOutcome::Invoke(Command::SaveAs))
    );
    assert_eq!(
        binding_for(WindowKind::TemplateEditor, chord("s", 1), &input),
        Some(ShortcutOutcome::Invoke(Command::SaveTemplate))
    );
    assert_eq!(
        binding_for(WindowKind::TemplateEditor, chord("s", 3), &input),
        Some(ShortcutOutcome::Invoke(Command::SaveTemplateAs))
    );
}

#[test]
fn every_busy_editor_shortcut_is_consumed_before_text_input() {
    let blocked = InputContext {
        editor_blocked: true,
        ..Default::default()
    };
    for (key, modifiers) in [
        ("s", 1),
        ("s", 3),
        ("o", 5),
        ("u", 5),
        ("enter", 1),
        ("w", 1),
    ] {
        assert_eq!(
            binding_for(WindowKind::TemplateEditor, chord(key, modifiers), &blocked),
            Some(ShortcutOutcome::Consume),
            "Disabled {key}/{modifiers} must not reach a focused input"
        );
    }
    for (key, modifiers) in [("o", 1), ("u", 1), ("s", 5), ("w", 3)] {
        assert_eq!(
            binding_for(WindowKind::TemplateEditor, chord(key, modifiers), &blocked),
            None,
            "Unregistered chords retain native input behavior"
        );
    }
}

#[test]
fn shortcut_labels_match_window_bindings() {
    let input = InputContext::default();
    let main = [
        ("o", 1, Command::Open, "Ctrl+O"),
        ("w", 1, Command::CloseFile, "Ctrl+W"),
        ("s", 3, Command::SaveAs, "Ctrl+Shift+S"),
        ("e", 3, Command::Export, "Ctrl+Shift+E"),
        ("F4", 4, Command::Exit, "Alt+F4"),
        ("F2", 0, Command::EditSelected, "F2"),
        ("e", 5, Command::ToggleEdit, "Ctrl+Alt+E"),
        ("z", 1, Command::Undo, "Ctrl+Z"),
        ("g", 1, Command::Goto, "Ctrl+G"),
        ("f", 1, Command::Search, "Ctrl+F"),
        ("F3", 0, Command::SearchNext, "F3"),
        ("F3", 2, Command::SearchPrevious, "Shift+F3"),
        ("t", 3, Command::TemplateEditor, "Ctrl+Shift+T"),
        ("Enter", 1, Command::ApplyTemplate, "Ctrl+Enter"),
        ("o", 5, Command::LoadTemplate, "Ctrl+Alt+O"),
        ("u", 5, Command::UnloadTemplate, "Ctrl+Alt+U"),
        ("t", 5, Command::ThemeToggle, "Ctrl+Alt+T"),
        ("1", 1, Command::Row16, "Ctrl+1"),
        ("2", 1, Command::Row32, "Ctrl+2"),
    ];
    for (key, modifiers, command, label) in main {
        assert_eq!(
            binding_for(WindowKind::Main, chord(key, modifiers), &input),
            Some(ShortcutOutcome::Invoke(command))
        );
        assert_eq!(
            shortcut_label(WindowKind::Main, command).as_deref(),
            Some(label)
        );
    }
    assert_eq!(
        shortcut_label(WindowKind::Main, Command::SaveTemplate),
        None
    );
    assert_eq!(
        shortcut_label(WindowKind::TemplateEditor, Command::SaveTemplate).as_deref(),
        Some("Ctrl+S")
    );
    assert_eq!(
        shortcut_label(WindowKind::TemplateEditor, Command::SaveTemplateAs).as_deref(),
        Some("Ctrl+Shift+S")
    );
}

#[test]
fn exact_modifiers_text_focus_ime_modal_and_editor_busy_are_preserved() {
    for (key, modifiers) in [
        ("o", 3),
        ("F2", 4),
        ("s", 5),
        ("a", 5),
        ("d", 5),
        ("l", 5),
        ("b", 1),
        ("b", 3),
        ("b", 5),
        ("m", 5),
        ("o", 9),
    ] {
        assert_eq!(
            binding_for(
                WindowKind::Main,
                chord(key, modifiers),
                &InputContext::default()
            ),
            None
        );
    }
    let text = InputContext {
        text_input: true,
        ..Default::default()
    };
    assert_eq!(binding_for(WindowKind::Main, chord("z", 1), &text), None);
    assert_eq!(binding_for(WindowKind::Main, chord("F2", 0), &text), None);
    assert_eq!(
        binding_for(WindowKind::Main, chord("s", 1), &text),
        Some(ShortcutOutcome::Consume)
    );
    assert_eq!(
        binding_for(WindowKind::Main, chord("s", 3), &text),
        Some(ShortcutOutcome::Invoke(Command::SaveAs))
    );
    let ime = InputContext {
        composing: true,
        ..Default::default()
    };
    assert_eq!(binding_for(WindowKind::Main, chord("o", 1), &ime), None);
    assert_eq!(
        binding_for(WindowKind::Main, chord("Escape", 0), &ime),
        None
    );
    let modal = InputContext {
        modal: true,
        ..Default::default()
    };
    assert_eq!(
        binding_for(WindowKind::Main, chord("o", 1), &modal),
        Some(ShortcutOutcome::Consume)
    );
    assert_eq!(
        binding_for(WindowKind::Main, chord("Escape", 0), &modal),
        None
    );
    let blocked = InputContext {
        editor_blocked: true,
        ..Default::default()
    };
    assert_eq!(
        binding_for(WindowKind::TemplateEditor, chord("s", 1), &blocked),
        Some(ShortcutOutcome::Consume)
    );
    assert_eq!(
        binding_for(WindowKind::TemplateEditor, chord("w", 1), &blocked),
        Some(ShortcutOutcome::Consume)
    );
    assert_eq!(
        binding_for(WindowKind::TemplateEditor, chord("o", 5), &blocked),
        Some(ShortcutOutcome::Consume)
    );
    let repeat = InputContext {
        repeat: true,
        ..Default::default()
    };
    assert_eq!(
        binding_for(WindowKind::Main, chord("F3", 0), &repeat),
        Some(ShortcutOutcome::Invoke(Command::SearchNext))
    );
}
