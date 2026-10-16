#[path = "support/capture_scene.rs"]
mod capture_scene;
#[path = "support/capture_spec.rs"]
mod capture_spec;
use capture_spec::CaptureSpec;

#[test]
fn capture_retains_exact_logical_and_physical_client_dimensions() {
    for dpr in [1.0, 1.25, 1.5, 2.0] {
        let spec = CaptureSpec::new("empty", "dark", [1280, 800], dpr).unwrap();
        assert_eq!(
            (&*spec.scene, &*spec.theme, spec.client_size),
            ("empty", "dark", [1280, 800])
        );
        assert_eq!(
            spec.physical_size(),
            [(1280.0 * dpr) as u32, (800.0 * dpr) as u32]
        );
        assert!(spec.accepts_frame(spec.physical_size(), dpr));
        assert!(
            !spec.accepts_frame([1282, 831], dpr),
            "decorations must not be included"
        );
        assert!(
            !spec.accepts_frame(spec.physical_size(), dpr + 0.01),
            "do not silently rescale"
        );
    }
}

#[test]
fn capture_waits_for_actual_visible_bytes_instead_of_an_arbitrary_frame_count() {
    use hexforge_controller::{
        controller::{AppController, InitialState, ViewportPage, WindowSnapshot},
        shortcuts::WindowKind,
    };
    use hexforge_core::dto::{FileInfoDto, PageResponse};
    use hexforge_native::views::hex_view::HexFrame;
    let spec = CaptureSpec::new("selected-field", "dark", [1280, 800], 1.0).unwrap();
    let WindowSnapshot::Main(mut snapshot) =
        AppController::new(InitialState::default()).snapshot(WindowKind::Main)
    else {
        unreachable!()
    };
    snapshot.file = Some(FileInfoDto {
        name: "parity.bin".into(),
        path: "parity.bin".into(),
        size: "16000".into(),
        revision: "1".into(),
        dirty: true,
    });
    let frame = HexFrame {
        top_row: 2,
        last_visible_row: Some(22),
        viewport_rect: eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(1182.0, 476.0),
        ),
        ..Default::default()
    };
    assert!(!capture_scene::frame_ready(&spec, &snapshot, &frame));
    snapshot.page = Some(ViewportPage {
        generation: 1,
        data: PageResponse {
            offset: "32".into(),
            bytes: vec![0x41; 336],
            modified_offsets: vec![],
            revision: "0".into(),
        },
    });
    assert!(
        !capture_scene::frame_ready(&spec, &snapshot, &frame),
        "Old revision cannot establish paint readiness"
    );
    snapshot.page.as_mut().unwrap().data.revision = "1".into();
    assert!(capture_scene::frame_ready(&spec, &snapshot, &frame));
    snapshot.page.as_mut().unwrap().data.bytes.pop();
    assert!(
        !capture_scene::frame_ready(&spec, &snapshot, &frame),
        "A partly missing final visible row is not a settled capture"
    );
    snapshot.page.as_mut().unwrap().data.offset = "33".into();
    snapshot.page.as_mut().unwrap().data.bytes.push(0x41);
    assert!(
        !capture_scene::frame_ready(&spec, &snapshot, &frame),
        "The first visible byte must be present too"
    );
    snapshot.file.as_mut().unwrap().size = "0".into();
    snapshot.page = None;
    assert!(
        capture_scene::frame_ready(&spec, &snapshot, &frame),
        "An empty file has no byte read obligation"
    );
    snapshot.file = None;
    assert!(capture_scene::frame_ready(&spec, &snapshot, &frame));
}

#[test]
fn capture_rejects_invalid_or_unbounded_framebuffers_and_unknown_scenes() {
    for size in [[0, 800], [1280, 0], [u32::MAX, 800], [100_000, 100_000]] {
        assert!(CaptureSpec::new("empty", "dark", size, 1.0).is_err());
    }
    for dpr in [0.0, -1.0, f64::INFINITY, f64::NAN, 100.0] {
        assert!(CaptureSpec::new("empty", "dark", [1280, 800], dpr).is_err());
    }
    assert!(CaptureSpec::new("splash", "dark", [1280, 800], 1.0).is_err());
    assert!(CaptureSpec::new("empty", "unknown", [1280, 800], 1.0).is_err());
}

#[test]
fn real_file_workflow_scenes_are_not_replaced_by_the_empty_interface() {
    assert_eq!(
        CaptureSpec::new("file-menu", "dark", [1280, 800], 1.0)
            .unwrap()
            .config()
            .menu
            .as_deref(),
        Some("file")
    );
    for scene in [
        "open",
        "empty-file",
        "edited",
        "search",
        "goto",
        "fit-top",
        "fit-middle",
        "fit-eof",
        "proportional-top",
        "proportional-middle",
        "proportional-eof",
    ] {
        assert!(
            CaptureSpec::new(scene, "dark", [1280, 800], 1.0).is_ok(),
            "Missing real capture scene: {scene}"
        );
    }
}

