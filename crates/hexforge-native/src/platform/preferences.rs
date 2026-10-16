//! Only the reference's two optional results-pane preferences are persisted.
//! This is OS storage, never eframe persistence or a file beside the executable.
use hexforge_controller::preferences::PanelPreferences;

pub const REGISTRY_PATH: &str = "Software\\HexForge\\UI";
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResultsPreferences {
    pub height: f32,
    pub collapsed: bool,
}
impl Default for ResultsPreferences {
    fn default() -> Self {
        Self {
            height: 220.0,
            collapsed: false,
        }
    }
}
impl ResultsPreferences {
    pub fn normalized(self) -> Self {
        let panel = PanelPreferences::restore(Some(self.height), self.collapsed);
        Self {
            height: panel.preferred_height,
            collapsed: panel.collapsed,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceError {
    Unavailable,
    Windows(u32),
}
impl std::fmt::Display for PreferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => f.write_str("Optional UI preference storage is unavailable."),
            Self::Windows(code) => write!(
                f,
                "Optional UI preference storage is unavailable (Windows {code})."
            ),
        }
    }
}
impl std::error::Error for PreferenceError {}
pub trait PreferenceStore: Send + Sync {
    fn read_results_preferences(&self) -> Result<Option<ResultsPreferences>, PreferenceError>;
    fn write_results_preferences(&self, values: &ResultsPreferences)
        -> Result<(), PreferenceError>;
}
#[derive(Default)]
pub struct WindowsPreferenceStore;
#[cfg(windows)]
pub(super) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
impl PreferenceStore for WindowsPreferenceStore {
    fn read_results_preferences(&self) -> Result<Option<ResultsPreferences>, PreferenceError> {
        read_results_at(REGISTRY_PATH)
    }
    fn write_results_preferences(
        &self,
        values: &ResultsPreferences,
    ) -> Result<(), PreferenceError> {
        write_results_at(REGISTRY_PATH, values)
    }
}

