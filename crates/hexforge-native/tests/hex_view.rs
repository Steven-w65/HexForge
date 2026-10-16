use eframe::egui::{self, Event, PointerButton, Pos2};
use hexforge_controller::{
    action::Action,
    controller::{
        AppController, ControllerEvent, Effect, InitialState, MainSnapshot, MinimapMode,
        WindowSnapshot,
    },
    shortcuts::WindowKind,
};
use hexforge_core::operations::{CoreRequest, CoreService};
use hexforge_native::{drawing::cache::HexDrawCache, views::hex_view::HexView};
use std::io::Write;

fn settle(controller: &mut AppController, core: &CoreService, events: Vec<ControllerEvent>) {
    let mut effects = std::collections::VecDeque::new();
    for event in events {
        effects.extend(controller.handle(event));
    }
    while let Some(effect) = effects.pop_front() {
        if let Effect::RunCore { stamp, request } = effect {
            let reply = core.execute(request, &mut |_| {});
            effects.extend(controller.handle(ControllerEvent::CoreCompleted { stamp, reply }));
        }
    }
}
fn snapshot(c: &AppController) -> MainSnapshot {
    match c.snapshot(WindowKind::Main) {
        WindowSnapshot::Main(s) => s,
        _ => unreachable!(),
    }
}
fn setup(size: usize) -> (tempfile::NamedTempFile, CoreService, AppController) {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&vec![65; size]).unwrap();
    file.flush().unwrap();
    let core = CoreService::default();
    let mut state = InitialState::default();
    state.minimap.mode = MinimapMode::Proportional;
    let mut c = AppController::new(state);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: file.path().into(),
                discard_unsaved: false,
            },
        ))],
    );
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ReadViewport {
            offset: 0,
            length: 4096,
        })],
    );
    (file, core, c)
}
struct Host {
    ctx: egui::Context,
    view: HexView,
    cache: HexDrawCache,
    height: f32,
}
impl Host {
    fn new() -> Self {
        let ctx = egui::Context::default();
        hexforge_native::style::fonts::install(&ctx);
        Self {
            ctx,
            view: HexView::default(),
            cache: HexDrawCache::default(),
            height: 252.0,
        }
    }
    fn frame(&mut self, c: &mut AppController, core: &CoreService, events: Vec<Event>, dpr: f32) {
        self.frame_at(c, core, events, dpr, true, Pos2::ZERO);
    }
    fn frame_at(
        &mut self,
        c: &mut AppController,
        core: &CoreService,
        events: Vec<Event>,
        dpr: f32,
        focused: bool,
        desktop: Pos2,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, self.height),
            )),
            events,
            focused,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(dpr);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .inner_rect = Some(egui::Rect::from_min_size(
            desktop,
            egui::vec2(1280.0, self.height),
        ));
        let s = snapshot(c);
        let mut actions = vec![];
        let mut output = self.ctx.run_ui(input, |ui| {
            actions.extend(self.view.show(ui, &s, &mut self.cache));
        });
        output.textures_delta.clear();
        settle(c, core, actions);
    }
    fn pointer(
        &mut self,
        c: &mut AppController,
        core: &CoreService,
        p: Pos2,
        pressed: Option<bool>,
    ) {
        let mut e = vec![Event::PointerMoved(p)];
        if let Some(pressed) = pressed {
            e.push(Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        self.frame(c, core, e, 1.0);
    }
}
#[test]
fn drag_box_top_to_bottom_moves_actual_viewer() {
    let (_file, core, mut c) = setup(16000);
    let mut host = Host::new();
    host.frame(&mut c, &core, vec![], 1.0);
    let initial = host.view.frame().clone();
    let box_rect = initial.viewport_box.unwrap();
    let mini = initial.minimap_rect.unwrap();
    let grab = 3.7;
    let p = egui::pos2(box_rect.center().x, box_rect.top() + grab);
    host.pointer(&mut c, &core, p, Some(true));
    host.pointer(
        &mut c,
        &core,
        egui::pos2(p.x, mini.top() + 232.0 / 2.0 + grab as f32),
        None,
    );
    assert_eq!(snapshot(&c).viewport_offset / 16, 495);
    assert!((host.view.frame().viewport_box.unwrap().top() - mini.top() - 116.0).abs() < 0.001);
    host.pointer(
        &mut c,
        &core,
        egui::pos2(p.x, mini.top() + 232.0 + grab as f32),
        None,
    );
    assert_eq!(snapshot(&c).viewport_offset / 16, 990);
    host.frame(&mut c, &core, vec![], 1.0);
    let f = host.view.frame();
    assert_eq!(f.top_row, 990);
    assert_eq!(f.last_visible_row, Some(999));
    assert_eq!(
        f.minimap_projection.as_ref().unwrap().virtual_origin.0,
        1748 * 1024
    );
    assert!((f.viewport_box.unwrap().top() - mini.top() - 232.0).abs() < 0.001);
    assert!(host.cache.minimap.image().size[1] <= 252);
}
#[test]
fn upward_drag_and_outside_capture_clamp_without_losing_grab_offset() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let b = h.view.frame().viewport_box.unwrap();
    let mini = h.view.frame().minimap_rect.unwrap();
    let p = egui::pos2(b.center().x, b.top() + 17.0);
    h.pointer(&mut c, &core, p, Some(true));
    h.pointer(&mut c, &core, egui::pos2(p.x, mini.bottom() + 50.0), None);
    assert_eq!(snapshot(&c).viewport_offset, 15840);
    h.pointer(&mut c, &core, egui::pos2(p.x, mini.top() + 133.0), None);
    assert_eq!(snapshot(&c).viewport_offset / 16, 495);
    h.pointer(&mut c, &core, egui::pos2(p.x, mini.top() - 50.0), None);
    assert_eq!(snapshot(&c).viewport_offset, 0);
    h.pointer(&mut c, &core, p, Some(false));
    assert!(!h.view.minimap_captured());
}
#[test]
fn minimap_wheel_is_routed_once_and_preview_click_never_starts_drag() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let mini = h.view.frame().minimap_rect.unwrap();
    let p = egui::pos2(mini.center().x, mini.top() + 30.0);
    h.pointer(&mut c, &core, p, Some(true));
    assert_eq!(snapshot(&c).viewport_offset / 16, 15);
    assert!(!h.view.minimap_captured());
    h.pointer(&mut c, &core, p, Some(false));
    h.frame(
        &mut c,
        &core,
        vec![
            Event::PointerMoved(p),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: Default::default(),
            },
        ],
        1.0,
    );
    assert_eq!(snapshot(&c).viewport_offset / 16, 16);
    h.frame(
        &mut c,
        &core,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 120.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        }],
        1.0,
    );
    assert_eq!(snapshot(&c).viewport_offset / 16, 15);
}
#[test]
fn dpi_change_retains_continuous_drag_and_sharp_backing_dimensions() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let b = h.view.frame().viewport_box.unwrap();
    let p = egui::pos2(b.center().x, b.top() + 4.0);
    h.pointer(&mut c, &core, p, Some(true));
    let q = egui::pos2(p.x, b.top() + 84.0);
    h.pointer(&mut c, &core, q, None);
    let top = h.view.frame().viewport_box.unwrap().top();
    h.frame(&mut c, &core, vec![], 1.25);
    assert!(h.view.minimap_captured());
    assert_eq!(h.view.frame().viewport_box.unwrap().top(), top);
    assert_eq!(h.cache.minimap.image().size, [105, 315]);
}
#[test]
fn dpi_change_rebases_active_drag() {
    for dpi in [1.0, 1.25, 1.5, 2.0] {
        let (_file, core, mut c) = setup(16000);
        let mut h = Host::new();
        h.frame(&mut c, &core, vec![], 1.0);
        let b = h.view.frame().viewport_box.unwrap();
        let start = egui::pos2(b.center().x, b.top() + 4.0);
        h.pointer(&mut c, &core, start, Some(true));
        let held = egui::pos2(start.x, b.top() + 84.0);
        h.pointer(&mut c, &core, held, None);
        assert_eq!(snapshot(&c).viewport_offset / 16, 341);
        let old_projection = h.view.frame().minimap_projection.clone().unwrap();
        h.view.retry_minimap(); // A sample retry must not reset pointer geometry.
                                // The physical pointer did not move, but its local logical coordinate
                                // changed when the window moved to a monitor with a different DPI.
        let remapped = egui::pos2(held.x / dpi, held.y / dpi);
        h.frame_at(
            &mut c,
            &core,
            vec![Event::PointerMoved(remapped)],
            dpi,
            true,
            egui::pos2(-1920.0, -400.0),
        );
        assert!(h.view.minimap_captured());
        assert_eq!(
            snapshot(&c).viewport_offset / 16,
            341,
            "DPI {dpi} must not navigate without physical drag movement"
        );
        assert_eq!(
            h.view.frame().minimap_projection.as_ref().unwrap(),
            &old_projection
        );
        // One tenth of the 232px box travel advances one tenth of the 990-row
        // editor range. The rebase must retain continuous local coordinates.
        h.frame_at(
            &mut c,
            &core,
            vec![Event::PointerMoved(remapped + egui::vec2(0.0, 23.2))],
            dpi,
            true,
            egui::pos2(-1920.0, -400.0),
        );
        assert_eq!(snapshot(&c).viewport_offset / 16, 440);
        assert!((h.view.frame().viewport_box.unwrap().top() - b.top() - 103.2).abs() < 0.001);
    }
}
#[test]
fn resize_and_dpi_change_preserve_a_held_minimap_drag() {
    for mode in [MinimapMode::Proportional, MinimapMode::Fit] {
        let (_file, core, mut c) = setup(16000);
        c.handle(ControllerEvent::Invoke {
            command: if mode == MinimapMode::Fit {
                hexforge_controller::command::Command::MinimapFit
            } else {
                hexforge_controller::command::Command::MinimapProportional
            },
            context: Default::default(),
        });
        let mut h = Host::new();
        h.frame(&mut c, &core, vec![], 1.0);
        let b = h.view.frame().viewport_box.unwrap();
        let start = egui::pos2(b.center().x, b.top() + b.height() * 0.3);
        h.pointer(&mut c, &core, start, Some(true));
        let held = start + egui::vec2(0.0, 80.0);
        h.pointer(&mut c, &core, held, None);
        let held_row = snapshot(&c).viewport_offset / 16;

        // A real monitor transition can change logical dimensions and DPI in
        // the same input frame. A layout refresh is not a new gesture.
        h.height = 362.0;
        let remapped = egui::pos2(held.x, held.y / 1.5);
        h.frame_at(
            &mut c,
            &core,
            vec![Event::PointerMoved(remapped)],
            1.5,
            true,
            egui::pos2(-1920.0, -400.0),
        );
        assert!(
            h.view.minimap_captured(),
            "{mode:?} lost its held pointer on resize"
        );
        assert_eq!(
            snapshot(&c).viewport_offset / 16,
            held_row,
            "Layout changes alone must not navigate"
        );
        h.frame_at(
            &mut c,
            &core,
            vec![Event::PointerMoved(remapped + egui::vec2(0.0, 33.2))],
            1.5,
            true,
            egui::pos2(-1920.0, -400.0),
        );
        assert!(
            snapshot(&c).viewport_offset / 16 > held_row,
            "Dragging must continue after resize"
        );
        assert!(h.view.minimap_captured());
        if mode == MinimapMode::Proportional {
            h.frame_at(
                &mut c,
                &core,
                vec![Event::PointerMoved(egui::pos2(remapped.x, 500.0))],
                1.5,
                true,
                egui::Pos2::ZERO,
            );
            assert_eq!(snapshot(&c).viewport_offset / 16, 985);
            h.frame_at(&mut c, &core, vec![], 1.5, true, egui::Pos2::ZERO);
            assert_eq!(h.view.frame().last_visible_row, Some(999));
        }
    }
}
#[test]
fn focus_loss_cancels_capture_without_clearing_selection() {
    let (_file, core, mut c) = setup(16000);
    c.handle(ControllerEvent::Action(Action::Select {
        start: 100,
        end: 103,
    }));
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let b = h.view.frame().viewport_box.unwrap();
    let p = egui::pos2(b.center().x, b.top() + 4.0);
    h.pointer(&mut c, &core, p, Some(true));
    assert!(h.view.minimap_captured());
    h.frame_at(
        &mut c,
        &core,
        vec![],
        1.0,
        false,
        egui::pos2(-1920.0, -400.0),
    );
    assert!(
        !h.view.minimap_captured(),
        "losing the window must cancel the held drag without waiting for a missing mouse-up"
    );
    // Window backends can repeat a lost-focus notification after capture was
    // already retired. Native capture is local state, not a second OS release.
    h.frame_at(
        &mut c,
        &core,
        vec![],
        1.0,
        false,
        egui::pos2(-1920.0, -400.0),
    );
    assert!(!h.view.minimap_captured());
    h.frame(
        &mut c,
        &core,
        vec![Event::PointerMoved(p + egui::vec2(0.0, 100.0))],
        1.0,
    );
    assert_eq!(snapshot(&c).viewport_offset, 0);
    let selection = snapshot(&c).selection.unwrap();
    assert_eq!((selection.start, selection.end), (100, 103));
}

