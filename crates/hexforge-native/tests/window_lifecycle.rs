use hexforge_controller::{
    command::Command,
    controller::{InitialState, InvokeContext, WindowSnapshot},
    shortcuts::WindowKind,
    template::coordinator::EditorSessionId,
};
use hexforge_core::operations::CoreService;
use hexforge_native::{
    app::NativeApp,
    platform::native_failure_message,
    platform::windows::{WindowChange, WindowRegistry},
};
use std::sync::Arc;
#[test]
fn native_editor_is_singleton() {
    let mut w = WindowRegistry::default();
    assert_eq!(w.open_or_focus(EditorSessionId(1)), WindowChange::Open);
    assert_eq!(w.open_or_focus(EditorSessionId(1)), WindowChange::Focus);
    assert_eq!(w.editor(), Some(EditorSessionId(1)));
    assert!(!w.observe_editor_closed(EditorSessionId(9)));
    assert_eq!(w.editor(), Some(EditorSessionId(1)));
}

#[test]
fn ctrl_shift_t_reuses_the_native_editor_and_keeps_its_unsent_controls() {
    use eframe::egui;
    let ctx = egui::Context::default();
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.attach(&ctx);
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        shift: true,
        ..Default::default()
    };
    let press = |app: &mut NativeApp| {
        let mut frame = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events: vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::Key {
                        key: egui::Key::T,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                ],
                ..Default::default()
            },
            |ui| app.paint_main(ui),
        );
        frame.textures_delta.clear();
    };
    press(&mut app);
    let first = app.windows.editor().unwrap();
    app.editor.as_mut().unwrap().model.name = "Unsent controls".into();
    for _ in 0..3 {
        press(&mut app);
    }
    assert_eq!(app.windows.editor(), Some(first));
    assert_eq!(app.editor_snapshot().session, Some(first));
    assert_eq!(app.editor.as_ref().unwrap().model.name, "Unsent controls");
    assert!(app.picker.is_none() && app.main_snapshot().error.is_none());
    assert!(!app.main_snapshot().template_applied);
}

#[test]
fn native_editor_creation_failure_releases_the_window_and_can_retry_without_losing_the_draft() {
    use hexforge_controller::{action::Action, template::coordinator::EditorMessage};
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let draft = serde_json::from_str(
        r#"{"name":"Pending draft","defaultEndianness":"little","fields":[]}"#,
    )
    .unwrap();
    app.gesture(Action::TemplateChanged(draft));
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    let failed = app.windows.editor().unwrap();
    app.editor_message(EditorMessage::OpenFailed {
        session: failed,
        message: "Native window initialization failed".into(),
    });
    assert_eq!(
        app.windows.editor(),
        None,
        "Failed OS window must not remain the singleton's active window"
    );
    assert_eq!(app.editor_snapshot().session, None);
    assert!(app
        .main_snapshot()
        .error
        .unwrap()
        .message
        .contains("Native window initialization failed"));
    let retained = app.main_snapshot().template_state;
    assert_eq!(retained.definition.name, "Pending draft");
    assert!(retained.dirty && retained.saved_path.is_none());
    app.gesture(Action::ClearError);
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    let retry = app.windows.editor().unwrap();
    assert_ne!(retry, failed);
    assert_eq!(
        app.editor_snapshot().template_state.definition.name,
        "Pending draft"
    );
    app.editor_message(EditorMessage::OpenFailed {
        session: failed,
        message: "Obsolete failure".into(),
    });
    assert_eq!(
        app.windows.editor(),
        Some(retry),
        "An old failure must not destroy the retry window"
    );
    assert!(app.main_snapshot().error.is_none());
}

#[test]
fn native_closed_editor_reopens_with_a_new_session_and_ignores_old_close_notifications() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    let first = app.windows.editor().unwrap();
    app.editor_destroyed(first);
    assert_eq!(app.windows.editor(), None);
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    let next = app.windows.editor().unwrap();
    assert_ne!(next, first);
    app.editor_destroyed(first);
    assert_eq!(app.windows.editor(), Some(next));
    assert_eq!(app.editor_snapshot().session, Some(next));
}

