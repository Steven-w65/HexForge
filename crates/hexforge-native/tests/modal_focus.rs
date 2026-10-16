//! Transferred dialog contracts exercise the real application and controls,
//! rather than asserting that egui's built-in Modal exists.
mod support;
use eframe::egui;
use hexforge_controller::{action::Action, command::Command, controller::InitialState};
use hexforge_core::operations::CoreService;
use hexforge_native::app::NativeApp;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use support::ui_harness::{CapturedFrame, NativeUiHarness};

fn opened() -> (tempfile::NamedTempFile, NativeApp, NativeUiHarness) {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(512).unwrap();
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let harness = NativeUiHarness::new();
    app.attach(&harness.context);
    app.gesture(Action::DropFiles(vec![file.path().into()]));
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().file.is_none() {
        app.pump();
        assert!(Instant::now() < deadline, "real open did not settle");
        std::thread::yield_now();
    }
    draw(&harness, &mut app, vec![]);
    (file, app, harness)
}
fn draw(h: &NativeUiHarness, app: &mut NativeApp, mut events: Vec<egui::Event>) -> CapturedFrame {
    let modifiers = events
        .iter()
        .rev()
        .find_map(|e| match e {
            egui::Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    h.main_frame(
        app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        },
    )
}
fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn modal_exposes_its_dialog_role_title_and_single_close_control() {
    let (_file, mut app, h) = opened();
    edit_dialog(&mut app);
    draw(&h, &mut app, vec![]);
    let frame = draw(&h, &mut app, vec![]);
    let tree = frame
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap();
    assert!(
        tree.nodes
            .iter()
            .any(|(_, n)| n.role() == egui::accesskit::Role::Dialog
                && n.label() == Some("Edit byte")
                && n.is_modal()),
        "real modal must carry its source title and dialog semantics: {:?}",
        tree.nodes
            .iter()
            .map(|(_, n)| (n.role(), n.label(), n.is_modal()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        frame
            .widgets
            .iter()
            .filter(|w| w.label == "Close dialog")
            .count(),
        1
    );
}
fn focus_label(h: &NativeUiHarness, frame: &CapturedFrame) -> Option<String> {
    let id = h.context.memory(|m| m.focused())?.accesskit_id();
    frame
        .output
        .platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find(|(node_id, _)| *node_id == id)
        .and_then(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
}
fn edit_dialog(app: &mut NativeApp) {
    app.gesture(Action::Select { start: 0, end: 0 });
    app.invoke(Command::ToggleEdit, Default::default());
    app.invoke(Command::EditSelected, Default::default());
    assert_eq!(app.main_snapshot().edit_dialog.as_ref().unwrap().value, "");
}

#[test]
fn edit_dialog_focuses_its_input_on_every_open() {
    let (_file, mut app, h) = opened();
    app.invoke(Command::Search, Default::default());
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    assert!(h
        .context
        .memory(|m| m.has_focus(egui::Id::new("search-bytes"))));
    edit_dialog(&mut app);
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    assert!(
        h.context
            .memory(|m| m.has_focus(egui::Id::new("edit-byte-value"))),
        "the byte value must accept typing immediately even when search had focus"
    );
    draw(
        &h,
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    draw(&h, &mut app, vec![]);
    app.invoke(Command::EditSelected, Default::default());
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    assert!(h
        .context
        .memory(|m| m.has_focus(egui::Id::new("edit-byte-value"))));
}

#[test]
fn dismissing_error_restores_the_invoking_search_input() {
    let (_file, mut app, h) = opened();
    app.invoke(Command::Search, Default::default());
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    app.gesture(Action::DropFiles(vec![]));
    assert!(app.main_snapshot().error.is_some());
    draw(&h, &mut app, vec![]);
    let shown = draw(&h, &mut app, vec![]);
    assert_eq!(focus_label(&h, &shown).as_deref(), Some("Close dialog"));
    draw(
        &h,
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.main_snapshot().error.is_none());
    draw(&h, &mut app, vec![]);
    assert!(
        h.context
            .memory(|m| m.has_focus(egui::Id::new("search-bytes"))),
        "Cancel must return focus to the still-present invoking control"
    );
}

#[test]
fn modal_tabs_wrap_both_directions_without_focusing_the_hex_controls() {
    let (_file, mut app, h) = opened();
    edit_dialog(&mut app);
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    let mut labels = vec![];
    let mut action_bounds = vec![];
    for _ in 0..5 {
        let frame = draw(
            &h,
            &mut app,
            vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        );
        labels.push(focus_label(&h, &frame));
        action_bounds = frame
            .widgets
            .iter()
            .filter(|w| matches!(w.label.as_str(), "Cancel" | "Apply"))
            .map(|w| (w.label.clone(), w.rect))
            .collect();
    }
    // Source DOM order is Close, input, Cancel, Apply, independent of how the
    // right-aligned buttons happen to be allocated by the native layout.
    assert_eq!(
        labels,
        vec![
            Some("Cancel".into()),
            Some("Apply".into()),
            Some("Close dialog".into()),
            Some("".into()),
            Some("Cancel".into())
        ],
        "action bounds: {action_bounds:?}"
    );
    let frame = draw(
        &h,
        &mut app,
        vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)],
    );
    assert_eq!(focus_label(&h, &frame).as_deref(), Some(""));
    let frame = draw(
        &h,
        &mut app,
        vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)],
    );
    assert_eq!(focus_label(&h, &frame).as_deref(), Some("Close dialog"));
    let frame = draw(
        &h,
        &mut app,
        vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)],
    );
    assert_eq!(focus_label(&h, &frame).as_deref(), Some("Apply"));
}

