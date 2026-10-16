use eframe::egui;
use hexforge_controller::template::coordinator::EditorSessionId;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowChange {
    Open,
    Focus,
}
#[derive(Default)]
pub struct WindowRegistry {
    editor: Option<EditorSessionId>,
    editor_closing: bool,
    main_closing: bool,
}
impl WindowRegistry {
    pub fn editor(&self) -> Option<EditorSessionId> {
        self.editor
    }
    pub fn editor_closing(&self) -> bool {
        self.editor_closing
    }
    pub fn main_closing(&self) -> bool {
        self.main_closing
    }
    pub fn open_or_focus(&mut self, session: EditorSessionId) -> WindowChange {
        if self.editor == Some(session) {
            WindowChange::Focus
        } else {
            self.editor = Some(session);
            self.editor_closing = false;
            WindowChange::Open
        }
    }
    pub fn prepare_editor_close(&mut self, session: EditorSessionId) -> bool {
        if self.editor == Some(session) {
            self.editor_closing = true;
            true
        } else {
            false
        }
    }
    pub fn observe_editor_closed(&mut self, session: EditorSessionId) -> bool {
        if self.editor == Some(session) {
            self.editor = None;
            self.editor_closing = false;
            true
        } else {
            false
        }
    }
    pub fn prepare_main_close(&mut self) {
        self.main_closing = true;
    }
}
pub fn editor_viewport(session: EditorSessionId) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("HexForge Template Editor", session.0))
}
pub fn focus_editor(ctx: &egui::Context, session: EditorSessionId) {
    // The reference restores hidden/minimized children before focusing them.
    // Focus alone does not restore a minimized OS window. Keep every command
    // on the same session-owned viewport and in that original order.
    let id = editor_viewport(session);
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
}
/// Eframe logs and returns on immediate-window initialization failure, but
/// egui then panics because no child callback ran. Normalize only that native
/// creation boundary into our existing retryable error workflow. A completed
/// callback must also have a successful real surface swap. A panic once editor
/// presentation has begun is not a creation error and must propagate.
pub fn show_editor(
    ctx: &egui::Context,
    session: EditorSessionId,
    mut paint: impl FnMut(&mut egui::Ui),
) -> bool {
    let id = editor_viewport(session);
    // A prior successful frame or another editor session cannot satisfy this
    // attempt. Only the real renderer's completed swap may reveal the child.
    let _ = eframe::viewport_presentation::take(ctx, id);
    let mut entered = false;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.show_viewport_immediate(editor_viewport(session), editor_builder(), |ui, _| {
            entered = true;
            paint(ui);
        });
    }));
    match result {
        Ok(()) => {
            let presented = eframe::viewport_presentation::take(ctx, id);
            entered && (ctx.embed_viewports() || matches!(presented, Some(Ok(()))))
        }
        Err(error) if entered => std::panic::resume_unwind(error),
        Err(_) => false,
    }
}
pub fn main_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("HexForge")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([960.0, 640.0])
        .with_icon(crate::assets::icon())
}
pub fn editor_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        // AccessKit must register before the OS first shows this window.
        // OpenOrFocusEditor reveals it only after show_editor painted/swapped
        // the real themed controls, also preventing an unpainted child flash.
        .with_visible(false)
        .with_title("HexForge — Template Editor")
        .with_inner_size([660.0, 760.0])
        .with_min_inner_size([480.0, 440.0])
        .with_icon(crate::assets::icon())
}
/// Shared by the portable application and real framebuffer evidence runner.
/// The source theme uses exact flat sRGB colors; gradient-band dithering would
/// introduce visible noise, even into these otherwise solid-color panels.
pub fn native_options(viewport: egui::ViewportBuilder) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        dithering: false,
        persist_window: false,
        ..Default::default()
    }
}
#[derive(Debug, Clone, Copy)]
pub struct NativeWindowHandle(pub usize);
impl raw_window_handle::HasWindowHandle for NativeWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let hwnd = std::num::NonZeroIsize::new(self.0 as isize)
            .ok_or(raw_window_handle::HandleError::Unavailable)?;
        let handle = raw_window_handle::Win32WindowHandle::new(hwnd);
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Win32(
                handle,
            ))
        })
    }
}
impl raw_window_handle::HasDisplayHandle for NativeWindowHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(raw_window_handle::DisplayHandle::windows())
    }
}
#[cfg(windows)]
pub fn find_owned_window(title: &str) -> Option<NativeWindowHandle> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId},
    };
    struct Find<'a> {
        title: &'a str,
        result: Option<usize>,
    }
    unsafe extern "system" fn callback(hwnd: HWND, data: LPARAM) -> i32 {
        let find = &mut *(data as *mut Find<'_>);
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, &mut process);
        if process != std::process::id() {
            return 1;
        }
        let mut text = [0u16; 256];
        let n = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
        if n > 0 && String::from_utf16_lossy(&text[..n as usize]) == find.title {
            find.result = Some(hwnd as usize);
            return 0;
        }
        1
    }
    let mut find = Find {
        title,
        result: None,
    };
    unsafe {
        EnumWindows(Some(callback), &mut find as *mut Find<'_> as isize);
    }
    find.result.map(NativeWindowHandle)
}
#[cfg(not(windows))]
pub fn find_owned_window(_title: &str) -> Option<NativeWindowHandle> {
    None
}
#[cfg(windows)]
pub fn is_window(handle: NativeWindowHandle) -> bool {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(handle.0 as _) != 0 }
}
#[cfg(not(windows))]
pub fn is_window(_handle: NativeWindowHandle) -> bool {
    false
}
