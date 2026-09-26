#[cfg(test)]
mod tests {
    use super::{
        decode, load_template_file, parse_template, save_template_file_create_new, validate_field,
        validate_template, Endian, FieldType, TemplateDefinition, TemplateField,
        TemplateFileSession,
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
        let cache_len_before_parse = session.cache_len();

        let mut template = valid_template();
        template
            .fields
            .push(field("past-end", 3, FieldType::U16, None));
        assert_eq!(
            parse_template(&mut session, &template).unwrap_err().code(),
            "template_out_of_bounds"
        );
        assert_eq!(session.cache_len(), cache_len_before_parse);

        template.fields.truncate(1);
        let parsed = parse_template(&mut session, &template).unwrap();
        assert_eq!(parsed[0].value, "22068");
        assert_eq!(session.cache_len(), cache_len_before_parse);
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
    fn saving_loaded_template_detects_external_edits_and_preserves_them_until_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("header.json");
        let original = valid_template();
        save_template_file_create_new(&path, &original).unwrap();
        let mut files = TemplateFileSession::default();
        assert_eq!(files.load(&path).unwrap(), original);
        std::fs::write(&path, b"external change").unwrap();
        let mut edited = original.clone();
        edited.name = "Edited".into();

        assert_eq!(
            files.save(&edited, false, None).unwrap_err().code(),
            "external_modification"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"external change");
        files.save(&edited, true, None).unwrap();
        assert_eq!(load_template_file(&path).unwrap(), edited);
        // Successful writes become the new external-change baseline.
        files.save(&edited, false, None).unwrap();
    }

    #[test]
    fn saving_loaded_template_recreates_a_deleted_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("header.json");
        let definition = valid_template();
        save_template_file_create_new(&path, &definition).unwrap();
        let mut files = TemplateFileSession::default();
        files.load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        files.save(&definition, false, None).unwrap();
        assert_eq!(load_template_file(&path).unwrap(), definition);
    }

    #[test]
    fn save_as_requires_confirmation_for_existing_target_and_updates_the_binding_only_after_success(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let original_path = dir.path().join("original.json");
        let target_path = dir.path().join("target.json");
        let definition = valid_template();
        save_template_file_create_new(&original_path, &definition).unwrap();
        std::fs::write(&target_path, b"keep").unwrap();
        let mut files = TemplateFileSession::default();
        files.load(&original_path).unwrap();

        assert_eq!(
            files
                .save_as(&target_path, &definition, false, None)
                .unwrap_err()
                .code(),
            "destination_exists"
        );
        assert_eq!(std::fs::read(&target_path).unwrap(), b"keep");
        assert_eq!(
            files.path(),
            Some(original_path.canonicalize().unwrap().as_path())
        );
        files
            .save_as(&target_path, &definition, true, None)
            .unwrap();
        assert_eq!(
            files.path(),
            Some(target_path.canonicalize().unwrap().as_path())
        );
        assert_eq!(load_template_file(&target_path).unwrap(), definition);
    }

    #[test]
    fn template_saves_never_overwrite_the_open_binary_source() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("source.bin");
        std::fs::write(&binary, b"ROM").unwrap();
        let mut files = TemplateFileSession::default();

        assert_eq!(
            files
                .save_as(&binary, &valid_template(), true, Some(&binary))
                .unwrap_err()
                .code(),
            "destination_is_source"
        );
        assert_eq!(std::fs::read(&binary).unwrap(), b"ROM");
    }

    #[test]
    fn unloading_template_clears_its_saved_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("header.json");
        let definition = valid_template();
        save_template_file_create_new(&path, &definition).unwrap();
        let mut files = TemplateFileSession::default();
        files.load(&path).unwrap();

        files.clear();
        assert_eq!(files.path(), None);
        assert_eq!(
            files.save(&definition, false, None).unwrap_err().code(),
            "invalid_path"
        );
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
    fn json_loading_rejects_unknown_root_field_and_nested_keys() {
        let dir = tempfile::tempdir().unwrap();
        let cases = [
            (
                "unknown-root.json",
                r#"{"version":1,"name":"Header","defaultEndianness":"little","fields":[],"metadata":"ignored"}"#,
            ),
            (
                "field-condition.json",
                r#"{"version":1,"name":"Header","defaultEndianness":"little","fields":[{"name":"x","offset":"0","type":"u8","comment":"","condition":"never"}]}"#,
            ),
            (
                "nested-fields.json",
                r#"{"version":1,"name":"Header","defaultEndianness":"little","fields":[{"name":"x","offset":"0","type":"u8","comment":"","fields":[]}]}"#,
            ),
        ];

        for (name, json) in cases {
            let path = dir.path().join(name);
            std::fs::write(&path, json).unwrap();
            assert_eq!(
                load_template_file(&path).unwrap_err().code(),
                "invalid_template"
            );
        }
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

    #[test]
    fn rejects_excessive_field_count_before_session_reads() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), vec![0_u8; 8192]).unwrap();
        let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
        let mut template = valid_template();
        template.fields = (0..=super::MAX_TEMPLATE_FIELDS)
            .map(|index| field(&format!("f{index}"), index as u64, FieldType::U8, None))
            .collect();

        assert_eq!(
            parse_template(&mut session, &template).unwrap_err().code(),
            "invalid_template"
        );
        assert_eq!(session.cache_len(), 0);
        assert_eq!(session.test_read_count(), 0);
    }

    #[test]
    fn rejects_aggregate_decoded_budget_before_session_reads() {
        let field_length = MAX_READ_RANGE;
        let field_count = super::MAX_TEMPLATE_DECODED_BYTES / (field_length * 4) + 1;
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(field_length * field_count).unwrap();
        let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
        let template = TemplateDefinition {
            version: 1,
            name: "Adversarial".into(),
            default_endianness: Endian::Little,
            fields: (0..field_count)
                .map(|index| {
                    field(
                        &format!("blob{index}"),
                        index * field_length,
                        FieldType::Bytes,
                        Some(field_length),
                    )
                })
                .collect(),
        };

        let error = parse_template(&mut session, &template).unwrap_err();
        assert_eq!(error.code(), "invalid_template");
        assert!(error.message.contains("decoded data budget"));
        assert_eq!(session.cache_len(), 0);
        assert_eq!(session.test_read_count(), 0);
    }
}
use crate::error::{AppError, ErrorCode};
use crate::session::{FileSession, MAX_READ_RANGE};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Templates are intentionally flat and bounded so validation, parsing, and CSV export
/// have a predictable memory ceiling even when a file contains many valid ranges.
pub const MAX_TEMPLATE_FIELDS: usize = 4096;
/// Maximum combined input buffers plus worst-case decoded value strings (16 MiB).
pub const MAX_TEMPLATE_DECODED_BYTES: u64 = 16 * 1024 * 1024;

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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateDefinition {
    pub version: u32,
    pub name: String,
    pub default_endianness: Endian,
    pub fields: Vec<TemplateField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    if template.fields.len() > MAX_TEMPLATE_FIELDS {
        return Err(invalid_template(
            "Templates may contain at most 4096 fields.",
        ));
    }

    let mut names = HashSet::with_capacity(template.fields.len());
    let mut validated = Vec::with_capacity(template.fields.len());
    let mut decoded_budget = 0_u64;
    for field in &template.fields {
        if field.name.trim().is_empty() {
            return Err(invalid_template("Field names must not be empty."));
        }
        if !names.insert(field.name.as_str()) {
            return Err(invalid_template("Template field names must be unique."));
        }
        let validated_field = validate_field(field, template.default_endianness, file_size)?;
        decoded_budget = decoded_budget
            .checked_add(decoded_allocation_budget(&validated_field))
            .ok_or_else(|| invalid_template("The template decoded data budget is too large."))?;
        if decoded_budget > MAX_TEMPLATE_DECODED_BYTES {
            return Err(invalid_template(
                "The template exceeds the 16 MiB decoded data budget.",
            ));
        }
        validated.push(validated_field);
    }
    Ok(validated)
}

