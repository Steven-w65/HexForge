//! Passive Windows startup observation. This reads the *desktop* client pixels,
//! never PrintWindow, an application FBO or a readiness flag. It does not send
//! input, focus/show/close windows, change OS settings or create runtime files.
//! It is unused on ordinary launches. An occluded/offscreen/changed window is a
//! measurement failure, not a slow launch to silently omit from a distribution.
use super::observer::{decode_top_down_bgra, MAX_CAPTURE_PIXELS};
use image::RgbaImage;
use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    Graphics::Gdi::{
        BitBlt, ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
        GdiFlush, GetDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
    },
    UI::{
        HiDpi::{
            GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        WindowsAndMessaging::{
            EnumWindows, GetAncestor, GetClientRect, GetForegroundWindow, GetSystemMetrics,
            GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic, IsWindow,
            IsWindowVisible, WindowFromPoint, GA_ROOT, GWL_STYLE, GW_OWNER, SM_CXVIRTUALSCREEN,
            SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WS_CAPTION,
        },
    },
};

fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}
fn api() -> io::Error {
    io::Error::last_os_error()
}
fn foreground_gate(
    foreground: bool,
    minimized: bool,
    previously_captured: bool,
) -> io::Result<bool> {
    if minimized || (!foreground && previously_captured) {
        return Err(invalid(
            "The measured main window is minimized, unfocused or occluded.",
        ));
    }
    // ShowWindow and activation are separate OS events on initial launch. Wait
    // for actual activation; do not capture the other foreground application's
    // pixels or interpret this pending state as a negative pixel observation.
    Ok(foreground)
}
fn belongs_to(hwnd: HWND, process_id: u32) -> bool {
    let mut actual = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut actual);
    }
    !hwnd.is_null() && process_id != 0 && actual == process_id && unsafe { IsWindow(hwnd) } != 0
}

/// Select exactly one unowned captioned root for the launched process. Titles
/// are not inspected and never constitute interface/presentation evidence.
pub fn find_main_window(process_id: u32) -> io::Result<Option<u64>> {
    if process_id == 0 {
        return Err(invalid("Missing launched process identity."));
    }
    struct Find {
        pid: u32,
        windows: Vec<u64>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
        let found = &mut *(data as *mut Find);
        if belongs_to(hwnd, found.pid)
            && GetWindow(hwnd, GW_OWNER).is_null()
            && (GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CAPTION) == WS_CAPTION
        {
            found.windows.push(hwnd as u64);
        }
        1
    }
    let mut found = Find {
        pid: process_id,
        windows: Vec::new(),
    };
    if unsafe { EnumWindows(Some(collect), &mut found as *mut Find as isize) } == 0 {
        return Err(api());
    }
    match found.windows.as_slice() {
        [] => Ok(None),
        [window] => Ok(Some(*window)),
        _ => Err(invalid(
            "The launched process has multiple candidate main windows.",
        )),
    }
}

