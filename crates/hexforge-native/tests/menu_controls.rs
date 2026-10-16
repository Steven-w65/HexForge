mod support;
use eframe::egui::{self, accesskit::Role, Event};
use hexforge_controller::{
    command::Command,
    controller::{
        AppController, ControllerEvent, InitialState, MainSnapshot, RowWidth, WindowSnapshot,
    },
    shortcuts::WindowKind,
};
use hexforge_core::{
    dto::FileInfoDto,
    template::{FieldType, ParsedNode},
};
use hexforge_native::{style::HexForgeStyle, views::top_menu::MenuView};
use support::ui_harness::{CapturedFrame, NativeUiHarness};

struct MenuFixture {
    harness: NativeUiHarness,
    view: MenuView,
    snapshot: MainSnapshot,
    events: Vec<ControllerEvent>,
}
fn key(key: egui::Key, modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}
impl MenuFixture {
    fn new() -> Self {
        let controller = AppController::new(InitialState::default());
        let WindowSnapshot::Main(mut snapshot) = controller.snapshot(WindowKind::Main) else {
            unreachable!()
        };
        snapshot.file = Some(FileInfoDto {
            name: "sample.bin".into(),
            path: "sample.bin".into(),
            size: "64".into(),
            revision: "0".into(),
            dirty: false,
        });
        snapshot.policy.has_file = true;
        snapshot.policy.has_bytes = true;
        snapshot.policy.single_byte_selected = true;
        snapshot.policy.edit_mode = true;
        snapshot.policy.can_undo = true;
        snapshot.policy.template_active = true;
        snapshot.policy.template_valid = true;
        snapshot.policy.template_has_fields = true;
        snapshot.policy.template_has_path = true;
        snapshot.policy.has_parsed_results = true;
        snapshot.policy.has_search_matches = true;
        snapshot.edit_mode = true;
        Self {
            harness: NativeUiHarness::new(),
            view: MenuView::default(),
            snapshot,
            events: vec![],
        }
    }
    fn frame(&mut self, mut events: Vec<Event>) -> CapturedFrame {
        let modifiers = events
            .iter()
            .rev()
            .find_map(|e| match e {
                Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } => {
                    Some(*modifiers)
                }
                _ => None,
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let style = HexForgeStyle::for_theme(self.snapshot.theme, 1.0);
        // Match NativeApp's actual theme/animation setup, not egui defaults.
        style.apply(&self.harness.context);
        self.harness.frame(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 34.0),
                    )),
                    |ui| {
                        self.events
                            .extend(self.view.show(ui, &style, &self.snapshot))
                    },
                );
            },
        )
    }
    fn settle(&mut self) -> CapturedFrame {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }
    fn open(&mut self, menu: egui::Key) -> CapturedFrame {
        self.frame(vec![key(menu, egui::Modifiers::ALT)]);
        self.settle()
    }
    fn click(&mut self, p: egui::Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }
    fn focus(&self, frame: &CapturedFrame) -> Option<String> {
        let id = self.harness.context.memory(|m| m.focused())?.accesskit_id();
        frame
            .output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .and_then(|(_, n)| n.label().map(str::to_owned))
    }
}
fn texts(frame: &CapturedFrame) -> Vec<(String, egui::Pos2)> {
    frame
        .output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => {
                Some((t.galley.job.text.clone(), t.pos + t.galley.size() / 2.0))
            }
            _ => None,
        })
        .collect()
}
fn rect(frame: &CapturedFrame, label: &str) -> egui::Rect {
    frame
        .widgets
        .iter()
        .find(|w| w.label == label)
        .unwrap_or_else(|| {
            panic!(
                "Missing {label}: {:?}",
                frame.widgets.iter().map(|w| &w.label).collect::<Vec<_>>()
            )
        })
        .rect
}

