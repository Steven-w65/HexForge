use eframe::egui::{self, Event, PointerButton};
use hexforge_controller::{
    action::Action,
    controller::{AppController, ControllerEvent, InitialState, WindowSnapshot},
    shortcuts::WindowKind,
};
use hexforge_core::{
    dto::FileInfoDto,
    template::{FieldType, ParsedNode},
};
use hexforge_native::views::results::ResultView;
#[test]
fn applied_empty_results_show_the_reference_no_fields_message() {
    let c = AppController::new(InitialState::default());
    let WindowSnapshot::Main(mut s) = c.snapshot(WindowKind::Main) else {
        unreachable!()
    };
    s.file = Some(FileInfoDto {
        name: "a".into(),
        path: "a".into(),
        size: "1".into(),
        revision: "0".into(),
        dirty: false,
    });
    s.template_state.source = hexforge_controller::template::status::TemplateSource::Draft;
    s.template_applied = true;
    let ctx = egui::Context::default();
    hexforge_native::style::fonts::install(&ctx);
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut view = ResultView::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 220.0),
            )),
            ..Default::default()
        },
        |ui| {
            view.show(ui, &style, &s, false);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(t) if t.galley.job.text=="No parsed fields.")));
}
#[test]
fn real_result_leaf_click_emits_exact_template_navigation() {
    let c = AppController::new(InitialState::default());
    let WindowSnapshot::Main(mut s) = c.snapshot(WindowKind::Main) else {
        unreachable!()
    };
    s.file = Some(FileInfoDto {
        name: "a".into(),
        path: "a".into(),
        size: "9007199254741001".into(),
        revision: "0".into(),
        dirty: false,
    });
    s.template_state.source = hexforge_controller::template::status::TemplateSource::Draft;
    s.template_applied = true;
    s.results = vec![ParsedNode {
        kind: "leaf".into(),
        path: "header.id".into(),
        name: "id".into(),
        field_type: FieldType::U64,
        offset: Some("9007199254740993".into()),
        length: Some("8".into()),
        value: Some("42".into()),
        endianness: None,
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    }];
    let ctx = egui::Context::default();
    hexforge_native::style::fonts::install(&ctx);
    let style = hexforge_native::style::HexForgeStyle::for_theme(s.theme, 1.0);
    let mut view = ResultView::default();
    let mut actions = vec![];
    let mut frame = |events: Vec<Event>| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 220.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions.extend(view.show(ui, &style, &s, false)),
        );
        output.textures_delta.clear();
    };
    let p = egui::pos2(60.0, 70.0);
    frame(vec![]);
    frame(vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }]);
    }
    assert!(actions.iter().any(|e| matches!(
        e,
        ControllerEvent::Action(Action::NavigateTemplate {
            start: 9_007_199_254_740_993,
            end: 9_007_199_254_741_000
        })
    )));
}
