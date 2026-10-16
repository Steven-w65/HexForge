//! Numeric layout observations from the actual 1280x800/DPR1 source capture,
//! not egui defaults. Font rasterization does not excuse missing CSS spacing.
//! Intrinsic OS-font advances can differ; CSS padding and gaps must not.
#[path = "support/capture_scene.rs"]
mod capture_scene;
#[path = "support/capture_spec.rs"]
#[allow(dead_code)]
mod capture_spec;
mod support;
use eframe::egui;
use hexforge_controller::controller::InitialState;
use hexforge_native::app::NativeApp;
use support::ui_harness::NativeUiHarness;

fn file_menu_frame() -> support::ui_harness::CapturedFrame {
    file_menu_frame_with_prior_keyboard(false)
}
fn file_menu_frame_with_prior_keyboard(prior_keyboard: bool) -> support::ui_harness::CapturedFrame {
    file_menu_frame_with_fonts(prior_keyboard, false)
}

fn file_menu_frame_with_fonts(
    prior_keyboard: bool,
    embedded_shortcuts: bool,
) -> support::ui_harness::CapturedFrame {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    if embedded_shortcuts {
        // Exercise real text layout with a different installed/fallback metric,
        // without changing production font selection or requiring OS changes.
        h.frame(Default::default(), |_| {});
        let mut definitions = h.context.fonts(|fonts| fonts.definitions().clone());
        definitions.families.insert(
            egui::FontFamily::Name("HexForge OS Monospace".into()),
            vec!["HexForge Regular".into()],
        );
        h.context.set_fonts(definitions);
    }
    for (time, events) in [
        (
            0.0,
            if prior_keyboard {
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            } else {
                vec![]
            },
        ),
        (
            0.001,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::ALT),
                egui::Event::Key {
                    key: egui::Key::F,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                },
            ],
        ),
    ] {
        h.main_frame(
            &mut app,
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
        );
    }
    // A sizing pass is necessary, but elapsed animation must not be necessary:
    // the production source CSS shows the real popup immediately, without fade.
    h.main_frame(
        &mut app,
        egui::RawInput {
            time: Some(0.002),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    )
}

#[test]
fn menu_has_source_grid_width_and_shortcut_spacing() {
    let frame = file_menu_frame();
    assert_menu_grid_width_and_shortcut_spacing(&frame);
}

#[test]
fn menu_grid_preserves_css_spacing_with_different_shortcut_font_metrics() {
    let frame = file_menu_frame_with_fonts(false, true);
    assert_menu_grid_width_and_shortcut_spacing(&frame);
}

fn assert_menu_grid_width_and_shortcut_spacing(frame: &support::ui_harness::CapturedFrame) {
    let open = frame
        .widgets
        .iter()
        .find(|w| w.label == "Open File…")
        .expect("Actual menu item");
    let text = |name: &str| {
        frame
            .output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == name => Some(text),
                _ => None,
            })
            .expect("Actual painted menu text")
    };
    // Export is the widest File-menu label and has the same-length shortcut
    // as Save As. Measure the painted, unrounded advances independently of
    // menu_width; do not pin the installed NSimSun version's glyph widths.
    let label = text("Export Parsed Results as CSV…");
    let shortcut = text("Ctrl+Shift+E");
    let label_width = label.galley.rows[0].size.x;
    let shortcut_width = shortcut.galley.rows[0].size.x;
    assert!(label_width > 120.0);
    assert_eq!(label.galley.job.sections[0].format.font_id.size, 12.0);
    assert_eq!(shortcut.galley.job.sections[0].format.font_id.size, 12.0);
    // Original CSS: two 9px paddings, 14px check column, two 7px gaps.
    let intrinsic_width = label_width + shortcut_width + 18.0 + 14.0 + 14.0;
    assert!(
        (open.rect.width() - intrinsic_width).abs() < 0.5,
        "CSS grid must contain text advances + 46px, not generic menu padding: label={label_width}, shortcut={shortcut_width}, row={:?}",
        open.rect
    );
    assert!((label.pos.x - open.rect.left() - 30.0).abs() < 0.01);
    assert!((open.rect.right() - shortcut.pos.x - shortcut.galley.size().x - 9.0).abs() < 0.01);
    assert!((shortcut.pos.x - label.pos.x - label_width - 7.0).abs() < 0.5);
    assert!((open.rect.left() - 134.0).abs() < 0.6);
    assert_eq!(open.rect.top(), 38.0);
    assert_eq!(open.rect.height(), 33.0);
}

#[test]
fn menu_is_fully_visible_without_an_egui_fade() {
    let frame = file_menu_frame();
    let open = frame
        .output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == "Open File…" => Some(t),
            _ => None,
        })
        .expect("Real menu text after its sizing pass");
    assert_eq!(
        open.override_text_color.unwrap_or(open.fallback_color),
        egui::Color32::from_rgb(201, 209, 217),
        "source CSS has no menu fade"
    );
}

#[test]
fn keyboard_opened_menu_keeps_the_first_enabled_row_visibly_focused() {
    let frame = file_menu_frame_with_prior_keyboard(true);
    let open = frame
        .widgets
        .iter()
        .find(|w| w.label == "Open File…")
        .unwrap();
    assert!(frame.output.shapes.iter().any(|c| matches!(&c.shape,
        egui::Shape::Rect(r) if r.rect == open.rect && r.fill == egui::Color32::from_rgb(37, 44, 53))),
        "The keyboard-focused source menu row has a hover fill, not only a logical index");
}

