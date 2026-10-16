#[path = "support/capture_scene.rs"]
#[allow(dead_code)]
mod capture_scene;
#[path = "support/capture_spec.rs"]
#[allow(dead_code)]
mod capture_spec;
mod support;
use eframe::egui::{self, Event};
use hexforge_controller::{
    action::Action, command::Command, controller::InitialState, template::editing::FieldPath,
};
use hexforge_native::{app::NativeApp, views::field_card::control_id};
use std::sync::Arc;
fn editor() -> (NativeApp, egui::Context) {
    let mut app = NativeApp::new(
        InitialState::default(),
        Arc::new(hexforge_core::operations::CoreService::default()),
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.invoke(Command::TemplateEditor, Default::default());
    (app, ctx)
}
fn frame(
    app: &mut NativeApp,
    ctx: &egui::Context,
    events: Vec<Event>,
) -> Vec<(String, egui::Pos2)> {
    frame_at_height(app, ctx, events, 760.0)
}
fn frame_at_height(
    app: &mut NativeApp,
    ctx: &egui::Context,
    events: Vec<Event>,
    height: f32,
) -> Vec<(String, egui::Pos2)> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(660.0, height),
            )),
            events,
            ..Default::default()
        },
        |ui| app.paint_editor(ui),
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => {
                let center = t.pos + t.galley.size() / 2.0;
                s.clip_rect
                    .contains(center)
                    .then(|| (t.galley.job.text.clone(), center))
            }
            _ => None,
        })
        .collect()
}
fn click_at(app: &mut NativeApp, ctx: &egui::Context, p: egui::Pos2, height: f32) {
    frame_at_height(app, ctx, vec![Event::PointerMoved(p)], height);
    for pressed in [true, false] {
        frame_at_height(
            app,
            ctx,
            vec![Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
            height,
        );
    }
}

#[test]
fn editor_scrollbar_uses_source_thumb_and_arrow_steps_without_recentring() {
    let (mut app, ctx) = editor();
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![]);
    }
    let mut output = ctx.run_ui(Default::default(), |ui| app.paint_editor(ui));
    // Independently measured source list: 557px visible / 1626px content,
    // 12px end buttons, round(533*557/1626)=183px handle. The 2px inset
    // is across the scrollbar only, not at the thumb's axial ends.
    assert!(
        output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(r) if r.fill==egui::Color32::from_rgb(59,69,81)
            && r.rect==egui::Rect::from_min_size(egui::pos2(640.0,156.0),egui::vec2(6.0,183.0)))),
        "actual editor scrollbar must use the source thumb extent, inset and theme color"
    );
    output.textures_delta.clear();
    click_at(&mut app, &ctx, egui::pos2(643.0, 696.0), 760.0);
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let top = ctx
        .read_response(control_id(id, "name", None))
        .unwrap()
        .rect
        .top();
    assert!(
        (top - 120.0).abs() < 0.1,
        "down arrow steps exactly 40px; it must not recenter an egui thumb: {top}"
    );
    click_at(&mut app, &ctx, egui::pos2(643.0, 149.0), 760.0);
    frame(&mut app, &ctx, vec![]);
    assert!(
        (ctx.read_response(control_id(id, "name", None))
            .unwrap()
            .rect
            .top()
            - 160.0)
            .abs()
            < 0.1
    );
    assert_eq!(
        app.editor.as_ref().unwrap().model.definition().name,
        "Structured preview"
    );
}

#[test]
fn editor_scrollbar_drag_keeps_its_grab_offset_and_source_last_margin() {
    let (mut app, ctx) = editor();
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![]);
    }
    let start = egui::pos2(643.0, 160.0); // four pixels below the handle top
    frame(&mut app, &ctx, vec![Event::PointerMoved(start)]);
    frame(
        &mut app,
        &ctx,
        vec![Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    frame(
        &mut app,
        &ctx,
        vec![Event::PointerMoved(egui::pos2(643.0, 200.0))],
    );
    for _ in 0..2 {
        frame(&mut app, &ctx, vec![]);
    }
    let first = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let top = ctx
        .read_response(control_id(first, "name", None))
        .unwrap()
        .rect
        .top();
    // The real source drag from160 to200 scrolls122px. Its input border box
    // moves155→33; this control's native inner text response has a5px inset.
    // 40px handle motion * 1069px content travel / 350px handle travel,
    // rounded only when the bounded content is rasterized.
    assert!(
        (top - 38.0).abs() < 0.1,
        "noncentral grab must not recenter: {top}"
    );
    let end = egui::pos2(643.0, 850.0);
    frame(&mut app, &ctx, vec![Event::PointerMoved(end)]);
    frame(
        &mut app,
        &ctx,
        vec![Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    frame(&mut app, &ctx, vec![]);
    let last = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(2))
        .unwrap();
    let top = ctx
        .read_response(control_id(last, "advanced", None))
        .unwrap()
        .rect
        .top();
    assert!(
        (top - 668.0).abs() < 0.1,
        "at EOF source keeps exactly the final card's 8px bottom margin: {top}"
    );
    let start = egui::pos2(643.0, 514.0);
    frame(&mut app, &ctx, vec![Event::PointerMoved(start)]);
    frame(
        &mut app,
        &ctx,
        vec![Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    let end = egui::pos2(643.0, -20.0);
    frame(&mut app, &ctx, vec![Event::PointerMoved(end)]);
    frame(
        &mut app,
        &ctx,
        vec![Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    frame(&mut app, &ctx, vec![]);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 160.0)
            .abs()
            < 0.1
    );
}

#[test]
fn editor_scrollbar_pages_and_wheel_scroll_share_one_content_offset() {
    let (mut app, ctx) = editor();
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![]);
    }
    let first = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    click_at(&mut app, &ctx, egui::pos2(643.0, 400.0), 760.0);
    frame(&mut app, &ctx, vec![]);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            + 327.0)
            .abs()
            < 0.1,
        "track clicks page floor(557*.875)=487px without starting a thumb drag"
    );
    click_at(&mut app, &ctx, egui::pos2(643.0, 160.0), 760.0);
    frame(&mut app, &ctx, vec![]);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 160.0)
            .abs()
            < 0.1
    );
    frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(egui::pos2(643.0, 400.0)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -60.0),
                phase: egui::TouchPhase::Move,
                modifiers: Default::default(),
            },
        ],
    );
    for _ in 0..30 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 100.0)
            .abs()
            < 0.1,
        "wheel over the bar scrolls the existing area exactly once"
    );
    frame(
        &mut app,
        &ctx,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 60.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        }],
    );
    for _ in 0..30 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 160.0)
            .abs()
            < 0.1
    );
}

#[test]
fn editor_scrollbar_hold_repeat_stops_on_focus_loss_and_release() {
    let (mut app, ctx) = editor();
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![]);
    }
    let first = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let pos = egui::pos2(643.0, 696.0);
    frame(&mut app, &ctx, vec![Event::PointerMoved(pos)]);
    let timed = |app: &mut NativeApp, time, events, focused| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                events,
                focused,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(660.0, 760.0),
                )),
                ..Default::default()
            },
            |ui| app.paint_editor(ui),
        );
        output.textures_delta.clear();
    };
    timed(
        &mut app,
        10.0,
        vec![Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
        true,
    );
    timed(&mut app, 10.24, vec![], true);
    timed(&mut app, 10.241, vec![], true);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 120.0)
            .abs()
            < 0.1
    );
    timed(&mut app, 10.26, vec![], true);
    timed(&mut app, 10.27, vec![], true);
    timed(&mut app, 10.28, vec![], true);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 80.0)
            .abs()
            < 0.1
    );
    timed(&mut app, 10.40, vec![], false);
    timed(&mut app, 10.60, vec![], true);
    timed(&mut app, 10.61, vec![], true);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 80.0)
            .abs()
            < 0.1,
        "focus loss cancels held-repeat even if no OS button-up was delivered"
    );
    timed(
        &mut app,
        10.62,
        vec![Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
        true,
    );
    timed(&mut app, 11.0, vec![], true);
    assert!(
        (ctx.read_response(control_id(first, "name", None))
            .unwrap()
            .rect
            .top()
            - 80.0)
            .abs()
            < 0.1
    );
}
fn settle(app: &mut NativeApp) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        app.pump();
        if !app.editor_snapshot().busy && !app.main_snapshot().policy.operation_busy {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "native workflow did not finish"
        );
        std::thread::yield_now();
    }
}
#[test]
fn new_edit_during_save_remains_visible_and_dirty() {
    let (mut app, ctx) = editor();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("saved.json");
    frame(&mut app, &ctx, vec![]);
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Saved draft",
    );
    click(&mut app, &ctx, "Save As…");
    let id = app.picker.take().unwrap().0;
    app.runtime.picker_finished(id, Ok(Some(path.clone())));
    app.pump();
    assert!(app.editor_snapshot().busy);
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Newer draft",
    );
    settle(&mut app);
    assert_eq!(app.main_snapshot().template.name, "Newer draft");
    assert_eq!(app.editor.as_ref().unwrap().model.name, "Newer draft");
    assert_eq!(
        app.editor_snapshot().template_state.saved_path.as_deref(),
        Some(path.as_path())
    );
    assert!(app.editor_snapshot().template_state.dirty);
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(written["name"], "Saved draft");
    assert!(app.windows.editor().is_some());
}
#[test]
fn save_failure_or_cancel_keeps_editor_and_draft() {
    for failed in [false, true] {
        let (mut app, ctx) = editor();
        let directory = tempfile::tempdir().unwrap();
        frame(&mut app, &ctx, vec![]);
        type_value(&mut app, &ctx, egui::Id::new("template-name"), "Keep draft");
        let session = app.windows.editor().unwrap();
        app.editor_message(
            hexforge_controller::template::coordinator::EditorMessage::CloseRequested { session },
        );
        assert!(app.editor_snapshot().close_prompt);
        frame(&mut app, &ctx, vec![]); // Native modal sizing pass precedes presentation.
        click(&mut app, &ctx, "Save As");
        let id = app
            .picker
            .take()
            .expect("close Save As opens an owned picker")
            .0;
        app.runtime.picker_finished(
            id,
            Ok(if failed {
                Some(directory.path().join("absent-parent").join("template.json"))
            } else {
                None
            }),
        );
        settle(&mut app);
        assert_eq!(app.main_snapshot().template.name, "Keep draft");
        assert_eq!(app.editor.as_ref().unwrap().model.name, "Keep draft");
        assert!(!app.windows.editor_closing());
        assert!(app.editor_snapshot().template_state.saved_path.is_none());
        assert!(app.editor_snapshot().close_prompt);
        assert_eq!(app.editor.as_ref().unwrap().error.is_some(), failed);
    }
}

#[test]
fn pending_close_save_as_never_expires_and_closes_only_after_the_successful_reply() {
    use hexforge_controller::template::coordinator::EditorMessage;
    let (mut app, ctx) = editor();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("after-delayed-picker.json");
    frame(&mut app, &ctx, vec![]);
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Waiting draft",
    );
    let session = app.windows.editor().unwrap();
    app.editor_message(EditorMessage::CloseRequested { session });
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Save As");
    let (id, picker) = app.picker.take().expect("close Save As owns one picker");
    assert_eq!(
        picker.owner,
        hexforge_controller::shortcuts::WindowKind::Main
    );

    // This is a real elapsed-time assertion, not an immediate model snapshot:
    // continue the native frame/event pump beyond the source's former 5s IPC
    // timeout while the platform picker deliberately has not replied.
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(6) {
        app.pump();
        frame(&mut app, &ctx, vec![]);
        assert_eq!(app.windows.editor(), Some(session));
        assert!(!app.windows.editor_closing());
        assert_eq!(app.main_snapshot().template.name, "Waiting draft");
        assert_eq!(app.editor.as_ref().unwrap().model.name, "Waiting draft");
        assert!(app.main_snapshot().template_state.saved_path.is_none());
        assert!(app.editor.as_ref().unwrap().error.is_none());
        assert!(app.editor_snapshot().busy);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(!path.exists());
    app.runtime.picker_finished(id, Ok(Some(path.clone())));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !app.windows.editor_closing() {
        app.pump();
        assert!(app.editor.as_ref().unwrap().error.is_none());
        assert!(
            std::time::Instant::now() < until,
            "accepted picker must finish saving before close"
        );
        std::thread::yield_now();
    }
    assert!(!app.main_snapshot().template_state.dirty);
    assert_eq!(
        app.main_snapshot().template_state.saved_path.as_deref(),
        Some(path.as_path())
    );
    assert!(!app.main_snapshot().template_applied);
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(written["name"], "Waiting draft");
    assert!(written.get("version").is_none());
}

