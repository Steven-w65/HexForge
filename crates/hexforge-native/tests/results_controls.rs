mod support;

use eframe::egui::{self, accesskit::Node, Event, PointerButton};
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{
        AppController, ByteSelection, ControllerEvent, InitialState, MainSnapshot, WindowSnapshot,
    },
    shortcuts::WindowKind,
    template::status::TemplateSource,
};
use hexforge_core::{
    dto::FileInfoDto,
    template::{Diagnostic, Endian, FieldType, ParsedNode},
};
use hexforge_native::{style::HexForgeStyle, views::results::ResultView};
use support::ui_harness::{CapturedFrame, NativeUiHarness};

fn leaf(path: &str, offset: &str, length: &str, value: &str) -> ParsedNode {
    ParsedNode {
        kind: "leaf".into(),
        path: path.into(),
        name: path.rsplit('.').next().unwrap().into(),
        field_type: FieldType::U8,
        offset: Some(offset.into()),
        length: Some(length.into()),
        value: Some(value.into()),
        endianness: Some(Endian::Little),
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    }
}

fn container(path: &str, kind: &str, children: Vec<ParsedNode>) -> ParsedNode {
    let mut node = leaf(path, "0", "8", "");
    node.kind = kind.into();
    node.field_type = if kind == "array" {
        FieldType::Array
    } else {
        FieldType::Struct
    };
    node.value = None;
    node.children = children;
    node
}

struct ResultsFixture {
    h: NativeUiHarness,
    view: ResultView,
    snapshot: MainSnapshot,
    collapsed: bool,
    events: Vec<ControllerEvent>,
}

impl ResultsFixture {
    fn new(results: Vec<ParsedNode>) -> Self {
        let controller = AppController::new(InitialState::default());
        let WindowSnapshot::Main(mut snapshot) = controller.snapshot(WindowKind::Main) else {
            unreachable!()
        };
        snapshot.file = Some(FileInfoDto {
            name: "fixture.bin".into(),
            path: "fixture.bin".into(),
            size: "9007199254741001".into(),
            revision: "0".into(),
            dirty: false,
        });
        snapshot.template_state.source = TemplateSource::File;
        snapshot.template_state.display_name = "header.json".into();
        snapshot.template_applied = true;
        snapshot.results = results;
        Self {
            h: NativeUiHarness::new(),
            view: ResultView::default(),
            snapshot,
            collapsed: false,
            events: vec![],
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> CapturedFrame {
        self.frame_size(egui::vec2(1280.0, 220.0), events)
    }

    fn frame_size(&mut self, size: egui::Vec2, events: Vec<Event>) -> CapturedFrame {
        let style = HexForgeStyle::for_theme(self.snapshot.theme, 1.0);
        style.apply(&self.h.context);
        self.h.frame(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                self.events
                    .extend(self.view.show(ui, &style, &self.snapshot, self.collapsed))
            },
        )
    }

    fn click(&mut self, point: egui::Pos2) {
        self.frame(vec![Event::PointerMoved(point)]);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    fn key(&mut self, key: egui::Key) -> CapturedFrame {
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }])
    }
}

fn nodes(frame: &CapturedFrame) -> impl Iterator<Item = &(egui::accesskit::NodeId, Node)> {
    frame
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
}

fn row<'a>(frame: &'a CapturedFrame, path: &str) -> &'a Node {
    &nodes(frame)
        .find(|(_, node)| {
            node.role() == egui::accesskit::Role::TreeItem && node.value() == Some(path)
        })
        .unwrap_or_else(|| panic!("missing accessible result row {path}"))
        .1
}

fn descendant(frame: &CapturedFrame, parent: &Node, target: egui::accesskit::NodeId) -> bool {
    parent.children().iter().any(|id| {
        *id == target
            || nodes(frame)
                .find(|(child_id, _)| child_id == id)
                .is_some_and(|(_, child)| descendant(frame, child, target))
    })
}

