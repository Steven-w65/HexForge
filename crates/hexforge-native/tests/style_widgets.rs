mod support;
use eframe::egui::{self, Color32};
use hexforge_controller::controller::Theme;
use hexforge_native::{
    assets,
    style::{metrics::Metrics, HexForgeStyle},
    widgets::button::{ButtonRole, ButtonSpec, StyledButton, VisualState},
};
use support::ui_harness::NativeUiHarness;
#[test]
fn palette_and_metrics_match_reference_tokens() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../hexforge-controller/tests/fixtures/style-tokens.json"
    ))
    .unwrap();
    for (index, theme) in [Theme::Dark, Theme::Light].into_iter().enumerate() {
        let s = HexForgeStyle::for_theme(theme, 1.0);
        let tokens = fixture["tokenSets"][index]["tokens"].as_object().unwrap();
        for (name, value) in tokens {
            if !name.starts_with("font-") {
                let c = s.palette.token(name).unwrap();
                assert_eq!(
                    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()),
                    value.as_str().unwrap(),
                    "{name}"
                );
            }
        }
        assert_eq!(s.metrics, Metrics::default());
        assert_eq!(s.metrics.button_height, 27.0);
        assert_eq!(s.metrics.input_height, 28.0);
        assert_eq!(s.metrics.button_radius, 3);
        assert_eq!(s.metrics.row_height, 22.0);
    }
}
#[test]
fn all_button_states_have_reference_style() {
    let s = HexForgeStyle::for_theme(Theme::Dark, 1.0);
    let spec = ButtonSpec::new("Save");
    assert_eq!(spec.visual(&s, VisualState::Normal).fill, s.palette.button);
    assert_eq!(spec.visual(&s, VisualState::Hover).fill, s.palette.hover);
    assert_eq!(spec.visual(&s, VisualState::Pressed).fill, s.palette.hover);
    assert_eq!(
        spec.visual(&s, VisualState::Focused).focus,
        Some(s.palette.selection)
    );
    assert_eq!(spec.visual(&s, VisualState::Disabled).opacity, 0.4);
    let menu = ButtonSpec::new("Open").role(ButtonRole::Menu);
    assert_eq!(menu.visual(&s, VisualState::Disabled).opacity, 0.38);
    let primary = ButtonSpec::new("Apply Template").role(ButtonRole::Primary);
    assert_eq!(
        primary.visual(&s, VisualState::Normal).fill,
        s.palette.selection
    );
    assert_eq!(primary.visual(&s, VisualState::Normal).text, Color32::WHITE);
}
#[test]
fn disabled_widget_cannot_invoke() {
    let h = NativeUiHarness::new();
    let s = HexForgeStyle::for_theme(Theme::Dark, 1.0);
    let mut rect = egui::Rect::NOTHING;
    let mut clicked = false;
    let render = |ctx: &mut egui::Ui| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let r = StyledButton::show(
                ui,
                &s,
                &ButtonSpec::new("Save").disabled("Save a template first."),
            );
            rect = r.rect;
            clicked |= r.clicked();
        });
    };
    let captured = h.frame(egui::RawInput::default(), render);
    assert!(!captured.output.shapes.is_empty());
    let pos = rect.center();
    for pressed in [true, false] {
        h.frame(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    clicked |= StyledButton::show(
                        ui,
                        &s,
                        &ButtonSpec::new("Save").disabled("Save a template first."),
                    )
                    .clicked();
                });
            },
        );
    }
    assert!(!clicked);
}
#[test]
fn dialog_buttons_match_editor_footer() {
    let s = HexForgeStyle::for_theme(Theme::Light, 1.5);
    let button = ButtonSpec::new("Don't Save");
    assert_eq!(
        button.visual(&s, VisualState::Normal),
        ButtonSpec::new("Load…").visual(&s, VisualState::Normal)
    );
    let h = NativeUiHarness::new();
    let mut height = 0.0;
    h.frame(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            height = StyledButton::show(ui, &s, &button).rect.height();
        });
    });
    assert_eq!(height, 27.0);
}
#[test]
fn font_metrics_match_hex_layout() {
    let regular = ttf_parser::Face::parse(assets::REGULAR_FONT, 0).unwrap();
    let bold = ttf_parser::Face::parse(assets::SEMIBOLD_FONT, 0).unwrap();
    for face in [&regular, &bold] {
        let a = face
            .glyph_hor_advance(face.glyph_index('0').unwrap())
            .unwrap();
        assert_eq!(a as f64 / face.units_per_em() as f64, 0.6);
        assert_eq!(
            face.glyph_hor_advance(face.glyph_index('W').unwrap()),
            Some(a)
        );
    }
    let h = NativeUiHarness::new();
    let mut width = 0.0;
    h.frame(Default::default(), |ctx| {
        ctx.fonts_mut(|f| {
            width = f
                .layout_no_wrap("00".into(), egui::FontId::monospace(13.0), Color32::WHITE)
                .size()
                .x;
        });
    });
    assert!(
        (width - 15.6).abs() < 0.8,
        "native measured hex advance {width}"
    );
}

