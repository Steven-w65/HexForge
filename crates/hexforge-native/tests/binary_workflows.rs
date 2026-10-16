//! Actual guarded binary transitions, disk bytes and pointer-driven decisions.
//! Platform picker replies are controlled; core saves/opens/edits are real.
#[path = "support/binary_ui.rs"]
mod ui;
use eframe::egui;
use hexforge_controller::{action::Action, command::Command};
use hexforge_core::error::{AppError, ErrorCode};
use hexforge_native::app::NativeApp;
use std::path::Path;

fn opened(source: &Path) -> (NativeApp, egui::Context) {
    let (mut app, ctx) = ui::native();
    app.invoke(Command::Open, Default::default());
    let (id, picker) = app.picker.take().unwrap();
    assert_eq!(
        picker.purpose,
        hexforge_controller::dialog::PickerPurpose::BinaryOpen
    );
    app.runtime.picker_finished(id, Ok(Some(source.into())));
    ui::settle(&mut app);
    (app, ctx)
}
fn edit(app: &mut NativeApp, value: &str) {
    app.gesture(Action::Select {
        start: 128,
        end: 128,
    });
    if !app.main_snapshot().edit_mode {
        app.invoke(Command::ToggleEdit, Default::default());
    }
    app.gesture(Action::EditSelectedByte(value.into()));
    ui::settle(app);
    app.gesture(Action::ReadViewport {
        offset: 0,
        length: 1024,
    });
    ui::wait_until(app, |app| {
        app.main_snapshot()
            .page
            .as_ref()
            .is_some_and(|p| p.revision == app.main_snapshot().file.as_ref().unwrap().revision)
    });
}
fn files() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.custom");
    let next = directory.path().join("next.rom");
    std::fs::write(&source, vec![0x41; 1024]).unwrap();
    std::fs::write(&next, vec![0x42; 1024]).unwrap();
    (directory, source, next)
}
fn request_replacement(app: &mut NativeApp, ctx: &egui::Context, next: &Path) {
    app.invoke(Command::Open, Default::default());
    let (id, _) = app.picker.take().unwrap();
    app.runtime.picker_finished(id, Ok(Some(next.into())));
    app.pump();
    assert!(app.main_snapshot().dialog.is_some());
    ui::click(app, ctx, "Save Copy and Continue");
}

#[test]
fn real_edit_dialog_rejects_invalid_hex_before_writing_and_refreshes_the_effective_page() {
    let (_directory, source, _) = files();
    let (mut app, ctx) = opened(&source);
    app.gesture(Action::ReadViewport {
        offset: 0,
        length: 1024,
    });
    ui::wait_until(&mut app, |a| a.main_snapshot().page.is_some());
    app.gesture(Action::Select { start: 7, end: 7 });
    app.invoke(Command::ToggleEdit, Default::default());
    app.invoke(Command::EditSelected, Default::default());
    ui::paint(&mut app, &ctx, vec![]);
    ui::paint(&mut app, &ctx, vec![]);
    ui::paint(&mut app, &ctx, vec![egui::Event::Text("GG".into())]);
    ui::click(&mut app, &ctx, "Apply");
    let invalid = app.main_snapshot();
    assert!(!invalid.policy.operation_busy);
    assert!(!invalid.file.as_ref().unwrap().dirty);
    assert!(!invalid.can_undo);
    assert_eq!(invalid.page.as_ref().unwrap().bytes[7], 0x41);
    assert_eq!(invalid.edit_dialog.as_ref().unwrap().value, "GG");
    assert!(invalid
        .edit_dialog
        .as_ref()
        .unwrap()
        .error
        .as_ref()
        .unwrap()
        .contains("exactly two"));
    assert!(ui::paint(&mut app, &ctx, vec![])
        .iter()
        .any(|(t, _)| t.contains("exactly two")));
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("edit-byte-value")));
    ui::paint(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        }],
    );
    ui::paint(&mut app, &ctx, vec![egui::Event::Text("ff".into())]);
    ui::click(&mut app, &ctx, "Apply");
    ui::wait_until(&mut app, |a| {
        let s = a.main_snapshot();
        s.edit_dialog.is_none() && s.page.as_ref().is_some_and(|p| p.bytes[7] == 255)
    });
    let edited = app.main_snapshot();
    assert!(edited.file.as_ref().unwrap().dirty);
    assert!(edited.can_undo);
    assert_eq!(edited.page.as_ref().unwrap().modified_offsets, ["7"]);
    assert_eq!(edited.selection.unwrap().start, 7);
    assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
}

