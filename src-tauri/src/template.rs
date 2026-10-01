//! Strict, unversioned template schema and bounded, ordered interpreter.

use crate::error::{AppError, ErrorCode};
use crate::session::{FileSession, MAX_READ_RANGE};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endian {
    Little,
    Big,
}

pub const MAX_TEMPLATE_DEPTH: usize = 8;
pub const MAX_TEMPLATE_DEFINITIONS: u64 = 4096;
pub const MAX_TEMPLATE_EXPANDED_NODES: u64 = 10_000;
pub const MAX_TEMPLATE_DECODED_BYTES: u64 = 16 * 1024 * 1024;
const PARSER_CACHE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateDefinition {
    pub name: String,
    pub default_endianness: Endian,
    pub fields: Vec<TemplateField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Bytes,
    Struct,
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextEncoding {
    Ascii,
    Utf8,
    Utf16le,
    Utf16be,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlacementMode {
    Absolute,
    Relative,
    Sequential,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Placement {
    pub mode: PlacementMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<OffsetSpec>,
}

/// Literal offsets remain decimal strings. Referenced offsets stay u64 from
/// decode through checked addition; no JavaScript/float expression evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OffsetSpec {
    Literal(String),
    Reference(OffsetReference),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffsetReference {
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LengthSpec {
    Fixed(u64),
    Reference(LengthReference),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LengthReference {
    #[serde(rename = "ref")]
    pub reference: String,
    pub max: u64,
}
impl LengthSpec {
    fn bound(&self) -> u64 {
        match self {
            Self::Fixed(value) => *value,
            Self::Reference(value) => value.max,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum ExpectedValue {
    Unsigned(String),
    Signed(String),
    Float(String),
    Bool(bool),
    String(String),
    Bytes(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Expectation {
    Equals {
        value: ExpectedValue,
    },
    OneOf {
        values: Vec<ExpectedValue>,
    },
    Range {
        min: ExpectedValue,
        max: ExpectedValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CountSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed: Option<u64>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Comparison {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum ConditionValue {
    Unsigned(String),
    Signed(String),
    Bool(bool),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    #[serde(rename = "ref")]
    pub reference: String,
    pub op: Comparison,
    pub value: ConditionValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitFlag {
    pub bit: u8,
    pub name: String,
}

/// Named fields belong to a root or struct. The element of an array is unnamed.
/// Cross-type combinations are rejected in `validate_template`, not ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateField {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endianness: Option<Endian>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<LengthSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<TextEncoding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<TemplateField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<Box<TemplateField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<CountSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_labels: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_flags: Option<Vec<BitFlag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expectation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedNode {
    pub kind: String,
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub offset: Option<String>,
    pub length: Option<String>,
    pub value: Option<String>,
    pub endianness: Option<Endian>,
    pub comment: String,
    pub enum_label: Option<String>,
    pub flags: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub children: Vec<ParsedNode>,
}

fn error(path: &str, code: ErrorCode, message: impl Into<String>) -> AppError {
    AppError::new(
        code,
        format!("{path}: {}", message.into()),
        Some(path.to_owned()),
    )
}

fn decimal_u64(value: &str, path: &str, label: &str) -> Result<u64, AppError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            format!("{label} must be an unsigned decimal string."),
        ));
    }
    value.parse().map_err(|_| {
        error(
            path,
            ErrorCode::InvalidTemplate,
            format!("{label} exceeds u64."),
        )
    })
}

fn decimal_i64(value: &str, path: &str, label: &str) -> Result<i64, AppError> {
    if value.is_empty()
        || value == "-"
        || !value
            .trim_start_matches('-')
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            format!("{label} must be a signed decimal string."),
        ));
    }
    value.parse().map_err(|_| {
        error(
            path,
            ErrorCode::InvalidTemplate,
            format!("{label} exceeds i64."),
        )
    })
}

fn width(kind: FieldType) -> Option<u64> {
    match kind {
        FieldType::U8 | FieldType::I8 | FieldType::Bool => Some(1),
        FieldType::U16 | FieldType::I16 => Some(2),
        FieldType::U32 | FieldType::I32 | FieldType::F32 => Some(4),
        FieldType::U64 | FieldType::I64 | FieldType::F64 => Some(8),
        _ => None,
    }
}

fn unsigned_type(kind: FieldType) -> bool {
    matches!(
        kind,
        FieldType::U8 | FieldType::U16 | FieldType::U32 | FieldType::U64
    )
}
fn signed_type(kind: FieldType) -> bool {
    matches!(
        kind,
        FieldType::I8 | FieldType::I16 | FieldType::I32 | FieldType::I64
    )
}
fn integer_type(kind: FieldType) -> bool {
    unsigned_type(kind) || signed_type(kind)
}

fn bounded_read(length: Option<u64>, path: &str, label: &str) -> Result<u64, AppError> {
    match length {
        Some(1..=MAX_READ_RANGE) => Ok(length.unwrap()),
        _ => Err(error(
            path,
            ErrorCode::InvalidTemplate,
            format!("{label} must be 1..={MAX_READ_RANGE} bytes."),
        )),
    }
}

fn checked_budget_add(left: u64, right: u64, path: &str) -> Result<u64, AppError> {
    left.checked_add(right)
        .filter(|value| *value <= MAX_TEMPLATE_DECODED_BYTES)
        .ok_or_else(|| {
            error(
                path,
                ErrorCode::InvalidTemplate,
                "The decoded-data limit is 16 MiB.",
            )
        })
}

fn checked_node_add(left: u64, right: u64, path: &str) -> Result<u64, AppError> {
    left.checked_add(right)
        .filter(|value| *value <= MAX_TEMPLATE_EXPANDED_NODES)
        .ok_or_else(|| {
            error(
                path,
                ErrorCode::InvalidTemplate,
                "The expanded-result limit is 10000 nodes.",
            )
        })
}

#[derive(Default)]
struct StaticLimits {
    definitions: u64,
}

/// Structural and fixed-count preflight. No binary read occurs before this succeeds.
pub fn validate_template(template: &TemplateDefinition) -> Result<(), AppError> {
    if template.name.trim().is_empty() {
        return Err(error(
            "template",
            ErrorCode::InvalidTemplate,
            "The template name must not be empty.",
        ));
    }
    let mut limits = StaticLimits::default();
    let (nodes, decoded) = validate_fields(&template.fields, "", 1, &mut limits)?;
    checked_node_add(0, nodes, "template")?;
    checked_budget_add(0, decoded, "template")?;
    validate_references(&template.fields, "", &mut HashMap::new())?;
    Ok(())
}

// Symbolic array scopes use `[]`; existence of a concrete index or a
// conditionally skipped value still has to be proven at runtime.
fn validate_references(
    fields: &[TemplateField],
    scope: &str,
    seen: &mut HashMap<String, FieldType>,
) -> Result<(), AppError> {
    for field in fields {
        let name = field.name.as_deref().unwrap_or("");
        let path = if scope.is_empty() {
            name.to_owned()
        } else {
            format!("{scope}.{name}")
        };
        validate_field_references(field, &path, seen)?;
        if unsigned_type(field.field_type)
            || signed_type(field.field_type)
            || matches!(field.field_type, FieldType::Bool | FieldType::String)
        {
            seen.insert(path, field.field_type);
        }
    }
    Ok(())
}

fn check_reference_type(
    reference: &str,
    path: &str,
    seen: &HashMap<String, FieldType>,
    allowed: impl Fn(FieldType) -> bool,
) -> Result<(), AppError> {
    let mut scope = parent_scope(path);
    let mut actual = None;
    while let Some(prefix) = scope {
        actual = seen.get(&format!("{prefix}.{reference}"));
        if actual.is_some() {
            break;
        }
        scope = parent_scope(prefix);
    }
    actual = actual.or_else(|| seen.get(reference));
    match actual {
        Some(kind) if allowed(*kind) => Ok(()),
        Some(_) => Err(error(path, ErrorCode::InvalidTemplate, format!("Reference '{reference}' has the wrong type; an earlier compatible value is required."))),
        None if reference.contains('[') => Ok(()),
        None => Err(error(path, ErrorCode::InvalidTemplate, format!("Reference '{reference}' must identify an earlier field."))),
    }
}

fn validate_field_references(
    field: &TemplateField,
    path: &str,
    seen: &mut HashMap<String, FieldType>,
) -> Result<(), AppError> {
    if let Some(LengthSpec::Reference(value)) = &field.length {
        check_reference_type(&value.reference, path, seen, unsigned_type)?;
    }
    if let Some(OffsetSpec::Reference(value)) = field
        .placement
        .as_ref()
        .and_then(|value| value.offset.as_ref())
    {
        check_reference_type(&value.reference, path, seen, unsigned_type)?;
    }
    if let Some(condition) = &field.condition {
        check_reference_type(&condition.reference, path, seen, |kind| {
            match condition.value {
                ConditionValue::Unsigned(_) => unsigned_type(kind),
                ConditionValue::Signed(_) => signed_type(kind),
                ConditionValue::Bool(_) => kind == FieldType::Bool,
                ConditionValue::String(_) => kind == FieldType::String,
            }
        })?;
    }
    if let Some(reference) = field
        .count
        .as_ref()
        .and_then(|value| value.reference.as_deref())
    {
        check_reference_type(reference, path, seen, unsigned_type)?;
    }
    if let Some(children) = &field.fields {
        validate_references(children, path, seen)?;
    }
    if let Some(element) = &field.element {
        validate_field_references(element, &format!("{path}[]"), seen)?;
    }
    Ok(())
}

fn validate_fields(
    fields: &[TemplateField],
    scope: &str,
    depth: usize,
    limits: &mut StaticLimits,
) -> Result<(u64, u64), AppError> {
    let mut names = HashSet::new();
    let mut nodes = 0;
    let mut decoded = 0;
    for field in fields {
        let name = field.name.as_deref().unwrap_or("");
        let path = if scope.is_empty() {
            name.to_owned()
        } else {
            format!("{scope}.{name}")
        };
        if name.trim().is_empty() || name.contains(['.', '[', ']']) {
            return Err(error(
                &path,
                ErrorCode::InvalidTemplate,
                "Field names must be nonempty and cannot contain '.', '[' or ']'.",
            ));
        }
        if !names.insert(name) {
            return Err(error(
                &path,
                ErrorCode::InvalidTemplate,
                "Field names must be unique within their structure.",
            ));
        }
        let (field_nodes, field_decoded) = validate_field(field, &path, depth, false, limits)?;
        nodes = checked_node_add(nodes, field_nodes, &path)?;
        decoded = checked_budget_add(decoded, field_decoded, &path)?;
    }
    Ok((nodes, decoded))
}

fn validate_field(
    field: &TemplateField,
    path: &str,
    depth: usize,
    element: bool,
    limits: &mut StaticLimits,
) -> Result<(u64, u64), AppError> {
    let (nodes, decoded) = validate_field_body(field, path, depth, element, limits)?;
    Ok((
        nodes,
        checked_budget_add(decoded, metadata_cost(field, path)?, path)?,
    ))
}

/// Arrays duplicate comments, labels and paths for every result. Charge that
/// output alongside value formatting, not just the bytes read from the file.
fn metadata_cost(field: &TemplateField, path: &str) -> Result<u64, AppError> {
    let mut cost = checked_budget_add(
        128,
        (path.len() as u64).checked_mul(2).ok_or_else(|| {
            error(
                path,
                ErrorCode::InvalidTemplate,
                "Result path budget overflows.",
            )
        })?,
        path,
    )?;
    cost = checked_budget_add(
        cost,
        field.comment.as_ref().map_or(0, |value| value.len() as u64),
        path,
    )?;
    if let Some(labels) = &field.enum_labels {
        cost = checked_budget_add(
            cost,
            labels
                .values()
                .map(|value| value.len() as u64)
                .max()
                .unwrap_or(0),
            path,
        )?;
    }
    if let Some(flags) = &field.bit_flags {
        for flag in flags {
            cost = checked_budget_add(cost, flag.name.len() as u64, path)?;
        }
    }
    Ok(cost)
}

fn validate_field_body(
    field: &TemplateField,
    path: &str,
    depth: usize,
    element: bool,
    limits: &mut StaticLimits,
) -> Result<(u64, u64), AppError> {
    if depth > MAX_TEMPLATE_DEPTH {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            "The nesting limit is 8.",
        ));
    }
    limits.definitions = limits
        .definitions
        .checked_add(1)
        .filter(|count| *count <= MAX_TEMPLATE_DEFINITIONS)
        .ok_or_else(|| {
            error(
                path,
                ErrorCode::InvalidTemplate,
                "The definition limit is 4096 fields.",
            )
        })?;
    if element && (field.name.is_some() || field.placement.is_some() || field.condition.is_some()) {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            "Array elements must be unnamed, sequential and unconditional.",
        ));
    }
    if let Some(placement) = &field.placement {
        match placement.mode {
            PlacementMode::Sequential if placement.offset.is_some() => {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Sequential placement cannot have an offset.",
                ))
            }
            PlacementMode::Absolute | PlacementMode::Relative => match &placement.offset {
                Some(OffsetSpec::Literal(value)) => {
                    decimal_u64(value, path, "Placement offset")?;
                }
                Some(OffsetSpec::Reference(value)) => {
                    if value.reference.trim().is_empty() {
                        return Err(error(
                            path,
                            ErrorCode::InvalidTemplate,
                            "Offset ref must name an earlier unsigned field.",
                        ));
                    }
                    if let Some(add) = &value.add {
                        decimal_u64(add, path, "Offset addition")?;
                    }
                }
                None => {
                    return Err(error(
                        path,
                        ErrorCode::InvalidTemplate,
                        "Placement requires an offset.",
                    ))
                }
            },
            _ => {}
        }
    }
    if let Some(LengthSpec::Reference(value)) = &field.length {
        if value.reference.trim().is_empty() {
            return Err(error(
                path,
                ErrorCode::InvalidTemplate,
                "Length ref must name an earlier unsigned field.",
            ));
        }
    }
    if let Some(expectation) = &field.expect {
        validate_expectation(expectation, field.field_type, path)?;
    }
    if let Some(align) = field.align {
        if align == 0 || !align.is_power_of_two() || align > MAX_READ_RANGE {
            return Err(error(
                path,
                ErrorCode::InvalidTemplate,
                "Alignment must be a power of two between 1 and 1048576.",
            ));
        }
    }
    if let Some(condition) = &field.condition {
        if condition.reference.trim().is_empty() {
            return Err(error(
                path,
                ErrorCode::InvalidTemplate,
                "Condition ref must name an earlier parsed value.",
            ));
        }
        match &condition.value {
            ConditionValue::Unsigned(value) => {
                decimal_u64(value, path, "Condition value")?;
            }
            ConditionValue::Signed(value) => {
                decimal_i64(value, path, "Condition value")?;
            }
            ConditionValue::Bool(_) | ConditionValue::String(_) => {
                if !matches!(condition.op, Comparison::Eq | Comparison::Ne) {
                    return Err(error(
                        path,
                        ErrorCode::InvalidTemplate,
                        "Boolean and text conditions support eq/ne only.",
                    ));
                }
            }
        }
    }
    if field.enum_labels.is_some() && !integer_type(field.field_type) {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            "Enum labels require an integer field.",
        ));
    }
    if let Some(labels) = &field.enum_labels {
        for (value, label) in labels {
            let bits = width(field.field_type).expect("enum labels require integers") * 8;
            let fits = if unsigned_type(field.field_type) {
                let number = decimal_u64(value, path, "Enum key")?;
                bits == 64 || number < (1_u64 << bits)
            } else {
                let number = decimal_i64(value, path, "Enum key")?;
                bits == 64 || (number >= -(1_i64 << (bits - 1)) && number < (1_i64 << (bits - 1)))
            };
            if !fits {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Enum key exceeds its integer field width.",
                ));
            }
            if label.trim().is_empty() {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Enum labels cannot be empty.",
                ));
            }
        }
    }
    if field.bit_flags.is_some() && !unsigned_type(field.field_type) {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            "Bit flags require an unsigned integer field.",
        ));
    }
    if let Some(flags) = &field.bit_flags {
        let mut bits = HashSet::new();
        let mut names = HashSet::new();
        for flag in flags {
            if flag.bit as u64 >= width(field.field_type).unwrap() * 8
                || !bits.insert(flag.bit)
                || flag.name.trim().is_empty()
                || !names.insert(flag.name.as_str())
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Bit flags need unique in-range bit numbers and nonempty names.",
                ));
            }
        }
    }
    let scalar_extras = field.length.is_some()
        || field.max_length.is_some()
        || field.encoding.is_some()
        || field.fields.is_some()
        || field.element.is_some()
        || field.count.is_some();
    if width(field.field_type).is_some() {
        if scalar_extras {
            return Err(error(
                path,
                ErrorCode::InvalidTemplate,
                "Numeric and bool fields cannot have text, byte or container properties.",
            ));
        }
        if field.field_type == FieldType::Bool
            && (field.endianness.is_some()
                || field.enum_labels.is_some()
                || field.bit_flags.is_some())
        {
            return Err(error(
                path,
                ErrorCode::InvalidTemplate,
                "Bool fields cannot have endian, enum or flag properties.",
            ));
        }
        return Ok((1, width(field.field_type).unwrap() * 4));
    }
    if field.enum_labels.is_some() || field.bit_flags.is_some() {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            "Only integer fields support labels and flags.",
        ));
    }
    match field.field_type {
        FieldType::String => {
            if field.endianness.is_some()
                || field.fields.is_some()
                || field.element.is_some()
                || field.count.is_some()
                || field.encoding.is_none()
                || (field.length.is_some() == field.max_length.is_some())
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Text needs encoding and exactly one of length or maxLength.",
                ));
            }
            let length = bounded_read(
                field
                    .length
                    .as_ref()
                    .map(LengthSpec::bound)
                    .or(field.max_length),
                path,
                "Text bound",
            )?;
            if matches!(
                field.encoding,
                Some(TextEncoding::Utf16le | TextEncoding::Utf16be)
            ) && length % 2 != 0
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "UTF-16 byte bounds must be even.",
                ));
            }
            Ok((
                1,
                length.checked_mul(4).ok_or_else(|| {
                    error(path, ErrorCode::InvalidTemplate, "Text budget overflow.")
                })?,
            ))
        }
        FieldType::Bytes => {
            if field.endianness.is_some()
                || field.max_length.is_some()
                || field.encoding.is_some()
                || field.fields.is_some()
                || field.element.is_some()
                || field.count.is_some()
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Bytes fields require only length.",
                ));
            }
            let length = bounded_read(
                field.length.as_ref().map(LengthSpec::bound),
                path,
                "Byte length",
            )?;
            Ok((
                1,
                length.checked_mul(4).ok_or_else(|| {
                    error(path, ErrorCode::InvalidTemplate, "Byte budget overflow.")
                })?,
            ))
        }
        FieldType::Struct => {
            if field.length.is_some()
                || field.max_length.is_some()
                || field.encoding.is_some()
                || field.element.is_some()
                || field.count.is_some()
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Struct fields require only fields.",
                ));
            }
            let children = field.fields.as_deref().ok_or_else(|| {
                error(path, ErrorCode::InvalidTemplate, "Struct requires fields.")
            })?;
            let (child_nodes, decoded) = validate_fields(children, path, depth + 1, limits)?;
            Ok((checked_node_add(1, child_nodes, path)?, decoded))
        }
        FieldType::Array => {
            if field.length.is_some()
                || field.max_length.is_some()
                || field.encoding.is_some()
                || field.fields.is_some()
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Array fields require count and element.",
                ));
            }
            let count = field
                .count
                .as_ref()
                .ok_or_else(|| error(path, ErrorCode::InvalidTemplate, "Array requires count."))?;
            if count.fixed.is_some() == count.reference.is_some()
                || count
                    .reference
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty())
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Array count needs exactly one fixed or ref value.",
                ));
            }
            let item = field.element.as_ref().ok_or_else(|| {
                error(path, ErrorCode::InvalidTemplate, "Array requires element.")
            })?;
            let (item_nodes, item_decoded) =
                validate_field(item, &format!("{path}[]"), depth + 1, true, limits)?;
            let fixed = count.fixed.unwrap_or(1);
            let nodes = item_nodes.checked_mul(fixed).ok_or_else(|| {
                error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Array node count overflows.",
                )
            })?;
            let decoded = item_decoded.checked_mul(fixed).ok_or_else(|| {
                error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Array decoded data overflows.",
                )
            })?;
            Ok((
                checked_node_add(1, nodes, path)?,
                checked_budget_add(0, decoded, path)?,
            ))
        }
        _ => unreachable!("fixed-width variants returned above"),
    }
}

