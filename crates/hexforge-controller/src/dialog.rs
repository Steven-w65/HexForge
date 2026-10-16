//! UI descriptions contain no native window or picker handles.
use crate::shortcuts::WindowKind;
use hexforge_core::error::AppError;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}
impl ControllerError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".into(),
            message: message.into(),
            detail: None,
        }
    }
    pub fn code(&self) -> &str {
        &self.code
    }
}
impl From<AppError> for ControllerError {
    fn from(error: AppError) -> Self {
        Self {
            code: error.code().into(),
            message: error.message,
            detail: error.detail,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerPurpose {
    BinaryOpen,
    BinarySave,
    TemplateLoad,
    TemplateSave,
    Csv,
}
#[derive(Debug, Clone)]
pub struct PickerRequest {
    pub purpose: PickerPurpose,
    pub title: String,
    pub default_path: Option<PathBuf>,
    pub filters: Vec<(String, Vec<String>)>,
    pub owner: WindowKind,
}
impl PickerRequest {
    pub fn binary_open() -> Self {
        Self {
            purpose: PickerPurpose::BinaryOpen,
            title: "Open binary file".into(),
            default_path: None,
            filters: vec![],
            owner: WindowKind::Main,
        }
    }
    pub fn binary_save(name: Option<&str>) -> Self {
        Self {
            purpose: PickerPurpose::BinarySave,
            title: "Save binary as".into(),
            default_path: name.map(|name| PathBuf::from(format!("{name}.copy"))),
            filters: vec![],
            owner: WindowKind::Main,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogDecision {
    Save,
    SaveAs,
    Discard,
    Cancel,
    Confirm,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    BinaryUnsaved,
    Confirmation,
    EditorUnsaved,
}
#[derive(Debug, Clone)]
pub struct DialogRequest {
    pub kind: DialogKind,
    pub title: String,
    pub message: String,
    pub owner: WindowKind,
}
