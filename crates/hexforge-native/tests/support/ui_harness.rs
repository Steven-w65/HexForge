use eframe::egui;
use hexforge_controller::controller::WindowSnapshot;
use hexforge_native::app::NativeApp;
pub struct NativeUiHarness {
    pub context: egui::Context,
}
pub struct CapturedFrame {
    pub output: egui::FullOutput,
    pub widgets: Vec<CapturedWidget>,
    pub snapshot: Option<WindowSnapshot>,
}
pub struct CapturedWidget {
    pub label: String,
    pub role: egui::accesskit::Role,
    /// Logical client coordinates; AccessKit's root transform applies DPI.
    pub rect: egui::Rect,
    pub enabled: bool,
    pub toggled: Option<egui::accesskit::Toggled>,
}
// Headless assertions inspect real paint/accessibility output. A renderer will
// consume deltas in pixel tests; model-only tests explicitly release them.
impl Drop for CapturedFrame {
    fn drop(&mut self) {
        self.output.textures_delta.clear();
    }
}
impl NativeUiHarness {
    pub fn new() -> Self {
        let context = egui::Context::default();
        context.enable_accesskit();
        hexforge_native::style::fonts::install(&context);
        Self { context }
    }
    pub fn frame(
        &self,
        mut input: egui::RawInput,
        render: impl FnMut(&mut egui::Ui),
    ) -> CapturedFrame {
        input.screen_rect.get_or_insert(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(660.0, 760.0),
        ));
        let output = self.context.run_ui(input, render);
        let widgets = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|tree| {
                tree.nodes
                    .iter()
                    .filter_map(|(_, node)| {
                        let label = node.label().or_else(|| node.value())?;
                        let bounds = node.bounds()?;
                        Some(CapturedWidget {
                            label: label.to_owned(),
                            role: node.role(),
                            rect: egui::Rect::from_min_max(
                                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                            ),
                            enabled: !node.is_disabled(),
                            toggled: node.toggled(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        CapturedFrame {
            output,
            widgets,
            snapshot: None,
        }
    }
    pub fn main_frame(&self, app: &mut NativeApp, input: egui::RawInput) -> CapturedFrame {
        let mut frame = self.frame(input, |ui| app.paint_main(ui));
        frame.snapshot = Some(WindowSnapshot::Main(app.main_snapshot()));
        frame
    }
    pub fn editor_frame(&self, app: &mut NativeApp, input: egui::RawInput) -> CapturedFrame {
        let mut frame = self.frame(input, |ui| app.paint_editor(ui));
        frame.snapshot = Some(WindowSnapshot::Editor(app.editor_snapshot()));
        frame
    }
}