#[derive(Clone)]
enum ReferenceValue {
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text(String),
}

fn hex_value(value: &str, path: &str) -> Result<Vec<u8>, AppError> {
    value
        .split_whitespace()
        .map(|part| {
            if part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Expected bytes must use two-digit hex pairs separated by spaces.",
                ));
            }
            u8::from_str_radix(part, 16)
                .map_err(|_| error(path, ErrorCode::InvalidTemplate, "Invalid expected byte."))
        })
        .collect()
}

fn compare_expected(
    actual: &str,
    expected: &ExpectedValue,
    path: &str,
) -> Result<Option<std::cmp::Ordering>, AppError> {
    Ok(match expected {
        ExpectedValue::Unsigned(value) => actual
            .parse::<u64>()
            .ok()
            .map(|number| number.cmp(&value.parse::<u64>().unwrap())),
        ExpectedValue::Signed(value) => actual
            .parse::<i64>()
            .ok()
            .map(|number| number.cmp(&value.parse::<i64>().unwrap())),
        ExpectedValue::Float(value) => actual
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(|number| number.partial_cmp(&value.parse::<f64>().unwrap())),
        ExpectedValue::Bool(value) => Some((actual == "true").cmp(value)),
        ExpectedValue::String(value) => Some(actual.cmp(value)),
        ExpectedValue::Bytes(value) => Some(hex_value(actual, path)?.cmp(&hex_value(value, path)?)),
    })
}

