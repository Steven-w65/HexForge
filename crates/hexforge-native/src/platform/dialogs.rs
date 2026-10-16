use super::windows::NativeWindowHandle;
use hexforge_controller::dialog::{PickerPurpose, PickerRequest};
use hexforge_core::error::AppError;
pub struct PlatformDialogs;
impl PlatformDialogs {
    pub fn open(
        request: PickerRequest,
        owner: Option<NativeWindowHandle>,
    ) -> Result<Option<std::path::PathBuf>, AppError> {
        let mut dialog = rfd::FileDialog::new().set_title(request.title);
        if let Some(owner) = owner {
            dialog = dialog.set_parent(&owner);
        }
        for (label, extensions) in &request.filters {
            let extensions: Vec<_> = extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(label, &extensions);
        }
        if let Some(path) = request.default_path {
            if let Some(name) = path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                dialog = dialog.set_directory(parent);
            }
        }
        Ok(match request.purpose {
            PickerPurpose::BinaryOpen | PickerPurpose::TemplateLoad => dialog.pick_file(),
            _ => dialog.save_file(),
        })
    }
}
