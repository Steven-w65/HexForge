//! Symbolic (unexpanded) reference bindings, matching runtime lexical lookup.
//! Concrete indices are retained when rewriting a container's named component.
use super::editing::{
    entries, node, node_mut, EditRejected, EditorModel, FieldId, FieldPath, Node,
};
use hexforge_core::template::{
    ConditionValue, FieldType, LengthSpec, OffsetSpec, TemplateDefinition, TemplateField,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Count,
    Length,
    Offset,
    Condition,
}
struct Binding {
    owner: FieldId,
    key: Key,
    reference: String,
    target: FieldId,
    full: String,
    scope: String,
}
pub fn parent(path: &str) -> Option<&str> {
    if path.ends_with(']') {
        path.rfind('[').map(|i| &path[..i])
    } else {
        path.rfind('.').map(|i| &path[..i])
    }
}
pub fn unsigned(t: FieldType) -> bool {
    matches!(
        t,
        FieldType::U8 | FieldType::U16 | FieldType::U32 | FieldType::U64
    )
}
pub fn signed(t: FieldType) -> bool {
    matches!(
        t,
        FieldType::I8 | FieldType::I16 | FieldType::I32 | FieldType::I64
    )
}
pub fn scalar(t: FieldType) -> bool {
    unsigned(t) || signed(t) || matches!(t, FieldType::Bool | FieldType::String)
}
fn refs(f: &TemplateField) -> Vec<(Key, &str)> {
    let mut out = vec![];
    if let Some(s) = f.count.as_ref().and_then(|c| c.reference.as_deref()) {
        out.push((Key::Count, s));
    }
    if let Some(LengthSpec::Reference(s)) = &f.length {
        out.push((Key::Length, &s.reference));
    }
    if let Some(OffsetSpec::Reference(s)) = f.placement.as_ref().and_then(|p| p.offset.as_ref()) {
        out.push((Key::Offset, &s.reference));
    }
    if let Some(c) = &f.condition {
        out.push((Key::Condition, &c.reference));
    }
    out
}
fn compatible(f: &TemplateField, key: Key, target: FieldType) -> bool {
    if key != Key::Condition {
        return unsigned(target);
    }
    match f.condition.as_ref().map(|c| &c.value) {
        Some(ConditionValue::Unsigned(_)) => unsigned(target),
        Some(ConditionValue::Signed(_)) => signed(target),
        Some(ConditionValue::Bool(_)) => target == FieldType::Bool,
        Some(ConditionValue::String(_)) => target == FieldType::String,
        _ => false,
    }
}
fn set_ref(f: &mut TemplateField, key: Key, reference: String) {
    match key {
        Key::Count => {
            if let Some(c) = &mut f.count {
                c.reference = Some(reference);
            }
        }
        Key::Length => {
            if let Some(LengthSpec::Reference(s)) = &mut f.length {
                s.reference = reference;
            }
        }
        Key::Offset => {
            if let Some(OffsetSpec::Reference(s)) =
                f.placement.as_mut().and_then(|p| p.offset.as_mut())
            {
                s.reference = reference;
            }
        }
        Key::Condition => {
            if let Some(c) = &mut f.condition {
                c.reference = reference;
            }
        }
    }
}
fn scopes(path: &str) -> Vec<String> {
    let mut out = vec![];
    let mut at = parent(path);
    while let Some(scope) = at {
        out.push(scope.into());
        at = parent(scope);
    }
    out.push(String::new());
    out
}
fn pattern(path: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(open) = path[at..].find('[').map(|i| i + at) {
        out.push_str(&path[at..open]);
        let Some(close) = path[open..].find(']').map(|i| i + open) else {
            out.push_str(&path[open..]);
            return out;
        };
        out.push_str("[]");
        at = close + 1;
    }
    out.push_str(&path[at..]);
    out
}
fn indexed_lookup<'a>(
    path: &str,
    all: &HashMap<String, &'a Node>,
    seen: &HashMap<String, &'a Node>,
) -> Option<&'a Node> {
    let mut at = 0;
    while let Some(open) = path[at..].find('[').map(|i| i + at) {
        let close = path[open..].find(']')? + open;
        let index = &path[open + 1..close];
        if !index.is_empty() {
            let array = all.get(&pattern(&path[..open]))?;
            if array.data.field_type != FieldType::Array {
                return None;
            }
            let index = index.parse::<u64>().ok()?;
            if array
                .data
                .count
                .as_ref()
                .and_then(|c| c.fixed)
                .is_some_and(|n| index >= n)
            {
                return None;
            }
        }
        at = close + 1;
    }
    seen.get(&pattern(path)).copied()
}
fn bindings(nodes: &[Node]) -> Vec<Binding> {
    let es = entries(nodes);
    let all = es
        .iter()
        .map(|e| (e.path.clone(), e.node))
        .collect::<HashMap<_, _>>();
    let mut seen = HashMap::new();
    let mut out = vec![];
    for e in &es {
        for (key, reference) in refs(&e.node.data) {
            for scope in scopes(&e.path) {
                let full = if scope.is_empty() {
                    reference.into()
                } else {
                    format!("{scope}.{reference}")
                };
                if let Some(target) = indexed_lookup(&full, &all, &seen) {
                    // Do not skip a nearer value merely because its type is wrong.
                    if compatible(&e.node.data, key, target.data.field_type) {
                        out.push(Binding {
                            owner: e.node.id,
                            key,
                            reference: reference.into(),
                            target: target.id,
                            full,
                            scope,
                        });
                    }
                    break;
                }
            }
        }
        if scalar(e.node.data.field_type) {
            seen.insert(e.path.clone(), e.node);
        }
    }
    out
}
fn tokens(path: &str) -> Vec<String> {
    let mut out = vec![];
    let mut start = 0;
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            if start < i {
                out.push(path[start..i].into());
            }
            i += 1;
            start = i;
        } else if bytes[i] == b'[' {
            if start < i {
                out.push(path[start..i].into());
            }
            let Some(close) = path[i..].find(']') else {
                break;
            };
            let end = i + close + 1;
            out.push(path[i..end].into());
            i = end;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < path.len() {
        out.push(path[start..].into());
    }
    out
}
fn renamed_path(path: &str, prefix: &str, name: &str) -> String {
    let mut parts = tokens(path);
    let original = tokens(prefix);
    if original.is_empty()
        || original.len() > parts.len()
        || !original
            .iter()
            .zip(&parts)
            .all(|(a, b)| a == b || a == "[]" && b.starts_with('[') && b.ends_with(']'))
    {
        return path.into();
    }
    parts[original.len() - 1] = name.into();
    let mut out = String::new();
    for part in parts {
        if !out.is_empty() && !part.starts_with('[') {
            out.push('.');
        }
        out.push_str(&part);
    }
    out
}
pub(crate) fn rename(nodes: &mut [Node], path: &FieldPath, name: &str) -> Result<(), EditRejected> {
    let es = entries(nodes);
    let id = node(nodes, &path.0)
        .ok_or_else(|| EditRejected("This field cannot be renamed.".into()))?
        .id;
    let entry = es.iter().find(|e| e.node.id == id).unwrap();
    if entry.node.data.name.is_none() {
        return Err(EditRejected("This field cannot be renamed.".into()));
    }
    if name.trim().is_empty() || name.contains(['.', '[', ']']) {
        return Err(EditRejected(
            "Use a nonempty field name without ., [ or ].".into(),
        ));
    }
    let parent_path = &path.0[..path.0.len() - 1];
    if es.iter().any(|e| {
        e.node.id != id
            && e.locator.0[..e.locator.0.len() - 1] == *parent_path
            && e.node.data.name.as_deref() == Some(name)
    }) {
        return Err(EditRejected(format!(
            "Field name '{name}' already exists in this structure."
        )));
    }
    let prefix = entry.path.clone();
    let before = bindings(nodes);
    let locators = es
        .iter()
        .map(|e| (e.node.id, e.locator.clone()))
        .collect::<HashMap<_, _>>();
    node_mut(nodes, &path.0).unwrap().data.name = Some(name.into());
    for b in &before {
        let full = renamed_path(&b.full, &prefix, name);
        let scope = renamed_path(&b.scope, &prefix, name);
        let reference = if !scope.is_empty() {
            full.strip_prefix(&format!("{scope}."))
                .unwrap_or(&full)
                .to_owned()
        } else {
            full
        };
        set_ref(
            &mut node_mut(nodes, &locators[&b.owner].0).unwrap().data,
            b.key,
            reference,
        );
    }
    for b in &before {
        if !bindings(nodes)
            .iter()
            .any(|now| now.owner == b.owner && now.key == b.key && now.target == b.target)
        {
            set_ref(
                &mut node_mut(nodes, &locators[&b.owner].0).unwrap().data,
                b.key,
                renamed_path(&b.full, &prefix, name),
            );
            if !bindings(nodes)
                .iter()
                .any(|now| now.owner == b.owner && now.key == b.key && now.target == b.target)
            {
                return Err(EditRejected(format!(
                    "Rename would change the reference used by {}.",
                    entries(nodes)
                        .iter()
                        .find(|e| e.node.id == b.owner)
                        .unwrap()
                        .path
                )));
            }
        }
    }
    Ok(())
}
pub(crate) fn broken(before: &[Node], after: &[Node]) -> Vec<String> {
    let current = entries(after);
    let now = bindings(after);
    let mut affected = vec![];
    let mut reported = HashSet::new();
    for binding in bindings(before) {
        let Some(owner) = current.iter().find(|e| e.node.id == binding.owner) else {
            continue;
        };
        if refs(&owner.node.data)
            .iter()
            .any(|(key, r)| *key == binding.key && *r == binding.reference)
            && !now.iter().any(|b| {
                b.owner == binding.owner && b.key == binding.key && b.target == binding.target
            })
            && reported.insert(owner.path.clone())
        {
            affected.push(owner.path.clone());
        }
    }
    affected
}
#[derive(Debug, Clone)]
pub struct ReferenceCandidate {
    pub reference: String,
    pub path: String,
    pub field_type: FieldType,
}
pub fn reference_candidates(
    definition: &TemplateDefinition,
    target: &str,
    literal: &str,
) -> Vec<ReferenceCandidate> {
    let model = EditorModel::new(definition.clone());
    let es = entries(&model.nodes);
    let mut seen: Vec<(String, FieldType)> = vec![];
    let mut stopped = false;
    for e in &es {
        if e.path == target {
            stopped = true;
            break;
        }
        if scalar(e.node.data.field_type) {
            seen.push((e.path.clone(), e.node.data.field_type));
        }
    }
    if !stopped {
        return vec![];
    }
    let scopes = scopes(target);
    let mut out: Vec<ReferenceCandidate> = vec![];
    for scope in &scopes {
        for (path, field_type) in &seen {
            if !match literal {
                "unsigned" => unsigned(*field_type),
                "signed" => signed(*field_type),
                "bool" => *field_type == FieldType::Bool,
                "string" => *field_type == FieldType::String,
                _ => false,
            } {
                continue;
            }
            if !scope.is_empty() && !path.starts_with(&format!("{scope}.")) {
                continue;
            }
            if path.contains("[]")
                && !scopes
                    .iter()
                    .any(|s| s.contains("[]") && path.starts_with(&format!("{s}.")))
            {
                continue;
            }
            let reference = if scope.is_empty() {
                path.clone()
            } else {
                path[scope.len() + 1..].into()
            };
            let resolves = scopes.iter().find_map(|s| {
                let full = if s.is_empty() {
                    reference.clone()
                } else {
                    format!("{s}.{reference}")
                };
                seen.iter().find(|(p, _)| *p == full).map(|(p, _)| p)
            });
            if reference.contains("[]")
                || resolves != Some(path)
                || out.iter().any(|v| v.path == *path)
            {
                continue;
            }
            out.push(ReferenceCandidate {
                reference,
                path: path.clone(),
                field_type: *field_type,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(fields: serde_json::Value) -> EditorModel {
        EditorModel::new(
            serde_json::from_value(json!({
                "name":"Test", "defaultEndianness":"little", "fields":fields
            }))
            .unwrap(),
        )
    }

    #[test]
    fn broken_references_report_only_new_binding_loss() {
        let m = model(json!([
            {"name":"size","type":"u8"},
            {"name":"data","type":"bytes","length":{"ref":"size","max":8}},
            {"name":"invalid","type":"bytes","length":{"ref":"missing","max":8}}
        ]));
        let original = m.nodes;
        assert_eq!(broken(&original, &original[1..]), ["data"]);
        let mut reordered = original.clone();
        reordered.swap(0, 1);
        assert_eq!(broken(&original, &reordered), ["data"]);
        assert!(broken(&original, &original).is_empty());
        let mut retargeted = original.clone();
        retargeted[0].data.name = Some("length".into());
        assert_eq!(broken(&original, &retargeted), ["data"]);
    }

    #[test]
    fn concrete_array_binding_loss_is_detected_without_expansion() {
        let m = model(json!([
            {"name":"items","type":"array","count":{"fixed":10000},"element":{
                "type":"struct","fields":[{"name":"count","type":"u8"}]
            }},
            {"name":"data","type":"bytes","length":{"ref":"items[9999].count","max":8}}
        ]));
        assert_eq!(broken(&m.nodes, &m.nodes[1..]), ["data"]);
    }

    #[test]
    fn safe_nested_renames_retain_original_reference_targets() {
        let mut m = model(json!([
            {"name":"records","type":"array","count":{"fixed":3},"element":{"type":"struct","fields":[
                {"name":"size","type":"u8"},
                {"name":"data","type":"bytes","length":{"ref":"size","max":8}}
            ]}}, {"name":"tail","type":"bytes","length":{"ref":"records[2].size","max":8}}
        ]));
        let before = m.nodes.clone();
        m.edit(super::super::editing::TemplateEdit::Rename {
            path: FieldPath::root(0).element().child(0),
            name: "length".into(),
        })
        .unwrap();
        m.edit(super::super::editing::TemplateEdit::Rename {
            path: FieldPath::root(0),
            name: "items".into(),
        })
        .unwrap();
        assert!(broken(&before, &m.nodes).is_empty());
    }
}