fn validate_expected_value(
    value: &ExpectedValue,
    kind: FieldType,
    path: &str,
) -> Result<(), AppError> {
    let compatible = match value {
        ExpectedValue::Unsigned(value) => {
            let number = decimal_u64(value, path, "Expected value")?;
            unsigned_type(kind)
                && (kind == FieldType::U64 || number < (1u64 << (width(kind).unwrap_or(8) * 8)))
        }
        ExpectedValue::Signed(value) => {
            let number = decimal_i64(value, path, "Expected value")?;
            let bits = width(kind).unwrap_or(8) * 8;
            signed_type(kind)
                && (bits == 64
                    || (number >= -(1i64 << (bits - 1)) && number < (1i64 << (bits - 1))))
        }
        ExpectedValue::Float(value) => {
            matches!(kind, FieldType::F32 | FieldType::F64)
                && value.parse::<f64>().is_ok_and(|value| value.is_finite())
        }
        ExpectedValue::Bool(_) => kind == FieldType::Bool,
        ExpectedValue::String(value) => {
            kind == FieldType::String && value.len() as u64 <= MAX_READ_RANGE
        }
        ExpectedValue::Bytes(value) => {
            kind == FieldType::Bytes && hex_value(value, path)?.len() as u64 <= MAX_READ_RANGE
        }
    };
    if !compatible {
        return Err(error(
            path,
            ErrorCode::InvalidTemplate,
            "Expected value must match the field type and width.",
        ));
    }
    Ok(())
}