#[test]
fn close_save_picker_failure_is_shown_in_its_editor_without_losing_the_draft() {
    use hexforge_controller::template::coordinator::EditorMessage;
    let (mut app, ctx) = editor();
    frame(&mut app, &ctx, vec![]);
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "Keep draft");
    let session = app.windows.editor().unwrap();
    app.editor_message(EditorMessage::CloseRequested { session });
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Save As");
    let (id, picker) = app
        .picker
        .take()
        .expect("close save owns its native picker");
    assert_eq!(
        picker.owner,
        hexforge_controller::shortcuts::WindowKind::Main
    );
    app.runtime.picker_finished(
        id,
        Err(hexforge_core::error::AppError::new(
            hexforge_core::error::ErrorCode::IoError,
            "Could not open the save dialog.",
            None,
        )),
    );
    settle(&mut app);

    // The source editor catches failed actions locally. Reporting only in the
    // main window would hide the reason behind the still-open child window.
    assert_eq!(
        app.editor.as_ref().unwrap().error.as_deref(),
        Some("Could not open the save dialog.")
    );
    assert_eq!(app.editor.as_ref().unwrap().model.name, "Keep draft");
    assert_eq!(app.main_snapshot().template.name, "Keep draft");
    assert!(app.editor_snapshot().close_prompt);
    assert!(app.editor_snapshot().template_state.dirty);
    assert!(app.editor_snapshot().template_state.saved_path.is_none());
    assert!(!app.windows.editor_closing());
    assert!(!app.editor_snapshot().busy);
    assert_eq!(app.windows.editor(), Some(session));
}
#[test]
fn advanced_metadata_and_typed_expectations_edit_real_json() {
    let (mut app, ctx) = editor();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Metadata","defaultEndianness":"little","fields":[{"name":"mode","type":"u64","enumLabels":{"1":"Ready"},"bitFlags":[{"bit":0,"name":"first"}],"expect":{"kind":"oneOf","values":[{"type":"unsigned","value":"1"}]}}]})).unwrap()));
    let height = 2400.0;
    for prefix in ["Advanced", "Enum labels", "Bit flags"] {
        let p = frame_at_height(&mut app, &ctx, vec![], height)
            .into_iter()
            .find(|(t, _)| t.starts_with(prefix))
            .unwrap()
            .1;
        click_at(&mut app, &ctx, p, height);
    }
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    for (key, text) in [
        ("enum-label", "Running"),
        ("flag-bit", "63"),
        ("flag-name", "high"),
        ("expect-allowed", "9007199254740993"),
    ] {
        ctx.memory_mut(|m| m.request_focus(control_id(id, key, Some(0))));
        let mods = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame_at_height(
            &mut app,
            &ctx,
            vec![
                Event::ModifiersChanged(mods),
                Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: mods,
                },
            ],
            height,
        );
        frame_at_height(
            &mut app,
            &ctx,
            vec![
                Event::ModifiersChanged(egui::Modifiers::NONE),
                Event::Text(text.into()),
            ],
            height,
        );
    }
    let json = serde_json::to_value(app.main_snapshot().template).unwrap();
    assert_eq!(json["fields"][0]["enumLabels"]["1"], "Running");
    assert_eq!(
        json["fields"][0]["bitFlags"][0],
        serde_json::json!({"bit":63,"name":"high"})
    );
    assert_eq!(
        json["fields"][0]["expect"]["values"][0],
        serde_json::json!({"type":"unsigned","value":"9007199254740993"})
    );
    let p = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .find(|(t, _)| t == "+ Value")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, p, height);
    assert_eq!(
        serde_json::to_value(app.main_snapshot().template).unwrap()["fields"][0]["expect"]
            ["values"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn main_save_flushes_enum_text_before_capturing_the_draft() {
    let (mut app, ctx) = editor();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("labels.json");
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Labels","defaultEndianness":"little","fields":[{"name":"mode","type":"u16","enumLabels":{"1":"First","20":"Second"}}]})).unwrap()));
    for prefix in ["Advanced", "Enum labels"] {
        let p = frame_at_height(&mut app, &ctx, vec![], 2400.0)
            .into_iter()
            .find(|(t, _)| t.starts_with(prefix))
            .unwrap()
            .1;
        click_at(&mut app, &ctx, p, 2400.0);
    }
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    ctx.memory_mut(|m| m.request_focus(control_id(id, "enum-key", Some(0))));
    let mods = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame_at_height(
        &mut app,
        &ctx,
        vec![
            Event::ModifiersChanged(mods),
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: mods,
            },
        ],
        2400.0,
    );
    frame_at_height(
        &mut app,
        &ctx,
        vec![
            Event::ModifiersChanged(egui::Modifiers::NONE),
            Event::Text("99".into()),
        ],
        2400.0,
    );
    assert!(app.editor.as_ref().unwrap().buffer.draft().pending_inputs);
    app.invoke(Command::SaveTemplateAs, Default::default());
    let picker = app
        .picker
        .take()
        .expect("main save flushes the focused child first");
    app.runtime
        .picker_finished(picker.0, Ok(Some(path.clone())));
    settle(&mut app);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["fields"][0]["enumLabels"]["99"], "First");
    assert_eq!(saved["fields"][0]["enumLabels"]["20"], "Second");
    assert!(!app.editor.as_ref().unwrap().buffer.draft().pending_inputs);
}
#[test]
fn main_close_flushes_the_child_buffer_and_cancel_retains_both_workspaces() {
    let (mut app, ctx) = editor();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Labels","defaultEndianness":"little","fields":[{"name":"mode","type":"u16","enumLabels":{"1":"First"}}]})).unwrap()));
    for heading in ["Advanced · 1 labels", "Enum labels"] {
        let p = frame_at_height(&mut app, &ctx, vec![], 2400.0)
            .into_iter()
            .find(|(t, _)| t == heading)
            .expect("advanced section")
            .1;
        click_at(&mut app, &ctx, p, 2400.0);
    }
    let field = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let id = control_id(field, "enum-key", Some(0));
    ctx.memory_mut(|m| m.request_focus(id));
    let mods = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame_at_height(
        &mut app,
        &ctx,
        vec![
            Event::ModifiersChanged(mods),
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: mods,
            },
        ],
        2400.0,
    );
    frame_at_height(
        &mut app,
        &ctx,
        vec![
            Event::ModifiersChanged(egui::Modifiers::NONE),
            Event::Text("99".into()),
        ],
        2400.0,
    );
    assert!(app.editor.as_ref().unwrap().controls.pending_inputs());
    app.invoke(Command::Exit, Default::default());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.main_snapshot().dialog.is_none() {
        app.pump();
        assert!(
            std::time::Instant::now() < deadline,
            "main close confirmation never arrived"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        app.main_snapshot().template.fields[0]
            .enum_labels
            .as_ref()
            .unwrap()
            .get("99")
            .map(String::as_str),
        Some("First")
    );
    assert!(!app.editor.as_ref().unwrap().controls.pending_inputs());
    let draw_main = |app: &mut NativeApp, events| {
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
        output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(t) if t.galley.job.text == "Cancel" => {
                Some(t.pos + t.galley.size() / 2.0)
            }
            _ => None,
        })
    };
    draw_main(&mut app, vec![]);
    let p = draw_main(&mut app, vec![]).expect("real main exit Cancel button");
    draw_main(&mut app, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        draw_main(
            &mut app,
            vec![Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
    assert!(app.main_snapshot().dialog.is_none());
    assert!(!app.windows.main_closing());
    assert!(!app.windows.editor_closing());
    assert!(app.windows.editor().is_some());
    assert_eq!(
        app.editor.as_ref().unwrap().model.definition(),
        app.main_snapshot().template
    );
    assert!(app.editor_snapshot().template_state.dirty);
}
#[test]
fn validation_click_expands_advanced_and_focuses_exact_control() {
    let (mut app, ctx) = editor();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Bad alignment","defaultEndianness":"little","fields":[{"name":"header","type":"struct","fields":[{"name":"id","type":"u16","align":3}]}]})).unwrap()));
    let child_toggle = frame(&mut app, &ctx, vec![])
        .into_iter()
        .filter(|(text, _)| text == "▾")
        .nth(1)
        .expect("expanded nested field toggle")
        .1;
    click_at(&mut app, &ctx, child_toggle, 760.0);
    click(&mut app, &ctx, "▾");
    let p = frame(&mut app, &ctx, vec![])
        .into_iter()
        .find(|(t, _)| t.starts_with("header.id ·") && t.contains("Alignment"))
        .unwrap()
        .1;
    click_at(&mut app, &ctx, p, 760.0);
    frame(&mut app, &ctx, vec![]);
    let path = FieldPath::root(0).child(0);
    let id = app.editor.as_ref().unwrap().model.id(&path).unwrap();
    assert!(ctx.memory(|m| m.has_focus(control_id(id, "align", None))));
    type_value(&mut app, &ctx, control_id(id, "align", None), "4");
    assert!(app.editor.as_ref().unwrap().model.valid());
}

#[test]
fn validation_reveal_focuses_the_affected_second_metadata_row() {
    for (field, control) in [
        (
            serde_json::json!({"name":"kind","type":"u8","enumLabels":{"1":"One","2":""}}),
            "enum-label",
        ),
        (
            serde_json::json!({"name":"kind","type":"u8","enumLabels":{"1":"One","256":"Too large"}}),
            "enum-key",
        ),
        (
            serde_json::json!({"name":"kind","type":"u8","bitFlags":[{"bit":0,"name":"first"},{"bit":1,"name":""}]}),
            "flag-name",
        ),
        (
            serde_json::json!({"name":"kind","type":"u8","bitFlags":[{"bit":0,"name":"first"},{"bit":0,"name":"second"}]}),
            "flag-bit",
        ),
    ] {
        let (mut app, ctx) = editor();
        app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Metadata","defaultEndianness":"little","fields":[field]})).unwrap()));
        frame(&mut app, &ctx, vec![]);
        let id = app
            .editor
            .as_ref()
            .unwrap()
            .model
            .id(&FieldPath::root(0))
            .unwrap();
        click(&mut app, &ctx, "▾");
        let issue = frame(&mut app, &ctx, vec![])
            .into_iter()
            .find(|(text, _)| text.starts_with("kind · "))
            .unwrap()
            .1;
        click_at(&mut app, &ctx, issue, 760.0);
        for _ in 0..24 {
            frame(&mut app, &ctx, vec![]);
        }
        let target = control_id(id, control, Some(1));
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(target),
            "{control} must focus row 2, not row 1"
        );
        let bounds = ctx
            .read_response(target)
            .expect("revealed metadata input")
            .rect;
        assert!(
            bounds.top() >= 144.0 && bounds.bottom() <= 701.0,
            "{control} must be revealed inside the source field viewport: {bounds:?}"
        );
        assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    }
}

#[test]
fn validation_reveal_of_a_field_named_template_does_not_focus_the_template_name() {
    let (mut app, ctx) = editor();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"template","type":"u8","align":3}]})).unwrap()));
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    click(&mut app, &ctx, "▾");
    let issue = frame(&mut app, &ctx, vec![])
        .into_iter()
        .find(|(text, _)| text.starts_with("template · Alignment"))
        .unwrap()
        .1;
    click_at(&mut app, &ctx, issue, 760.0);
    frame(&mut app, &ctx, vec![]);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(control_id(id, "align", None))
    );
    assert!(!ctx.memory(|memory| memory.has_focus(egui::Id::new("template-name"))));
    type_value(&mut app, &ctx, control_id(id, "align", None), "4");
    assert!(app.editor.as_ref().unwrap().model.valid());
    assert_eq!(app.main_snapshot().template.name, "Header");
}
#[test]
fn validation_reveals_an_offscreen_select_in_the_actual_editor_scroll_area() {
    let (mut app, ctx) = editor();
    let mut fields = (0..8)
        .map(|i| serde_json::json!({"name":format!("field{i}"),"type":"u16"}))
        .collect::<Vec<_>>();
    fields.push(serde_json::json!({"name":"last","type":"u16","placement":{"mode":"sequential","offset":"0"}}));
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(
            serde_json::json!({"name":"Reveal","defaultEndianness":"little","fields":fields}),
        )
        .unwrap(),
    ));
    let p = frame(&mut app, &ctx, vec![])
        .into_iter()
        .find(|(text, _)| text.starts_with("last · Sequential placement"))
        .expect("validation summary points to last field")
        .1;
    click_at(&mut app, &ctx, p, 760.0);
    // Both interfaces reveal smoothly. Advance 400 ms of default frame time,
    // beyond the native animation's 300 ms upper bound, before checking bounds.
    for _ in 0..24 {
        frame(&mut app, &ctx, vec![]);
    }
    let field = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(8))
        .unwrap();
    let select = control_id(field, "placement-mode", None);
    assert!(
        ctx.memory(|m| m.has_focus(select)),
        "reveal retains the exact select focus"
    );
    let response = ctx.read_response(select).expect("select is rendered");
    assert!(response.rect.top() >= 200.0 && response.rect.bottom() <= 700.0, "validation must reveal the focused control inside the field list, not below the footer: {:?}", response.rect);
}
#[test]
fn configured_delete_and_reference_loss_require_two_native_confirmations() {
    let (mut app, ctx) = editor();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"References","defaultEndianness":"little","fields":[{"name":"count","type":"u16"},{"name":"data","type":"array","count":{"ref":"count"},"element":{"type":"u8"}}]})).unwrap()));
    click(&mut app, &ctx, "×");
    assert_eq!(app.main_snapshot().template.fields.len(), 2);
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Remove Field");
    assert_eq!(app.main_snapshot().template.fields.len(), 2);
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Continue");
    assert_eq!(app.main_snapshot().template.fields.len(), 1);
    assert!(!app.editor.as_ref().unwrap().model.valid());
    assert_eq!(
        app.main_snapshot().template.fields[0].name.as_deref(),
        Some("data")
    );
}
#[test]
fn collapsed_fields_keep_size_count_and_error_information() {
    for (field, expected) in [
        (
            serde_json::json!({"name":"header","type":"struct","fields":[{"name":"id","type":"u8"}]}),
            "header · struct · 1 field",
        ),
        (
            serde_json::json!({"name":"items","type":"array","count":{"fixed":3},"element":{"type":"u8"}}),
            "items · array · 3 elements",
        ),
        (
            serde_json::json!({"name":"payload","type":"bytes","length":4}),
            "payload · bytes · 4 B",
        ),
        (
            serde_json::json!({"name":"text","type":"string","encoding":"utf8","maxLength":16}),
            "text · string · 16 B",
        ),
        (
            serde_json::json!({"name":"payload","type":"bytes","length":{"ref":"missing","max":16}}),
            "payload · bytes · Length: missing",
        ),
    ] {
        let (mut app, ctx) = editor();
        app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Summary","defaultEndianness":"little","fields":[field.clone()]})).unwrap()));
        click(&mut app, &ctx, "▾");
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(
            texts.iter().any(|(t, _)| t == expected),
            "{expected}: {texts:?}"
        );
        if field["length"].is_object() {
            assert!(
                texts.iter().any(|(t, _)| t == "1 !"),
                "a collapsed invalid field must retain its error count"
            );
        }
    }
}
#[test]
fn enum_key_typing_keeps_its_original_label_until_commit() {
    let (mut app, ctx) = editor();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Labels","defaultEndianness":"little","fields":[{"name":"mode","type":"u16","enumLabels":{"1":"First","20":"Second"}}]})).unwrap()));
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    for heading in ["Advanced · 2 labels", "Enum labels"] {
        let p = frame_at_height(&mut app, &ctx, vec![], 2400.0)
            .into_iter()
            .find(|(t, _)| t == heading)
            .unwrap_or_else(|| panic!("missing {heading}"))
            .1;
        click_at(&mut app, &ctx, p, 2400.0);
    }
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    ctx.memory_mut(|m| m.request_focus(control_id(id, "enum-key", Some(0))));
    let mods = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame_at_height(
        &mut app,
        &ctx,
        vec![
            Event::ModifiersChanged(mods),
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: mods,
            },
        ],
        2400.0,
    );
    frame_at_height(
        &mut app,
        &ctx,
        vec![
            Event::ModifiersChanged(egui::Modifiers::NONE),
            Event::Text("9".into()),
        ],
        2400.0,
    );
    assert_eq!(
        app.main_snapshot().template.fields[0]
            .enum_labels
            .as_ref()
            .unwrap()
            .get("1")
            .map(String::as_str),
        Some("First")
    );
    frame_at_height(&mut app, &ctx, vec![Event::Text("9".into())], 2400.0);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("template-name")));
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    let labels = app.main_snapshot().template.fields[0]
        .enum_labels
        .clone()
        .unwrap();
    assert_eq!(labels.get("99").map(String::as_str), Some("First"));
    assert_eq!(labels.get("20").map(String::as_str), Some("Second"));
    assert!(!labels.contains_key("1"));
}
fn click(app: &mut NativeApp, ctx: &egui::Context, text: &str) {
    let p = frame(app, ctx, vec![])
        .into_iter()
        .find(|(t, _)| t == text)
        .unwrap_or_else(|| panic!("missing {text}"))
        .1;
    frame(app, ctx, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
}
fn type_value(app: &mut NativeApp, ctx: &egui::Context, id: egui::Id, value: &str) {
    type_value_at_height(app, ctx, id, value, 760.0);
}
fn type_value_at_height(
    app: &mut NativeApp,
    ctx: &egui::Context,
    id: egui::Id,
    value: &str,
    height: f32,
) {
    ctx.memory_mut(|m| m.request_focus(id));
    let control = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame_at_height(
        app,
        ctx,
        vec![
            Event::ModifiersChanged(control),
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: control,
            },
        ],
        height,
    );
    frame_at_height(
        app,
        ctx,
        vec![
            Event::ModifiersChanged(egui::Modifiers::NONE),
            if value.is_empty() {
                Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }
            } else {
                Event::Text(value.into())
            },
        ],
        height,
    );
}