#[test]
fn real_menu_groups_preserve_all_source_commands_and_only_two_flyouts() {
    let top = ["File", "Edit", "Navigate", "Template", "View"];
    for (key_value, expected) in [
        (
            egui::Key::F,
            vec![
                "Open File…",
                "Close File",
                "Save As…",
                "Export Parsed Results as CSV…",
                "Exit",
            ],
        ),
        (
            egui::Key::E,
            vec!["Edit Selected Byte…", "Edit Mode", "Undo Byte Edit"],
        ),
        (
            egui::Key::N,
            vec![
                "Go To Offset…",
                "Search Bytes…",
                "Next Search Match",
                "Previous Search Match",
                "Parsed Results",
            ],
        ),
        (
            egui::Key::T,
            vec![
                "Template Editor…",
                "Apply Template",
                "Load Template…",
                "Unload Template",
                "Save Template",
                "Save Template As…",
            ],
        ),
        (
            egui::Key::V,
            vec!["Toggle Theme", "16 Bytes", "32 Bytes", "Minimap"],
        ),
    ] {
        let mut fixture = MenuFixture::new();
        let frame = fixture.open(key_value);
        let mut actual: Vec<_> = frame
            .widgets
            .iter()
            .filter(|w| {
                matches!(w.role, Role::MenuItem | Role::MenuItemCheckBox)
                    && !top.contains(&w.label.as_str())
            })
            .collect();
        actual.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
        assert_eq!(
            actual.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(),
            expected
        );
        assert!(texts(&frame).iter().any(|(text, _)| text == "HexForge"));
        assert!(!texts(&frame)
            .iter()
            .any(|(text, _)| text == "Template Fields" || text == "Add Field"));
    }
}

#[test]
fn painted_menu_shortcuts_match_the_real_routes_without_template_save_aliases() {
    for (menu, expected) in [
        (
            egui::Key::F,
            vec![
                ("Open File…", Some("Ctrl+O")),
                ("Close File", Some("Ctrl+W")),
                ("Save As…", Some("Ctrl+Shift+S")),
                ("Export Parsed Results as CSV…", Some("Ctrl+Shift+E")),
                ("Exit", Some("Alt+F4")),
            ],
        ),
        (
            egui::Key::E,
            vec![
                ("Edit Selected Byte…", Some("F2")),
                ("Edit Mode", Some("Ctrl+Alt+E")),
                ("Undo Byte Edit", Some("Ctrl+Z")),
            ],
        ),
        (
            egui::Key::N,
            vec![
                ("Go To Offset…", Some("Ctrl+G")),
                ("Search Bytes…", Some("Ctrl+F")),
                ("Next Search Match", Some("F3")),
                ("Previous Search Match", Some("Shift+F3")),
            ],
        ),
        (
            egui::Key::T,
            vec![
                ("Template Editor…", Some("Ctrl+Shift+T")),
                ("Apply Template", Some("Ctrl+Enter")),
                ("Load Template…", Some("Ctrl+Alt+O")),
                ("Unload Template", Some("Ctrl+Alt+U")),
                ("Save Template", None),
                ("Save Template As…", None),
            ],
        ),
        (
            egui::Key::V,
            vec![
                ("Toggle Theme", Some("Ctrl+Alt+T")),
                ("16 Bytes", Some("Ctrl+1")),
                ("32 Bytes", Some("Ctrl+2")),
            ],
        ),
    ] {
        let mut fixture = MenuFixture::new();
        let frame = fixture.open(menu);
        let painted = texts(&frame);
        for (label, shortcut) in expected {
            let row = rect(&frame, label);
            let hints: Vec<_> = painted
                .iter()
                .filter(|(text, p)| row.contains(*p) && text != label && text != "✓")
                .map(|(t, _)| t.as_str())
                .collect();
            assert_eq!(hints, shortcut.into_iter().collect::<Vec<_>>(), "{label}");
        }
    }
}

#[test]
fn binary_copy_and_template_menu_saves_dispatch_to_distinct_commands() {
    for (menu, label, expected) in [
        (egui::Key::F, "Save As…", Command::SaveAs),
        (egui::Key::T, "Save Template", Command::SaveTemplate),
        (egui::Key::T, "Save Template As…", Command::SaveTemplateAs),
    ] {
        let mut fixture = MenuFixture::new();
        let frame = fixture.open(menu);
        fixture.click(rect(&frame, label).center());
        assert!(
            matches!(fixture.events.as_slice(),[ControllerEvent::Invoke {command,..}]
            if *command==expected),
            "actual menu action {label} must retain its own workflow"
        );
        assert!(!fixture.view.is_open());
    }
}