struct DpiScope(DPI_AWARENESS_CONTEXT);
impl DpiScope {
    fn enter() -> io::Result<Self> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.is_null() {
            Err(api())
        } else {
            Ok(Self(previous))
        }
    }
}
impl Drop for DpiScope {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct Surface {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut std::ffi::c_void,
    width: u32,
    height: u32,
}
impl Surface {
    fn new(width: u32, height: u32) -> io::Result<Self> {
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > MAX_CAPTURE_PIXELS
            || width > i32::MAX as u32
            || height > i32::MAX as u32
        {
            return Err(invalid("Unbounded desktop capture dimensions."));
        }
        // Null fields make every partially initialized failure recoverable.
        let mut surface = Self {
            screen: ptr::null_mut(),
            memory: ptr::null_mut(),
            bitmap: ptr::null_mut(),
            previous: ptr::null_mut(),
            bits: ptr::null_mut(),
            width,
            height,
        };
        surface.screen = unsafe { GetDC(ptr::null_mut()) };
        if surface.screen.is_null() {
            return Err(api());
        }
        surface.memory = unsafe { CreateCompatibleDC(surface.screen) };
        if surface.memory.is_null() {
            return Err(api());
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        surface.bitmap = unsafe {
            CreateDIBSection(
                surface.screen,
                &info,
                DIB_RGB_COLORS,
                &mut surface.bits,
                ptr::null_mut(),
                0,
            )
        };
        if surface.bitmap.is_null() || surface.bits.is_null() {
            return Err(api());
        }
        surface.previous = unsafe { SelectObject(surface.memory, surface.bitmap) };
        if surface.previous.is_null() || surface.previous as isize == -1 {
            surface.previous = ptr::null_mut();
            return Err(api());
        }
        Ok(surface)
    }
    fn read(&mut self, x: i32, y: i32) -> io::Result<RgbaImage> {
        if unsafe {
            BitBlt(
                self.memory,
                0,
                0,
                self.width as i32,
                self.height as i32,
                self.screen,
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            )
        } == 0
            || unsafe { GdiFlush() } == 0
        {
            return Err(api());
        }
        // GdiFlush synchronizes the DIB readback, not another application's
        // compositor queue. The pixels themselves are independent evidence.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                self.bits.cast::<u8>(),
                self.width as usize * self.height as usize * 4,
            )
        };
        decode_top_down_bgra(self.width, self.height, bytes).map_err(|e| invalid(&e.to_string()))
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.memory.is_null() {
                DeleteDC(self.memory);
            }
            if !self.screen.is_null() {
                ReleaseDC(ptr::null_mut(), self.screen);
            }
        }
    }
}