#[test]
fn source_replacement_retires_captured_minimap_navigation_and_old_sample_replies() {
    let (_file, core, mut c) = setup(16000);
    let mut replacement = tempfile::NamedTempFile::new().unwrap();
    replacement.write_all(&vec![0x42; 16000]).unwrap();
    replacement.flush().unwrap();
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let b = h.view.frame().viewport_box.unwrap();
    let grab = egui::pos2(b.center().x, b.top() + 4.0);
    h.pointer(&mut c, &core, grab, Some(true));
    h.pointer(&mut c, &core, grab + egui::vec2(0.0, 100.0), None);
    assert!(h.view.minimap_captured());
    assert!(snapshot(&c).viewport_offset > 0);
    let source = snapshot(&c).source;
    let (old_ticket, old_request) = h.view.next_minimap_request().unwrap();
    let old_reply = core.execute(old_request, &mut |_| {});
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: replacement.path().into(),
                discard_unsaved: false,
            },
        ))],
    );
    assert_ne!(snapshot(&c).source, source);
    // Input still contains the previous window's held pointer, but a source
    // boundary must retire its grab before processing this next native frame.
    h.pointer(&mut c, &core, grab + egui::vec2(0.0, 150.0), None);
    assert_eq!(snapshot(&c).viewport_offset, 0);
    assert_eq!(h.view.frame().top_row, 0);
    assert!(!h.view.minimap_captured());
    assert!(h.view.complete_minimap(old_ticket, old_reply).is_none());
    h.pointer(&mut c, &core, grab + egui::vec2(0.0, 200.0), None);
    assert_eq!(snapshot(&c).viewport_offset, 0);
    h.pointer(&mut c, &core, grab + egui::vec2(0.0, 200.0), Some(false));
    assert_eq!(snapshot(&c).viewport_offset, 0);
    assert!(snapshot(&c).selection.is_none());
    let page = snapshot(&c).page.unwrap();
    assert!(page.bytes.iter().all(|&byte| byte == 0x42));
}
#[test]
fn fit_dpi_rebase_retains_the_current_row_and_normal_fit_sampling() {
    for (bytes, row) in [(1600, 50), (16000, 500)] {
        for dpi in [1.25, 1.5, 2.0] {
            let (_file, core, mut c) = setup(bytes);
            c.handle(ControllerEvent::Invoke {
                command: hexforge_controller::command::Command::MinimapFit,
                context: Default::default(),
            });
            c.handle(ControllerEvent::Action(Action::ScrollViewport {
                offset: row * 16,
            }));
            let mut h = Host::new();
            h.frame(&mut c, &core, vec![], 1.0);
            let b = h.view.frame().viewport_box.unwrap();
            let pointer = egui::pos2(b.center().x, b.top() + b.height() / 2.0);
            h.pointer(&mut c, &core, pointer, Some(true));
            assert_eq!(snapshot(&c).viewport_offset / 16, row);
            let remapped = egui::pos2(pointer.x, pointer.y / dpi);
            h.frame_at(
                &mut c,
                &core,
                vec![Event::PointerMoved(remapped)],
                dpi,
                true,
                egui::pos2(-1920.0, -400.0),
            );
            assert!(
                h.view.minimap_captured(),
                "the remapped fit pointer is still inside the minimap"
            );
            assert_eq!(
                snapshot(&c).viewport_offset / 16,
                row,
                "fit mode must not jump at {dpi} DPI"
            );
            h.frame_at(
                &mut c,
                &core,
                vec![Event::PointerMoved(remapped + egui::vec2(0.0, 4.0))],
                dpi,
                true,
                egui::pos2(-1920.0, -400.0),
            );
            let after = snapshot(&c).viewport_offset / 16;
            if bytes == 1600 {
                assert_eq!(after, row + 2);
            } else {
                assert!((row + 15..=row + 16).contains(&after));
            }
        }
    }
}
#[test]
fn viewport_scroll_preserves_selection_and_template_highlight() {
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::NavigateTemplate {
            start: 7,
            end: 9,
        })],
    );
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 160,
        })],
    );
    let s = snapshot(&c);
    assert_eq!(s.viewport_offset, 160);
    assert_eq!(s.selection.as_ref().unwrap().start, 7);
    assert_eq!(s.template_range.as_ref().unwrap().end, 9);
}
#[test]
fn empty_file_has_no_byte_reads_or_minimap_drag() {
    let (_file, core, mut c) = setup(0);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.view.frame().last_visible_row, None);
    assert!(h.view.next_minimap_request().is_none());
    assert!(h.view.frame().viewport_box.is_none());
}
#[test]
fn row_render_is_viewport_bounded() {
    use hexforge_controller::hex::layout::{HexLayout, HexMetrics};
    use hexforge_native::drawing::hex::{paint_hex, HexScene};
    let (_file, _core, c) = setup(16000);
    let s = snapshot(&c);
    let ctx = egui::Context::default();
    hexforge_native::style::fonts::install(&ctx);
    let layout = HexLayout::new(
        HexMetrics {
            width: 1000.0,
            char_width: 8.0,
        },
        s.row_width,
    );
    let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 252.0));
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut out = ctx.run_ui(Default::default(), |ui| {
        paint_hex(
            ui.painter(),
            &HexScene {
                rect,
                layout: &layout,
                snapshot: &s,
                top_row: 0,
                horizontal_px: 0.0,
            },
            &style,
        );
    });
    out.textures_delta.clear();
    let text: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::epaint::Shape::Text(t) = &s.shape {
                Some(t.galley.job.text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        text.iter()
            .filter(|t| t.len() == 8 && t.starts_with("000000"))
            .count(),
        10
    );
    assert!(text.len() <= 333);
    out.textures_delta.clear();
}

#[test]
fn actual_view_uses_source_fixed_eight_pixel_cells_not_font_advance() {
    let (_file, _core, c) = setup(16000);
    let mut host = Host::new();
    let s = snapshot(&c);
    let mut output = host.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            host.view.show(ui, &s, &mut host.cache);
        },
    );
    output.textures_delta.clear();
    let text = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(text("OFFSET").pos.x, 16.0);
    assert_eq!(text("HEX BYTES").pos.x, 160.0);
    assert_eq!(text("ASCII").pos.x, 576.0);
    assert_eq!(text("41").pos.x, 160.0);
    assert_eq!(text("41").galley.job.sections[0].format.font_id.size, 12.0);
}

#[test]
fn native_light_hex_canvas_keeps_its_distinct_source_colors() {
    let (_file, _core, mut c) = setup(16000);
    c.handle(ControllerEvent::SystemThemeChanged(
        hexforge_controller::controller::Theme::Light,
    ));
    let mut host = Host::new();
    let s = snapshot(&c);
    let mut output = host.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            host.view.show(ui, &s, &mut host.cache);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(r) if r.rect == host.view.frame().viewport_rect && r.fill == egui::Color32::WHITE)),
        "Light Canvas is white, not the application's #f6f8fa background");
    let byte = output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == "41" => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(byte.fallback_color, egui::Color32::from_rgb(31, 35, 40));
}

#[test]
fn dedicated_minimap_and_scrollbar_borders_do_not_steal_preview_pixels() {
    let (_file, _core, c) = setup(16000);
    // Source flex-basis is content-box: preview 84/136 plus its 1px border;
    // main scrollbar 12 plus its own 1px border, not 84 + 12 total.
    for (width, enabled, grid_right, preview_left, preview_width) in [
        (
            hexforge_controller::controller::RowWidth::Sixteen,
            true,
            1182.0,
            Some(1183.0),
            84.0,
        ),
        (
            hexforge_controller::controller::RowWidth::ThirtyTwo,
            true,
            1130.0,
            Some(1131.0),
            136.0,
        ),
        (
            hexforge_controller::controller::RowWidth::Sixteen,
            false,
            1267.0,
            None,
            0.0,
        ),
    ] {
        let mut host = Host::new();
        let mut s = snapshot(&c);
        s.row_width = width;
        s.minimap.enabled = enabled;
        let mut output = host.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                host.view.show(ui, &s, &mut host.cache);
            },
        );
        output.textures_delta.clear();
        assert_eq!(host.view.frame().viewport_rect.right(), grid_right);
        assert_eq!(
            host.view.frame().minimap_rect.map(|r| r.left()),
            preview_left
        );
        if let Some(mini) = host.view.frame().minimap_rect {
            assert_eq!(mini.width(), preview_width);
            assert_eq!(mini.right(), 1267.0);
        }
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == egui::Rect::from_min_size(egui::pos2(1267.0, 0.0), egui::vec2(1.0, 252.0))
                && r.fill == egui::Color32::from_rgb(42, 49, 58))),
            "the main-scrollbar border remains when the minimap is disabled");
        assert!(
            output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect.left() == 1269.0 && r.rect.width() == 10.0)),
            "thumb's 1px inset starts after the source content-box border"
        );
    }
}

