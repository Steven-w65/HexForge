//! Structured editor state. Stable identities and rejected input are sidecars:
//! neither is serialized into the template or treated as a second workspace.
use super::{
    references,
    validation::{collect_validation, RawInputState},
};
use hexforge_core::template::{FieldType, TemplateDefinition, TemplateField};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u64);
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldStep {
    Child(usize),
    Element,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPath(pub Vec<FieldStep>);
impl FieldPath {
    pub fn root(index: usize) -> Self {
        Self(vec![FieldStep::Child(index)])
    }
    pub fn child(mut self, index: usize) -> Self {
        self.0.push(FieldStep::Child(index));
        self
    }
    pub fn element(mut self) -> Self {
        self.0.push(FieldStep::Element);
        self
    }
}
#[derive(Debug, Clone)]
pub enum TemplateEdit {
    Add {
        parent: Option<FieldPath>,
    },
    Rename {
        path: FieldPath,
        name: String,
    },
    Patch {
        path: FieldPath,
        property: String,
        value: Option<Value>,
    },
    Type {
        path: FieldPath,
        field_type: FieldType,
    },
    Delete {
        path: FieldPath,
    },
    Duplicate {
        path: FieldPath,
    },
    Move {
        path: FieldPath,
        delta: i32,
    },
    EnumKey {
        path: FieldPath,
        old: String,
        new: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationKind {
    Remove,
    Type,
    References,
}
#[derive(Debug, Clone)]
pub struct Confirmation {
    pub kind: ConfirmationKind,
    pub field_name: String,
}
#[derive(Debug, Clone)]
pub struct EditOutcome {
    pub definition: TemplateDefinition,
    pub affected_references: Vec<String>,
    pub confirmation: Option<Confirmation>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRejected(pub String);
impl std::fmt::Display for EditRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for EditRejected {}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub id: FieldId,
    pub data: TemplateField,
    pub children: Option<Vec<Node>>,
    pub element: Option<Box<Node>>,
}
impl Node {
    fn new(mut data: TemplateField, next: &mut u64) -> Self {
        *next = next.checked_add(1).expect("bounded template IDs");
        let id = FieldId(*next);
        let children = data
            .fields
            .take()
            .map(|fields| fields.into_iter().map(|f| Self::new(f, next)).collect());
        let element = data
            .element
            .take()
            .map(|field| Box::new(Self::new(*field, next)));
        Self {
            id,
            data,
            children,
            element,
        }
    }
    pub fn export(&self) -> TemplateField {
        let mut out = self.data.clone();
        out.fields = self
            .children
            .as_ref()
            .map(|v| v.iter().map(Self::export).collect());
        out.element = self.element.as_ref().map(|n| Box::new(n.export()));
        out
    }
    fn duplicate(&self, next: &mut u64) -> Self {
        Self::new(self.export(), next)
    }
}
#[derive(Debug, Clone)]
enum Pending {
    Edit {
        edit: TemplateEdit,
        baseline: TemplateDefinition,
    },
    References {
        nodes: Vec<Node>,
        baseline: TemplateDefinition,
        affected: Vec<String>,
    },
}
#[derive(Debug, Clone)]
pub struct EditorModel {
    pub name: String,
    pub default_endianness: hexforge_core::template::Endian,
    pub(crate) nodes: Vec<Node>,
    next_id: u64,
    pub raw: RawInputState,
    pending: Option<Pending>,
}
impl EditorModel {
    pub fn new(definition: TemplateDefinition) -> Self {
        let mut next_id = 0;
        let nodes = definition
            .fields
            .into_iter()
            .map(|f| Node::new(f, &mut next_id))
            .collect();
        let mut out = Self {
            name: definition.name,
            default_endianness: definition.default_endianness,
            nodes,
            next_id,
            raw: Default::default(),
            pending: None,
        };
        out.sync_raw();
        out
    }
    pub fn definition(&self) -> TemplateDefinition {
        TemplateDefinition {
            name: self.name.clone(),
            default_endianness: self.default_endianness,
            fields: self.nodes.iter().map(Node::export).collect(),
        }
    }
    pub fn replace(&mut self, definition: TemplateDefinition) {
        *self = Self::new(definition);
    }
    pub fn valid(&self) -> bool {
        collect_validation(&self.definition(), &self.raw).is_empty()
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }
    pub fn id(&self, path: &FieldPath) -> Option<FieldId> {
        node(&self.nodes, &path.0).map(|n| n.id)
    }
    pub fn path_for_id(&self, id: FieldId) -> Option<FieldPath> {
        entries(&self.nodes)
            .into_iter()
            .find(|e| e.node.id == id)
            .map(|e| e.locator)
    }
    pub fn field(&self, path: &FieldPath) -> Option<TemplateField> {
        node(&self.nodes, &path.0).map(Node::export)
    }
    pub fn named_entries(&self) -> Vec<(FieldId, FieldPath, String, TemplateField)> {
        entries(&self.nodes)
            .into_iter()
            .map(|e| (e.node.id, e.locator, e.path, e.node.export()))
            .collect()
    }
    pub fn edit(&mut self, edit: TemplateEdit) -> Result<EditOutcome, EditRejected> {
        self.pending = None;
        self.perform(edit, false)
    }
    pub fn confirm(&mut self) -> Result<EditOutcome, EditRejected> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| EditRejected("The pending change is no longer current.".into()))?;
        match pending {
            Pending::Edit { edit, baseline } if baseline == self.definition() => {
                self.perform(edit, true)
            }
            Pending::References {
                nodes,
                baseline,
                affected,
            } if baseline == self.definition() => {
                self.nodes = nodes;
                self.sync_raw();
                Ok(EditOutcome {
                    definition: self.definition(),
                    affected_references: affected,
                    confirmation: None,
                })
            }
            _ => Err(EditRejected(
                "The template changed. Review the change again.".into(),
            )),
        }
    }
    pub fn rejected_input(
        &mut self,
        id: FieldId,
        control: &str,
        index: Option<usize>,
        text: &str,
        message: &str,
    ) {
        self.pending = None;
        self.raw.set(id, control, index, text, message);
    }
    fn sync_raw(&mut self) {
        let paths = self
            .named_entries()
            .into_iter()
            .map(|(id, _, path, f)| (id, path, f))
            .collect::<Vec<_>>();
        self.raw.reconcile(&paths);
    }
    fn outcome(&self, affected: Vec<String>, confirmation: Option<Confirmation>) -> EditOutcome {
        EditOutcome {
            definition: self.definition(),
            affected_references: affected,
            confirmation,
        }
    }
    fn perform(&mut self, edit: TemplateEdit, approved: bool) -> Result<EditOutcome, EditRejected> {
        let baseline = self.definition();
        let mut next = self.nodes.clone();
        let path = match &edit {
            TemplateEdit::Add { .. } => None,
            TemplateEdit::Rename { path, .. }
            | TemplateEdit::Patch { path, .. }
            | TemplateEdit::Type { path, .. }
            | TemplateEdit::Delete { path }
            | TemplateEdit::Duplicate { path }
            | TemplateEdit::Move { path, .. }
            | TemplateEdit::EnumKey { path, .. } => Some(path.clone()),
        };
        if let Some(path) = &path {
            let n = node(&next, &path.0)
                .ok_or_else(|| EditRejected("The field no longer exists.".into()))?;
            let configured_type = type_configured(n);
            let kind = match &edit {
                TemplateEdit::Delete { .. } if remove_configured(n) => {
                    Some(ConfirmationKind::Remove)
                }
                TemplateEdit::Type { field_type, .. }
                    if *field_type != n.data.field_type && configured_type =>
                {
                    Some(ConfirmationKind::Type)
                }
                _ => None,
            };
            if !approved {
                if let Some(kind) = kind {
                    let field_name = n
                        .data
                        .name
                        .clone()
                        .unwrap_or_else(|| "Array element".into());
                    self.pending = Some(Pending::Edit { edit, baseline });
                    return Ok(self.outcome(vec![], Some(Confirmation { kind, field_name })));
                }
            }
        }
        match edit {
            TemplateEdit::Add { parent } => {
                let siblings = if let Some(p) = parent {
                    node_mut(&mut next, &p.0)
                        .and_then(|n| n.children.as_mut())
                        .ok_or_else(|| EditRejected("This field has no fields list.".into()))?
                } else {
                    &mut next
                };
                let mut ordinal = siblings.len() + 1;
                while siblings
                    .iter()
                    .any(|n| n.data.name.as_deref() == Some(&format!("field{ordinal}")))
                {
                    ordinal += 1;
                }
                siblings.push(Node::new(
                    new_field(FieldType::U8, Some(format!("field{ordinal}"))),
                    &mut self.next_id,
                ));
            }
            TemplateEdit::Rename { path, name } => {
                references::rename(&mut next, &path, &name)?;
            }
            TemplateEdit::Patch {
                path,
                property,
                value,
            } => {
                if ["name", "type", "fields", "element"].contains(&property.as_str()) {
                    return Err(EditRejected(
                        "Use the structural field action for this property.".into(),
                    ));
                }
                let n = node_mut(&mut next, &path.0).unwrap();
                let mut json = serde_json::to_value(n.export()).expect("template JSON");
                if let Some(value) = value {
                    json[&property] = value;
                } else {
                    json.as_object_mut().unwrap().remove(&property);
                }
                let mut replacement: TemplateField =
                    serde_json::from_value(json).map_err(|e| EditRejected(e.to_string()))?;
                replacement.fields = None;
                replacement.element = None;
                n.data = replacement;
            }
            TemplateEdit::Type { path, field_type } => {
                let n = node_mut(&mut next, &path.0).unwrap();
                if n.data.field_type == field_type {
                    return Ok(self.outcome(vec![], None));
                }
                let mut f = new_field(field_type, n.data.name.clone());
                f.placement = n.data.placement.clone();
                f.align = n.data.align;
                f.comment = n.data.comment.clone();
                f.condition = n.data.condition.clone();
                if has_endian(n.data.field_type) && has_endian(field_type) {
                    f.endianness = n.data.endianness;
                }
                let mut fresh = Node::new(f, &mut self.next_id);
                fresh.id = n.id;
                *n = fresh;
            }
            TemplateEdit::Delete { path } => {
                let (siblings, index) = siblings_mut(&mut next, &path)?;
                siblings.remove(index);
            }
            TemplateEdit::Duplicate { path } => {
                let (siblings, index) = siblings_mut(&mut next, &path)?;
                let Some(name) = &siblings[index].data.name else {
                    return Ok(self.outcome(vec![], None));
                };
                let base = format!("{name}Copy");
                let mut name = base.clone();
                let mut suffix = 2;
                while siblings.iter().any(|n| n.data.name.as_ref() == Some(&name)) {
                    name = format!("{base}{suffix}");
                    suffix += 1;
                }
                let mut copy = siblings[index].duplicate(&mut self.next_id);
                copy.data.name = Some(name);
                siblings.insert(index + 1, copy);
            }
            TemplateEdit::Move { path, delta } => {
                let (siblings, index) = siblings_mut(&mut next, &path)?;
                let Some(other) = index
                    .checked_add_signed(delta as isize)
                    .filter(|i| *i < siblings.len())
                else {
                    return Ok(self.outcome(vec![], None));
                };
                siblings.swap(index, other);
            }
            TemplateEdit::EnumKey { path, old, new } => {
                let n = node_mut(&mut next, &path.0).unwrap();
                let labels = n.data.enum_labels.get_or_insert_with(Default::default);
                if new != old && labels.contains_key(&new) {
                    return Err(EditRejected(format!(
                        "Enum value {new} already exists. Choose a unique value."
                    )));
                }
                let label = labels.remove(&old).unwrap_or_default();
                labels.insert(new, label);
            }
        }
        let affected = references::broken(&self.nodes, &next);
        if !affected.is_empty() {
            self.pending = Some(Pending::References {
                nodes: next,
                baseline,
                affected: affected.clone(),
            });
            Ok(self.outcome(
                affected,
                Some(Confirmation {
                    kind: ConfirmationKind::References,
                    field_name: String::new(),
                }),
            ))
        } else {
            self.nodes = next;
            self.sync_raw();
            Ok(self.outcome(vec![], None))
        }
    }
}
pub fn apply_edit(
    definition: &TemplateDefinition,
    edit: TemplateEdit,
) -> Result<EditOutcome, EditRejected> {
    EditorModel::new(definition.clone()).edit(edit)
}
pub fn new_field(field_type: FieldType, name: Option<String>) -> TemplateField {
    let mut value = json!({"type":field_type});
    if let Some(name) = name {
        value["name"] = json!(name);
    }
    match field_type {
        FieldType::String => {
            value["encoding"] = json!("utf8");
            value["length"] = json!(1);
        }
        FieldType::Bytes => value["length"] = json!(1),
        FieldType::Struct => value["fields"] = json!([]),
        FieldType::Array => {
            value["count"] = json!({"fixed":1});
            value["element"] = json!({"type":"u8"});
        }
        _ => {}
    }
    serde_json::from_value(value).expect("built-in field defaults")
}
pub fn has_endian(t: FieldType) -> bool {
    !matches!(t, FieldType::Bool | FieldType::Bytes | FieldType::String)
}
fn type_configured(n: &Node) -> bool {
    if n.data.field_type == FieldType::Array || n.children.as_ref().is_some_and(|v| !v.is_empty()) {
        return true;
    }
    let defaults = new_field(n.data.field_type, None);
    let f = &n.data;
    f.encoding.is_some() && f.encoding != defaults.encoding
        || f.length.is_some() && f.length != defaults.length
        || f.max_length.is_some()
        || f.enum_labels.is_some()
        || f.bit_flags.is_some()
        || f.expect.is_some()
}
fn remove_configured(n: &Node) -> bool {
    let generic = n
        .data
        .name
        .as_deref()
        .and_then(|s| s.strip_prefix("field"))
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
    type_configured(n)
        || n.data.placement.is_some()
        || n.data.align.is_some()
        || n.data.condition.is_some()
        || n.data.comment.as_ref().is_some_and(|s| !s.is_empty())
        || !generic
}
pub(crate) fn node<'a>(nodes: &'a [Node], path: &[FieldStep]) -> Option<&'a Node> {
    let (FieldStep::Child(i), rest) = path.split_first()? else {
        return None;
    };
    let n = nodes.get(*i)?;
    node_rest(n, rest)
}
fn node_rest<'a>(n: &'a Node, rest: &[FieldStep]) -> Option<&'a Node> {
    match rest.split_first() {
        None => Some(n),
        Some((FieldStep::Element, tail)) => node_rest(n.element.as_deref()?, tail),
        Some((FieldStep::Child(_), _)) => node(n.children.as_ref()?, rest),
    }
}
pub(crate) fn node_mut<'a>(nodes: &'a mut [Node], path: &[FieldStep]) -> Option<&'a mut Node> {
    let (FieldStep::Child(i), rest) = path.split_first()? else {
        return None;
    };
    let n = nodes.get_mut(*i)?;
    node_rest_mut(n, rest)
}
fn node_rest_mut<'a>(n: &'a mut Node, rest: &[FieldStep]) -> Option<&'a mut Node> {
    match rest.split_first() {
        None => Some(n),
        Some((FieldStep::Element, tail)) => node_rest_mut(n.element.as_deref_mut()?, tail),
        Some((FieldStep::Child(_), _)) => node_mut(n.children.as_mut()?, rest),
    }
}
fn siblings_mut<'a>(
    nodes: &'a mut Vec<Node>,
    path: &FieldPath,
) -> Result<(&'a mut Vec<Node>, usize), EditRejected> {
    let Some((FieldStep::Child(i), parent)) = path.0.split_last() else {
        return Err(EditRejected(
            "Array elements cannot be moved or removed.".into(),
        ));
    };
    let siblings = if parent.is_empty() {
        nodes
    } else {
        node_mut(nodes, parent)
            .and_then(|n| n.children.as_mut())
            .ok_or_else(|| EditRejected("Missing containing structure.".into()))?
    };
    if *i >= siblings.len() {
        return Err(EditRejected("The field no longer exists.".into()));
    }
    Ok((siblings, *i))
}
pub(crate) struct Entry<'a> {
    pub node: &'a Node,
    pub path: String,
    pub locator: FieldPath,
}
pub(crate) fn entries(nodes: &[Node]) -> Vec<Entry<'_>> {
    fn visit<'a>(n: &'a Node, path: String, locator: FieldPath, out: &mut Vec<Entry<'a>>) {
        out.push(Entry {
            node: n,
            path: path.clone(),
            locator: locator.clone(),
        });
        if let Some(children) = &n.children {
            for (i, child) in children.iter().enumerate() {
                visit(
                    child,
                    format!("{path}.{}", child.data.name.as_deref().unwrap_or("")),
                    locator.clone().child(i),
                    out,
                );
            }
        }
        if let Some(element) = &n.element {
            visit(element, format!("{path}[]"), locator.element(), out);
        }
    }
    let mut out = vec![];
    for (i, n) in nodes.iter().enumerate() {
        visit(
            n,
            n.data.name.clone().unwrap_or_default(),
            FieldPath::root(i),
            &mut out,
        );
    }
    out
}
