use eframe::egui::{
    self,
    accesskit::{Node, Role, Toggled},
};
use hexforge_controller::controller::InitialState;
use hexforge_core::operations::CoreService;
use hexforge_native::app::NativeApp;
use std::sync::Arc;

fn frame(app: &mut NativeApp, ctx: &egui::Context, mut events: Vec<egui::Event>) -> Vec<Node> {
    let modifiers = events
        .iter()
        .rev()
        .find_map(|event| match event {
            egui::Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    output.textures_delta.clear();
    output
        .platform_output
        .accesskit_update
        .expect("real accessible output")
        .nodes
        .into_iter()
        .map(|(_, node)| node)
        .collect()
}
fn node<'a>(nodes: &'a [Node], label: &str) -> &'a Node {
    nodes
        .iter()
        .find(|n| n.label() == Some(label))
        .unwrap_or_else(|| {
            panic!(
                "missing accessible {label}; labels={:?}",
                nodes.iter().filter_map(Node::label).collect::<Vec<_>>()
            )
        })
}
fn app() -> (NativeApp, egui::Context) {
    let mut app = NativeApp::new(InitialState::default(), Arc::new(CoreService::default()));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.attach(&ctx);
    (app, ctx)
}

#[test]
fn main_controls_expose_their_existing_names_and_active_states() {
    let (mut app, ctx) = app();
    let nodes = frame(&mut app, &ctx, vec![]);
    for label in ["File", "Edit", "Navigate", "Template", "View"] {
        assert_eq!(node(&nodes, label).role(), Role::MenuItem);
        assert_eq!(node(&nodes, label).is_expanded(), Some(false));
    }
    assert_eq!(node(&nodes, "Open File").role(), Role::Button);
    assert_eq!(node(&nodes, "Collapse parsed results").role(), Role::Button);
    assert_eq!(node(&nodes, "16").toggled(), Some(Toggled::True));
    assert_eq!(node(&nodes, "32").toggled(), Some(Toggled::False));
    app.invoke(
        hexforge_controller::command::Command::Row32,
        Default::default(),
    );
    let nodes = frame(&mut app, &ctx, vec![]);
    assert_eq!(node(&nodes, "16").toggled(), Some(Toggled::False));
    assert_eq!(node(&nodes, "32").toggled(), Some(Toggled::True));
}

#[test]
fn accessible_menu_items_keep_disabled_policy_reasons() {
    let (mut app, ctx) = app();
    frame(&mut app, &ctx, vec![]);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        }],
    );
    let nodes = frame(&mut app, &ctx, vec![]);
    assert_eq!(node(&nodes, "File").is_expanded(), Some(true));
    assert_eq!(node(&nodes, "Open File…").role(), Role::MenuItem);
    assert!(!node(&nodes, "Open File…").is_disabled());
    let close = node(&nodes, "Close File");
    assert_eq!(close.role(), Role::MenuItem);
    assert!(close.is_disabled());
    assert_eq!(close.description(), Some("Open a binary file first."));
}