#[test]
fn alt_access_key_after_pointer_intent_preserves_focus_without_inventing_focus_visible() {
    let frame = file_menu_frame();
    let open = frame
        .widgets
        .iter()
        .find(|w| w.label == "Open File…")
        .unwrap();
    assert!(frame.output.shapes.iter().any(|c| matches!(&c.shape,
        egui::Shape::Rect(r) if r.rect == open.rect && r.fill == egui::Color32::TRANSPARENT)),
        "Actual focused reference after pointer then Alt+F has transparent fill until unmodified navigation");
    let focused = frame
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .focus;
    assert!(
        frame
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(id, node)| *id == focused && node.label() == Some("Open File…")),
        "Appearance must not remove real keyboard focus or navigation"
    );
}

#[test]
fn parsed_result_flyout_keeps_its_source_font_and_two_column_inset() {
    let h = NativeUiHarness::new();
    let style = hexforge_native::style::HexForgeStyle::for_theme(
        hexforge_controller::controller::Theme::Dark,
        1.0,
    );
    let frame = h.frame(Default::default(), |ui| {
        ui.set_width(230.0);
        hexforge_native::widgets::menu::result_item(ui, &style, "header.id", "0x0020000000000001");
    });
    let text = |name: &str| {
        frame
            .output
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == name => Some(t),
                _ => None,
            })
            .unwrap()
    };
    let label = text("header.id");
    assert_eq!(label.pos.x, 9.0, "result rows have no checkmark column");
    let offset = text("0x0020000000000001");
    assert_eq!(
        offset.galley.job.sections[0].format.font_id,
        hexforge_native::style::fonts::regular(11.0)
    );
    assert_eq!(
        offset.override_text_color.unwrap_or(offset.fallback_color),
        style.palette.muted
    );
}

#[test]
fn focused_menu_uses_source_hover_fill_without_a_generic_focus_outline() {
    use hexforge_native::widgets::button::{ButtonRole, ButtonSpec, VisualState};
    let style = hexforge_native::style::HexForgeStyle::for_theme(
        hexforge_controller::controller::Theme::Dark,
        1.0,
    );
    let visual = ButtonSpec::new("Open File…")
        .role(ButtonRole::Menu)
        .visual(&style, VisualState::Focused);
    assert_eq!(visual.fill, style.palette.hover);
    assert_eq!(
        visual.focus, None,
        "source menu focus-visible sets outline:none"
    );
}

#[test]
fn unopened_editor_matches_source_header_form_and_empty_list_geometry() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    app.invoke(
        hexforge_controller::command::Command::TemplateEditor,
        Default::default(),
    );
    assert!(!app.editor_snapshot().template_state.active);
    let frame = h.editor_frame(&mut app, Default::default());
    let text = |label: &str| {
        frame
            .output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing actual editor text: {label}"))
    };
    let header = text("Template Editor");
    assert!((header.pos.y + header.galley.rect.center().y - 20.5).abs() < 0.1);
    let state = text(" · No template");
    assert!(
        (state.pos.x + state.galley.rect.left() - 64.8125).abs() < 0.3,
        "status follows the name rather than stretching across the header: {:?}",
        state.pos
    );
    assert!((state.pos.y + state.galley.rect.center().y - 38.0).abs() < 0.1);
    let input = frame
        .output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(shape)
                if (shape.rect.width() - 477.0).abs() < 0.1 && shape.rect.height() == 28.0 =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .expect("Actual template name input frame");
    assert_eq!(
        input.top(),
        84.0,
        "source's 43px header and 14px label line box"
    );
    let note = text("Fields parse in order. References use earlier result paths.");
    assert!((note.pos.y + note.galley.rect.center().y - 128.0).abs() < 0.1);
    let empty = text("No fields defined. Add a field to start a template.");
    assert!(
        (empty.pos.y + empty.galley.rect.center().y - 164.0).abs() < 0.1,
        "source's field-list top is 144px with a 12px paragraph margin: {:?}",
        empty.pos
    );
}

#[test]
fn empty_main_controls_match_source_client_geometry() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    let frame = h.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    );
    let file = frame.widgets.iter().find(|w| w.label == "File").unwrap();
    assert!(
        (file.rect.left() - 129.453125).abs() < 0.25,
        "CSS brand letter spacing was lost: {:?}",
        file.rect
    );
    let rows = frame.widgets.iter().find(|w| w.label == "16").unwrap();
    assert_eq!(
        rows.rect.height(),
        16.0,
        "source inline button is 14px line height plus 1px padding, not 20px"
    );
    assert!((rows.rect.top() - 781.0).abs() < 0.1);
    assert!(
        (rows.rect.left() - 320.234375).abs() < 0.3,
        "source Rows includes its trailing space: {:?}",
        rows.rect
    );
    let open = frame
        .widgets
        .iter()
        .find(|w| w.label == "Open File")
        .unwrap();
    assert!((open.rect.left() - 595.578125).abs() < 0.25);
    assert_eq!(open.rect.top(), 339.0);
    assert_eq!(open.rect.height(), 28.0);
}