fn painted_text(frame: &CapturedFrame) -> Vec<&str> {
    frame
        .output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn result_rows_expose_scope_expansion_and_keyboard_focus_without_losing_navigation() {
    // Dropping row semantics, expansion state, or the active descendant must
    // fail even when the same text happens to be painted on the screen.
    let root = container(
        "header",
        "struct",
        vec![leaf("header.id", "9007199254740993", "8", "42")],
    );
    let mut fixture = ResultsFixture::new(vec![root]);
    let frame = fixture.frame(vec![]);
    let header = row(&frame, "header");
    assert_eq!(header.is_expanded(), Some(true));
    assert_eq!(header.level(), Some(1));
    assert_eq!(header.position_in_set(), Some(1));
    assert_eq!(header.size_of_set(), Some(1));
    let id = row(&frame, "header.id");
    assert_eq!(id.level(), Some(2));
    assert_eq!(id.is_expanded(), None);
    assert_eq!(id.position_in_set(), Some(1));
    assert_eq!(id.size_of_set(), Some(1));
    let tree = nodes(&frame)
        .find(|(_, node)| node.role() == egui::accesskit::Role::Tree)
        .unwrap();
    let header_id = nodes(&frame)
        .find(|(_, node)| {
            node.value() == Some("header") && node.role() == egui::accesskit::Role::TreeItem
        })
        .unwrap()
        .0;
    assert_eq!(tree.1.active_descendant(), Some(header_id));
    assert!(descendant(&frame, &tree.1, header_id));
    fixture.click(egui::pos2(50.0, 70.0));
    let frame = fixture.frame(vec![]);
    assert_eq!(row(&frame, "header").is_expanded(), Some(false));
    assert!(
        !nodes(&frame).any(|(_, node)| node.role() == egui::accesskit::Role::TreeItem
            && node.value() == Some("header.id"))
    );
    fixture.snapshot.results[0].children[0].value = Some("2".into());
    let frame = fixture.frame(vec![]);
    assert_eq!(
        row(&frame, "header").is_expanded(),
        Some(false),
        "reparse must not reopen a collapsed path"
    );
    let frame = fixture.key(egui::Key::ArrowRight);
    assert_eq!(
        row(&frame, "header").is_expanded(),
        Some(true),
        "Right must expand the focused container"
    );
    let frame = fixture.key(egui::Key::ArrowDown);
    let id = nodes(&frame)
        .find(|(_, node)| {
            node.value() == Some("header.id") && node.role() == egui::accesskit::Role::TreeItem
        })
        .unwrap()
        .0;
    let tree = nodes(&frame)
        .find(|(_, node)| node.role() == egui::accesskit::Role::Tree)
        .unwrap();
    assert_eq!(tree.1.active_descendant(), Some(id));
    fixture.key(egui::Key::Enter);
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Action(Action::NavigateTemplate {
            start: 9_007_199_254_740_993,
            end: 9_007_199_254_741_000
        })]
    ));
    fixture.key(egui::Key::ArrowLeft);
    let frame = fixture.key(egui::Key::ArrowLeft);
    assert_eq!(row(&frame, "header").is_expanded(), Some(false));
    let frame = fixture.key(egui::Key::ArrowRight);
    assert_eq!(row(&frame, "header").is_expanded(), Some(true));
    assert_eq!(fixture.events.len(), 1);
}

#[test]
fn result_highlighting_tracks_only_the_current_template_range_in_paint_and_semantics() {
    let mut fixture = ResultsFixture::new(vec![
        leaf("first", "16", "4", "1"),
        leaf("second", "20", "1", "2"),
    ]);
    fixture.snapshot.template_range = Some(ByteSelection { start: 16, end: 19 });
    let frame = fixture.frame(vec![]);
    assert_eq!(
        row(&frame, "first").aria_current(),
        Some(egui::accesskit::AriaCurrent::Location)
    );
    assert_eq!(row(&frame, "second").aria_current(), None);
    assert!(frame.output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::epaint::Shape::Rect(r) if r.rect.min == egui::pos2(0.0, 57.0) && r.rect.size() == egui::vec2(2.0, 28.0) && r.fill == egui::Color32::from_rgb(31, 111, 235)
    )));
    fixture.snapshot.template_range = Some(ByteSelection { start: 20, end: 20 });
    let frame = fixture.frame(vec![]);
    assert_eq!(row(&frame, "first").aria_current(), None);
    assert_eq!(
        row(&frame, "second").aria_current(),
        Some(egui::accesskit::AriaCurrent::Location)
    );
    assert!(frame.output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::epaint::Shape::Rect(r) if r.rect.min == egui::pos2(0.0, 85.0) && r.rect.size() == egui::vec2(2.0, 28.0) && r.fill == egui::Color32::from_rgb(31, 111, 235)
    )));
    fixture.snapshot.template_range = None;
    let frame = fixture.frame(vec![]);
    assert_eq!(row(&frame, "first").aria_current(), None);
    assert_eq!(row(&frame, "second").aria_current(), None);
    assert!(!frame.output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::epaint::Shape::Rect(r) if r.rect.width() == 2.0 && r.fill == egui::Color32::from_rgb(31, 111, 235)
    )));
}

