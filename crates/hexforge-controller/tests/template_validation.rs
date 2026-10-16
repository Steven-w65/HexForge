use hexforge_controller::template::{
    editing::{EditorModel, FieldPath},
    validation::{collect_validation, validation_target, RawInputState, ValidationIssue},
};
use serde_json::json;
fn model(fields: serde_json::Value) -> EditorModel {
    EditorModel::new(
        serde_json::from_value(json!({"name":"Test","defaultEndianness":"little","fields":fields}))
            .unwrap(),
    )
}
#[test]
fn invalid_raw_input_blocks_close_save() {
    let mut m = model(json!([{"name":"data","type":"bytes","length":8}]));
    let id = m.id(&FieldPath::root(0)).unwrap();
    m.raw.set(
        id,
        "length",
        None,
        "-2",
        "Bytes require only a length from 1 to 1048576.",
    );
    assert_eq!(m.raw.get(id, "length", None).unwrap().text, "-2");
    assert!(!m.valid());
    m.raw.clear(id, "length", None);
    assert!(m.valid());
}
#[test]
fn validation_focus_reveals_exact_nested_control() {
    let m = model(
        json!([{"name":"header","type":"struct","fields":[{"name":"bits","type":"u8","bitFlags":[{"bit":0,"name":"same"},{"bit":1,"name":"same"}]}]},{"name":"template","type":"u8","align":3}]),
    );
    let issues = collect_validation(&m.definition(), &RawInputState::default());
    let issue = issues.iter().find(|i| i.path == "header.bits").unwrap();
    let target = validation_target(issue, &m.definition());
    assert_eq!(target.control, "flag-name");
    assert_eq!(target.index, Some(1));
    assert_eq!(target.path, "header.bits");
    let target = validation_target(
        issues.iter().find(|i| i.path == "template").unwrap(),
        &m.definition(),
    );
    assert_eq!(target.control, "align");
    let target = validation_target(
        &ValidationIssue::new("template", "Template name must not be empty."),
        &m.definition(),
    );
    assert_eq!(target.control, "template-name");
}
#[test]
fn validation_collects_all_shapes_references_ranges_and_budgets() {
    let m = model(
        json!([{"name":"size","type":"i16"},{"name":"payload","type":"bytes","length":{"ref":"size","max":0}},{"name":"value","type":"u8","expect":{"kind":"range","min":{"type":"unsigned","value":"5"},"max":{"type":"unsigned","value":"1"}}},{"name":"flag","type":"bool","endianness":"big"},{"name":"items","type":"array","count":{"fixed":10000},"element":{"type":"u8"}}]),
    );
    let issues = collect_validation(&m.definition(), &Default::default());
    for path in ["payload", "value", "flag", "items"] {
        assert!(issues.iter().any(|i| i.path == path), "{path}: {issues:?}");
    }
    assert!(issues.iter().any(|i| i.message.contains("10000 nodes")));
    let m = model(
        json!([{"name":"items","type":"array","count":{"fixed":5},"element":{"type":"bytes","length":1048576}}]),
    );
    assert!(collect_validation(&m.definition(), &Default::default())
        .iter()
        .any(|i| i.message.contains("16 MiB")));
}

#[test]
fn every_golden_editor_validation_issue_matches_reference() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/template-validation.json")).unwrap();
    for (index, case) in cases.into_iter().enumerate() {
        let definition = serde_json::from_value(case["template"].clone()).unwrap();
        let actual = collect_validation(&definition, &Default::default())
            .into_iter()
            .map(|i| json!({"path":i.path,"message":i.message}))
            .collect::<Vec<_>>();
        assert_eq!(
            json!(actual),
            case["issues"],
            "case {index}: {}",
            case["template"]
        );
    }
}

#[test]
fn typed_checks_and_bounded_earlier_references_keep_literal_forms() {
    let m = model(json!([
        {"name":"offset","type":"u64"}, {"name":"size","type":"u16"},
        {"name":"magic","type":"bytes","length":2,"expect":{"kind":"equals","value":{"type":"bytes","value":"4D 5A"}}},
        {"name":"text","type":"string","encoding":"utf8","length":{"ref":"size","max":256},"placement":{"mode":"absolute","offset":{"ref":"offset","add":"2"}}}
    ]));
    assert!(collect_validation(&m.definition(), &Default::default()).is_empty());
    let value = serde_json::to_value(m.definition()).unwrap();
    assert_eq!(value["fields"][2]["expect"]["value"]["value"], "4D 5A");
    assert_eq!(value["fields"][3]["placement"]["offset"]["add"], "2");
}

