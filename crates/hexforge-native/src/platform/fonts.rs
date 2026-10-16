//! Read the same installed font resource used by the reference Windows UI.
//! Nothing is copied to disk, embedded in the EXE, registered, or modified.
use std::sync::OnceLock;

pub struct SystemFont {
    pub bytes: Vec<u8>,
    pub index: u32,
}

/// Reject substitution and excessive reads before allocating. The generic
/// monospace face was observed through DevTools' actual platform-glyph data;
/// it also provides the CJK fallback that the retained webfont delegates to OS.
pub fn generic_monospace() -> Option<&'static SystemFont> {
    static FONT: OnceLock<Option<SystemFont>> = OnceLock::new();
    FONT.get_or_init(|| read_system_font("NSimSun", 32 * 1024 * 1024))
        .as_ref()
}

#[cfg(not(windows))]
pub fn read_system_font(_family: &str, _maximum: usize) -> Option<SystemFont> {
    None
}

#[cfg(windows)]
pub fn read_system_font(family: &str, maximum: usize) -> Option<SystemFont> {
    use windows_sys::Win32::Graphics::Gdi::*;
    // LOGFONT face names are bounded UTF-16 strings; do not permit truncation
    // or embedded terminators to silently select a different face.
    let mut name = family.encode_utf16().collect::<Vec<_>>();
    if name.is_empty() || name.len() >= 32 || name.contains(&0) || maximum == 0 {
        return None;
    }
    name.push(0);
    struct SelectedFont {
        dc: HDC,
        font: HFONT,
        previous: HGDIOBJ,
    }
    impl Drop for SelectedFont {
        fn drop(&mut self) {
            unsafe {
                if !self.previous.is_null() {
                    SelectObject(self.dc, self.previous);
                }
                if !self.font.is_null() {
                    DeleteObject(self.font);
                }
                if !self.dc.is_null() {
                    DeleteDC(self.dc);
                }
            }
        }
    }
    // SAFETY: Owned compatible DC/font handles are released on every exit.
    // All output pointers are sized writable Rust buffers, never user paths.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let mut handles = SelectedFont {
            dc,
            font: std::ptr::null_mut(),
            previous: std::ptr::null_mut(),
        };
        handles.font = CreateFontW(
            -12,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            0,
            FIXED_PITCH as u32,
            name.as_ptr(),
        );
        if handles.font.is_null() {
            return None;
        }
        handles.previous = SelectObject(dc, handles.font);
        if handles.previous.is_null() || handles.previous as usize == usize::MAX {
            handles.previous = std::ptr::null_mut();
            return None;
        }
        let mut resolved = [0u16; 128];
        if GetTextFaceW(dc, resolved.len() as i32, resolved.as_mut_ptr()) <= 0 {
            return None;
        }
        let end = resolved.iter().position(|&ch| ch == 0)?;
        if !String::from_utf16(&resolved[..end])
            .ok()?
            .eq_ignore_ascii_case(family)
        {
            return None;
        }
        // Preserve the whole original collection so its absolute table offsets
        // remain valid; for a standalone TTF the collection request is absent.
        let mut table = 0x6663_7474; // GDI's little-endian 'ttcf'.
        let mut length = GetFontData(dc, table, 0, std::ptr::null_mut(), 0);
        if length == GDI_ERROR as u32 {
            table = 0;
            length = GetFontData(dc, table, 0, std::ptr::null_mut(), 0);
        }
        if length == 0
            || length == GDI_ERROR as u32
            || length as usize > maximum.min(32 * 1024 * 1024)
        {
            return None;
        }
        let mut bytes = vec![0u8; length as usize];
        if GetFontData(dc, table, 0, bytes.as_mut_ptr().cast(), length) != length {
            return None;
        }
        let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
        if count > 256 {
            return None;
        }
        let index = (0..count).find(|&index| {
            ttf_parser::Face::parse(&bytes, index)
                .ok()
                .is_some_and(|face| {
                    face.names().into_iter().any(|name| {
                        name.to_string()
                            .is_some_and(|name| name.eq_ignore_ascii_case(family))
                    })
                })
        })?;
        Some(SystemFont { bytes, index })
    }
}