#[test]
fn expanded_large_result_arrays_paint_a_bounded_tile_and_navigate_the_last_leaf() {
    let children = (0..1000)
        .map(|index| {
            let mut node = leaf(
                &format!("items[{index}]"),
                &index.to_string(),
                "1",
                &index.to_string(),
            );
            node.name = index.to_string();
            node
        })
        .collect();
    let mut fixture = ResultsFixture::new(vec![container("items", "array", children)]);
    let frame = fixture.frame(vec![]);
    assert_eq!(
        nodes(&frame)
            .filter(|(_, node)| node.role() == egui::accesskit::Role::TreeItem)
            .count(),
        1
    );
    assert_eq!(row(&frame, "items").is_expanded(), Some(false));
    fixture.click(egui::pos2(50.0, 70.0));
    let frame = fixture.frame(vec![]);
    let rows = nodes(&frame)
        .filter(|(_, node)| node.role() == egui::accesskit::Role::TreeItem)
        .count();
    assert!(
        rows > 1 && rows < 60,
        "only viewport rows plus overscan may be painted, got {rows}"
    );
    let frame = fixture.key(egui::Key::End);
    let last = row(&frame, "items[999]");
    assert_eq!(last.position_in_set(), Some(1000));
    assert_eq!(last.size_of_set(), Some(1000));
    let bounds = last.bounds().unwrap();
    assert!(
        bounds.y0 >= 57.0 && bounds.y1 <= 220.01,
        "End must reveal the entire final row: {bounds:?}"
    );
    assert!(
        nodes(&frame)
            .filter(|(_, node)| node.role() == egui::accesskit::Role::TreeItem)
            .count()
            < 60
    );
    assert!(
        fixture.events.is_empty(),
        "focus navigation is not byte activation"
    );
    fixture.click(egui::pos2(
        (bounds.x0 + 40.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    ));
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Action(Action::NavigateTemplate {
            start: 999,
            end: 999
        })]
    ));
}

#[test]
fn result_diagnostics_keep_full_text_and_never_activate_incomplete_values() {
    let mut id = leaf("records[2].id", "9007199254740993", "8", "42");
    id.field_type = FieldType::U64;
    id.endianness = Some(Endian::Big);
    id.enum_label = Some("Answer".into());
    let message = "payload: requested bytes exceed the declared length bound. ".repeat(20);
    let mut issue = leaf("records[2].missing", "10", "8", "");
    issue.kind = "error".into();
    issue.value = None;
    issue.diagnostics.push(Diagnostic {
        code: "invalid_template".into(),
        message: message.clone(),
    });
    let mut fixture = ResultsFixture::new(vec![container("records", "array", vec![id, issue])]);
    let frame = fixture.frame(vec![]);
    assert!(painted_text(&frame).contains(&"u64 · BE"));
    assert!(painted_text(&frame).contains(&"42 · Answer"));
    assert!(painted_text(&frame).contains(&message.as_str()));
    fixture.click(egui::pos2(50.0, 98.0));
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Action(Action::NavigateTemplate {
            start: 9_007_199_254_740_993,
            end: 9_007_199_254_741_000
        })]
    ));
    fixture.click(egui::pos2(50.0, 126.0));
    let frame = fixture.frame(vec![]);
    assert_eq!(
        fixture.events.len(),
        1,
        "error activation must not emit another range"
    );
    let full_detail = frame
        .output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == message => Some(text),
            _ => None,
        })
        .find(|text| text.galley.rows.len() > 1)
        .expect("the detail pane must wrap the whole diagnostic, not the row's ellipsis");
    assert_eq!(full_detail.galley.job.text, message);
    assert_eq!(
        fixture.view.tree.diagnostic_node().unwrap().path,
        "records[2].missing"
    );
    fixture.click(egui::pos2(50.0, 70.0));
    let frame = fixture.frame(vec![]);
    assert_eq!(row(&frame, "records").is_expanded(), Some(false));
    assert!(!nodes(&frame).any(|(_, node)| node.value() == Some("records[2].id")));
}

