//! All-issues immediate editor feedback, plus raw-input diagnostics. Core
//! validation remains authoritative for persistence and dynamic interpretation.
use super::{editing::FieldId, references::parent};
use hexforge_core::template::{TemplateDefinition, TemplateField};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
const MAX_READ: u64 = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
    pub field_id: Option<FieldId>,
    pub control: Option<String>,
    pub index: Option<usize>,
}
impl ValidationIssue {
    pub fn new(path: &str, message: &str) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            field_id: None,
            control: None,
            index: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RawKey {
    id: FieldId,
    control: String,
    index: Option<usize>,
}
#[derive(Debug, Clone)]
pub struct RawInput {
    pub text: String,
    pub message: String,
}
#[derive(Debug, Clone, Default)]
pub struct RawInputState {
    inputs: BTreeMap<RawKey, RawInput>,
    paths: BTreeMap<FieldId, String>,
}
impl RawInputState {
    pub fn set(
        &mut self,
        id: FieldId,
        control: &str,
        index: Option<usize>,
        text: &str,
        message: &str,
    ) {
        self.inputs.insert(
            RawKey {
                id,
                control: control.into(),
                index,
            },
            RawInput {
                text: text.into(),
                message: message.into(),
            },
        );
    }
    pub fn get(&self, id: FieldId, control: &str, index: Option<usize>) -> Option<&RawInput> {
        self.inputs.get(&RawKey {
            id,
            control: control.into(),
            index,
        })
    }
    pub fn clear(&mut self, id: FieldId, control: &str, index: Option<usize>) {
        self.inputs.remove(&RawKey {
            id,
            control: control.into(),
            index,
        });
    }
    pub fn pending(&self) -> bool {
        !self.inputs.is_empty()
    }
    pub fn reset(&mut self) {
        self.inputs.clear();
    }
    pub(crate) fn reconcile(&mut self, fields: &[(FieldId, String, TemplateField)]) {
        self.paths = fields
            .iter()
            .map(|(id, path, _)| (*id, path.clone()))
            .collect();
        self.inputs.retain(|key, _| {
            fields
                .iter()
                .find(|(id, _, _)| *id == key.id)
                .is_some_and(|(_, _, f)| control_exists(f, &key.control, key.index))
        });
    }
}
fn control_exists(f: &TemplateField, control: &str, index: Option<usize>) -> bool {
    let v = serde_json::to_value(f).unwrap();
    let t = string(&v["type"]);
    match control {
        "name" => f.name.is_some(),
        "enum-key" | "enum-label" => {
            matches!(
                t,
                "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64"
            ) && f
                .enum_labels
                .as_ref()
                .is_some_and(|m| index.unwrap_or(0) < m.len())
        }
        "flag-bit" | "flag-name" => {
            t.starts_with('u')
                && f.bit_flags
                    .as_ref()
                    .is_some_and(|m| index.unwrap_or(0) < m.len())
        }
        c if c.starts_with("expect-") => f.expect.is_some(),
        c if c.starts_with("condition-") => f.condition.is_some(),
        c if c.starts_with("count-") => t == "array",
        "length" | "length-ref" | "length-max" | "text-length" => matches!(t, "bytes" | "string"),
        "offset-ref" | "offset-add" | "placement-offset" => f
            .placement
            .as_ref()
            .is_some_and(|p| p.mode != hexforge_core::template::PlacementMode::Sequential),
        _ => true,
    }
}
fn string(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn present(v: &Value, key: &str) -> bool {
    v.get(key).is_some()
}
fn unsigned(t: &str) -> bool {
    matches!(t, "u8" | "u16" | "u32" | "u64")
}
fn signed(t: &str) -> bool {
    matches!(t, "i8" | "i16" | "i32" | "i64")
}
fn numeric(t: &str) -> bool {
    unsigned(t) || signed(t) || matches!(t, "f32" | "f64")
}
fn width(t: &str) -> u32 {
    if t.ends_with("64") {
        64
    } else if t.ends_with("32") {
        32
    } else if t.ends_with("16") {
        16
    } else {
        8
    }
}
fn decimal(s: &str, signed: bool) -> Option<i128> {
    let digits = if signed {
        s.strip_prefix('-').unwrap_or(s)
    } else {
        s
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = s.parse::<i128>().ok()?;
    if signed {
        (n >= i64::MIN as i128 && n <= i64::MAX as i128).then_some(n)
    } else {
        (n >= 0 && n <= u64::MAX as i128).then_some(n)
    }
}
fn matches_type(t: &str, literal: &str) -> bool {
    match literal {
        "unsigned" => unsigned(t),
        "signed" => signed(t),
        "bool" => t == "bool",
        "string" => t == "string",
        _ => false,
    }
}
fn integer_fits(s: &str, t: &str) -> bool {
    decimal(s, signed(t)).is_some_and(|n| {
        if unsigned(t) {
            n < (1i128 << width(t))
        } else if signed(t) {
            n >= -(1i128 << (width(t) - 1)) && n < (1i128 << (width(t) - 1))
        } else {
            false
        }
    })
}
fn expected(v: &Value, t: &str) -> bool {
    let literal = string(&v["type"]);
    let value = &v["value"];
    match literal {
        "unsigned" | "signed" => matches_type(t, literal) && integer_fits(string(value), t),
        "float" => {
            matches!(t, "f32" | "f64") && string(value).parse::<f64>().is_ok_and(|n| n.is_finite())
        }
        "bool" => t == "bool" && value.is_boolean(),
        "string" => t == "string" && value.is_string() && string(value).len() <= MAX_READ as usize,
        "bytes" => {
            t == "bytes"
                && value.is_string()
                && string(value)
                    .split_whitespace()
                    .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
                && string(value).len() <= MAX_READ as usize * 3
        }
        _ => false,
    }
}
struct Validator {
    issues: Vec<ValidationIssue>,
    seen: HashMap<String, String>,
    definitions: u64,
}
impl Validator {
    fn add(&mut self, path: &str, message: &str) {
        self.issues.push(ValidationIssue::new(path, message));
    }
    fn reference(&mut self, path: &str, reference: &str, literal: &str, label: &str) {
        if reference.trim().is_empty() {
            self.add(path, &format!("{label} needs a reference path."));
            return;
        }
        let mut scope = parent(path);
        let mut found = None;
        while let Some(at) = scope {
            if let Some(t) = self.seen.get(&format!("{at}.{reference}")) {
                found = Some(t.clone());
                break;
            }
            scope = parent(at);
        }
        found = found.or_else(|| self.seen.get(reference).cloned());
        match found {
            None if reference.contains('[') => {}
            None => self.add(
                path,
                &format!("{label} must reference an earlier field: {reference}."),
            ),
            Some(t) if !matches_type(&t, literal) => self.add(
                path,
                &format!("{label} has the wrong reference type: {reference}."),
            ),
            _ => {}
        }
    }
    fn fields(&mut self, fields: &[Value], scope: &str, depth: usize) {
        let mut names = HashSet::new();
        for f in fields {
            let name = string(&f["name"]);
            let path = if scope.is_empty() {
                name.into()
            } else {
                format!("{scope}.{name}")
            };
            if name.trim().is_empty() || name.contains(['.', '[', ']']) {
                self.add(
                    &path,
                    "Field name must be nonempty and cannot include ., [ or ].",
                );
            }
            if !names.insert(name) {
                self.add(&path, "Field names must be unique in this structure.");
            }
            self.field(f, &path, depth, false);
            let t = string(&f["type"]);
            if unsigned(t) || signed(t) || matches!(t, "bool" | "string") {
                self.seen.insert(path, t.into());
            }
        }
    }
    fn field(&mut self, f: &Value, path: &str, depth: usize, element: bool) {
        self.definitions += 1;
        if self.definitions > 4096 {
            self.add(path, "Template exceeds 4096 definitions.");
        }
        if depth > 8 {
            self.add(path, "Template exceeds 8 nesting levels.");
        }
        if element
            && ["name", "placement", "condition"]
                .iter()
                .any(|k| present(f, k))
        {
            self.add(
                path,
                "Array elements must be unnamed, sequential and unconditional.",
            );
        }
        if let Some(p) = f.get("placement") {
            if p["mode"] == "sequential" && present(p, "offset") {
                self.add(path, "Sequential placement cannot specify offset.");
            }
            if p["mode"] != "sequential" {
                if p["offset"].is_object() {
                    self.reference(path, string(&p["offset"]["ref"]), "unsigned", "Offset");
                    if present(&p["offset"], "add")
                        && decimal(string(&p["offset"]["add"]), false).is_none()
                    {
                        self.add(path, "Offset addition must be a u64 decimal string.");
                    }
                } else if decimal(string(&p["offset"]), false).is_none() {
                    self.add(
                        path,
                        "Placement offset must be a u64 decimal string or unsigned reference.",
                    );
                }
            }
        }
        if present(f, "align")
            && !f["align"]
                .as_u64()
                .is_some_and(|n| n >= 1 && n <= MAX_READ && n.is_power_of_two())
        {
            self.add(path, "Alignment must be a power of two up to 1048576.");
        }
        if let Some(c) = f.get("condition") {
            let literal = string(&c["value"]["type"]);
            self.reference(path, string(&c["ref"]), literal, "Condition");
            if matches!(literal, "bool" | "string") && !matches!(string(&c["op"]), "eq" | "ne") {
                self.add(path, "Boolean/text conditions support eq and ne only.");
            }
            if matches!(literal, "unsigned" | "signed")
                && decimal(string(&c["value"]["value"]), literal == "signed").is_none()
            {
                self.add(path, "Condition integer must fit its type.");
            }
        }
        let t = string(&f["type"]);
        if present(f, "enumLabels") && !unsigned(t) && !signed(t) {
            self.add(path, "Enum labels require an integer field.");
        }
        if present(f, "bitFlags") && !unsigned(t) {
            self.add(path, "Bit flags require an unsigned integer field.");
        }
        if let Some(flags) = f["bitFlags"].as_array() {
            let mut bits = HashSet::new();
            let mut names = HashSet::new();
            for flag in flags {
                let bit = flag["bit"].as_u64();
                let name = string(&flag["name"]);
                if bit.is_none_or(|b| b >= width(t) as u64)
                    || !bits.insert(bit)
                    || name.trim().is_empty()
                    || !names.insert(name)
                {
                    self.add(path, "Bit flags need unique in-range bits and names.");
                }
            }
        }
        if let Some(labels) = f["enumLabels"].as_object() {
            for (key, label) in labels {
                if !integer_fits(key, t) || string(label).trim().is_empty() {
                    self.add(
                        path,
                        "Enum keys must fit the integer field width and labels cannot be blank.",
                    );
                }
            }
        }
        if f["length"].is_object() {
            self.reference(path, string(&f["length"]["ref"]), "unsigned", "Length");
        }
        if let Some(check) = f.get("expect") {
            let values: Vec<&Value> = match string(&check["kind"]) {
                "equals" => vec![&check["value"]],
                "oneOf" => check["values"]
                    .as_array()
                    .map(|v| v.iter().collect())
                    .unwrap_or_default(),
                _ => vec![&check["min"], &check["max"]],
            };
            if values.is_empty() || values.len() > 256 || values.iter().any(|v| !expected(v, t)) {
                self.add(path,"Expected values must match the field type and width; oneOf needs 1 to 256 values.");
            } else if check["kind"] == "range" {
                if !numeric(t) {
                    self.add(path, "Expected ranges require numeric fields.");
                } else {
                    let a = string(&check["min"]["value"]);
                    let b = string(&check["max"]["value"]);
                    let inverted = if matches!(t, "f32" | "f64") {
                        a.parse::<f64>().unwrap() > b.parse::<f64>().unwrap()
                    } else {
                        a.parse::<i128>().unwrap() > b.parse::<i128>().unwrap()
                    };
                    if inverted {
                        self.add(path, "Expected range minimum exceeds maximum.");
                    }
                }
            }
        }
        let bound = if f["length"].is_object() {
            f["length"]["max"].as_u64()
        } else {
            f.get("length")
                .or_else(|| f.get("maxLength"))
                .and_then(Value::as_u64)
        };
        let valid_bound = bound.is_some_and(|n| n >= 1 && n <= MAX_READ);
        let has = |keys: &[&str]| keys.iter().any(|k| present(f, k));
        match t {
            _ if numeric(t) || t == "bool" => {
                if has(&[
                    "length",
                    "maxLength",
                    "encoding",
                    "fields",
                    "element",
                    "count",
                ]) {
                    self.add(
                        path,
                        "Scalar fields cannot have text/byte/container properties.",
                    );
                }
                if t == "bool" && has(&["endianness", "enumLabels", "bitFlags"]) {
                    self.add(path, "Bool cannot have endian, enum or flag properties.");
                }
            }
            "bytes" => {
                if !valid_bound
                    || !present(f, "length")
                    || has(&[
                        "endianness",
                        "maxLength",
                        "encoding",
                        "fields",
                        "element",
                        "count",
                    ])
                {
                    self.add(path, "Bytes require only a length from 1 to 1048576.");
                }
            }
            "string" => {
                if !valid_bound
                    || present(f, "length") == present(f, "maxLength")
                    || !present(f, "encoding")
                    || has(&["endianness", "fields", "element", "count"])
                {
                    self.add(path, "Text needs encoding and exactly one bounded length.");
                }
                if matches!(string(&f["encoding"]), "utf16le" | "utf16be")
                    && bound.is_some_and(|n| n % 2 != 0)
                {
                    self.add(path, "UTF-16 bounds must contain whole code units.");
                }
            }
            "struct" => {
                if !f["fields"].is_array()
                    || has(&["length", "maxLength", "encoding", "element", "count"])
                {
                    self.add(path, "Struct needs a fields list.");
                } else {
                    self.fields(f["fields"].as_array().unwrap(), path, depth + 1);
                }
            }
            "array" => {
                if !present(f, "count")
                    || !present(f, "element")
                    || has(&["length", "maxLength", "encoding", "fields"])
                {
                    self.add(path, "Array needs count and element.");
                }
                if let Some(c) = f.get("count") {
                    if present(c, "fixed") == present(c, "ref") {
                        self.add(
                            path,
                            "Array count must have exactly one fixed or ref value.",
                        );
                    }
                    if present(c, "fixed") && !c["fixed"].as_u64().is_some_and(|n| n <= 10000) {
                        self.add(path, "Fixed array count exceeds the result limit.");
                    }
                    if present(c, "ref") {
                        self.reference(path, string(&c["ref"]), "unsigned", "Array count");
                    }
                }
                if let Some(e) = f.get("element") {
                    self.field(e, &format!("{path}[]"), depth + 1, true);
                }
            }
            _ => {}
        }
    }
}
fn budget(f: &Value, depth: usize, path: &str) -> (u128, u128) {
    if depth > 8 {
        return (0, 0);
    }
    let label = f["enumLabels"]
        .as_object()
        .map(|m| {
            m.values()
                .map(|v| string(v).len() as u128)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let flags = f["bitFlags"]
        .as_array()
        .map(|v| {
            v.iter()
                .map(|v| string(&v["name"]).len() as u128)
                .sum::<u128>()
        })
        .unwrap_or(0);
    let mut nodes = 1u128;
    let mut decoded =
        128 + path.len() as u128 * 2 + string(&f["comment"]).len() as u128 + label + flags;
    let t = string(&f["type"]);
    if t == "struct" {
        if let Some(children) = f["fields"].as_array() {
            for c in children {
                let (n, d) = budget(c, depth + 1, &format!("{path}.{}", string(&c["name"])));
                nodes = nodes.saturating_add(n);
                decoded = decoded.saturating_add(d);
            }
        }
    } else if t == "array" && present(f, "element") {
        let (n, d) = budget(&f["element"], depth + 1, &format!("{path}[]"));
        let count = f["count"].get("fixed").and_then(Value::as_u64).unwrap_or(1) as u128;
        nodes = nodes.saturating_add(n.saturating_mul(count));
        decoded = decoded.saturating_add(d.saturating_mul(count));
    } else {
        let bytes = if numeric(t) || t == "bool" {
            if t == "bool" {
                1
            } else {
                width(t) as u64 / 8
            }
        } else if f["length"].is_object() {
            f["length"]["max"].as_u64().unwrap_or(0)
        } else {
            f.get("length")
                .or_else(|| f.get("maxLength"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        decoded = decoded.saturating_add(bytes as u128 * 4);
    }
    (nodes, decoded)
}
pub fn collect_validation(
    definition: &TemplateDefinition,
    raw: &RawInputState,
) -> Vec<ValidationIssue> {
    let v = serde_json::to_value(definition).expect("template JSON");
    let mut validator = Validator {
        issues: vec![],
        seen: HashMap::new(),
        definitions: 0,
    };
    if definition.name.trim().is_empty() {
        validator.add("template", "Template name must not be empty.");
    }
    validator.fields(v["fields"].as_array().unwrap(), "", 1);
    let (mut nodes, mut decoded) = (0u128, 0u128);
    for f in v["fields"].as_array().unwrap() {
        let path = string(&f["name"]);
        let (n, d) = budget(f, 1, path);
        nodes = nodes.saturating_add(n);
        decoded = decoded.saturating_add(d);
        if nodes > 10000 {
            validator.add(path, "The expanded-result limit is 10000 nodes.");
        }
        if decoded > 16777216 {
            validator.add(path, "The decoded-data limit is 16 MiB.");
        }
    }
    for (key, input) in &raw.inputs {
        if let Some(path) = raw.paths.get(&key.id) {
            validator.issues.push(ValidationIssue {
                path: path.clone(),
                message: input.message.clone(),
                field_id: Some(key.id),
                control: Some(key.control.clone()),
                index: key.index,
            });
        }
    }
    validator.issues
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPath {
    pub path: String,
    pub control: String,
    pub index: Option<usize>,
}
pub fn validation_target(issue: &ValidationIssue, definition: &TemplateDefinition) -> ControlPath {
    if let Some(control) = &issue.control {
        return ControlPath {
            path: issue.path.clone(),
            control: control.clone(),
            index: issue.index,
        };
    }
    let m = super::editing::EditorModel::new(definition.clone());
    let es = m.named_entries();
    let field = if issue.message.contains("unique in this structure") {
        es.iter().rev().find(|(_, _, p, _)| *p == issue.path)
    } else {
        es.iter().find(|(_, _, p, _)| *p == issue.path)
    };
    let f = field
        .map(|(_, _, _, f)| serde_json::to_value(f).unwrap())
        .unwrap_or(Value::Null);
    let t = string(&f["type"]);
    let text = issue.message.as_str();
    let mut index = None;
    let control = if issue.path == "template" && text == "Template name must not be empty." {
        "template-name"
    } else if text.to_lowercase().contains("field name") {
        "name"
    } else if text.contains("Alignment") {
        "align"
    } else if text.contains("Offset addition") {
        "offset-add"
    } else if text.starts_with("Offset") {
        "offset-ref"
    } else if text.contains("Placement offset") {
        "placement-offset"
    } else if text.contains("Sequential placement") {
        "placement-mode"
    } else if text.starts_with("Condition integer") {
        "condition-value"
    } else if text.contains("Boolean/text conditions") {
        "condition-op"
    } else if text.starts_with("Condition") {
        "condition-ref"
    } else if text.starts_with("Length") {
        "length-ref"
    } else if text.starts_with("Array count") {
        "count-ref"
    } else if t == "array"
        && (text.contains("Fixed array count") || text.contains("expanded-result limit"))
    {
        "count-fixed"
    } else if text.contains("Bit flags") {
        if !unsigned(t) {
            "type"
        } else {
            let mut bits = HashSet::new();
            let mut names = HashSet::new();
            let mut c = "flag-bit";
            if let Some(flags) = f["bitFlags"].as_array() {
                for (i, flag) in flags.iter().enumerate() {
                    let bit = flag["bit"].as_u64();
                    let name = string(&flag["name"]);
                    if bit.is_none_or(|b| b >= width(t) as u64) || !bits.insert(bit) {
                        index = Some(i);
                        break;
                    }
                    if name.trim().is_empty() || !names.insert(name) {
                        c = "flag-name";
                        index = Some(i);
                        break;
                    }
                }
            }
            c
        }
    } else if text.contains("Enum") {
        if !unsigned(t) && !signed(t) {
            "type"
        } else {
            let mut c = "enum-key";
            if let Some(labels) = f["enumLabels"].as_object() {
                let mut labels = labels.iter().collect::<Vec<_>>();
                labels.sort_by(|(a, _), (b, _)| enum_order(a).cmp(&enum_order(b)));
                for (i, (key, label)) in labels.into_iter().enumerate() {
                    if !integer_fits(key, t) {
                        index = Some(i);
                        break;
                    }
                    if string(label).trim().is_empty() {
                        index = Some(i);
                        c = "enum-label";
                        break;
                    }
                }
            }
            c
        }
    } else if text.contains("Expected range minimum") {
        "expect-min"
    } else if text.contains("Expected") {
        match string(&f["expect"]["kind"]) {
            "oneOf" => "expect-allowed",
            "range" => "expect-min",
            _ => "expect-value",
        }
    } else if text.contains("UTF-16")
        || text.contains("Text needs")
        || text.contains("Bytes require")
    {
        if f["length"].is_object() {
            "length-max"
        } else if t == "string" {
            "text-length"
        } else {
            "length"
        }
    } else {
        "type"
    };
    ControlPath {
        path: issue.path.clone(),
        control: control.into(),
        index,
    }
}
pub fn enum_order(key: &str) -> (u8, u32, String) {
    match key
        .parse::<u32>()
        .ok()
        .filter(|n| *n < u32::MAX && n.to_string() == key)
    {
        Some(n) => (0, n, String::new()),
        None => (1, 0, key.into()),
    }
}
