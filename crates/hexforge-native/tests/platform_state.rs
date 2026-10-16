use eframe::egui;
use hexforge_controller::{
    command::Command,
    controller::{InitialState, Theme},
    preferences::PanelPreferences,
};
use hexforge_core::operations::CoreService;
use hexforge_native::{
    app::NativeApp,
    platform::{
        preferences::{PreferenceError, PreferenceStore, ResultsPreferences},
        theme::SystemThemeSource,
    },
};
use std::sync::Arc;

fn frame(app: &mut NativeApp, ctx: &egui::Context, theme: Option<egui::Theme>) {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            system_theme: theme,
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
}
#[test]
fn manual_theme_survives_until_actual_system_change() {
    let mut source = SystemThemeSource::from_initial(Theme::Dark);
    assert_eq!(source.current(), Theme::Dark);
    assert_eq!(source.observe(Theme::Dark), None);
    assert_eq!(source.observe(Theme::Light), Some(Theme::Light));
    assert_eq!(source.observe(Theme::Light), None);
    assert_eq!(source.observe(Theme::Dark), Some(Theme::Dark));

    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    app.invoke(Command::ThemeToggle, Default::default());
    assert_eq!(app.main_snapshot().theme, Theme::Light);
    for _ in 0..3 {
        frame(&mut app, &ctx, Some(egui::Theme::Dark));
    }
    assert_eq!(
        app.main_snapshot().theme,
        Theme::Light,
        "polling an unchanged OS preference must not erase the manual override"
    );
    frame(&mut app, &ctx, Some(egui::Theme::Light));
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    assert_eq!(app.main_snapshot().theme, Theme::Dark);
}

#[test]
fn initial_and_reopened_apps_paint_the_system_theme_not_a_stale_context_override() {
    for (system, stale, background) in [
        (
            Theme::Light,
            egui::Theme::Dark,
            egui::Color32::from_rgb(246, 248, 250),
        ),
        (
            Theme::Dark,
            egui::Theme::Light,
            egui::Color32::from_rgb(17, 20, 24),
        ),
    ] {
        let ctx = egui::Context::default();
        ctx.set_theme(stale);
        let initial = || InitialState {
            theme: system,
            ..Default::default()
        };
        let mut app = NativeApp::new(initial(), Arc::new(CoreService::default()));
        app.attach(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| app.paint_main(ui),
        );
        assert!(output.shapes.iter().any(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.rect == screen && r.fill == background)),
            "The real first interface must use the selected OS palette, not the stale egui context");
        output.textures_delta.clear();
        assert_eq!(app.main_snapshot().theme, system);
        app.invoke(Command::ThemeToggle, Default::default());
        let detached = app.main_snapshot();
        assert_ne!(detached.theme, system);
        drop(app);
        let mut reopened = NativeApp::new(initial(), Arc::new(CoreService::default()));
        reopened.attach(&ctx);
        frame(&mut reopened, &ctx, None);
        assert_eq!(reopened.main_snapshot().theme, system);
        assert_ne!(
            detached.theme,
            reopened.main_snapshot().theme,
            "A disposed presenter has no global theme listener or hidden session override"
        );
    }
}

#[test]
fn native_theme_updates_are_deterministic_and_recolor_both_windows() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.invoke(Command::TemplateEditor, Default::default());
    for (observed, theme) in [
        (egui::Theme::Light, Theme::Light),
        (egui::Theme::Light, Theme::Light),
        (egui::Theme::Dark, Theme::Dark),
        (egui::Theme::Dark, Theme::Dark),
    ] {
        frame(&mut app, &ctx, Some(observed));
        assert_eq!(app.main_snapshot().theme, theme);
        assert_eq!(app.editor_snapshot().theme, theme);
        assert!(app.main_snapshot().error.is_none());
    }
}