#[test]
fn result_leaf_columns_comment_and_click_preserve_the_reference_values() {
    let mut magic = leaf("magic", "16", "4", "0x1234");
    magic.field_type = FieldType::U32;
    magic.comment = "Header".into();
    let mut fixture = ResultsFixture::new(vec![magic]);
    let frame = fixture.frame(vec![]);
    let text = painted_text(&frame);
    for expected in [
        "Path",
        "Value / Diagnostic",
        "Offset",
        "Size",
        "Type",
        "magic",
        "0x1234",
        "0x00000010",
        "4 B",
        "u32 · LE",
    ] {
        assert!(
            text.contains(&expected),
            "missing rendered result column {expected}"
        );
    }
    assert_eq!(row(&frame, "magic").tooltip(), Some("Header"));
    let name = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == "magic" => Some(text),
            _ => None,
        })
        .expect("CSS nowrap collapses the leaf's leading spacer at the line start");
    assert_eq!(
        name.pos.x, 12.0,
        "the first leaf glyph must align with the source 2px border + 10px padding"
    );
    assert_eq!(
        row(&frame, "magic").label(),
        Some("magic 0x1234 0x00000010 4 B u32 · LE"),
        "assistive navigation must announce the same decoded columns as the visible row"
    );
    fixture.click(egui::pos2(50.0, 70.0));
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Action(Action::NavigateTemplate {
            start: 16,
            end: 19
        })]
    ));
}

#[test]
fn result_diagnostic_details_use_source_intrinsic_height_and_content_box_limit() {
    // Literal measurements from the retained SFC at 1280x220: a one-line
    // diagnostic is 14 + 5 + 16.5 + 16 padding + 1 border = 52.5px.
    // max-height:110px applies to its content box, so long details occupy
    // 127px, not a fixed per-diagnostic count or half the result viewport.
    for (message, top) in [
        ("Short diagnostic.".into(), 167.5),
        (
            "payload: requested bytes exceed the declared length bound. ".repeat(20),
            93.0,
        ),
    ] {
        let mut issue = leaf("payload", "10", "8", "");
        issue.kind = "error".into();
        issue.value = None;
        issue.diagnostics.push(Diagnostic {
            code: "invalid_template".into(),
            message,
        });
        let mut fixture = ResultsFixture::new(vec![issue]);
        fixture.frame(vec![]);
        fixture.click(egui::pos2(50.0, 70.0));
        let frame = fixture.frame(vec![]);
        assert!(
            frame
                .output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape,
                    egui::epaint::Shape::LineSegment { points, .. }
                    if points[0] == egui::pos2(0.0, top) && points[1] == egui::pos2(1280.0, top)
                )),
            "detail top must be {top}, not an estimated per-message height; actual borders={:?}",
            frame
                .output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::LineSegment { points, .. }
                        if points[0].x == 0.0 && points[1].x == 1280.0 =>
                        Some(points[0].y),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        let heading = frame
            .output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "payload" && text.pos.y > top =>
                {
                    Some(text)
                }
                _ => None,
            })
            .expect("detail heading is visible below its border");
        assert_eq!(heading.pos.y, top + 9.0);
        assert_eq!(
            heading.galley.job.sections[0].format.line_height,
            Some(14.0)
        );
    }
}

#[test]
fn result_header_normalizes_its_name_like_the_status_prompt() {
    for (input, name) in [("", "Untitled"), ("  Draft  ", "Draft")] {
        let mut fixture = ResultsFixture::new(vec![]);
        fixture.snapshot.template_state.source = TemplateSource::Draft;
        fixture.snapshot.template_state.display_name = input.into();
        fixture.snapshot.template_applied = false;
        let frame = fixture.frame(vec![]);
        let text = painted_text(&frame);
        assert!(text.contains(&format!("{name} · Unsaved draft · Pending apply").as_str()));
        assert!(text.contains(&format!("Unsaved template draft: \"{name}\".").as_str()));
    }
}

#[test]
fn result_fold_icon_centers_its_visible_glyph_in_both_directions_at_each_dpi() {
    use hexforge_controller::controller::Theme;
    // Centering the font line box instead of the painted glyph makes the down
    // arrow sit at the button's bottom and the up arrow sit at its top.
    for theme in [Theme::Dark, Theme::Light] {
        for dpr in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for collapsed in [false, true] {
                let mut fixture = ResultsFixture::new(vec![]);
                fixture.snapshot.theme = theme;
                fixture.collapsed = collapsed;
                let style = HexForgeStyle::for_theme(theme, dpr);
                style.apply(&fixture.h.context);
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 220.0),
                    )),
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(dpr);
                let frame = fixture.h.frame(input, |ui| {
                    fixture.events.extend(fixture.view.show(
                        ui,
                        &style,
                        &fixture.snapshot,
                        fixture.collapsed,
                    ));
                });
                let label = if collapsed {
                    "Expand parsed results"
                } else {
                    "Collapse parsed results"
                };
                let button = frame.widgets.iter().find(|w| w.label == label).unwrap();
                assert_eq!(
                    button.rect,
                    egui::Rect::from_min_size(egui::pos2(1250.0, 3.5), egui::vec2(22.0, 22.0),)
                );
                let glyph = if collapsed { "⌃" } else { "⌄" };
                let text = frame
                    .output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == glyph => Some(text),
                        _ => None,
                    })
                    .expect("the actual fold control paints its arrow");
                let center = text.pos + text.galley.mesh_bounds.center().to_vec2();
                assert!((center - egui::pos2(1261.0, 14.5)).length() < 0.01,
                    "visible {glyph} must be centered, not its font's baseline: {center:?}, {theme:?}, DPR {dpr}");
                assert!(
                    fixture.events.is_empty(),
                    "rendering must not toggle the panel"
                );
            }
        }
    }
}

