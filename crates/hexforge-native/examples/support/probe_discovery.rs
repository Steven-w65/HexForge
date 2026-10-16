//! Development observer only; no launch, focus, input or close APIs.
use hexforge_native::startup::presentation::{MeasurementError, ProcessTimeline, Tick};
use std::path::Path;

pub fn is_new_identity(
    expected: &Path,
    actual: &Path,
    armed: &Tick,
    process: &ProcessTimeline,
) -> Result<bool, MeasurementError> {
    if armed.domain != process.started_at().domain {
        return Err(MeasurementError::UncorrelatedClock);
    }
    // Compare complete canonical paths, never only an executable basename.
    // Windows' ASCII case/separator variants are equivalent; preserve all
    // non-ASCII code units rather than introducing lossy path collisions.
    #[cfg(windows)]
    let same_path = {
        use std::os::windows::ffi::OsStrExt;
        let normalize = |path: &Path| {
            let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
            if units.starts_with(&[92, 92, 63, 92]) {
                units.drain(..4);
            }
            for unit in &mut units {
                if *unit == 47 {
                    *unit = 92;
                }
                if (65..=90).contains(unit) {
                    *unit += 32;
                }
            }
            units
        };
        normalize(expected) == normalize(actual)
    };
    #[cfg(not(windows))]
    let same_path = expected == actual;
    // The wall-clock/QPC correlation has an interval, not an exact origin.
    // Select only a definitely new process so an old root near the arming
    // boundary cannot be mistaken for an implausibly fast fresh launch.
    let earliest_start =
        i128::from(process.started_at().ticks) - i128::from(process.origin_uncertainty_ticks());
    Ok(same_path && earliest_start >= i128::from(armed.ticks))
}

#[cfg(windows)]
pub fn process_image(pid: u32) -> std::io::Result<std::path::PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    if pid == 0 {
        return Err(std::io::Error::other("Missing process identity"));
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let mut units = vec![0; 32_768];
    let mut length = units.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(handle, 0, units.as_mut_ptr(), &mut length) };
    let error = std::io::Error::last_os_error();
    unsafe { CloseHandle(handle) };
    if success == 0 || length == 0 || length as usize > units.len() {
        return Err(error);
    }
    Ok(std::path::PathBuf::from(OsString::from_wide(
        &units[..length as usize],
    )))
}

/// Observe a newly created root after the user launches it through Computer
/// Use. Unlike spawning from a background observer, this preserves the same
/// normal Windows launch/activation path for native and retained executables.
#[cfg(windows)]
pub fn find_new_root(
    expected: &Path,
    armed: &Tick,
    clock: &hexforge_native::startup::clock::NativeClock,
) -> std::io::Result<Option<(u32, u64, ProcessTimeline)>> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, GWL_STYLE,
            GW_OWNER, WS_CAPTION,
        },
    };
    struct Find<'a> {
        expected: &'a Path,
        armed: &'a Tick,
        clock: &'a hexforge_native::startup::clock::NativeClock,
        matches: Vec<(u32, u64, ProcessTimeline)>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
        let find = &mut *(data as *mut Find<'_>);
        if !GetWindow(hwnd, GW_OWNER).is_null()
            || (GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CAPTION) != WS_CAPTION
        {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let Ok(image) = process_image(pid) else {
            return 1;
        };
        if image.file_name() != find.expected.file_name() {
            return 1;
        }
        let Ok(timeline) = find.clock.process_timeline(pid) else {
            return 1;
        };
        if is_new_identity(find.expected, &image, find.armed, &timeline) == Ok(true) {
            find.matches.push((pid, hwnd as u64, timeline));
        }
        1
    }
    let mut find = Find {
        expected,
        armed,
        clock,
        matches: Vec::new(),
    };
    if unsafe { EnumWindows(Some(collect), (&mut find as *mut Find<'_>) as isize) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    match find.matches.len() {
        0 => Ok(None),
        1 => Ok(find.matches.pop()),
        _ => Err(std::io::Error::other(
            "Multiple newly launched root windows; launch measurement is ambiguous",
        )),
    }
}
