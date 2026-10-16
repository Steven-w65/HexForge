use hexforge_controller::template::editing::{
    ConfirmationKind, EditorModel, FieldPath, TemplateEdit,
};
use hexforge_core::template::{FieldType, TemplateDefinition};
use serde_json::json;
fn model(fields: serde_json::Value) -> EditorModel {
    EditorModel::new(
        serde_json::from_value(json!({"name":"Test","defaultEndianness":"little","fields":fields}))
            .unwrap(),
    )
}
fn p(index: usize) -> FieldPath {
    FieldPath::root(index)
}

#[test]
fn all_schema_controls_round_trip() {
    let fields = json!([
        {"name":"size","type":"u64"},
        {"name":"record","type":"struct","placement":{"mode":"absolute","offset":{"ref":"size","add":"2"}},"align":8,"endianness":"big","comment":"record","fields":[
            {"name":"text","type":"string","encoding":"utf16le","length":{"ref":"size","max":256},"condition":{"ref":"size","op":"gt","value":{"type":"unsigned","value":"0"}},"expect":{"kind":"equals","value":{"type":"string","value":"Hello"}}},
            {"name":"terminated","type":"string","encoding":"utf16be","maxLength":32},
            {"name":"codes","type":"array","count":{"fixed":2},"element":{"type":"u16","enumLabels":{"1":"one"},"bitFlags":[{"bit":0,"name":"active"}],"expect":{"kind":"oneOf","values":[{"type":"unsigned","value":"1"}]}}}
        ]}
    ]);
    let mut m = model(fields.clone());
    m.edit(TemplateEdit::Patch {
        path: p(1),
        property: "comment".into(),
        value: Some(json!("edited")),
    })
    .unwrap();
    let out = serde_json::to_value(m.definition()).unwrap();
    assert!(out.get("version").is_none());
    assert_eq!(out["fields"][1]["fields"], fields[1]["fields"]);
    let decoded: TemplateDefinition = serde_json::from_value(out).unwrap();
    assert_eq!(decoded, m.definition());
}
#[test]
fn duplicate_is_deep_and_unique_in_scope() {
    let mut m = model(
        json!([{"name":"record","type":"struct","fields":[{"name":"id","type":"u64"}]},{"name":"recordCopy","type":"u8"}]),
    );
    let id = m.id(&p(0)).unwrap();
    m.edit(TemplateEdit::Duplicate { path: p(0) }).unwrap();
    assert_eq!(
        m.definition()
            .fields
            .iter()
            .map(|f| f.name.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["record", "recordCopy2", "recordCopy"]
    );
    assert_eq!(
        serde_json::to_value(&m.definition().fields[1]).unwrap(),
        json!({"name":"recordCopy2","type":"struct","fields":[{"name":"id","type":"u64"}]})
    );
    assert_eq!(
        m.definition().fields[1].name.as_deref(),
        Some("recordCopy2")
    );
    assert_ne!(m.id(&p(1)).unwrap(), id);
    m.edit(TemplateEdit::Rename {
        path: p(1).child(0),
        name: "changed".into(),
    })
    .unwrap();
    assert_eq!(
        m.definition().fields[0].fields.as_ref().unwrap()[0]
            .name
            .as_deref(),
        Some("id")
    );
}
#[test]
fn reorder_delete_type_change_respect_reference_confirmation() {
    let fields = json!([{"name":"size","type":"u8"},{"name":"data","type":"bytes","length":{"ref":"size","max":8}}]);
    let mut m = model(fields.clone());
    let out = m
        .edit(TemplateEdit::Move {
            path: p(0),
            delta: 1,
        })
        .unwrap();
    assert_eq!(out.affected_references, vec!["data"]);
    assert_eq!(out.confirmation.unwrap().kind, ConfirmationKind::References);
    assert_eq!(m.definition().fields[0].name.as_deref(), Some("size"));
    m.confirm().unwrap();
    assert_eq!(m.definition().fields[0].name.as_deref(), Some("data"));
    let mut m = model(fields);
    assert_eq!(
        m.edit(TemplateEdit::Delete { path: p(0) })
            .unwrap()
            .confirmation
            .unwrap()
            .kind,
        ConfirmationKind::Remove
    );
    assert_eq!(
        m.confirm().unwrap().confirmation.unwrap().kind,
        ConfirmationKind::References
    );
    m.cancel();
    assert_eq!(m.definition().fields.len(), 2);
    let mut m =
        model(json!([{"name":"bytes","type":"bytes","length":20,"comment":"keep","align":4}]));
    assert_eq!(
        m.edit(TemplateEdit::Type {
            path: p(0),
            field_type: FieldType::U16
        })
        .unwrap()
        .confirmation
        .unwrap()
        .kind,
        ConfirmationKind::Type
    );
    m.confirm().unwrap();
    assert_eq!(m.definition().fields[0].align, Some(4));
    assert!(m.definition().fields[0].length.is_none());
}
#[test]
fn stale_mutation_confirmation_is_rejected() {
    let mut m =
        model(json!([{"name":"configured","type":"struct","fields":[{"name":"id","type":"u8"}]}]));
    m.edit(TemplateEdit::Delete { path: p(0) }).unwrap();
    m.replace(
        serde_json::from_value(json!({"name":"New","defaultEndianness":"little","fields":[]}))
            .unwrap(),
    );
    assert!(m.confirm().is_err());
    assert_eq!(m.definition().name, "New");
}
#[test]
fn duplicate_enum_key_keeps_previous_value() {
    let mut m = model(json!([{"name":"code","type":"u8","enumLabels":{"0":"Zero","1":"One"}}]));
    let before = m.definition();
    assert!(m
        .edit(TemplateEdit::EnumKey {
            path: p(0),
            old: "0".into(),
            new: "1".into()
        })
        .is_err());
    assert_eq!(m.definition(), before);
}

#[test]
fn raw_buffers_are_cleared_only_when_their_controls_are_removed() {
    let mut m = model(json!([{"name":"code","type":"u8","enumLabels":{"0":"Zero"}}]));
    let id = m.id(&p(0)).unwrap();
    m.rejected_input(id, "enum-key", Some(0), "bad", "Enum key invalid.");
    m.edit(TemplateEdit::Patch {
        path: p(0),
        property: "comment".into(),
        value: Some(json!("unrelated")),
    })
    .unwrap();
    assert!(m.raw.pending());
    m.edit(TemplateEdit::Type {
        path: p(0),
        field_type: FieldType::String,
    })
    .unwrap();
    m.confirm().unwrap();
    assert!(!m.raw.pending());
    assert!(m.valid());
    m.rejected_input(
        id,
        "text-length",
        None,
        "1.5",
        "Text needs encoding and exactly one bounded length.",
    );
    m.edit(TemplateEdit::Delete { path: p(0) }).unwrap();
    m.confirm().unwrap();
    assert!(!m.raw.pending());
}
#[test]
fn every_field_type_is_created_with_the_reference_defaults() {
    use hexforge_controller::template::editing::new_field;
    for t in [
        FieldType::U8,
        FieldType::U16,
        FieldType::U32,
        FieldType::U64,
        FieldType::I8,
        FieldType::I16,
        FieldType::I32,
        FieldType::I64,
        FieldType::F32,
        FieldType::F64,
        FieldType::Bool,
        FieldType::String,
        FieldType::Bytes,
        FieldType::Struct,
        FieldType::Array,
    ] {
        let m = EditorModel::new(TemplateDefinition {
            name: "T".into(),
            default_endianness: hexforge_core::template::Endian::Little,
            fields: vec![new_field(t, Some("field1".into()))],
        });
        assert!(m.valid(), "{t:?}");
    }
    let mut m = model(json!([{"name":"field2","type":"u8"}]));
    m.edit(TemplateEdit::Add { parent: None }).unwrap();
    assert_eq!(m.definition().fields[1].name.as_deref(), Some("field3"));
}