#[test]
fn result_container_names_reserve_the_source_chevron_and_space_advance() {
    let mut fixture = ResultsFixture::new(vec![container(
        "header",
        "struct",
        vec![leaf("header.id", "0", "1", "1")],
    )]);
    for collapsed in [false, true] {
        if collapsed {
            fixture.click(egui::pos2(50.0, 70.0));
        }
        let frame = fixture.frame(vec![]);
        let first_name_glyph_x = frame
            .output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.job.text == "header"
                        || text.galley.job.text.ends_with(" header") =>
                {
                    text.galley.rows[0]
                        .glyphs
                        .iter()
                        .find(|glyph| glyph.chr == 'h')
                        .map(|glyph| text.pos.x + glyph.pos.x)
                }
                _ => None,
            })
            .unwrap();
        // Source at 1280px: 12px inset, 11px fallback chevron advance,
        // 7.1875px space advance. Rasterization may round that tiny fraction.
        assert!((first_name_glyph_x - 30.1875).abs() < 0.05, "container name must retain source chevron spacing, not inherit the native fallback glyph's narrower advance: {first_name_glyph_x}");
    }
}

#[test]
fn accessibility_result_focus_updates_the_keyboard_row_without_activating_bytes() {
    let mut fixture = ResultsFixture::new(vec![
        leaf("first", "16", "4", "1"),
        leaf("second", "20", "1", "2"),
    ]);
    let frame = fixture.frame(vec![]);
    let second = nodes(&frame)
        .find(|(_, node)| {
            node.value() == Some("second") && node.role() == egui::accesskit::Role::TreeItem
        })
        .unwrap()
        .0;
    fixture.frame(vec![Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Focus,
            target_node: second,
            target_tree: egui::accesskit::TreeId::ROOT,
            data: None,
        },
    )]);
    let frame = fixture.frame(vec![]);
    let tree = nodes(&frame)
        .find(|(_, node)| node.role() == egui::accesskit::Role::Tree)
        .unwrap();
    assert_eq!(
        tree.1.active_descendant(),
        Some(second),
        "native focus must update the same stable row as pointer/keyboard focus"
    );
    assert!(fixture.events.is_empty(), "focus is not activation");
    fixture.key(egui::Key::ArrowUp);
    fixture.key(egui::Key::Enter);
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Action(Action::NavigateTemplate {
            start: 16,
            end: 19
        })]
    ));
    let frame = fixture.key(egui::Key::Tab);
    let collapse = nodes(&frame)
        .find(|(_, node)| node.label() == Some("Collapse parsed results"))
        .unwrap()
        .0;
    let focused = fixture
        .h
        .context
        .memory(|memory| memory.focused())
        .unwrap()
        .accesskit_id();
    assert_eq!(
        focused, collapse,
        "only the current row/tree owner belongs in Tab order, not every virtual row"
    );
}

#[test]
fn collapsed_results_keep_template_status_and_dispatch_only_the_expand_action() {
    let mut fixture = ResultsFixture::new(vec![leaf("id", "0", "1", "42")]);
    fixture.collapsed = true;
    fixture.snapshot.template_applied = false;
    let frame = fixture.frame(vec![]);
    let text = painted_text(&frame);
    assert!(text.contains(&"PARSED RESULTS"));
    assert!(text.contains(&"header.json · Saved · Pending apply"));
    assert!(!text.contains(&"Path"));
    assert!(!nodes(&frame).any(|(_, node)| node.role() == egui::accesskit::Role::Tree));
    let expand = frame
        .widgets
        .iter()
        .find(|widget| widget.label == "Expand parsed results")
        .unwrap()
        .rect
        .center();
    fixture.click(expand);
    assert!(matches!(
        fixture.events.as_slice(),
        [ControllerEvent::Invoke {
            command: Command::ToggleRightPanel,
            ..
        }]
    ));
    fixture.snapshot.template_applied = true;
    let frame = fixture.frame(vec![]);
    assert!(painted_text(&frame).contains(&"header.json · Saved · Applied"));
}