#[test]
fn main_scrollbar_track_reaches_results_without_a_splitter_gap() {
    use hexforge_controller::controller::{RowWidth, Theme};
    use hexforge_native::{
        style::HexForgeStyle,
        views::main_window::{self, MainControls},
    };
    let (_file, _core, c) = setup(16000);
    // The editor keeps its existing 4px resize strip and page geometry. Only
    // the rightmost scrollbar track must continue to the results top border.
    for theme in [Theme::Dark, Theme::Light] {
        for dpr in [1.0, 1.75] {
            for row_width in [RowWidth::Sixteen, RowWidth::ThirtyTwo] {
                for minimap in [false, true] {
                    for collapsed in [false, true] {
                        let mut host = Host::new();
                        let mut s = snapshot(&c);
                        s.theme = theme;
                        s.row_width = row_width;
                        s.minimap.enabled = minimap;
                        let style = HexForgeStyle::for_theme(theme, dpr);
                        style.apply(&host.ctx);
                        let mut controls = MainControls::default();
                        controls.panel.collapsed = collapsed;
                        let mut input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(1280.0, 800.0),
                            )),
                            ..Default::default()
                        };
                        input
                            .viewports
                            .get_mut(&egui::ViewportId::ROOT)
                            .unwrap()
                            .native_pixels_per_point = Some(dpr);
                        let mut output = host.ctx.run_ui(input, |ui| {
                            main_window::show(
                                ui,
                                &style,
                                &s,
                                &mut host.view,
                                &mut host.cache,
                                &mut controls,
                            );
                        });
                        output.textures_delta.clear();
                        // Hand-derived: 800px - 24px status - 220px results,
                        // or a 28px header when collapsed.
                        let results_top = if collapsed { 748.0 } else { 556.0 };
                        let final_fill = |point: Pos2| {
                            output
                                .shapes
                                .iter()
                                .rev()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Rect(r)
                                        if r.fill != egui::Color32::TRANSPARENT
                                            && r.rect.contains(point)
                                            && shape.clip_rect.contains(point) =>
                                    {
                                        Some(r.fill)
                                    }
                                    _ => None,
                                })
                        };
                        assert_eq!(final_fill(egui::pos2(1273.0, results_top - 0.5)),
                            Some(style.palette.text.gamma_multiply(0.08)),
                            "the track must meet the result border even across the resize strip: {theme:?}, DPR {dpr}, {row_width:?}, minimap {minimap}, collapsed {collapsed}");
                        assert_eq!(
                            final_fill(egui::pos2(1267.5, results_top - 0.5)),
                            Some(style.palette.border),
                            "the track's left border must continue with its background"
                        );
                        let grid_bottom = results_top
                            - if collapsed { 0.0 } else { 4.0 }
                            - if row_width == RowWidth::ThirtyTwo {
                                10.0
                            } else {
                                0.0
                            };
                        assert_eq!(host.view.frame().viewport_rect.bottom(), grid_bottom,
                            "fixing the painted gap must not add blank rows or change EOF/page height");
                    }
                }
            }
        }
    }
}

#[test]
fn main_scrollbar_eof_thumb_meets_results_border_without_growing_hex_page() {
    use hexforge_controller::controller::{RowWidth, Theme};
    use hexforge_native::{
        style::HexForgeStyle,
        views::main_window::{self, MainControls},
    };
    for size in [0, 32, 16000] {
        let (_file, _core, c) = setup(size);
        for theme in [Theme::Dark, Theme::Light] {
            for dpr in [1.0, 1.75] {
                for row_width in [RowWidth::Sixteen, RowWidth::ThirtyTwo] {
                    let mut h = Host::new();
                    let mut s = snapshot(&c);
                    s.theme = theme;
                    s.row_width = row_width;
                    s.viewport_offset = u64::MAX;
                    let style = HexForgeStyle::for_theme(theme, dpr);
                    style.apply(&h.ctx);
                    let mut controls = MainControls::default();
                    let mut input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1280.0, 800.0),
                        )),
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(dpr);
                    let mut output = h.ctx.run_ui(input, |ui| {
                        main_window::show(ui, &style, &s, &mut h.view, &mut h.cache, &mut controls);
                    });
                    output.textures_delta.clear();
                    let thumb = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Rect(r)
                                if r.rect.left() == 1269.0 && r.rect.width() == 10.0 =>
                            {
                                Some(r.rect.intersect(shape.clip_rect))
                            }
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(thumb.bottom(), 556.0,
                        "the EOF thumb must meet the results border, not stop above its resize strip: {theme:?}, DPR {dpr}, {row_width:?}, {size} bytes");
                    assert_eq!(
                        h.view.frame().viewport_rect.bottom(),
                        if row_width == RowWidth::ThirtyTwo {
                            542.0
                        } else {
                            552.0
                        }
                    );
                    assert_eq!(
                        h.view.frame().last_visible_row,
                        match (size, row_width) {
                            (0, _) => None,
                            (32, RowWidth::Sixteen) => Some(1),
                            (32, _) => Some(0),
                            (_, RowWidth::Sixteen) => Some(999),
                            _ => Some(499),
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn scrollbar_bottom_capture_does_not_resize_results_and_divider_still_resizes() {
    use hexforge_native::{
        style::HexForgeStyle,
        views::main_window::{self, MainControls},
    };
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::Row32,
            context: Default::default(),
        }],
    );
    let mut h = Host::new();
    let mut controls = MainControls::default();
    let mut frame = |events: Vec<Event>| {
        let s = snapshot(&c);
        let style = HexForgeStyle::for_theme(s.theme, 1.0);
        style.apply(&h.ctx);
        let mut actions = vec![];
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                actions.extend(main_window::show(
                    ui,
                    &style,
                    &s,
                    &mut h.view,
                    &mut h.cache,
                    &mut controls,
                ));
            },
        );
        output.textures_delta.clear();
        settle(&mut c, &core, actions);
        (
            snapshot(&c).viewport_offset,
            controls.panel.preferred_height,
        )
    };
    let pointer = |p: Pos2, pressed: Option<bool>| {
        let mut events = vec![Event::PointerMoved(p)];
        if let Some(pressed) = pressed {
            events.push(Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        events
    };
    frame(vec![]);
    let bottom = egui::pos2(1274.0, 555.5); // Last pixel of the extended side track.
    frame(pointer(bottom, None));
    assert_eq!(frame(pointer(bottom, Some(true))), (15392, 220.0),
        "the bottom of the side track must navigate to the last full page, not capture the resize handle");
    let outside = egui::pos2(1274.0, 575.5);
    assert_eq!(
        frame(pointer(outside, None)),
        (15392, 220.0),
        "captured side-scroll dragging must not change the results height"
    );
    frame(pointer(outside, Some(false)));
    // The neighbouring 4px divider remains fully functional away from the
    // side track. Moving it up by 20px increases the pane by exactly 20px.
    let divider = egui::pos2(600.0, 554.0);
    frame(pointer(divider, None));
    frame(pointer(divider, Some(true)));
    assert_eq!(frame(pointer(egui::pos2(600.0, 534.0), None)).1, 240.0);
    frame(pointer(egui::pos2(600.0, 534.0), Some(false)));
}

#[test]
fn main_scrollbar_thumb_reaches_full_track_at_eof_with_horizontal_overflow() {
    use hexforge_controller::controller::{RowWidth, Theme};
    // 252px side track, 242px hex content after its horizontal scrollbar.
    // Drawing against 242px while dragging against 252px leaves an 8px gap
    // below the bordered thumb and makes the thumb move after mouse release.
    for theme in [Theme::Dark, Theme::Light] {
        for dpr in [1.0, 1.75] {
            for size in [0, 32, 16000] {
                let (_file, core, mut c) = setup(size);
                let mut h = Host::new();
                let mut s = snapshot(&c);
                s.theme = theme;
                s.row_width = RowWidth::ThirtyTwo;
                s.viewport_offset = u64::MAX;
                let draw = |h: &mut Host, s: &MainSnapshot| {
                    let mut input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1280.0, 252.0),
                        )),
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(dpr);
                    let mut output = h.ctx.run_ui(input, |ui| {
                        h.view.show(ui, s, &mut h.cache);
                    });
                    output.textures_delta.clear();
                    output
                };
                let output = draw(&mut h, &s);
                let thumb = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(r)
                            if r.rect.left() == 1269.0 && r.rect.width() == 10.0 =>
                        {
                            Some(r.rect.intersect(shape.clip_rect))
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(thumb.bottom(), 252.0,
                    "the painted thumb must reach the actual side track bottom: {theme:?}, DPR {dpr}, {size} bytes");
                assert_eq!(thumb.top(), if size == 16000 { 228.0 } else { 0.0 });
                assert_eq!(h.view.frame().viewport_rect.height(), 242.0);
                assert_eq!(
                    h.view.frame().last_visible_row,
                    match size {
                        0 => None,
                        32 => Some(0),
                        _ => Some(499),
                    }
                );
                if size == 16000 {
                    // Noncentral grab and a one-pixel movement must not jump
                    // upward because painted and hit-tested thumb origins differ.
                    c.handle(ControllerEvent::Invoke {
                        command: hexforge_controller::command::Command::Row32,
                        context: Default::default(),
                    });
                    c.handle(ControllerEvent::Action(Action::ScrollViewport {
                        offset: 15712,
                    }));
                    h.frame(&mut c, &core, vec![], dpr);
                    h.pointer(&mut c, &core, egui::pos2(1274.0, 235.0), Some(true));
                    assert_eq!(snapshot(&c).viewport_offset, 15712);
                    h.pointer(&mut c, &core, egui::pos2(1274.0, 236.0), None);
                    assert_eq!(snapshot(&c).viewport_offset, 15712);
                    h.pointer(&mut c, &core, egui::pos2(1274.0, 236.0), Some(false));
                    draw(&mut h, &snapshot(&c));
                    assert_eq!(h.view.frame().top_row, 491);
                }
            }
        }
    }
}