#[test]
fn failed_immediate_renderer_reports_a_retryable_editor_error_instead_of_panicking() {
    use eframe::egui;
    use hexforge_controller::action::Action;
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    // Match eframe's real initialization-error boundary: it returns without
    // invoking the child UI. Keep egui's actual immediate-viewport code real.
    egui::Context::set_immediate_viewport_renderer(|_, _| {});
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.gesture(Action::TemplateChanged(
        serde_json::from_str(
            r#"{"name":"Kept after renderer failure","defaultEndianness":"little","fields":[]}"#,
        )
        .unwrap(),
    ));
    app.invoke(Command::TemplateEditor, Default::default());
    let first = app.windows.editor().unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.paint_editor_viewport(&ctx)
    }));
    assert!(
        outcome.is_ok(),
        "A failed native child must become an operation error, not crash the main app"
    );
    assert!(app.windows.editor().is_none());
    assert!(app.editor_snapshot().session.is_none());
    assert_eq!(
        app.main_snapshot().error.unwrap().message,
        "The Template Editor window could not be opened."
    );
    assert_eq!(
        app.main_snapshot().template_state.definition.name,
        "Kept after renderer failure"
    );
    assert!(app.main_snapshot().template_state.dirty);
    app.gesture(Action::ClearError);
    app.invoke(Command::TemplateEditor, Default::default());
    assert_ne!(app.windows.editor().unwrap(), first);
}

#[test]
fn painting_a_child_without_a_successful_swap_must_not_reveal_it() {
    use eframe::egui;
    use hexforge_controller::action::Action;
    fn renderer(swap_result: Option<Result<(), &'static str>>) {
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let mut input = egui::RawInput {
                viewport_id: viewport.ids.this,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(660.0, 760.0),
                )),
                ..Default::default()
            };
            input
                .viewports
                .insert(viewport.ids.this, Default::default());
            let mut frame = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
            frame.textures_delta.clear();
            if let Some(outcome) = swap_result {
                // Substitute only the surface swap at the same boundary used
                // by the shipping Glow renderer, not child UI or state logic.
                let _ = eframe::viewport_presentation::swap(ctx, viewport.ids.this, || outcome);
            }
        });
    }
    for swap_result in [None, Some(Err("surface swap failed"))] {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        // Native drawing can complete even though its surface never presents.
        // Invoke the actual child UI, but provide no successful swap result.
        renderer(swap_result);
        let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
        app.attach(&ctx);
        app.gesture(Action::TemplateChanged(serde_json::from_str(
        r#"{"name":"Draft survives failed presentation","defaultEndianness":"little","fields":[]}"#,
    ).unwrap()));
        app.invoke(Command::TemplateEditor, Default::default());
        let failed = app.windows.editor().unwrap();
        let id = hexforge_native::platform::windows::editor_viewport(failed);
        // A previous frame's success cannot authorize a missing/failed new swap.
        eframe::viewport_presentation::swap(&ctx, id, || Ok::<(), &str>(())).unwrap();
        let mut frame = ctx.run_ui(Default::default(), |ui| app.paint_editor_viewport(ui.ctx()));
        let commands = &frame.viewport_output[&id].commands;
        assert!(
            !commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Visible(true) | egui::ViewportCommand::Focus
            )),
            "An unpresented child must never be shown or focused"
        );
        frame.textures_delta.clear();
        assert!(app.windows.editor().is_none());
        assert_eq!(
            app.main_snapshot().template_state.definition.name,
            "Draft survives failed presentation"
        );
        assert!(app.main_snapshot().template_state.dirty);
        assert!(app.main_snapshot().error.is_some());
        app.gesture(Action::ClearError);
        app.invoke(Command::TemplateEditor, Default::default());
        let retry = app.windows.editor().unwrap();
        assert_ne!(retry, failed);
        renderer(Some(Ok(())));
        let mut frame = ctx.run_ui(Default::default(), |ui| app.paint_editor_viewport(ui.ctx()));
        assert!(
            frame.viewport_output[&hexforge_native::platform::windows::editor_viewport(retry)]
                .commands
                .contains(&egui::ViewportCommand::Visible(true))
        );
        frame.textures_delta.clear();
        assert_eq!(app.windows.editor(), Some(retry));
        assert!(app.main_snapshot().error.is_none());
        assert_eq!(
            app.main_snapshot().template_state.definition.name,
            "Draft survives failed presentation"
        );
    }
}

#[test]
fn focusing_an_existing_editor_requests_show_restore_and_focus_in_order() {
    use eframe::egui;
    let context = egui::Context::default();
    context.set_embed_viewports(false);
    let session = EditorSessionId(7);
    let id = hexforge_native::platform::windows::editor_viewport(session);
    let mut output = context.run_ui(Default::default(), |ui| {
        ui.ctx()
            .show_viewport_deferred(id, egui::ViewportBuilder::default(), |_, _| {});
        hexforge_native::platform::windows::focus_editor(ui.ctx(), session);
    });
    output.textures_delta.clear();
    let commands = &output
        .viewport_output
        .get(&id)
        .expect("The existing child owns its restore commands")
        .commands;
    assert!(matches!(
        commands.as_slice(),
        [
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Minimized(false),
            egui::ViewportCommand::Focus
        ]
    ));
    assert!(
        output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .unwrap()
            .commands
            .iter()
            .all(|command| !matches!(
                command,
                egui::ViewportCommand::Visible(_)
                    | egui::ViewportCommand::Minimized(_)
                    | egui::ViewportCommand::Focus
                    | egui::ViewportCommand::OuterPosition(_)
            )),
        "Restoring the editor must not minimize, hide or move the main window"
    );
}