#[test]
fn inactive_results_render_exact_state_prompts_without_operation_buttons() {
    for (has_file, source, name, message) in [
        (false, TemplateSource::None, "", "No template loaded."),
        (
            false,
            TemplateSource::File,
            "header.json",
            "Template: \"header.json\" loaded. Open binary file to preview.",
        ),
        (
            true,
            TemplateSource::None,
            "",
            "No template loaded. Load or create template from Template menu.",
        ),
        (
            false,
            TemplateSource::Draft,
            "Draft",
            "Unsaved template draft: \"Draft\".",
        ),
        (
            true,
            TemplateSource::Draft,
            "Draft",
            "Unsaved template draft: \"Draft\".",
        ),
    ] {
        let mut fixture = ResultsFixture::new(vec![]);
        if !has_file {
            fixture.snapshot.file = None;
        }
        fixture.snapshot.template_state.source = source;
        fixture.snapshot.template_state.display_name = name.into();
        fixture.snapshot.template_applied = false;
        let frame = fixture.frame(vec![]);
        assert!(
            painted_text(&frame).contains(&message),
            "missing prompt {message}"
        );
        let buttons: Vec<_> = frame
            .widgets
            .iter()
            .filter(|widget| widget.role == egui::accesskit::Role::Button)
            .map(|widget| widget.label.as_str())
            .collect();
        assert_eq!(
            buttons,
            ["Collapse parsed results"],
            "no operation controls belong in this state"
        );
    }
}

#[test]
fn pending_results_actions_use_the_shared_policy_and_distinct_dispatch() {
    let mut fixture = ResultsFixture::new(vec![]);
    fixture.snapshot.template_applied = false;
    fixture.snapshot.policy.has_file = true;
    fixture.snapshot.policy.template_active = true;
    fixture.snapshot.policy.template_valid = true;
    fixture.snapshot.policy.template_has_fields = true;
    let frame = fixture.frame(vec![]);
    assert!(painted_text(&frame).contains(&"Template: \"header.json\" loaded, pending apply."));
    let mut actions: Vec<_> = frame
        .widgets
        .iter()
        .filter(|widget| widget.role == egui::accesskit::Role::Button && widget.rect.top() > 29.0)
        .collect();
    // AccessKit updates are keyed by widget ID, not paint/DOM order. Observe
    // the actual left-to-right action layout rather than map iteration.
    actions.sort_by(|left, right| left.rect.left().total_cmp(&right.rect.left()));
    assert_eq!(
        actions
            .iter()
            .map(|widget| widget.label.as_str())
            .collect::<Vec<_>>(),
        ["Template Editor", "Apply Template"]
    );
    assert!(actions.iter().all(|widget| widget.enabled));
    let editor = actions[0].rect.center();
    let apply = actions[1].rect.center();
    fixture.click(editor);
    fixture.click(apply);
    assert!(matches!(
        fixture.events.as_slice(),
        [
            ControllerEvent::Invoke {
                command: Command::TemplateEditor,
                ..
            },
            ControllerEvent::Invoke {
                command: Command::ApplyTemplate,
                ..
            },
        ]
    ));
    fixture.events.clear();
    fixture.snapshot.policy.template_has_fields = false;
    let frame = fixture.frame(vec![]);
    assert!(
        !frame
            .widgets
            .iter()
            .find(|widget| widget.label == "Apply Template")
            .unwrap()
            .enabled
    );
    fixture.click(apply);
    assert!(fixture.events.is_empty());
    fixture.snapshot.policy.operation_busy = true;
    let frame = fixture.frame(vec![]);
    assert!(
        !frame
            .widgets
            .iter()
            .find(|widget| widget.label == "Template Editor")
            .unwrap()
            .enabled
    );
    fixture.click(editor);
    assert!(fixture.events.is_empty());
}