#[test]
fn css_bold_headings_use_the_original_700_weight_not_semibold() {
    let face = ttf_parser::Face::parse(assets::BOLD_FONT, 0).unwrap();
    assert_eq!(face.weight().to_number(), 700);
    assert_eq!(
        face.glyph_hor_advance(face.glyph_index('0').unwrap())
            .unwrap() as f64
            / face.units_per_em() as f64,
        0.6
    );
}
#[test]
fn embedded_fonts_render_all_interface_symbols_without_replacement() {
    let h = NativeUiHarness::new();
    let mut missing_symbols = Vec::new();
    h.frame(Default::default(), |ui| {
        ui.fonts_mut(|fonts| {
            for font in [
                hexforge_native::style::fonts::regular(12.0),
                hexforge_native::style::fonts::semibold(13.0),
            ] {
                for c in "▾▸⌄⌃⧉↑↓✓×…—→".chars() {
                    // Check actual raster output, not egui's face-level
                    // has_glyph predicate (it can report false for a supported
                    // glyph in the same face as its replacement character).
                    let actual = fonts.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE);
                    let missing =
                        fonts.layout_no_wrap("\u{10ffff}".into(), font.clone(), Color32::WHITE);
                    if actual.rows[0].glyphs[0].uv_rect == missing.rows[0].glyphs[0].uv_rect {
                        missing_symbols.push(c);
                    }
                }
            }
        });
    });
    assert!(
        missing_symbols.is_empty(),
        "missing interface glyphs: {missing_symbols:?}"
    );
}

#[test]
fn menu_text_uses_the_reference_checkmark_column_not_centering() {
    let h = NativeUiHarness::new();
    let s = HexForgeStyle::for_theme(Theme::Dark, 1.0);
    let mut rect = egui::Rect::NOTHING;
    let f = h.frame(Default::default(), |ui| {
        rect =
            hexforge_native::widgets::menu::item(ui, &s, "Open File…", Some("Ctrl+O"), None, None)
                .rect;
    });
    let text = f
        .output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.job.text == "Open File…" => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.pos.x, rect.left() + 30.0);
}

#[test]
fn splitter_retains_shift_steps_and_home_end_limits() {
    let h = NativeUiHarness::new();
    let s = HexForgeStyle::for_theme(Theme::Dark, 1.0);
    let mut height = 220.0;
    let mut id = egui::Id::NULL;
    h.frame(Default::default(), |ui| {
        id = hexforge_native::widgets::splitter::show(ui, &s, &mut height, 84.0, 600.0, 600.0).id;
    });
    h.context.memory_mut(|m| m.request_focus(id));
    for (key, shift, expected) in [
        (egui::Key::ArrowUp, false, 240.0),
        (egui::Key::ArrowDown, true, 235.0),
        (egui::Key::Home, false, 84.0),
        (egui::Key::End, false, 600.0),
    ] {
        let mods = egui::Modifiers {
            shift,
            ..Default::default()
        };
        h.frame(
            egui::RawInput {
                events: vec![
                    egui::Event::ModifiersChanged(mods),
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: mods,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                hexforge_native::widgets::splitter::show(ui, &s, &mut height, 84.0, 600.0, 600.0);
            },
        );
        assert_eq!(height, expected, "{key:?} shift={shift}");
    }
}
#[test]
fn splitter_uses_original_drag_anchor_after_boundary_clamping() {
    let h = NativeUiHarness::new();
    let s = HexForgeStyle::for_theme(Theme::Dark, 1.0);
    let mut height = 220.0;
    let mut frame = |events| {
        h.frame(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                hexforge_native::widgets::splitter::show(ui, &s, &mut height, 84.0, 600.0, 600.0);
            },
        );
    };
    let p = egui::pos2(100.0, 2.0);
    frame(vec![]);
    frame(vec![egui::Event::PointerMoved(p)]);
    frame(vec![egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }]);
    frame(vec![egui::Event::PointerMoved(egui::pos2(100.0, -1000.0))]);
    frame(vec![egui::Event::PointerMoved(egui::pos2(100.0, -3.0))]);
    assert_eq!(height, 225.0);
}
#[test]
fn select_popup_uses_stable_focus_and_cannot_change_when_disabled() {
    let h = NativeUiHarness::new();
    let s = HexForgeStyle::for_theme(Theme::Light, 1.0);
    let id = egui::Id::new("test-select");
    let mut value = 0;
    let render = |ui: &mut egui::Ui, value: &mut usize, enabled: bool| {
        ui.add_enabled_ui(enabled, |ui| {
            hexforge_native::widgets::input::combo(
                ui,
                &s,
                id,
                value,
                &[(0, "First"), (1, "Second")],
                150.0,
            );
        });
    };
    h.frame(Default::default(), |ui| render(ui, &mut value, true));
    let rect = h.context.read_response(id).unwrap().rect;
    assert_eq!(rect.size(), egui::vec2(150.0, 28.0));
    for pressed in [true, false] {
        h.frame(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                ..Default::default()
            },
            |ui| render(ui, &mut value, true),
        );
    }
    let frame = h.frame(Default::default(), |ui| render(ui, &mut value, true));
    let p = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(t) if t.galley.job.text == "Second" => {
                Some(t.pos + t.galley.size() / 2.0)
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        h.frame(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                ..Default::default()
            },
            |ui| render(ui, &mut value, true),
        );
    }
    assert_eq!(value, 1);
    assert!(h.context.memory(|m| m.has_focus(id)));
    h.frame(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Home,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| render(ui, &mut value, false),
    );
    assert_eq!(
        value, 1,
        "a disabled select must not accept keyboard changes"
    );
}
