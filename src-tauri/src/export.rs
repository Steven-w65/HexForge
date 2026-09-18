use crate::edit_buffer::EditBuffer;
use crate::error::{AppError, ErrorCode};
use crate::session::{FileSession, MAX_READ_RANGE};
use crate::template::{Endian, FieldType, ParsedField};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub const MAX_EXPORT_CHUNK_SIZE: usize = MAX_READ_RANGE as usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSummary {
    pub bytes_written: u64,
    pub destination: PathBuf,
}

pub fn save_session_as(
    session: &mut FileSession,
    destination: &Path,
    chunk_size: usize,
) -> Result<SaveSummary, AppError> {
    save_session_as_with_progress(session, destination, chunk_size, &mut |_| {})
}

pub fn save_session_as_with_progress(
    session: &mut FileSession,
    destination: &Path,
    chunk_size: usize,
    progress: &mut dyn FnMut(u64),
) -> Result<SaveSummary, AppError> {
    save_session_as_with_stage_progress(
        session,
        destination,
        chunk_size,
        create_staged_file,
        progress,
    )
}

#[cfg(test)]
fn save_session_as_with_stage<S, F>(
    session: &mut FileSession,
    destination: &Path,
    chunk_size: usize,
    create_stage: F,
) -> Result<SaveSummary, AppError>
where
    S: OwnedStagedOutput,
    F: FnOnce(&Path) -> Result<S, AppError>,
{
    save_session_as_with_stage_progress(session, destination, chunk_size, create_stage, &mut |_| {})
}

fn save_session_as_with_stage_progress<S, F>(
    session: &mut FileSession,
    destination: &Path,
    chunk_size: usize,
    create_stage: F,
    progress: &mut dyn FnMut(u64),
) -> Result<SaveSummary, AppError>
where
    S: OwnedStagedOutput,
    F: FnOnce(&Path) -> Result<S, AppError>,
{
    let chunk_size = validated_chunk_size(chunk_size)?;
    let source = std::fs::canonicalize(session.source_path())
        .map_err(|error| AppError::from_io(error, Some(session.source_path())))?;
    let destination = normalized_destination(destination)?;

    if source == destination
        || std::fs::canonicalize(&destination)
            .map(|path| path == source)
            .unwrap_or(false)
    {
        return Err(AppError::new(
            ErrorCode::DestinationIsSource,
            "The destination must be different from the source file.",
            Some(destination.to_string_lossy().into_owned()),
        ));
    }
    refuse_existing_destination(&destination)?;

    let parent = destination
        .parent()
        .expect("normalized destination has a parent");
    let mut output = create_stage(parent)?;
    let result = session
        .copy_effective_into(&mut output, chunk_size, progress)
        .and_then(|bytes_written| {
            output
                .sync_all()
                .map_err(|error| AppError::from_io(error, Some(&destination)))?;
            Ok(bytes_written)
        });
    let bytes_written = match result {
        Ok(bytes_written) => bytes_written,
        Err(error) => return Err(cleanup_owned_stage(output, error)),
    };
    if let Err((output, error)) = output.persist_noclobber(&destination) {
        return Err(cleanup_owned_stage(
            output,
            AppError::from_io(error, Some(&destination)),
        ));
    }
    session.clear_edits();
    Ok(SaveSummary {
        bytes_written,
        destination,
    })
}

pub fn export_csv_create_new(path: &Path, fields: &[ParsedField]) -> Result<(), AppError> {
    export_csv_create_new_with_progress(path, fields, &mut |_| {})
}

pub fn export_csv_create_new_with_progress(
    path: &Path,
    fields: &[ParsedField],
    progress: &mut dyn FnMut(u64),
) -> Result<(), AppError> {
    export_csv_create_new_with_stage_progress(path, fields, create_staged_file, progress)
}