#[test]
fn file_size_labels_preserve_javascript_half_up_rounding_and_precise_status_kib() {
    use hexforge_native::views::file_strip::format_size;
    assert_eq!(
        format_size(16000, true),
        "15.63 KiB",
        "FileInfoBar.toFixed(2) is not Rust's ties-to-even formatting"
    );
    assert_eq!(format_size(16000, false), "15.625 KiB");
    assert_eq!(format_size(1023, true), "1023 B");
    assert_eq!(format_size(1048576, true), "1.00 MiB");
    assert_eq!(format_size(16_777_216_000, true), "15.63 GiB");
}

#[test]
fn floating_search_controls_keep_source_flex_widths_and_detail_inset() {
    let frame = floating_bar_frame("search");
    let search = frame.widgets.iter().find(|w| w.label == "Search").unwrap();
    assert!(
        (search.rect.left() - 971.390625).abs() < 0.1,
        "source input is the remaining flex width, actual {:?}",
        search.rect
    );
    assert!((search.rect.width() - 53.609375).abs() < 0.1);
    assert_eq!(search.rect.top(), 91.0);
    let detail = frame
        .output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t)
                if t.galley.text() == "Enter hexadecimal bytes, then press Enter." =>
            {
                Some(t)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        detail.pos.x, 794.0,
        "source detail has its own 2px horizontal inset"
    );
    let close = frame
        .output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == "×" => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(close.fallback_color, egui::Color32::from_rgb(139, 148, 158));
}

#[test]
fn floating_goto_keeps_source_flex_width_and_shared_detail_style() {
    let frame = floating_bar_frame("goto");
    let go = frame.widgets.iter().find(|w| w.label == "Go").unwrap();
    assert!(
        (go.rect.left() - 1107.796875).abs() < 0.1,
        "actual Go bounds {:?}",
        go.rect
    );
    assert!((go.rect.width() - 27.203125).abs() < 0.1);
    assert_eq!(go.rect.top(), 91.0);
    let detail = frame
        .output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t)
                if t.galley.text() == "Enter a decimal or 0x-prefixed hexadecimal offset." =>
            {
                Some(t)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(detail.pos.x, 794.0);
    assert_eq!(detail.galley.job.sections[0].format.font_id.size, 11.0);
}

#[test]
fn floating_bar_detail_uses_its_fifteen_pixel_source_line_not_widget_minimum() {
    fn panel_rect(shape: &egui::Shape) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().find_map(panel_rect),
            egui::Shape::Rect(r)
                if r.fill == egui::Color32::from_rgb(21, 25, 30)
                    && r.rect.width() > 380.0
                    && r.rect.width() < 400.0
                    && r.rect.top() < 120.0
                    && r.corner_radius == egui::CornerRadius::same(5) =>
            {
                Some(r.rect)
            }
            _ => None,
        }
    }
    for scene in ["search", "goto"] {
        let frame = floating_bar_frame(scene);
        let panel = frame
            .output
            .shapes
            .iter()
            .find_map(|c| panel_rect(&c.shape))
            .expect("Actual painted floating frame");
        assert!((panel.top() - 83.0).abs() < 1.0, "{panel:?}");
        assert!((panel.height()-63.0).abs()<1.0,"{scene}: source 16px frame/padding + 27px control + 5px gap + 15px detail, not a 27px interactive row: {panel:?}");
        let detail = frame
            .output
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text().starts_with("Enter ") => Some(t),
                _ => None,
            })
            .expect("Actual help text");
        assert!(
            (detail.pos.y + detail.galley.rect.center().y - 130.5).abs() < 0.1,
            "Source detail text line must remain centered in its 15px row: {:?}",
            detail.pos
        );
    }
}

#[test]
fn file_strip_uses_source_svg_paths_and_hides_path_at_narrow_widths() {
    use hexforge_native::{style::HexForgeStyle, views::file_strip::FileStrip};
    let h = NativeUiHarness::new();
    let file = hexforge_core::dto::FileInfoDto {
        name: "parity.bin".into(),
        path: "C:\\fixture\\parity.bin".into(),
        size: "16000".into(),
        revision: "0".into(),
        dirty: false,
    };
    let style = HexForgeStyle::for_theme(hexforge_controller::controller::Theme::Dark, 1.0);
    for width in [1280.0, 600.0] {
        let mut strip = FileStrip::default();
        let frame = h.frame(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 42.0),
                )),
                ..Default::default()
            },
            |ui| {
                strip.show(ui, &style, Some(&file));
            },
        );
        let paths = frame
            .output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Path(p) => Some(p),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            paths.iter().any(|p| p.points
                == vec![
                    egui::pos2(13.75, 14.875),
                    egui::pos2(18.75, 14.875),
                    egui::pos2(21.25, 17.375),
                    egui::pos2(21.25, 26.125),
                    egui::pos2(13.75, 26.125),
                    egui::pos2(13.75, 14.875)
                ]),
            "source 15px SVG keeps its 24-unit path, not an approximate polygon"
        );
        assert!(
            paths.iter().any(|p| p.points.len() == 3
                && (p.points[1].x - (width - 23.0)).abs() < 0.01
                && (p.points[1].y - 22.25).abs() < 0.01),
            "source toggle uses its centered 14px SVG, not a displaced text caret"
        );
        if width == 600.0 {
            assert!(!frame.output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==file.path || t.galley.text()=="·")),
                "source <=720px hides path and separators without hiding file name/size");
        }
    }
}