fn select_at_height(
    app: &mut NativeApp,
    ctx: &egui::Context,
    id: egui::Id,
    label: &str,
    height: f32,
) {
    // Let a newly displayed validation summary finish layout before hit
    // testing; the presenter publishes validation after its input pass.
    for _ in 0..3 {
        frame_at_height(app, ctx, vec![], height);
    }
    let bounds = ctx.read_response(id).expect("actual select control").rect;
    click_at(app, ctx, bounds.center(), height);
    for _ in 0..6 {
        if let Some((_, p)) = frame_at_height(app, ctx, vec![], height)
            .into_iter()
            .filter(|(text, _)| text == label)
            .next_back()
        {
            click_at(app, ctx, p, height);
            return;
        }
        // The real native select menu is bounded. Scroll it instead of
        // assuming every option is already painted in the first tile.
        frame_at_height(
            app,
            ctx,
            vec![
                Event::PointerMoved(bounds.left_bottom() + egui::vec2(20.0, 55.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -120.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            height,
        );
        for _ in 0..12 {
            frame_at_height(app, ctx, vec![], height);
        }
    }
    panic!(
        "missing select option {label}; bounds={bounds:?}; text={:?}; error={:?}",
        frame_at_height(app, ctx, vec![], height)
            .into_iter()
            .map(|(text, _)| text)
            .collect::<Vec<_>>(),
        app.editor.as_ref().unwrap().error
    );
}

fn open_binary_fixture(app: &mut NativeApp) -> tempfile::NamedTempFile {
    let binary = tempfile::NamedTempFile::new().unwrap();
    binary.as_file().set_len(8).unwrap();
    app.gesture(Action::DropFiles(vec![binary.path().to_owned()]));
    settle(app);
    binary
}

fn assert_editor_command_enabled(
    app: &mut NativeApp,
    ctx: &egui::Context,
    label: &str,
    enabled: bool,
    height: f32,
) {
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(660.0, height),
            )),
            ..Default::default()
        },
        |ui| app.paint_editor(ui),
    );
    let node = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
        })
        .unwrap();
    assert_eq!(
        !node.1.is_disabled(),
        enabled,
        "actual {label} availability"
    );
    output.textures_delta.clear();
}

#[test]
fn native_rename_is_atomic_with_references_and_rejects_duplicate_names() {
    let (mut app, ctx) = editor();
    let _binary = open_binary_fixture(&mut app);
    let original = serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"size","type":"u8"},{"name":"payload","type":"bytes","length":{"ref":"size","max":8}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    type_value(&mut app, &ctx, control_id(id, "name", None), "length");
    let accepted = serde_json::to_value(&app.main_snapshot().template).unwrap();
    assert_eq!(accepted["fields"][0]["name"], "length");
    assert_eq!(
        accepted["fields"][1]["length"],
        serde_json::json!({"ref":"length","max":8})
    );
    type_value(&mut app, &ctx, control_id(id, "name", None), "payload");
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        accepted
    );
    let texts = frame(&mut app, &ctx, vec![]);
    assert!(texts
        .iter()
        .any(|(text, _)| text.contains("already exists")));
    assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_editor_command_enabled(&mut app, &ctx, "Apply Template", false, 760.0);
    assert_eq!(original["fields"][0]["name"], "size");
}

#[test]
fn destructive_native_type_changes_allow_cancel_and_invalidate_newer_edits() {
    let (mut app, ctx) = editor();
    let original = serde_json::json!({"name":"Records","defaultEndianness":"little","fields":[{"name":"record","type":"struct","fields":[{"name":"id","type":"u8"}]}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    select_at_height(&mut app, &ctx, control_id(id, "type", None), "u8", 760.0);
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_some());
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    click(&mut app, &ctx, "Cancel");
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    select_at_height(&mut app, &ctx, control_id(id, "type", None), "u8", 760.0);
    let mut newer = original.clone();
    newer["fields"][0]["fields"][0]["type"] = serde_json::json!("u64");
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(newer.clone()).unwrap(),
    ));
    let texts = frame(&mut app, &ctx, vec![]);
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_none());
    assert!(!texts.iter().any(|(text, _)| text == "Change Type"));
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        newer
    );
    select_at_height(&mut app, &ctx, control_id(id, "type", None), "u8", 760.0);
    click(&mut app, &ctx, "Change Type");
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap()["fields"],
        serde_json::json!([{"name":"record","type":"u8"}])
    );
    assert_eq!(original["fields"][0]["fields"][0]["type"], "u8");
}

#[test]
fn native_reference_reorder_allows_cancel_and_rejects_stale_confirmation() {
    let (mut app, ctx) = editor();
    let original = serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"size","type":"u8"},{"name":"payload","type":"bytes","length":{"ref":"size","max":8}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "↓");
    assert!(frame(&mut app, &ctx, vec![])
        .iter()
        .any(|(text, _)| text.contains("payload") && text.contains("earlier reference")));
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    click(&mut app, &ctx, "Cancel");
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    click(&mut app, &ctx, "↓");
    let mut newer = original.clone();
    newer["name"] = serde_json::json!("Newer edit");
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(newer.clone()).unwrap(),
    ));
    frame(&mut app, &ctx, vec![]);
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_none());
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        newer
    );
    click(&mut app, &ctx, "↓");
    click(&mut app, &ctx, "Continue");
    assert_eq!(
        app.main_snapshot()
            .template
            .fields
            .iter()
            .map(|field| field.name.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["payload", "size"]
    );
    assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_eq!(original["fields"][0]["name"], "size");
}

#[test]
fn nested_reference_source_removal_lists_its_dependent_before_cancel() {
    let (mut app, ctx) = editor();
    let original = serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"header","type":"struct","fields":[{"name":"size","type":"u8"}]},{"name":"payload","type":"bytes","length":{"ref":"header.size","max":8}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    let height = 2400.0;
    let nested_remove = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .filter(|(text, _)| text == "×")
        .nth(1)
        .unwrap()
        .1;
    click_at(&mut app, &ctx, nested_remove, height);
    let remove = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .find(|(text, _)| text == "Remove Field")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, remove, height);
    let texts = frame_at_height(&mut app, &ctx, vec![], height);
    assert!(texts
        .iter()
        .any(|(text, _)| text.contains("earlier reference") && text.contains("payload")));
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    let cancel = texts
        .into_iter()
        .find(|(text, _)| text == "Cancel")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, cancel, height);
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_none());
}