#[cfg(test)]
fn export_csv_create_new_with_stage<S, F>(
    path: &Path,
    fields: &[ParsedField],
    create_stage: F,
) -> Result<(), AppError>
where
    S: OwnedStagedOutput,
    F: FnOnce(&Path) -> Result<S, AppError>,
{
    export_csv_create_new_with_stage_progress(path, fields, create_stage, &mut |_| {})
}

fn export_csv_create_new_with_stage_progress<S, F>(
    path: &Path,
    fields: &[ParsedField],
    create_stage: F,
    progress: &mut dyn FnMut(u64),
) -> Result<(), AppError>
where
    S: OwnedStagedOutput,
    F: FnOnce(&Path) -> Result<S, AppError>,
{
    let destination = normalized_destination(path)?;
    refuse_existing_destination(&destination)?;
    let parent = destination
        .parent()
        .expect("normalized destination has a parent");
    let mut output = create_stage(parent)?;
    let result = write_csv(&mut output, fields, &destination, progress).and_then(|_| {
        output
            .sync_all()
            .map_err(|error| AppError::from_io(error, Some(&destination)))
    });
    if let Err(error) = result {
        return Err(cleanup_owned_stage(output, error));
    }
    if let Err((output, error)) = output.persist_noclobber(&destination) {
        return Err(cleanup_owned_stage(
            output,
            AppError::from_io(error, Some(&destination)),
        ));
    }
    Ok(())
}

fn normalized_destination(destination: &Path) -> Result<PathBuf, AppError> {
    let name = destination
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidPath,
                "The destination must name a file.",
                Some(destination.to_string_lossy().into_owned()),
            )
        })?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent =
        std::fs::canonicalize(parent).map_err(|error| AppError::from_io(error, Some(parent)))?;
    Ok(canonical_parent.join(name))
}

fn refuse_existing_destination(destination: &Path) -> Result<(), AppError> {
    if destination.exists() {
        return Err(AppError::new(
            ErrorCode::DestinationExists,
            "The destination already exists.",
            Some(destination.to_string_lossy().into_owned()),
        ));
    }
    Ok(())
}

fn validated_chunk_size(chunk_size: usize) -> Result<usize, AppError> {
    if chunk_size == 0 {
        return Err(AppError::new(
            ErrorCode::InvalidLength,
            "Export chunks must be at least one byte.",
            None,
        ));
    }
    Ok(chunk_size.min(MAX_EXPORT_CHUNK_SIZE))
}

/// Streams the effective source bytes through a reusable bounded buffer.
pub(crate) fn copy_effective<R: Read + Seek, W: Write>(
    source: &mut R,
    output: &mut W,
    source_size: u64,
    chunk_size: usize,
    edits: &EditBuffer,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, AppError> {
    let chunk_size = validated_chunk_size(chunk_size)?;
    source.seek(SeekFrom::Start(0)).map_err(AppError::from)?;
    let mut buffer = vec![0; chunk_size];
    let mut processed = 0u64;

    while processed < source_size {
        let length = (source_size - processed).min(buffer.len() as u64) as usize;
        source
            .read_exact(&mut buffer[..length])
            .map_err(AppError::from)?;
        edits.overlay(processed, &mut buffer[..length]);
        output
            .write_all(&buffer[..length])
            .map_err(AppError::from)?;
        processed += length as u64;
        progress(processed);
    }

    output.flush().map_err(AppError::from)?;
    Ok(processed)
}

fn write_csv<W: Write>(
    output: &mut W,
    fields: &[ParsedField],
    path: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<(), AppError> {
    let mut writer = csv::WriterBuilder::new().from_writer(output);
    writer
        .write_record([
            "name",
            "offset",
            "length",
            "type",
            "endianness",
            "value",
            "comment",
        ])
        .map_err(|error| csv_error(error, path))?;
    for (index, field) in fields.iter().enumerate() {
        let length = field.length.to_string();
        writer
            .write_record([
                field.name.as_str(),
                field.offset.as_str(),
                length.as_str(),
                field_type_name(&field.field_type),
                endian_name(field.endianness),
                field.value.as_str(),
                field.comment.as_str(),
            ])
            .map_err(|error| csv_error(error, path))?;
        progress(index as u64 + 1);
    }
    writer
        .flush()
        .map_err(|error| AppError::from_io(error, Some(path)))
}

fn field_type_name(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::U8 => "u8",
        FieldType::U16 => "u16",
        FieldType::U32 => "u32",
        FieldType::I8 => "i8",
        FieldType::I16 => "i16",
        FieldType::I32 => "i32",
        FieldType::F32 => "f32",
        FieldType::F64 => "f64",
        FieldType::String => "string",
        FieldType::Bytes => "bytes",
    }
}

fn endian_name(endianness: Endian) -> &'static str {
    match endianness {
        Endian::Little => "little",
        Endian::Big => "big",
    }
}