fn expected_text(value: &ExpectedValue) -> String {
    match value {
        ExpectedValue::Unsigned(value)
        | ExpectedValue::Signed(value)
        | ExpectedValue::Float(value)
        | ExpectedValue::String(value)
        | ExpectedValue::Bytes(value) => value.clone(),
        ExpectedValue::Bool(value) => value.to_string(),
    }
}

fn validate_expectation(
    expectation: &Expectation,
    kind: FieldType,
    path: &str,
) -> Result<(), AppError> {
    match expectation {
        Expectation::Equals { value } => validate_expected_value(value, kind, path),
        Expectation::OneOf { values } => {
            if values.is_empty() || values.len() > 256 {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "oneOf requires 1..=256 typed values.",
                ));
            }
            for value in values {
                validate_expected_value(value, kind, path)?;
            }
            Ok(())
        }
        Expectation::Range { min, max } => {
            if !integer_type(kind) && !matches!(kind, FieldType::F32 | FieldType::F64) {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Expected ranges require numeric fields.",
                ));
            }
            validate_expected_value(min, kind, path)?;
            validate_expected_value(max, kind, path)?;
            if compare_expected(&expected_text(min), max, path)?
                == Some(std::cmp::Ordering::Greater)
            {
                return Err(error(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Expected range minimum exceeds maximum.",
                ));
            }
            Ok(())
        }
    }
}

fn check_expectation(
    expectation: &Expectation,
    actual: &str,
    path: &str,
) -> Result<(), ParseFailure> {
    use std::cmp::Ordering;
    let matches = match expectation {
        Expectation::Equals { value } => {
            compare_expected(actual, value, path).map_err(ParseFailure::fatal)?
                == Some(Ordering::Equal)
        }
        Expectation::OneOf { values } => {
            let mut found = false;
            for value in values {
                found |= compare_expected(actual, value, path).map_err(ParseFailure::fatal)?
                    == Some(Ordering::Equal);
            }
            found
        }
        Expectation::Range { min, max } => {
            matches!(
                compare_expected(actual, min, path).map_err(ParseFailure::fatal)?,
                Some(Ordering::Equal | Ordering::Greater)
            ) && matches!(
                compare_expected(actual, max, path).map_err(ParseFailure::fatal)?,
                Some(Ordering::Equal | Ordering::Less)
            )
        }
    };
    if matches {
        Ok(())
    } else {
        fn short(value: &str) -> String {
            let mut chars = value.chars();
            let mut result: String = chars.by_ref().take(80).collect();
            if chars.next().is_some() {
                result.push('…');
            }
            result
        }
        let expected = match expectation {
            Expectation::Equals { value } => short(&expected_text(value)),
            Expectation::OneOf { values } => format!(
                "one of [{}{}]",
                values
                    .iter()
                    .take(8)
                    .map(|value| short(&expected_text(value)))
                    .collect::<Vec<_>>()
                    .join(", "),
                if values.len() > 8 { ", …" } else { "" }
            ),
            Expectation::Range { min, max } => format!(
                "{}..={}",
                short(&expected_text(min)),
                short(&expected_text(max))
            ),
        };
        Err(ParseFailure::field(
            path,
            ErrorCode::TemplateExpectationFailed,
            format!("Expected {expected}; decoded {}.", short(actual)),
        ))
    }
}

