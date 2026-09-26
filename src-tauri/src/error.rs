use serde::Serialize;
use std::fmt;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    FileNotFound,
    PermissionDenied,
    DestinationExists,
    ExternalModification,
    DiskFull,
    InvalidPath,
    InvalidOffset,
    InvalidLength,
    NoActiveFile,
    UnsavedChanges,
    IoError,
    OperationFailed,
    InvalidSearch,
    InvalidTemplate,
    TemplateOutOfBounds,
    DestinationIsSource,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileNotFound => "file_not_found",
            Self::PermissionDenied => "permission_denied",
            Self::DestinationExists => "destination_exists",
            Self::ExternalModification => "external_modification",
            Self::DiskFull => "disk_full",
            Self::InvalidPath => "invalid_path",
            Self::InvalidOffset => "invalid_offset",
            Self::InvalidLength => "invalid_length",
            Self::NoActiveFile => "no_active_file",
            Self::UnsavedChanges => "unsaved_changes",
            Self::IoError => "io_error",
            Self::OperationFailed => "operation_failed",
            Self::InvalidSearch => "invalid_search",
            Self::InvalidTemplate => "invalid_template",
            Self::TemplateOutOfBounds => "template_out_of_bounds",
            Self::DestinationIsSource => "destination_is_source",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail,
        }
    }

    pub fn code(&self) -> &'static str {
        self.code.as_str()
    }

    pub fn from_io(error: io::Error, path: Option<&Path>) -> Self {
        let raw_code = error.raw_os_error();
        let (code, message) = match error.kind() {
            io::ErrorKind::NotFound => (ErrorCode::FileNotFound, "The file could not be found."),
            io::ErrorKind::PermissionDenied => (
                ErrorCode::PermissionDenied,
                "Permission was denied for this file.",
            ),
            io::ErrorKind::AlreadyExists => (
                ErrorCode::DestinationExists,
                "The destination already exists.",
            ),
            io::ErrorKind::WriteZero => {
                (ErrorCode::DiskFull, "The disk does not have enough space.")
            }
            _ if matches!(raw_code, Some(112 | 28)) => {
                (ErrorCode::DiskFull, "The disk does not have enough space.")
            }
            _ => (
                ErrorCode::IoError,
                "The file operation could not be completed.",
            ),
        };

        Self::new(
            code,
            message,
            path.map(|value| value.to_string_lossy().into_owned()),
        )
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        Self::from_io(error, None)
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, ErrorCode};
    use std::io;

    #[test]
    fn not_found_io_errors_have_a_stable_friendly_code() {
        let error = AppError::from_io(io::Error::from(io::ErrorKind::NotFound), None);
        assert_eq!(error.code, ErrorCode::FileNotFound);
        assert_eq!(error.code(), "file_not_found");
        assert_eq!(error.message, "The file could not be found.");
    }
}