#[test]
fn cancelled_and_failed_open_pickers_are_safe_and_render_real_native_errors() {
    let (mut app, ctx) = ui::native();
    for fail in [false, true] {
        app.invoke(Command::Open, Default::default());
        let (id, _) = app.picker.take().unwrap();
        app.runtime.picker_finished(
            id,
            if fail {
                Err(AppError::new(
                    ErrorCode::OperationFailed,
                    "Native open failed.",
                    None,
                ))
            } else {
                Ok(None)
            },
        );
        ui::settle(&mut app);
        assert!(app.main_snapshot().file.is_none());
        assert_eq!(app.main_snapshot().error.is_some(), fail);
        let text = ui::paint(&mut app, &ctx, vec![]);
        assert!(text.iter().any(|(s, _)| s == "Open File"));
        if fail {
            assert!(text.iter().any(|(s, _)| s == "Native open failed."));
            assert!(text.iter().any(|(s, _)| s == "Operation failed"));
        } else {
            assert!(app.main_snapshot().completion_notice.is_none());
        }
    }
}

#[test]
fn rejected_native_save_picker_renders_its_error_without_changing_the_active_file() {
    let (directory, source, _) = files();
    let (mut app, ctx) = opened(&source);
    edit(&mut app, "FF");
    let before = app.main_snapshot();
    app.invoke(Command::SaveAs, Default::default());
    let (id, _) = app.picker.take().unwrap();
    app.runtime.picker_finished(
        id,
        Err(AppError::new(
            ErrorCode::OperationFailed,
            "Native save failed.",
            None,
        )),
    );
    ui::settle(&mut app);
    let after = app.main_snapshot();
    assert_eq!(after.source, before.source);
    assert_eq!(after.selection, before.selection);
    assert_eq!(
        after.page.as_ref().unwrap().bytes,
        before.page.as_ref().unwrap().bytes
    );
    assert!(after.file.as_ref().unwrap().dirty);
    assert!(after.can_undo);
    assert!(!after.policy.operation_busy);
    let text = ui::paint(&mut app, &ctx, vec![]);
    assert!(text.iter().any(|(s, _)| s == "Operation failed"));
    assert!(text.iter().any(|(s, _)| s == "Native save failed."));
    assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        2,
        "a rejected picker must never create a copy or a sidecar"
    );
}