#[test]
fn overflowing_grid_reserves_source_scrollbar_height_and_keeps_full_height_minimap() {
    // The actual Windows/Edge thin scrollbar occupies 10px. The source
    // The shortened content box determines visible rows and long-file thumb
    // size. Its position and pointer capture share the full-height side track.
    for (size, requested, top, last, thumb_top, thumb_height) in [
        (16000, u64::MAX, 491, Some(499), 228.0, 24.0),
        (32, 0, 0, Some(0), 0.0, 252.0),
    ] {
        let (_file, core, mut c) = setup(size);
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Invoke {
                command: hexforge_controller::command::Command::Row32,
                context: Default::default(),
            }],
        );
        c.handle(ControllerEvent::Action(Action::ScrollViewport {
            offset: requested,
        }));
        let mut host = Host::new();
        let s = snapshot(&c);
        let mut output = host.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                host.view.show(ui, &s, &mut host.cache);
            },
        );
        output.textures_delta.clear();
        let frame = host.view.frame();
        assert_eq!(frame.viewport_rect.height(), 242.0);
        assert_eq!(frame.minimap_rect.unwrap().height(), 252.0);
        assert_eq!((frame.top_row, frame.last_visible_row), (top, last));
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == egui::Rect::from_min_size(
                egui::pos2(1269.0, thumb_top), egui::vec2(10.0, thumb_height + 2.0)))));
        if size == 16000 {
            host.pointer(&mut c, &core, egui::pos2(1274.0, 12.0), Some(true));
            host.pointer(&mut c, &core, egui::pos2(1274.0, 250.0), None);
            assert_eq!(snapshot(&c).viewport_offset, 15712);
            host.pointer(&mut c, &core, egui::pos2(1274.0, 250.0), Some(false));
        }
    }
}
#[test]
fn horizontal_scroll_preserves_address_labels() {
    use hexforge_controller::hex::layout::{HexLayout, HexMetrics};
    use hexforge_native::drawing::hex::{paint_hex, HexScene};
    let (_file, _core, c) = setup(16000);
    let s = snapshot(&c);
    let ctx = egui::Context::default();
    hexforge_native::style::fonts::install(&ctx);
    let layout = HexLayout::new(
        HexMetrics {
            width: 600.0,
            char_width: 8.0,
        },
        s.row_width,
    );
    let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 252.0));
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    for horizontal_px in [0.0, 100.0, 400.0] {
        let mut out = ctx.run_ui(Default::default(), |ui| {
            paint_hex(
                ui.painter(),
                &HexScene {
                    rect,
                    layout: &layout,
                    snapshot: &s,
                    top_row: 0,
                    horizontal_px,
                },
                &style,
            );
        });
        out.textures_delta.clear();
        let label = out
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::epaint::Shape::Text(t) = &s.shape {
                    (t.galley.job.text == "00000000").then_some(t)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(label.pos.x, 16.0);
        out.textures_delta.clear();
    }
}
#[test]
fn horizontal_scrollbar_uses_source_track_background_and_thumb_insets() {
    // Fresh source framebuffer at1280px: 1130px content box, 12px end
    // buttons, round(1106*1130/1272)=983px thumb. The inset is across the
    // scrollbar only, so its first painted X is12, not10+2 by coincidence.
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::Row32,
            context: Default::default(),
        }],
    );
    let mut host = Host::new();
    let s = snapshot(&c);
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut out = host.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            host.view.show(ui, &s, &mut host.cache);
        },
    );
    out.textures_delta.clear();
    let has_rect = |rect, color| {
        out.shapes.iter().any(|shape| {
            matches!(&shape.shape,
        egui::Shape::Rect(r) if r.rect == rect && r.fill == color)
        })
    };
    assert!(
        has_rect(
            egui::Rect::from_min_size(egui::pos2(0.0, 242.0), egui::vec2(1130.0, 10.0)),
            style.palette.bg
        ),
        "the source's transparent horizontal track must expose the app background, not the panel"
    );
    assert!(
        has_rect(
            egui::Rect::from_min_size(egui::pos2(12.0, 244.0), egui::vec2(983.0, 6.0)),
            style.palette.border_strong
        ),
        "the source thumb reserves both12px end buttons and only the cross-axis inset"
    );
}
#[test]
fn horizontal_thumb_drag_keeps_reference_travel_and_file_navigation_unchanged() {
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::Row32,
            context: Default::default(),
        }],
    );
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    // Five pixels into the reference's983px thumb;50px motion over123px of
    // thumb travel advances57.723577...px over the142px content range. Keep
    // that local fractional motion rather than quantizing a logical file row.
    h.pointer(&mut c, &core, egui::pos2(17.0, 247.0), Some(true));
    h.pointer(&mut c, &core, egui::pos2(67.0, 247.0), None);
    assert!((h.view.frame().horizontal_px - 57.72357723577236).abs() < 1e-9);
    h.pointer(&mut c, &core, egui::pos2(1300.0, 247.0), None);
    assert_eq!(h.view.frame().horizontal_px, 142.0);
    h.pointer(&mut c, &core, egui::pos2(1300.0, 247.0), Some(false));
    h.pointer(&mut c, &core, egui::pos2(140.0, 247.0), Some(true));
    h.pointer(&mut c, &core, egui::pos2(90.0, 247.0), None);
    assert!((h.view.frame().horizontal_px - 84.27642276422764).abs() < 1e-9);
    h.pointer(&mut c, &core, egui::pos2(-100.0, 247.0), None);
    assert_eq!(h.view.frame().horizontal_px, 0.0);
    h.pointer(&mut c, &core, egui::pos2(-100.0, 247.0), Some(false));
    assert_eq!(snapshot(&c).viewport_offset, 0);
    assert_eq!(snapshot(&c).selection, None);
    assert_eq!(snapshot(&c).file.unwrap().revision, "0");
}

#[test]
fn horizontal_arrow_buttons_step_without_moving_file_rows_or_selection() {
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::Row32,
            context: Default::default(),
        }],
    );
    let mut host = Host::new();
    host.frame(&mut c, &core, vec![], 1.0);
    host.pointer(&mut c, &core, egui::pos2(1125.0, 247.0), Some(true));
    assert_eq!(
        host.view.frame().horizontal_px,
        40.0,
        "clicking the source's right arrow scrolls 40px rather than jumping to the end"
    );
    host.pointer(&mut c, &core, egui::pos2(1125.0, 247.0), Some(false));
    host.pointer(&mut c, &core, egui::pos2(5.0, 247.0), Some(true));
    assert_eq!(host.view.frame().horizontal_px, 0.0);
    host.pointer(&mut c, &core, egui::pos2(5.0, 247.0), Some(false));
    assert_eq!(snapshot(&c).viewport_offset, 0);
    assert_eq!(snapshot(&c).selection, None);
}
#[test]
fn horizontal_track_click_pages_instead_of_recentering_the_thumb() {
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::Row32,
            context: Default::default(),
        }],
    );
    let mut h = Host::new();
    let p = egui::pos2(400.0, 247.0);
    for events in [
        vec![],
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    ] {
        let s = snapshot(&c);
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(800.0, 252.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                h.view.show(ui, &s, &mut h.cache);
            },
        );
        output.textures_delta.clear();
    }
    assert_eq!(
        h.view.frame().horizontal_px,
        568.0,
        "the real reference pages by floor(650px * 0.875), not a centered-thumb jump"
    );
}
#[test]
fn horizontal_scrollbar_hover_tracks_the_real_thumb_and_arrow() {
    for (theme, expected) in [
        (
            hexforge_controller::controller::Theme::Dark,
            egui::Color32::from_rgb(99, 107, 116),
        ),
        (
            hexforge_controller::controller::Theme::Light,
            egui::Color32::from_rgb(128, 135, 142),
        ),
    ] {
        let (_file, core, mut c) = setup(16000);
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Invoke {
                command: hexforge_controller::command::Command::Row32,
                context: Default::default(),
            }],
        );
        c.handle(ControllerEvent::SystemThemeChanged(theme));
        let mut h = Host::new();
        h.frame(&mut c, &core, vec![], 1.0);
        for (pointer, thumb) in [
            (egui::pos2(1125.0, 247.0), false),
            (egui::pos2(500.0, 247.0), true),
        ] {
            let s = snapshot(&c);
            let mut output = h.ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(1280.0, 252.0),
                    )),
                    events: vec![Event::PointerMoved(pointer)],
                    ..Default::default()
                },
                |ui| {
                    h.view.show(ui, &s, &mut h.cache);
                },
            );
            output.textures_delta.clear();
            assert!(output.shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::Path(p) => !thumb && p.fill == expected && p.points.iter().all(|point| point.x > 1120.0),
                egui::Shape::Rect(r) => thumb && r.fill == expected && r.rect == egui::Rect::from_min_size(egui::pos2(12.0, 244.0), egui::vec2(983.0, 6.0)),
                _ => false,
            }), "hover must affect the control under the pointer in {theme:?}, not the entire scrollbar");
        }
        assert_eq!(h.view.frame().horizontal_px, 0.0);
    }
}
#[test]
fn held_horizontal_arrow_repeats_and_focus_loss_cancels_it() {
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::Row32,
            context: Default::default(),
        }],
    );
    let mut h = Host::new();
    let p = egui::pos2(1125.0, 247.0);
    for (time, focused, events, want) in [
        (1.0, true, vec![], 0.0),
        (
            1.01,
            true,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            40.0,
        ),
        (1.30, true, vec![], 80.0),
        (1.31, false, vec![], 80.0),
        (2.0, true, vec![], 80.0),
    ] {
        let s = snapshot(&c);
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                time: Some(time),
                focused,
                events,
                ..Default::default()
            },
            |ui| {
                h.view.show(ui, &s, &mut h.cache);
            },
        );
        output.textures_delta.clear();
        assert_eq!(
            h.view.frame().horizontal_px,
            want,
            "time {time}: a held arrow must repeat, but never after focus loss"
        );
    }
    assert_eq!(snapshot(&c).viewport_offset, 0);
    assert_eq!(snapshot(&c).selection, None);
}
#[test]
fn all_highlight_layers_are_independent() {
    use hexforge_controller::hex::layout::{HexLayout, HexMetrics};
    use hexforge_native::drawing::hex::{paint_hex, HexScene};
    let (_file, _core, c) = setup(16000);
    let mut s = snapshot(&c);
    s.selection = Some(hexforge_controller::controller::ByteSelection { start: 0, end: 0 });
    s.template_range = s.selection.clone();
    s.matches = vec![0];
    s.search.match_length = 1;
    s.page.as_mut().unwrap().data.modified_offsets = vec!["0".into()];
    let ctx = egui::Context::default();
    hexforge_native::style::fonts::install(&ctx);
    let layout = HexLayout::new(
        HexMetrics {
            width: 1000.0,
            char_width: 8.0,
        },
        s.row_width,
    );
    let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 252.0));
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut out = ctx.run_ui(Default::default(), |ui| {
        paint_hex(
            ui.painter(),
            &HexScene {
                rect,
                layout: &layout,
                snapshot: &s,
                top_row: 0,
                horizontal_px: 0.0,
            },
            &style,
        );
    });
    out.textures_delta.clear();
    let rs: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::epaint::Shape::Rect(r) = &s.shape {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        rs.iter()
            .filter(|r| r.fill == egui::Color32::from_rgba_unmultiplied(240, 136, 62, 56))
            .count(),
        2
    );
    assert_eq!(
        rs.iter()
            .filter(|r| r.fill == egui::Color32::from_rgba_unmultiplied(139, 111, 42, 107))
            .count(),
        2
    );
    assert_eq!(
        rs.iter()
            .filter(|r| r.fill == egui::Color32::from_rgba_unmultiplied(31, 111, 235, 122))
            .count(),
        2
    );
    assert_eq!(
        rs.iter()
            .filter(|r| r.stroke.color == egui::Color32::from_rgb(57, 197, 207))
            .count(),
        2
    );
    assert_eq!(
        rs.iter()
            .filter(|r| r.fill == egui::Color32::from_rgb(240, 136, 62))
            .count(),
        2
    );
    out.textures_delta.clear();
}

