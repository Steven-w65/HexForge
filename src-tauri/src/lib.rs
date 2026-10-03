pub const APP_NAME: &str = "HexForge";

pub mod commands;

pub mod edit_buffer;
pub mod error;
pub mod export;
pub mod minimap;
pub mod page_cache;
pub mod search;
pub mod session;
pub mod startup;
pub mod template;
pub mod template_file;

pub fn run() {
    use tauri::{webview::PageLoadEvent, Manager};
    let startup = startup::StartupDiagnostics::from_environment();
    let profiling_script = startup.initialization_script();
    let mut context = tauri::generate_context!();
    if let Some(directory) = startup.profile_data_directory() {
        if let Some(window) = context.config_mut().app.windows.first_mut() {
            window.data_directory = Some(directory);
        }
    }
    startup.mark_native("context-created");
    let app = tauri::Builder::default()
        .append_invoke_initialization_script(profiling_script)
        .manage(startup)
        .manage(commands::AppState::default())
        .setup(|app| {
            app.state::<startup::StartupDiagnostics>()
                .mark_native("windows-created");
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if webview.label() == "main" {
                webview
                    .state::<startup::StartupDiagnostics>()
                    .mark_native(match payload.event() {
                        PageLoadEvent::Started => "page-started",
                        PageLoadEvent::Finished => "page-finished",
                    });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::startup_clock,
            commands::frontend_ready,
            commands::open_file,
            commands::close_file,
            commands::get_file_info,
            commands::read_page,
            commands::read_minimap_samples,
            commands::edit_byte,
            commands::undo_edit,
            commands::get_dirty_state,
            commands::get_modified_overview,
            commands::save_as,
            commands::search_bytes,
            commands::apply_template,
            commands::load_template,
            commands::save_template,
            commands::save_template_as,
            commands::unload_template_file,
            commands::export_results_csv,
        ])
        .plugin(tauri_plugin_dialog::init())
        .build(context)
        .expect("error while running HexForge");
    app.state::<startup::StartupDiagnostics>()
        .mark_native("app-built");
    app.run(|_, _| {});
}

#[cfg(test)]
mod tests {
    #[test]
    fn application_name_is_stable() {
        assert_eq!(super::APP_NAME, "HexForge");
    }
}
