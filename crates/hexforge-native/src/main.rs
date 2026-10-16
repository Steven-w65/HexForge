#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use hexforge_native::{app::NativeApp, platform, platform::windows};
fn main() {
    let mut startup =
        match hexforge_native::startup::diagnostics::StartupDiagnostics::from_environment() {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                // Optional measurement failure must not stop ordinary product work.
                // A probe missing its report must fail, never fall back to a title.
                eprintln!("Startup diagnostics unavailable: {error}");
                Default::default()
            }
        };
    // Read the real OS preference before the renderer/window's first scene.
    // Manual menu changes remain session-only; native ThemeChanged events resume
    // automatic matching without an extra option or polling timer.
    let initial = hexforge_controller::controller::InitialState {
        theme: platform::theme::SystemThemeSource::native().current(),
        ..Default::default()
    };
    startup.mark(hexforge_native::startup::trace::StartupPhase::ThemeReady);
    let options = windows::native_options(windows::main_builder().with_visible(false));
    let result = eframe::run_native(
        "HexForge",
        options,
        Box::new(move |cc| {
            #[cfg(windows)]
            {
                use raw_window_handle::HasWindowHandle;
                if let Ok(handle) = cc.window_handle() {
                    if let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() {
                        startup.bind_main_window(handle.hwnd.get() as u64);
                    }
                }
            }
            startup.mark(hexforge_native::startup::trace::StartupPhase::RendererReady);
            let mut app = NativeApp::with_startup_diagnostics(
                initial,
                std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
                startup,
            );
            app.configure_preferences(std::sync::Arc::new(
                platform::preferences::WindowsPreferenceStore,
            ));
            app.attach(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
    if let Err(error) = result {
        platform::show_startup_error(&error.to_string());
    }
}