#[test]
fn close_file_cancel_and_discard_keep_source_protection_and_clear_only_after_approval() {
    let (_directory, source, _) = files();
    let (mut app, ctx) = opened(&source);
    edit(&mut app, "FF");
    app.gesture(Action::SetSearchPattern("41".into()));
    app.gesture(Action::SubmitSearch {
        force: false,
        direction: hexforge_controller::search::MatchDirection::Next,
    });
    ui::settle(&mut app);
    let definition = serde_json::from_str(r#"{"name":"Keep template","defaultEndianness":"little","fields":[{"name":"current","type":"u8","placement":{"mode":"absolute","offset":"128"}}]}"#).unwrap();
    app.gesture(Action::TemplateChanged(definition));
    app.invoke(Command::ApplyTemplate, Default::default());
    ui::settle(&mut app);
    ui::click(&mut app, &ctx, "current");
    app.gesture(Action::ScrollViewport { offset: 256 });
    let before = app.main_snapshot();
    assert!(!before.matches.is_empty());
    assert_eq!(before.results[0].value.as_deref(), Some("255"));
    assert!(before.template_range.is_some());
    assert!(before.template_applied);
    assert!(before.edit_mode);
    assert_eq!(before.viewport_offset, 256);
    assert!(before
        .page
        .as_ref()
        .unwrap()
        .modified_offsets
        .contains(&"128".into()));
    app.invoke(Command::CloseFile, Default::default());
    assert_eq!(app.main_snapshot().source, before.source);
    ui::click(&mut app, &ctx, "Cancel");
    assert!(app.main_snapshot().file.as_ref().unwrap().dirty);
    assert_eq!(app.main_snapshot().selection, before.selection);
    assert_eq!(app.main_snapshot().matches, before.matches);
    assert_eq!(app.main_snapshot().results, before.results);
    assert_eq!(app.main_snapshot().template_range, before.template_range);
    assert_eq!(app.main_snapshot().viewport_offset, before.viewport_offset);
    assert_eq!(app.main_snapshot().page.as_ref().unwrap().bytes[128], 255);
    assert!(!app.main_snapshot().policy.operation_busy);
    app.invoke(Command::CloseFile, Default::default());
    ui::click(&mut app, &ctx, "Discard");
    ui::settle(&mut app);
    let after = app.main_snapshot();
    assert!(after.file.is_none());
    assert!(after.source.is_none());
    assert!(after.selection.is_none());
    assert!(after.page.is_none());
    assert!(after.template_range.is_none());
    assert!(!after.can_undo);
    assert!(!after.edit_mode);
    assert_eq!(after.viewport_offset, 0);
    assert!(after.matches.is_empty());
    assert!(after.results.is_empty());
    assert!(!after.template_applied);
    assert!(after.template_state.active);
    assert_eq!(after.template.name, "Keep template");
    assert!(after.modified_overview.bins.is_empty());
    assert!(ui::paint(&mut app, &ctx, vec![])
        .iter()
        .any(|(s, _)| s == "Drop a binary file"));
    assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
}

#[test]
fn replacement_binary_waits_for_saved_effective_bytes_and_ignores_concurrent_open_and_drop() {
    let (directory, source, next) = files();
    let copy = directory.path().join("copy.bin");
    let unwanted = directory.path().join("unwanted.bin");
    std::fs::write(&unwanted, [0x99]).unwrap();
    let (mut app, ctx) = opened(&source);
    edit(&mut app, "FF");
    let before = app.main_snapshot();
    request_replacement(&mut app, &ctx, &next);
    let (id, picker) = app.picker.take().unwrap();
    assert_eq!(
        picker.purpose,
        hexforge_controller::dialog::PickerPurpose::BinarySave
    );
    app.invoke(Command::Open, Default::default());
    app.gesture(Action::DropFiles(vec![unwanted]));
    assert!(app.picker.is_none());
    assert_eq!(app.main_snapshot().source, before.source);
    assert!(app.main_snapshot().file.as_ref().unwrap().dirty);
    assert!(
        !copy.exists(),
        "replacement must wait for successful persistence"
    );
    app.runtime.picker_finished(id, Ok(Some(copy.clone())));
    ui::settle(&mut app);
    let mut expected = vec![0x41; 1024];
    expected[128] = 255;
    assert_eq!(std::fs::read(copy).unwrap(), expected);
    assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
    let after = app.main_snapshot();
    assert_eq!(after.file.as_ref().unwrap().name, "next.rom");
    assert!(!after.file.as_ref().unwrap().dirty);
    assert_ne!(after.source, before.source);
    assert!(after.selection.is_none());
    assert!(!after.can_undo);
    app.gesture(Action::ReadViewport {
        offset: 0,
        length: 1024,
    });
    ui::wait_until(&mut app, |app| app.main_snapshot().page.is_some());
    assert_eq!(app.main_snapshot().page.unwrap().bytes, vec![0x42; 1024]);
    assert!(ui::paint(&mut app, &ctx, vec![]).iter().any(|(s, _)| s == "next.rom"),
        "the native file strip must render the accepted replacement, not just update a worker model");
}

#[test]
fn cancelled_or_failed_copy_before_replacement_keeps_the_original_edited_page_and_markers() {
    for fail in [false, true] {
        let (directory, source, next) = files();
        let (mut app, ctx) = opened(&source);
        edit(&mut app, "FF");
        let before = app.main_snapshot();
        request_replacement(&mut app, &ctx, &next);
        let (id, _) = app.picker.take().unwrap();
        app.runtime.picker_finished(
            id,
            Ok(if fail {
                Some(directory.path().into())
            } else {
                None
            }),
        );
        ui::settle(&mut app);
        let after = app.main_snapshot();
        assert_eq!(after.file.as_ref().unwrap().name, "source.custom");
        assert_eq!(after.source, before.source);
        assert_eq!(after.selection, before.selection);
        assert_eq!(
            after.page.as_ref().unwrap().bytes,
            before.page.as_ref().unwrap().bytes
        );
        assert_eq!(
            after.page.as_ref().unwrap().modified_offsets,
            before.page.as_ref().unwrap().modified_offsets
        );
        assert!(after.file.as_ref().unwrap().dirty);
        assert!(after.can_undo);
        assert_eq!(after.error.is_some(), fail);
        assert!(!after.policy.operation_busy);
        assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
    }
}

#[test]
fn save_then_close_persists_the_effective_copy_before_clearing_the_active_binary() {
    let (directory, source, _) = files();
    let copy = directory.path().join("copy.bin");
    let (mut app, ctx) = opened(&source);
    edit(&mut app, "FF");
    app.invoke(Command::CloseFile, Default::default());
    ui::click(&mut app, &ctx, "Save Copy and Continue");
    let (id, _) = app.picker.take().unwrap();
    assert!(app.main_snapshot().file.as_ref().unwrap().dirty);
    assert!(!copy.exists());
    app.runtime.picker_finished(id, Ok(Some(copy.clone())));
    ui::settle(&mut app);
    assert!(app.main_snapshot().file.is_none());
    let mut expected = vec![0x41; 1024];
    expected[128] = 255;
    assert_eq!(std::fs::read(copy).unwrap(), expected);
    assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
}

#[test]
fn exit_copy_cancel_failure_and_picker_error_release_the_close_guard_and_allow_later_edits() {
    for outcome in ["cancel", "save-failure", "picker-error"] {
        let (directory, source, _) = files();
        let (mut app, ctx) = opened(&source);
        edit(&mut app, "FF");
        app.invoke(Command::TemplateEditor, Default::default());
        let child = app.windows.editor().unwrap();
        let before = app.main_snapshot();
        app.invoke(Command::Exit, Default::default());
        ui::wait_until(&mut app, |app| app.main_snapshot().dialog.is_some());
        ui::click(&mut app, &ctx, "Save Copy and Continue");
        let (id, _) = app.picker.take().unwrap();
        assert!(!app.windows.editor_closing());
        assert!(!app.windows.main_closing());
        app.runtime.picker_finished(
            id,
            match outcome {
                "cancel" => Ok(None),
                "save-failure" => Ok(Some(directory.path().into())),
                _ => Err(AppError::new(
                    ErrorCode::OperationFailed,
                    "Save picker failed.",
                    None,
                )),
            },
        );
        ui::settle(&mut app);
        let after = app.main_snapshot();
        assert_eq!(app.windows.editor(), Some(child));
        assert!(!app.windows.editor_closing());
        assert!(!app.windows.main_closing());
        assert_eq!(after.source, before.source);
        assert_eq!(after.selection, before.selection);
        assert!(after.file.as_ref().unwrap().dirty);
        assert_eq!(after.error.is_some(), outcome != "cancel");
        app.gesture(Action::ClearError);
        edit(&mut app, "FE");
        assert_eq!(
            app.main_snapshot().page.as_ref().unwrap().bytes[128],
            254,
            "a cancelled/failed main exit must release the mutation barrier"
        );
        assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
    }
}

#[test]
fn successful_exit_copy_waits_for_the_disk_save_and_the_companion_close_acknowledgement() {
    let (directory, source, _) = files();
    let copy = directory.path().join("copy.bin");
    let (mut app, ctx) = opened(&source);
    edit(&mut app, "FF");
    app.invoke(Command::TemplateEditor, Default::default());
    let child = app.windows.editor().unwrap();
    app.invoke(Command::Exit, Default::default());
    ui::wait_until(&mut app, |app| app.main_snapshot().dialog.is_some());
    ui::click(&mut app, &ctx, "Save Copy and Continue");
    let (id, _) = app.picker.take().unwrap();
    assert!(!app.windows.main_closing());
    assert!(!app.windows.editor_closing());
    assert!(!copy.exists());
    app.runtime.picker_finished(id, Ok(Some(copy.clone())));
    ui::wait_until(&mut app, |app| app.windows.editor_closing());
    assert!(
        !app.windows.main_closing(),
        "main must wait for the actual companion close acknowledgement"
    );
    assert!(!app.main_snapshot().file.as_ref().unwrap().dirty);
    let mut expected = vec![0x41; 1024];
    expected[128] = 255;
    assert_eq!(std::fs::read(copy).unwrap(), expected);
    app.editor_destroyed(child);
    assert!(app.windows.main_closing());
    assert!(app.windows.editor().is_none());
    assert_eq!(std::fs::read(source).unwrap(), vec![0x41; 1024]);
}