#[test]
fn native_field_advanced_summary_and_empty_delete_keep_the_source_rules() {
    let (mut app, ctx) = editor();
    click(&mut app, &ctx, "+ Field");
    click(&mut app, &ctx, "×");
    assert!(app.main_snapshot().template.fields.is_empty());
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_none());
    let original = serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"id","type":"u8","align":4,"comment":"Identifier","expect":{"kind":"equals","value":{"type":"unsigned","value":"7"}}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    let texts = frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    assert!(texts.iter().any(|(text, _)| text.starts_with("Advanced")
        && text.contains("Align 4")
        && text.contains("Expected: 7")));
    assert!(ctx
        .read_response(control_id(id, "placement-mode", None))
        .is_some());
    assert!(ctx.read_response(control_id(id, "align", None)).is_none());
    let advanced = texts
        .into_iter()
        .find(|(text, _)| text.starts_with("Advanced"))
        .unwrap()
        .1;
    click_at(&mut app, &ctx, advanced, 760.0);
    frame(&mut app, &ctx, vec![]);
    assert!(ctx.read_response(control_id(id, "align", None)).is_some());
    click(&mut app, &ctx, "×");
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_some());
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    click(&mut app, &ctx, "Cancel");
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    let configured = serde_json::json!({"name":"Records","defaultEndianness":"little","fields":[{"name":"record","type":"struct","fields":[{"name":"id","type":"u8"}]}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(configured.clone()).unwrap(),
    ));
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    click(&mut app, &ctx, "×");
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        configured
    );
    click(&mut app, &ctx, "Remove Field");
    assert!(app.main_snapshot().template.fields.is_empty());
    assert_eq!(
        configured["fields"][0]["fields"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn confirmed_type_change_clears_rejected_enum_configuration_and_raw_inputs() {
    let (mut app, ctx) = editor();
    let original = serde_json::json!({"name":"Labels","defaultEndianness":"little","fields":[{"name":"kind","type":"u8","enumLabels":{"1":"One","2":"Two"}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let height = 2400.0;
    for heading in ["Advanced · 2 labels", "Enum labels"] {
        let p = frame_at_height(&mut app, &ctx, vec![], height)
            .into_iter()
            .find(|(text, _)| text == heading)
            .unwrap()
            .1;
        click_at(&mut app, &ctx, p, height);
    }
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "enum-key", Some(0)),
        "2",
        height,
    );
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("template-name")));
    frame_at_height(&mut app, &ctx, vec![], height);
    assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    select_at_height(&mut app, &ctx, control_id(id, "type", None), "bool", height);
    let change = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .find(|(text, _)| text == "Change Type")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, change, height);
    let texts = frame_at_height(&mut app, &ctx, vec![], height);
    assert!(!texts
        .iter()
        .any(|(text, _)| text.contains("already exists")));
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap()["fields"],
        serde_json::json!([{"name":"kind","type":"bool"}])
    );
    assert!(app.editor.as_ref().unwrap().buffer.draft().valid);
    assert!(!app.editor.as_ref().unwrap().model.raw.pending());
    assert!(!app.editor.as_ref().unwrap().controls.pending_inputs());
    assert_eq!(
        original["fields"][0]["enumLabels"],
        serde_json::json!({"1":"One","2":"Two"})
    );
}

#[test]
fn native_field_collapse_identity_survives_reorder_and_parent_rename() {
    let (mut app, ctx) = editor();
    let original = serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"header","type":"struct","fields":[{"name":"id","type":"u8"}]},{"name":"tail","type":"u8"}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    let root = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let child = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0).child(0))
        .unwrap();
    let child_toggle = frame_at_height(&mut app, &ctx, vec![], 2400.0)
        .into_iter()
        .filter(|(text, _)| text == "▾")
        .nth(1)
        .unwrap()
        .1;
    click_at(&mut app, &ctx, child_toggle, 2400.0);
    let parent_toggle = frame_at_height(&mut app, &ctx, vec![], 2400.0)
        .into_iter()
        .find(|(text, _)| text == "▾")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, parent_toggle, 2400.0);
    let texts = frame_at_height(&mut app, &ctx, vec![], 2400.0);
    assert!(texts
        .iter()
        .any(|(text, _)| text == "header · struct · 1 field"));
    let down = texts.into_iter().find(|(text, _)| text == "↓").unwrap().1;
    click_at(&mut app, &ctx, down, 2400.0);
    assert_eq!(
        app.editor.as_ref().unwrap().model.id(&FieldPath::root(1)),
        Some(root)
    );
    assert_eq!(
        app.editor
            .as_ref()
            .unwrap()
            .model
            .id(&FieldPath::root(1).child(0)),
        Some(child)
    );
    let texts = frame_at_height(&mut app, &ctx, vec![], 2400.0);
    assert!(texts
        .iter()
        .any(|(text, _)| text == "header · struct · 1 field"));
    assert!(texts.iter().any(|(text, _)| text == "tail"));
    assert!(!texts.iter().any(|(text, _)| text == "tail · u8"));
    let expand = texts.into_iter().find(|(text, _)| text == "▸").unwrap().1;
    click_at(&mut app, &ctx, expand, 2400.0);
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(root, "name", None),
        "metadata",
        2400.0,
    );
    let texts = frame_at_height(&mut app, &ctx, vec![], 2400.0);
    assert!(
        texts.iter().any(|(text, _)| text == "id · u8"),
        "child must remain collapsed after its parent's rename"
    );
    assert!(app
        .editor
        .as_ref()
        .unwrap()
        .model
        .named_entries()
        .iter()
        .any(|(id, _, path, _)| *id == child && path == "metadata.id"));
    let json = serde_json::to_value(&app.main_snapshot().template).unwrap();
    assert_eq!(
        json["fields"][1],
        serde_json::json!({"name":"metadata","type":"struct","fields":[{"name":"id","type":"u8"}]})
    );
    assert_eq!(original["fields"][0]["name"], "header");
}

#[test]
fn native_duplicate_actions_deep_copy_the_top_and_nested_scopes() {
    let (mut app, ctx) = editor();
    let original = serde_json::json!({"name":"Records","defaultEndianness":"little","fields":[{"name":"record","type":"struct","fields":[{"name":"id","type":"u8"}]}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    let height = 2400.0;
    let duplicate = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .find(|(text, _)| text == "⧉")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, duplicate, height);
    let nested_duplicate = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .filter(|(text, _)| text == "⧉")
        .nth(1)
        .unwrap()
        .1;
    click_at(&mut app, &ctx, nested_duplicate, height);
    let json = serde_json::to_value(&app.main_snapshot().template).unwrap();
    assert_eq!(
        json["fields"],
        serde_json::json!([
            {"name":"record","type":"struct","fields":[{"name":"id","type":"u8"},{"name":"idCopy","type":"u8"}]},
            {"name":"recordCopy","type":"struct","fields":[{"name":"id","type":"u8"}]}
        ])
    );
    assert_eq!(original["fields"][0]["fields"].as_array().unwrap().len(), 1);
}

#[test]
fn native_controls_edit_bounded_length_and_exact_offset_references() {
    let (mut app, ctx) = editor();
    let _binary = open_binary_fixture(&mut app);
    let original = serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"size","type":"u16"},{"name":"offset","type":"u64"},{"name":"payload","type":"bytes","length":2,"placement":{"mode":"absolute","offset":"0"}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame_at_height(&mut app, &ctx, vec![], 3000.0);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(2))
        .unwrap();
    select_at_height(
        &mut app,
        &ctx,
        control_id(id, "length-source", None),
        "Earlier unsigned field",
        3000.0,
    );
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "length-ref", None),
        "size",
        3000.0,
    );
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "length-max", None),
        "256",
        3000.0,
    );
    select_at_height(
        &mut app,
        &ctx,
        control_id(id, "offset-source", None),
        "Earlier unsigned field",
        3000.0,
    );
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "offset-ref", None),
        "offset",
        3000.0,
    );
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "offset-add", None),
        "2",
        3000.0,
    );
    let json = serde_json::to_value(&app.main_snapshot().template).unwrap();
    assert_eq!(
        json["fields"][2],
        serde_json::json!({"name":"payload","type":"bytes","length":{"ref":"size","max":256},"placement":{"mode":"absolute","offset":{"ref":"offset","add":"2"}}})
    );
    assert!(app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_editor_command_enabled(&mut app, &ctx, "Apply Template", true, 3000.0);
    assert_eq!(original["fields"][2]["length"], 2);
}

#[test]
fn native_expected_range_reports_inversion_and_recovers_with_typed_values() {
    let (mut app, ctx) = editor();
    let _binary = open_binary_fixture(&mut app);
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"size","type":"u16"}]})).unwrap()));
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    click(&mut app, &ctx, "Advanced");
    click(&mut app, &ctx, "Expected value check");
    select_at_height(
        &mut app,
        &ctx,
        control_id(id, "expect-kind", None),
        "Inclusive range",
        2400.0,
    );
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "expect-min", None),
        "5",
        2400.0,
    );
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "expect-max", None),
        "1",
        2400.0,
    );
    assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_editor_command_enabled(&mut app, &ctx, "Apply Template", false, 2400.0);
    assert!(frame_at_height(&mut app, &ctx, vec![], 2400.0)
        .iter()
        .any(|(text, _)| text.contains("minimum")));
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "expect-max", None),
        "10",
        2400.0,
    );
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap()["fields"][0]["expect"],
        serde_json::json!({"kind":"range","min":{"type":"unsigned","value":"5"},"max":{"type":"unsigned","value":"10"}})
    );
    assert!(app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_editor_command_enabled(&mut app, &ctx, "Apply Template", true, 2400.0);
}

#[test]
fn native_array_reference_correction_and_condition_keep_immediate_validation() {
    let (mut app, ctx) = editor();
    let _binary = open_binary_fixture(&mut app);
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Records","defaultEndianness":"little","fields":[{"name":"count","type":"u8"},{"name":"records","type":"array","count":{"ref":"missing"},"element":{"type":"u8"}}]})).unwrap()));
    let height = 3000.0;
    let texts = frame_at_height(&mut app, &ctx, vec![], height);
    assert!(texts.iter().any(|(text, _)| text.contains("earlier field")));
    assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_editor_command_enabled(&mut app, &ctx, "Apply Template", false, height);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(1))
        .unwrap();
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "count-ref", None),
        "count",
        height,
    );
    assert!(app.editor.as_ref().unwrap().buffer.draft().valid);
    // Dismiss the focused reference suggestion popup and finish the layout
    // after the validation summary disappears before clicking Advanced.
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("template-name")));
    for _ in 0..3 {
        frame_at_height(&mut app, &ctx, vec![], height);
    }
    let advanced = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .filter(|(text, _)| text == "Advanced")
        .nth(1)
        .unwrap()
        .1;
    click_at(&mut app, &ctx, advanced, height);
    let condition_texts = frame_at_height(&mut app, &ctx, vec![], height);
    let condition = condition_texts
        .iter()
        .find(|(text, _)| text == "Condition")
        .unwrap_or_else(|| panic!("missing condition control: {condition_texts:?}"))
        .1;
    click_at(&mut app, &ctx, condition, height);
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "condition-ref", None),
        "count",
        height,
    );
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap()["fields"][1]["condition"],
        serde_json::json!({"ref":"count","op":"eq","value":{"type":"unsigned","value":"0"}})
    );
    assert!(app.editor.as_ref().unwrap().buffer.draft().valid);
    assert_editor_command_enabled(&mut app, &ctx, "Apply Template", true, height);
}

#[test]
fn native_editor_creates_nested_structures_and_arrays_from_empty_defaults() {
    let (mut app, ctx) = editor();
    let original = app.main_snapshot().template;
    assert!(original.fields.is_empty());
    click(&mut app, &ctx, "+ Field");
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    select_at_height(
        &mut app,
        &ctx,
        control_id(id, "type", None),
        "struct",
        2400.0,
    );
    let nested_add = frame_at_height(&mut app, &ctx, vec![], 2400.0)
        .into_iter()
        .find(|(text, _)| text == "+ Field")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, nested_add, 2400.0);
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap()["fields"][0],
        serde_json::json!({"name":"field1","type":"struct","fields":[{"name":"field1","type":"u8"}]})
    );
    let footer_add = frame_at_height(&mut app, &ctx, vec![], 2400.0)
        .into_iter()
        .filter(|(text, _)| text == "+ Field")
        .next_back()
        .unwrap()
        .1;
    click_at(&mut app, &ctx, footer_add, 2400.0);
    let array = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(1))
        .unwrap();
    select_at_height(
        &mut app,
        &ctx,
        control_id(array, "type", None),
        "array",
        2400.0,
    );
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap()["fields"][1],
        serde_json::json!({"name":"field2","type":"array","count":{"fixed":1},"element":{"type":"u8"}})
    );
    let up = frame_at_height(&mut app, &ctx, vec![], 2400.0)
        .into_iter()
        .filter(|(text, _)| text == "↑")
        .next_back()
        .unwrap()
        .1;
    click_at(&mut app, &ctx, up, 2400.0);
    assert_eq!(
        app.main_snapshot()
            .template
            .fields
            .iter()
            .map(|field| field.field_type)
            .collect::<Vec<_>>(),
        vec![
            hexforge_core::template::FieldType::Array,
            hexforge_core::template::FieldType::Struct
        ]
    );
    assert!(original.fields.is_empty());
}

