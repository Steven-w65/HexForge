//! Controlled OS picker boundaries; all application transitions and effective
//! file reads run through the real NativeApp/controller/core implementation.
use super::capture_spec::CaptureSpec;
use hexforge_controller::{action::Action, command::Command};
use hexforge_native::app::NativeApp;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Screenshot evidence must include the real accepted visible bytes, not just
/// elapsed frames. This is a steady-state QA gate, never startup readiness.
pub fn frame_ready(
    spec: &CaptureSpec,
    snapshot: &hexforge_controller::controller::MainSnapshot,
    frame: &hexforge_native::views::hex_view::HexFrame,
) -> bool {
    if spec.config().surface == "editor" {
        return true;
    }
    let Some(file) = &snapshot.file else {
        return true;
    };
    let Ok(size) = file.size.parse::<u64>() else {
        return false;
    };
    if size == 0 {
        return true;
    }
    let Some(last_row) = frame.last_visible_row else {
        return false;
    };
    if frame.viewport_rect.width() <= 0.0 || frame.viewport_rect.height() <= 0.0 {
        return false;
    }
    let width = u128::from(snapshot.row_width.bytes());
    let first = u128::from(frame.top_row) * width;
    let end = ((u128::from(last_row) + 1) * width).min(u128::from(size));
    snapshot.page.as_ref().is_some_and(|page| {
        page.revision == file.revision
            && page.offset.parse::<u64>().ok().is_some_and(|offset| {
                u128::from(offset) <= first && u128::from(offset) + page.bytes.len() as u128 >= end
            })
    })
}