#[test]
fn minimap_marks_only_the_exact_successful_parsed_leaf_and_preserves_other_layers() {
    use hexforge_controller::controller::ByteSelection;
    use hexforge_core::{
        dto::ModifiedOverviewDto,
        template::{FieldType, ParsedNode},
    };
    let (_file, _core, controller) = setup(4096);
    let mut s = snapshot(&controller);
    s.results = vec![ParsedNode {
        kind: "leaf".into(),
        path: "magic".into(),
        name: "magic".into(),
        field_type: FieldType::Bytes,
        offset: Some("32".into()),
        length: Some("4".into()),
        value: Some("41 41 41 41".into()),
        endianness: None,
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    }];
    s.matches = vec![128];
    s.search.match_length = 1;
    s.modified_overview = ModifiedOverviewDto {
        bin_count: 1024,
        bins: vec![512],
    };
    let mut host = Host::new();
    for (range, expected) in [
        (None, false),
        (Some((32, 35)), true),
        (Some((32, 34)), false),
        (None, false),
    ] {
        s.template_range = range.map(|(start, end)| ByteSelection { start, end });
        let mut output = host.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                host.view.show(ui, &s, &mut host.cache);
            },
        );
        let marker_colors: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(r) => Some(r.fill),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        assert_eq!(marker_colors.iter().any(|c| [140, 166].iter().any(|a| *c == egui::Color32::from_rgba_unmultiplied(57, 197, 207, *a))), expected,
            "Only the current exact parsed leaf belongs in the minimap; stale/mismatched ranges do not");
        assert!(marker_colors.contains(&egui::Color32::from_rgba_unmultiplied(189, 166, 74, 166)));
        assert!(marker_colors.contains(&egui::Color32::from_rgba_unmultiplied(240, 136, 62, 230)));
    }
}

#[test]
fn minimap_matches_nested_leaf_ranges_without_rounding_large_offsets() {
    use hexforge_controller::controller::ByteSelection;
    use hexforge_core::template::{FieldType, ParsedNode};
    let (_file, _core, controller) = setup(32);
    let mut s = snapshot(&controller);
    s.file.as_mut().unwrap().size = "9007199254741001".into();
    s.minimap.mode = MinimapMode::Fit;
    let leaf = ParsedNode {
        kind: "leaf".into(),
        path: "header.id".into(),
        name: "id".into(),
        field_type: FieldType::U64,
        offset: Some("9007199254740993".into()),
        length: Some("8".into()),
        value: Some("18446744073709551615".into()),
        endianness: None,
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    };
    let mut parent = leaf.clone();
    parent.kind = "struct".into();
    parent.field_type = FieldType::Struct;
    parent.name = "header".into();
    parent.path = "header".into();
    parent.offset = Some("9007199254740992".into());
    parent.length = Some("9".into());
    parent.value = None;
    parent.children = vec![leaf];
    s.results = vec![parent];
    let mut host = Host::new();
    for (start, expected) in [
        (9_007_199_254_740_993, true),
        (9_007_199_254_740_992, false),
    ] {
        s.template_range = Some(ByteSelection {
            start,
            end: 9_007_199_254_741_000,
        });
        let mut output = host.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                host.view.show(ui, &s, &mut host.cache);
            },
        );
        let marked = output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgba_unmultiplied(57, 197, 207, 166)));
        output.textures_delta.clear();
        assert_eq!(
            marked, expected,
            "The enclosing struct and adjacent >2^53 offset are not the selected leaf"
        );
    }
}

#[test]
fn proportional_preview_layers_and_drag_border_use_the_source_theme_states() {
    use hexforge_controller::controller::{ByteSelection, Theme};
    use hexforge_core::template::{FieldType, ParsedNode};
    for (theme, base, hover, active, hover_border, active_border) in [
        (
            Theme::Dark,
            egui::Color32::from_rgba_unmultiplied(145, 165, 185, 41),
            egui::Color32::from_rgba_unmultiplied(145, 165, 185, 54),
            egui::Color32::from_rgba_unmultiplied(145, 165, 185, 64),
            egui::Color32::from_rgb(184, 212, 239),
            egui::Color32::from_rgb(31, 111, 235),
        ),
        (
            Theme::Light,
            egui::Color32::from_rgba_unmultiplied(70, 100, 135, 26),
            egui::Color32::from_rgba_unmultiplied(70, 100, 135, 38),
            egui::Color32::from_rgba_unmultiplied(70, 100, 135, 51),
            egui::Color32::from_rgb(87, 110, 138),
            egui::Color32::from_rgb(9, 105, 218),
        ),
    ] {
        let (_file, _core, controller) = setup(16000);
        let mut s = snapshot(&controller);
        s.theme = theme;
        s.viewport_offset = 7920;
        s.template_range = Some(ByteSelection {
            start: 8000,
            end: 8000,
        });
        s.results = vec![ParsedNode {
            kind: "leaf".into(),
            path: "field".into(),
            name: "field".into(),
            field_type: FieldType::U8,
            offset: Some("8000".into()),
            length: Some("1".into()),
            value: Some("65".into()),
            endianness: None,
            comment: String::new(),
            enum_label: None,
            flags: vec![],
            diagnostics: vec![],
            children: vec![],
        }];
        let mut host = Host::new();
        let mut draw = |events| {
            let mut output = host.ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(1280.0, 252.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    host.view.show(ui, &s, &mut host.cache);
                },
            );
            output.textures_delta.clear();
            output
        };
        let output = draw(vec![]);
        let expected_box =
            egui::Rect::from_min_size(egui::pos2(1183.0, 116.0), egui::vec2(84.0, 20.0));
        let raster = output
            .shapes
            .iter()
            .position(|shape| matches!(&shape.shape, egui::Shape::Mesh(_)))
            .unwrap();
        let marker = output.shapes.iter().position(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgba_unmultiplied(57, 197, 207, 166))).unwrap();
        let viewport = output
            .shapes
            .iter()
            .position(|shape| {
                matches!(&shape.shape,
            egui::Shape::Rect(r) if r.fill == base && r.rect == expected_box)
            })
            .unwrap();
        assert!(
            raster < marker && marker < viewport,
            "Preview raster, markers, then the shaded viewport"
        );
        let pointer = egui::pos2(1191.0, 119.0);
        let hovered = draw(vec![Event::PointerMoved(pointer)]);
        assert!(hovered.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == expected_box && r.fill == hover)));
        assert!(hovered.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == expected_box && r.stroke.color == hover_border)));
        let dragged = draw(vec![Event::PointerButton {
            pos: pointer,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        assert!(dragged.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == expected_box && r.fill == active)));
        assert!(dragged.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == expected_box && r.stroke.color == active_border)));
    }
}
#[test]
fn stale_sample_does_not_repaint_new_tile() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 9600,
        })],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    let (old, request) = h
        .view
        .next_minimap_request()
        .expect("old tile needs more bytes");
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 14400,
        })],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    h.frame(&mut c, &core, vec![], 1.0);
    let before = h.cache.minimap.image().pixels.clone();
    assert!(h
        .view
        .complete_minimap(old, core.execute(request, &mut |_| {}))
        .is_none());
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.cache.minimap.image().pixels, before);
    assert_eq!(snapshot(&c).viewport_offset, 14400);
}
#[test]
fn fit_short_rows_keep_natural_height_and_fit_wheel_policy() {
    let (_file, core, mut c) = setup(16000);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::MinimapFit,
            context: Default::default(),
        }],
    );
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let mini = h.view.frame().minimap_rect.unwrap();
    let p = egui::pos2(mini.center().x, mini.top() + 100.0);
    h.pointer(&mut c, &core, p, Some(true));
    assert_eq!(snapshot(&c).viewport_offset / 16, 393);
    assert!(h.view.minimap_captured());
    h.pointer(&mut c, &core, p, Some(false));
    h.frame(
        &mut c,
        &core,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        }],
        1.0,
    );
    assert_eq!(snapshot(&c).viewport_offset / 16, 398);
    let (_short, core, mut c) = setup(32);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.cache.minimap.image().size, [84, 4]);
    assert_eq!(h.view.frame().viewport_box.unwrap().height(), 4.0);
}
#[test]
fn native_app_draws_the_actual_opened_bytes() {
    use hexforge_native::app::NativeApp;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    let (file, core, _) = setup(16000);
    let mut app = NativeApp::new(InitialState::default(), Arc::new(core));
    let ctx = egui::Context::default();
    app.attach(&ctx);
    app.gesture(Action::DropFiles(vec![file.path().into()]));
    let until = Instant::now() + Duration::from_secs(3);
    while app.main_snapshot().file.is_none() {
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    let mut text = vec![];
    while app.main_snapshot().page.is_none() {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| app.paint_main(ui),
        );
        output.textures_delta.clear();
        app.pump();
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
    for s in &output.shapes {
        if let egui::epaint::Shape::Text(t) = &s.shape {
            text.push(t.galley.job.text.clone());
        }
    }
    assert!(text.iter().any(|t| t == "00000000"));
    assert!(text.iter().any(|t| t == "41"));
}
#[test]
fn viewport_reads_reject_unbounded_or_outside_requests() {
    let (_file, core, mut c) = setup(16000);
    for (offset, length) in [(0, 0), (16000, 1), (0, 1_048_577)] {
        let effects = c.handle(ControllerEvent::Action(Action::ReadViewport {
            offset,
            length,
        }));
        assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    }
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(snapshot(&c).file.unwrap().size, "16000");
}
#[test]
fn scroll_inside_pending_page_keeps_that_covering_read_current() {
    let (_file, core, mut c) = setup(16000);
    let effects = c.handle(ControllerEvent::Action(Action::ReadViewport {
        offset: 0,
        length: 1024,
    }));
    let (stamp, request) = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::RunCore { stamp, request } = e {
                Some((stamp, request))
            } else {
                None
            }
        })
        .unwrap();
    c.handle(ControllerEvent::Action(Action::ScrollViewport {
        offset: 16,
    }));
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::CoreCompleted {
            stamp,
            reply: core.execute(request, &mut |_| {}),
        }],
    );
    assert_eq!(
        snapshot(&c).page.unwrap().bytes.len(),
        1024,
        "scrolling inside a coalesced read makes it stale but never asks again"
    );
}

#[test]
fn main_empty_view_releases_old_minimap_tile() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.cache.minimap.image().size, [84, 252]);
    let mut s = snapshot(&c);
    s.file = None;
    s.source = None;
    s.page = None;
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| {
            hexforge_native::views::main_window::show(
                ui,
                &style,
                &s,
                &mut h.view,
                &mut h.cache,
                &mut hexforge_native::views::main_window::MainControls::default(),
            );
        },
    );
    output.textures_delta.clear();
    assert_eq!(h.cache.minimap.image().size, [1, 1]);
    assert!(h.view.next_minimap_request().is_none());
    assert!(!h.view.minimap_captured());
}

#[test]
fn transient_narrow_frame_keeps_horizontal_coordinates_finite() {
    let (_file, _core, c) = setup(16000);
    let mut h = Host::new();
    let p = egui::pos2(0.5, 246.0);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(8.0, 252.0),
            )),
            events: vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &snapshot(&c), &mut h.cache);
        },
    );
    output.textures_delta.clear();
    assert!(h.view.frame().horizontal_px.is_finite());
}