#[test]
fn native_editor_preview_navigates_the_exact_range_above_two_to_the_53() {
    use hexforge_controller::template::editing::EditorModel;
    use hexforge_core::template::{Endian, FieldType, ParsedNode};
    use hexforge_native::{
        style::HexForgeStyle,
        views::{
            field_card::{EditorControlState, FieldEvent},
            template_editor::{show_template_editor, EditorUiEvent},
        },
    };
    let (app, _) = editor();
    let model = EditorModel::new(serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"id","type":"u64"}]})).unwrap());
    let mut snapshot = app.editor_snapshot();
    snapshot.results = vec![ParsedNode {
        kind: "leaf".into(),
        name: "id".into(),
        path: "id".into(),
        field_type: FieldType::U64,
        offset: Some("9007199254740993".into()),
        length: Some("8".into()),
        value: Some("9007199254740993".into()),
        endianness: Some(Endian::Little),
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    }];
    let h = support::ui_harness::NativeUiHarness::new();
    let style = HexForgeStyle::for_theme(snapshot.theme, 1.0);
    style.apply(&h.context);
    let mut state = EditorControlState::default();
    let mut emitted = vec![];
    let mut draw = |events| {
        h.frame(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                emitted.extend(show_template_editor(
                    ui, &style, &snapshot, &model, &mut state, None, None,
                ))
            },
        )
    };
    let frame = draw(vec![]);
    let button = frame
        .widgets
        .iter()
        .find(|widget| widget.label.starts_with("↗ "))
        .unwrap()
        .rect
        .center();
    draw(vec![Event::PointerMoved(button)]);
    for pressed in [true, false] {
        draw(vec![Event::PointerButton {
            pos: button,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    assert!(matches!(
        emitted.as_slice(),
        [EditorUiEvent::Field(FieldEvent::Navigate {
            start: 9_007_199_254_740_993,
            end: 9_007_199_254_741_000
        })]
    ));
}

#[test]
fn native_nested_reorder_keeps_the_focused_input_identity() {
    let (mut app, ctx) = editor();
    ctx.enable_accesskit();
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"header","type":"struct","fields":[{"name":"first","type":"u8"},{"name":"second","type":"u8"}]}]})).unwrap()));
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    let first = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0).child(0))
        .unwrap();
    let input = control_id(first, "name", None);
    ctx.memory_mut(|memory| memory.request_focus(input));
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(660.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| app.paint_editor(ui),
    );
    let move_down = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("↓") && !node.is_disabled())
        .unwrap()
        .0;
    output.textures_delta.clear();
    // The source dispatches a programmatic click without a pointer focus
    // transfer. Use the native accessibility Click action, not a model edit.
    frame_at_height(
        &mut app,
        &ctx,
        vec![Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_node: move_down,
                target_tree: egui::accesskit::TreeId::ROOT,
                data: None,
            },
        )],
        2400.0,
    );
    frame_at_height(&mut app, &ctx, vec![], 2400.0);
    assert_eq!(
        app.editor
            .as_ref()
            .unwrap()
            .model
            .id(&FieldPath::root(0).child(1)),
        Some(first)
    );
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(input));
    assert!(ctx.read_response(input).is_some());
    assert_eq!(
        app.main_snapshot().template.fields[0]
            .fields
            .as_ref()
            .unwrap()
            .iter()
            .map(|field| field.name.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["second", "first"]
    );
}

#[test]
fn duplicate_enum_change_gates_save_and_apply_without_overwriting_labels() {
    let (mut app, ctx) = editor();
    ctx.enable_accesskit();
    let directory = tempfile::tempdir().unwrap();
    let binary = tempfile::NamedTempFile::new().unwrap();
    binary.as_file().set_len(8).unwrap();
    app.gesture(Action::DropFiles(vec![binary.path().to_owned()]));
    settle(&mut app);
    let original = serde_json::json!({"name":"Labels","defaultEndianness":"little","fields":[{"name":"kind","type":"u8","enumLabels":{"1":"One","2":"Two"}}]});
    app.gesture(Action::TemplateChanged(
        serde_json::from_value(original.clone()).unwrap(),
    ));
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Save As…");
    let request = app.picker.take().unwrap().0;
    let saved = directory.path().join("labels.json");
    app.runtime
        .picker_finished(request, Ok(Some(saved.clone())));
    settle(&mut app);
    let before = std::fs::read(&saved).unwrap();
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let height = 2400.0;
    for heading in ["Advanced · 2 labels", "Enum labels"] {
        let p = frame_at_height(&mut app, &ctx, vec![], height)
            .into_iter()
            .find(|(t, _)| t == heading)
            .unwrap()
            .1;
        click_at(&mut app, &ctx, p, height);
    }
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "enum-key", Some(0)),
        "2",
        height,
    );
    // The source uses @change (the Vue setValue fixture emits that event),
    // not @input. Keep typing local until blur, then validate its commit.
    assert!(app.editor.as_ref().unwrap().controls.pending_inputs());
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("template-name")));
    frame_at_height(&mut app, &ctx, vec![], height);
    let texts = frame_at_height(&mut app, &ctx, vec![], height);
    assert!(
        texts
            .iter()
            .any(|(text, _)| text.contains("already exists")),
        "a duplicate committed value must be reported"
    );
    assert_eq!(
        serde_json::to_value(&app.main_snapshot().template).unwrap(),
        original
    );
    assert!(!app.editor.as_ref().unwrap().buffer.draft().valid);
    let assert_buttons = |app: &mut NativeApp, expected: bool| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(660.0, height),
                )),
                ..Default::default()
            },
            |ui| app.paint_editor(ui),
        );
        for label in ["Save", "Save As…", "Apply Template"] {
            let node = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
                })
                .unwrap();
            assert_eq!(
                !node.1.is_disabled(),
                expected,
                "{label} must follow committed raw-input validity"
            );
        }
        output.textures_delta.clear();
    };
    assert_buttons(&mut app, false);
    app.invoke(Command::SaveTemplate, Default::default());
    settle(&mut app);
    assert_eq!(std::fs::read(&saved).unwrap(), before);
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "enum-key", Some(0)),
        "3",
        height,
    );
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("template-name")));
    frame_at_height(&mut app, &ctx, vec![], height);
    let texts = frame_at_height(&mut app, &ctx, vec![], height);
    assert!(!texts
        .iter()
        .any(|(text, _)| text.contains("already exists")));
    assert_buttons(&mut app, true);
    app.invoke(Command::SaveTemplate, Default::default());
    settle(&mut app);
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&saved).unwrap()).unwrap();
    assert_eq!(
        written["fields"][0]["enumLabels"],
        serde_json::json!({"2":"Two","3":"One"})
    );
    assert_eq!(
        original["fields"][0]["enumLabels"],
        serde_json::json!({"1":"One","2":"Two"})
    );
    // The saved binding and binary stay available while an explicitly
    // confirmed type change removes an invalid metadata control.
    type_value_at_height(
        &mut app,
        &ctx,
        control_id(id, "enum-key", Some(0)),
        "3",
        height,
    );
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("template-name")));
    for _ in 0..3 {
        frame_at_height(&mut app, &ctx, vec![], height);
    }
    assert_buttons(&mut app, false);
    select_at_height(&mut app, &ctx, control_id(id, "type", None), "bool", height);
    let change = frame_at_height(&mut app, &ctx, vec![], height)
        .into_iter()
        .find(|(text, _)| text == "Change Type")
        .unwrap()
        .1;
    click_at(&mut app, &ctx, change, height);
    frame_at_height(&mut app, &ctx, vec![], height);
    assert_buttons(&mut app, true);
    assert_eq!(
        app.editor_snapshot().template_state.saved_path.as_deref(),
        Some(saved.as_path())
    );
    let still_saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&saved).unwrap()).unwrap();
    assert_eq!(
        still_saved, written,
        "changing a field never automatically saves it"
    );
}
#[test]
fn native_editor_metadata_and_add_duplicate_reorder_are_real_controls() {
    let (mut app, ctx) = editor();
    let texts = frame(&mut app, &ctx, vec![]);
    for label in [
        "Name",
        "Default endian",
        "Fields parse in order. References use earlier result paths.",
        "+ Field",
        "Load…",
        "Unload",
        "Save",
        "Save As…",
        "Apply Template",
    ] {
        assert!(texts.iter().any(|(t, _)| t == label), "missing {label}");
    }
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "Header");
    assert_eq!(app.main_snapshot().template.name, "Header");
    click(&mut app, &ctx, "+ Field");
    assert_eq!(app.main_snapshot().template.fields.len(), 1);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    type_value(&mut app, &ctx, control_id(id, "name", None), "magic");
    assert_eq!(
        app.main_snapshot().template.fields[0].name.as_deref(),
        Some("magic")
    );
    click(&mut app, &ctx, "⧉");
    assert_eq!(
        app.main_snapshot().template.fields[1].name.as_deref(),
        Some("magicCopy")
    );
    click(&mut app, &ctx, "↓");
    let names = app
        .main_snapshot()
        .template
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(names, vec![Some("magicCopy".into()), Some("magic".into())]);
    assert_eq!(
        app.editor.as_ref().unwrap().model.id(&FieldPath::root(1)),
        Some(id),
        "reorder retains the same editing identity"
    );
}
#[test]
fn footer_button_availability_matches_main_policy() {
    let (mut app, ctx) = editor();
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Save");
    assert!(app.picker.is_none());
    click(&mut app, &ctx, "Save As…");
    assert!(
        app.picker.is_none(),
        "opening the editor alone does not activate a template"
    );
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "Header");
    click(&mut app, &ctx, "Save As…");
    assert_eq!(
        app.picker.as_ref().unwrap().1.purpose,
        hexforge_controller::dialog::PickerPurpose::TemplateSave
    );
    assert_eq!(
        app.picker.as_ref().unwrap().1.owner,
        hexforge_controller::shortcuts::WindowKind::Main
    );
    let id = app.picker.take().unwrap().0;
    app.runtime.picker_finished(id, Ok(None));
    app.pump();
    assert!(
        app.editor.as_ref().unwrap().error.is_none(),
        "cancelling a picker is not an error"
    );
    assert!(app.windows.editor().is_some());
    assert!(app.editor_snapshot().template_state.saved_path.is_none());
}
#[test]
fn native_select_owns_its_stable_focus_and_keyboard_value() {
    let (mut app, ctx) = editor();
    frame(&mut app, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("template-endian")));
    frame(
        &mut app,
        &ctx,
        vec![Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(
        app.main_snapshot().template.default_endianness,
        hexforge_core::template::Endian::Big
    );
    assert!(ctx.memory(|m| m.has_focus(egui::Id::new("template-endian"))));
}
#[test]
fn every_schema_feature_has_a_live_native_control() {
    use hexforge_controller::template::editing::FieldStep;
    let (mut app, ctx) = editor();
    let definition=serde_json::from_value(serde_json::json!({"name":"Schema","defaultEndianness":"little","fields":[
        {"name":"count","type":"u16","align":4,"enumLabels":{"1":"Ready"},"bitFlags":[{"bit":0,"name":"enabled"}],"expect":{"kind":"range","min":{"type":"unsigned","value":"0"},"max":{"type":"unsigned","value":"8"}}},
        {"name":"name","type":"string","encoding":"ascii","length":4,"expect":{"kind":"equals","value":{"type":"string","value":"TEXT"}}},
        {"name":"payload","type":"bytes","length":{"ref":"count","max":16},"placement":{"mode":"absolute","offset":{"ref":"count","add":"0"}}},
        {"name":"records","type":"array","count":{"ref":"count"},"element":{"type":"struct","fields":[{"name":"id","type":"u64"},{"name":"value","type":"f64"}]}},
        {"name":"conditional","type":"string","encoding":"utf16be","length":4,"condition":{"ref":"count","op":"gt","value":{"type":"unsigned","value":"0"}}},
        {"name":"text","type":"string","encoding":"utf8","maxLength":16}
    ]})).unwrap();
    app.gesture(Action::TemplateChanged(definition));
    let draw_large = |app: &mut NativeApp, events: Vec<Event>| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(660.0, 8000.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.paint_editor(ui),
        );
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) => {
                    Some((t.galley.job.text.clone(), t.pos + t.galley.size() / 2.0))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // Open each actual Advanced control once. Its source label does not encode
    // expansion in a font glyph, so it cannot be used as a loop condition.
    let fields = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .named_entries()
        .into_iter()
        .map(|(id, _, _, _)| id)
        .collect::<Vec<_>>();
    for id in fields {
        draw_large(&mut app, vec![]);
        let Some(response) = ctx.read_response(control_id(id, "advanced", None)) else {
            continue;
        };
        let p = response.rect.left_center() + egui::vec2(44.0, 0.0);
        draw_large(&mut app, vec![Event::PointerMoved(p)]);
        for pressed in [true, false] {
            draw_large(
                &mut app,
                vec![Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
    }
    let edits = [
        (0, "align", "8"),
        (1, "text-length", "6"),
        (2, "length-ref", "count"),
        (2, "length-max", "32"),
        (2, "offset-add", "9007199254740993"),
        (3, "count-ref", "count"),
        (4, "condition-value", "1"),
        (5, "text-length", "24"),
    ];
    for (root, control, text) in edits {
        let id = app
            .editor
            .as_ref()
            .unwrap()
            .model
            .id(&FieldPath::root(root))
            .unwrap();
        ctx.memory_mut(|m| m.request_focus(control_id(id, control, None)));
        let control_mods = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        draw_large(
            &mut app,
            vec![
                Event::ModifiersChanged(control_mods),
                Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: control_mods,
                },
            ],
        );
        assert!(
            ctx.memory(|m| m.has_focus(control_id(id, control, None))),
            "missing editable {control}"
        );
        draw_large(
            &mut app,
            vec![
                Event::ModifiersChanged(egui::Modifiers::NONE),
                Event::Text(text.into()),
            ],
        );
    }
    let json = serde_json::to_value(app.main_snapshot().template).unwrap();
    assert_eq!(json["fields"][0]["align"], 8);
    assert_eq!(json["fields"][1]["length"], 6);
    assert_eq!(json["fields"][2]["length"]["max"], 32);
    assert_eq!(
        json["fields"][2]["placement"]["offset"]["add"],
        "9007199254740993"
    );
    assert_eq!(json["fields"][4]["condition"]["value"]["value"], "1");
    assert_eq!(json["fields"][5]["maxLength"], 24);
    let child = FieldPath(vec![
        FieldStep::Child(3),
        FieldStep::Element,
        FieldStep::Child(0),
    ]);
    assert!(app.editor.as_ref().unwrap().model.id(&child).is_some());
    let texts = draw_large(&mut app, vec![]);
    for text in [
        "Structure fields",
        "Array element definition",
        "Expected value check",
        "Enum labels",
        "Bit flags",
        "Count source",
        "Offset source",
        "Encoding",
    ] {
        assert!(
            texts.iter().any(|(t, _)| t.contains(text)),
            "missing {text}"
        );
    }
}
#[test]
fn rejected_raw_input_counts_as_unsaved_before_native_close() {
    let (mut app, ctx) = editor();
    let definition=serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"data","type":"bytes","length":1}]})).unwrap();
    app.gesture(Action::TemplateChanged(definition));
    // An authoritative draft replacement is accepted before this local input.
    frame(&mut app, &ctx, vec![]);
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    type_value(&mut app, &ctx, control_id(id, "length", None), "-");
    assert!(app.editor.as_ref().unwrap().model.raw.pending());
    assert!(app.editor.as_ref().unwrap().buffer.draft().pending_inputs);
    let session = app.windows.editor().unwrap();
    app.editor_message(
        hexforge_controller::template::coordinator::EditorMessage::CloseRequested { session },
    );
    assert!(app.editor_snapshot().close_prompt);
    assert!(!app.windows.editor_closing());
}