#[derive(Clone, Copy)]
struct AttemptedRange {
    offset: u64,
    length: Option<u64>,
}

enum ParseFailure {
    Field(AppError, Option<AttemptedRange>),
    Fatal(AppError),
}

/// Walk result-path scopes from the innermost instantiated structure outward.
/// An array index is a scope component too: `records[1].payload` can resolve
/// `length` against `records[1].length`, not against the first record.
fn parent_scope(path: &str) -> Option<&str> {
    if path.ends_with(']') {
        return path.rfind('[').map(|index| &path[..index]);
    }
    path.rfind('.').map(|index| &path[..index])
}

impl ParseFailure {
    fn field(path: &str, code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Field(error(path, code, message), None)
    }
    fn fatal(value: AppError) -> Self {
        Self::Fatal(value)
    }
    fn at(self, start: u64, length: u64) -> Self {
        match self {
            Self::Field(issue, _) => Self::Field(
                issue,
                Some(AttemptedRange {
                    offset: start,
                    length: Some(length),
                }),
            ),
            other => other,
        }
    }
}

struct Interpreter<'a> {
    session: &'a mut FileSession,
    refs: HashMap<String, ReferenceValue>,
    decoded: u64,
    nodes: u64,
    cache_start: u64,
    cache: Vec<u8>,
    last_progress: u64,
}

