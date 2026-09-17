#[cfg(test)]
mod tests {
    use super::{
        decode, load_template_file, parse_template, save_template_file_create_new, validate_field,
        validate_template, Endian, FieldType, TemplateDefinition, TemplateField,
    };
    use crate::session::{FileSession, MAX_READ_RANGE};

    fn field(name: &str, offset: u64, field_type: FieldType, length: Option<u64>) -> TemplateField {
        TemplateField {
            name: name.to_owned(),
            offset: offset.to_string(),
            field_type,
            length,
            endianness: None,
            comment: String::new(),
        }
    }

    fn valid_template() -> TemplateDefinition {
        TemplateDefinition {
            version: 1,
            name: "Header".to_owned(),
            default_endianness: Endian::Little,
            fields: vec![field("count", 0, FieldType::U16, None)],
        }
    }

    #[test]
    fn bytes_requires_positive_length_and_numeric_rejects_length() {
        let bytes = field("blob", 0, FieldType::Bytes, None);
        assert_eq!(
            validate_field(&bytes, Endian::Little, 16)
                .unwrap_err()
                .code(),
            "invalid_template"
        );
        let number = field("count", 0, FieldType::U32, Some(4));
        assert_eq!(
            validate_field(&number, Endian::Little, 16)
                .unwrap_err()
                .code(),
            "invalid_template"
        );
    }

    #[test]
    fn rejects_field_past_end_without_clamping() {
        let field = field("tail", 14, FieldType::U32, None);
        assert_eq!(
            validate_field(&field, Endian::Little, 16)
                .unwrap_err()
                .code(),
            "template_out_of_bounds"
        );
    }

    #[test]
    fn template_validation_rejects_runtime_version_names_and_bad_offsets() {
        let mut template = valid_template();
        template.version = 2;
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "invalid_template"
        );