#[test]
fn template_menu_without_a_saved_path_keeps_save_as_and_editor_available() {
    let mut fixture = MenuFixture::new();
    fixture.snapshot.policy.template_has_path = false;
    fixture.snapshot.policy.has_file = false;
    fixture.snapshot.file = None;
    let frame = fixture.open(egui::Key::T);
    for (label, enabled) in [
        ("Save Template", false),
        ("Save Template As…", true),
        ("Template Editor…", true),
    ] {
        assert_eq!(
            frame
                .widgets
                .iter()
                .find(|w| w.label == label)
                .unwrap()
                .enabled,
            enabled,
            "{label}"
        );
    }
    fixture.click(rect(&frame, "Save Template").center());
    assert!(fixture.events.is_empty());
    let frame = fixture.settle();
    fixture.click(rect(&frame, "Save Template As…").center());
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Invoke {
            command: Command::SaveTemplateAs,
            ..
        }]
    ));
    let mut fixture = MenuFixture::new();
    fixture.snapshot.policy.has_file = false;
    fixture.snapshot.file = None;
    let frame = fixture.open(egui::Key::T);
    fixture.click(rect(&frame, "Template Editor…").center());
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Invoke {
            command: Command::TemplateEditor,
            ..
        }]
    ));
}

#[test]
fn disabled_pointer_commands_remain_inert_while_open_is_enabled() {
    let mut fixture = MenuFixture::new();
    fixture.snapshot.file = None;
    fixture.snapshot.policy.has_file = false;
    fixture.snapshot.policy.has_bytes = false;
    fixture.snapshot.policy.has_parsed_results = false;
    let frame = fixture.open(egui::Key::F);
    for label in ["Close File", "Save As…", "Export Parsed Results as CSV…"] {
        assert!(
            !frame
                .widgets
                .iter()
                .find(|w| w.label == label)
                .unwrap()
                .enabled
        );
        fixture.click(rect(&frame, label).center());
        assert!(
            fixture.events.is_empty(),
            "disabled {label} must not dispatch"
        );
    }
    assert!(fixture.view.is_open());
    fixture.click(rect(&frame, "Open File…").center());
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Invoke {
            command: Command::Open,
            ..
        }]
    ));
    assert!(!fixture.view.is_open());
}

#[test]
fn menu_checked_states_and_minimap_flyout_keep_the_source_settings() {
    use egui::accesskit::Toggled;
    let mut fixture = MenuFixture::new();
    let frame = fixture.open(egui::Key::E);
    assert_eq!(
        frame
            .widgets
            .iter()
            .find(|w| w.label == "Edit Mode")
            .unwrap()
            .toggled,
        Some(Toggled::True)
    );
    fixture.frame(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    let frame = fixture.open(egui::Key::V);
    for (label, toggled) in [("16 Bytes", Toggled::True), ("32 Bytes", Toggled::False)] {
        assert_eq!(
            frame
                .widgets
                .iter()
                .find(|w| w.label == label)
                .unwrap()
                .toggled,
            Some(toggled)
        );
    }
    fixture.click(rect(&frame, "Minimap").center());
    let frame = fixture.settle();
    let labels = [
        "Show Minimap",
        "Fit File",
        "Proportional",
        "Characters",
        "Color Blocks",
        "Scale 1×",
        "Scale 2×",
        "Scale 3×",
    ];
    let mut items: Vec<_> = frame
        .widgets
        .iter()
        .filter(|w| labels.contains(&w.label.as_str()))
        .collect();
    items.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
    assert_eq!(
        items.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(),
        labels
    );
    assert_eq!(
        items
            .iter()
            .find(|w| w.label == "Fit File")
            .unwrap()
            .toggled,
        Some(Toggled::True)
    );
    assert!(!texts(&frame)
        .iter()
        .any(|(t, _)| t == "16 Columns" || t == "32 Columns"));
    fixture.click(rect(&frame, "Proportional").center());
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Invoke {
            command: Command::MinimapProportional,
            ..
        }]
    ));
    fixture.snapshot.row_width = RowWidth::ThirtyTwo;
    let frame = fixture.open(egui::Key::V);
    assert_eq!(
        frame
            .widgets
            .iter()
            .find(|w| w.label == "32 Bytes")
            .unwrap()
            .toggled,
        Some(Toggled::True)
    );
}