#[test]
fn modified_badge_follows_intrinsic_summary_instead_of_the_right_edge() {
    use hexforge_native::{style::HexForgeStyle, views::file_strip::FileStrip};
    let h = NativeUiHarness::new();
    let file = hexforge_core::dto::FileInfoDto {
        name: "parity.bin".into(),
        path: "C:\\fixture\\parity.bin".into(),
        size: "16000".into(),
        revision: "1".into(),
        dirty: true,
    };
    let style = HexForgeStyle::for_theme(hexforge_controller::controller::Theme::Dark, 1.0);
    for (width, expected) in [(1280.0, 361.2), (600.0, 188.4)] {
        let mut strip = FileStrip::default();
        let frame = h.frame(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 42.0),
                )),
                ..Default::default()
            },
            |ui| strip.show(ui, &style, Some(&file)),
        );
        let badge = frame
            .output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Modified" => Some(t),
                _ => None,
            })
            .unwrap();
        // Source text advances: 32px + 72px filename, 7px gaps, 6.6px
        // separators, 59.4px size, 138.6px path, then 6px dot/5px gap.
        assert!(
            (badge.pos.x - expected).abs() < 1.0,
            "source flex summary at width {width}: expected {expected}, actual {:?}",
            badge.pos
        );
    }
}

fn floating_bar_frame(scene: &str) -> support::ui_harness::CapturedFrame {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new(scene, "dark", [1280, 800], 1.0).unwrap(),
    )
    .unwrap();
    for _ in 0..3 {
        h.main_frame(
            &mut app,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
        );
    }
    h.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    )
}

#[test]
fn empty_prompt_matches_source_box_sizing_and_text_line_height() {
    let stage = egui::Rect::from_min_max(egui::pos2(0.0, 76.0), egui::pos2(1280.0, 552.0));
    let layout = hexforge_native::views::main_window::empty_layout(stage);
    assert_eq!(
        layout.icon,
        egui::Rect::from_min_size(egui::pos2(618.0, 261.0), egui::vec2(44.0, 44.0))
    );
    // CSS rounds to 1/64px while native local positions remain f32; compare
    // within that subpixel quantum, not the unequal arithmetic bit patterns.
    assert!((layout.prompt.left() - 575.2).abs() < 0.02);
    assert!((layout.prompt.width() - 129.6).abs() < 0.02);
    assert_eq!(layout.prompt.top(), 312.0);
    assert_eq!(layout.prompt.height(), 16.0);
    assert_eq!(layout.button.top(), 339.0);
}

#[test]
fn results_empty_text_uses_source_center_and_css_heading_spacing() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    let frame = h.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    );
    let texts = frame
        .output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    let heading = texts
        .iter()
        .find(|t| t.galley.text() == "PARSED RESULTS")
        .unwrap();
    assert_eq!(
        heading.galley.job.sections[0].format.extra_letter_spacing, 0.88,
        "source .08em at 11px"
    );
    let message = texts
        .iter()
        .find(|t| t.galley.text() == "No template loaded.")
        .unwrap();
    assert!(
        (message.pos.y + message.galley.size().y / 2.0 - 680.5).abs() < 0.1,
        "source flex center after the 1px panel border: {:?}",
        message.pos
    );
    // Center-aligned egui galleys have a negative local X origin. Inspect the
    // actual laid-out bounds, not size()/2, so a double centering is caught.
    assert!(
        (message.pos.x + message.galley.rect.center().x - 640.0).abs() < 0.1,
        "source flex center is the full panel center: position {:?}, bounds {:?}",
        message.pos,
        message.galley.rect
    );
}
#[test]
fn positioned_menu_border_overlays_file_strip_without_shifting_the_workspace() {
    // Source nav is positioned, with 34px content height plus its border.
    // CSS paints that border over the first pixel of the normal-flow file
    // strip; it does not insert an extra grid row or subtract menu content.
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    let frame = h.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    );
    let style = hexforge_native::style::HexForgeStyle::for_theme(app.main_snapshot().theme, 1.0);
    let file_fill = frame.output.shapes.iter().position(|s| matches!(&s.shape,
        egui::Shape::Rect(r) if r.rect == egui::Rect::from_min_size(egui::pos2(0.0, 34.0), egui::vec2(1280.0, 42.0))
            && r.fill == style.palette.surface)).unwrap();
    let border = frame.output.shapes.iter().position(|s| matches!(&s.shape,
        egui::Shape::LineSegment { points, stroke } if points == &[egui::pos2(0.0, 34.5), egui::pos2(1280.0, 34.5)]
            && stroke.color == style.palette.border));
    assert!(border.is_some(), "source nav's border is at y34, not y33");
    assert!(
        border.unwrap() > file_fill,
        "the positioned nav border paints above the file strip's background"
    );
    assert!(!frame.output.shapes.iter().any(|s| matches!(&s.shape,
        egui::Shape::LineSegment { points, .. } if points == &[egui::pos2(0.0, 33.5), egui::pos2(1280.0, 33.5)])));
    let message = frame
        .output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "Drop a binary file" => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        message.pos.y + message.galley.rect.center().y,
        320.0,
        "correcting a painted border must not move the stage or its controls"
    );
}

