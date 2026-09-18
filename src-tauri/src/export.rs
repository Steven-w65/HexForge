use crate::edit_buffer::EditBuffer;
use crate::error::{AppError, ErrorCode};
use crate::session::{FileSession, MAX_READ_RANGE};
use crate::template::{Endian, FieldType, ParsedField};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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

    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
        .map_err(|error| AppError::from_io(error, Some(&destination)))?;
    let mut progress = |_| {};
    let result = session
        .copy_effective_into(&mut output, chunk_size, &mut progress)
        .and_then(|bytes_written| {
            output
                .sync_all()
                .map_err(|error| AppError::from_io(error, Some(&destination)))?;
            Ok(bytes_written)
        });
    drop(output);

    let bytes_written = cleanup_created_output(&destination, result)?;
    session.clear_edits();
    Ok(SaveSummary {
        bytes_written,
        destination,
    })
}

pub fn export_csv_create_new(path: &Path, fields: &[ParsedField]) -> Result<(), AppError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| AppError::from_io(error, Some(path)))?;
    let result = write_csv(&mut file, fields, path);
    drop(file);
    cleanup_created_output(path, result)
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
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent =
        std::fs::canonicalize(parent).map_err(|error| AppError::from_io(error, Some(parent)))?;
    Ok(canonical_parent.join(name))
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
    for field in fields {
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
    }
    writer.flush().map_err(|error| csv_error(error, path))
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

/// Removes a destination only after this operation successfully created it.
fn cleanup_created_output<T>(path: &Path, result: Result<T, AppError>) -> Result<T, AppError> {
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{cleanup_created_output, copy_effective, export_csv_create_new};
    use crate::edit_buffer::EditBuffer;
    use crate::error::ErrorCode;
    use crate::session::FileSession;
    use crate::template::{Endian, FieldType, ParsedField};
    use std::io::{self, Cursor, Write};

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
    fn cleanup_created_output_removes_only_the_known_partial_destination() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("partial.bin");
        std::fs::write(&partial, [1]).unwrap();
        let expected = crate::error::AppError::new(ErrorCode::IoError, "write failed", None);

        let error = cleanup_created_output(&partial, Err::<(), _>(expected.clone())).unwrap_err();

        assert_eq!(error, expected);
        assert!(!partial.exists());
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