impl Interpreter<'_> {
    fn unsigned_reference(&self, reference: &str, path: &str) -> Result<u64, ParseFailure> {
        match self.resolve_reference(reference, path)? {
            ReferenceValue::Unsigned(value) => Ok(*value),
            _ => Err(ParseFailure::field(
                path,
                ErrorCode::InvalidTemplate,
                "Reference must identify an earlier unsigned integer.",
            )),
        }
    }

    fn length(&self, spec: &LengthSpec, path: &str) -> Result<u64, ParseFailure> {
        match spec {
            LengthSpec::Fixed(value) => Ok(*value),
            LengthSpec::Reference(value) => {
                let length = self.unsigned_reference(&value.reference, path)?;
                if length > value.max {
                    return Err(ParseFailure::field(
                        path,
                        ErrorCode::TemplateOutOfBounds,
                        format!(
                            "Decoded length {length} exceeds its declared maximum {}.",
                            value.max
                        ),
                    ));
                }
                Ok(length)
            }
        }
    }

    fn offset(&self, spec: &OffsetSpec, path: &str) -> Result<u64, ParseFailure> {
        match spec {
            OffsetSpec::Literal(value) => {
                decimal_u64(value, path, "Placement offset").map_err(ParseFailure::fatal)
            }
            OffsetSpec::Reference(value) => self
                .unsigned_reference(&value.reference, path)?
                .checked_add(
                    value
                        .add
                        .as_deref()
                        .map(|value| decimal_u64(value, path, "Offset addition"))
                        .transpose()
                        .map_err(ParseFailure::fatal)?
                        .unwrap_or(0),
                )
                .ok_or_else(|| {
                    ParseFailure::field(
                        path,
                        ErrorCode::TemplateOutOfBounds,
                        "Referenced offset addition overflows u64.",
                    )
                }),
        }
    }
    fn reserve_node(&mut self, path: &str, field: &TemplateField) -> Result<(), ParseFailure> {
        self.nodes = checked_node_add(self.nodes, 1, path).map_err(ParseFailure::fatal)?;
        self.decoded = checked_budget_add(
            self.decoded,
            metadata_cost(field, path).map_err(ParseFailure::fatal)?,
            path,
        )
        .map_err(ParseFailure::fatal)?;
        Ok(())
    }

    fn reserve_bytes(&mut self, size: u64, path: &str) -> Result<(), ParseFailure> {
        let worst = size.checked_mul(4).ok_or_else(|| {
            ParseFailure::fatal(error(
                path,
                ErrorCode::InvalidTemplate,
                "Decoded data overflows.",
            ))
        })?;
        self.decoded =
            checked_budget_add(self.decoded, worst, path).map_err(ParseFailure::fatal)?;
        Ok(())
    }

    fn read(&mut self, start: u64, length: u64, path: &str) -> Result<Vec<u8>, ParseFailure> {
        let end = start.checked_add(length).ok_or_else(|| {
            ParseFailure::field(
                path,
                ErrorCode::TemplateOutOfBounds,
                "Byte range overflows u64.",
            )
        })?;
        if end > self.session.info().size {
            return Err(ParseFailure::field(
                path,
                ErrorCode::TemplateOutOfBounds,
                format!("Requested {length} bytes at offset {start}; {} available. Byte range extends beyond the file.", self.session.info().size.saturating_sub(start)),
            ));
        }
        self.reserve_bytes(length, path)?;
        if length == 0 {
            return Ok(Vec::new());
        }
        if length <= PARSER_CACHE_BYTES as u64 {
            let cached_end = self.cache_start + self.cache.len() as u64;
            if start < self.cache_start || end > cached_end {
                self.cache_start = start;
                self.cache.resize(
                    (self.session.info().size - start).min(PARSER_CACHE_BYTES as u64) as usize,
                    0,
                );
                let read = self
                    .session
                    .read_effective_chunk(start, &mut self.cache)
                    .map_err(ParseFailure::fatal)?;
                self.cache.truncate(read);
            }
            let from = (start - self.cache_start) as usize;
            return Ok(self.cache[from..from + length as usize].to_vec());
        }
        let mut bytes = vec![0; length as usize];
        let read = self
            .session
            .read_effective_chunk(start, &mut bytes)
            .map_err(ParseFailure::fatal)?;
        if read != bytes.len() {
            return Err(ParseFailure::field(
                path,
                ErrorCode::TemplateOutOfBounds,
                "The source was truncated.",
            ));
        }
        Ok(bytes)
    }

    fn resolve_reference(
        &self,
        reference: &str,
        path: &str,
    ) -> Result<&ReferenceValue, ParseFailure> {
        let mut scope = parent_scope(path);
        while let Some(prefix) = scope {
            if let Some(value) = self.refs.get(&format!("{prefix}.{reference}")) {
                return Ok(value);
            }
            scope = parent_scope(prefix);
        }
        self.refs.get(reference).ok_or_else(|| {
            ParseFailure::field(
                path,
                ErrorCode::InvalidTemplate,
                format!("Reference '{reference}' is missing or not earlier in parse order."),
            )
        })
    }

    fn evaluate_condition(&self, condition: &Condition, path: &str) -> Result<bool, ParseFailure> {
        use std::cmp::Ordering;
        let actual = self.resolve_reference(&condition.reference, path)?;
        let order = match (actual, &condition.value) {
            (ReferenceValue::Unsigned(left), ConditionValue::Unsigned(right)) => {
                left.cmp(&decimal_u64(right, path, "Condition value").map_err(ParseFailure::fatal)?)
            }
            (ReferenceValue::Signed(left), ConditionValue::Signed(right)) => {
                left.cmp(&decimal_i64(right, path, "Condition value").map_err(ParseFailure::fatal)?)
            }
            (ReferenceValue::Bool(left), ConditionValue::Bool(right)) => left.cmp(right),
            (ReferenceValue::Text(left), ConditionValue::String(right)) => left.cmp(right),
            _ => {
                return Err(ParseFailure::field(
                    path,
                    ErrorCode::InvalidTemplate,
                    "Condition value type does not match its reference.",
                ))
            }
        };
        Ok(match condition.op {
            Comparison::Eq => order == Ordering::Equal,
            Comparison::Ne => order != Ordering::Equal,
            Comparison::Lt => order == Ordering::Less,
            Comparison::Lte => order != Ordering::Greater,
            Comparison::Gt => order == Ordering::Greater,
            Comparison::Gte => order != Ordering::Less,
        })
    }

    fn start(
        &self,
        field: &TemplateField,
        base: u64,
        cursor: u64,
        path: &str,
    ) -> Result<u64, ParseFailure> {
        let placed = match field
            .placement
            .as_ref()
            .map(|value| value.mode)
            .unwrap_or(PlacementMode::Sequential)
        {
            PlacementMode::Sequential => cursor,
            PlacementMode::Absolute => self.offset(
                field.placement.as_ref().unwrap().offset.as_ref().unwrap(),
                path,
            )?,
            PlacementMode::Relative => base
                .checked_add(self.offset(
                    field.placement.as_ref().unwrap().offset.as_ref().unwrap(),
                    path,
                )?)
                .ok_or_else(|| {
                    ParseFailure::field(
                        path,
                        ErrorCode::TemplateOutOfBounds,
                        "Relative offset overflows u64.",
                    )
                })?,
        };
        if let Some(align) = field.align {
            let mask = align - 1;
            placed
                .checked_add(mask)
                .map(|value| value & !mask)
                .ok_or_else(|| {
                    ParseFailure::field(
                        path,
                        ErrorCode::TemplateOutOfBounds,
                        "Alignment overflows u64.",
                    )
                })
        } else {
            Ok(placed)
        }
    }

    fn fields(
        &mut self,
        fields: &[TemplateField],
        scope: &str,
        base: u64,
        mut cursor: u64,
        endian: Endian,
        progress: &mut dyn FnMut(u64),
        root: bool,
    ) -> Result<(Vec<ParsedNode>, u64), AppError> {
        let mut nodes = Vec::with_capacity(fields.len());
        for field in fields {
            let name = field.name.as_deref().expect("validated named field");
            let path = if scope.is_empty() {
                name.to_owned()
            } else {
                format!("{scope}.{name}")
            };
            match self.one(field, &path, name, base, cursor, endian, progress) {
                Ok(Some((node, end))) => {
                    let failed = !node.diagnostics.is_empty();
                    cursor = end;
                    nodes.push(node);
                    if root {
                        self.report_progress(progress, fields.len() <= 128);
                    }
                    if failed {
                        break;
                    }
                }
                Ok(None) => {}
                Err(ParseFailure::Field(issue, range)) => {
                    nodes.push(ParsedNode {
                        kind: "error".into(),
                        name: name.into(),
                        path,
                        field_type: field.field_type,
                        offset: range.map(|value| value.offset.to_string()),
                        length: range
                            .and_then(|value| value.length)
                            .map(|value| value.to_string()),
                        value: None,
                        endianness: None,
                        comment: field.comment.clone().unwrap_or_default(),
                        enum_label: None,
                        flags: Vec::new(),
                        diagnostics: vec![Diagnostic {
                            code: issue.code().into(),
                            message: issue.message,
                        }],
                        children: Vec::new(),
                    });
                    break;
                }
                Err(ParseFailure::Fatal(issue)) => return Err(issue),
            }
        }
        Ok((nodes, cursor))
    }

    fn one(
        &mut self,
        field: &TemplateField,
        path: &str,
        name: &str,
        base: u64,
        cursor: u64,
        endian: Endian,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Option<(ParsedNode, u64)>, ParseFailure> {
        let before = self.nodes;
        let result = self
            .one_inner(field, path, name, base, cursor, endian, progress)
            .and_then(|parsed| {
                if let (Some(expectation), Some((node, _))) = (&field.expect, &parsed) {
                    check_expectation(
                        expectation,
                        node.value.as_deref().expect("validated leaf expectation"),
                        path,
                    )?;
                }
                Ok(parsed)
            });
        // Errors before placement/condition evaluation still occupy one result
        // node. Errors after a read reuse the already-reserved node, not two.
        if matches!(&result, Err(ParseFailure::Field(_, _))) && self.nodes == before {
            self.reserve_node(path, field)?;
        }
        self.report_progress(progress, false);
        result.map_err(|failure| match failure {
            ParseFailure::Field(mut issue, range) => {
                // Keep the declared/requested length even when the reference
                // exceeds its cap. This records an attempted range, never a
                // successful value or permission to perform that read.
                let size = width(field.field_type).or_else(|| match &field.length {
                    Some(LengthSpec::Fixed(value)) => Some(*value),
                    Some(LengthSpec::Reference(value)) => {
                        self.unsigned_reference(&value.reference, path).ok()
                    }
                    None => field.max_length,
                });
                let range = range.or_else(|| {
                    self.start(field, base, cursor, path)
                        .ok()
                        .map(|offset| AttemptedRange {
                            offset,
                            length: size,
                        })
                });
                if let Some(AttemptedRange {
                    offset: start,
                    length: Some(length),
                }) = range
                {
                    if !issue.message.contains("Requested ") {
                        issue.message = format!(
                            "{} (offset {start}, requested {length} bytes, {} available).",
                            issue.message,
                            self.session.info().size.saturating_sub(start).min(length)
                        );
                    }
                } else if let Some(range) = range {
                    issue.message =
                        format!("{} (attempted offset {}).", issue.message, range.offset);
                }
                ParseFailure::Field(issue, range)
            }
            other => other,
        })
    }

    // Nested fields report bounded batches, while ordinary top-level fields
    // retain their immediate progress notifications. No per-byte IPC flood.
    fn report_progress(&mut self, progress: &mut dyn FnMut(u64), force: bool) {
        if self.nodes > self.last_progress && (force || self.nodes - self.last_progress >= 128) {
            self.last_progress = self.nodes;
            progress(self.nodes);
        }
    }

    fn one_inner(
        &mut self,
        field: &TemplateField,
        path: &str,
        name: &str,
        base: u64,
        cursor: u64,
        inherited_endian: Endian,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Option<(ParsedNode, u64)>, ParseFailure> {
        if let Some(condition) = &field.condition {
            if !self.evaluate_condition(condition, path)? {
                return Ok(None);
            }
        }
        let start = self.start(field, base, cursor, path)?;
        let endian = field.endianness.unwrap_or(inherited_endian);
        self.reserve_node(path, field)?;
        let mut node = ParsedNode {
            kind: "leaf".into(),
            name: name.into(),
            path: path.into(),
            field_type: field.field_type,
            offset: Some(start.to_string()),
            length: None,
            value: None,
            endianness: Some(endian),
            comment: field.comment.clone().unwrap_or_default(),
            enum_label: None,
            flags: Vec::new(),
            diagnostics: Vec::new(),
            children: Vec::new(),
        };
        let end = if let Some(size) = width(field.field_type) {
            let bytes = self.read(start, size, path)?;
            let (value, reference) = decode_scalar(field, &bytes, endian, path)?;
            if let Some(reference) = reference {
                self.refs.insert(path.into(), reference);
            }
            if let Some(labels) = &field.enum_labels {
                node.enum_label = labels.get(&value).cloned();
            }
            if let Some(flags) = &field.bit_flags {
                let bits = value.parse::<u64>().expect("validated unsigned scalar");
                node.flags = flags
                    .iter()
                    .filter(|flag| bits & (1u64 << flag.bit) != 0)
                    .map(|flag| flag.name.clone())
                    .collect();
            }
            node.value = Some(value);
            start.checked_add(size).ok_or_else(|| {
                ParseFailure::field(
                    path,
                    ErrorCode::TemplateOutOfBounds,
                    "Byte range overflows u64.",
                )
            })?
        } else {
            match field.field_type {
                FieldType::Bytes => {
                    let size =
                        self.length(field.length.as_ref().expect("validated length"), path)?;
                    let bytes = self.read(start, size, path)?;
                    let mut output = String::with_capacity(bytes.len().saturating_mul(3));
                    for (index, byte) in bytes.iter().enumerate() {
                        if index > 0 {
                            output.push(' ');
                        }
                        write!(output, "{byte:02X}").expect("string formatting cannot fail");
                    }
                    node.value = Some(output);
                    start.checked_add(size).ok_or_else(|| {
                        ParseFailure::field(
                            path,
                            ErrorCode::TemplateOutOfBounds,
                            "Byte range overflows u64.",
                        )
                    })?
                }
                FieldType::String => {
                    let (bytes, consumed) = if let Some(spec) = &field.length {
                        let size = self.length(spec, path)?;
                        (self.read(start, size, path)?, size)
                    } else {
                        let maximum = field.max_length.expect("validated bound");
                        let remaining =
                            self.session.info().size.checked_sub(start).ok_or_else(|| {
                                ParseFailure::field(
                                    path,
                                    ErrorCode::TemplateOutOfBounds,
                                    "Text starts beyond the file.",
                                )
                            })?;
                        let bytes = self.read(start, maximum.min(remaining), path)?;
                        let unit = if matches!(
                            field.encoding,
                            Some(TextEncoding::Utf16le | TextEncoding::Utf16be)
                        ) {
                            2
                        } else {
                            1
                        };
                        let position = bytes
                            .chunks_exact(unit)
                            .position(|part| part.iter().all(|byte| *byte == 0))
                            .ok_or_else(|| {
                                ParseFailure::field(
                                    path,
                                    ErrorCode::TemplateOutOfBounds,
                                    "A bounded null terminator was not found.",
                                )
                            })?;
                        let prefix = position * unit;
                        (bytes[..prefix].to_vec(), (prefix + unit) as u64)
                    };
                    let value =
                        decode_text(&bytes, field.encoding.expect("validated encoding"), path)
                            .map_err(|failure| failure.at(start, consumed))?;
                    self.refs
                        .insert(path.into(), ReferenceValue::Text(value.clone()));
                    node.value = Some(value);
                    start.checked_add(consumed).ok_or_else(|| {
                        ParseFailure::field(
                            path,
                            ErrorCode::TemplateOutOfBounds,
                            "Text range overflows u64.",
                        )
                    })?
                }
                FieldType::Struct => {
                    node.kind = "struct".into();
                    let (children, _) = self
                        .fields(
                            field.fields.as_deref().expect("validated struct"),
                            path,
                            start,
                            start,
                            endian,
                            progress,
                            false,
                        )
                        .map_err(ParseFailure::fatal)?;
                    let end = child_end(start, &children)?;
                    if has_diagnostics(&children) {
                        node.diagnostics.push(Diagnostic {
                            code: "child_error".into(),
                            message: format!(
                                "{path}: One or more child fields could not be parsed."
                            ),
                        });
                    }
                    node.children = children;
                    end
                }
                FieldType::Array => {
                    node.kind = "array".into();
                    let count =
                        self.array_count(field.count.as_ref().expect("validated count"), path)?;
                    if count > MAX_TEMPLATE_EXPANDED_NODES {
                        return Err(ParseFailure::fatal(error(
                            path,
                            ErrorCode::InvalidTemplate,
                            "The expanded-result limit is 10000 nodes.",
                        )));
                    }
                    let element = field.element.as_ref().expect("validated element");
                    let mut item_cursor = start;
                    for index in 0..count {
                        let item_path = format!("{path}[{index}]");
                        let item_name = format!("[{index}]");
                        match self.one(
                            element,
                            &item_path,
                            &item_name,
                            item_cursor,
                            item_cursor,
                            endian,
                            progress,
                        ) {
                            Ok(Some((item, end))) => {
                                let failed = has_diagnostics(std::slice::from_ref(&item));
                                if end <= item_cursor && index + 1 < count {
                                    return Err(ParseFailure::field(
                                        &item_path,
                                        ErrorCode::InvalidTemplate,
                                        "Array element has zero width.",
                                    ));
                                }
                                item_cursor = end;
                                node.children.push(item);
                                if failed {
                                    break;
                                }
                            }
                            Ok(None) => unreachable!("array elements are unconditional"),
                            Err(ParseFailure::Field(issue, range)) => {
                                node.children.push(ParsedNode {
                                    kind: "error".into(),
                                    name: item_name,
                                    path: item_path,
                                    field_type: element.field_type,
                                    offset: range.map(|value| value.offset.to_string()),
                                    length: range
                                        .and_then(|value| value.length)
                                        .map(|value| value.to_string()),
                                    value: None,
                                    endianness: None,
                                    comment: element.comment.clone().unwrap_or_default(),
                                    enum_label: None,
                                    flags: Vec::new(),
                                    diagnostics: vec![Diagnostic {
                                        code: issue.code().into(),
                                        message: issue.message,
                                    }],
                                    children: Vec::new(),
                                });
                                break;
                            }
                            Err(ParseFailure::Fatal(issue)) => {
                                return Err(ParseFailure::Fatal(issue))
                            }
                        }
                    }
                    if has_diagnostics(&node.children) {
                        node.diagnostics.push(Diagnostic {
                            code: "child_error".into(),
                            message: format!(
                                "{path}: One or more array elements could not be parsed."
                            ),
                        });
                    }
                    item_cursor
                }
                _ => unreachable!("fixed-width variants handled above"),
            }
        };
        node.length = Some((end - start).to_string());
        Ok(Some((node, end)))
    }

    fn array_count(&self, count: &CountSpec, path: &str) -> Result<u64, ParseFailure> {
        if let Some(fixed) = count.fixed {
            return Ok(fixed);
        }
        let reference = count.reference.as_deref().expect("validated count ref");
        match self.resolve_reference(reference, path)? {
            ReferenceValue::Unsigned(value) => Ok(*value),
            _ => Err(ParseFailure::field(
                path,
                ErrorCode::InvalidTemplate,
                "Array count ref must identify an earlier unsigned integer.",
            )),
        }
    }
}

fn child_end(start: u64, children: &[ParsedNode]) -> Result<u64, ParseFailure> {
    let mut end = start;
    for child in children {
        if let (Some(offset), Some(length)) = (&child.offset, &child.length) {
            let candidate = offset
                .parse::<u64>()
                .unwrap()
                .checked_add(length.parse::<u64>().unwrap())
                .ok_or_else(|| {
                    ParseFailure::field(
                        &child.path,
                        ErrorCode::TemplateOutOfBounds,
                        "Child range overflows u64.",
                    )
                })?;
            end = end.max(candidate);
        }
    }
    Ok(end)
}

fn has_diagnostics(nodes: &[ParsedNode]) -> bool {
    nodes
        .iter()
        .any(|node| !node.diagnostics.is_empty() || has_diagnostics(&node.children))
}

fn decode_text(bytes: &[u8], encoding: TextEncoding, path: &str) -> Result<String, ParseFailure> {
    let output = match encoding {
        TextEncoding::Ascii => {
            if !bytes.is_ascii() {
                return Err(ParseFailure::field(
                    path,
                    ErrorCode::TemplateDataInvalid,
                    "ASCII text contains a non-ASCII byte.",
                ));
            }
            String::from_utf8(bytes.to_vec()).expect("ASCII is UTF-8")
        }
        TextEncoding::Utf8 => String::from_utf8(bytes.to_vec()).map_err(|_| {
            ParseFailure::field(
                path,
                ErrorCode::TemplateDataInvalid,
                "Text is not valid UTF-8.",
            )
        })?,
        TextEncoding::Utf16le | TextEncoding::Utf16be => {
            if bytes.len() % 2 != 0 {
                return Err(ParseFailure::field(
                    path,
                    ErrorCode::TemplateDataInvalid,
                    "UTF-16 text has an odd byte length.",
                ));
            }
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| match encoding {
                    TextEncoding::Utf16le => u16::from_le_bytes([pair[0], pair[1]]),
                    _ => u16::from_be_bytes([pair[0], pair[1]]),
                })
                .collect();
            String::from_utf16(&units).map_err(|_| {
                ParseFailure::field(
                    path,
                    ErrorCode::TemplateDataInvalid,
                    "Text is not valid UTF-16.",
                )
            })?
        }
    };
    Ok(output.trim_end_matches('\0').to_owned())
}

