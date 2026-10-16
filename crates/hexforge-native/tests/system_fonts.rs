//! The retained Windows renderer resolves generic monospace to NSimSun on
//! this reference machine. Use the OS resource, never redistribute its file.
#![cfg(windows)]
use hexforge_native::platform::fonts::read_system_font;

#[test]
fn system_font_matches_the_requested_face_and_has_real_cjk_glyphs() {
    let font = read_system_font("NSimSun", 32 * 1024 * 1024).expect("Reference OS font");
    let face = ttf_parser::Face::parse(&font.bytes, font.index).expect("Original OS font data");
    assert!(face
        .names()
        .into_iter()
        .any(|name| name.to_string().as_deref() == Some("NSimSun")));
    for ch in ['A', '汉', '字'] {
        let glyph = face
            .glyph_index(ch)
            .expect("Real system fallback glyph, not tofu");
        assert_ne!(glyph.0, 0);
        assert!(face.glyph_bounding_box(glyph).is_some());
    }
}

#[test]
fn system_font_read_is_bounded_and_rejects_silent_gdi_substitution() {
    assert!(
        read_system_font("NSimSun", 1).is_none(),
        "Check size before allocating the font"
    );
    assert!(
        read_system_font("HexForge missing font 62A7", 32 * 1024 * 1024).is_none(),
        "Do not claim a substituted face matches the source"
    );
    assert!(read_system_font("NSimSun\0Other", 32 * 1024 * 1024).is_none());
}