#[test]
fn editor_template_pickers_focus_main_and_restore_the_same_child_after_completion() {
    use eframe::egui;
    use hexforge_controller::{
        dialog::PickerPurpose,
        identity::RequestId,
        template::coordinator::{EditorAction, EditorMessage},
    };
    for command in [Command::LoadTemplate, Command::SaveTemplateAs] {
        for outcome in ["cancel", "success", "picker-error"] {
            let ctx = egui::Context::default();
            ctx.set_embed_viewports(false);
            let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
            app.attach(&ctx);
            app.invoke(Command::TemplateEditor, Default::default());
            if command == Command::SaveTemplateAs {
                app.editor.as_mut().unwrap().model.name = "Editor draft".into();
                app.publish_editor().unwrap();
            }
            let session = app.windows.editor().unwrap();
            let child = hexforge_native::platform::windows::editor_viewport(session);
            let run = |app: &mut NativeApp, action: &mut dyn FnMut(&mut NativeApp)| {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    ui.ctx().show_viewport_deferred(
                        child,
                        egui::ViewportBuilder::default(),
                        |_, _| {},
                    );
                    action(app);
                });
                output.textures_delta.clear();
                output
            };
            let draft = app.editor.as_ref().unwrap().buffer.draft();
            let output = run(&mut app, &mut |app| {
                app.editor_message(EditorMessage::Action {
                    request_id: RequestId(1),
                    draft: draft.clone(),
                    action: EditorAction::Command(command),
                });
            });
            let (id, picker) = app
                .picker
                .take()
                .expect("enabled editor action opens one picker");
            assert_eq!(
                picker.owner,
                WindowKind::Main,
                "Native template dialogs belong to main; their error origin remains the editor"
            );
            assert_eq!(
                picker.purpose,
                if command == Command::LoadTemplate {
                    PickerPurpose::TemplateLoad
                } else {
                    PickerPurpose::TemplateSave
                }
            );
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .filter(|c| matches!(c, egui::ViewportCommand::Focus))
                    .count(),
                1,
                "Bring main forward before presenting its native picker"
            );
            assert!(output.viewport_output[&child].commands.is_empty());
            assert!(app.editor_snapshot().busy);

            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("header.json");
            if command == Command::LoadTemplate {
                std::fs::write(
                    &path,
                    br#"{"name":"Loaded","defaultEndianness":"little","fields":[]}"#,
                )
                .unwrap();
            }
            app.runtime.picker_finished(
                id,
                match outcome {
                    "cancel" => Ok(None),
                    "success" => Ok(Some(path.clone())),
                    _ => Err(hexforge_core::error::AppError::new(
                        hexforge_core::error::ErrorCode::IoError,
                        "Could not open the template picker.",
                        None,
                    )),
                },
            );
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            let mut returned = vec![];
            loop {
                let output = run(&mut app, &mut |app| app.pump());
                returned.extend(output.viewport_output[&child].commands.clone());
                if !app.editor_snapshot().busy {
                    break;
                }
                assert!(
                    returned.is_empty(),
                    "Do not restore the child before template persistence completes"
                );
                assert!(std::time::Instant::now() < until);
                std::thread::yield_now();
            }
            assert!(
                matches!(
                    returned.as_slice(),
                    [
                        egui::ViewportCommand::Visible(true),
                        egui::ViewportCommand::Minimized(false),
                        egui::ViewportCommand::Focus,
                    ]
                ),
                "Cancel, success and failure must restore the same editor once: {returned:?}"
            );
            assert_eq!(app.windows.editor(), Some(session));
            assert!(!app.windows.editor_closing());
            assert!(!app.main_snapshot().template_applied);
            if outcome == "success" {
                assert_eq!(app.editor_snapshot().template_state.saved_path, Some(path));
                assert!(!app.editor_snapshot().template_state.dirty);
                assert_eq!(
                    app.editor.as_ref().unwrap().model.name,
                    if command == Command::LoadTemplate {
                        "Loaded"
                    } else {
                        "Editor draft"
                    }
                );
            } else {
                assert!(app.editor_snapshot().template_state.saved_path.is_none());
                assert_eq!(
                    app.editor.as_ref().unwrap().model.name,
                    if command == Command::LoadTemplate {
                        "Untitled"
                    } else {
                        "Editor draft"
                    }
                );
                assert_eq!(
                    app.editor.as_ref().unwrap().error.as_deref(),
                    (outcome == "picker-error").then_some("Could not open the template picker.")
                );
            }
            let output = run(&mut app, &mut |app| app.pump());
            assert!(
                output.viewport_output[&child].commands.is_empty(),
                "Do not refocus on every idle frame"
            );
        }
    }
}