        template.version = 1;
        template.name = " \t".to_owned();
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "invalid_template"
        );

        template.name = "Header".to_owned();
        template.fields[0].name = " ".to_owned();
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "invalid_template"
        );

        template.fields[0].name = "count".to_owned();
        template.fields[0].offset = "12x".to_owned();
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "invalid_template"
        );
    }

    #[test]
    fn template_validation_rejects_duplicate_names_offset_overflow_and_zero_length() {
        let mut template = valid_template();
        template.fields.push(field("count", 2, FieldType::U8, None));
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "invalid_template"
        );

        template.fields.truncate(1);
        template.fields[0].offset = u64::MAX.to_string();
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "template_out_of_bounds"
        );

        template.fields[0] = field("text", 0, FieldType::String, Some(0));
        assert_eq!(
            validate_template(&template, 16).unwrap_err().code(),
            "invalid_template"
        );
    }

    #[test]
    fn decodes_signed_unsigned_float_string_and_bytes() {
        assert_eq!(
            decode(FieldType::U16, &[0x34, 0x12], Endian::Little).unwrap(),
            "4660"
        );
        assert_eq!(
            decode(FieldType::I16, &[0xff, 0xfe], Endian::Big).unwrap(),
            "-2"
        );
        assert_eq!(
            decode(FieldType::F32, &1.5f32.to_be_bytes(), Endian::Big).unwrap(),
            "1.5"
        );
        assert_eq!(
            decode(FieldType::String, b"ROM\0\0", Endian::Little).unwrap(),
            "ROM"
        );
        assert_eq!(
            decode(FieldType::Bytes, &[0xde, 0xad], Endian::Little).unwrap(),
            "DE AD"
        );
    }

    #[test]
    fn decodes_every_numeric_type_in_both_endian_modes() {
        let cases = [
            (FieldType::U8, vec![0xfe], Endian::Little, "254"),
            (FieldType::U8, vec![0xfe], Endian::Big, "254"),
            (FieldType::U16, vec![0x34, 0x12], Endian::Little, "4660"),
            (FieldType::U16, vec![0x12, 0x34], Endian::Big, "4660"),
            (
                FieldType::U32,
                vec![0x78, 0x56, 0x34, 0x12],
                Endian::Little,
                "305419896",
            ),
            (
                FieldType::U32,
                vec![0x12, 0x34, 0x56, 0x78],
                Endian::Big,
                "305419896",
            ),
            (FieldType::I8, vec![0xfe], Endian::Little, "-2"),
            (FieldType::I8, vec![0xfe], Endian::Big, "-2"),
            (FieldType::I16, vec![0xfe, 0xff], Endian::Little, "-2"),
            (FieldType::I16, vec![0xff, 0xfe], Endian::Big, "-2"),
            (
                FieldType::I32,
                vec![0xfe, 0xff, 0xff, 0xff],
                Endian::Little,
                "-2",
            ),
            (
                FieldType::I32,
                vec![0xff, 0xff, 0xff, 0xfe],
                Endian::Big,
                "-2",
            ),
            (
                FieldType::F32,
                1.5f32.to_le_bytes().to_vec(),
                Endian::Little,
                "1.5",
            ),
            (
                FieldType::F32,
                1.5f32.to_be_bytes().to_vec(),
                Endian::Big,
                "1.5",
            ),
            (
                FieldType::F64,
                1.5f64.to_le_bytes().to_vec(),
                Endian::Little,
                "1.5",
            ),
            (
                FieldType::F64,
                1.5f64.to_be_bytes().to_vec(),
                Endian::Big,
                "1.5",
            ),
        ];

        for (field_type, bytes, endian, expected) in cases {
            assert_eq!(decode(field_type, &bytes, endian).unwrap(), expected);
        }
    }

    #[test]
    fn decode_rejects_wrong_numeric_byte_width() {
        assert_eq!(
            decode(FieldType::U32, &[1, 2], Endian::Little)
                .unwrap_err()
                .code(),
            "invalid_template"
        );
    }

    #[test]
    fn parse_validates_all_fields_before_reading_and_uses_sparse_edits() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [0x34, 0x12, 0, 0]).unwrap();
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(1, 0x56).unwrap();

        let mut template = valid_template();
        template
            .fields
            .push(field("past-end", 3, FieldType::U16, None));
        assert_eq!(
            parse_template(&mut session, &template).unwrap_err().code(),
            "template_out_of_bounds"
        );
        assert_eq!(session.cache_len(), 0);

        template.fields.truncate(1);
        let parsed = parse_template(&mut session, &template).unwrap();
        assert_eq!(parsed[0].value, "22068");
        assert_eq!(session.cache_len(), 0);
    }

    #[test]
    fn template_file_io_reports_invalid_json_and_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        assert_eq!(
            load_template_file(&missing).unwrap_err().code(),
            "file_not_found"
        );

        let invalid = dir.path().join("invalid.json");
        std::fs::write(&invalid, "{not json}").unwrap();
        assert_eq!(
            load_template_file(&invalid).unwrap_err().code(),
            "invalid_template"
        );

        let existing = dir.path().join("existing.json");
        std::fs::write(&existing, "keep").unwrap();
        let definition = valid_template();
        assert_eq!(
            save_template_file_create_new(&existing, &definition)
                .unwrap_err()
                .code(),
            "destination_exists"
        );
        assert_eq!(std::fs::read_to_string(existing).unwrap(), "keep");
    }

    #[test]
    fn template_file_io_loads_valid_json_and_enforces_version_at_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("template.json");
        std::fs::write(
            &path,
            r#"{"version":2,"name":"bad","defaultEndianness":"little","fields":[]}"#,
        )
        .unwrap();
        assert_eq!(
            load_template_file(&path).unwrap_err().code(),
            "invalid_template"
        );

        let definition = valid_template();
        let output = dir.path().join("created.json");
        save_template_file_create_new(&output, &definition).unwrap();
        assert!(std::fs::read_to_string(&output)
            .unwrap()
            .contains("\"version\": 1"));
        assert_eq!(load_template_file(&output).unwrap(), definition);
    }

    #[test]
    fn malformed_field_type_is_rejected_by_json_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid-type.json");
        std::fs::write(
            &path,
            r#"{"version":1,"name":"bad","defaultEndianness":"little","fields":[{"name":"x","offset":"0","type":"u64"}]}"#,
        )
        .unwrap();
        assert_eq!(
            load_template_file(&path).unwrap_err().code(),
            "invalid_template"
        );
    }

    #[test]
    fn loading_rejects_template_json_larger_than_the_bounded_input_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("too-large.json");
        let comment = "x".repeat(MAX_READ_RANGE as usize);
        let json = format!(
            r#"{{"version":1,"name":"Header","defaultEndianness":"little","fields":[{{"name":"byte","offset":"0","type":"u8","comment":"{comment}"}}]}}"#
        );
        std::fs::write(&path, json).unwrap();

        assert_eq!(
            load_template_file(&path).unwrap_err().code(),
            "invalid_template"
        );
    }
}
use crate::error::{AppError, ErrorCode};
use crate::session::{FileSession, MAX_READ_RANGE};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    U8,
    U16,
    U32,
    I8,
    I16,
    I32,
    F32,
    F64,
    String,
    Bytes,
}

