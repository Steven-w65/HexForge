//! Development capture contract, shared by regression tests and the example.
//! The production portable application has no capture flag or output files.
#[derive(Clone, Debug)]
pub struct CaptureSpec {
    pub scene: String,
    pub theme: String,
    pub client_size: [u32; 2],
    pub dpr: f64,
    physical_size: [u32; 2],
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneConfig {
    pub name: String,
    pub surface: String,
    pub menu: Option<String>,
    pub binary: Option<String>,
    pub edit: Option<SceneEdit>,
    pub search: Option<String>,
    pub goto: Option<String>,
    pub mode: Option<String>,
    pub navigate_row: Option<u64>,
    pub bytes_per_row: Option<u32>,
    pub template: Option<String>,
    pub apply: Option<bool>,
    pub selected_path: Option<String>,
    pub clear_selection: Option<u64>,
    pub editor_name: Option<String>,
    pub editor_close: Option<bool>,
    pub save_error: Option<bool>,
    pub busy: Option<bool>,
    pub error: Option<bool>,
    pub close_binary: Option<bool>,
    pub flyout: Option<String>,
    pub focus_result: Option<String>,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneEdit {
    pub offset: u64,
    pub value: u8,
}
impl CaptureSpec {
    pub fn config(&self) -> SceneConfig {
        Self::scene_config(&self.scene).expect("capture scene was validated")
    }
    fn scene_config(name: &str) -> Option<SceneConfig> {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/capture-scenes.json"))
                .expect("reviewed fixture catalogue");
        let scenes: Vec<SceneConfig> =
            serde_json::from_value(value["scenes"].clone()).expect("typed capture scene");
        scenes.into_iter().find(|s| s.name == name)
    }
}
impl CaptureSpec {
    pub fn new(scene: &str, theme: &str, size: [u32; 2], dpr: f64) -> Result<Self, String> {
        if Self::scene_config(scene).is_none() {
            return Err("Unknown real-interface capture scene".into());
        }
        if !matches!(theme, "dark" | "light") {
            return Err("Unknown theme".into());
        }
        if !dpr.is_finite() || !(0.5..=4.0).contains(&dpr) || size.contains(&0) {
            return Err("Invalid client dimensions or DPI".into());
        }
        let physical = size.map(|n| f64::from(n) * dpr);
        if physical.iter().any(|n| n.fract() != 0.0 || *n > 16_384.0)
            || physical[0] * physical[1] > 32_000_000.0
        {
            return Err("Capture framebuffer is unbounded or not pixel-exact".into());
        }
        Ok(Self {
            scene: scene.into(),
            theme: theme.into(),
            client_size: size,
            dpr,
            physical_size: physical.map(|n| n as u32),
        })
    }
    pub fn physical_size(&self) -> [u32; 2] {
        self.physical_size
    }
    pub fn accepts_frame(&self, physical: [u32; 2], dpr: f64) -> bool {
        physical == self.physical_size && (dpr - self.dpr).abs() < 1e-6
    }
}