#[test]
fn applied_empty_results_keep_the_tree_and_columns_without_pending_controls() {
    let mut fixture = ResultsFixture::new(vec![]);
    let frame = fixture.frame(vec![]);
    assert!(nodes(&frame).any(|(_, node)| node.role() == egui::accesskit::Role::Tree));
    let text = painted_text(&frame);
    assert!(text.contains(&"Path"));
    assert!(text.contains(&"No parsed fields."));
    assert!(!text.iter().any(|text| text.contains("pending apply.")));
    let buttons: Vec<_> = frame
        .widgets
        .iter()
        .filter(|widget| widget.role == egui::accesskit::Role::Button)
        .map(|widget| widget.label.as_str())
        .collect();
    assert_eq!(buttons, ["Collapse parsed results"]);
    fixture.snapshot.file = None;
    fixture.frame(vec![]); // An applied template without a binary is still loaded, not a result tree.
    let frame = fixture.frame(vec![]);
    assert!(!nodes(&frame).any(|(_, node)| node.role() == egui::accesskit::Role::Tree));
}

#[test]
fn overflowing_result_tree_reserves_the_source_gutter_and_keeps_columns_sticky() {
    // The source thin scrollbar occupies 10px of the entire list, including
    // its sticky 28px heading. Floating bars must not shift or cover columns.
    let mut fixture = ResultsFixture::new(
        (0..20)
            .map(|i| leaf(&format!("item{i}"), &i.to_string(), "1", &i.to_string()))
            .collect(),
    );
    fixture.frame(vec![]);
    let first = fixture.frame(vec![]);
    let bounds = row(&first, "item0").bounds().unwrap();
    assert_eq!((bounds.x0, bounds.x1), (0.0, 1270.0));
    let bar = &nodes(&first)
        .find(|(_, n)| n.role() == egui::accesskit::Role::ScrollBar)
        .expect("The overflowing list needs its real source-styled scrollbar")
        .1;
    assert_eq!(
        bar.bounds().unwrap(),
        egui::accesskit::Rect {
            x0: 1270.0,
            y0: 29.0,
            x1: 1280.0,
            y1: 220.0,
        }
    );
    let heading = first
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Offset" => Some(t.pos),
            _ => None,
        })
        .unwrap();
    assert!(
        (heading.x - 944.0).abs() < 0.01,
        "Sticky column starts at {heading:?}"
    );
    let leaf_value = first
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "0x00000000" => Some(t.pos),
            _ => None,
        })
        .unwrap();
    assert!(
        (leaf_value.x - 944.0).abs() < 0.01,
        "Row's 2px left border participates in its independent grid layout"
    );
    fixture.frame(vec![
        Event::PointerMoved(egui::pos2(200.0, 130.0)),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -100.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        },
    ]);
    for _ in 0..20 {
        fixture.frame(vec![]);
    }
    let scrolled = fixture.frame(vec![]);
    let sticky = scrolled
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Offset" => Some(t.pos),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        sticky, heading,
        "Scrolling content must not move the column heading"
    );
    let item = row(&scrolled, "item4");
    assert!(
        item.bounds().unwrap().y0 < 169.0,
        "The real wheel must move result rows"
    );
}