#[test]
fn unavailable_system_theme_falls_back_to_dark_without_disabling_manual_theme() {
    let source = SystemThemeSource::from_preference(None);
    let mut app = NativeApp::new(
        InitialState {
            theme: source.current(),
            ..Default::default()
        },
        Arc::new(CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    frame(&mut app, &ctx, None);
    assert_eq!(app.main_snapshot().theme, Theme::Dark);
    app.invoke(Command::ThemeToggle, Default::default());
    frame(&mut app, &ctx, None);
    assert_eq!(app.main_snapshot().theme, Theme::Light);
    assert!(app.main_snapshot().error.is_none());
}
#[test]
fn editor_and_main_share_theme_without_changing_drafts_or_bindings() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.invoke(Command::TemplateEditor, Default::default());
    app.editor.as_mut().unwrap().model.name = "Pending draft".into();
    app.publish_editor().unwrap();
    let before = app.editor_snapshot().template_state;
    frame(&mut app, &ctx, Some(egui::Theme::Light));
    assert_eq!(app.main_snapshot().theme, Theme::Light);
    assert_eq!(app.editor_snapshot().theme, Theme::Light);
    assert_eq!(
        app.editor_snapshot().template_state.definition,
        before.definition
    );
    assert_eq!(
        app.editor_snapshot().template_state.workspace_revision,
        before.workspace_revision
    );
    assert_eq!(app.editor_snapshot().template_state.dirty, before.dirty);
    app.invoke(Command::ThemeToggle, Default::default());
    assert_eq!(app.editor_snapshot().theme, Theme::Dark);
}
struct UnavailableStore;
impl PreferenceStore for UnavailableStore {
    fn read_results_preferences(&self) -> Result<Option<ResultsPreferences>, PreferenceError> {
        Err(PreferenceError::Unavailable)
    }
    fn write_results_preferences(&self, _: &ResultsPreferences) -> Result<(), PreferenceError> {
        Err(PreferenceError::Unavailable)
    }
}
#[test]
fn optional_preferences_fail_nonfatally_and_controls_still_work() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.configure_preferences(Arc::new(UnavailableStore));
    assert_eq!(
        app.results_preferences(),
        ResultsPreferences {
            height: 220.0,
            collapsed: false
        }
    );
    app.invoke(Command::ToggleRightPanel, Default::default());
    assert!(app.results_preferences().collapsed);
    assert!(app.main_snapshot().error.is_none());
    app.invoke(Command::TemplateEditor, Default::default());
    assert!(app.windows.editor().is_some());
    assert_eq!(
        PanelPreferences::restore(Some(f32::NAN), false).preferred_height,
        220.0
    );
}
#[test]
fn native_composition_owns_shortcuts_across_frames_in_both_windows() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let draw = |app: &mut NativeApp, events: Vec<egui::Event>, editor: bool| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                if editor {
                    app.paint_editor(ui)
                } else {
                    app.paint_main(ui)
                }
            },
        );
        output.textures_delta.clear();
    };
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    draw(
        &mut app,
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "文".into(),
            active_range_chars: None,
        })],
        false,
    );
    draw(&mut app, vec![key(egui::Key::O, ctrl)], false);
    assert!(
        app.picker.is_none(),
        "a later composition key frame must not open a file picker"
    );
    draw(
        &mut app,
        vec![egui::Event::Ime(egui::ImeEvent::Commit("文".into()))],
        false,
    );
    draw(&mut app, vec![key(egui::Key::O, ctrl)], false);
    let picker = app.picker.take().expect("shortcut resumes after commit");
    app.runtime.picker_finished(picker.0, Ok(None));
    app.pump();
    app.invoke(Command::TemplateEditor, Default::default());
    draw(
        &mut app,
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "草稿".into(),
            active_range_chars: None,
        })],
        true,
    );
    draw(&mut app, vec![key(egui::Key::W, ctrl)], true);
    assert!(
        !app.windows.editor_closing(),
        "Ctrl+W remains owned by the composing editor input"
    );
    draw(
        &mut app,
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        })],
        true,
    );
    draw(&mut app, vec![key(egui::Key::W, ctrl)], true);
    assert!(app.windows.editor_closing());
}
#[test]
fn composing_escape_does_not_dismiss_a_native_error_dialog() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.gesture(hexforge_controller::action::Action::DropFiles(vec![]));
    let draw = |app: &mut NativeApp, events| {
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
    };
    draw(
        &mut app,
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "候補".into(),
            active_range_chars: None,
        })],
    );
    let escape = || egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    draw(&mut app, vec![escape()]);
    assert!(
        app.main_snapshot().error.is_some(),
        "the composing input owns Escape, not the native modal"
    );
    draw(
        &mut app,
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            escape(),
        ],
    );
    assert!(
        app.main_snapshot().error.is_none(),
        "ordinary Escape resumes after composition is dismissed"
    );
}
#[test]
fn unicode_windows_paths_survive_native_drop_and_picker() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("文件 Ελληνικά 😀.bin");
    std::fs::write(&path, [0, 1, 255]).unwrap();
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            dropped_files: vec![Arc::new(DroppedPath(path.clone()))],
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        app.main_snapshot()
            .file
            .expect("the native viewport must actually route a drop")
            .name,
        "文件 Ελληνικά 😀.bin"
    );
    app.invoke(Command::Open, Default::default());
    let picker = app.picker.take().unwrap();
    app.runtime.picker_finished(picker.0, Ok(Some(path)));
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        app.main_snapshot().file.unwrap().name,
        "文件 Ελληνικά 😀.bin"
    );
}