impl FieldType {
    fn fixed_width(&self) -> Option<u64> {
        match self {
            Self::U8 | Self::I8 => Some(1),
            Self::U16 | Self::I16 => Some(2),
            Self::U32 | Self::I32 | Self::F32 => Some(4),
            Self::F64 => Some(8),
            Self::String | Self::Bytes => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDefinition {
    pub version: u32,
    pub name: String,
    pub default_endianness: Endian,
    pub fields: Vec<TemplateField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateField {
    pub name: String,
    pub offset: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endianness: Option<Endian>,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedField {
    pub name: String,
    pub offset: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub length: u64,
    pub endianness: Endian,
    pub value: String,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedField {
    pub name: String,
    pub offset: u64,
    pub field_type: FieldType,
    pub length: u64,
    pub endianness: Endian,
    pub comment: String,
}

pub fn validate_template(
    template: &TemplateDefinition,
    file_size: u64,
) -> Result<Vec<ValidatedField>, AppError> {
    if template.version != 1 {
        return Err(invalid_template("Only template version 1 is supported."));
    }
    if template.name.trim().is_empty() {
        return Err(invalid_template("Template names must not be empty."));
    }

    let mut names = HashSet::with_capacity(template.fields.len());
    let mut validated = Vec::with_capacity(template.fields.len());
    for field in &template.fields {
        if field.name.trim().is_empty() {
            return Err(invalid_template("Field names must not be empty."));
        }
        if !names.insert(field.name.as_str()) {
            return Err(invalid_template("Template field names must be unique."));
        }
        validated.push(validate_field(
            field,
            template.default_endianness,
            file_size,
        )?);
    }
    Ok(validated)
}

fn validate_field(
    field: &TemplateField,
    default_endianness: Endian,
    file_size: u64,
) -> Result<ValidatedField, AppError> {
    if field.name.trim().is_empty() {
        return Err(invalid_template("Field names must not be empty."));
    }

    let offset = parse_decimal_offset(&field.offset)?;
    let length = match field.field_type.fixed_width() {
        Some(width) => {
            if field.length.is_some() {
                return Err(invalid_template(
                    "Fixed-width numeric fields must not specify a length.",
                ));
            }
            width
        }
        None => {
            let Some(length) = field.length else {
                return Err(invalid_template(
                    "String and bytes fields require a positive length.",
                ));
            };
            if length == 0 || length > MAX_READ_RANGE {
                return Err(invalid_template(
                    "String and bytes field lengths must be between 1 and 1048576 bytes.",
                ));
            }
            length
        }
    };
    let end = offset.checked_add(length).ok_or_else(|| {
        template_out_of_bounds("The template field range overflows the supported file size.")
    })?;
    if end > file_size {
        return Err(template_out_of_bounds(
            "The template field range extends beyond the file.",
        ));
    }

    Ok(ValidatedField {
        name: field.name.clone(),
        offset,
        field_type: field.field_type.clone(),
        length,
        endianness: field.endianness.unwrap_or(default_endianness),
        comment: field.comment.clone(),
    })
}

pub fn parse_template(
    session: &mut FileSession,
    template: &TemplateDefinition,
) -> Result<Vec<ParsedField>, AppError> {
    let fields = validate_template(template, session.info().size)?;
    let mut parsed = Vec::with_capacity(fields.len());

    for field in fields {
        let mut bytes = vec![0; field.length as usize];
        let read = session.read_effective_chunk(field.offset, &mut bytes)?;
        if read != bytes.len() {
            return Err(template_out_of_bounds(
                "The template field range extends beyond the file.",
            ));
        }
        let value = decode(field.field_type.clone(), &bytes, field.endianness)?;
        parsed.push(ParsedField {
            name: field.name,
            offset: field.offset.to_string(),
            field_type: field.field_type,
            length: field.length,
            endianness: field.endianness,
            value,
            comment: field.comment,
        });
    }

    Ok(parsed)
}

pub fn decode(field_type: FieldType, bytes: &[u8], endian: Endian) -> Result<String, AppError> {
    match field_type {
        FieldType::U8 => Ok(expect_array::<1>(bytes)?
            .first()
            .copied()
            .unwrap()
            .to_string()),
        FieldType::U16 => Ok(match endian {
            Endian::Little => u16::from_le_bytes(expect_array(bytes)?),
            Endian::Big => u16::from_be_bytes(expect_array(bytes)?),
        }
        .to_string()),
        FieldType::U32 => Ok(match endian {
            Endian::Little => u32::from_le_bytes(expect_array(bytes)?),
            Endian::Big => u32::from_be_bytes(expect_array(bytes)?),
        }
        .to_string()),
        FieldType::I8 => Ok((expect_array::<1>(bytes)?[0] as i8).to_string()),
        FieldType::I16 => Ok(match endian {
            Endian::Little => i16::from_le_bytes(expect_array(bytes)?),
            Endian::Big => i16::from_be_bytes(expect_array(bytes)?),
        }
        .to_string()),
        FieldType::I32 => Ok(match endian {
            Endian::Little => i32::from_le_bytes(expect_array(bytes)?),
            Endian::Big => i32::from_be_bytes(expect_array(bytes)?),
        }
        .to_string()),
        FieldType::F32 => Ok(match endian {
            Endian::Little => f32::from_le_bytes(expect_array(bytes)?),
            Endian::Big => f32::from_be_bytes(expect_array(bytes)?),
        }
        .to_string()),
        FieldType::F64 => Ok(match endian {
            Endian::Little => f64::from_le_bytes(expect_array(bytes)?),
            Endian::Big => f64::from_be_bytes(expect_array(bytes)?),
        }
        .to_string()),
        FieldType::String => Ok(String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_owned()),
        FieldType::Bytes => Ok(bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ")),
    }
}

pub fn load_template_file(path: &Path) -> Result<TemplateDefinition, AppError> {
    let metadata = std::fs::metadata(path).map_err(|error| AppError::from_io(error, Some(path)))?;
    if metadata.len() > MAX_READ_RANGE {
        return Err(invalid_template(
            "Template files must not exceed 1048576 bytes.",
        ));
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| AppError::from_io(error, Some(path)))?;
    let template = serde_json::from_str::<TemplateDefinition>(&contents)
        .map_err(|_| invalid_template("The template JSON is invalid."))?;
    validate_template(&template, u64::MAX)?;
    Ok(template)
}

pub fn save_template_file_create_new(
    path: &Path,
    template: &TemplateDefinition,
) -> Result<(), AppError> {
    validate_template(template, u64::MAX)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| AppError::from_io(error, Some(path)))?;

    let result = serde_json::to_writer_pretty(&mut file, template)
        .map_err(|_| {
            AppError::new(
                ErrorCode::IoError,
                "The template file could not be written.",
                None,
            )
        })
        .and_then(|_| {
            file.write_all(b"\n")
                .map_err(|error| AppError::from_io(error, Some(path)))
        })
        .and_then(|_| {
            file.flush()
                .map_err(|error| AppError::from_io(error, Some(path)))
        });
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

fn expect_array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], AppError> {
    bytes.try_into().map_err(|_| {
        invalid_template("The template field data does not match the field type width.")
    })
}

fn parse_decimal_offset(value: &str) -> Result<u64, AppError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_template(
            "Template offsets must be unsigned decimal strings.",
        ));
    }
    value
        .parse::<u64>()
        .map_err(|_| invalid_template("Template offsets are outside the supported range."))
}

fn invalid_template(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidTemplate, message, None)
}

fn template_out_of_bounds(message: &str) -> AppError {
    AppError::new(ErrorCode::TemplateOutOfBounds, message, None)
}