#[test]
fn reopened_error_takes_a_fresh_focus_checkpoint() {
    let (_file, mut app, h) = opened();
    // The first error has no invoking focus. A later opening must not reuse
    // that empty checkpoint after a new search input becomes active.
    app.gesture(Action::DropFiles(vec![]));
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    draw(
        &h,
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    draw(&h, &mut app, vec![]);
    app.invoke(Command::Search, Default::default());
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    app.gesture(Action::DropFiles(vec![]));
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    draw(
        &h,
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    draw(&h, &mut app, vec![]);
    assert!(h
        .context
        .memory(|m| m.has_focus(egui::Id::new("search-bytes"))));
}

#[test]
fn binary_unsaved_prompt_initial_focus_is_cancel_not_a_destructive_action() {
    let (_file, mut app, h) = opened();
    app.gesture(Action::Select { start: 0, end: 0 });
    app.invoke(Command::ToggleEdit, Default::default());
    app.gesture(Action::EditSelectedByte("FF".into()));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.main_snapshot().file.as_ref().unwrap().dirty {
        app.pump();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    app.invoke(Command::CloseFile, Default::default());
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().dialog.is_none() {
        app.pump();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    draw(&h, &mut app, vec![]);
    let frame = draw(&h, &mut app, vec![]);
    assert_eq!(focus_label(&h, &frame).as_deref(), Some("Cancel"));
    draw(
        &h,
        &mut app,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(app.main_snapshot().dialog.is_none());
    assert!(app.main_snapshot().file.as_ref().unwrap().dirty);
    assert!(!app.windows.main_closing());
}

fn draw_editor(
    h: &NativeUiHarness,
    app: &mut NativeApp,
    mut events: Vec<egui::Event>,
) -> CapturedFrame {
    let modifiers = events
        .iter()
        .rev()
        .find_map(|e| match e {
            egui::Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    h.editor_frame(
        app,
        egui::RawInput {
            events,
            ..Default::default()
        },
    )
}

#[test]
fn editor_close_focuses_cancel_and_skips_disabled_save_during_tab_cycle() {
    use hexforge_controller::template::coordinator::EditorMessage;
    let (_file, mut app, h) = opened();
    app.invoke(Command::TemplateEditor, Default::default());
    let session = app.windows.editor().unwrap();
    app.editor.as_mut().unwrap().model.name = "Unsaved draft".into();
    app.publish_editor().unwrap();
    app.editor_message(EditorMessage::CloseRequested { session });
    draw_editor(&h, &mut app, vec![]);
    let frame = draw_editor(&h, &mut app, vec![]);
    assert_eq!(focus_label(&h, &frame).as_deref(), Some("Cancel"));
    assert!(
        !frame
            .widgets
            .iter()
            .find(|w| w.label == "Save")
            .unwrap()
            .enabled
    );
    let mut labels = vec![];
    for _ in 0..4 {
        let frame = draw_editor(
            &h,
            &mut app,
            vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        );
        labels.push(focus_label(&h, &frame));
    }
    assert_eq!(
        labels,
        vec![
            Some("Close dialog".into()),
            Some("Save As".into()),
            Some("Don't Save".into()),
            Some("Cancel".into())
        ]
    );
    draw_editor(
        &h,
        &mut app,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(!app.editor_snapshot().close_prompt);
    assert_eq!(app.main_snapshot().template.name, "Unsaved draft");
    assert!(!app.windows.editor_closing());
}

fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn error_close_control_and_backdrop_do_not_activate_background_controls() {
    let (_file, mut app, h) = opened();
    app.gesture(Action::DropFiles(vec![]));
    draw(&h, &mut app, vec![]);
    let frame = draw(&h, &mut app, vec![]);
    let close = frame
        .widgets
        .iter()
        .find(|w| w.label == "Close dialog")
        .unwrap()
        .rect
        .center();
    for event in [
        egui::Event::PointerMoved(close),
        pointer(close, true),
        pointer(close, false),
    ] {
        draw(&h, &mut app, vec![event]);
    }
    assert!(app.main_snapshot().error.is_none());
    draw(&h, &mut app, vec![]);
    app.gesture(Action::DropFiles(vec![]));
    draw(&h, &mut app, vec![]);
    let frame = draw(&h, &mut app, vec![]);
    let file = frame
        .widgets
        .iter()
        .find(|w| w.label == "File")
        .unwrap()
        .rect
        .center();
    for event in [
        egui::Event::PointerMoved(file),
        pointer(file, true),
        pointer(file, false),
    ] {
        draw(&h, &mut app, vec![event]);
    }
    assert!(app.main_snapshot().error.is_none());
    let frame = draw(&h, &mut app, vec![]);
    assert!(!frame.widgets.iter().any(|w| w.label == "Open File…"));
    assert!(app.picker.is_none());
    assert!(app.main_snapshot().file.is_some());
}

#[test]
fn nested_editor_error_retains_parent_close_protection_and_return_focus() {
    use hexforge_controller::template::coordinator::EditorMessage;
    let (_file, mut app, h) = opened();
    app.invoke(Command::TemplateEditor, Default::default());
    let session = app.windows.editor().unwrap();
    app.editor.as_mut().unwrap().model.name = "Protected draft".into();
    app.publish_editor().unwrap();
    app.editor_message(EditorMessage::CloseRequested { session });
    draw_editor(&h, &mut app, vec![]);
    let parent = draw_editor(&h, &mut app, vec![]);
    assert_eq!(focus_label(&h, &parent).as_deref(), Some("Cancel"));
    // A worker error can appear over an already-open close prompt. This is a
    // controlled incoming presenter value, not a replacement for the reducer.
    app.editor.as_mut().unwrap().error = Some("The destination could not be written.".into());
    draw_editor(&h, &mut app, vec![]);
    let child = draw_editor(&h, &mut app, vec![]);
    assert_eq!(focus_label(&h, &child).as_deref(), Some("Close dialog"));
    for _ in 0..3 {
        let child = draw_editor(
            &h,
            &mut app,
            vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        );
        assert_eq!(focus_label(&h, &child).as_deref(), Some("Close dialog"));
        assert!(app.editor_snapshot().close_prompt);
    }
    draw_editor(
        &h,
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.editor.as_ref().unwrap().error.is_none());
    assert!(app.editor_snapshot().close_prompt);
    draw_editor(&h, &mut app, vec![]);
    let parent = draw_editor(&h, &mut app, vec![]);
    assert_eq!(focus_label(&h, &parent).as_deref(), Some("Cancel"));
    assert_eq!(app.main_snapshot().template.name, "Protected draft");
    assert!(!app.windows.editor_closing());
}

#[test]
fn nested_editor_error_keeps_the_parent_prompt_painted_but_inert() {
    use hexforge_controller::template::coordinator::EditorMessage;
    let (_file, mut app, h) = opened();
    app.invoke(Command::TemplateEditor, Default::default());
    let session = app.windows.editor().unwrap();
    app.editor.as_mut().unwrap().model.name = "Protected draft".into();
    app.publish_editor().unwrap();
    app.editor_message(EditorMessage::CloseRequested { session });
    draw_editor(&h, &mut app, vec![]);
    let parent = draw_editor(&h, &mut app, vec![]);
    let discard = parent
        .widgets
        .iter()
        .find(|w| w.label == "Don't Save")
        .unwrap()
        .rect
        .center();
    app.editor.as_mut().unwrap().error = Some("The destination could not be written.".into());
    draw_editor(&h, &mut app, vec![]);
    let nested = draw_editor(&h, &mut app, vec![]);
    let tree = nested
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap();
    for title in ["Unsaved template edits", "Template action failed"] {
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::Dialog
                    && node.label() == Some(title)),
            "the covered parent must remain painted: missing {title}"
        );
    }
    assert!(tree
        .nodes
        .iter()
        .any(|(_, node)| node.role() == egui::accesskit::Role::Dialog
            && node.label() == Some("Unsaved template edits")
            && node.is_disabled()));
    assert!(nested
        .widgets
        .iter()
        .any(|widget| widget.label == "Don't Save"));
    assert_eq!(focus_label(&h, &nested).as_deref(), Some("Close dialog"));
    // This point is outside the shorter error dialog but inside the parent's
    // destructive button. The top backdrop may dismiss its error; it must
    // never accept the underlying discard on that same click.
    for event in [
        egui::Event::PointerMoved(discard),
        pointer(discard, true),
        pointer(discard, false),
    ] {
        draw_editor(&h, &mut app, vec![event]);
    }
    assert_eq!(app.main_snapshot().template.name, "Protected draft");
    assert!(app.editor_snapshot().close_prompt);
    assert!(!app.windows.editor_closing());
    if app.editor.as_ref().unwrap().error.is_some() {
        draw_editor(
            &h,
            &mut app,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
    }
    draw_editor(&h, &mut app, vec![]);
    let parent = draw_editor(&h, &mut app, vec![]);
    assert!(parent
        .widgets
        .iter()
        .any(|widget| widget.label == "Don't Save"));
    assert_eq!(focus_label(&h, &parent).as_deref(), Some("Cancel"));
    draw_editor(
        &h,
        &mut app,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(!app.editor_snapshot().close_prompt);
    assert_eq!(app.main_snapshot().template.name, "Protected draft");
    assert!(!app.windows.editor_closing());
    draw_editor(&h, &mut app, vec![]);
    let restored = draw_editor(&h, &mut app, vec![]);
    let add = restored
        .widgets
        .iter()
        .find(|widget| widget.label == "+ Field")
        .unwrap();
    assert!(add.enabled);
    let position = add.rect.center();
    for event in [
        egui::Event::PointerMoved(position),
        pointer(position, true),
        pointer(position, false),
    ] {
        draw_editor(&h, &mut app, vec![event]);
    }
    assert_eq!(
        app.main_snapshot().template.fields.len(),
        1,
        "all modal locks must release when the last dialog closes"
    );
}

#[test]
fn rapid_modal_tabs_preserve_each_requested_step() {
    let (_file, mut app, h) = opened();
    edit_dialog(&mut app);
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    let frame = draw(
        &h,
        &mut app,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE); 3],
    );
    assert_eq!(focus_label(&h, &frame).as_deref(), Some("Close dialog"));
    let frame = draw(
        &h,
        &mut app,
        vec![key(egui::Key::Tab, egui::Modifiers::SHIFT); 2],
    );
    assert_eq!(focus_label(&h, &frame).as_deref(), Some("Cancel"));
}

#[test]
fn main_modal_checkpoint_cannot_restore_focus_into_the_editor_viewport() {
    let (_file, mut app, h) = opened();
    app.invoke(Command::Search, Default::default());
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    app.gesture(Action::DropFiles(vec![]));
    draw(&h, &mut app, vec![]);
    draw(&h, &mut app, vec![]);
    // Native viewports share the app/context, not their focus checkpoints.
    let child = egui::ViewportId::from_hash_of("real separate editor fixture");
    app.invoke(
        Command::TemplateEditor,
        hexforge_controller::controller::InvokeContext {
            window: Some(hexforge_controller::shortcuts::WindowKind::TemplateEditor),
            ..Default::default()
        },
    );
    assert!(app.editor.is_some());
    let mut child_input = egui::RawInput {
        viewport_id: child,
        ..Default::default()
    };
    child_input.viewports.insert(
        child,
        egui::ViewportInfo {
            parent: Some(egui::ViewportId::ROOT),
            ..Default::default()
        },
    );
    h.editor_frame(&mut app, child_input.clone());
    let name = egui::Id::new("template-name");
    let rect = h.context.read_response(name).map(|r| r.rect);
    // Focus through the real child input, not a parent-memory request.
    let child_frame = h.editor_frame(&mut app, child_input.clone());
    let name_rect = child_frame
        .widgets
        .iter()
        .find(|w| w.label == "Untitled")
        .map(|w| w.rect)
        .or(rect)
        .unwrap();
    for event in [
        egui::Event::PointerMoved(name_rect.center()),
        pointer(name_rect.center(), true),
        pointer(name_rect.center(), false),
    ] {
        child_input.events = vec![event];
        h.editor_frame(&mut app, child_input.clone());
    }
    child_input.events.clear();
    let before = h.editor_frame(&mut app, child_input.clone());
    assert_eq!(
        before
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .focus,
        name.accesskit_id()
    );
    draw(
        &h,
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    draw(&h, &mut app, vec![]);
    let after = h.editor_frame(&mut app, child_input);
    assert_eq!(
        after
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .focus,
        name.accesskit_id()
    );
    assert!(app.main_snapshot().search.open);
}
