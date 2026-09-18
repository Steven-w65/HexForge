pub const APP_NAME: &str = "HexForge";

pub mod edit_buffer;
pub mod error;
pub mod export;
pub mod page_cache;
pub mod search;
pub mod session;
pub mod template;

pub fn run() {
    tauri::Builder::default()
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
