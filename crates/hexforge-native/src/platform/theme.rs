//! OS preference observation is separate from the user's session-only choice.
//! Unchanged observations never erase a manual toggle; only a real transition
//! publishes a controller event. The root native window owns this observation.
use hexforge_controller::controller::Theme;

pub struct SystemThemeSource {
    last: Theme,
}
impl SystemThemeSource {
    pub fn from_initial(theme: Theme) -> Self {
        Self { last: theme }
    }
    pub fn native() -> Self {
        Self::from_preference(read_system_theme())
    }
    /// An unavailable OS preference uses the source's dark fallback without
    /// turning an optional theme read into an application startup error.
    pub fn from_preference(theme: Option<Theme>) -> Self {
        Self::from_initial(theme.unwrap_or(Theme::Dark))
    }
    pub fn current(&self) -> Theme {
        self.last
    }
    pub fn observe(&mut self, theme: Theme) -> Option<Theme> {
        if theme == self.last {
            None
        } else {
            self.last = theme;
            Some(theme)
        }
    }
    /// Available for platform diagnostics. Normal presentation consumes winit's
    /// root ThemeChanged observation rather than scheduling a polling timer.
    pub fn poll_change(&mut self) -> Option<Theme> {
        self.observe(read_system_theme()?)
    }
}
#[cfg(windows)]
fn read_system_theme() -> Option<Theme> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let path = super::preferences::wide(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
    );
    let name = super::preferences::wide("AppsUseLightTheme");
    let mut value = 0u32;
    let mut size = std::mem::size_of_val(&value) as u32;
    // The byte count and typed destination bound the read to one DWORD. No OS
    // setting is written and an unavailable preference retains the dark fallback.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0 && size == 4).then_some(if value == 0 {
        Theme::Dark
    } else {
        Theme::Light
    })
}
#[cfg(not(windows))]
fn read_system_theme() -> Option<Theme> {
    None
}
