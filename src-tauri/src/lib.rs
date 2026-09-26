pub const APP_NAME: &str = "HexForge";

pub mod commands;

pub mod edit_buffer;
pub mod error;
pub mod export;
pub mod page_cache;
pub mod search;
pub mod session;
pub mod template;

pub fn run() {
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::close_file,
            commands::get_file_info,
            commands::read_page,
            commands::edit_byte,
            commands::undo_edit,
            commands::get_dirty_state,
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
        .run(tauri::generate_context!())
        .expect("error while running HexForge");
}

#[cfg(test)]
mod tests {
    #[test]
    fn application_name_is_stable() {
        assert_eq!(super::APP_NAME, "HexForge");
    }
}