#[test]
fn sparse_multi_gigabyte_view_reads_only_the_last_visible_page() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(5 * 1024 * 1024 * 1024).unwrap();
    let core = CoreService::default();
    let mut initial = InitialState::default();
    initial.minimap.mode = MinimapMode::Proportional;
    let mut c = AppController::new(initial);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: file.path().into(),
                discard_unsaved: false,
            },
        ))],
    );
    c.handle(ControllerEvent::Action(Action::ScrollViewport {
        offset: 5_368_709_119,
    }));
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.view.frame().top_row, 335_544_310);
    assert_eq!(h.view.frame().last_visible_row, Some(335_544_319));
    let page = snapshot(&c).page.unwrap();
    assert!(
        page.bytes.len() <= 1024,
        "a 5 GiB source must not be allocated into the presenter"
    );
    assert_eq!(h.cache.minimap.image().pixels.len(), 84 * 252);
    if let Some((_, request)) = h.view.next_minimap_request() {
        match request {
            CoreRequest::ReadPage { length, .. } => assert!(length <= 65_536),
            CoreRequest::ReadMinimapSamples { rows, .. } => assert!(rows.len() <= 32),
            _ => panic!("minimap submitted a non-read operation"),
        }
    }
}

#[test]
fn actual_grid_drag_selects_inclusive_bytes_and_respects_group_gaps() {
    let (_file, core, mut c) = setup(4096);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    h.pointer(&mut c, &core, egui::pos2(210.0, 37.0), Some(true));
    h.pointer(&mut c, &core, egui::pos2(282.0, 37.0), None);
    h.pointer(&mut c, &core, egui::pos2(282.0, 37.0), Some(false));
    assert_eq!(
        snapshot(&c).selection,
        Some(hexforge_controller::controller::ByteSelection { start: 2, end: 5 })
    );
    h.pointer(&mut c, &core, egui::pos2(357.0, 37.0), Some(true)); // after byte 7, before byte 8
    h.pointer(&mut c, &core, egui::pos2(357.0, 37.0), Some(false));
    assert_eq!(
        snapshot(&c).selection,
        Some(hexforge_controller::controller::ByteSelection { start: 2, end: 5 })
    );
}

#[test]
fn double_click_only_opens_the_real_byte_editor_in_edit_mode() {
    let (_file, core, mut c) = setup(4096);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let p = egui::pos2(258.0, 37.0);
    for _ in 0..2 {
        h.pointer(&mut c, &core, p, Some(true));
        h.pointer(&mut c, &core, p, Some(false));
    }
    assert!(snapshot(&c).edit_dialog.is_none());
    // Starting the second user gesture after the read-only probe, rather than
    // injecting six clicks inside egui's triple-click interval. A real mode
    // menu gesture also separates these grid click sequences.
    let s = snapshot(&c);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            time: Some(1.0),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::ToggleEdit,
            context: Default::default(),
        }],
    );
    for _ in 0..2 {
        h.pointer(&mut c, &core, p, Some(true));
        h.pointer(&mut c, &core, p, Some(false));
    }
    let s = snapshot(&c);
    let edit = s
        .edit_dialog
        .expect("An enabled grid double click must use the real edit workflow");
    assert_eq!(edit.offset, 4);
    // App.vue beginEdit opens an empty replacement input, not the current byte.
    assert!(edit.value.is_empty());
    assert_eq!(s.page.as_ref().unwrap().bytes[4], 0x41);
    assert!(
        !s.file.unwrap().dirty,
        "Opening the editor must not write a byte"
    );
}

#[test]
fn disabling_the_minimap_removes_its_column_and_releases_only_preview_data() {
    use hexforge_controller::command::Command;
    let (_file, core, mut c) = setup(4096);
    let mut h = Host::new();
    c.handle(ControllerEvent::Action(Action::Select { start: 4, end: 4 }));
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.view.frame().viewport_rect.width(), 1182.0);
    assert_eq!(h.cache.minimap.image().size, [84, 252]);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: Command::MinimapToggle,
            context: Default::default(),
        }],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.view.frame().viewport_rect.width(), 1267.0);
    assert!(h.view.frame().minimap_rect.is_none());
    assert!(h.view.frame().viewport_box.is_none());
    assert_eq!(h.cache.minimap.image().size, [1, 1]);
    assert!(h.view.next_minimap_request().is_none());
    assert_eq!(
        snapshot(&c).selection,
        Some(hexforge_controller::controller::ByteSelection { start: 4, end: 4 })
    );
    assert_eq!(snapshot(&c).page.unwrap().bytes[4], 0x41);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: Command::MinimapToggle,
            context: Default::default(),
        }],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(h.view.frame().viewport_rect.width(), 1182.0);
    assert_eq!(h.view.frame().minimap_rect.unwrap().width(), 84.0);
}

#[test]
fn three_minimap_wheels_are_coalesced_into_one_authoritative_navigation_frame() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let p = h.view.frame().minimap_rect.unwrap().center();
    let mut input = vec![Event::PointerMoved(p)];
    for _ in 0..3 {
        input.push(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -44.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        });
    }
    let s = snapshot(&c);
    let mut actions = vec![];
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            events: input,
            ..Default::default()
        },
        |ui| {
            actions.extend(h.view.show(ui, &s, &mut h.cache));
        },
    );
    output.textures_delta.clear();
    assert_eq!(
        actions
            .iter()
            .filter(|e| matches!(e, ControllerEvent::Action(Action::ScrollViewport { .. })))
            .count(),
        1
    );
    settle(&mut c, &core, actions);
    assert_eq!(snapshot(&c).viewport_offset, 48);
    assert_eq!(h.view.frame().top_row, 3);
    assert_eq!(
        h.view
            .frame()
            .minimap_projection
            .as_ref()
            .unwrap()
            .editor_row,
        3
    );
}

#[test]
fn row_width_changes_preserve_the_visible_byte_and_recalculate_actual_hit_testing() {
    use hexforge_controller::command::Command;
    let (_file, core, mut c) = setup(4096);
    let mut h = Host::new();
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 32,
        })],
    );
    for (command, expected_width, mini_width) in
        [(Command::Row32, 32, 136.0), (Command::Row16, 16, 84.0)]
    {
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Invoke {
                command,
                context: Default::default(),
            }],
        );
        h.frame(&mut c, &core, vec![], 1.0);
        assert_eq!(snapshot(&c).viewport_offset, 32);
        assert_eq!(snapshot(&c).row_width.bytes(), expected_width);
        assert_eq!(h.view.frame().minimap_rect.unwrap().width(), mini_width);
        assert_eq!(h.cache.minimap.image().size, [mini_width as usize, 252]);
        h.pointer(&mut c, &core, egui::pos2(162.0, 37.0), Some(true));
        h.pointer(&mut c, &core, egui::pos2(162.0, 37.0), Some(false));
        assert_eq!(
            snapshot(&c).selection,
            Some(hexforge_controller::controller::ByteSelection { start: 32, end: 32 })
        );
        let page = snapshot(&c).page.unwrap();
        assert_eq!(page.offset.parse::<u64>().unwrap() % expected_width, 0);
        assert!(page.bytes.len() <= 4096);
    }
}

#[test]
fn minimap_only_failure_keeps_the_real_page_and_retry_does_not_navigate() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Invoke {
            command: hexforge_controller::command::Command::MinimapFit,
            context: Default::default(),
        }],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    let (ticket, _) = h
        .view
        .next_minimap_request()
        .expect("The long fit file needs sparse samples");
    assert!(h
        .view
        .complete_minimap(
            ticket,
            Err(hexforge_core::error::AppError::new(
                hexforge_core::error::ErrorCode::OperationFailed,
                "The preview could not be read.",
                None
            ))
        )
        .is_none());
    assert!(h.view.next_minimap_request().is_none());
    let before = snapshot(&c);
    let mut actions = vec![];
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            actions.extend(h.view.show(ui, &before, &mut h.cache));
        },
    );
    output.textures_delta.clear();
    settle(&mut c, &core, actions);
    let retry = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "↻" => Some(t.pos + t.galley.size() / 2.0),
            _ => None,
        })
        .expect("An inline retry, not a modal failure");
    assert!(output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41")));
    assert!(snapshot(&c).error.is_none());
    assert_eq!(snapshot(&c).page.unwrap().bytes, before.page.unwrap().bytes);
    h.pointer(&mut c, &core, retry, Some(true));
    h.pointer(&mut c, &core, retry, Some(false));
    assert_eq!(
        snapshot(&c).viewport_offset,
        0,
        "The retry button must not bubble into fit navigation"
    );
    let (ticket, request) = h
        .view
        .next_minimap_request()
        .expect("The actual retry click schedules another bounded read");
    assert!(h
        .view
        .complete_minimap(ticket, core.execute(request, &mut |_| {}))
        .is_none());
    let s = snapshot(&c);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    assert!(!output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "↻")));
    assert!(snapshot(&c).error.is_none());
}

#[test]
fn minimap_retry_keeps_source_bounds_style_and_accessible_control_name() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.ctx.enable_accesskit();
    h.frame(&mut c, &core, vec![], 1.0);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 9600,
        })],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    let (ticket, _) = h.view.next_minimap_request().unwrap();
    assert!(h
        .view
        .complete_minimap(
            ticket,
            Err(hexforge_core::error::AppError::new(
                hexforge_core::error::ErrorCode::OperationFailed,
                "The preview could not be read.",
                None
            ))
        )
        .is_none());
    let s = snapshot(&c);
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    let expected = egui::Rect::from_min_size(egui::pos2(1237.0, 6.0), egui::vec2(24.0, 24.0));
    assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect == expected && r.fill == style.palette.panel && r.corner_radius == egui::CornerRadius::same(4))));
    assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect == expected && r.stroke.color == style.palette.border_strong && r.stroke.width == 1.0)));
    assert!(output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| node.role() == egui::accesskit::Role::Button
            && node.label() == Some("Retry minimap preview")));
}

#[test]
fn main_scrollbar_retains_its_grab_offset_and_last_full_page_at_source_heights() {
    for (size, height, eof_offset, last_row) in [(4096, 500.0, 3760, 255), (64, 100.0, 16, 3)] {
        let (_file, core, mut c) = setup(size);
        let mut h = Host::new();
        let mut draw = |events: Vec<Event>| {
            let s = snapshot(&c);
            let mut actions = vec![];
            let mut output = h.ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(1280.0, height),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    actions.extend(h.view.show(ui, &s, &mut h.cache));
                },
            );
            output.textures_delta.clear();
            settle(&mut c, &core, actions);
            output
        };
        let click = |y, pressed| {
            vec![
                Event::PointerMoved(egui::pos2(1275.0, y)),
                Event::PointerButton {
                    pos: egui::pos2(1275.0, y),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]
        };
        let first = draw(vec![]);
        if size == 4096 {
            let thumb = |output: &egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if r.rect.width() == 10.0
                                && r.rect.left() > 1267.0
                                && r.fill
                                    == egui::Color32::from_rgb(201, 209, 217)
                                        .gamma_multiply(0.17) =>
                        {
                            Some(r.rect)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            assert!(thumb(&first).height() > 24.0);
            draw(click(250.0, true));
            let middle = draw(click(250.0, false));
            let old_top = thumb(&middle).top();
            draw(click(old_top + 5.0, true));
            let dragged = draw(vec![Event::PointerMoved(egui::pos2(
                1275.0,
                old_top + 105.0,
            ))]);
            assert!((thumb(&dragged).top() - old_top - 100.0).abs() <= 2.0);
            draw(click(old_top + 105.0, false));
        }
        draw(click(height - 1.0, true));
        draw(click(height - 1.0, false));
        draw(vec![]);
        assert_eq!(snapshot(&c).viewport_offset, eof_offset);
        assert_eq!(h.view.frame().last_visible_row, Some(last_row));
        assert_eq!(h.view.frame().viewport_rect.height(), height);
    }
}