fn settle(app: &mut NativeApp) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.pump();
        let snapshot = app.main_snapshot();
        if let Some(error) = snapshot.error {
            return Err(error.message);
        }
        if !snapshot.policy.operation_busy {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("Actual capture workflow did not complete".into());
        }
        std::thread::yield_now();
    }
}
pub fn prepare_scene(app: &mut NativeApp, spec: &CaptureSpec) -> Result<(), String> {
    let scene = spec.config();
    if let Some(name) = scene.binary {
        let expected_size = match name.as_str() {
            "empty.bin" => 0,
            "parity.bin" => 16000,
            _ => return Err("Unknown controlled binary fixture".into()),
        };
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/visual")
            .join(name)
            .canonicalize()
            .map_err(|e| format!("Resolve controlled binary fixture: {e}"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("Read controlled fixture: {e}"))?;
        if bytes.len() != expected_size
            || bytes.iter().enumerate().any(|(i, b)| *b != (i % 256) as u8)
        {
            return Err("Actual binary fixture differs from the shared scene bytes".into());
        }
        app.invoke(Command::Open, Default::default());
        let id = app.picker.take().ok_or("Open did not request a picker")?.0;
        app.runtime.picker_finished(id, Ok(Some(path)));
        settle(app)?;
        if app.main_snapshot().file.as_ref().map(|f| f.size.as_str())
            != Some(bytes.len().to_string().as_str())
        {
            return Err("Native file did not open as the declared capture size".into());
        }
    }
    if let Some(edit) = scene.edit {
        app.gesture(Action::Select {
            start: edit.offset,
            end: edit.offset,
        });
        app.invoke(Command::ToggleEdit, Default::default());
        app.gesture(Action::EditSelectedByte(format!("{:02X}", edit.value)));
        settle(app)?;
    }
    if let Some(value) = scene.search {
        app.invoke(Command::Search, Default::default());
        app.gesture(Action::SetSearchPattern(value));
    }
    if let Some(width) = scene.bytes_per_row {
        app.invoke(
            match width {
                16 => Command::Row16,
                32 => Command::Row32,
                _ => return Err("Unsupported capture row width".into()),
            },
            Default::default(),
        );
    }
    if let Some(value) = scene.goto {
        app.invoke(Command::Goto, Default::default());
        app.gesture(Action::SetGotoValue(value));
    }
    if scene.mode.as_deref() == Some("proportional") {
        app.invoke(Command::MinimapProportional, Default::default());
    }
    if let Some(row) = scene.navigate_row {
        app.gesture(Action::ScrollViewport {
            offset: row
                .checked_mul(u64::from(scene.bytes_per_row.unwrap_or(16)))
                .ok_or("Capture offset overflow")?,
        });
    }
    if let Some(name) = scene.template {
        if !matches!(name.as_str(), "structures.json" | "diagnostics.json") {
            return Err("Unknown controlled template fixture".into());
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/visual")
            .join(name)
            .canonicalize()
            .map_err(|e| format!("Resolve controlled template fixture: {e}"))?;
        app.invoke(Command::LoadTemplate, Default::default());
        let id = app
            .picker
            .take()
            .ok_or("Template load did not request a picker")?
            .0;
        app.runtime.picker_finished(id, Ok(Some(path)));
        settle(app)?;
    }
    if scene.apply == Some(true) {
        app.invoke(Command::ApplyTemplate, Default::default());
        settle(app)?;
        if !app.main_snapshot().template_applied {
            return Err("Actual template was not applied".into());
        }
    }
    if let Some(path) = scene.selected_path {
        fn leaf<'a>(
            nodes: &'a [hexforge_core::template::ParsedNode],
            path: &str,
        ) -> Option<&'a hexforge_core::template::ParsedNode> {
            nodes.iter().find_map(|node| {
                if node.path == path {
                    Some(node)
                } else {
                    leaf(&node.children, path)
                }
            })
        }
        let snapshot = app.main_snapshot();
        let node = leaf(&snapshot.results, &path).ok_or("Declared result path is absent")?;
        let (start, end) = hexforge_controller::results::navigable_range(node)
            .ok_or("Declared result does not have a successfully decoded byte range")?;
        app.gesture(Action::NavigateTemplate { start, end });
    }
    if let Some(offset) = scene.clear_selection {
        app.gesture(Action::Select {
            start: offset,
            end: offset,
        });
    }
    if scene.surface == "editor" {
        app.invoke(Command::TemplateEditor, Default::default());
        if let Some(name) = scene.editor_name {
            app.editor
                .as_mut()
                .ok_or("Actual editor did not open")?
                .model
                .name = name;
            app.publish_editor().map_err(|e| e.message)?;
        }
        if scene.editor_close == Some(true) {
            let session = app
                .editor_snapshot()
                .session
                .ok_or("Actual editor session is absent")?;
            app.editor_message(
                hexforge_controller::template::coordinator::EditorMessage::CloseRequested {
                    session,
                },
            );
            if !app.editor_snapshot().close_prompt {
                return Err("Actual editor close did not protect its changed checkpoint".into());
            }
        }
        if scene.save_error == Some(true) {
            use hexforge_controller::template::coordinator::{EditorCloseDecision, EditorMessage};
            let session = app
                .editor_snapshot()
                .session
                .ok_or("Actual editor session is absent")?;
            app.editor_message(EditorMessage::CloseDecision {
                session,
                decision: EditorCloseDecision::SaveAs,
            });
            let id = app
                .picker
                .take()
                .ok_or("Actual close Save As did not request a picker")?
                .0;
            // Fail only the owned OS picker boundary. The authoritative save /
            // close coordinator retains the loaded binding and changed draft.
            app.runtime.picker_finished(
                id,
                Err(hexforge_core::error::AppError::new(
                    hexforge_core::error::ErrorCode::IoError,
                    "Could not save the template.",
                    None,
                )),
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.main_snapshot().policy.operation_busy {
                app.pump();
                if Instant::now() > deadline {
                    return Err("Actual failed-save workflow did not finish".into());
                }
                std::thread::yield_now();
            }
        }
    }
    if scene.close_binary == Some(true) {
        app.invoke(Command::CloseFile, Default::default());
    }
    if scene.busy == Some(true) {
        app.invoke(Command::Open, Default::default());
    }
    if scene.error == Some(true) {
        app.gesture(Action::DropFiles(vec![]));
    }
    Ok(())
}

/// Trusted fixture input is delivered to the real presenters, never used to
/// mutate MenuView / ResultView private state or claim compositor readiness.
pub fn frame_input(spec: &CaptureSpec, frame: usize) -> Vec<eframe::egui::Event> {
    use eframe::egui::{self, Event, Key, Modifiers};
    fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
        vec![
            Event::ModifiersChanged(modifiers),
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ]
    }
    let scene = spec.config();
    if frame == 2 {
        if let Some(menu) = scene.menu.as_deref() {
            return key(
                match menu {
                    "file" => Key::F,
                    "edit" => Key::E,
                    "navigate" => Key::N,
                    "template" => Key::T,
                    "view" => Key::V,
                    _ => unreachable!("reviewed menu fixture"),
                },
                Modifiers::ALT,
            );
        }
    }
    match (scene.flyout.as_deref(), frame) {
        (Some("minimap"), 4 | 6 | 8) => return key(Key::ArrowDown, Modifiers::NONE),
        (Some("minimap"), 10) => return key(Key::ArrowRight, Modifiers::NONE),
        (Some("results"), 4) => return key(Key::R, Modifiers::NONE),
        _ => {}
    }
    if scene.focus_result.as_deref() == Some("header.signature") {
        let pos = egui::pos2(60.0, spec.client_size[1] as f32 - 145.0);
        if frame == 4 || frame == 5 {
            return vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 4,
                    modifiers: Modifiers::NONE,
                },
            ];
        }
        if frame == 6 {
            return key(Key::ArrowDown, Modifiers::NONE);
        }
    }
    vec![]
}