#[test]
fn both_keyboard_flyouts_restore_focus_to_their_trigger() {
    for (menu, trigger, first) in [
        (egui::Key::V, "Minimap", "Show Minimap"),
        (egui::Key::N, "Parsed Results", "magic"),
    ] {
        let mut fixture = MenuFixture::new();
        fixture.snapshot.results = vec![ParsedNode {
            kind: "leaf".into(),
            name: "magic".into(),
            path: "magic".into(),
            field_type: FieldType::Bytes,
            offset: Some("0".into()),
            length: Some("4".into()),
            endianness: None,
            value: Some("48 45 58 46".into()),
            comment: String::new(),
            enum_label: None,
            flags: vec![],
            diagnostics: vec![],
            children: vec![],
        }];
        fixture.open(menu);
        let count = if menu == egui::Key::V { 3 } else { 4 };
        for _ in 0..count {
            fixture.frame(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        }
        let frame = fixture.settle();
        assert_eq!(fixture.focus(&frame).as_deref(), Some(trigger));
        fixture.frame(vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
        let frame = fixture.settle();
        assert_eq!(fixture.focus(&frame).as_deref(), Some(first));
        fixture.frame(vec![key(egui::Key::ArrowLeft, egui::Modifiers::NONE)]);
        let frame = fixture.settle();
        assert_eq!(fixture.focus(&frame).as_deref(), Some(trigger));
        assert!(!frame.widgets.iter().any(|w| w.label == first));
    }
}

#[test]
fn pointer_and_keyboard_menu_switches_reanchor_to_actual_headings() {
    let mut fixture = MenuFixture::new();
    let initial = fixture.settle();
    let file = rect(&initial, "File");
    fixture.click(file.center());
    let frame = fixture.settle();
    assert!(
        (rect(&frame, "Open File…").left() - (file.left() + 5.0)).abs() < 0.5,
        "popup remains attached after raster-grid rounding"
    );
    assert!(rect(&frame, "Open File…").top() >= file.bottom());
    fixture.frame(vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
    let frame = fixture.settle();
    let edit = rect(&frame, "Edit");
    assert!((rect(&frame, "Edit Selected Byte…").left() - (edit.left() + 5.0)).abs() < 0.5);
    assert!(!frame.widgets.iter().any(|w| w.label == "Open File…"));
    fixture.frame(vec![key(egui::Key::V, egui::Modifiers::ALT)]);
    let frame = fixture.settle();
    let view = rect(&frame, "View");
    assert!((rect(&frame, "Toggle Theme").left() - (view.left() + 5.0)).abs() < 0.5);
}

#[test]
fn parsed_result_flyout_pointer_and_access_key_emit_the_same_exact_range() {
    for pointer in [false, true] {
        let mut fixture = MenuFixture::new();
        fixture.snapshot.results = vec![ParsedNode {
            kind: "leaf".into(),
            name: "magic".into(),
            path: "magic".into(),
            field_type: FieldType::Bytes,
            offset: Some("0".into()),
            length: Some("4".into()),
            endianness: None,
            value: Some("48 45 58 46".into()),
            comment: String::new(),
            enum_label: None,
            flags: vec![],
            diagnostics: vec![],
            children: vec![],
        }];
        let frame = fixture.open(egui::Key::N);
        if pointer {
            fixture.click(rect(&frame, "Parsed Results").center());
        } else {
            fixture.frame(vec![key(egui::Key::R, egui::Modifiers::NONE)]);
        }
        let frame = fixture.settle();
        assert_eq!(fixture.focus(&frame).as_deref(), Some("magic"));
        if pointer {
            fixture.click(rect(&frame, "magic").center());
        } else {
            fixture.frame(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        }
        assert!(matches!(
            fixture.events.as_slice(),
            [ControllerEvent::Action(
                hexforge_controller::action::Action::NavigateTemplate { start: 0, end: 3 }
            )]
        ));
        assert!(!fixture.view.is_open());
    }
}

fn painted_shapes(frame: &CapturedFrame) -> Vec<&egui::Shape> {
    fn collect<'a>(shape: &'a egui::Shape, into: &mut Vec<&'a egui::Shape>) {
        if let egui::Shape::Vec(children) = shape {
            for child in children {
                collect(child, into);
            }
        } else {
            into.push(shape);
        }
    }
    let mut shapes = vec![];
    for shape in &frame.output.shapes {
        collect(&shape.shape, &mut shapes);
    }
    shapes
}

#[test]
fn flyouts_start_at_the_source_trigger_top_in_both_themes() {
    for theme in [
        hexforge_controller::controller::Theme::Dark,
        hexforge_controller::controller::Theme::Light,
    ] {
        for (menu, trigger, down) in [
            (egui::Key::V, "Minimap", 3),
            (egui::Key::N, "Parsed Results", 4),
        ] {
            let mut fixture = MenuFixture::new();
            fixture.snapshot.theme = theme;
            fixture.snapshot.results = vec![ParsedNode {
                kind: "leaf".into(),
                name: "magic".into(),
                path: "magic".into(),
                field_type: FieldType::Bytes,
                offset: Some("0".into()),
                length: Some("4".into()),
                endianness: None,
                value: Some("48 45 58 46".into()),
                comment: String::new(),
                enum_label: None,
                flags: vec![],
                diagnostics: vec![],
                children: vec![],
            }];
            fixture.open(menu);
            for _ in 0..down {
                fixture.frame(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
            }
            fixture.frame(vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
            let frame = fixture.settle();
            let branch = rect(&frame, trigger);
            let style = HexForgeStyle::for_theme(theme, 1.0);
            let flyout = painted_shapes(&frame)
                .into_iter()
                .find_map(|shape| match shape {
                    egui::Shape::Rect(r)
                        if r.fill == style.palette.panel && r.rect.left() > branch.right() =>
                    {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .expect("actual submenu painted frame");
            // The source's absolute top uses trigger.offsetTop: measured
            // flyout and trigger top edges coincide, without subtracting its padding.
            assert_eq!(
                flyout.top(),
                branch.top(),
                "{trigger} flyout is attached to the row, not four pixels above it"
            );
        }
    }
}

fn long_results_flyout() -> MenuFixture {
    let mut fixture = MenuFixture::new();
    fixture.snapshot.results = (0..20)
        .map(|i| ParsedNode {
            kind: "leaf".into(),
            name: format!("field{i}"),
            path: format!("field{i}"),
            field_type: FieldType::U8,
            offset: Some(i.to_string()),
            length: Some("1".into()),
            endianness: None,
            value: Some("0".into()),
            comment: String::new(),
            enum_label: None,
            flags: vec![],
            diagnostics: vec![],
            children: vec![],
        })
        .collect();
    fixture.open(egui::Key::N);
    for _ in 0..4 {
        fixture.frame(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
    }
    fixture.frame(vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
    fixture.settle();
    fixture
}

#[test]
fn overflowing_result_flyout_reserves_and_paints_the_source_thin_scrollbar() {
    let mut fixture = long_results_flyout();
    let frame = fixture.settle();
    let first = rect(&frame, "field0");
    assert_eq!(
        first.width(),
        220.0,
        "source230px content reserves its10px thin scrollbar slot"
    );
    let color = HexForgeStyle::for_theme(fixture.snapshot.theme, 1.0)
        .palette
        .border_strong;
    let thumb = painted_shapes(&frame)
        .into_iter()
        .find_map(|shape| match shape {
            egui::Shape::Rect(r)
                if r.fill == color
                    && r.rect.width() == 6.0
                    && r.corner_radius == egui::CornerRadius::same(3) =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("the source CSS overflow scrollbar must be visible and usable");
    assert_eq!(
        thumb.height(),
        114.0,
        "288px padded scroll viewport over668px content,12px end arrows"
    );
    assert_eq!(thumb.top(), first.top() + 8.0);
}

#[test]
fn result_flyout_keyboard_focus_reveals_the_last_offscreen_leaf() {
    let mut fixture = long_results_flyout();
    for _ in 0..19 {
        fixture.frame(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        fixture.settle();
    }
    let frame = fixture.settle();
    assert_eq!(fixture.focus(&frame).as_deref(), Some("field19"));
    let leaf = rect(&frame, "field19");
    assert!(
        leaf.bottom() <= 469.0,
        "focused last leaf must be revealed inside the source flyout, got{leaf:?}"
    );
    assert!(
        fixture.events.is_empty(),
        "moving menu focus must not activate a byte selection"
    );
    fixture.frame(vec![key(egui::Key::ArrowUp, egui::Modifiers::NONE)]);
    let frame = fixture.settle();
    assert_eq!(fixture.focus(&frame).as_deref(), Some("field18"));
}

#[test]
fn result_flyout_thumb_drag_and_wheel_reach_the_last_leaf_without_activation() {
    let mut fixture = long_results_flyout();
    let frame = fixture.settle();
    let first = rect(&frame, "field0");
    // Independently measured source: 288px viewport, 668px padded content,
    // 114px thumb and 150px travel. Preserve a noncentral 17px grab offset.
    let grab = egui::pos2(first.right() + 5.0, first.top() + 8.0 + 17.0);
    fixture.frame(vec![Event::PointerMoved(grab)]);
    fixture.frame(vec![Event::PointerButton {
        pos: grab,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }]);
    let end = grab + egui::vec2(0.0, 150.0);
    fixture.frame(vec![Event::PointerMoved(end)]);
    fixture.frame(vec![Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let frame = fixture.settle();
    assert_eq!(rect(&frame, "field19").bottom(), first.top() + 280.0);
    assert!(fixture.events.is_empty());
    assert!(
        fixture.view.is_open(),
        "dragging the bar must not close the menu"
    );

    fixture.frame(vec![
        Event::PointerMoved(egui::pos2(first.left() + 30.0, first.top() + 80.0)),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 60.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    for _ in 0..40 {
        fixture.frame(vec![]);
    }
    let frame = fixture.settle();
    assert!((rect(&frame, "field12").top() - (first.top() + 76.0)).abs() < 0.1);
    assert!(
        fixture.events.is_empty(),
        "wheel/reveal must not navigate bytes"
    );
    fixture.click(rect(&frame, "field12").center());
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Action(
            hexforge_controller::action::Action::NavigateTemplate { start: 12, end: 12 }
        )]
    ));
    assert!(!fixture.view.is_open());
}

#[test]
fn menu_separator_matches_source_one_pixel_line_and_four_pixel_margins() {
    for (menu, before, after) in [
        (egui::Key::N, "Previous Search Match", "Parsed Results"),
        (egui::Key::V, "32 Bytes", "Minimap"),
    ] {
        let mut fixture = MenuFixture::new();
        let frame = fixture.open(menu);
        let previous = rect(&frame, before);
        let next = rect(&frame, after);
        assert_eq!(
            next.top() - previous.bottom(),
            9.0,
            "TopMenu.vue .separator is 1px with 4px vertical margins"
        );
        let expected = egui::Rect::from_min_max(
            egui::pos2(previous.left() + 3.0, previous.bottom() + 4.0),
            egui::pos2(previous.right() - 3.0, previous.bottom() + 5.0),
        );
        let style = HexForgeStyle::for_theme(fixture.snapshot.theme, 1.0);
        assert!(
            painted_shapes(&frame).iter().any(|shape| matches!(shape,
            egui::Shape::Rect(r) if r.rect == expected && r.fill == style.palette.border)),
            "separator must use source horizontal inset and border token"
        );
    }
}

#[test]
fn disabled_parsed_results_branch_dims_its_chevron_as_well_as_its_label() {
    for theme in [
        hexforge_controller::controller::Theme::Dark,
        hexforge_controller::controller::Theme::Light,
    ] {
        let mut fixture = MenuFixture::new();
        fixture.snapshot.theme = theme;
        fixture.snapshot.file = None;
        let frame = fixture.open(egui::Key::N);
        let branch = rect(&frame, "Parsed Results");
        assert!(
            !frame
                .widgets
                .iter()
                .find(|w| w.label == "Parsed Results")
                .unwrap()
                .enabled
        );
        let shapes = painted_shapes(&frame);
        let chevron = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(text)
                    if text.galley.text() == "›" && branch.contains(text.pos) =>
                {
                    Some(text)
                }
                _ => None,
            })
            .expect("the actual branch paints a chevron");
        let expected = HexForgeStyle::for_theme(theme, 1.0)
            .palette
            .text
            .gamma_multiply(0.38);
        assert_eq!(
            chevron
                .override_text_color
                .unwrap_or(chevron.fallback_color),
            expected
        );
        fixture.click(branch.center());
        assert!(fixture.events.is_empty());
        assert!(!fixture.settle().widgets.iter().any(|w| w.label == "magic"));
    }
}