#[test]
fn captured_minimap_drag_prefetches_the_new_end_tile_without_a_full_file_bitmap() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    let draw = |c: &mut AppController, h: &mut Host, events: Vec<Event>| {
        let s = snapshot(c);
        let mut actions = vec![];
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 100.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                actions.extend(h.view.show(ui, &s, &mut h.cache));
            },
        );
        output.textures_delta.clear();
        settle(c, &core, actions);
    };
    draw(&mut c, &mut h, vec![]);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 900 * 16,
        })],
    );
    draw(&mut c, &mut h, vec![]);
    draw(&mut c, &mut h, vec![]);
    let (ticket, request) = h
        .view
        .next_minimap_request()
        .expect("A moved tile needs its surrounding preview rows");
    assert!(h
        .view
        .complete_minimap(ticket, core.execute(request, &mut |_| {}))
        .is_none());
    draw(&mut c, &mut h, vec![]);
    let b = h.view.frame().viewport_box.unwrap();
    assert_eq!(b.height(), 6.0); // floor((100 - 32) / 22) editor rows at 2 mini pixels each.
    let p = egui::pos2(b.center().x, b.top() + 3.0);
    draw(
        &mut c,
        &mut h,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    draw(
        &mut c,
        &mut h,
        vec![Event::PointerMoved(egui::pos2(p.x, 110.0))],
    );
    assert!(h.view.minimap_captured());
    assert_eq!(snapshot(&c).viewport_offset, 997 * 16);
    assert_eq!(h.view.frame().viewport_box.unwrap().top(), 94.0);
    assert_eq!(
        h.view
            .frame()
            .minimap_projection
            .as_ref()
            .unwrap()
            .virtual_origin
            .0,
        1900 * 1024
    );
    draw(&mut c, &mut h, vec![]);
    let (ticket, request) = h
        .view
        .next_minimap_request()
        .expect("Dragging must prefetch the newly exposed EOF tile");
    match &request {
        CoreRequest::ReadPage { offset, length } => {
            assert!(*offset >= 940 * 16);
            assert!(*length <= 1024);
            assert!(*offset + u64::from(*length) <= 16000);
        }
        _ => panic!("The proportional preview must use a bounded page read"),
    }
    assert!(h
        .view
        .complete_minimap(ticket, core.execute(request, &mut |_| {}))
        .is_none());
    draw(&mut c, &mut h, vec![]);
    assert_eq!(h.view.frame().top_row, 997);
    assert_eq!(h.view.frame().last_visible_row, Some(999));
    assert_eq!(h.cache.minimap.image().size, [84, 100]);
    let ink = egui::Color32::from_rgb(174, 192, 208);
    assert!(
        h.cache.minimap.image().pixels[98 * 84..100 * 84].contains(&ink),
        "The last row must be present in the last two preview pixels"
    );
}

#[test]
fn accepted_page_keeps_hex_text_during_prefetched_wheels_and_a_pending_replacement() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let before = snapshot(&c).page.unwrap();
    let s = snapshot(&c);
    let mut actions = vec![];
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            events: vec![
                Event::PointerMoved(egui::pos2(16.0, 100.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -100.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            actions.extend(h.view.show(ui, &s, &mut h.cache));
        },
    );
    output.textures_delta.clear();
    assert!(!actions
        .iter()
        .any(|e| matches!(e, ControllerEvent::Action(Action::ReadViewport { .. }))));
    assert!(output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41")));
    settle(&mut c, &core, actions);
    assert_eq!(snapshot(&c).viewport_offset, 16);
    assert_eq!(snapshot(&c).page.unwrap().bytes, before.bytes);
    // The accepted 4096-byte fixture covers rows 0..255. At row 252,
    // four old rows overlap while six visible rows need a replacement read.
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 252 * 16,
        })],
    );
    let s = snapshot(&c);
    let mut actions = vec![];
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            actions.extend(h.view.show(ui, &s, &mut h.cache));
        },
    );
    output.textures_delta.clear();
    assert!(actions
        .iter()
        .any(|e| matches!(e, ControllerEvent::Action(Action::ReadViewport { .. }))));
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41")),
        "Overlapping accepted bytes stay painted before the read is dispatched"
    );
    assert_eq!(
        output
            .shapes
            .iter()
            .filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41"))
            .count(),
        64
    );
    let pending = actions
        .into_iter()
        .flat_map(|e| c.handle(e))
        .find_map(|e| match e {
            Effect::RunCore { stamp, request }
                if matches!(request, CoreRequest::ReadPage { .. }) =>
            {
                Some((stamp, request))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(snapshot(&c).page.unwrap().bytes, before.bytes);
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::CoreCompleted {
            stamp: pending.0,
            reply: core.execute(pending.1, &mut |_| {}),
        }],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(snapshot(&c).viewport_offset, 252 * 16);
}

#[test]
fn fixed_address_gutter_wheel_keeps_both_source_row_widths_scrollable() {
    use hexforge_controller::command::Command;
    for (command, width) in [(Command::Row16, 16), (Command::Row32, 32)] {
        let (_file, core, mut c) = setup(16000);
        let mut h = Host::new();
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Invoke {
                command,
                context: Default::default(),
            }],
        );
        h.frame(&mut c, &core, vec![], 1.0);
        h.frame(
            &mut c,
            &core,
            vec![
                Event::PointerMoved(egui::pos2(16.0, 100.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -100.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Default::default(),
                },
            ],
            1.0,
        );
        assert_eq!(snapshot(&c).viewport_offset, width);
        assert_eq!(h.view.frame().top_row, 1);
    }
}

#[test]
fn reopened_sources_cannot_repaint_old_bytes_or_keep_old_horizontal_scroll() {
    use hexforge_controller::command::Command;
    use std::io::{Seek, SeekFrom};
    for same_path in [false, true] {
        let (mut original, core, mut c) = setup(16000);
        let mut replacement = tempfile::NamedTempFile::new().unwrap();
        replacement.write_all(&vec![0x42; 16000]).unwrap();
        replacement.flush().unwrap();
        let mut h = Host::new();
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Invoke {
                command: Command::Row32,
                context: Default::default(),
            }],
        );
        h.frame(&mut c, &core, vec![], 1.0);
        h.pointer(&mut c, &core, egui::pos2(1125.0, 247.0), Some(true));
        h.pointer(&mut c, &core, egui::pos2(1125.0, 247.0), Some(false));
        assert_eq!(h.view.frame().horizontal_px, 40.0);
        let old_source = snapshot(&c).source;
        let (stamp, request) = c
            .handle(ControllerEvent::Action(Action::ReadViewport {
                offset: 0,
                length: 1024,
            }))
            .into_iter()
            .find_map(|e| match e {
                Effect::RunCore { stamp, request } => Some((stamp, request)),
                _ => None,
            })
            .unwrap();
        let old_reply = core.execute(request, &mut |_| {});
        if same_path {
            original.as_file_mut().seek(SeekFrom::Start(0)).unwrap();
            original.write_all(&vec![0x42; 16000]).unwrap();
            original.flush().unwrap();
        }
        let path = if same_path {
            original.path()
        } else {
            replacement.path()
        };
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Action(Action::Core(
                CoreRequest::OpenFile {
                    path: path.into(),
                    discard_unsaved: false,
                },
            ))],
        );
        let current = snapshot(&c);
        assert_ne!(current.source, old_source);
        assert_eq!(current.file.as_ref().unwrap().revision, "0");
        assert!(current.page.is_none());
        let mut actions = vec![];
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                actions.extend(h.view.show(ui, &current, &mut h.cache));
            },
        );
        output.textures_delta.clear();
        assert_eq!(h.view.frame().horizontal_px, 0.0);
        assert!(!output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41")));
        assert!(actions
            .iter()
            .any(|e| matches!(e, ControllerEvent::Action(Action::ReadViewport { .. }))));
        c.handle(ControllerEvent::CoreCompleted {
            stamp,
            reply: old_reply,
        });
        assert!(
            snapshot(&c).page.is_none(),
            "Even the same pristine path/revision needs its new identity's measured page"
        );
        settle(&mut c, &core, actions);
        let s = snapshot(&c);
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                h.view.show(ui, &s, &mut h.cache);
            },
        );
        output.textures_delta.clear();
        assert!(output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "42")));
        assert!(!output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41")));
        assert!(snapshot(&c).error.is_none());
    }
}

#[test]
fn edited_revision_keeps_the_visible_byte_and_rejects_an_older_read_completion() {
    use hexforge_controller::command::Command;
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 160,
        })],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    let (stamp, request) = c
        .handle(ControllerEvent::Action(Action::ReadViewport {
            offset: 160,
            length: 1024,
        }))
        .into_iter()
        .find_map(|e| match e {
            Effect::RunCore { stamp, request } => Some((stamp, request)),
            _ => None,
        })
        .unwrap();
    let old_reply = core.execute(request, &mut |_| {});
    settle(
        &mut c,
        &core,
        vec![
            ControllerEvent::Invoke {
                command: Command::ToggleEdit,
                context: Default::default(),
            },
            ControllerEvent::Action(Action::Core(CoreRequest::EditByte {
                offset: 160,
                value: 0x42,
            })),
        ],
    );
    c.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: old_reply,
    });
    h.frame(&mut c, &core, vec![], 1.0);
    assert_eq!(snapshot(&c).viewport_offset, 160);
    assert_eq!(snapshot(&c).file.as_ref().unwrap().revision, "1");
    let s = snapshot(&c);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(
        |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "42" && t.pos.x == 160.0)
    ));
    assert!(
        h.cache
            .minimap
            .image()
            .pixels
            .contains(&egui::Color32::from_rgb(240, 136, 62)),
        "The accepted edited page also supplies the orange minimap glyph"
    );
    h.pointer(&mut c, &core, egui::pos2(162.0, 37.0), Some(true));
    h.pointer(&mut c, &core, egui::pos2(162.0, 37.0), Some(false));
    assert_eq!(
        snapshot(&c).selection,
        Some(hexforge_controller::controller::ByteSelection {
            start: 160,
            end: 160
        })
    );
    assert!(snapshot(&c).file.unwrap().dirty);
}