#[test]
fn editor_unload_confirmation_returns_focus_before_the_core_continuation() {
    use eframe::egui;
    use hexforge_controller::{
        identity::RequestId,
        template::coordinator::{EditorAction, EditorMessage},
    };
    for confirm in [false, true] {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
        app.attach(&ctx);
        app.invoke(Command::TemplateEditor, Default::default());
        app.editor.as_mut().unwrap().model.name = "Unsaved editor draft".into();
        app.publish_editor().unwrap();
        let session = app.windows.editor().unwrap();
        let child = hexforge_native::platform::windows::editor_viewport(session);
        let run = |app: &mut NativeApp, events, action: &mut dyn FnMut(&mut NativeApp)| {
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
                    ui.ctx().show_viewport_deferred(
                        child,
                        egui::ViewportBuilder::default(),
                        |_, _| {},
                    );
                    action(app);
                    app.paint_main(ui);
                },
            );
            output.textures_delta.clear();
            output
        };
        let draft = app.editor.as_ref().unwrap().buffer.draft();
        let output = run(&mut app, vec![], &mut |app| {
            app.editor_message(EditorMessage::Action {
                request_id: RequestId(1),
                draft: draft.clone(),
                action: EditorAction::Command(Command::UnloadTemplate),
            })
        });
        assert_eq!(
            app.main_snapshot().dialog.unwrap().1.owner,
            WindowKind::Main
        );
        assert_eq!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .filter(|c| matches!(c, egui::ViewportCommand::Focus))
                .count(),
            1
        );
        assert!(output.viewport_output[&child].commands.is_empty());
        assert_eq!(app.main_snapshot().template.name, "Unsaved editor draft");
        let mut button = None;
        for _ in 0..3 {
            let output = run(&mut app, vec![], &mut |_| {});
            button = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t)
                        if t.galley.job.text == if confirm { "OK" } else { "Cancel" } =>
                    {
                        Some(t.pos + t.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .or(button);
        }
        let point = button.expect("Main window owns the confirmation's real control");
        run(
            &mut app,
            vec![egui::Event::PointerMoved(point)],
            &mut |_| {},
        );
        let mut returned = vec![];
        for pressed in [true, false] {
            let output = run(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
                &mut |_| {},
            );
            returned.extend(output.viewport_output[&child].commands.clone());
        }
        assert!(matches!(returned.as_slice(), [egui::ViewportCommand::Visible(true), egui::ViewportCommand::Minimized(false), egui::ViewportCommand::Focus]), "Return the child when this standalone confirmation finishes, not after a second core operation: {returned:?}");
        assert!(app.main_snapshot().dialog.is_none());
        if confirm {
            assert!(
                app.main_snapshot().template_state.busy,
                "Core unload has not been acknowledged by the UI yet"
            );
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while app.main_snapshot().template_state.busy {
                let output = run(&mut app, vec![], &mut |app| app.pump());
                assert!(output.viewport_output[&child].commands.is_empty());
                assert!(std::time::Instant::now() < until);
                std::thread::yield_now();
            }
            assert!(!app.main_snapshot().template_state.active);
            assert_eq!(app.main_snapshot().template.name, "Untitled");
        } else {
            assert_eq!(app.main_snapshot().template.name, "Unsaved editor draft");
            assert!(app.main_snapshot().template_state.dirty);
        }
        assert_eq!(app.windows.editor(), Some(session));
    }
}
#[test]
fn main_close_waits_for_child() {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    let session = app.windows.editor().unwrap();
    app.invoke(Command::Exit, InvokeContext::default());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !app.windows.editor_closing() {
        app.pump();
        assert!(std::time::Instant::now() < until);
        std::thread::yield_now();
    }
    assert!(!app.windows.main_closing());
    app.editor_destroyed(session);
    assert!(app.windows.main_closing());
    assert!(
        matches!(app.snapshot(WindowKind::TemplateEditor),WindowSnapshot::Editor(s)if s.session.is_none())
    );
}
#[test]
fn renderer_failure_has_visible_native_error() {
    let message = native_failure_message("OpenGL initialization failed");
    assert!(message.contains("HexForge could not start"));
    assert!(message.contains("OpenGL initialization failed"));
    assert!(!message.contains("localhost"));
}
#[test]
fn native_presenter_has_no_raw_core_dispatch_path() {
    use hexforge_controller::action::Action;
    use hexforge_core::operations::CoreRequest;
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    assert!(!app.gesture(Action::Core(CoreRequest::UnloadTemplate)));
    assert!(app.gesture(Action::ClearSelection));
}
#[test]
fn controller_actions_request_root_repaint_when_child_is_active() {
    let context = eframe::egui::Context::default();
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.attach(&context);
    for _ in 0..3 {
        let mut output = context.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
    }
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = count.clone();
    context.set_request_repaint_callback(move |request| {
        if request.viewport_id == eframe::egui::ViewportId::ROOT {
            observed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    assert!(
        count.load(std::sync::atomic::Ordering::Relaxed) > 0,
        "a child-window edit can leave the authoritative main window asleep"
    );
}
#[test]
fn editor_paints_themed_background_across_entire_client() {
    let context = eframe::egui::Context::default();
    hexforge_native::style::fonts::install(&context);
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    app.invoke(Command::TemplateEditor, InvokeContext::default());
    let snapshot = app.editor_snapshot();
    let style = hexforge_native::style::HexForgeStyle::for_theme(snapshot.theme, 1.0);
    let mut bounds = eframe::egui::Rect::NOTHING;
    let mut output = context.run_ui(
        eframe::egui::RawInput {
            screen_rect: Some(eframe::egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(660.0, 760.0),
            )),
            ..Default::default()
        },
        |ui| {
            bounds = ui.max_rect();
            app.paint_editor(ui);
        },
    );
    let covered = output.shapes.iter().any(|shape| {
        matches!(&shape.shape,
        eframe::egui::epaint::Shape::Rect(r) if r.rect == bounds && r.fill == style.palette.bg)
    });
    output.textures_delta.clear();
    assert!(
        covered,
        "child viewport leaves its client background unpainted"
    );
}

#[test]
fn native_main_x_waits_for_the_pending_edit_before_checking_authoritative_dirty_state() {
    use eframe::egui;
    use hexforge_controller::{action::Action, dialog::DialogKind};
    use std::time::{Duration, Instant};
    // Holding the real core lock keeps the real mutation pending. No worker
    // reply is fabricated: releasing it runs the existing edit and dirty query.
    for (raw_byte, dirty) in [("FF", true), ("41", false)] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.bin");
        std::fs::write(&source, vec![0x41; 1024]).unwrap();
        let core = Arc::new(CoreService::default());
        let mut app = NativeApp::new(InitialState::default(), core.clone());
        let ctx = egui::Context::default();
        app.attach(&ctx);
        app.gesture(Action::DropFiles(vec![source.clone()]));
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.main_snapshot().policy.operation_busy {
            app.pump();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.gesture(Action::Select { start: 0, end: 0 });
        app.invoke(Command::ToggleEdit, Default::default());
        app.invoke(Command::EditSelected, Default::default());
        let held = core.session.lock().unwrap();
        app.gesture(Action::EditSelectedByte(raw_byte.into()));
        assert!(app.main_snapshot().edit_dialog.as_ref().unwrap().pending);
        let mut frame = eframe::Frame::_new_kittest();
        for _ in 0..2 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            let mut output = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::CancelClose)),
                "the real native X must stay protected until the pending mutation settles"
            );
            output.textures_delta.clear();
            assert!(!app.windows.main_closing());
            assert!(app.main_snapshot().dialog.is_none());
        }
        drop(held);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.pump();
            let state = app.main_snapshot();
            if dirty && state.dialog.is_some() || !dirty && app.windows.main_closing() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the real native close intent was lost while the edit was pending: {state:#?}"
            );
            std::thread::yield_now();
        }
        let state = app.main_snapshot();
        assert_eq!(state.file.as_ref().unwrap().dirty, dirty);
        assert!(state.edit_dialog.is_none());
        assert_eq!(
            std::fs::read(source).unwrap(),
            vec![0x41; 1024],
            "neither close outcome may overwrite the original binary"
        );
        if dirty {
            assert_eq!(state.dialog.unwrap().1.kind, DialogKind::BinaryUnsaved);
            assert!(!app.windows.main_closing());
        } else {
            assert!(
                state.dialog.is_none(),
                "a completed no-op edit needs no discard confirmation"
            );
            assert!(app.windows.main_closing());
        }
    }
}