#[test]
fn pending_template_actions_follow_the_source_centered_grid_and_padding() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    let spec =
        capture_spec::CaptureSpec::new("pending-template", "dark", [1280, 800], 1.0).unwrap();
    capture_scene::prepare_scene(&mut app, &spec).unwrap();
    let frame = h.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    );
    let editor = frame
        .widgets
        .iter()
        .find(|w| w.label == "Template Editor")
        .unwrap();
    let apply = frame
        .widgets
        .iter()
        .find(|w| w.label == "Apply Template")
        .unwrap();
    assert!(
        (editor.rect.left() - 511.59375).abs() < 0.5,
        "source place-content:center, not a left-aligned generic horizontal UI: {:?}",
        editor.rect
    );
    assert!(
        (editor.rect.width() - 128.0).abs() < 0.1,
        "source 10px horizontal padding on each side"
    );
    assert!((apply.rect.left() - 647.59375).abs() < 0.5);
    assert!((apply.rect.width() - 120.8125).abs() < 0.1);
    assert!((editor.rect.top() - 681.5).abs() < 0.1 && (apply.rect.top() - 681.5).abs() < 0.1);
    assert_eq!((editor.rect.height(), apply.rect.height()), (27.0, 27.0));
    for widget in [editor, apply] {
        assert!(frame.output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect == widget.rect && r.corner_radius == egui::CornerRadius::same(4))),
            "results actions retain their source 4px corner radius");
    }
    let message = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t)
                if t.galley.text() == "Template: \"structures.json\" loaded, pending apply." =>
            {
                Some(t)
            }
            _ => None,
        })
        .unwrap();
    assert!((message.pos.y + message.galley.rect.center().y - 661.5).abs() < 0.1);
}

#[test]
fn populated_editor_reserves_its_source_scrollbar_slot_and_nested_card_gaps() {
    use hexforge_controller::template::editing::FieldPath;
    use hexforge_native::views::field_card::control_id;
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    let spec = capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap();
    capture_scene::prepare_scene(&mut app, &spec).unwrap();
    let mut frame = None;
    for _ in 0..5 {
        frame = Some(h.editor_frame(&mut app, Default::default()));
    }
    let frame = frame.unwrap();
    let style = hexforge_native::style::HexForgeStyle::for_theme(app.main_snapshot().theme, 1.0);
    for (path, expected) in [
        (FieldPath::root(0), [54.0, 155.0, 326.0, 28.0]),
        (FieldPath::root(0).child(0), [76.0, 320.0, 293.0, 28.0]),
        (FieldPath::root(0).child(1), [76.0, 453.0, 293.0, 28.0]),
        (FieldPath::root(0).child(2), [76.0, 586.0, 293.0, 28.0]),
    ] {
        let field = app.editor.as_ref().unwrap().model.id(&path).unwrap();
        let response = h
            .context
            .read_response(control_id(field, "name", None))
            .unwrap();
        let rect = frame
            .output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(r)
                    if r.fill == style.palette.input
                        && r.rect.height() == 28.0
                        && r.rect.contains(response.rect.center()) =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .min_by(|a, b| a.width().total_cmp(&b.width()))
            .expect("actual painted name input");
        let got = [rect.left(), rect.top(), rect.width(), rect.height()];
        assert!(
            got.iter().zip(expected).all(|(a, b)| (a - b).abs() < 0.1),
            "source card/control rectangle {expected:?}, actual {got:?} at {path:?}"
        );
    }
    let root_placement = control_id(
        app.editor
            .as_ref()
            .unwrap()
            .model
            .id(&FieldPath::root(0))
            .unwrap(),
        "placement-mode",
        None,
    );
    let placement = h.context.read_response(root_placement).unwrap().rect;
    assert!((placement.top() - 208.0).abs() < 0.1 && (placement.width() - 195.65625).abs() < 0.1);
    assert!(
        frame.output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text()=="Advanced")
        ),
        "plain numeric fields retain their Advanced disclosure"
    );
}

#[test]
fn modal_blocks_editor_controls_without_giving_them_disabled_css_opacity() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-close", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    let mut frame = h.editor_frame(&mut app, Default::default());
    for index in 1..4 {
        frame = h.editor_frame(
            &mut app,
            egui::RawInput {
                events: capture_scene::frame_input(
                    &capture_spec::CaptureSpec::new("editor-close", "dark", [660, 760], 1.0)
                        .unwrap(),
                    index,
                ),
                ..Default::default()
            },
        );
    }
    // Source modalFocus makes the editor inert. It does not set :disabled or
    // add opacity to its controls; only the 42% backdrop shades their pixels.
    let load = frame.widgets.iter().find(|w| w.label == "Load…").unwrap();
    assert!(!load.enabled, "background input remains blocked");
    let fill = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(r) if r.rect == load.rect => Some(r.fill),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        fill,
        egui::Color32::from_rgb(34, 40, 49),
        "inert is not the source's 40%-opacity disabled button state"
    );
    let little = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Little" => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        little.override_text_color.unwrap_or(little.fallback_color),
        egui::Color32::from_rgb(201, 209, 217)
    );
    let name_before = app.editor.as_ref().unwrap().model.name.clone();
    for pressed in [true, false] {
        h.editor_frame(
            &mut app,
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(load.rect.center()),
                    egui::Event::PointerButton {
                        pos: load.rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
        );
    }
    assert!(app.picker.is_none(), "inert Load cannot open a picker");
    assert_eq!(app.editor.as_ref().unwrap().model.name, name_before);
}

