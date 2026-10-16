use hexforge_controller::template::{
    editing::{EditorModel, FieldPath, TemplateEdit},
    references::reference_candidates,
};
use serde_json::json;
fn model(fields: serde_json::Value) -> EditorModel {
    EditorModel::new(
        serde_json::from_value(json!({"name":"Test","defaultEndianness":"little","fields":fields}))
            .unwrap(),
    )
}
#[test]
fn rename_preserves_earlier_scoped_references() {
    let mut m = model(json!([
        {"name":"size","type":"u8"}, {"name":"header","type":"struct","fields":[
            {"name":"size","type":"u16"},{"name":"data","type":"bytes","length":{"ref":"size","max":32},"placement":{"mode":"absolute","offset":{"ref":"size"}},"condition":{"ref":"size","op":"gt","value":{"type":"unsigned","value":"0"}}}
        ]}, {"name":"entries","type":"array","count":{"ref":"header.size"},"element":{"type":"u8"}}, {"name":"tail","type":"bytes","length":{"ref":"size","max":32}}
    ]));
    let path = FieldPath::root(1).child(0);
    let id = m.id(&path);
    let before = m.definition();
    m.edit(TemplateEdit::Rename {
        path: path.clone(),
        name: "length".into(),
    })
    .unwrap();
    let v = serde_json::to_value(m.definition()).unwrap();
    assert_eq!(v["fields"][1]["fields"][1]["length"]["ref"], "length");
    assert_eq!(v["fields"][1]["fields"][1]["condition"]["ref"], "length");
    assert_eq!(
        v["fields"][1]["fields"][1]["placement"]["offset"]["ref"],
        "length"
    );
    assert_eq!(v["fields"][2]["count"]["ref"], "header.length");
    assert_eq!(v["fields"][3]["length"]["ref"], "size");
    assert_eq!(m.id(&path), id);
    assert_eq!(
        before.fields[1].fields.as_ref().unwrap()[0].name.as_deref(),
        Some("size")
    );
}
#[test]
fn nested_array_rename_preserves_concrete_indices() {
    let mut m = model(
        json!([{"name":"records","type":"array","count":{"fixed":3},"element":{"type":"struct","fields":[{"name":"size","type":"u8"},{"name":"data","type":"bytes","length":{"ref":"size","max":8}}]}},{"name":"tail","type":"bytes","length":{"ref":"records[2].size","max":8}}]),
    );
    m.edit(TemplateEdit::Rename {
        path: FieldPath::root(0).element().child(0),
        name: "length".into(),
    })
    .unwrap();
    let changed = serde_json::to_value(m.definition()).unwrap();
    assert_eq!(
        changed["fields"][0]["element"]["fields"][1]["length"],
        json!({"ref":"length","max":8})
    );
    assert_eq!(
        changed["fields"][1]["length"],
        json!({"ref":"records[2].length","max":8})
    );
    m.edit(TemplateEdit::Rename {
        path: FieldPath::root(0),
        name: "items".into(),
    })
    .unwrap();
    assert_eq!(
        serde_json::to_value(m.definition()).unwrap()["fields"][1]["length"]["ref"],
        "items[2].length"
    );
}
#[test]
fn suggestions_exclude_later_and_unrelated_array_scopes() {
    let m = model(
        json!([{"name":"count","type":"u8"},{"name":"title","type":"string","encoding":"utf8","length":4},{"name":"records","type":"array","count":{"ref":"count"},"element":{"type":"struct","fields":[{"name":"size","type":"u16"},{"name":"data","type":"bytes","length":{"ref":"size","max":16}}]}},{"name":"later","type":"u32"}]),
    );
    let refs = |target: &str, kind: &str| {
        reference_candidates(&m.definition(), target, kind)
            .into_iter()
            .map(|v| v.reference)
            .collect::<Vec<_>>()
    };
    assert_eq!(refs("records[].data", "unsigned"), vec!["size", "count"]);
    assert_eq!(refs("records", "unsigned"), vec!["count"]);
    assert_eq!(refs("later", "unsigned"), vec!["count"]);
    assert_eq!(refs("records", "string"), vec!["title"]);
    assert!(refs("count", "unsigned").is_empty());
}
#[test]
fn invalid_rename_preserves_the_entire_current_template() {
    let mut m = model(json!([{"name":"size","type":"u8"},{"name":"other","type":"u8"}]));
    let before = m.definition();
    for name in ["", "a.b", "other"] {
        assert!(m
            .edit(TemplateEdit::Rename {
                path: FieldPath::root(0),
                name: name.into()
            })
            .is_err());
        assert_eq!(m.definition(), before);
    }
}

#[test]
fn rename_uses_qualified_fallback_instead_of_retargeting_a_shadowed_binding() {
    let mut m = model(
        json!([{"name":"size","type":"u8"},{"name":"nested","type":"struct","fields":[{"name":"length","type":"u16"},{"name":"data","type":"bytes","length":{"ref":"size","max":8}}]}]),
    );
    let before = m.definition();
    assert!(m
        .edit(TemplateEdit::Rename {
            path: FieldPath::root(0),
            name: "length".into()
        })
        .is_err());
    assert_eq!(m.definition(), before);
    // There is no syntax for escaping the nearest root-name shadow. Refuse
    // the rename atomically when even the fully qualified root stays shadowed.
    let mut m = model(
        json!([{"name":"header","type":"struct","fields":[{"name":"size","type":"u8"},{"name":"nested","type":"struct","fields":[{"name":"length","type":"u16"},{"name":"data","type":"bytes","length":{"ref":"size","max":8}}]}]}]),
    );
    m.edit(TemplateEdit::Rename {
        path: FieldPath::root(0).child(0),
        name: "length".into(),
    })
    .unwrap();
    assert_eq!(
        serde_json::to_value(m.definition()).unwrap()["fields"][0]["fields"][1]["fields"][1]
            ["length"]["ref"],
        "header.length"
    );
}