#[test]
fn result_scrollbar_matches_the_reference_thumb_extent_and_drag_travel() {
    // The retained production list was measured at DPR1: 191px clientHeight,
    // 560px scrollHeight (19 rows plus its sticky heading). Its real scrollbar
    // paints a 57px thumb and a 10px noncentral drag scrolls 34px. A shortened
    // thumb changes both the appearance and the pointer-to-row relationship.
    let mut fixture = ResultsFixture::new(
        (0..19)
            .map(|i| leaf(&format!("item{i}"), &i.to_string(), "1", &i.to_string()))
            .collect(),
    );
    for _ in 0..3 {
        fixture.frame(vec![]);
    }
    let frame = fixture.frame(vec![]);
    let thumb = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(r)
                if r.fill == egui::Color32::from_rgb(59, 69, 81)
                    && r.rect.left() == 1272.0
                    && r.rect.width() == 6.0 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("The result scrollbar must paint its visible thumb");
    assert_eq!(
        thumb,
        egui::Rect::from_min_size(egui::pos2(1272.0, 41.0), egui::vec2(6.0, 57.0))
    );
    for events in [
        vec![Event::PointerMoved(egui::pos2(1275.0, 49.0))],
        vec![Event::PointerButton {
            pos: egui::pos2(1275.0, 49.0),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![Event::PointerMoved(egui::pos2(1275.0, 59.0))],
        vec![Event::PointerButton {
            pos: egui::pos2(1275.0, 59.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        fixture.frame(events);
    }
    let dragged = fixture.frame(vec![]);
    assert_eq!(row(&dragged, "item2").bounds().unwrap().y0, 79.0);
    assert!(
        fixture.events.is_empty(),
        "Thumb input must not select parsed bytes"
    );
}

#[test]
fn result_scrollbar_keeps_the_reference_minimum_thumb_for_a_long_list() {
    // The isolated source theme saturates at an 11px thumb, not the generic
    // egui minimum. This keeps the same drag travel for a bounded row tile.
    let mut fixture = ResultsFixture::new(
        (0..200)
            .map(|i| leaf(&format!("item{i}"), &i.to_string(), "1", &i.to_string()))
            .collect(),
    );
    for _ in 0..3 {
        fixture.frame(vec![]);
    }
    let frame = fixture.frame(vec![]);
    let thumb = frame
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(r)
                if r.fill == egui::Color32::from_rgb(59, 69, 81)
                    && r.rect.left() == 1272.0
                    && r.rect.width() == 6.0 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("A long result list still paints its bounded scrollbar thumb");
    assert_eq!(
        thumb,
        egui::Rect::from_min_size(egui::pos2(1272.0, 41.0), egui::vec2(6.0, 11.0))
    );
    assert!(
        nodes(&frame)
            .filter(|(_, n)| n.role() == egui::accesskit::Role::TreeItem)
            .count()
            < 20,
        "Matching the thumb must not unvirtualize the result list"
    );
}

#[test]
fn result_scrollbar_arrow_and_noncentral_drag_keep_their_applied_offsets() {
    let mut fixture = ResultsFixture::new(
        (0..20)
            .map(|i| leaf(&format!("item{i}"), &i.to_string(), "1", &i.to_string()))
            .collect(),
    );
    for _ in 0..3 {
        fixture.frame(vec![]);
    }
    fixture.click(egui::pos2(1275.0, 214.0));
    let clicked = fixture.frame(vec![]);
    assert_eq!(
        row(&clicked, "item2").bounds().unwrap().y0,
        73.0,
        "The down arrow's 40px row scroll must survive the following frame"
    );
    // Grab inside the thumb away from its centre, then move outside the lower
    // end: capture clamps to EOF and preserves the pointer's starting offset.
    fixture.frame(vec![Event::PointerMoved(egui::pos2(1275.0, 61.0))]);
    fixture.frame(vec![Event::PointerButton {
        pos: egui::pos2(1275.0, 61.0),
        button: PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }]);
    fixture.frame(vec![Event::PointerMoved(egui::pos2(1275.0, 260.0))]);
    fixture.frame(vec![]);
    fixture.frame(vec![Event::PointerButton {
        pos: egui::pos2(1275.0, 260.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let bottom = fixture.frame(vec![]);
    assert_eq!(
        row(&bottom, "item19").bounds().unwrap().y1,
        220.0,
        "The final result remains visible with no empty space after the last row"
    );
    assert!(
        fixture.events.is_empty(),
        "Scrollbar operations must not navigate or select parsed fields"
    );
}

#[test]
fn horizontal_result_scroll_moves_the_sticky_heading_with_rows_and_preserves_native_grid_minima() {
    let mut fixture = ResultsFixture::new(
        (0..20)
            .map(|i| leaf(&format!("item{i}"), &i.to_string(), "1", &i.to_string()))
            .collect(),
    );
    let size = egui::vec2(480.0, 220.0);
    for _ in 0..3 {
        fixture.frame_size(size, vec![]);
    }
    let first = fixture.frame_size(size, vec![]);
    let text_position = |frame: &CapturedFrame, label: &str| {
        frame
            .output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(text_position(&first, "Value / Diagnostic").x, 138.0);
    assert_eq!(text_position(&first, "0").x, 140.0);
    let heading = text_position(&first, "Offset");
    let value = text_position(&first, "0x00000000");
    let bar = &nodes(&first)
        .find(|(_, n)| n.label() == Some("Horizontal scroll bar"))
        .unwrap()
        .1;
    assert_eq!(
        bar.bounds().unwrap(),
        egui::accesskit::Rect {
            x0: 0.0,
            y0: 210.0,
            x1: 470.0,
            y1: 220.0
        }
    );
    for events in [
        vec![Event::PointerMoved(egui::pos2(464.0, 215.0))],
        vec![Event::PointerButton {
            pos: egui::pos2(464.0, 215.0),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![Event::PointerButton {
            pos: egui::pos2(464.0, 215.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        fixture.frame_size(size, events);
    }
    let scrolled = fixture.frame_size(size, vec![]);
    assert_eq!(text_position(&scrolled, "0x00000000").x, value.x - 40.0);
    assert_eq!(
        text_position(&scrolled, "Offset").x,
        heading.x - 40.0,
        "The heading is sticky vertically, not horizontally"
    );
    assert_eq!(text_position(&scrolled, "Offset").y, heading.y);
    assert!(fixture.events.is_empty());
}