struct SavedEditorFixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
}
impl SavedEditorFixture {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}
fn loaded_editor() -> (NativeApp, egui::Context, SavedEditorFixture) {
    let (mut app, ctx) = editor();
    let directory = tempfile::tempdir().unwrap();
    let file = SavedEditorFixture {
        path: directory.path().join("header.json"),
        _directory: directory,
    };
    // Leave no fixture-owned file handle open: Windows correctly refuses an
    // atomic replacement of a target held open without delete sharing.
    std::fs::write(file.path(), br#"{"name":"Header","defaultEndianness":"little","fields":[{"name":"first","type":"u8"},{"name":"second","type":"u8"}]}"#).unwrap();
    app.invoke(Command::LoadTemplate, Default::default());
    let id = take_editor_picker(&mut app);
    app.runtime
        .picker_finished(id, Ok(Some(file.path().into())));
    settle(&mut app);
    frame(&mut app, &ctx, vec![]);
    (app, ctx, file)
}

fn take_editor_picker(app: &mut NativeApp) -> hexforge_controller::identity::RequestId {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        app.pump();
        if let Some((id, _)) = app.picker.take() {
            return id;
        }
        assert!(
            std::time::Instant::now() < until,
            "Expected the owned native picker"
        );
        std::thread::yield_now();
    }
}

fn editor_key(
    app: &mut NativeApp,
    ctx: &egui::Context,
    key: egui::Key,
    modifiers: egui::Modifiers,
    repeat: bool,
) {
    frame(
        app,
        ctx,
        vec![
            Event::ModifiersChanged(modifiers),
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat,
                modifiers,
            },
        ],
    );
    frame(
        app,
        ctx,
        vec![Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }],
    );
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn close_choice(app: &mut NativeApp, ctx: &egui::Context, label: &str) {
    if app
        .main_snapshot()
        .dialog
        .as_ref()
        .is_some_and(|(_, d)| d.owner == hexforge_controller::shortcuts::WindowKind::Main)
    {
        // Template pickers and their confirmations belong to main in the
        // reference. Exercise that actual presenter, not a duplicate child UI.
        let main_ctx = egui::Context::default();
        hexforge_native::style::fonts::install(&main_ctx);
        let render = |app: &mut NativeApp, events| {
            let mut output = main_ctx.run_ui(
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
        };
        let mut point = None;
        for _ in 0..3 {
            let output = render(app, vec![]);
            point = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos + t.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .or(point);
        }
        let point =
            point.unwrap_or_else(|| panic!("Missing main-owned confirmation choice {label}"));
        render(app, vec![Event::PointerMoved(point)]);
        for pressed in [true, false] {
            render(
                app,
                vec![Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
        return;
    }
    frame(app, ctx, vec![]);
    let pos = frame(app, ctx, vec![])
        .into_iter()
        .find(|(text, pos)| text == label && pos.y < 650.0)
        .unwrap_or_else(|| panic!("Missing enabled modal choice {label}"))
        .1;
    click_at(app, ctx, pos, 760.0);
}

#[test]
fn native_editor_close_uses_only_the_exact_chord_and_single_native_lifecycle() {
    for extra in [
        egui::Modifiers::NONE,
        egui::Modifiers::ALT,
        egui::Modifiers::SHIFT,
        egui::Modifiers {
            mac_cmd: true,
            ..Default::default()
        },
    ] {
        let (mut app, ctx, _file) = loaded_editor();
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(!texts
            .iter()
            .any(|(text, _)| matches!(text.as_str(), "New v2" | "v1" | "v2" | "✕")));
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(660.0, 760.0),
                )),
                ..Default::default()
            },
            |ui| app.paint_editor(ui),
        );
        let header_controls = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, node)| {
                matches!(
                    node.role(),
                    egui::accesskit::Role::Button | egui::accesskit::Role::ComboBox
                ) && node.bounds().is_some_and(|r| r.y0 < 75.0)
            })
            .count();
        output.textures_delta.clear();
        assert_eq!(
            header_controls, 0,
            "Neither an in-page close nor a format switcher belongs in the editor header"
        );
        let session = app.windows.editor().unwrap();
        type_value(&mut app, &ctx, egui::Id::new("template-name"), "Header");
        let modifiers = egui::Modifiers {
            alt: extra.alt,
            shift: extra.shift,
            mac_cmd: extra.mac_cmd,
            ..ctrl()
        };
        editor_key(&mut app, &ctx, egui::Key::W, modifiers, false);
        assert_eq!(app.windows.editor_closing(), extra == egui::Modifiers::NONE);
        assert!(!app.editor_snapshot().close_prompt);
        assert_eq!(app.windows.editor(), Some(session));
        if extra == egui::Modifiers::NONE {
            editor_key(&mut app, &ctx, egui::Key::W, ctrl(), true);
            assert_eq!(app.windows.editor(), Some(session));
            app.editor_destroyed(session);
            assert!(app.windows.editor().is_none());
            assert!(app.editor_snapshot().session.is_none());
        }
    }
}

#[test]
fn native_editor_close_cancel_then_discard_restores_the_loaded_checkpoint() {
    let (mut app, ctx, file) = loaded_editor();
    let original = std::fs::read(file.path()).unwrap();
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "Edited");
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    assert!(app.editor_snapshot().close_prompt && !app.windows.editor_closing());
    close_choice(&mut app, &ctx, "Cancel");
    assert!(!app.editor_snapshot().close_prompt);
    assert_eq!(app.editor.as_ref().unwrap().model.name, "Edited");
    assert!(app.main_snapshot().template_state.dirty);
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    close_choice(&mut app, &ctx, "Don't Save");
    assert!(app.windows.editor_closing());
    assert_eq!(app.main_snapshot().template.name, "Header");
    assert!(!app.main_snapshot().template_state.dirty);
    assert_eq!(
        app.main_snapshot().template_state.saved_path.as_deref(),
        Some(file.path())
    );
    assert_eq!(std::fs::read(file.path()).unwrap(), original);
    let session = app.windows.editor().unwrap();
    app.editor_destroyed(session);
    assert!(app.windows.editor().is_none());
}

#[test]
fn native_editor_close_save_and_save_as_wait_for_successful_disk_persistence() {
    for save_as in [false, true] {
        let (mut app, ctx, file) = if save_as {
            let (app, ctx) = editor();
            (app, ctx, None)
        } else {
            let (app, ctx, file) = loaded_editor();
            (app, ctx, Some(file))
        };
        let directory = tempfile::tempdir().unwrap();
        let path = if save_as {
            directory.path().join("saved.json")
        } else {
            file.as_ref().unwrap().path().into()
        };
        type_value(
            &mut app,
            &ctx,
            egui::Id::new("template-name"),
            "Saved close draft",
        );
        editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
        assert!(!app.windows.editor_closing());
        if save_as {
            assert_editor_command_enabled(&mut app, &ctx, "Save", false, 760.0);
        }
        close_choice(&mut app, &ctx, if save_as { "Save As" } else { "Save" });
        if save_as {
            let id = take_editor_picker(&mut app);
            assert!(!app.windows.editor_closing());
            app.runtime.picker_finished(id, Ok(Some(path.clone())));
        }
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !app.windows.editor_closing() {
            app.pump();
            assert!(
                app.editor.as_ref().unwrap().error.is_none(),
                "Close save failed: {:?}",
                app.editor.as_ref().unwrap().error
            );
            assert!(
                std::time::Instant::now() < until,
                "Save must finish before native destruction"
            );
            std::thread::yield_now();
        }
        assert!(!app.main_snapshot().template_state.dirty);
        assert_eq!(
            app.main_snapshot().template_state.saved_path.as_deref(),
            Some(path.as_path())
        );
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["name"], "Saved close draft");
        assert!(!app.main_snapshot().template_applied);
    }
}

#[test]
fn native_editor_save_shortcuts_keep_template_persistence_inside_text_inputs() {
    for shift in [false, true] {
        let (mut app, ctx, file) = loaded_editor();
        let directory = tempfile::tempdir().unwrap();
        let path = if shift {
            directory.path().join("copy.json")
        } else {
            file.path().into()
        };
        type_value(
            &mut app,
            &ctx,
            egui::Id::new("template-name"),
            "Keyboard saved",
        );
        assert_eq!(
            app.main_snapshot().template.name,
            "Keyboard saved",
            "Typing must publish before Save"
        );
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("template-name"))
        );
        editor_key(
            &mut app,
            &ctx,
            egui::Key::S,
            egui::Modifiers { shift, ..ctrl() },
            false,
        );
        if shift {
            let id = take_editor_picker(&mut app);
            app.runtime.picker_finished(id, Ok(Some(path.clone())));
        }
        settle(&mut app);
        assert_eq!(
            app.main_snapshot().template.name,
            "Keyboard saved",
            "Save must not replace the current editor draft: {:?}",
            app.editor_snapshot()
        );
        assert!(
            app.editor.as_ref().unwrap().error.is_none(),
            "Save error: {:?}; main: {:?}",
            app.editor.as_ref().unwrap().error,
            app.main_snapshot().error
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap()
                ["name"],
            "Keyboard saved"
        );
        assert!(!app.main_snapshot().template_state.dirty);
        assert!(app.main_snapshot().file.is_none());
        assert!(!app.windows.editor_closing());
    }
}

