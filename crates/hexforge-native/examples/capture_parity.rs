//! Lossless GL framebuffer evidence from the production presenters. This is a
//! development-only example, not a packaged mode, splash, or startup signal.
#[path = "../tests/support/capture_scene.rs"]
mod capture_scene;
#[path = "../tests/support/capture_spec.rs"]
mod capture_spec;
use capture_spec::CaptureSpec;
use eframe::egui;
use hexforge_controller::controller::{InitialState, Theme};
use hexforge_native::{app::NativeApp, style::palette::Palette};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct Capture {
    spec: CaptureSpec,
    output: PathBuf,
    app: NativeApp,
    frames: usize,
    requested: bool,
    scene_navigated: bool,
    started: Instant,
    result: Arc<Mutex<Option<Result<(), String>>>>,
}
fn observed_focus(ctx: &egui::Context) -> Option<String> {
    let focused = ctx.memory(|memory| memory.focused())?;
    // The reference records the actual focused input's DOM ID. These are the
    // same persistent control identities, not a requested fixture focus.
    for name in ["search-bytes", "goto-offset", "template-name"] {
        if focused == egui::Id::new(name) && ctx.read_response(focused).is_some() {
            return Some(format!("#{name}"));
        }
    }
    ctx.accesskit_node_builder(focused, |node| {
        if node.role() == egui::accesskit::Role::Tree {
            node.value().map(|path| format!("result:{path}"))
        } else {
            node.label().or_else(|| node.value()).map(str::to_owned)
        }
    })
    .flatten()
}
impl eframe::App for Capture {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        // Controlled fixture input, like the isolated source browser's trusted
        // keyboard event. This never activates or changes another desktop app.
        // Real product focus-loss behavior is covered separately by UI tests.
        input.focused = true;
        if let Some(viewport) = input.viewports.get_mut(&input.viewport_id) {
            viewport.focused = Some(true);
        }
        // This hidden fixture must not receive incidental desktop input while
        // the person uses another application. Keep framebuffer replies and
        // real viewport/resize/close metadata; only the scripted scene may
        // supply interaction events. Production windows do not use this hook.
        input
            .events
            .retain(|e| matches!(e, egui::Event::Screenshot { .. }));
        input
            .events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        input.dropped_files.clear();
        input.hovered_files.clear();
        // Pointer state is derived at begin_pass; adding events in draw() is
        // too late to exercise hit testing, focus, capture, or real activation.
        input
            .events
            .extend(capture_scene::frame_input(&self.spec, self.frames));
    }
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.pump();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Palette::new(if self.spec.theme == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        })
        .bg
        .to_normalized_gamma_f32()
    }
}
impl Capture {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.result.lock().unwrap().is_some() {
            // A completed framebuffer is terminal. The editor body is hosted
            // at this fixture's root; invoking its real close coordinator again
            // would cancel root Close and later overwrite success with timeout.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let native_dpr = ui.input(|i| i.viewport().native_pixels_per_point.unwrap_or(1.0));
        let desired_dpr = self.spec.dpr as f32;
        if (ctx.pixels_per_point() - desired_dpr).abs() > 0.00001 {
            ctx.set_zoom_factor(desired_dpr / native_dpr);
        }
        let physical_size = self.spec.physical_size();
        // ViewportCommand sizes are egui points and already use its zoom factor.
        // Dividing by the native monitor DPR a second time shrinks the window.
        let logical_size = egui::vec2(
            self.spec.client_size[0] as f32,
            self.spec.client_size[1] as f32,
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(logical_size));
        ui.input_mut(|i| {
            // Controlled scene inputs, not changes to Windows theme or DPI.
            i.raw.system_theme = Some(if self.spec.theme == "dark" {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
            i.events.push(egui::Event::PointerGone);
        });
        self.app.pump();
        if !self.scene_navigated
            && self.frames >= 12
            && ui.max_rect().size() == logical_size
            && (ctx.pixels_per_point() - desired_dpr).abs() <= 0.00001
        {
            // The reference delivers its navigation prop after actual client
            // measurement. Do the same after the OS/DPI resize, rather than
            // preserving an earlier clamp from an initially taller viewport.
            if let Some(row) = self.spec.config().navigate_row {
                self.app
                    .gesture(hexforge_controller::action::Action::ScrollViewport {
                        offset: row
                            .checked_mul(u64::from(self.spec.config().bytes_per_row.unwrap_or(16)))
                            .expect("bounded scene row"),
                    });
            }
            self.scene_navigated = true;
        }
        if self.spec.config().surface == "editor" {
            self.app.paint_editor(ui);
        } else {
            self.app.paint_main(ui);
        }
        self.frames += 1;
        let screenshots = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        if let Some(image) = screenshots.first() {
            let actual = image.size.map(|n| u32::try_from(n).unwrap_or(u32::MAX));
            let result = (|| {
                if !self
                    .spec
                    .accepts_frame(actual, f64::from(ctx.pixels_per_point()))
                {
                    return Err(format!("Framebuffer {:?} at DPR {} differs from {:?} at DPR {}; no rescaling is permitted", actual, ctx.pixels_per_point(), physical_size, self.spec.dpr));
                }
                let theme = if self.spec.theme == "dark" {
                    Theme::Dark
                } else {
                    Theme::Light
                };
                if self.app.main_snapshot().theme != theme {
                    return Err("Actual theme changed from the controlled capture scene".into());
                }
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    &self.output,
                    &rgba,
                    actual[0],
                    actual[1],
                    image::ColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
                let snapshot = self.app.main_snapshot();
                let metadata = serde_json::json!({
                    "scene": self.spec.scene, "theme": self.spec.theme,
                    "clientSize": self.spec.client_size, "physicalSize": actual,
                    "dpr": self.spec.dpr, "dpi": self.spec.dpr * 96.0,
                    "evidence": "lossless", "capture": "eframe GL framebuffer",
                    "controlledWindowFocus": true,
                    "focusedWidgetId": ctx.memory(|m| m.focused()).map(|id| format!("{id:?}")),
                    "focusLabel": observed_focus(&ctx),
                    "state": { "file": snapshot.file,
                        "completionNotice": snapshot.completion_notice,
                        "viewportOffset": snapshot.viewport_offset.to_string(),
                        "selection": snapshot.selection.as_ref().map(|s| [s.start.to_string(), s.end.to_string()]),
                        "mode": match snapshot.minimap.mode {
                            hexforge_controller::controller::MinimapMode::Fit => "fit",
                            hexforge_controller::controller::MinimapMode::Proportional => "proportional"
                        }, "bytesPerRow": snapshot.row_width.bytes(),
                        "template": { "definition": snapshot.template,
                            "path": snapshot.template_state.saved_path,
                            "active": snapshot.template_state.active,
                            "applied": snapshot.template_applied,
                            "dirty": snapshot.template_state.dirty,
                            "valid": snapshot.template_state.valid,
                            "range": snapshot.template_range.map(|r| [r.start.to_string(), r.end.to_string()]) },
                        "results": snapshot.results },
                    "productionPresenter": if self.spec.config().surface == "editor" { "NativeApp::paint_editor" } else { "NativeApp::paint_main" },
                    "verified": false, "note": "A framebuffer is not a passed paired visual gate or a compositor startup timestamp"
                });
                std::fs::write(
                    self.output.with_extension("json"),
                    serde_json::to_vec_pretty(&metadata).unwrap(),
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })();
            *self.result.lock().unwrap() = Some(result);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.started.elapsed() > Duration::from_secs(15) {
            *self.result.lock().unwrap() =
                Some(Err("No actual framebuffer screenshot received".into()));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Allow real font atlas uploads, resizing and the actual scene to
            // settle. The paired reference scenes represent the steady state
            // after transient notices expire. Let the production clock remove
            // those notices; never mutate a snapshot to conceal a difference.
            // This is visual QA, never a first-paint/startup measurement.
            if self.frames >= 32
                && !self.requested
                && self.app.main_snapshot().completion_notice.is_none()
                && capture_scene::frame_ready(
                    &self.spec,
                    &self.app.main_snapshot(),
                    self.app.hex_frame(),
                )
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.requested = true;
            }
            ctx.request_repaint();
        }
    }
}
fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 6 {
        return Err("Usage: capture_parity SCENE dark|light WIDTH HEIGHT DPR OUTPUT.png".into());
    }
    let spec = CaptureSpec::new(
        &args[0],
        &args[1],
        [
            args[2].parse().map_err(|_| "Invalid width")?,
            args[3].parse().map_err(|_| "Invalid height")?,
        ],
        args[4].parse().map_err(|_| "Invalid DPR")?,
    )?;
    let output = PathBuf::from(&args[5]);
    if output.extension().is_none_or(|ext| ext != "png") {
        return Err("Lossless PNG output required".into());
    }
    let result = Arc::new(Mutex::new(None));
    let completion = result.clone();
    let builder = egui::ViewportBuilder::default()
        .with_title("HexForge parity framebuffer fixture")
        .with_inner_size(egui::vec2(
            spec.client_size[0] as f32,
            spec.client_size[1] as f32,
        ))
        .with_resizable(false);
    eframe::run_native(
        "HexForge parity framebuffer fixture",
        hexforge_native::platform::windows::native_options(builder),
        Box::new(move |cc| {
            cc.egui_ctx.enable_accesskit();
            let mut app = NativeApp::new(
                InitialState {
                    theme: if spec.theme == "dark" {
                        Theme::Dark
                    } else {
                        Theme::Light
                    },
                    ..Default::default()
                },
                Arc::new(hexforge_core::operations::CoreService::default()),
            );
            app.attach(&cc.egui_ctx);
            capture_scene::prepare_scene(&mut app, &spec).map_err(std::io::Error::other)?;
            Ok(Box::new(Capture {
                spec,
                output,
                app,
                frames: 0,
                requested: false,
                scene_navigated: false,
                started: Instant::now(),
                result: completion,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    let captured = result
        .lock()
        .unwrap()
        .take()
        .unwrap_or_else(|| Err("Capture fixture closed without framebuffer evidence".into()));
    captured
}

#[cfg(test)]
mod tests {
    use super::*;
    use hexforge_controller::command::Command;

    // The injected image tests the capture writer's observation contract only.
    // It is never retained or submitted as real visual/presentation evidence.
    fn written_metadata(scene: &str, modify: impl FnOnce(&mut NativeApp)) -> serde_json::Value {
        let directory = tempfile::tempdir().unwrap();
        let output_path = directory.path().join("metadata-contract.png");
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = NativeApp::new(
            InitialState::default(),
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&ctx);
        let spec = CaptureSpec::new(scene, "dark", [1280, 800], 1.0).unwrap();
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        modify(&mut app);
        let result = Arc::new(Mutex::new(None));
        let mut capture = Capture {
            spec,
            output: output_path.clone(),
            app,
            frames: 0,
            requested: false,
            scene_navigated: false,
            started: Instant::now(),
            result: result.clone(),
        };
        for frame in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            };
            if frame == 2 {
                input.events.push(egui::Event::Screenshot {
                    viewport_id: egui::ViewportId::ROOT,
                    user_data: egui::UserData::default(),
                    image: Arc::new(egui::ColorImage::filled([1280, 800], egui::Color32::BLACK)),
                });
            }
            eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| capture.draw(ui));
            output.textures_delta.clear();
        }
        assert_eq!(result.lock().unwrap().as_ref().unwrap(), &Ok(()));
        serde_json::from_slice(&std::fs::read(output_path.with_extension("json")).unwrap()).unwrap()
    }

    #[test]
    fn capture_metadata_observes_actual_row_width_and_mode_not_scene_defaults() {
        let metadata = written_metadata("open-32", |app| {
            app.invoke(Command::MinimapProportional, Default::default());
        });
        assert_eq!(metadata["state"]["bytesPerRow"], 32);
        assert_eq!(metadata["state"]["mode"], "proportional");
        assert_eq!(metadata["focusLabel"], serde_json::Value::Null);
        assert!(metadata.as_object().unwrap().contains_key("focusLabel"));
    }

    #[test]
    fn capture_focus_identity_comes_from_the_current_real_input() {
        let metadata = written_metadata("search", |_| {});
        assert_eq!(metadata["focusLabel"], "#search-bytes");
        assert!(metadata["focusedWidgetId"].is_string());
    }

    #[test]
    fn capture_metadata_records_the_actual_transient_notice() {
        let metadata = written_metadata("results-diagnostics", |_| {});
        assert_eq!(
            metadata["state"]["completionNotice"],
            "Parsing completed with field errors. Inspect parsed results."
        );
    }

    #[test]
    fn capture_waits_for_the_real_notice_expiry_before_requesting_a_framebuffer() {
        let ctx = egui::Context::default();
        let spec = CaptureSpec::new("results-diagnostics", "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            InitialState::default(),
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&ctx);
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let mut capture = Capture {
            spec,
            output: PathBuf::from("unused-settled-notice-contract.png"),
            app,
            frames: 32,
            requested: false,
            scene_navigated: true,
            started: Instant::now(),
            result: Arc::new(Mutex::new(None)),
        };
        // Drive the real presentation clock, not an ExpireNotice action or a
        // modified snapshot. These are scene-settling times, not startup data.
        for (time, want_capture) in [(0.0, false), (4.999, false), (5.0, true)] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| capture.draw(ui),
            );
            let screenshot_requested = output.viewport_output.values().any(|viewport| {
                viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Screenshot(_)))
            });
            output.textures_delta.clear();
            assert_eq!(
                screenshot_requested, want_capture,
                "The settled scene must retain the production five-second notice at {time}s"
            );
            assert_eq!(
                capture.app.main_snapshot().completion_notice.is_none(),
                want_capture
            );
        }
    }