pub struct DesktopCapture {
    pid: u32,
    hwnd: HWND,
    surface: Option<Surface>,
    dpi: u32,
}
impl DesktopCapture {
    pub fn new(process_id: u32, window_id: u64) -> io::Result<Self> {
        let hwnd = usize::try_from(window_id)
            .map_err(|_| invalid("Invalid main window identity."))? as HWND;
        if !belongs_to(hwnd, process_id) {
            return Err(invalid(
                "The main window does not belong to the launched process.",
            ));
        }
        Ok(Self {
            pid: process_id,
            hwnd,
            surface: None,
            dpi: 0,
        })
    }
    pub fn dpi(&self) -> u32 {
        self.dpi
    }
    /// Passive failure context. It is not a readiness signal and never relaxes
    /// capture validation. Physical coordinates use the same scoped DPI policy.
    pub fn diagnostic_status(&self) -> io::Result<String> {
        let _dpi = DpiScope::enter()?;
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        if unsafe { GetClientRect(self.hwnd, &mut rect) } == 0
            || unsafe { ClientToScreen(self.hwnd, &mut origin) } == 0
        {
            return Err(api());
        }
        let mut points = Vec::new();
        for iy in 0..8 {
            for ix in 0..8 {
                let point = POINT {
                    x: origin.x + (rect.right - rect.left) * (2 * ix + 1) / 16,
                    y: origin.y + (rect.bottom - rect.top) * (2 * iy + 1) / 16,
                };
                let root = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
                if root != self.hwnd {
                    points.push(format!("({}, {}):{:#x}", point.x, point.y, root as usize));
                }
            }
        }
        Ok(format!("window={:#x}, foreground={:#x}, visible={}, minimized={}, origin=({}, {}), client={}x{}, dpi={}, mismatched roots={points:?}",
            self.hwnd as usize, unsafe { GetForegroundWindow() } as usize,
            unsafe { IsWindowVisible(self.hwnd) }, unsafe { IsIconic(self.hwnd) },
            origin.x, origin.y, rect.right-rect.left, rect.bottom-rect.top, unsafe { GetDpiForWindow(self.hwnd) }))
    }
    fn bounds(&self) -> io::Result<Option<(POINT, u32, u32, u32)>> {
        if !belongs_to(self.hwnd, self.pid) {
            return Err(invalid("The launched main window no longer exists."));
        }
        if unsafe { IsWindowVisible(self.hwnd) } == 0 {
            return Ok(None);
        }
        if !foreground_gate(
            unsafe { GetForegroundWindow() } == self.hwnd,
            unsafe { IsIconic(self.hwnd) } != 0,
            self.surface.is_some(),
        )? {
            return Ok(None);
        }
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        if unsafe { GetClientRect(self.hwnd, &mut rect) } == 0
            || unsafe { ClientToScreen(self.hwnd, &mut origin) } == 0
        {
            return Err(api());
        }
        let width =
            u32::try_from(rect.right - rect.left).map_err(|_| invalid("Invalid client width."))?;
        let height =
            u32::try_from(rect.bottom - rect.top).map_err(|_| invalid("Invalid client height."))?;
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_CAPTURE_PIXELS {
            return Err(invalid("Unbounded client pixels."));
        }
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 {
            return Err(api());
        }
        let (left, top, right, bottom) = unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            (
                x,
                y,
                i64::from(x) + i64::from(GetSystemMetrics(SM_CXVIRTUALSCREEN)),
                i64::from(y) + i64::from(GetSystemMetrics(SM_CYVIRTUALSCREEN)),
            )
        };
        if origin.x < left
            || origin.y < top
            || i64::from(origin.x) + i64::from(width) > right
            || i64::from(origin.y) + i64::from(height) > bottom
        {
            return Err(invalid("The measured main interface is partly offscreen."));
        }
        // Read-only z-order validation, including WebView child HWNDs. An
        // unrelated topmost overlay invalidates the run, not its paint time.
        for iy in 0..8 {
            for ix in 0..8 {
                let point = POINT {
                    x: origin.x + ((u64::from(width) * (2 * ix + 1)) / 16) as i32,
                    y: origin.y + ((u64::from(height) * (2 * iy + 1)) / 16) as i32,
                };
                let top = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
                if top != self.hwnd {
                    // DWM's initial reveal / z-order transition may trail
                    // activation. Await the first unobscured pixel read without
                    // assigning it negative-frame evidence. Once sampling has
                    // begun, any occlusion invalidates the measurement.
                    if self.surface.is_none() {
                        return Ok(None);
                    }
                    return Err(invalid(
                        &format!("The measured interface is occluded at ({}, {}) by root {:#x} instead of {:#x}.",
                            point.x, point.y, top as usize, self.hwnd as usize),
                    ));
                }
            }
        }
        Ok(Some((origin, width, height, dpi)))
    }
    /// Capture only this client's desktop rectangle. A hidden window has no
    /// image; it is never rendered offscreen and passed off as presented.
    pub fn capture(&mut self) -> io::Result<Option<RgbaImage>> {
        let _dpi = DpiScope::enter()?;
        let Some((origin, width, height, dpi)) = self.bounds()? else {
            return Ok(None);
        };
        if let Some(surface) = &self.surface {
            if surface.width != width || surface.height != height || self.dpi != dpi {
                return Err(invalid("The measurement window changed size or DPI."));
            }
        } else {
            self.surface = Some(Surface::new(width, height)?);
            self.dpi = dpi;
        }
        let image = self
            .surface
            .as_mut()
            .expect("allocated surface")
            .read(origin.x, origin.y)?;
        let after = self
            .bounds()?
            .ok_or_else(|| invalid("The measured interface became hidden during capture."))?;
        if after.0.x != origin.x
            || after.0.y != origin.y
            || (after.1, after.2, after.3) != (width, height, dpi)
        {
            return Err(invalid("The measured window moved during capture."));
        }
        Ok(Some(image))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn first_presentation_waits_for_activation_but_later_focus_loss_invalidates_the_run() {
        // Windows sends visibility and activation separately. Neither a pending
        // activation nor a minimized window is presented-pixel evidence.
        assert!(!super::foreground_gate(false, false, false).unwrap());
        assert!(super::foreground_gate(true, false, false).unwrap());
        assert!(super::foreground_gate(true, false, true).unwrap());
        assert!(super::foreground_gate(false, false, true).is_err());
        assert!(super::foreground_gate(true, true, false).is_err());
        assert!(super::foreground_gate(false, true, true).is_err());
    }
}