#[test]
fn native_editor_pending_save_consumes_repeated_and_conflicting_shortcuts() {
    let (mut app, ctx, file) = loaded_editor();
    let original = std::fs::read(file.path()).unwrap();
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Pending save",
    );
    assert_eq!(app.main_snapshot().template.name, "Pending save");
    editor_key(
        &mut app,
        &ctx,
        egui::Key::S,
        egui::Modifiers {
            shift: true,
            ..ctrl()
        },
        false,
    );
    let id = take_editor_picker(&mut app);
    assert_eq!(
        app.main_snapshot().template.name,
        "Pending save",
        "Picker must preserve the current draft: {:?}",
        app.editor_snapshot()
    );
    assert!(app.editor_snapshot().busy);
    for label in ["Load…", "Unload", "Save", "Save As…"] {
        assert_editor_command_enabled(&mut app, &ctx, label, false, 760.0);
    }
    for (key, modifiers) in [
        (egui::Key::S, ctrl()),
        (
            egui::Key::S,
            egui::Modifiers {
                shift: true,
                ..ctrl()
            },
        ),
        (
            egui::Key::O,
            egui::Modifiers {
                alt: true,
                ..ctrl()
            },
        ),
        (
            egui::Key::U,
            egui::Modifiers {
                alt: true,
                ..ctrl()
            },
        ),
        (egui::Key::Enter, ctrl()),
        (egui::Key::W, ctrl()),
    ] {
        editor_key(&mut app, &ctx, key, modifiers, true);
        assert!(app.picker.is_none());
        assert!(app.editor_snapshot().busy);
        assert!(!app.editor_snapshot().close_prompt && !app.windows.editor_closing());
        assert_eq!(
            app.main_snapshot().template.name,
            "Pending save",
            "Blocked {key:?} {modifiers:?} reached the editor input"
        );
    }
    app.runtime.picker_finished(id, Ok(None));
    settle(&mut app);
    assert!(!app.editor_snapshot().busy);
    assert_eq!(std::fs::read(file.path()).unwrap(), original);
}

#[test]
fn native_editor_field_confirmation_blocks_close_save_and_load_shortcuts() {
    let (mut app, ctx, _file) = loaded_editor();
    click(&mut app, &ctx, "×");
    assert!(app.editor.as_ref().unwrap().controls.confirmation.is_some());
    for (key, modifiers) in [
        (egui::Key::W, ctrl()),
        (egui::Key::S, ctrl()),
        (
            egui::Key::S,
            egui::Modifiers {
                shift: true,
                ..ctrl()
            },
        ),
        (
            egui::Key::O,
            egui::Modifiers {
                alt: true,
                ..ctrl()
            },
        ),
    ] {
        editor_key(&mut app, &ctx, key, modifiers, false);
        assert!(!app.editor_snapshot().close_prompt && !app.windows.editor_closing());
        assert!(app.picker.is_none());
        assert!(app.editor.as_ref().unwrap().controls.confirmation.is_some());
    }
    assert_eq!(app.main_snapshot().template.fields.len(), 2);
    click(&mut app, &ctx, "Cancel");
    editor_key(
        &mut app,
        &ctx,
        egui::Key::A,
        egui::Modifiers {
            alt: true,
            ..ctrl()
        },
        false,
    );
    assert_eq!(app.main_snapshot().template.fields.len(), 2);
    click(&mut app, &ctx, "+ Field");
    assert_eq!(app.main_snapshot().template.fields.len(), 3);
}

#[test]
fn native_editor_invalid_name_gates_both_save_shortcuts_and_close_choices() {
    let (mut app, ctx, file) = loaded_editor();
    let original = std::fs::read(file.path()).unwrap();
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "");
    assert_eq!(app.main_snapshot().template.name, "");
    assert!(!app.editor.as_ref().unwrap().model.valid());
    for shift in [false, true] {
        editor_key(
            &mut app,
            &ctx,
            egui::Key::S,
            egui::Modifiers { shift, ..ctrl() },
            false,
        );
        assert!(app.picker.is_none());
        assert!(
            !app.editor_snapshot().busy,
            "Invalid save changed state: {:?}; local name {:?}",
            app.editor_snapshot(),
            app.editor.as_ref().unwrap().model.name
        );
    }
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    assert!(app.editor_snapshot().close_prompt);
    assert_editor_command_enabled(&mut app, &ctx, "Save As", false, 760.0);
    assert_editor_command_enabled(&mut app, &ctx, "Save", false, 760.0);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(660.0, 760.0),
            )),
            ..Default::default()
        },
        |ui| app.paint_editor(ui),
    );
    let reasons = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::Button
                && matches!(node.label(), Some("Save") | Some("Save As"))
        })
        .map(|(_, node)| node.description().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    output.textures_delta.clear();
    assert!(
        reasons.len() >= 2 && reasons.iter().all(|reason| reason.contains("Correct")),
        "Disabled close controls must explain validation: {reasons:?}"
    );
    close_choice(&mut app, &ctx, "Cancel");
    assert_eq!(app.editor.as_ref().unwrap().model.name, "");
    assert_eq!(std::fs::read(file.path()).unwrap(), original);
}

#[test]
fn native_editor_rejected_rename_survives_cancel_and_identical_file_reload() {
    let (mut app, ctx, file) = loaded_editor();
    ctx.enable_accesskit();
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    type_value(&mut app, &ctx, control_id(id, "name", None), "second");
    assert!(!app.editor.as_ref().unwrap().model.valid());
    assert_eq!(
        app.main_snapshot().template.fields[0].name.as_deref(),
        Some("first")
    );
    for shift in [false, true] {
        editor_key(
            &mut app,
            &ctx,
            egui::Key::S,
            egui::Modifiers { shift, ..ctrl() },
            false,
        );
        assert!(app.picker.is_none());
    }
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    close_choice(&mut app, &ctx, "Cancel");
    assert!(app.editor.as_ref().unwrap().model.raw.pending());
    let painted = frame(&mut app, &ctx, vec![]);
    let input_rect = ctx
        .read_response(control_id(id, "name", None))
        .unwrap()
        .rect;
    // Egui's accessible text lives in TextRun children, not the TextInput
    // parent's value. Assert the actual text painted in this stable input.
    assert!(painted
        .iter()
        .any(|(text, position)| text == "second" && input_rect.contains(*position)));
    let before = app.main_snapshot().template_state.workspace_revision;
    editor_key(
        &mut app,
        &ctx,
        egui::Key::O,
        egui::Modifiers {
            alt: true,
            ..ctrl()
        },
        false,
    );
    let picker = take_editor_picker(&mut app);
    app.runtime
        .picker_finished(picker, Ok(Some(file.path().into())));
    app.pump();
    assert!(
        app.main_snapshot().dialog.is_some(),
        "Replacing a rejected draft asks first"
    );
    close_choice(&mut app, &ctx, "OK");
    settle(&mut app);
    frame(&mut app, &ctx, vec![]);
    assert!(app.main_snapshot().template_state.workspace_revision > before);
    assert!(!app.editor.as_ref().unwrap().model.raw.pending());
    assert!(app.editor.as_ref().unwrap().model.valid());
    assert_eq!(
        app.editor.as_ref().unwrap().model.definition().fields[0]
            .name
            .as_deref(),
        Some("first")
    );
    let fresh_id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    let painted = frame(&mut app, &ctx, vec![]);
    let input_rect = ctx
        .read_response(control_id(fresh_id, "name", None))
        .unwrap()
        .rect;
    assert!(painted
        .iter()
        .any(|(text, position)| text == "first" && input_rect.contains(*position)));
    assert_editor_command_enabled(&mut app, &ctx, "Save", true, 760.0);
}

#[test]
fn native_editor_close_save_keeps_a_newer_authoritative_draft_open() {
    let (mut app, ctx, _file) = loaded_editor();
    let directory = tempfile::tempdir().unwrap();
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Saved draft",
    );
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    close_choice(&mut app, &ctx, "Save As");
    let picker = take_editor_picker(&mut app);
    let path = directory.path().join("checkpoint.json");
    app.runtime.picker_finished(picker, Ok(Some(path.clone())));
    app.pump();
    assert!(app.editor_snapshot().busy);
    // Equivalent to a newer main-window snapshot arriving while the editor's
    // save reply is in flight. The modal must not allow typing through it.
    let mut newer = app.main_snapshot().template;
    newer.name = "Newer unsaved edit".into();
    app.gesture(Action::TemplateChanged(newer));
    settle(&mut app);
    assert!(!app.windows.editor_closing());
    assert!(app.editor_snapshot().close_prompt);
    assert_eq!(
        app.editor.as_ref().unwrap().model.name,
        "Newer unsaved edit"
    );
    assert!(app.main_snapshot().template_state.dirty);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap()["name"],
        "Saved draft"
    );
}

#[test]
fn native_editor_unsaved_shortcuts_disable_save_but_allow_save_as() {
    let (mut app, ctx) = editor();
    click(&mut app, &ctx, "+ Field");
    assert!(app.editor_snapshot().template_state.saved_path.is_none());
    assert_editor_command_enabled(&mut app, &ctx, "Save", false, 760.0);
    assert_editor_command_enabled(&mut app, &ctx, "Save As…", true, 760.0);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("template-name")));
    editor_key(&mut app, &ctx, egui::Key::S, ctrl(), false);
    assert!(app.picker.is_none() && !app.editor_snapshot().busy);
    editor_key(
        &mut app,
        &ctx,
        egui::Key::S,
        egui::Modifiers {
            shift: true,
            ..ctrl()
        },
        false,
    );
    let picker = take_editor_picker(&mut app);
    app.runtime.picker_finished(picker, Ok(None));
    settle(&mut app);
    assert!(app.editor_snapshot().template_state.saved_path.is_none());
    assert_eq!(app.main_snapshot().template.fields.len(), 1);
    assert!(app.editor.as_ref().unwrap().error.is_none());
    assert!(!app.windows.editor_closing());
}

#[test]
fn native_editor_inactive_apply_and_unload_never_modify_a_focused_input() {
    let (mut app, ctx) = editor();
    frame(&mut app, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("template-name")));
    for (key, modifiers) in [
        (egui::Key::Enter, ctrl()),
        (
            egui::Key::U,
            egui::Modifiers {
                alt: true,
                ..ctrl()
            },
        ),
    ] {
        editor_key(&mut app, &ctx, key, modifiers, false);
        assert_eq!(app.editor.as_ref().unwrap().model.name, "Untitled");
        assert!(!app.editor_snapshot().template_state.active);
        assert!(!app.main_snapshot().template_applied);
        assert!(!app.editor_snapshot().busy);
        assert!(app.picker.is_none());
    }
}

#[test]
fn native_editor_failed_load_retains_draft_and_close_protection() {
    let (mut app, ctx, file) = loaded_editor();
    let original = std::fs::read(file.path()).unwrap();
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "Edited");
    editor_key(
        &mut app,
        &ctx,
        egui::Key::O,
        egui::Modifiers {
            alt: true,
            ..ctrl()
        },
        false,
    );
    let picker = take_editor_picker(&mut app);
    app.runtime
        .picker_finished(picker, Ok(Some(file.path().with_file_name("missing.json"))));
    app.pump();
    close_choice(&mut app, &ctx, "OK");
    settle(&mut app);
    assert!(app.editor.as_ref().unwrap().error.is_some());
    assert_eq!(app.editor.as_ref().unwrap().model.name, "Edited");
    assert_eq!(app.main_snapshot().template.fields.len(), 2);
    assert_eq!(
        app.main_snapshot().template_state.saved_path.as_deref(),
        Some(file.path())
    );
    assert_eq!(std::fs::read(file.path()).unwrap(), original);
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(660.0, 760.0),
        )),
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = ctx.run_ui(input, |ui| app.paint_editor(ui));
    output.textures_delta.clear();
    assert!(app.editor_snapshot().close_prompt);
    assert!(!app.windows.editor_closing());
    editor_key(
        &mut app,
        &ctx,
        egui::Key::Escape,
        egui::Modifiers::NONE,
        false,
    );
    close_choice(&mut app, &ctx, "Don't Save");
    assert!(app.windows.editor_closing());
    assert_eq!(app.main_snapshot().template.name, "Header");
}

