use eframe::egui::{self, Event, PointerButton};
use hexforge_controller::{
    controller::{AppController, InitialState, WindowSnapshot},
    shortcuts::WindowKind,
};
use hexforge_native::app::NativeApp;
use hexforge_native::views::results::panel_state;

fn draw(
    app: &mut NativeApp,
    ctx: &egui::Context,
    mut events: Vec<Event>,
) -> Vec<(String, egui::Pos2)> {
    let modifiers = events
        .iter()
        .rev()
        .find_map(|event| match event {
            Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } => {
                Some(*modifiers)
            }
            _ => None,
        })
        .unwrap_or_default();
    events.insert(0, Event::ModifiersChanged(modifiers));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::epaint::Shape::Text(t) = &s.shape {
                Some((t.galley.job.text.clone(), t.pos + t.galley.size() / 2.0))
            } else {
                None
            }
        })
        .collect()
}
fn key(key: egui::Key, modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}
#[test]
fn native_menu_access_keys_and_arrows_invoke_the_enabled_command() {
    let (_file, mut app, ctx) = opened_native();
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F, egui::Modifiers::ALT)],
    );
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(texts.iter().any(|(text, _)| text == "Open File…"));
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)],
    );
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.main_snapshot().file.is_some() {
        app.pump();
        assert!(std::time::Instant::now() < until);
        std::thread::yield_now();
    }
    assert!(
        app.picker.is_none(),
        "Down selects Close File, not Open File"
    );
}
#[test]
fn native_menu_modified_enter_does_not_click_the_focused_entry() {
    for has_file in [false, true] {
        let (_file, mut app, ctx) = if has_file {
            let (file, app, ctx) = opened_native();
            (Some(file), app, ctx)
        } else {
            let mut app = NativeApp::new(
                InitialState::default(),
                std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
            );
            let ctx = egui::Context::default();
            app.attach(&ctx);
            (None, app, ctx)
        };
        draw(
            &mut app,
            &ctx,
            vec![key(egui::Key::F, egui::Modifiers::ALT)],
        );
        draw(&mut app, &ctx, vec![]);
        draw(
            &mut app,
            &ctx,
            vec![key(egui::Key::Enter, egui::Modifiers::CTRL)],
        );
        assert!(app.picker.is_none());
        assert_eq!(app.main_snapshot().file.is_some(), has_file);
        assert!(!app.main_snapshot().template_applied);
        assert!(!ctx.input(|input| input.events.iter().any(|event| matches!(
            event,
            Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                ..
            }
        ))));
    }
}
#[test]
fn enabled_ctrl_enter_over_the_file_menu_applies_only_the_template() {
    use hexforge_controller::{action::Action, command::Command};
    use std::time::{Duration, Instant};
    let (_source, mut app, ctx) = opened_native();
    app.gesture(Action::TemplateChanged(serde_json::from_str(
        r#"{"name":"Menu focus draft","defaultEndianness":"little","fields":[{"name":"id","type":"u8"}]}"#
    ).unwrap()));
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F, egui::Modifiers::ALT)],
    );
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(texts.iter().any(|(text, _)| text == "Open File…"));
    let source = app.main_snapshot().source;
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::CTRL)],
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.main_snapshot().template_applied {
        app.pump();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        app.picker.is_none(),
        "Modified Enter cannot click the focused Open File menu entry"
    );
    assert_eq!(app.main_snapshot().source, source);
    assert_eq!(app.main_snapshot().results[0].path, "id");
    assert!(app.main_snapshot().template_state.saved_path.is_none());
    assert!(app.main_snapshot().template_state.dirty);
    assert!(app.main_snapshot().error.is_none());
    // Apply did not become a template save or a binary-copy request.
    assert_ne!(
        app.main_snapshot().completion_notice.as_deref(),
        Some("Template saved.")
    );
    assert!(hexforge_controller::policy::command_enabled(
        Command::ApplyTemplate,
        &app.main_snapshot().policy
    ));
}
#[test]
fn native_menu_access_key_remains_owned_by_active_ime_composition() {
    let (_file, mut app, ctx) = opened_native();
    draw(
        &mut app,
        &ctx,
        vec![Event::Ime(egui::ImeEvent::Preedit {
            text: "文".into(),
            active_range_chars: None,
        })],
    );
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F, egui::Modifiers::ALT)],
    );
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(!texts.iter().any(|(text, _)| text == "Open File…"));
    draw(
        &mut app,
        &ctx,
        vec![Event::Ime(egui::ImeEvent::Commit("文".into()))],
    );
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F, egui::Modifiers::ALT)],
    );
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(
        texts.iter().any(|(text, _)| text == "Open File…"),
        "access keys resume when composition ends"
    );
}
#[test]
fn modal_keeps_the_open_menu_and_blocks_main_dispatch_until_dismissed() {
    use hexforge_controller::{action::Action, command::Command, controller::RowWidth};
    let (_file, mut app, ctx) = opened_native();
    app.gesture(Action::Select { start: 2, end: 4 });
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F, egui::Modifiers::ALT)],
    );
    draw(&mut app, &ctx, vec![]);
    app.gesture(Action::DropFiles(vec![]));
    assert!(app.main_snapshot().error.is_some());
    app.invoke(Command::Row32, Default::default());
    assert_eq!(app.main_snapshot().row_width, RowWidth::Sixteen);
    for (key_value, modifiers) in [
        (egui::Key::O, egui::Modifiers::CTRL),
        (egui::Key::S, egui::Modifiers::CTRL),
    ] {
        draw(&mut app, &ctx, vec![key(key_value, modifiers)]);
        assert!(app.picker.is_none());
        assert!(app.main_snapshot().error.is_some());
        assert_eq!(
            app.main_snapshot()
                .selection
                .as_ref()
                .map(|s| (s.start, s.end)),
            Some((2, 4))
        );
    }
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::V, egui::Modifiers::ALT)],
    );
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.main_snapshot().error.is_none());
    assert_eq!(
        app.main_snapshot()
            .selection
            .as_ref()
            .map(|s| (s.start, s.end)),
        Some((2, 4))
    );
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(texts.iter().any(|(text, _)| text == "Open File…"));
    assert!(!texts.iter().any(|(text, _)| text == "Light Theme"));
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::O, egui::Modifiers::CTRL)],
    );
    assert!(
        app.picker.is_some(),
        "normal shortcut dispatch must resume after modal dismissal"
    );
}
#[test]
fn native_menu_opens_dropdown_instead_of_file_picker() {
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let texts = draw(&mut app, &ctx, vec![]);
    let p = texts.iter().find(|(text, _)| text == "File").unwrap().1;
    draw(&mut app, &ctx, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        draw(
            &mut app,
            &ctx,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(app.picker.is_none(), "opening a menu is not opening a file");
    assert!(texts.iter().any(|(text, _)| text == "Open File…"));
    assert!(texts.iter().any(|(text, _)| text == "Ctrl+Shift+S"));
}
#[test]
fn initial_main_contains_real_bottom_results_and_status_controls() {
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let texts = draw(&mut app, &ctx, vec![]);
    let results = texts
        .iter()
        .find(|(text, _)| text == "PARSED RESULTS")
        .expect("bottom panel heading");
    assert!(results.1.y > 550.0 && results.1.y < 600.0);
    assert!(texts.iter().any(|(text, _)| text == "No template loaded."));
    for label in ["16", "32", "Clean"] {
        assert!(texts.iter().any(|(text, _)| text == label));
    }
    assert!(texts
        .iter()
        .any(|(text, p)| text == "Size 0 B" && p.y > 780.0));
}
#[test]
fn real_main_shortcuts_distinguish_binary_copy_from_template_save() {
    use std::time::{Duration, Instant};
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(16).unwrap();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.gesture(hexforge_controller::action::Action::DropFiles(vec![file
        .path()
        .into()]));
    let until = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().file.is_none() {
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    let key = |shift| Event::Key {
        key: egui::Key::S,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        },
    };
    draw(&mut app, &ctx, vec![key(false)]);
    assert!(app.picker.is_none());
    draw(&mut app, &ctx, vec![key(true)]);
    assert_eq!(
        app.picker.as_ref().map(|(_, p)| p.purpose),
        Some(hexforge_controller::dialog::PickerPurpose::BinarySave)
    );
}

#[test]
fn reserved_main_save_keys_never_save_background_drafts_in_any_binary_state() {
    use hexforge_controller::{action::Action, command::Command};
    use std::time::{Duration, Instant};
    for binary_dirty in [None, Some(false), Some(true)] {
        let source = tempfile::NamedTempFile::new().unwrap();
        let templates = tempfile::tempdir().unwrap();
        let saved_template = templates.path().join("header.json");
        source.as_file().set_len(16).unwrap();
        let mut app = NativeApp::new(
            InitialState::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        let ctx = egui::Context::default();
        app.attach(&ctx);
        if binary_dirty.is_some() {
            app.gesture(Action::DropFiles(vec![source.path().into()]));
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.main_snapshot().file.is_none() {
                app.pump();
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            app.gesture(Action::Select { start: 0, end: 0 });
        } else {
            std::fs::write(
                &saved_template,
                br#"{"name":"Header","defaultEndianness":"little","fields":[]}"#,
            )
            .unwrap();
            app.invoke(Command::LoadTemplate, Default::default());
            let picker = app.picker.take().unwrap();
            app.runtime
                .picker_finished(picker.0, Ok(Some(saved_template.clone())));
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                app.pump();
                if !app.main_snapshot().policy.operation_busy {
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert!(app.main_snapshot().template_state.saved_path.is_some());
        }
        let template = serde_json::from_str(r#"{"name":"Unsaved template","defaultEndianness":"little","fields":[{"name":"id","type":"u8"}]}"#).unwrap();
        app.gesture(Action::TemplateChanged(template));
        if binary_dirty == Some(true) {
            app.invoke(Command::ToggleEdit, Default::default());
            app.gesture(Action::EditSelectedByte("FF".into()));
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.main_snapshot().policy.operation_busy {
                app.pump();
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert!(app.main_snapshot().file.as_ref().unwrap().dirty);
        }
        let before = app.main_snapshot();
        draw(
            &mut app,
            &ctx,
            vec![key(
                egui::Key::S,
                egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            )],
        );
        assert!(
            app.picker.is_none(),
            "Ctrl+S is reserved in the main window, never a save path"
        );
        assert_eq!(
            app.main_snapshot().template_state.definition,
            before.template_state.definition
        );
        assert!(app.main_snapshot().template_state.dirty);
        assert_eq!(
            app.main_snapshot().template_state.saved_path,
            before.template_state.saved_path
        );
        assert_eq!(app.main_snapshot().selection, before.selection);
        assert_eq!(
            app.main_snapshot()
                .file
                .as_ref()
                .map(|f| (&f.path, f.dirty)),
            before.file.as_ref().map(|f| (&f.path, f.dirty))
        );
        assert!(!ctx.input(|input| input.events.iter().any(|event| matches!(
            event,
            Event::Key {
                key: egui::Key::S,
                pressed: true,
                ..
            }
        ))));
        draw(
            &mut app,
            &ctx,
            vec![key(
                egui::Key::S,
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            )],
        );
        assert_eq!(
            app.picker.as_ref().map(|(_, p)| p.purpose),
            binary_dirty.map(|_| hexforge_controller::dialog::PickerPurpose::BinarySave)
        );
        assert_eq!(
            app.main_snapshot().template_state.saved_path,
            before.template_state.saved_path
        );
        assert!(app.main_snapshot().template_state.dirty);
        assert!(!ctx.input(|input| input.events.iter().any(|event| matches!(
            event,
            Event::Key {
                key: egui::Key::S,
                pressed: true,
                ..
            }
        ))));
        if binary_dirty.is_none() {
            let saved: hexforge_core::template::TemplateDefinition =
                serde_json::from_slice(&std::fs::read(&saved_template).unwrap()).unwrap();
            assert_eq!(
                saved.name, "Header",
                "Reserved main-window keys must not save even a bound template"
            );
            assert_eq!(
                app.main_snapshot().template_state.display_name,
                "header.json"
            );
        }
        assert_eq!(
            std::fs::read(source.path()).unwrap(),
            vec![0; 16],
            "Neither shortcut overwrites the protected source"
        );
    }
}

#[test]
fn native_binary_save_shortcut_cancel_and_failure_preserve_edits_and_coalesce_the_picker() {
    use hexforge_controller::{action::Action, command::Command};
    use std::time::{Duration, Instant};
    for fail_save in [false, true] {
        let source = tempfile::NamedTempFile::new().unwrap();
        source.as_file().set_len(1024).unwrap();
        let destination_directory = tempfile::tempdir().unwrap();
        let mut app = NativeApp::new(
            InitialState::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        let ctx = egui::Context::default();
        app.attach(&ctx);
        app.gesture(Action::DropFiles(vec![source.path().into()]));
        let wait = |app: &mut NativeApp| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.main_snapshot().policy.operation_busy {
                app.pump();
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        };
        wait(&mut app);
        app.gesture(Action::Select {
            start: 128,
            end: 128,
        });
        app.invoke(Command::ToggleEdit, Default::default());
        app.gesture(Action::EditSelectedByte("FF".into()));
        wait(&mut app);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            draw(&mut app, &ctx, vec![]);
            app.pump();
            if app
                .main_snapshot()
                .page
                .as_ref()
                .is_some_and(|p| p.revision == "1" && p.bytes.get(128) == Some(&255))
            {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let before = app.main_snapshot();
        assert!(before.file.as_ref().unwrap().dirty);
        let save_key = || {
            key(
                egui::Key::S,
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            )
        };
        draw(&mut app, &ctx, vec![save_key()]);
        let (id, picker) = app
            .picker
            .take()
            .expect("Actual main Save As shortcut must request a binary picker");
        assert_eq!(
            picker.purpose,
            hexforge_controller::dialog::PickerPurpose::BinarySave
        );
        draw(&mut app, &ctx, vec![save_key()]);
        assert!(
            app.picker.is_none(),
            "Pending Save As owns its single picker"
        );
        app.runtime.picker_finished(
            id,
            Ok(if fail_save {
                Some(destination_directory.path().into())
            } else {
                None
            }),
        );
        app.pump();
        wait(&mut app);
        let after = app.main_snapshot();
        assert_eq!(after.source, before.source);
        assert_eq!(after.selection, before.selection);
        assert_eq!(after.file.as_ref().unwrap().revision, "1");
        assert!(after.file.as_ref().unwrap().dirty);
        assert!(after.can_undo);
        let page = after
            .page
            .as_ref()
            .expect("Cancel/failure must retain the accepted effective bytes");
        assert_eq!(page.bytes.get(128), Some(&255));
        assert!(page.modified_offsets.contains(&"128".into()));
        assert_eq!(
            after.notice_revision, before.notice_revision,
            "A cancelled/failed copy cannot report new save success"
        );
        assert_eq!(after.error.is_some(), fail_save);
        assert_eq!(
            std::fs::read(source.path()).unwrap()[128],
            0,
            "A cancelled/failed copy never modifies the source"
        );
        app.gesture(Action::ClearError);
        draw(&mut app, &ctx, vec![save_key()]);
        assert_ne!(
            app.picker
                .as_ref()
                .expect("Cancel/failure releases duplicate-action protection")
                .0,
            id
        );
    }
}

#[test]
fn main_binary_copy_shortcut_keeps_an_invalid_template_independent_and_never_applies_it() {
    use hexforge_controller::dialog::PickerPurpose;
    use hexforge_controller::{action::Action, command::Command};
    use std::time::{Duration, Instant};
    let (source, mut app, ctx) = opened_native();
    let copies = tempfile::tempdir().unwrap();
    let destination = copies.path().join("copy.bin");
    app.gesture(Action::Select { start: 4, end: 4 });
    app.invoke(Command::ToggleEdit, Default::default());
    app.gesture(Action::EditSelectedByte("FF".into()));
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let template = serde_json::from_str(r#"{"name":"Unsaved draft","defaultEndianness":"little","fields":[{"name":"id","type":"u8"}]}"#).unwrap();
    app.gesture(Action::TemplateDraft {
        definition: template,
        valid: false,
    });
    let before = app.main_snapshot();
    assert!(!before.policy.template_valid);
    draw(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::S,
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        )],
    );
    let (id, picker) = app.picker.take().unwrap();
    assert_eq!(picker.purpose, PickerPurpose::BinarySave);
    assert_eq!(
        picker.default_path,
        Some(std::path::PathBuf::from(format!(
            "{}.copy",
            source.path().file_name().unwrap().to_string_lossy()
        )))
    );
    app.runtime
        .picker_finished(id, Ok(Some(destination.clone())));
    app.pump();
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let after = app.main_snapshot();
    assert!(after.error.is_none());
    assert!(after
        .file
        .as_ref()
        .unwrap()
        .path
        .ends_with(destination.to_string_lossy().as_ref()));
    assert!(!after.file.as_ref().unwrap().dirty);
    assert_eq!(after.selection, before.selection);
    assert_eq!(
        after.template_state.definition,
        before.template_state.definition
    );
    assert_eq!(after.template_state.saved_path, None);
    assert!(after.template_state.dirty && !after.template_state.valid);
    assert!(!after.template_applied && after.results.is_empty());
    assert_eq!(std::fs::read(source.path()).unwrap()[4], 0);
    assert_eq!(std::fs::read(&destination).unwrap()[4], 255);
}
#[test]
fn disabled_native_shortcuts_are_consumed_without_focused_control_fallthrough() {
    use hexforge_controller::command::Command;
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    draw(&mut app, &ctx, vec![]);
    for (key_value, modifiers) in [
        (egui::Key::W, egui::Modifiers::CTRL),
        (egui::Key::F, egui::Modifiers::CTRL),
        (egui::Key::G, egui::Modifiers::CTRL),
        (egui::Key::Enter, egui::Modifiers::CTRL),
        (egui::Key::F3, egui::Modifiers::NONE),
        (egui::Key::F3, egui::Modifiers::SHIFT),
        (egui::Key::S, egui::Modifiers::CTRL),
        (
            egui::Key::S,
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        ),
    ] {
        draw(&mut app, &ctx, vec![key(key_value, modifiers)]);
        assert!(
            !ctx.input(|input| input.events.iter().any(|event| matches!(event,
            Event::Key { key, pressed: true, .. } if *key == key_value))),
            "disabled {modifiers:?}+{key_value:?} must not reach a focused widget"
        );
        assert!(app.picker.is_none());
        assert!(app.main_snapshot().file.is_none());
        assert!(!app.main_snapshot().search.open && !app.main_snapshot().goto.open);
        assert!(app.main_snapshot().error.is_none());
    }
    // Open is normally enabled; a real pending picker disables it. A second
    // Ctrl+O must consume the key without replacing that accepted request.
    app.invoke(Command::Open, Default::default());
    let pending = app.picker.as_ref().unwrap().0;
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::O, egui::Modifiers::CTRL)],
    );
    assert!(!ctx.input(|input| input.events.iter().any(|event| matches!(
        event,
        Event::Key {
            key: egui::Key::O,
            pressed: true,
            ..
        }
    ))));
    assert_eq!(app.picker.as_ref().unwrap().0, pending);
}
#[test]
fn removed_main_shortcuts_do_not_change_native_template_theme_or_panels() {
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    draw(&mut app, &ctx, vec![]);
    for (key_value, modifiers) in [
        (
            egui::Key::D,
            egui::Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            },
        ),
        (
            egui::Key::L,
            egui::Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            },
        ),
        (egui::Key::B, egui::Modifiers::CTRL),
        (
            egui::Key::B,
            egui::Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            },
        ),
        (
            egui::Key::A,
            egui::Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            },
        ),
    ] {
        draw(&mut app, &ctx, vec![key(key_value, modifiers)]);
        assert!(
            ctx.input(|input| input.events.iter().any(|event| matches!(event,
            Event::Key { key, pressed: true, .. } if *key == key_value))),
            "removed key must not be consumed as an app command"
        );
        assert!(app.picker.is_none() && app.windows.editor().is_none());
        assert!(app.main_snapshot().template.fields.is_empty());
        assert_eq!(
            app.main_snapshot().theme,
            hexforge_controller::controller::Theme::Dark
        );
        assert!(!app.results_preferences().collapsed);
        assert!(app.main_snapshot().error.is_none());
    }
}
#[test]
fn native_text_input_retains_undo_and_f2_without_editing_file_bytes() {
    use hexforge_controller::command::Command;
    let (_file, mut app, ctx) = opened_native();
    app.invoke(Command::Search, Default::default());
    for _ in 0..3 {
        draw(&mut app, &ctx, vec![]);
    }
    draw(&mut app, &ctx, vec![Event::Text("41".into())]);
    assert_eq!(app.main_snapshot().search.raw, "41");
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F2, egui::Modifiers::NONE)],
    );
    assert!(app.main_snapshot().edit_dialog.is_none());
    draw(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::Z,
            egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        )],
    );
    assert_eq!(
        app.main_snapshot().search.raw,
        "",
        "Undo belongs to the focused search input"
    );
    assert_eq!(app.main_snapshot().file.as_ref().unwrap().revision, "0");
    assert!(!app.main_snapshot().file.as_ref().unwrap().dirty);
}
#[test]
fn native_escape_closes_popups_before_clearing_selection() {
    use hexforge_controller::{action::Action, command::Command};
    let (_file, mut app, ctx) = opened_native();
    app.gesture(Action::Select { start: 2, end: 4 });
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::F, egui::Modifiers::ALT)],
    );
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert_eq!(
        app.main_snapshot()
            .selection
            .as_ref()
            .map(|s| (s.start, s.end)),
        Some((2, 4))
    );
    assert!(!draw(&mut app, &ctx, vec![])
        .iter()
        .any(|(text, _)| text == "Open File…"));
    app.invoke(Command::Search, Default::default());
    draw(&mut app, &ctx, vec![]);
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!app.main_snapshot().search.open);
    assert!(app.main_snapshot().selection.is_some());
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.main_snapshot().selection.is_none());
    assert!(app.main_snapshot().template_range.is_none());
}
#[test]
fn every_reference_main_shortcut_reaches_its_real_native_command() {
    use hexforge_controller::dialog::PickerPurpose;
    use hexforge_controller::{
        action::Action,
        command::Command,
        controller::{RowWidth, Theme},
    };
    use hexforge_core::template::TemplateDefinition;
    let wait = |app: &mut NativeApp, ready: &dyn Fn(&NativeApp) -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !ready(app) {
            app.pump();
            assert!(std::time::Instant::now() < deadline, "native command did not settle: error={:?}, busy={}, dialog={:?}, editor_close={}, window_closing={}",
                app.main_snapshot().error, app.main_snapshot().policy.operation_busy,
                app.main_snapshot().dialog, app.main_snapshot().editor_close_prompt, app.windows.main_closing());
            std::thread::yield_now();
        }
    };
    for (key_value, modifiers, command) in [
        (egui::Key::O, 1, Command::Open),
        (egui::Key::W, 1, Command::CloseFile),
        (egui::Key::S, 3, Command::SaveAs),
        (egui::Key::E, 3, Command::Export),
        (egui::Key::F4, 4, Command::Exit),
        (egui::Key::F2, 0, Command::EditSelected),
        (egui::Key::E, 5, Command::ToggleEdit),
        (egui::Key::Z, 1, Command::Undo),
        (egui::Key::G, 1, Command::Goto),
        (egui::Key::F, 1, Command::Search),
        (egui::Key::Enter, 1, Command::ApplyTemplate),
        (egui::Key::O, 5, Command::LoadTemplate),
        (egui::Key::T, 5, Command::ThemeToggle),
        (egui::Key::Num1, 1, Command::Row16),
        (egui::Key::Num2, 1, Command::Row32),
    ] {
        let (_file, mut app, ctx) = opened_native();
        if matches!(command, Command::Export | Command::ApplyTemplate) {
            let template: TemplateDefinition = serde_json::from_value(serde_json::json!({
                "name": "Keyboard fixture", "defaultEndianness": "little",
                "fields": [{"name": "value", "type": "u8", "placement": {"mode": "sequential"}}]
            }))
            .unwrap();
            app.gesture(Action::TemplateChanged(template));
            if command == Command::Export {
                app.invoke(Command::ApplyTemplate, Default::default());
                wait(&mut app, &|app| app.main_snapshot().template_applied);
                assert_eq!(app.main_snapshot().results.len(), 1);
            }
        }
        if command == Command::EditSelected {
            app.invoke(Command::ToggleEdit, Default::default());
            app.gesture(Action::Select { start: 4, end: 4 });
        }
        if command == Command::Undo {
            app.invoke(Command::ToggleEdit, Default::default());
            app.gesture(Action::Select { start: 0, end: 0 });
            app.invoke(Command::EditSelected, Default::default());
            app.gesture(Action::EditSelectedByte("41".into()));
            wait(&mut app, &|app| app.main_snapshot().can_undo);
            assert!(app.main_snapshot().file.as_ref().unwrap().dirty);
        }
        if command == Command::Row16 {
            app.invoke(Command::Row32, Default::default());
        }
        draw(
            &mut app,
            &ctx,
            vec![key(
                key_value,
                egui::Modifiers {
                    ctrl: modifiers & 1 != 0,
                    command: modifiers & 1 != 0,
                    shift: modifiers & 2 != 0,
                    alt: modifiers & 4 != 0,
                    ..Default::default()
                },
            )],
        );
        match command {
            Command::Open => assert_eq!(
                app.picker.as_ref().unwrap().1.purpose,
                PickerPurpose::BinaryOpen
            ),
            Command::CloseFile => wait(&mut app, &|app| app.main_snapshot().file.is_none()),
            Command::SaveAs => assert_eq!(
                app.picker.as_ref().unwrap().1.purpose,
                PickerPurpose::BinarySave
            ),
            Command::Export => {
                assert_eq!(app.picker.as_ref().unwrap().1.purpose, PickerPurpose::Csv)
            }
            Command::Exit => wait(&mut app, &|app| app.windows.main_closing()),
            Command::EditSelected => {
                assert_eq!(app.main_snapshot().edit_dialog.as_ref().unwrap().offset, 4)
            }
            Command::ToggleEdit => assert!(app.main_snapshot().edit_mode),
            Command::Undo => {
                wait(&mut app, &|app| !app.main_snapshot().can_undo);
                assert!(!app.main_snapshot().file.as_ref().unwrap().dirty);
            }
            Command::Goto => assert!(app.main_snapshot().goto.open),
            Command::Search => assert!(app.main_snapshot().search.open),
            Command::ApplyTemplate => wait(&mut app, &|app| app.main_snapshot().template_applied),
            Command::LoadTemplate => assert_eq!(
                app.picker.as_ref().unwrap().1.purpose,
                PickerPurpose::TemplateLoad
            ),
            Command::ThemeToggle => assert_eq!(app.main_snapshot().theme, Theme::Light),
            Command::Row16 => assert_eq!(app.main_snapshot().row_width, RowWidth::Sixteen),
            Command::Row32 => assert_eq!(app.main_snapshot().row_width, RowWidth::ThirtyTwo),
            _ => unreachable!(),
        }
        assert!(app.main_snapshot().error.is_none(), "{command:?} failed");
    }
}