#[test]
fn mismatched_reference_bounds_and_expected_types_report_each_field() {
    let m = model(json!([
        {"name":"size","type":"i16"},
        {"name":"payload","type":"bytes","length":{"ref":"size","max":0}},
        {"name":"value","type":"u8","expect":{"kind":"range","min":{"type":"unsigned","value":"5"},"max":{"type":"unsigned","value":"1"}}},
        {"name":"flag","type":"bool","expect":{"kind":"equals","value":{"type":"unsigned","value":"1"}}}
    ]));
    let issues = collect_validation(&m.definition(), &Default::default());
    for path in ["payload", "value", "flag"] {
        assert!(issues.iter().any(|i| i.path == path), "{path}: {issues:?}");
    }
}

#[test]
fn containers_nested_expansion_and_repeated_metadata_count_toward_limits() {
    for fields in [
        json!([{"name":"items","type":"array","count":{"fixed":10000},"element":{"type":"u8"}}]),
        json!([{"name":"items","type":"array","count":{"fixed":100},"element":{"type":"array","count":{"fixed":100},"element":{"type":"u8"}}}]),
    ] {
        let m = model(fields);
        assert!(collect_validation(&m.definition(), &Default::default())
            .iter()
            .any(|i| i.message.contains("10000")));
    }
    let m = model(
        json!([{"name":"items","type":"array","count":{"fixed":1000},"element":{"type":"u8","comment":"x".repeat(60000)}}]),
    );
    assert!(collect_validation(&m.definition(), &Default::default())
        .iter()
        .any(|i| i.message.contains("16 MiB")));
}

#[test]
fn empty_draft_has_only_the_unversioned_contract_and_no_validation_errors() {
    let draft = hexforge_controller::template::workspace::empty();
    assert_eq!(
        serde_json::to_value(&draft).unwrap(),
        json!({"name":"Untitled","defaultEndianness":"little","fields":[]})
    );
    assert!(collect_validation(&draft, &Default::default()).is_empty());
}

#[test]
fn nested_conditions_require_earlier_typed_values_in_the_correct_scope() {
    let m = model(json!([
        {"name":"count","type":"u8"},
        {"name":"records","type":"array","count":{"ref":"count"},"element":{"type":"struct","fields":[{"name":"id","type":"u16"}]}},
        {"name":"tail","type":"u8","condition":{"ref":"count","op":"gt","value":{"type":"unsigned","value":"0"}}}
    ]));
    assert!(collect_validation(&m.definition(), &Default::default()).is_empty());
    let m = model(json!([
        {"name":"bad","type":"array","count":{"ref":"later"},"element":{"type":"u8"}},
        {"name":"later","type":"i8"}
    ]));
    assert!(collect_validation(&m.definition(), &Default::default())
        .iter()
        .any(|i| i.path == "bad"));
    let m = model(json!([
        {"name":"recordCount","type":"u8"},
        {"name":"records","type":"array","count":{"ref":"recordCount"},"element":{"type":"struct","fields":[
            {"name":"length","type":"u8"},
            {"name":"payload","type":"array","count":{"ref":"length"},"element":{"type":"u8"}}
        ]}}
    ]));
    assert!(collect_validation(&m.definition(), &Default::default()).is_empty());
}

#[test]
fn invalid_scalar_endian_enum_width_and_duplicate_flag_names_are_all_reported() {
    let m = model(json!([
        {"name":"flag","type":"bool","endianness":"big"},
        {"name":"text","type":"string","encoding":"utf8","length":4,"endianness":"big"},
        {"name":"code","type":"u8","enumLabels":{"256":"too wide"}},
        {"name":"bits","type":"u8","bitFlags":[{"bit":0,"name":"duplicate"},{"bit":1,"name":"duplicate"}]}
    ]));
    let issues = collect_validation(&m.definition(), &Default::default());
    for path in ["flag", "text", "code", "bits"] {
        assert!(issues.iter().any(|i| i.path == path), "{path}: {issues:?}");
    }
}