#[test]
fn system_theme_change_recolors_accepted_hex_text_and_the_existing_preview() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    h.frame(&mut c, &core, vec![], 1.0);
    let before = h.cache.minimap.image().pixels.clone();
    c.handle(ControllerEvent::SystemThemeChanged(
        hexforge_controller::controller::Theme::Light,
    ));
    let s = snapshot(&c);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "41" && t.fallback_color == egui::Color32::from_rgb(31, 35, 40))));
    assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "OFFSET" && t.fallback_color == egui::Color32::from_rgb(87, 96, 106))));
    assert_ne!(h.cache.minimap.image().pixels, before);
    assert!(h
        .cache
        .minimap
        .image()
        .pixels
        .contains(&egui::Color32::from_rgb(66, 95, 119)));
    assert_eq!(snapshot(&c).page.unwrap().revision, "0");
    assert_eq!(snapshot(&c).viewport_offset, 0);
}

#[test]
fn saved_binary_copy_retains_the_visible_bytes_and_selection_in_the_actual_view() {
    use hexforge_controller::command::Command;
    let (original, core, mut c) = setup(16000);
    let mut h = Host::new();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("copy.bin");
    settle(
        &mut c,
        &core,
        vec![
            ControllerEvent::Invoke {
                command: Command::ToggleEdit,
                context: Default::default(),
            },
            ControllerEvent::Action(Action::Core(CoreRequest::EditByte {
                offset: 160,
                value: 0x42,
            })),
            ControllerEvent::Action(Action::Navigate {
                start: 160,
                end: 160,
            }),
        ],
    );
    h.frame(&mut c, &core, vec![], 1.0);
    let old_source = snapshot(&c).source;
    let request_id = c
        .handle(ControllerEvent::Invoke {
            command: Command::SaveAs,
            context: Default::default(),
        })
        .into_iter()
        .find_map(|e| match e {
            Effect::OpenPicker { request_id, .. } => Some(request_id),
            _ => None,
        })
        .unwrap();
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::PickerCompleted {
            request_id,
            path: Some(destination.clone()),
        }],
    );
    assert_ne!(snapshot(&c).source, old_source);
    assert_eq!(snapshot(&c).viewport_offset, 160);
    assert!(!snapshot(&c).file.as_ref().unwrap().dirty);
    assert!(!snapshot(&c).can_undo);
    h.frame(&mut c, &core, vec![], 1.0);
    let s = snapshot(&c);
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 252.0),
            )),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(
        |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "42" && t.pos.x == 160.0)
    ));
    assert_eq!(h.view.frame().top_row, 10);
    assert_eq!(
        snapshot(&c).selection,
        Some(hexforge_controller::controller::ByteSelection {
            start: 160,
            end: 160
        })
    );
    assert_eq!(std::fs::read(original.path()).unwrap()[160], 0x41);
    assert_eq!(std::fs::read(destination).unwrap()[160], 0x42);
    assert!(snapshot(&c).error.is_none());
}

#[test]
fn first_measured_native_frame_requests_one_stamped_bounded_main_page() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    // Clear the accepted page by a real same-path reopen; no frontend/layout
    // timer is allowed to manufacture a page before its first measured frame.
    let path = snapshot(&c).file.unwrap().path;
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: path.into(),
                discard_unsaved: false,
            },
        ))],
    );
    assert!(snapshot(&c).page.is_none());
    let s = snapshot(&c);
    let mut actions = vec![];
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 500.0),
            )),
            ..Default::default()
        },
        |ui| {
            actions.extend(h.view.show(ui, &s, &mut h.cache));
        },
    );
    output.textures_delta.clear();
    assert_eq!(h.view.frame().viewport_rect.height(), 500.0);
    assert_eq!(
        actions
            .iter()
            .filter(|e| matches!(e, ControllerEvent::Action(Action::ReadViewport { .. })))
            .count(),
        1
    );
    let effects: Vec<_> = actions
        .into_iter()
        .flat_map(|event| c.handle(event))
        .collect();
    let runs: Vec<_> = effects
        .into_iter()
        .filter_map(|e| match e {
            Effect::RunCore {
                stamp,
                request: CoreRequest::ReadPage { offset, length },
            } => Some((stamp, offset, length)),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1, 0);
    assert!(runs[0].2 > 0 && runs[0].2 <= 1_048_576);
    assert!(runs[0].0.request_id.0 > 0);
    assert_eq!(runs[0].0.source, snapshot(&c).source);
}

#[test]
fn proportional_drag_uses_the_container_origin_not_the_shifted_preview_content() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    settle(
        &mut c,
        &core,
        vec![ControllerEvent::Action(Action::ScrollViewport {
            offset: 35 * 16,
        })],
    );
    let draw = |c: &mut AppController, h: &mut Host, events: Vec<Event>| {
        let s = snapshot(c);
        let mut actions = vec![];
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 120.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(
                    egui::Rect::from_min_size(egui::pos2(0.0, 20.0), egui::vec2(1280.0, 100.0)),
                ));
                actions.extend(h.view.show(&mut child, &s, &mut h.cache));
            },
        );
        output.textures_delta.clear();
        settle(c, &core, actions);
    };
    draw(&mut c, &mut h, vec![]);
    assert_eq!(h.view.frame().minimap_rect.unwrap().top(), 20.0);
    let p = egui::pos2(1220.0, 24.0);
    draw(
        &mut c,
        &mut h,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    assert!(h.view.minimap_captured());
    draw(
        &mut c,
        &mut h,
        vec![Event::PointerMoved(egui::pos2(p.x, 34.0))],
    );
    assert_eq!(snapshot(&c).viewport_offset, 141 * 16);
    let b = h.view.frame().viewport_box.unwrap();
    assert!((b.top() - 33.3).abs() < 0.01);
}

#[test]
fn huge_metadata_marker_click_navigates_exactly_without_allocating_the_logical_file() {
    use hexforge_core::{dto::FileInfoDto, operations::CoreReply};
    let mut c = AppController::new(InitialState::default());
    let mut h = Host::new();
    // A typed metadata boundary fixture, like the source component's 2^60
    // file-size prop. No OS can create that fixture here; no bytes are read.
    let stamp = c
        .handle(ControllerEvent::Action(Action::Core(
            CoreRequest::OpenFile {
                path: "huge.bin".into(),
                discard_unsaved: false,
            },
        )))
        .into_iter()
        .find_map(|e| match e {
            Effect::RunCore { stamp, .. } => Some(stamp),
            _ => None,
        })
        .unwrap();
    c.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: Ok(CoreReply::FileInfo(FileInfoDto {
            name: "huge.bin".into(),
            path: "huge.bin".into(),
            size: "1152921504606846976".into(),
            revision: "0".into(),
            dirty: false,
        })),
    });
    let mut s = snapshot(&c);
    s.matches = vec![864_691_128_455_135_232];
    s.search.match_length = 1;
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 500.0),
            )),
            ..Default::default()
        },
        |ui| {
            h.view.show(ui, &s, &mut h.cache);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect.left() >= 1268.0 && r.rect.top() == 374.0 && r.rect.height() == 2.0)), "The file-wide three-quarter search marker stays in the overview ruler");
    let p = egui::pos2(1275.0, 374.0);
    let mut actions = vec![];
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 500.0),
            )),
            events: vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            actions.extend(h.view.show(ui, &s, &mut h.cache));
        },
    );
    output.textures_delta.clear();
    // 362px / 476px effective thumb travel across 2^56 - 21 top rows.
    assert_eq!(h.view.frame().top_row, 54_800_103_028_844_338);
    assert!(actions.iter().any(|e| matches!(
        e,
        ControllerEvent::Action(Action::ScrollViewport {
            offset: 876_801_648_461_509_408
        })
    )));
    assert!(actions.iter().any(|e| matches!(e, ControllerEvent::Action(Action::ReadViewport { length, .. }) if *length > 0 && *length <= 1_048_576)));
    assert_eq!(h.cache.minimap.image().size, [84, 500]);
}

#[test]
fn overview_markers_keep_source_minimum_height_layer_order_and_eof_clip() {
    for theme in [
        hexforge_controller::controller::Theme::Dark,
        hexforge_controller::controller::Theme::Light,
    ] {
        let (_file, _core, mut c) = setup(16000);
        let mut h = Host::new();
        c.handle(ControllerEvent::SystemThemeChanged(theme));
        let mut s = snapshot(&c);
        s.matches = vec![64, 15999];
        s.search.match_length = 1;
        s.modified_overview = hexforge_core::dto::ModifiedOverviewDto {
            bin_count: 1024,
            bins: vec![0],
        };
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 252.0),
                )),
                ..Default::default()
            },
            |ui| {
                h.view.show(ui, &s, &mut h.cache);
            },
        );
        output.textures_delta.clear();
        let search = egui::Color32::from_rgba_unmultiplied(189, 166, 74, 166);
        let modified = egui::Color32::from_rgba_unmultiplied(240, 136, 62, 230);
        let markers: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(r)
                    if r.rect.left() == 1268.0 && [search, modified].contains(&r.fill) =>
                {
                    Some((r, shape.clip_rect))
                }
                _ => None,
            })
            .collect();
        assert_eq!(markers.len(), 3);
        assert_eq!(
            markers
                .iter()
                .map(|(r, _)| (r.rect.top(), r.rect.height(), r.fill))
                .collect::<Vec<_>>(),
            [
                (1.0, 2.0, search),
                (251.0, 2.0, search),
                (0.0, 2.0, modified)
            ],
            "The source groups all search elements before higher-priority modified elements"
        );
        for (r, clip) in markers {
            assert_eq!(r.rect.width(), 12.0);
            assert_eq!(r.corner_radius, egui::CornerRadius::same(1));
            assert_eq!(
                clip.bottom(),
                252.0,
                "An EOF marker must not paint over the panel below the viewer"
            );
        }
    }
}

#[test]
fn main_scrollbar_paints_the_source_content_box_border_without_changing_travel() {
    let (_file, core, mut c) = setup(16000);
    let mut h = Host::new();
    for (row, top) in [(0, 0.0), (495, 240.0), (979, 476.0)] {
        settle(
            &mut c,
            &core,
            vec![ControllerEvent::Action(Action::ScrollViewport {
                offset: row * 16,
            })],
        );
        let s = snapshot(&c);
        let mut output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 500.0),
                )),
                ..Default::default()
            },
            |ui| {
                h.view.show(ui, &s, &mut h.cache);
            },
        );
        output.textures_delta.clear();
        let thumb = output.shapes.iter().find(|shape| matches!(&shape.shape, egui::Shape::Rect(r) if r.rect.left() == 1269.0 && r.rect.width() == 10.0 && r.fill == egui::Color32::from_rgb(201, 209, 217).gamma_multiply(0.17))).unwrap();
        let egui::Shape::Rect(r) = &thumb.shape else {
            unreachable!()
        };
        assert_eq!(r.rect.top(), top);
        // Source height is a 24px content box plus two 1px borders; the
        // logical 476px travel still uses the unchanged 24px thumb model.
        assert_eq!(r.rect.height(), 26.0);
        assert_eq!(thumb.clip_rect.bottom(), 500.0);
        assert_eq!(h.view.frame().top_row, row);
    }
}