#[test]
fn actual_dialogs_keep_source_message_line_boxes_and_action_margins() {
    for (scene, surface, size, expected) in [
        (
            "editor-close",
            "editor",
            [660, 760],
            [120.0, 307.5, 420.0, 145.0],
        ),
        ("error", "main", [1280, 800], [430.0, 357.0, 420.0, 86.0]),
    ] {
        let h = NativeUiHarness::new();
        let mut app = NativeApp::new(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&h.context);
        let spec = capture_spec::CaptureSpec::new(scene, "dark", size, 1.0).unwrap();
        capture_scene::prepare_scene(&mut app, &spec).unwrap();
        let mut frame = None;
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0] as f32, size[1] as f32),
                )),
                ..Default::default()
            };
            frame = Some(if surface == "editor" {
                h.editor_frame(&mut app, input)
            } else {
                h.main_frame(&mut app, input)
            });
        }
        let frame = frame.unwrap();
        fn dialog_rect(shape: &egui::Shape) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Rect(r)
                    if r.corner_radius == egui::CornerRadius::same(7)
                        && r.fill == egui::Color32::from_rgb(21, 25, 30) =>
                {
                    Some(r.rect)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(dialog_rect),
                _ => None,
            }
        }
        let rect = frame
            .output
            .shapes
            .iter()
            .find_map(|shape| dialog_rect(&shape.shape))
            .expect("real themed dialog frame");
        let actual = [rect.left(), rect.top(), rect.width(), rect.height()];
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 0.1),
            "source CSS dialog {scene}: expected {expected:?}, actual {actual:?}"
        );
        if scene == "editor-close" {
            let cancel = frame.widgets.iter().find(|w| w.label == "Cancel").unwrap();
            assert!((cancel.rect.top() - 409.5).abs() < 0.1);
        }
    }
}

#[test]
fn empty_binary_retains_its_fixed_address_gutter_above_the_empty_overlay() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        InitialState {
            theme: hexforge_controller::controller::Theme::Light,
            ..Default::default()
        },
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    let spec = capture_spec::CaptureSpec::new("empty-file", "light", [1280, 800], 1.0).unwrap();
    capture_scene::prepare_scene(&mut app, &spec).unwrap();
    let frame = h.main_frame(
        &mut app,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
    );
    let overlay = frame.output.shapes.iter().rposition(|shape|matches!(&shape.shape,
        egui::Shape::Rect(r) if r.rect == egui::Rect::from_min_max(egui::pos2(0.0,76.0),egui::pos2(1184.0,552.0))
            && r.fill==egui::Color32::from_rgb(246,248,250))).unwrap();
    let offset = frame
        .output
        .shapes
        .iter()
        .enumerate()
        .find(|(index, shape)| {
            *index > overlay
                && matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="OFFSET")
        });
    let (_, painted) =
        offset.expect("source's z-index:1 fixed gutter remains above the empty-file overlay");
    // The lossless reference is white at x151, its divider is at x152,
    // and the empty-overlay background starts at x153.
    assert_eq!(
        painted.clip_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 76.0), egui::pos2(153.0, 552.0))
    );
    assert!(frame
        .output
        .shapes
        .iter()
        .skip(overlay + 1)
        .any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(r) if r.fill==egui::Color32::WHITE && shape.clip_rect.right()==153.0)));
    assert!(app.main_snapshot().selection.is_none());
}

#[test]
fn modal_autofocus_and_keyboard_focus_have_their_distinct_source_appearance() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-close", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    let mut frame = h.editor_frame(&mut app, Default::default());
    for _ in 0..3 {
        frame = h.editor_frame(&mut app, Default::default());
    }
    let cancel = frame.widgets.iter().find(|w| w.label == "Cancel").unwrap();
    let focus = h.context.memory(|m| m.focused()).unwrap();
    assert_eq!(
        h.context.read_response(focus).unwrap().rect,
        cancel.rect,
        "autofocus is retained, not removed to conceal its outline"
    );
    assert!(
        !frame
            .output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(r) if r.rect==cancel.rect.expand(2.0)
            && r.stroke.color==egui::Color32::from_rgb(31,111,235))),
        "source :focus-visible is false for this programmatic modal autofocus"
    );
    h.editor_frame(
        &mut app,
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
    );
    let frame = h.editor_frame(&mut app, Default::default());
    let close = frame
        .widgets
        .iter()
        .find(|w| w.label == "Close dialog")
        .unwrap();
    let focused = h.context.memory(|m| m.focused()).unwrap();
    assert_eq!(h.context.read_response(focused).unwrap().rect, close.rect);
    assert!(
        frame
            .output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(r) if r.rect==close.rect && r.fill==egui::Color32::from_rgb(37,44,53)
            && r.corner_radius==egui::CornerRadius::same(4))),
        "the actual keyboard-focused dialog X uses the source hover fill, not a generic blue ring"
    );
}