fn decode_scalar(
    field: &TemplateField,
    bytes: &[u8],
    endian: Endian,
    path: &str,
) -> Result<(String, Option<ReferenceValue>), ParseFailure> {
    let read_unsigned = || {
        let mut padded = [0u8; 8];
        if endian == Endian::Little {
            padded[..bytes.len()].copy_from_slice(bytes);
            u64::from_le_bytes(padded)
        } else {
            padded[8 - bytes.len()..].copy_from_slice(bytes);
            u64::from_be_bytes(padded)
        }
    };
    let result = match field.field_type {
        FieldType::U8 | FieldType::U16 | FieldType::U32 | FieldType::U64 => {
            let value = read_unsigned();
            (value.to_string(), Some(ReferenceValue::Unsigned(value)))
        }
        FieldType::I8 | FieldType::I16 | FieldType::I32 | FieldType::I64 => {
            let unsigned = read_unsigned();
            let bits = (bytes.len() * 8) as u32;
            let value = if bits == 64 {
                unsigned as i64
            } else {
                ((unsigned << (64 - bits)) as i64) >> (64 - bits)
            };
            (value.to_string(), Some(ReferenceValue::Signed(value)))
        }
        FieldType::Bool => match bytes[0] {
            0 => ("false".into(), Some(ReferenceValue::Bool(false))),
            1 => ("true".into(), Some(ReferenceValue::Bool(true))),
            _ => {
                return Err(ParseFailure::field(
                    path,
                    ErrorCode::TemplateDataInvalid,
                    "Bool byte must be 0 or 1.",
                ))
            }
        },
        FieldType::F32 => {
            let raw: [u8; 4] = bytes.try_into().expect("validated width");
            let value = if endian == Endian::Little {
                f32::from_le_bytes(raw)
            } else {
                f32::from_be_bytes(raw)
            };
            (value.to_string(), None)
        }
        FieldType::F64 => {
            let raw: [u8; 8] = bytes.try_into().expect("validated width");
            let value = if endian == Endian::Little {
                f64::from_le_bytes(raw)
            } else {
                f64::from_be_bytes(raw)
            };
            (value.to_string(), None)
        }
        _ => unreachable!("scalar decoder called for a container"),
    };
    Ok(result)
}

pub fn parse_template(
    session: &mut FileSession,
    template: &TemplateDefinition,
) -> Result<Vec<ParsedNode>, AppError> {
    parse_template_with_progress(session, template, &mut |_| {})
}

pub fn parse_template_with_progress(
    session: &mut FileSession,
    template: &TemplateDefinition,
    progress: &mut dyn FnMut(u64),
) -> Result<Vec<ParsedNode>, AppError> {
    validate_template(template)?;
    session.ensure_source_unchanged()?;
    let mut interpreter = Interpreter {
        session,
        refs: HashMap::new(),
        decoded: 0,
        nodes: 0,
        cache_start: 0,
        cache: Vec::new(),
        last_progress: 0,
    };
    let parsed = interpreter
        .fields(
            &template.fields,
            "",
            0,
            0,
            template.default_endianness,
            progress,
            true,
        )
        .map(|(nodes, _)| nodes);
    interpreter.report_progress(progress, true);
    // Cached effective bytes are valid only for this application. Even a
    // source change during cache hits must reject the entire result.
    interpreter.session.ensure_source_unchanged()?;
    parsed
}