fn decoded_allocation_budget(field: &ValidatedField) -> u64 {
    let output = match field.field_type {
        // Lossy UTF-8 can replace each invalid source byte with a three-byte replacement.
        FieldType::String => field.length.saturating_mul(3),
        // "FF " needs at most three output bytes for every source byte.
        FieldType::Bytes => field.length.saturating_mul(3),
        // Numeric formatting is small, but reserve enough for sign/exponent/precision.
        _ => 32,
    };
    field.length.saturating_add(output)
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
    parse_template_with_progress(session, template, &mut |_| {})
}

pub fn parse_template_with_progress(
    session: &mut FileSession,
    template: &TemplateDefinition,
    progress: &mut dyn FnMut(u64),
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
        progress(parsed.len() as u64);
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
        FieldType::Bytes => {
            let capacity = bytes.len().saturating_mul(3).saturating_sub(1);
            let mut output = String::with_capacity(capacity);
            for (index, byte) in bytes.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                }
                write!(output, "{byte:02X}").expect("writing to a String cannot fail");
            }
            Ok(output)
        }
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

/// Tracks only the associated JSON file and its last observed bytes. Binary file
/// contents and template drafts remain in their existing, separate sessions.
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
        let definition = serde_json::from_slice::<TemplateDefinition>(&bytes)
            .map_err(|_| invalid_template("The template JSON is invalid."))?;
        validate_template(&definition, u64::MAX)?;
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(AppError::from_io(error, Some(path))),
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
                .map_err(|error| AppError::from_io(error, Some(&destination)))?,
        );
        self.baseline = output;
        Ok(())
    }
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

fn serialize_template(template: &TemplateDefinition) -> Result<Vec<u8>, AppError> {
    validate_template(template, u64::MAX)?;
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