fn csv_error(error: csv::Error, path: &Path) -> AppError {
    match error.into_kind() {
        csv::ErrorKind::Io(error) => AppError::from_io(error, Some(path)),
        _ => AppError::new(
            ErrorCode::IoError,
            "The CSV file could not be written.",
            Some(path.to_string_lossy().into_owned()),
        ),
    }
}

trait OwnedStagedOutput: Write + Sized {
    fn path(&self) -> &Path;
    fn sync_all(&mut self) -> std::io::Result<()>;
    fn persist_noclobber(self, destination: &Path) -> Result<(), (Self, std::io::Error)>;
    fn cleanup(self) -> std::io::Result<()>;
}

struct StagedFile {
    file: NamedTempFile,
}

impl Write for StagedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.file.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl OwnedStagedOutput for StagedFile {
    fn path(&self) -> &Path {
        self.file.path()
    }

    fn sync_all(&mut self) -> std::io::Result<()> {
        self.file.as_file_mut().sync_all()
    }

    fn persist_noclobber(self, destination: &Path) -> Result<(), (Self, std::io::Error)> {
        match self.file.persist_noclobber(destination) {
            Ok(_) => Ok(()),
            Err(error) => Err((Self { file: error.file }, error.error)),
        }
    }

    fn cleanup(self) -> std::io::Result<()> {
        self.file.close()
    }
}

fn create_staged_file(parent: &Path) -> Result<StagedFile, AppError> {
    NamedTempFile::new_in(parent)
        .map(|file| StagedFile { file })
        .map_err(|error| AppError::from_io(error, Some(parent)))
}