    #[test]
    fn capture_pointer_input_is_processed_before_the_actual_presenter_frame() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let spec = CaptureSpec::new("results-tree-focus", "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            InitialState::default(),
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&ctx);
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let mut capture = Capture {
            spec,
            output: PathBuf::from("unused-pointer-contract.png"),
            app,
            frames: 0,
            requested: false,
            scene_navigated: false,
            started: Instant::now(),
            result: Arc::new(Mutex::new(None)),
        };
        let mut focus = None;
        for _ in 0..16 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| {
                capture.draw(ui);
                focus = observed_focus(&ctx);
            });
            output.textures_delta.clear();
        }
        let range = capture
            .app
            .main_snapshot()
            .template_range
            .expect("GPU fixture must process the actual signature click");
        assert_eq!((range.start, range.end), (0, 3));
        assert_eq!(focus.as_deref(), Some("result:header.entryCount"));
    }

    #[test]
    fn capture_ignores_external_shortcuts_without_changing_its_scene() {
        let ctx = egui::Context::default();
        let spec = CaptureSpec::new("cleared-field", "light", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            InitialState {
                theme: Theme::Light,
                ..Default::default()
            },
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&ctx);
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let mut capture = Capture {
            spec,
            output: PathBuf::from("unused-external-input-contract.png"),
            app,
            frames: 0,
            requested: false,
            scene_navigated: false,
            started: Instant::now(),
            result: Arc::new(Mutex::new(None)),
        };
        for _ in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| capture.draw(ui));
            output.textures_delta.clear();
        }
        // These are not the scene's scripted inputs. An incidental desktop
        // shortcut must not change the controlled search/focus/selection.
        let pos = egui::pos2(404.0, 249.0);
        for pressed in [None, Some(true), Some(false)] {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events: vec![egui::Event::PointerMoved(pos)],
                ..Default::default()
            };
            if let Some(pressed) = pressed {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| capture.draw(ui));
            output.textures_delta.clear();
        }
        let snapshot = capture.app.main_snapshot();
        assert_eq!(
            snapshot.selection.as_ref().map(|s| (s.start, s.end)),
            Some((128, 128))
        );
        assert!(snapshot.template_range.is_none());
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
        let mut output = ctx.run_ui(input, |ui| capture.draw(ui));
        output.textures_delta.clear();
        assert!(
            !capture.app.main_snapshot().search.open,
            "Incidental desktop shortcuts may not mutate an isolated fixture"
        );
    }

    #[test]
    fn eof_capture_navigation_runs_after_client_resize_just_like_the_reference() {
        let ctx = egui::Context::default();
        let spec = CaptureSpec::new("fit-eof", "dark", [1280, 800], 1.0).unwrap();
        let mut app = NativeApp::new(
            InitialState::default(),
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&ctx);
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let mut capture = Capture {
            spec,
            output: PathBuf::from("unused-eof-contract.png"),
            app,
            frames: 0,
            requested: false,
            scene_navigated: false,
            started: Instant::now(),
            result: Arc::new(Mutex::new(None)),
        };
        for frame in 0..16 {
            // The OS/DPI resize settles after initial frames. Navigating before
            // that can legitimately clamp to an earlier page in the taller view.
            let height = if frame < 2 { 1400.0 } else { 800.0 };
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, height),
                )),
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
            let mut output = ctx.run_ui(input, |ui| capture.draw(ui));
            output.textures_delta.clear();
        }
        // 476px hex viewport minus 32px header gives 20 fully visible rows.
        assert_eq!(capture.app.main_snapshot().viewport_offset, 15_680);
    }

    #[test]
    fn controlled_capture_preserves_keyboard_focus_without_dismissing_close_events() {
        let ctx = egui::Context::default();
        let app = NativeApp::new(
            InitialState::default(),
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        let mut capture = Capture {
            spec: CaptureSpec::new("file-menu", "dark", [1280, 800], 1.0).unwrap(),
            output: PathBuf::from("unused-controlled-input.png"),
            app,
            frames: 0,
            requested: false,
            scene_navigated: false,
            started: Instant::now(),
            result: Arc::new(Mutex::new(None)),
        };
        let mut input = egui::RawInput {
            focused: false,
            events: vec![egui::Event::WindowFocused(false)],
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        eframe::App::raw_input_hook(&mut capture, &ctx, &mut input);
        assert!(
            input.focused,
            "A controlled keyboard-open scene must not depend on which other app is foreground"
        );
        assert!(!input
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::WindowFocused(false))));
        assert!(
            input.viewports[&egui::ViewportId::ROOT].close_requested(),
            "Actual fixture close still works"
        );
    }
    #[test]
    fn completed_capture_cannot_be_replaced_by_editor_close_or_timeout() {
        let ctx = egui::Context::default();
        let mut app = NativeApp::new(
            InitialState::default(),
            Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&ctx);
        app.invoke(Command::TemplateEditor, Default::default());
        let result = Arc::new(Mutex::new(Some(Ok(()))));
        let mut capture = Capture {
            spec: CaptureSpec::new("editor-empty", "dark", [660, 760], 1.0).unwrap(),
            output: PathBuf::from("unused-after-completion.png"),
            app,
            frames: 10,
            requested: true,
            scene_navigated: true,
            started: Instant::now() - Duration::from_secs(20),
            result: result.clone(),
        };
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
        let mut output = ctx.run_ui(input, |ui| capture.draw(ui));
        // No GPU consumes this lifecycle-only regression's deltas. Release
        // them before assertions, including failing-test unwind paths.
        output.textures_delta.clear();
        assert_eq!(
            result.lock().unwrap().as_ref().unwrap(),
            &Ok(()),
            "A saved framebuffer is terminal, not a later timeout"
        );
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close)));
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose)),
            "Do not invoke real editor close protection after the fixture has completed"
        );
        assert!(!capture.app.windows.editor_closing());
    }
}