#[test]
fn native_editor_failed_in_place_close_save_keeps_prompt_binding_and_draft() {
    let (mut app, ctx, file) = loaded_editor();
    // A controlled owned fixture loses its parent after Load. This forces an
    // actual atomic-write error without mocking or altering parser/persistence.
    std::fs::remove_file(file.path()).unwrap();
    std::fs::remove_dir(file.path().parent().unwrap()).unwrap();
    type_value(&mut app, &ctx, egui::Id::new("template-name"), "Edited");
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    close_choice(&mut app, &ctx, "Save");
    settle(&mut app);
    assert!(app.editor.as_ref().unwrap().error.is_some());
    assert!(app.editor_snapshot().close_prompt);
    assert!(!app.windows.editor_closing());
    assert_eq!(app.editor.as_ref().unwrap().model.name, "Edited");
    assert_eq!(
        app.main_snapshot().template_state.saved_path.as_deref(),
        Some(file.path())
    );
    assert!(app.main_snapshot().template_state.dirty);
    let before = app.editor.as_ref().unwrap().error.clone();
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    assert_eq!(app.editor.as_ref().unwrap().error, before);
    assert!(app.editor_snapshot().close_prompt && !app.windows.editor_closing());
}

#[test]
fn native_editor_right_click_has_no_browser_menu_or_template_mutation() {
    let (mut app, ctx, _file) = loaded_editor();
    ctx.enable_accesskit();
    frame(&mut app, &ctx, vec![]);
    let pos = ctx
        .read_response(egui::Id::new("template-name"))
        .unwrap()
        .rect
        .center();
    let before = app.main_snapshot().template;
    frame(&mut app, &ctx, vec![Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(660.0, 760.0),
            )),
            ..Default::default()
        },
        |ui| app.paint_editor(ui),
    );
    let menu_present = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| node.role() == egui::accesskit::Role::Menu);
    output.textures_delta.clear();
    assert!(!menu_present);
    assert_eq!(app.main_snapshot().template, before);
    assert!(app.editor.as_ref().unwrap().error.is_none());
}

#[test]
fn native_editor_footer_workflows_reach_the_authoritative_main_and_disk() {
    use std::io::Write;
    let (mut app, ctx) = editor();
    let mut binary = tempfile::NamedTempFile::new().unwrap();
    binary.write_all(&[41; 32]).unwrap();
    binary.flush().unwrap();
    app.gesture(Action::DropFiles(vec![binary.path().into()]));
    settle(&mut app);
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("header.json");
    std::fs::write(
        &original,
        br#"{"name":"Header","defaultEndianness":"little","fields":[]}"#,
    )
    .unwrap();
    click(&mut app, &ctx, "Load…");
    let picker = take_editor_picker(&mut app);
    app.runtime
        .picker_finished(picker, Ok(Some(original.clone())));
    settle(&mut app);
    assert!(!app.main_snapshot().template_applied);
    click(&mut app, &ctx, "+ Field");
    assert_eq!(app.main_snapshot().template.fields.len(), 1);
    click(&mut app, &ctx, "Save");
    settle(&mut app);
    let persisted = std::fs::read(&original).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&persisted).unwrap();
    assert_eq!(saved["fields"][0]["name"], "field1");
    assert!(!app.main_snapshot().template_applied);
    click(&mut app, &ctx, "Save As…");
    let picker = take_editor_picker(&mut app);
    let copy = directory.path().join("saved.json");
    app.runtime.picker_finished(picker, Ok(Some(copy.clone())));
    settle(&mut app);
    assert_eq!(
        app.main_snapshot().template_state.saved_path.as_deref(),
        Some(copy.as_path())
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(copy).unwrap()).unwrap(),
        saved
    );
    click(&mut app, &ctx, "Apply Template");
    settle(&mut app);
    let snapshot = app.main_snapshot();
    assert!(snapshot.template_applied);
    assert_eq!(snapshot.results[0].value.as_deref(), Some("41"));
    assert!(snapshot.selection.is_none());
    let preview = frame(&mut app, &ctx, vec![])
        .into_iter()
        .find(|(text, _)| text.starts_with("↗ "))
        .expect("Parsed leaf preview")
        .1;
    click_at(&mut app, &ctx, preview, 760.0);
    assert_eq!(
        app.main_snapshot()
            .template_range
            .as_ref()
            .map(|s| (s.start, s.end)),
        Some((0, 0))
    );
    editor_key(
        &mut app,
        &ctx,
        egui::Key::U,
        egui::Modifiers {
            alt: true,
            ..ctrl()
        },
        false,
    );
    settle(&mut app);
    assert!(!app.main_snapshot().template_state.active);
    assert!(!app.main_snapshot().template_applied);
    assert!(app.main_snapshot().results.is_empty() && app.main_snapshot().template_range.is_none());
    assert_eq!(
        std::fs::read(original).unwrap(),
        persisted,
        "Unload never rewrites the saved file"
    );
    assert!(app.windows.editor().is_some());
}

#[test]
fn native_template_save_prompts_for_external_changes_and_does_not_apply() {
    let (mut app, ctx, file) = loaded_editor();
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Edited in editor",
    );
    let external = br#"{"name":"External change","defaultEndianness":"little","fields":[]}"#;
    std::fs::write(file.path(), external).unwrap();
    for approved in [false, true] {
        editor_key(&mut app, &ctx, egui::Key::S, ctrl(), false);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.main_snapshot().dialog.is_none() {
            app.pump();
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        let dialog = app.main_snapshot().dialog.unwrap().1;
        assert_eq!(
            dialog.message,
            "The file has been modified by another program. Overwrite?"
        );
        assert_eq!(
            dialog.owner,
            hexforge_controller::shortcuts::WindowKind::Main
        );
        assert_eq!(std::fs::read(file.path()).unwrap(), external);
        assert!(app.main_snapshot().template_state.dirty);
        close_choice(&mut app, &ctx, if approved { "OK" } else { "Cancel" });
        settle(&mut app);
        let snapshot = app.main_snapshot();
        assert_eq!(snapshot.template.name, "Edited in editor");
        assert_eq!(
            snapshot.template_state.saved_path.as_deref(),
            Some(file.path())
        );
        assert!(!snapshot.template_applied && snapshot.results.is_empty());
        assert_eq!(snapshot.template_state.dirty, !approved);
        if approved {
            let json: serde_json::Value =
                serde_json::from_slice(&std::fs::read(file.path()).unwrap()).unwrap();
            assert_eq!(json["name"], "Edited in editor");
            assert_eq!(json["fields"].as_array().unwrap().len(), 2);
        } else {
            assert_eq!(std::fs::read(file.path()).unwrap(), external);
            assert_eq!(snapshot.template_state.baseline.name, "Header");
        }
    }
}

#[test]
fn native_template_save_as_binds_an_existing_target_only_after_approved_overwrite() {
    let (mut app, ctx) = editor();
    type_value(
        &mut app,
        &ctx,
        egui::Id::new("template-name"),
        "Internal draft title",
    );
    click(&mut app, &ctx, "+ Field");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.json");
    let original = br#"{"name":"Original on disk","defaultEndianness":"little","fields":[]}"#;
    std::fs::write(&path, original).unwrap();
    for approved in [false, true] {
        editor_key(
            &mut app,
            &ctx,
            egui::Key::S,
            egui::Modifiers {
                shift: true,
                ..ctrl()
            },
            false,
        );
        let id = take_editor_picker(&mut app);
        app.runtime.picker_finished(id, Ok(Some(path.clone())));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.main_snapshot().dialog.is_none() {
            app.pump();
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        let dialog = app.main_snapshot().dialog.unwrap().1;
        assert_eq!(dialog.message, "File already exists. Overwrite?");
        assert_eq!(
            dialog.owner,
            hexforge_controller::shortcuts::WindowKind::Main
        );
        assert!(app.main_snapshot().template_state.saved_path.is_none());
        assert!(app.main_snapshot().template_state.dirty);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        close_choice(&mut app, &ctx, if approved { "OK" } else { "Cancel" });
        settle(&mut app);
        let snapshot = app.main_snapshot();
        assert_eq!(snapshot.template.name, "Internal draft title");
        assert_eq!(snapshot.template.fields.len(), 1);
        assert!(!snapshot.template_applied && snapshot.results.is_empty());
        assert_eq!(snapshot.template_state.dirty, !approved);
        if approved {
            assert_eq!(
                snapshot.template_state.saved_path.as_deref(),
                Some(path.as_path())
            );
            assert_eq!(snapshot.template_state.display_name, "existing.json");
            assert_eq!(snapshot.template_state.baseline, snapshot.template);
            assert_eq!(
                snapshot.completion_notice.as_deref(),
                Some("Template saved as existing.json.")
            );
            let json: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(json["name"], "Internal draft title");
            assert_eq!(json["fields"].as_array().unwrap().len(), 1);
        } else {
            assert!(snapshot.template_state.saved_path.is_none());
            assert!(snapshot.completion_notice.is_none());
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    assert!(!app.editor_snapshot().close_prompt);
    assert!(
        app.windows.editor_closing(),
        "Successful Save As cleans the close baseline"
    );
}

#[test]
fn native_dont_save_restores_an_already_dirty_checkpoint_and_keeps_main_exit_protected() {
    for bound in [false, true] {
        let (mut app, ctx, file) = if bound {
            let (app, ctx, file) = loaded_editor();
            (app, ctx, Some(file))
        } else {
            let (app, ctx) = editor();
            (app, ctx, None)
        };
        let disk = file
            .as_ref()
            .map(|file| std::fs::read(file.path()).unwrap());
        type_value(
            &mut app,
            &ctx,
            egui::Id::new("template-name"),
            "Already dirty",
        );
        let initial = app.main_snapshot().template_state;
        app.editor_destroyed(app.windows.editor().unwrap());
        app.invoke(Command::TemplateEditor, Default::default());
        type_value(
            &mut app,
            &ctx,
            egui::Id::new("template-name"),
            "Temporary editor edit",
        );
        editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
        assert!(app.editor_snapshot().close_prompt);
        close_choice(&mut app, &ctx, "Don't Save");
        assert!(app.windows.editor_closing());
        let restored = app.main_snapshot().template_state;
        assert_eq!(restored.definition, initial.definition);
        assert_eq!(restored.baseline, initial.baseline);
        assert_eq!(restored.saved_path, initial.saved_path);
        assert!(restored.dirty);
        app.editor_destroyed(app.windows.editor().unwrap());
        app.invoke(Command::Exit, Default::default());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.main_snapshot().dialog.is_none() {
            app.pump();
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        let request = app.main_snapshot().dialog.unwrap().1;
        assert_eq!(
            request.message,
            "Unsaved template changes will be lost. Exit HexForge?"
        );
        assert!(!app.windows.main_closing());
        close_choice(&mut app, &ctx, "Cancel");
        assert!(!app.windows.main_closing());
        assert_eq!(app.main_snapshot().template.name, "Already dirty");
        assert!(app.main_snapshot().template_state.dirty);
        if let Some(file) = file {
            assert_eq!(std::fs::read(file.path()).unwrap(), disk.unwrap());
        }
    }
}

#[test]
fn native_editor_view_only_close_preserves_a_preexisting_dirty_workspace() {
    let (mut app, ctx) = editor();
    let initial = serde_json::from_value(
        serde_json::json!({"name":"Already modified","defaultEndianness":"little","fields":[]}),
    )
    .unwrap();
    app.gesture(Action::TemplateChanged(initial));
    // Reopen creates the source's entry checkpoint around the already dirty
    // workspace, not a new saved baseline or a discarded template.
    let session = app.windows.editor().unwrap();
    app.editor_destroyed(session);
    app.invoke(Command::TemplateEditor, Default::default());
    frame(&mut app, &ctx, vec![]);
    assert!(app.main_snapshot().template_state.dirty);
    editor_key(&mut app, &ctx, egui::Key::W, ctrl(), false);
    assert!(app.windows.editor_closing());
    assert!(!app.editor_snapshot().close_prompt);
    assert_eq!(app.main_snapshot().template.name, "Already modified");
    assert!(app.main_snapshot().template_state.dirty);
}

#[test]
fn native_editor_composition_owns_save_and_close_shortcuts() {
    let (mut app, ctx, file) = loaded_editor();
    let original = std::fs::read(file.path()).unwrap();
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("template-name")));
    frame(
        &mut app,
        &ctx,
        vec![Event::Ime(egui::ImeEvent::Preedit {
            text: "文".into(),
            active_range_chars: None,
        })],
    );
    for key in [egui::Key::S, egui::Key::W] {
        editor_key(&mut app, &ctx, key, ctrl(), false);
        assert!(!app.editor_snapshot().busy);
        assert!(!app.editor_snapshot().close_prompt && !app.windows.editor_closing());
        assert!(app.picker.is_none());
        assert_eq!(std::fs::read(file.path()).unwrap(), original);
    }
}