#[test]
fn populated_editor_preserves_source_nested_trailing_margins_and_label_only_headers() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    let mut frame = h.editor_frame(&mut app, Default::default());
    for _ in 0..3 {
        frame = h.editor_frame(&mut app, Default::default());
    }
    let cards = frame
        .output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(r)
                if r.fill == egui::Color32::from_rgb(25, 30, 36)
                    && r.corner_radius == egui::CornerRadius::same(5) =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        (cards[0].height() - 708.0).abs() < 0.1,
        "source header card includes the last nested child's 8px margin, actual {:?}",
        cards[0]
    );
    // Offscreen card backgrounds are intentionally not painted. Observe the
    // actual laid-out inputs instead, including descendants below the clip.
    use hexforge_controller::template::editing::FieldPath;
    use hexforge_native::views::field_card::control_id;
    for (path, expected) in [
        (FieldPath::root(1), [61.0, 876.0, 312.0, 18.0]),
        (
            FieldPath::root(1).element().child(0),
            [105.0, 1245.0, 246.0, 18.0],
        ),
        (
            FieldPath::root(1).element().child(1),
            [105.0, 1378.0, 246.0, 18.0],
        ),
        (FieldPath::root(2), [61.0, 1549.0, 312.0, 18.0]),
    ] {
        let id = app.editor.as_ref().unwrap().model.id(&path).unwrap();
        let rect = h
            .context
            .read_response(control_id(id, "name", None))
            .unwrap()
            .rect;
        let actual = [rect.left(), rect.top(), rect.width(), rect.height()];
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 0.1),
            "source nested layout at {path:?}: expected {expected:?}, got {actual:?}"
        );
    }
}

#[test]
fn array_element_heading_keeps_its_type_selector_at_the_source_flex_edge() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-advanced", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    for _ in 0..4 {
        h.editor_frame(&mut app, Default::default());
    }
    use hexforge_controller::template::editing::FieldPath;
    use hexforge_native::views::field_card::control_id;
    let id = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(1).element())
        .unwrap();
    let rect = h
        .context
        .read_response(control_id(id, "type", None))
        .unwrap()
        .rect;
    let actual = [rect.left(), rect.top(), rect.width(), rect.height()];
    let expected = [495.0, 1075.0, 118.0, 28.0];
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 0.1),
        "the source Array element label flexes without squeezing the type selector: {actual:?}"
    );
}

#[test]
fn binary_close_buttons_keep_their_source_padding_and_not_editor_dimensions() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("binary-close", "dark", [1280, 800], 1.0).unwrap(),
    )
    .unwrap();
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    for _ in 0..4 {
        h.main_frame(&mut app, raw());
    }
    let frame = h.main_frame(&mut app, raw());
    for (label, expected) in [
        (
            "Save Copy and Continue",
            [483.984375, 438.5, 186.40625, 28.0],
        ),
        ("Discard", [677.390625, 438.5, 78.40625, 28.0]),
        ("Cancel", [762.796875, 438.5, 71.203125, 28.0]),
    ] {
        let rect = frame
            .widgets
            .iter()
            .find(|w| w.label == label)
            .unwrap()
            .rect;
        let actual = [rect.left(), rect.top(), rect.width(), rect.height()];
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 0.1),
            "source binary prompt {label}: {actual:?}"
        );
        assert!(frame
            .output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(r) if r.rect==rect && r.corner_radius==egui::CornerRadius::same(4)
                && r.fill==egui::Color32::from_rgb(34,40,49))));
    }
}

fn validation_shapes(frame: &support::ui_harness::CapturedFrame) -> Vec<&egui::Shape> {
    fn collect<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
        if let egui::Shape::Vec(children) = shape {
            for child in children {
                collect(child, out);
            }
        } else {
            out.push(shape);
        }
    }
    let mut out = vec![];
    for shape in &frame.output.shapes {
        collect(&shape.shape, &mut out);
    }
    out
}

#[test]
fn editor_disclosures_use_source_triangle_geometry_and_text_inset_not_font_glyphs() {
    for (scene, label, points, text_x, color) in [
        (
            "editor-dirty",
            "Advanced",
            vec![
                egui::pos2(23.0, 247.0),
                egui::pos2(23.0, 255.0),
                egui::pos2(30.0, 251.0),
            ],
            34.640625,
            egui::Color32::from_rgb(139, 148, 158),
        ),
        (
            "editor-invalid",
            "1 template error",
            vec![
                egui::pos2(22.0, 156.0),
                egui::pos2(30.0, 156.0),
                egui::pos2(26.0, 163.0),
            ],
            33.640625,
            egui::Color32::from_rgb(240, 136, 62),
        ),
    ] {
        let h = NativeUiHarness::new();
        let mut app = NativeApp::new(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&h.context);
        capture_scene::prepare_scene(
            &mut app,
            &capture_spec::CaptureSpec::new(scene, "dark", [660, 760], 1.0).unwrap(),
        )
        .unwrap();
        let frame = h.editor_frame(&mut app, Default::default());
        let shapes = validation_shapes(&frame);
        assert!(shapes.iter().any(|shape|matches!(shape,egui::Shape::Path(path) if path.points==points && path.fill==color && path.closed)),"{scene}: source CSS disclosure triangle has its own geometry, not the smaller fallback glyph");
        let text = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t),
                _ => None,
            })
            .expect("source disclosure's plain text");
        assert_eq!(
            text.pos.x, text_x,
            "source marker advances11.640625px before its text"
        );
    }
}

