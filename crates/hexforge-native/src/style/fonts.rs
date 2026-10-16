use eframe::egui::{self, FontFamily, FontId};
pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("HexForge SemiBold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("HexForge Bold".into()))
}
pub fn generic_monospace(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("HexForge OS Monospace".into()))
}
/// Retain CSS letter-spacing in continuous local coordinates.
pub fn spaced(
    text: &str,
    font_id: FontId,
    color: egui::Color32,
    spacing: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        text.into(),
        egui::TextFormat {
            font_id,
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    job.round_output_to_gui = false;
    job
}
pub fn install(ctx: &egui::Context) {
    let mut d = egui::FontDefinitions::empty();
    d.font_data.insert(
        "HexForge Regular".into(),
        egui::FontData::from_static(crate::assets::REGULAR_FONT).into(),
    );
    d.font_data.insert(
        "HexForge SemiBold".into(),
        egui::FontData::from_static(crate::assets::SEMIBOLD_FONT).into(),
    );
    d.font_data.insert(
        "HexForge Bold".into(),
        egui::FontData::from_static(crate::assets::BOLD_FONT).into(),
    );
    d.font_data.insert(
        "HexForge Symbols".into(),
        egui::FontData::from_static(crate::assets::SYMBOL_FONT).into(),
    );
    d.font_data.insert(
        "HexForge Symbol Text".into(),
        egui::FontData::from_static(crate::assets::SYMBOL_TEXT_FONT).into(),
    );
    // The reference webfont relies on OS fallback for these symbols. Keep the
    // original face first; math supplies U+29C9 (Duplicate) missing in both
    // symbol faces, without replacing the already matched text/other icons.
    d.font_data.insert(
        "HexForge Math Symbols".into(),
        egui::FontData::from_static(crate::assets::MATH_FONT).into(),
    );
    let system_face = crate::platform::fonts::generic_monospace().map(|font| {
        let mut data = egui::FontData::from_static(&font.bytes);
        data.index = font.index;
        d.font_data
            .insert("HexForge OS Monospace".into(), data.into());
        "HexForge OS Monospace".to_owned()
    });
    d.families.insert(
        FontFamily::Name("HexForge OS Monospace".into()),
        system_face
            .clone()
            .into_iter()
            .chain(std::iter::once("HexForge Regular".into()))
            .collect(),
    );
    for family in [FontFamily::Monospace, FontFamily::Proportional] {
        d.families.insert(
            family,
            vec![
                "HexForge Regular".into(),
                "HexForge Symbol Text".into(),
                "HexForge Symbols".into(),
                "HexForge Math Symbols".into(),
            ],
        );
    }
    d.families.insert(
        FontFamily::Name("HexForge SemiBold".into()),
        vec![
            "HexForge SemiBold".into(),
            "HexForge Symbol Text".into(),
            "HexForge Symbols".into(),
            "HexForge Math Symbols".into(),
        ],
    );
    d.families.insert(
        FontFamily::Name("HexForge Bold".into()),
        vec![
            "HexForge Bold".into(),
            "HexForge Symbol Text".into(),
            "HexForge Symbols".into(),
            "HexForge Math Symbols".into(),
        ],
    );
    // Original webfont first, then licensed embedded symbol faces. Installed
    // OS fallback fills missing CJK without changing matched Latin glyphs.
    if let Some(face) = system_face {
        for family in [
            FontFamily::Monospace,
            FontFamily::Proportional,
            FontFamily::Name("HexForge SemiBold".into()),
            FontFamily::Name("HexForge Bold".into()),
        ] {
            d.families.get_mut(&family).unwrap().push(face.clone());
        }
    }
    ctx.set_fonts(d);
}