#[test]
fn actual_main_unload_shortcut_clears_the_clean_binding_results_and_owned_range_only() {
    use hexforge_controller::{action::Action, command::Command};
    let (_source, mut app, ctx) = opened_native();
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("header.json");
    let original = br#"{"name":"Loaded header","defaultEndianness":"little","fields":[{"name":"id","type":"u8"}]}"#;
    std::fs::write(&template_path, original).unwrap();
    app.invoke(Command::LoadTemplate, Default::default());
    let id = app.picker.take().unwrap().0;
    app.runtime
        .picker_finished(id, Ok(Some(template_path.clone())));
    let wait = |app: &mut NativeApp, ready: &dyn Fn(&NativeApp) -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !ready(app) {
            app.pump();
            assert!(
                std::time::Instant::now() < deadline,
                "native unload trace did not settle: {:?}",
                app.main_snapshot().error
            );
            std::thread::yield_now();
        }
    };
    wait(&mut app, &|app| {
        app.main_snapshot().template_state.saved_path.is_some()
    });
    assert!(!app.main_snapshot().template_applied);
    assert!(app.windows.editor().is_none());
    app.invoke(Command::ApplyTemplate, Default::default());
    wait(&mut app, &|app| app.main_snapshot().template_applied);
    app.gesture(Action::NavigateTemplate { start: 0, end: 0 });
    let before = app.main_snapshot();
    assert!(before.template_range.is_some());
    assert!(!before.template_state.dirty);
    draw(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::U,
            egui::Modifiers {
                ctrl: true,
                command: true,
                alt: true,
                ..Default::default()
            },
        )],
    );
    wait(&mut app, &|app| !app.main_snapshot().template_state.active);
    let after = app.main_snapshot();
    assert_eq!(after.source, before.source);
    assert_eq!(
        after.file.as_ref().unwrap().path,
        before.file.as_ref().unwrap().path
    );
    assert_eq!(
        after.file.as_ref().unwrap().dirty,
        before.file.as_ref().unwrap().dirty
    );
    assert!(after.template_state.saved_path.is_none());
    assert!(after.results.is_empty());
    assert!(!after.template_applied);
    assert!(after.template_range.is_none());
    assert!(after.selection.is_none());
    assert!(after.dialog.is_none());
    assert!(after.error.is_none());
    assert!(app.windows.editor().is_none());
    assert_eq!(std::fs::read(template_path).unwrap(), original);
}
fn opened_native() -> (tempfile::NamedTempFile, NativeApp, egui::Context) {
    opened_native_contents(&[0; 512])
}
fn opened_native_contents(bytes: &[u8]) -> (tempfile::NamedTempFile, NativeApp, egui::Context) {
    use std::time::{Duration, Instant};
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.gesture(hexforge_controller::action::Action::DropFiles(vec![file
        .path()
        .into()]));
    let until = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().file.is_none() {
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    (file, app, ctx)
}
fn click_text(app: &mut NativeApp, ctx: &egui::Context, label: &str) {
    let texts = draw(app, ctx, vec![]);
    let p = texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("missing {label}; texts={texts:?}"))
        .1;
    draw(app, ctx, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        draw(
            app,
            ctx,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
}
#[test]
fn floating_search_clear_preserves_byte_selection_and_has_no_modal() {
    use hexforge_controller::{action::Action, command::Command};
    let mut bytes = [0; 512];
    bytes[8] = 0x41;
    let (_file, mut app, ctx) = opened_native_contents(&bytes);
    app.invoke(Command::Search, Default::default());
    assert!(app.main_snapshot().search.open);
    draw(&mut app, &ctx, vec![]); // Area measurement is not a presented control.
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(texts
        .iter()
        .any(|(text, _)| text == "Enter hexadecimal bytes, then press Enter."));
    app.gesture(Action::Select { start: 5, end: 7 });
    assert_eq!(app.main_snapshot().selection.unwrap().count(), 3);
    app.gesture(Action::SetSearchPattern("4".into()));
    click_text(&mut app, &ctx, "Search");
    assert!(!app.main_snapshot().policy.operation_busy);
    assert!(app
        .main_snapshot()
        .error
        .as_ref()
        .unwrap()
        .message
        .contains("two hexadecimal"));
    assert_eq!(app.main_snapshot().selection.unwrap().start, 5);
    app.gesture(Action::ClearError);
    app.gesture(Action::SetSearchPattern("41".into()));
    click_text(&mut app, &ctx, "Search");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(app.main_snapshot().matches, [8]);
    assert_eq!(app.main_snapshot().selection.unwrap().start, 8);
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!app.main_snapshot().search.open);
    assert_eq!(app.main_snapshot().matches, [8]);
    app.invoke(Command::Search, Default::default());
    assert_eq!(app.main_snapshot().search.raw, "41");
    draw(&mut app, &ctx, vec![]);
    click_text(&mut app, &ctx, "Clear");
    let s = app.main_snapshot();
    assert_eq!(s.search.raw, "");
    assert!(s.matches.is_empty());
    assert_eq!(s.selection.unwrap().start, 8);
    assert!(s.dialog.is_none());
}
#[test]
fn real_goto_input_navigates_and_displays_errors_inline() {
    use hexforge_controller::{action::Action, command::Command};
    let (_file, mut app, ctx) = opened_native();
    app.invoke(Command::Goto, Default::default());
    assert!(app.main_snapshot().goto.open);
    draw(&mut app, &ctx, vec![]);
    let texts = draw(&mut app, &ctx, vec![]);
    assert!(texts
        .iter()
        .any(|(text, _)| text == "Enter a decimal or 0x-prefixed hexadecimal offset."));
    for input in ["512", "0xZZ"] {
        app.gesture(Action::SetGotoValue(input.into()));
        click_text(&mut app, &ctx, "Go");
        let texts = draw(&mut app, &ctx, vec![]);
        let s = app.main_snapshot();
        assert!(s.goto.open);
        assert_eq!(s.goto.raw, input);
        assert!(s.error.is_none());
        assert!(s.dialog.is_none());
        assert!(texts
            .iter()
            .any(|(text, _)| Some(text.as_str()) == s.goto.error.as_deref()));
    }
    app.gesture(Action::SetGotoValue("0x100".into()));
    click_text(&mut app, &ctx, "Go");
    assert_eq!(app.main_snapshot().selection.unwrap().start, 256);
    assert!(!app.main_snapshot().goto.open);
}
#[test]
fn floating_bars_focus_their_inputs_after_native_measurement() {
    use hexforge_controller::command::Command;
    let (_file, mut app, ctx) = opened_native();
    for (command, id) in [
        (Command::Search, "search-bytes"),
        (Command::Goto, "goto-offset"),
        (Command::Search, "search-bytes"),
    ] {
        app.invoke(command, Default::default());
        draw(&mut app, &ctx, vec![]);
        draw(&mut app, &ctx, vec![]);
        assert!(
            ctx.memory(|m| m.has_focus(egui::Id::new(id))),
            "{id} must accept typing immediately"
        );
        let s = app.main_snapshot();
        assert_eq!(s.search.open, command == Command::Search);
        assert_eq!(s.goto.open, command == Command::Goto);
        assert!(s.dialog.is_none());
    }
}

#[test]
fn live_search_refocus_and_match_controls_preserve_drafts_without_repeating_the_scan() {
    use hexforge_controller::{action::Action, command::Command, operation::OperationKind};
    use std::time::{Duration, Instant};
    let mut bytes = [0; 512];
    for start in [4, 8, 16] {
        bytes[start..start + 2].copy_from_slice(&[0x41, 0x42]);
    }
    let (_file, mut app, ctx) = opened_native_contents(&bytes);
    // Windows egui-winit supplies both ctrl and the logical command modifier.
    // TextEdit uses command for Select All; application chords use ctrl.
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    draw(&mut app, &ctx, vec![key(egui::Key::F, ctrl)]);
    draw(&mut app, &ctx, vec![]);
    draw(&mut app, &ctx, vec![Event::Text("41 4".into())]);
    draw(&mut app, &ctx, vec![key(egui::Key::F, ctrl)]);
    draw(&mut app, &ctx, vec![]);
    assert_eq!(app.main_snapshot().search.raw, "41 4");
    assert!(ctx.memory(|m| m.has_focus(egui::Id::new("search-bytes"))));
    draw(&mut app, &ctx, vec![key(egui::Key::A, ctrl)]);
    draw(&mut app, &ctx, vec![Event::Text("41 42".into())]);
    assert_eq!(app.main_snapshot().search.raw, "41 42");
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, Default::default())],
    );
    assert!(app.main_snapshot().busy[&OperationKind::Search]);
    let until = Instant::now() + Duration::from_secs(3);
    while app
        .main_snapshot()
        .busy
        .get(&OperationKind::Search)
        .copied()
        .unwrap_or(false)
    {
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    let expect_match = |app: &NativeApp, start, end| {
        let s = app.main_snapshot();
        assert_eq!(s.matches, [4, 8, 16]);
        let selection = s.selection.unwrap();
        assert_eq!(
            (selection.start, selection.end, selection.count()),
            (start, end, 2)
        );
        assert!(
            !s.busy.get(&OperationKind::Search).copied().unwrap_or(false),
            "match controls must not submit another scan even before pumping its replies"
        );
    };
    expect_match(&app, 4, 5);
    click_text(&mut app, &ctx, "↓");
    expect_match(&app, 8, 9);
    // The pointer leaves focus on the button; Ctrl+F restores the real input.
    draw(&mut app, &ctx, vec![key(egui::Key::F, ctrl)]);
    draw(&mut app, &ctx, vec![]);
    draw(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::Enter,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        )],
    );
    expect_match(&app, 4, 5);
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, Default::default())],
    );
    expect_match(&app, 8, 9);
    draw(&mut app, &ctx, vec![key(egui::Key::F3, Default::default())]);
    expect_match(&app, 16, 17);
    draw(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::F3,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        )],
    );
    expect_match(&app, 8, 9);
    draw(&mut app, &ctx, vec![key(egui::Key::A, ctrl)]);
    draw(&mut app, &ctx, vec![Event::Text("41   42".into())]);
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, Default::default())],
    );
    expect_match(&app, 16, 17);
    app.gesture(Action::SetSearchPattern("CC".into()));
    draw(&mut app, &ctx, vec![key(egui::Key::F3, Default::default())]);
    assert_eq!(app.main_snapshot().selection.unwrap().start, 16);
    assert!(!app.main_snapshot().policy.has_search_matches);
    draw(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, Default::default())],
    );
    assert!(
        app.main_snapshot().busy[&OperationKind::Search],
        "a changed query must start a fresh scan"
    );
    let until = Instant::now() + Duration::from_secs(3);
    while app
        .main_snapshot()
        .busy
        .get(&OperationKind::Search)
        .copied()
        .unwrap_or(false)
    {
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert_eq!(app.main_snapshot().search.query, "CC");
    assert!(app.main_snapshot().matches.is_empty());
    click_text(&mut app, &ctx, "Clear");
    assert!(app.main_snapshot().matches.is_empty());
    assert_eq!(app.main_snapshot().selection.unwrap().start, 16);
    assert!(!app.main_snapshot().policy.has_search_matches);
    app.invoke(Command::Goto, Default::default());
    assert!(!app.main_snapshot().search.open);
}
#[test]
fn real_result_tree_keyboard_navigation_survives_row_click() {
    use hexforge_controller::{action::Action, command::Command};
    use std::time::{Duration, Instant};
    let (_file, mut app, ctx) = opened_native();
    let template=serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"first","type":"u8","placement":{"mode":"sequential"}},{"name":"second","type":"u8","placement":{"mode":"sequential"}}]})).unwrap();
    app.gesture(Action::TemplateChanged(template));
    app.invoke(Command::ApplyTemplate, Default::default());
    let until = Instant::now() + Duration::from_secs(3);
    while !app.main_snapshot().template_applied {
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    click_text(&mut app, &ctx, "first");
    assert_eq!(app.main_snapshot().selection.unwrap().start, 0);
    for key in [egui::Key::ArrowDown, egui::Key::Enter] {
        draw(
            &mut app,
            &ctx,
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
    }
    assert_eq!(app.main_snapshot().selection.unwrap().start, 1);
    assert_eq!(app.main_snapshot().template_range.unwrap().start, 1);
}
#[test]
fn only_pending_saved_template_has_panel_buttons() {
    let c = AppController::new(InitialState::default());
    let WindowSnapshot::Main(mut s) = c.snapshot(WindowKind::Main) else {
        unreachable!()
    };
    assert_eq!(
        panel_state(&s).message.as_deref(),
        Some("No template loaded.")
    );
    assert!(!panel_state(&s).actions);
    s.file = Some(hexforge_core::dto::FileInfoDto {
        name: "a.bin".into(),
        path: "a.bin".into(),
        size: "10".into(),
        revision: "0".into(),
        dirty: false,
    });
    assert_eq!(
        panel_state(&s).message.as_deref(),
        Some("No template loaded. Load or create template from Template menu.")
    );
    use hexforge_controller::template::status::TemplateSource;
    s.template_state.source = TemplateSource::Draft;
    s.template_state.display_name = "Untitled".into();
    assert_eq!(
        panel_state(&s).message.as_deref(),
        Some("Unsaved template draft: \"Untitled\".")
    );
    assert!(!panel_state(&s).actions);
    s.template_state.source = TemplateSource::File;
    s.template_state.display_name = "example.json".into();
    assert_eq!(
        panel_state(&s).message.as_deref(),
        Some("Template: \"example.json\" loaded, pending apply.")
    );
    assert!(panel_state(&s).actions);
    s.template_applied = true;
    assert!(panel_state(&s).show_results);
    assert!(!panel_state(&s).actions);
    s.file = None;
    assert_eq!(
        panel_state(&s).message.as_deref(),
        Some("Template: \"example.json\" loaded. Open binary file to preview.")
    );
    assert!(!panel_state(&s).actions);
}
