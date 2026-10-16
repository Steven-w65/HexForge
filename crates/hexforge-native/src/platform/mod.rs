pub mod dialogs;
pub mod fonts;
pub mod input;
pub mod preferences;
pub mod theme;
pub mod windows;
pub fn native_failure_message(error: &str) -> String {
    format!("HexForge could not start.\n\n{error}\n\nNo files or drafts were changed.")
}
pub fn show_startup_error(error: &str) {
    rfd::MessageDialog::new()
        .set_title("HexForge — Startup error")
        .set_description(native_failure_message(error))
        .set_level(rfd::MessageLevel::Error)
        .show();
}