fn cleanup_owned_stage<S: OwnedStagedOutput>(stage: S, error: AppError) -> AppError {
    let temporary_path = stage.path().to_path_buf();
    match stage.cleanup() {
        Ok(()) => error,
        Err(cleanup_error) => AppError::new(
            error.code,
            error.message,
            Some(
                serde_json::json!({
                    "temporaryOutput": temporary_path,
                    "cleanupError": cleanup_error.to_string(),
                    "originalDetail": error.detail,
                })
                .to_string(),
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        copy_effective, export_csv_create_new, export_csv_create_new_with_stage,
        normalized_destination, save_session_as_with_stage, OwnedStagedOutput,
    };
    use crate::edit_buffer::EditBuffer;
    use crate::error::ErrorCode;
    use crate::session::FileSession;
    use crate::template::{Endian, FieldType, ParsedField};
    use std::io::{self, Cursor, Write};
    use std::path::Path;

    struct FailingWriter {
        remaining: usize,
        error_code: i32,
    }

    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::from_raw_os_error(self.error_code));
            }
            let written = bytes.len().min(self.remaining);
            self.remaining -= written;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FlushFailingWriter(Vec<u8>);

    impl Write for FlushFailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum StageFault {
        Write,
        Flush,
        Sync,
        PersistReplacement,
        Cleanup,
    }

    struct TestStage {
        file: tempfile::NamedTempFile,
        fault: StageFault,
    }

    impl TestStage {
        fn new(parent: &Path, fault: StageFault) -> Self {
            Self {
                file: tempfile::NamedTempFile::new_in(parent).unwrap(),
                fault,
            }
        }
    }

    impl Write for TestStage {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if matches!(self.fault, StageFault::Write) {
                return Err(io::Error::from_raw_os_error(if cfg!(windows) {
                    112
                } else {
                    28
                }));
            }
            self.file.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            if matches!(self.fault, StageFault::Flush) {
                return Err(io::Error::other("flush failed"));
            }
            self.file.flush()
        }
    }

    impl OwnedStagedOutput for TestStage {
        fn path(&self) -> &Path {
            self.file.path()
        }

        fn sync_all(&mut self) -> io::Result<()> {
            if matches!(self.fault, StageFault::Sync | StageFault::Cleanup) {
                return Err(io::Error::other("sync failed"));
            }
            self.file.as_file_mut().sync_all()
        }

        fn persist_noclobber(self, destination: &Path) -> Result<(), (Self, io::Error)> {
            if matches!(self.fault, StageFault::PersistReplacement) {
                std::fs::write(destination, b"replacement").unwrap();
                return Err((self, io::Error::from(io::ErrorKind::AlreadyExists)));
            }
            let fault = self.fault;
            match self.file.persist_noclobber(destination) {
                Ok(_) => Ok(()),
                Err(error) => Err((
                    Self {
                        file: error.file,
                        fault,
                    },
                    error.error,
                )),
            }
        }

        fn cleanup(self) -> io::Result<()> {
            if matches!(self.fault, StageFault::Cleanup) {
                self.file.close()?;
                return Err(io::Error::other("cleanup failed"));
            }
            self.file.close()
        }
    }

    fn parsed_field() -> ParsedField {
        ParsedField {
            name: "field, \"quoted\"\nline".to_owned(),
            offset: "7".to_owned(),
            field_type: FieldType::U16,
            length: 2,
            endianness: Endian::Big,
            value: "value, \"quoted\"\nline".to_owned(),
            comment: "comment, \"quoted\"\nline".to_owned(),
        }
    }

    #[test]
    fn copy_effective_overlays_multiple_chunks_and_reports_monotonic_progress() {
        let mut source = Cursor::new(vec![0, 1, 2, 3, 4, 5, 6]);
        let mut output = Vec::new();
        let mut edits = EditBuffer::default();
        edits.apply(1, 1, 9);
        edits.apply(5, 5, 8);
        let mut progress = Vec::new();

        let written = copy_effective(&mut source, &mut output, 7, 2, &edits, &mut |processed| {
            progress.push(processed)
        })
        .unwrap();

        assert_eq!(written, 7);
        assert_eq!(output, vec![0, 9, 2, 3, 4, 8, 6]);
        assert_eq!(progress, vec![2, 4, 6, 7]);
        assert!(progress.windows(2).all(|window| window[0] < window[1]));
    }

    #[test]
    fn copy_effective_rejects_zero_chunk_size_before_writing() {
        let mut source = Cursor::new(vec![1]);
        let mut output = Vec::new();
        let mut progress = |_| {};

        let error = copy_effective(
            &mut source,
            &mut output,
            1,
            0,
            &EditBuffer::default(),
            &mut progress,
        )
        .unwrap_err();

        assert_eq!(error.code(), "invalid_length");
        assert!(output.is_empty());
    }

    #[test]
    fn copy_effective_maps_disk_full_and_leaves_session_edits_dirty() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [1, 2, 3, 4]).unwrap();
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(1, 9).unwrap();
        let mut output = FailingWriter {
            remaining: 1,
            error_code: if cfg!(windows) { 112 } else { 28 },
        };
        let mut progress = |_| {};

        let error = session
            .copy_effective_into(&mut output, 2, &mut progress)
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::DiskFull);
        assert!(session.is_dirty());
    }

    #[test]
    fn copy_effective_flush_failure_leaves_session_edits_dirty() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [1, 2]).unwrap();
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(0, 9).unwrap();
        let mut output = FlushFailingWriter(Vec::new());
        let mut progress = |_| {};

        let error = session
            .copy_effective_into(&mut output, 2, &mut progress)
            .unwrap_err();

        assert_eq!(error.code(), "io_error");
        assert!(session.is_dirty());
    }

    #[test]
    fn normalized_destination_uses_current_directory_for_a_bare_filename() {
        let destination = normalized_destination(Path::new("copy.bin")).unwrap();

        assert_eq!(
            destination,
            std::fs::canonicalize(".").unwrap().join("copy.bin")
        );
    }

    #[test]
    fn save_as_owned_stage_failures_preserve_source_edits_and_final_destination() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.bin");
        std::fs::write(&source, [1, 2]).unwrap();

        for fault in [StageFault::Write, StageFault::Flush, StageFault::Sync] {
            let output = dir.path().join(format!("save-{fault:?}.bin"));
            let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
            session.edit_byte(0, 9).unwrap();

            let error = save_session_as_with_stage(&mut session, &output, 2, |parent| {
                Ok(TestStage::new(parent, fault))
            })
            .unwrap_err();

            assert!(!output.exists());
            assert_eq!(std::fs::read(&source).unwrap(), [1, 2]);
            assert!(session.is_dirty());
            assert!(matches!(error.code(), "disk_full" | "io_error"));
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        let output = dir.path().join("persist.bin");
        let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
        session.edit_byte(0, 9).unwrap();
        let error = save_session_as_with_stage(&mut session, &output, 2, |parent| {
            Ok(TestStage::new(parent, StageFault::PersistReplacement))
        })
        .unwrap_err();

        assert_eq!(error.code(), "destination_exists");
        assert_eq!(std::fs::read(&output).unwrap(), b"replacement");
        assert_eq!(std::fs::read(&source).unwrap(), [1, 2]);
        assert!(session.is_dirty());
    }

    #[test]
    fn csv_owned_stage_failures_leave_no_partial_final_or_remove_replacement() {
        let dir = tempfile::tempdir().unwrap();

        for fault in [StageFault::Write, StageFault::Flush, StageFault::Sync] {
            let output = dir.path().join(format!("csv-{fault:?}.csv"));
            let error = export_csv_create_new_with_stage(&output, &[parsed_field()], |parent| {
                Ok(TestStage::new(parent, fault))
            })
            .unwrap_err();

            assert!(!output.exists());
            assert!(matches!(error.code(), "disk_full" | "io_error"));
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let output = dir.path().join("persist.csv");
        let error = export_csv_create_new_with_stage(&output, &[parsed_field()], |parent| {
            Ok(TestStage::new(parent, StageFault::PersistReplacement))
        })
        .unwrap_err();

        assert_eq!(error.code(), "destination_exists");
        assert_eq!(std::fs::read(output).unwrap(), b"replacement");
    }

    #[test]
    fn cleanup_failure_keeps_the_original_error_code_and_records_the_owned_temp_path() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("cleanup.csv");
        let error = export_csv_create_new_with_stage(&output, &[parsed_field()], |parent| {
            Ok(TestStage::new(parent, StageFault::Cleanup))
        })
        .unwrap_err();

        assert_eq!(error.code(), "io_error");
        assert!(!output.exists());
        assert!(error.detail.unwrap().contains("temporaryOutput"));
    }

    #[test]
    fn csv_export_quotes_literals_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("fields.csv");

        export_csv_create_new(&output, &[parsed_field()]).unwrap();

        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            "name,offset,length,type,endianness,value,comment\n\"field, \"\"quoted\"\"\nline\",7,2,u16,big,\"value, \"\"quoted\"\"\nline\",\"comment, \"\"quoted\"\"\nline\"\n"
        );
        std::fs::write(&output, "preserve").unwrap();
        let error = export_csv_create_new(&output, &[parsed_field()]).unwrap_err();
        assert_eq!(error.code(), "destination_exists");
        assert_eq!(std::fs::read_to_string(output).unwrap(), "preserve");
    }
}