// The same bounded registry adapter serves runtime and development imports.
// Private paths also let integration assertions use isolated keys, without
// temporarily changing any of the user's real pane preferences.
#[cfg(windows)]
fn read_results_at(registry_path: &str) -> Result<Option<ResultsPreferences>, PreferenceError> {
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_UNSUPPORTED_TYPE},
        System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ},
    };
    let path = wide(registry_path);
    let mut collapsed = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            wide("ResultsCollapsed").as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut collapsed as *mut u32).cast(),
            &mut size,
        )
    };
    if ![0, ERROR_FILE_NOT_FOUND, ERROR_UNSUPPORTED_TYPE].contains(&status) {
        return Err(PreferenceError::Windows(status));
    }
    let has_collapsed = status == 0 && size == 4;
    // A decimal string retains fractional CSS heights. Both allocation and
    // registry reads are bounded, even if an external program corrupts it.
    let mut height = [0u16; 64];
    let mut bytes = std::mem::size_of_val(&height) as u32;
    let height_status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            wide("ResultsPaneHeight").as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            height.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if height_status != 0
        && height_status != ERROR_FILE_NOT_FOUND
        && height_status != ERROR_UNSUPPORTED_TYPE
        && height_status != windows_sys::Win32::Foundation::ERROR_MORE_DATA
    {
        return Err(PreferenceError::Windows(height_status));
    }
    let has_height =
        height_status == 0 && bytes <= std::mem::size_of_val(&height) as u32 && bytes % 2 == 0;
    if !has_collapsed && !has_height {
        return Ok(None);
    }
    let end = height.iter().position(|c| *c == 0).unwrap_or(height.len());
    let height = has_height
        .then(|| String::from_utf16_lossy(&height[..end]).parse::<f32>().ok())
        .flatten()
        .unwrap_or(220.0);
    Ok(Some(
        ResultsPreferences {
            height,
            collapsed: has_collapsed && collapsed == 1,
        }
        .normalized(),
    ))
}
#[cfg(windows)]
fn write_results_at(
    registry_path: &str,
    values: &ResultsPreferences,
) -> Result<(), PreferenceError> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    let values = values.normalized();
    let mut handle = std::ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(registry_path).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(PreferenceError::Windows(status));
    }
    let key = Key(handle);
    let collapsed = u32::from(values.collapsed);
    let status = unsafe {
        RegSetValueExW(
            key.0,
            wide("ResultsCollapsed").as_ptr(),
            0,
            REG_DWORD,
            (&collapsed as *const u32).cast(),
            4,
        )
    };
    if status != 0 {
        return Err(PreferenceError::Windows(status));
    }
    let height = wide(&values.height.to_string());
    let status = unsafe {
        RegSetValueExW(
            key.0,
            wide("ResultsPaneHeight").as_ptr(),
            0,
            REG_SZ,
            height.as_ptr().cast(),
            (height.len() * 2) as u32,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(PreferenceError::Windows(status))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    struct TestKey(String);
    impl TestKey {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(format!(
                "Software\\HexForge\\PreferenceTests-{}-{nonce}",
                std::process::id()
            ))
        }
        fn set(&self, name: &str, kind: u32, bytes: &[u8]) {
            let mut key = std::ptr::null_mut();
            assert_eq!(
                unsafe {
                    RegCreateKeyExW(
                        HKEY_CURRENT_USER,
                        wide(&self.0).as_ptr(),
                        0,
                        std::ptr::null(),
                        REG_OPTION_NON_VOLATILE,
                        KEY_SET_VALUE,
                        std::ptr::null(),
                        &mut key,
                        std::ptr::null_mut(),
                    )
                },
                0
            );
            let status = unsafe {
                RegSetValueExW(
                    key,
                    wide(name).as_ptr(),
                    0,
                    kind,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                )
            };
            unsafe {
                RegCloseKey(key);
            }
            assert_eq!(status, 0);
        }
        fn text(&self, name: &str, value: &str) {
            let bytes: Vec<u8> = wide(value).into_iter().flat_map(u16::to_le_bytes).collect();
            self.set(name, REG_SZ, &bytes);
        }
    }
    impl Drop for TestKey {
        fn drop(&mut self) {
            // Delete only this test's exact, unique key, never the real UI key.
            assert!(self.0.starts_with("Software\\HexForge\\PreferenceTests-"));
            assert_ne!(self.0, REGISTRY_PATH);
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.0).as_ptr());
            }
        }
    }

    #[test]
    fn windows_storage_round_trips_fractional_pane_preferences() {
        let key = TestKey::new();
        assert_eq!(read_results_at(&key.0).unwrap(), None);
        let values = ResultsPreferences {
            height: 345.5,
            collapsed: true,
        };
        write_results_at(&key.0, &values).unwrap();
        assert_eq!(read_results_at(&key.0).unwrap(), Some(values));
        write_results_at(&key.0, &ResultsPreferences::default()).unwrap();
        assert_eq!(
            read_results_at(&key.0).unwrap(),
            Some(ResultsPreferences::default())
        );
    }

    #[test]
    fn windows_storage_bounds_corrupt_optional_values() {
        let key = TestKey::new();
        key.set("ResultsCollapsed", REG_DWORD, &1u32.to_le_bytes());
        for invalid in ["NaN".to_owned(), "27".to_owned(), "a".repeat(128)] {
            key.text("ResultsPaneHeight", &invalid);
            assert_eq!(
                read_results_at(&key.0).unwrap(),
                Some(ResultsPreferences {
                    height: 220.0,
                    collapsed: true,
                })
            );
        }
        key.set("ResultsPaneHeight", REG_DWORD, &345u32.to_le_bytes());
        key.text("ResultsCollapsed", "true");
        assert_eq!(read_results_at(&key.0).unwrap(), None);
    }
}
#[cfg(not(windows))]
impl PreferenceStore for WindowsPreferenceStore {
    fn read_results_preferences(&self) -> Result<Option<ResultsPreferences>, PreferenceError> {
        Ok(None)
    }
    fn write_results_preferences(&self, _: &ResultsPreferences) -> Result<(), PreferenceError> {
        Err(PreferenceError::Unavailable)
    }
}
