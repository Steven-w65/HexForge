//! All shipping assets are compile-time bytes, never sidecar files.
pub const REGULAR_FONT: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
pub const SEMIBOLD_FONT: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf");
pub const BOLD_FONT: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
pub const SYMBOL_FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansSymbols2-Regular.ttf");
pub const SYMBOL_TEXT_FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansSymbols-Regular.ttf");
pub const MATH_FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansMath-Regular.ttf");
pub const FONT_LICENSE: &str = include_str!("../assets/fonts/OFL.txt");
pub const SYMBOL_LICENSE: &str = include_str!("../assets/fonts/NotoSymbols-OFL.txt");
pub const MATH_LICENSE: &str = include_str!("../assets/fonts/NotoMath-OFL.txt");
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
pub fn icon() -> eframe::egui::IconData {
    let image = image::load_from_memory(ICON_PNG)
        .expect("embedded HexForge icon")
        .to_rgba8();
    eframe::egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