#[derive(Debug)]
struct DroppedPath(std::path::PathBuf);
impl egui::DroppedFile for DroppedPath {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        // File contents must be read by the existing bounded CoreService,
        // never by the dropped-file UI adapter.
        panic!("the presenter must not read dropped file bytes")
    }
}

#[test]
fn raw_native_drop_rejects_multiple_paths_and_consumes_a_single_path_only_once() {
    use hexforge_controller::action::Action;
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.bin");
    let second = directory.path().join("second.bin");
    std::fs::write(&first, [0x41, 0x42]).unwrap();
    std::fs::write(&second, [0x43, 0x44]).unwrap();
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let drop_frame = |app: &mut NativeApp, paths: Vec<std::path::PathBuf>| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                dropped_files: paths
                    .into_iter()
                    .map(|p| Arc::new(DroppedPath(p)) as Arc<dyn egui::DroppedFile + Send + Sync>)
                    .collect(),
                ..Default::default()
            },
            |ui| app.paint_main(ui),
        );
        output.textures_delta.clear();
    };
    drop_frame(&mut app, vec![first.clone(), second.clone()]);
    assert_eq!(
        app.main_snapshot().error.unwrap().message,
        "Drop exactly one file at a time."
    );
    assert!(app.main_snapshot().file.is_none());
    assert!(!app.main_snapshot().policy.operation_busy);
    assert!(app.picker.is_none());
    app.gesture(Action::ClearError);
    drop_frame(&mut app, vec![second.clone()]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.main_snapshot().policy.operation_busy {
        app.pump();
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let accepted = app.main_snapshot();
    assert_eq!(accepted.file.as_ref().unwrap().name, "second.bin");
    assert_eq!(accepted.file.as_ref().unwrap().size, "2");
    assert!(accepted.error.is_none());
    // Subsequent frames must not resubmit/reopen the already consumed OS event.
    for _ in 0..3 {
        drop_frame(&mut app, vec![]);
        app.pump();
        assert_eq!(app.main_snapshot().source, accepted.source);
        assert!(!app.main_snapshot().policy.operation_busy);
    }
    assert_eq!(std::fs::read(first).unwrap(), [0x41, 0x42]);
    assert_eq!(std::fs::read(second).unwrap(), [0x43, 0x44]);
}

#[test]
fn native_drop_cannot_replace_a_file_behind_an_error_modal() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.gesture(hexforge_controller::action::Action::DropFiles(vec![]));
    let before = app.main_snapshot().error;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            dropped_files: vec![Arc::new(DroppedPath(file.path().into()))],
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
    assert!(
        !app.main_snapshot().policy.operation_busy,
        "a raw OS drop must not bypass the foreground modal"
    );
    assert!(app.main_snapshot().file.is_none());
    assert_eq!(app.main_snapshot().error, before);
}
