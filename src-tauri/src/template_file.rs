//! Persistence for the sole, unversioned template JSON format.
//! A failed load or save leaves both the associated path and baseline intact.

use crate::error::{AppError, ErrorCode};
use crate::session::MAX_READ_RANGE;
use crate::template::{validate_template, TemplateDefinition};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

#[derive(Default)]
pub struct TemplateFileSession {
    path: Option<PathBuf>,
    baseline: Vec<u8>,
}

impl TemplateFileSession {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn clear(&mut self) {
        self.path = None;
        self.baseline.clear();
    }

    pub fn load(&mut self, path: &Path) -> Result<TemplateDefinition, AppError> {
        let metadata = fs::metadata(path).map_err(|error| AppError::from_io(error, Some(path)))?;
        if metadata.len() > MAX_READ_RANGE {
            return Err(invalid_template(
                "Template files must not exceed 1048576 bytes.",
            ));
        }
        let bytes = fs::read(path).map_err(|error| AppError::from_io(error, Some(path)))?;
        if bytes.len() as u64 > MAX_READ_RANGE {
            return Err(invalid_template(
                "Template files must not exceed 1048576 bytes.",
            ));
        }
        // deny_unknown_fields rejects any top-level format key before changing state.
        let definition = serde_json::from_slice::<TemplateDefinition>(&bytes)
            .map_err(|issue| invalid_template(&format!("The template JSON is invalid: {issue}")))?;
        validate_template(&definition)?;
        let canonical =
            fs::canonicalize(path).map_err(|error| AppError::from_io(error, Some(path)))?;
        self.path = Some(canonical);
        self.baseline = bytes;
        Ok(definition)
    }

    pub fn save(
        &mut self,
        template: &TemplateDefinition,
        overwrite_external: bool,
        binary_source: Option<&Path>,
    ) -> Result<(), AppError> {
        let path = self.path.as_ref().ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidPath,
                "This template has no saved file path.",
                None,
            )
        })?;
        reject_binary_source(path, binary_source)?;
        let output = serialize_template(template)?;
        let exists = match fs::read(path) {
            Ok(current) => {
                if current != self.baseline && !overwrite_external {
                    return Err(AppError::new(
                        ErrorCode::ExternalModification,
                        "The template file has been modified by another program.",
                        Some(path.to_string_lossy().into_owned()),
                    ));
                }
                true
            }
            Err(issue) if issue.kind() == std::io::ErrorKind::NotFound => false,
            Err(issue) => return Err(AppError::from_io(issue, Some(path))),
        };
        persist_template(path, &output, exists)?;
        self.baseline = output;
        Ok(())
    }

    pub fn save_as(
        &mut self,
        path: &Path,
        template: &TemplateDefinition,
        overwrite: bool,
        binary_source: Option<&Path>,
    ) -> Result<(), AppError> {
        let destination = normalized_template_path(path)?;
        reject_binary_source(&destination, binary_source)?;
        let output = serialize_template(template)?;
        persist_template(&destination, &output, overwrite)?;
        self.path = Some(
            fs::canonicalize(&destination)
                .map_err(|issue| AppError::from_io(issue, Some(&destination)))?,
        );
        self.baseline = output;
        Ok(())
    }
}

fn invalid_template(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidTemplate, message, None)
}

fn serialize_template(template: &TemplateDefinition) -> Result<Vec<u8>, AppError> {
    validate_template(template)?;
    let mut bytes = serde_json::to_vec_pretty(template).map_err(|_| {
        AppError::new(
            ErrorCode::IoError,
            "The template file could not be written.",
            None,
        )
    })?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_READ_RANGE {
        return Err(invalid_template(
            "Template files must not exceed 1048576 bytes.",
        ));
    }
    Ok(bytes)
}

fn normalized_template_path(path: &Path) -> Result<PathBuf, AppError> {
    let name = path.file_name().ok_or_else(|| {
        AppError::new(ErrorCode::InvalidPath, "Choose a template file name.", None)
    })?;
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent =
        fs::canonicalize(parent).map_err(|error| AppError::from_io(error, Some(parent)))?;
    Ok(parent.join(name))
}

fn reject_binary_source(destination: &Path, binary_source: Option<&Path>) -> Result<(), AppError> {
    let Some(source) = binary_source else {
        return Ok(());
    };
    let source =
        fs::canonicalize(source).map_err(|error| AppError::from_io(error, Some(source)))?;
    if fs::canonicalize(destination).is_ok_and(|path| path == source) {
        return Err(AppError::new(
            ErrorCode::DestinationIsSource,
            "A template cannot overwrite the open binary file.",
            Some(destination.to_string_lossy().into_owned()),
        ));
    }
    Ok(())
}

fn persist_template(path: &Path, bytes: &[u8], overwrite: bool) -> Result<(), AppError> {
    let parent = path
        .parent()
        .expect("normalized template path has a parent");
    let mut staged =
        NamedTempFile::new_in(parent).map_err(|error| AppError::from_io(error, Some(path)))?;
    staged
        .write_all(bytes)
        .map_err(|error| AppError::from_io(error, Some(path)))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| AppError::from_io(error, Some(path)))?;
    if overwrite {
        staged
            .persist(path)
            .map_err(|error| AppError::from_io(error.error, Some(path)))?;
    } else {
        staged
            .persist_noclobber(path)
            .map_err(|error| AppError::from_io(error.error, Some(path)))?;
    }
    Ok(())
}