#[test]
fn file_scenes_execute_the_actual_open_edit_search_and_navigation_workflows() {
    use hexforge_controller::controller::{InitialState, MinimapMode};
    use hexforge_native::app::NativeApp;
    for scene in [
        "open",
        "empty-file",
        "edited",
        "search",
        "goto",
        "proportional-eof",
    ] {
        let spec = CaptureSpec::new(scene, "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            InitialState::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let s = app.main_snapshot();
        assert_eq!(
            s.file.as_ref().unwrap().size,
            if scene == "empty-file" { "0" } else { "16000" }
        );
        if scene == "edited" {
            assert!(s.file.as_ref().unwrap().dirty && s.can_undo && s.edit_mode);
            assert_eq!(s.selection.unwrap().start, 128);
        }
        if scene == "search" {
            assert!(s.search.open);
            assert_eq!(s.search.raw, "41 42 43");
        }
        if scene == "goto" {
            assert!(s.goto.open);
            assert_eq!(s.goto.raw, "0x100");
        }
        if scene == "proportional-eof" {
            assert_eq!(s.minimap.mode, MinimapMode::Proportional);
            assert_eq!(s.viewport_offset, 15984);
        }
    }
}

#[test]
fn populated_capture_scenes_load_without_apply_and_keep_result_selection_owned() {
    use hexforge_native::app::NativeApp;
    for scene in [
        "pending-template",
        "selected-field",
        "cleared-field",
        "results-diagnostics",
    ] {
        let spec = CaptureSpec::new(scene, "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let s = app.main_snapshot();
        assert!(s.template_state.active);
        assert!(!s.template_state.dirty);
        let name = if scene == "results-diagnostics" {
            "diagnostics.json"
        } else {
            "structures.json"
        };
        assert_eq!(
            s.template_state
                .saved_path
                .as_ref()
                .unwrap()
                .file_name()
                .unwrap(),
            name
        );
        if scene == "pending-template" {
            assert!(!s.template_applied);
            assert!(s.results.is_empty());
        } else if scene == "results-diagnostics" {
            assert!(s.template_applied);
            let signature = &s.results[0].children[0];
            assert_eq!(
                (&*signature.path, &*signature.kind),
                ("header.signature", "error")
            );
            assert!(signature.value.is_none());
            assert_eq!(signature.diagnostics[0].code, "template_expectation_failed");
            assert_eq!(s.results[0].diagnostics[0].code, "child_error");
        } else {
            assert!(s.template_applied);
            assert_eq!(s.results.len(), 3);
            let flags = &s.results[0].children[2];
            assert_eq!(flags.enum_label.as_deref(), Some("Enabled"));
            assert_eq!(flags.flags, ["Read", "Execute"]);
            assert_eq!(s.results[1].children.len(), 4);
            let leaf = &s.results[1].children[2].children[0];
            assert_eq!(
                (
                    &*leaf.path,
                    leaf.offset.as_deref(),
                    leaf.length.as_deref(),
                    leaf.value.as_deref()
                ),
                (
                    "entries[2].id",
                    Some("32"),
                    Some("8"),
                    Some("2820983053732684064")
                )
            );
            assert_eq!(s.results[2].value.as_deref(), Some("@ABCDEFGHIJKLMNO"));
            let selection = s.selection.unwrap();
            if scene == "selected-field" {
                assert_eq!((selection.start, selection.end), (32, 39));
                assert_eq!(s.template_range.unwrap().start, 32);
            } else {
                assert_eq!((selection.start, selection.end), (128, 128));
                assert!(
                    s.template_range.is_none(),
                    "manual selection must clear only the template-owned marker"
                );
            }
        }
    }
}

#[test]
fn editor_capture_scenes_use_loaded_checkpoints_and_actual_dirty_close_states() {
    use hexforge_native::app::NativeApp;
    for scene in [
        "editor-advanced",
        "editor-dirty",
        "editor-invalid",
        "editor-close",
    ] {
        let spec = CaptureSpec::new(scene, "light", [660, 760], 1.0).unwrap();
        let mut app = NativeApp::new(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let s = app.editor_snapshot();
        let local = app.editor.as_ref().expect("real separate editor session");
        assert!(s.template_state.active);
        assert!(s.template_state.saved_path.is_some());
        assert_eq!(local.model.definition().fields.len(), 3);
        assert!(s.results.is_empty(), "loading and editing must not apply");
        assert_eq!(
            s.template_state.checkpoint_definition.name,
            "Structured preview"
        );
        assert_eq!(s.close_prompt, scene == "editor-close");
        if scene == "editor-advanced" {
            assert!(!s.template_state.dirty && local.model.valid());
            assert_eq!(local.model.name, "Structured preview");
        } else if scene == "editor-invalid" {
            assert!(s.template_state.dirty && !local.model.valid());
            assert_eq!(local.model.name, "");
        } else {
            assert!(s.template_state.dirty && local.model.valid());
            assert_eq!(local.model.name, "Edited preview");
        }
    }
}

#[test]
fn required_visual_states_are_real_declared_application_scenes() {
    for scene in [
        "empty",
        "open",
        "open-32",
        "empty-file",
        "edited",
        "search",
        "goto",
        "file-menu",
        "edit-menu",
        "navigate-menu",
        "template-menu",
        "view-menu",
        "minimap-menu",
        "results-menu",
        "busy",
        "error",
        "pending-template",
        "fit-top",
        "fit-middle",
        "fit-eof",
        "proportional-top",
        "proportional-middle",
        "proportional-eof",
        "selected-field",
        "cleared-field",
        "results-diagnostics",
        "results-tree-focus",
        "editor-empty",
        "editor-advanced",
        "editor-invalid",
        "editor-dirty",
        "editor-close",
        "editor-save-error",
        "binary-close",
    ] {
        assert!(
            CaptureSpec::new(scene, "dark", [1280, 800], 1.0).is_ok(),
            "missing actual application state {scene}"
        );
    }
}

#[test]
fn failed_pending_and_close_scenes_keep_their_actual_controller_guards() {
    use hexforge_native::app::NativeApp;
    for scene in ["busy", "error", "binary-close", "editor-save-error"] {
        let spec = CaptureSpec::new(scene, "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let s = app.main_snapshot();
        match scene {
            "busy" => {
                assert!(s.policy.operation_busy);
                assert!(app.picker.is_some() && s.file.is_none());
                assert!(
                    s.activity.is_none(),
                    "a pending file picker is not a fabricated progress scan"
                );
            }
            "error" => assert_eq!(s.error.unwrap().message, "Drop exactly one file at a time."),
            "binary-close" => {
                let (_, dialog) = s.dialog.unwrap();
                assert_eq!(dialog.title, "Unsaved byte edits");
                assert_eq!(dialog.message, "Save unsaved byte edits to \"parity.bin\" before closing this binary? The original binary will not be overwritten.");
                assert!(s.file.unwrap().dirty && s.can_undo && s.edit_mode);
                assert_eq!(s.selection.unwrap().start, 128);
            }
            "editor-save-error" => {
                let e = app.editor.as_ref().unwrap();
                assert_eq!(e.error.as_deref(), Some("Could not save the template."));
                assert_eq!(e.model.name, "Edited preview");
                assert!(s.template_state.dirty && !s.policy.operation_busy);
                assert!(s
                    .template_state
                    .saved_path
                    .unwrap()
                    .ends_with("structures.json"));
                assert!(app.editor_snapshot().close_prompt);
                assert!(!app.windows.editor_closing());
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn capture_inputs_open_real_flyouts_and_move_real_result_focus_without_activation() {
    use eframe::egui;
    use hexforge_native::app::NativeApp;
    for scene in ["minimap-menu", "results-menu", "results-tree-focus"] {
        let spec = CaptureSpec::new(scene, "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        app.attach(&ctx);
        let mut last = vec![];
        let mut focus_rect = None;
        let mut stroked_rects = vec![];
        let mut focused_identity = None;
        for frame in 0..16 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    events: capture_scene::frame_input(&spec, frame),
                    ..Default::default()
                },
                |ui| {
                    app.paint_main(ui);
                    focused_identity = ctx.memory(|m| m.focused()).and_then(|id| {
                        ctx.accesskit_node_builder(id, |n| (n.role(), n.value().map(str::to_owned)))
                    });
                },
            );
            last = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.job.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            focus_rect = output.shapes.iter().find_map(|s| match &s.shape {
                egui::epaint::Shape::Rect(r)
                    if r.rect.min.y == 669.0
                        && r.rect.height() == 28.0
                        && r.stroke.width == 1.0 =>
                {
                    Some(r.rect)
                }
                _ => None,
            });
            stroked_rects = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::epaint::Shape::Rect(r)
                        if r.stroke.width > 0.0 && r.rect.min.y > 550.0 =>
                    {
                        Some((r.rect, r.stroke))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            output.textures_delta.clear();
        }
        match scene {
            "minimap-menu" => {
                for text in ["Show Minimap", "Fit File", "Proportional", "Scale 3×"] {
                    assert!(
                        last.iter().any(|s| s == text),
                        "actual minimap flyout did not render {text}"
                    );
                }
                assert!(ctx.memory(|m| m.focused().is_some()));
            }
            "results-menu" => {
                assert!(last.iter().any(|s| s == "header.signature"));
                assert!(last.iter().any(|s| s == "entries[2].id"));
                assert!(
                    app.main_snapshot().selection.is_none(),
                    "opening a flyout must not navigate its focused leaf"
                );
            }
            "results-tree-focus" => {
                let snapshot = app.main_snapshot();
                let range = snapshot
                    .template_range
                    .expect("actual signature click navigated");
                assert_eq!((range.start, range.end), (0, 3));
                // ArrowDown moves keyboard focus, not the selected byte range.
                // 556px panel top + 1px border + two 28px headers +
                // two preceding 28px rows places entryCount at 669px.
                assert!(focus_rect.is_some(), "the real entryCount row must have its focus outline at 669px: {stroked_rects:?}");
                assert!(ctx.memory(|m| m.focused().is_some()));
                assert_eq!(focused_identity, Some((egui::accesskit::Role::Tree, Some("header.entryCount".into()))),
                    "assistive clients must observe the actual focused result, not only the generic panel label");
            }
            _ => unreachable!(),
        }
    }
}