#[test]
fn validation_summary_retains_source_border_padding_and_fourteen_pixel_line_boxes() {
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    capture_scene::prepare_scene(
        &mut app,
        &capture_spec::CaptureSpec::new("editor-invalid", "dark", [660, 760], 1.0).unwrap(),
    )
    .unwrap();
    let frame = h.editor_frame(&mut app, Default::default());
    let shapes = validation_shapes(&frame);
    let bounds = shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Rect(r)
                if r.rect.top() == 144.0 && r.rect.left() == 12.0 && r.rect.width() == 636.0 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("the production validation summary border");
    assert_eq!(
        bounds.height(),
        57.0,
        "source: 2px border + 14px padding + 14px summary + 5px gap + 22px issue button"
    );
    let issue = frame
        .widgets
        .iter()
        .find(|w| w.label == "template · Template name must not be empty.")
        .expect("source issue button is accessible");
    assert_eq!(
        issue.rect,
        egui::Rect::from_min_size(egui::pos2(22.0, 171.0), egui::vec2(616.0, 22.0))
    );
}

#[test]
fn validation_issue_keeps_normal_text_color_and_only_its_path_bold() {
    for theme in [
        hexforge_controller::controller::Theme::Dark,
        hexforge_controller::controller::Theme::Light,
    ] {
        let h = NativeUiHarness::new();
        let mut app = NativeApp::new(
            InitialState {
                theme,
                ..Default::default()
            },
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        );
        app.attach(&h.context);
        let theme_name = if theme == hexforge_controller::controller::Theme::Dark {
            "dark"
        } else {
            "light"
        };
        capture_scene::prepare_scene(
            &mut app,
            &capture_spec::CaptureSpec::new("editor-invalid", theme_name, [660, 760], 1.0).unwrap(),
        )
        .unwrap();
        let frame = h.editor_frame(&mut app, Default::default());
        let shapes = validation_shapes(&frame);
        let text = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(t)
                    if t.galley.text() == "template · Template name must not be empty." =>
                {
                    Some(t)
                }
                _ => None,
            })
            .expect("actual issue text");
        let color = hexforge_native::style::HexForgeStyle::for_theme(theme, 1.0)
            .palette
            .text;
        assert_eq!(
            text.override_text_color.unwrap_or(text.fallback_color),
            color
        );
        assert_eq!(
            text.galley.job.sections.len(),
            2,
            "source strong path and normal message have distinct font sections"
        );
        assert_eq!(
            text.galley.job.sections[0].format.font_id,
            hexforge_native::style::fonts::bold(11.0)
        );
        assert_eq!(
            text.galley.job.sections[1].format.font_id,
            hexforge_native::style::fonts::regular(11.0)
        );
        assert!(text
            .galley
            .job
            .sections
            .iter()
            .all(|section| section.format.color == color));
    }
}

#[test]
fn validation_issue_wraps_at_narrow_width_and_keyboard_activation_reveals_its_control() {
    use hexforge_controller::{action::Action, command::Command, template::editing::FieldPath};
    let h = NativeUiHarness::new();
    let mut app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    app.attach(&h.context);
    app.invoke(Command::TemplateEditor, Default::default());
    app.gesture(Action::TemplateChanged(serde_json::from_value(serde_json::json!({"name":"Wrap","defaultEndianness":"little","fields":[{"name":"header_field_with_long_name","type":"u16","align":3}]})).unwrap()));
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(480.0, 760.0),
        )),
        events,
        ..Default::default()
    };
    let mut frame = h.editor_frame(&mut app, input(vec![]));
    let label = "header_field_with_long_name · Alignment must be a power of two up to 1048576.";
    let issue = frame
        .widgets
        .iter()
        .find(|w| w.label == label)
        .expect("reveal error is a real keyboard-accessible button");
    assert_eq!(issue.role, egui::accesskit::Role::Button);
    assert_eq!(
        issue.rect.height(),
        36.0,
        "the source's two14px lines plus8px padding must wrap instead of clip"
    );
    assert_eq!(issue.rect.width(), 436.0);
    let shapes = validation_shapes(&frame);
    let text = shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.galley.rows.len(), 2);
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let mut focused = false;
    for _ in 0..10 {
        frame = h.editor_frame(&mut app, input(vec![key(egui::Key::Tab)]));
        let id = h
            .context
            .memory(|m| m.focused())
            .map(|id| id.accesskit_id());
        focused = frame
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .is_some_and(|tree| {
                tree.nodes
                    .iter()
                    .any(|(node_id, node)| Some(*node_id) == id && node.label() == Some(label))
            });
        if focused {
            break;
        }
    }
    assert!(
        focused,
        "source issue button must be reachable through the actual editor tab sequence"
    );
    h.editor_frame(&mut app, input(vec![key(egui::Key::Enter)]));
    h.editor_frame(&mut app, input(vec![]));
    let field = app
        .editor
        .as_ref()
        .unwrap()
        .model
        .id(&FieldPath::root(0))
        .unwrap();
    assert!(h.context.memory(
        |m| m.has_focus(hexforge_native::views::field_card::control_id(
            field, "align", None
        ))
    ));
}
