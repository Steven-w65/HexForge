use hexforge_lib::session::FileSession;
use hexforge_lib::template::{
    parse_template, parse_template_with_progress, validate_template, TemplateDefinition,
};

fn definition(fields: serde_json::Value) -> TemplateDefinition {
    serde_json::from_value(
        serde_json::json!({"name":"Checks","defaultEndianness":"little","fields":fields}),
    )
    .unwrap()
}

fn parse(fields: serde_json::Value, bytes: &[u8]) -> Vec<hexforge_lib::template::ParsedNode> {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    parse_template(&mut session, &definition(fields)).unwrap()
}

#[test]
fn preflight_rejects_forward_and_wrong_type_references_without_binary_reads() {
    for fields in [
        serde_json::json!([{"name":"items","type":"array","count":{"ref":"later"},"element":{"type":"u8"}}, {"name":"later","type":"u8"}]),
        serde_json::json!([{"name":"count","type":"i8"}, {"name":"items","type":"array","count":{"ref":"count"},"element":{"type":"u8"}}]),
    ] {
        let issue = validate_template(&definition(fields)).unwrap_err();
        assert!(issue.message.contains("items"));
    }
}

#[test]
fn repeated_result_metadata_is_bounded_before_fixed_arrays_are_read() {
    let template = definition(serde_json::json!([
        {"name":"items","type":"array","count":{"fixed":1000},"element":{"type":"u8","comment":"x".repeat(60000)}}
    ]));
    assert!(validate_template(&template)
        .unwrap_err()
        .message
        .contains("16 MiB"));
}

#[test]
fn truncated_field_retains_attempted_range_and_reports_available_bytes() {
    let nodes = parse(
        serde_json::json!([{"name":"id","type":"u32","placement":{"mode":"absolute","offset":"1"}}]),
        &[0, 1, 2],
    );
    assert_eq!(nodes[0].offset.as_deref(), Some("1"));
    assert_eq!(nodes[0].length.as_deref(), Some("4"));
    assert!(nodes[0].value.is_none());
    assert!(nodes[0].diagnostics[0].message.contains("2 available"));
}

#[test]
fn nested_progress_is_monotonic_and_arrives_before_a_large_array_finishes() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), vec![7; 2000]).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    let mut progress = Vec::new();
    let nodes = parse_template_with_progress(
        &mut session,
        &definition(serde_json::json!([
            {"name":"items","type":"array","count":{"fixed":2000},"element":{"type":"u8"}}
        ])),
        &mut |value| progress.push(value),
    )
    .unwrap();
    assert_eq!(nodes[0].children.len(), 2000);
    assert!(progress.len() >= 3 && progress.len() < 100, "{progress:?}");
    assert!(progress.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(progress.last(), Some(&2001));
}

#[test]
fn expected_bytes_text_integer_sets_ranges_and_floats_are_typed_checks() {
    let nodes = parse(
        serde_json::json!([
            {"name":"magic","type":"bytes","length":2,"expect":{"kind":"equals","value":{"type":"bytes","value":"4d 5a"}}},
            {"name":"tag","type":"string","encoding":"ascii","length":2,"expect":{"kind":"equals","value":{"type":"string","value":"OK"}}},
            {"name":"kind","type":"u8","expect":{"kind":"oneOf","values":[{"type":"unsigned","value":"1"},{"type":"unsigned","value":"2"}]}},
            {"name":"size","type":"i8","expect":{"kind":"range","min":{"type":"signed","value":"-3"},"max":{"type":"signed","value":"3"}}},
            {"name":"float","type":"f32","expect":{"kind":"range","min":{"type":"float","value":"1"},"max":{"type":"float","value":"2"}}}
        ]),
        &[0x4D, 0x5A, b'O', b'K', 2, 0xFE, 0, 0, 0xC0, 0x3F],
    );
    assert_eq!(nodes.len(), 5);
    assert!(nodes.iter().all(|node| node.diagnostics.is_empty()));
    assert_eq!(nodes[4].value.as_deref(), Some("1.5"));
}

#[test]
fn failed_expectations_have_no_success_value_and_cannot_export() {
    let nodes = parse(
        serde_json::json!([
            {"name":"magic","type":"bytes","length":2,"expect":{"kind":"equals","value":{"type":"bytes","value":"4D 5A"}}},
            {"name":"tail","type":"u8"}
        ]),
        &[0, 0, 7],
    );
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].kind, "error");
    assert!(nodes[0].value.is_none());
    assert_eq!(nodes[0].offset.as_deref(), Some("0"));
    assert_eq!(nodes[0].diagnostics[0].code, "template_expectation_failed");
    let dir = tempfile::tempdir().unwrap();
    assert!(hexforge_lib::export::export_results_csv(&dir.path().join("bad.csv"), &nodes).is_err());
    assert!(!dir.path().join("bad.csv").exists());
}

#[test]
fn reference_lengths_and_offsets_are_bounded_and_use_each_structure_scope() {
    let nodes = parse(
        serde_json::json!([
            {"name":"offset","type":"u8"},
            {"name":"records","type":"array","count":{"fixed":2},"element":{"type":"struct","fields":[
                {"name":"size","type":"u8"},
                {"name":"text","type":"string","encoding":"ascii","length":{"ref":"size","max":8}}
            ]}},
            {"name":"payload","type":"bytes","length":2,"placement":{"mode":"absolute","offset":{"ref":"offset","add":"1"}}}
        ]),
        &[8, 1, b'A', 2, b'B', b'C', 0, 0, 0, 0xAA, 0xBB],
    );
    assert_eq!(nodes[1].children[0].children[1].value.as_deref(), Some("A"));
    assert_eq!(
        nodes[1].children[1].children[1].value.as_deref(),
        Some("BC")
    );
    assert_eq!(nodes[2].offset.as_deref(), Some("9"));
    assert_eq!(nodes[2].value.as_deref(), Some("AA BB"));
}

#[test]
fn dynamic_zero_lengths_are_valid_but_oversized_or_odd_utf16_lengths_fail_safely() {
    let fields = serde_json::json!([
        {"name":"size","type":"u8"},
        {"name":"payload","type":"bytes","length":{"ref":"size","max":4}},
        {"name":"tail","type":"u8"}
    ]);
    let empty = parse(fields.clone(), &[0, 9]);
    assert_eq!(empty[1].value.as_deref(), Some(""));
    assert_eq!(empty[1].length.as_deref(), Some("0"));
    assert_eq!(empty[2].value.as_deref(), Some("9"));
    let oversized = parse(fields, &[5, 0, 0, 0, 0, 0]);
    assert_eq!(oversized[1].kind, "error");
    assert_eq!(oversized[1].offset.as_deref(), Some("1"));
    assert_eq!(oversized[1].length.as_deref(), Some("5"));
    assert!(oversized[1].diagnostics[0].message.contains("maximum"));
    let odd = parse(
        serde_json::json!([
            {"name":"size","type":"u8"},
            {"name":"text","type":"string","encoding":"utf16le","length":{"ref":"size","max":8}}
        ]),
        &[3, 65, 0, 0],
    );
    assert_eq!(odd[1].diagnostics[0].code, "template_data_invalid");
}

#[test]
fn exact_reference_offsets_and_checked_additions_never_round_or_wrap() {
    let fields = serde_json::json!([
        {"name":"offset","type":"u64"},
        {"name":"value","type":"u8","placement":{"mode":"absolute","offset":{"ref":"offset"}}}
    ]);
    let nodes = parse(fields, &9007199254740993u64.to_le_bytes());
    assert_eq!(nodes[1].offset.as_deref(), Some("9007199254740993"));
    let overflow = parse(
        serde_json::json!([
            {"name":"offset","type":"u64"},
            {"name":"value","type":"u8","placement":{"mode":"absolute","offset":{"ref":"offset","add":"1"}}}
        ]),
        &u64::MAX.to_le_bytes(),
    );
    assert!(overflow[1].diagnostics[0].message.contains("overflows"));
}

#[test]
fn new_properties_reject_unknown_keys_wrong_types_unbounded_lengths_and_inverted_ranges() {
    for field in [
        serde_json::json!({"name":"bad","type":"bytes","length":{"ref":"size"}}),
        serde_json::json!({"name":"bad","type":"u8","expect":{"kind":"equals","value":{"type":"string","value":"x"}}}),
        serde_json::json!({"name":"bad","type":"u8","expect":{"kind":"range","min":{"type":"unsigned","value":"4"},"max":{"type":"unsigned","value":"1"}}}),
        serde_json::json!({"name":"bad","type":"u8","placement":{"mode":"absolute","offset":{"ref":"size","script":"x"}}}),
    ] {
        let doc = serde_json::json!({"name":"Invalid","defaultEndianness":"little","fields":[{"name":"size","type":"u8"},field]});
        assert!(serde_json::from_value::<TemplateDefinition>(doc)
            .map(|template| validate_template(&template).is_err())
            .unwrap_or(true));
    }
}

#[test]
fn source_changes_during_cache_hits_reject_the_entire_result() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), [1, 2]).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    let issue = parse_template_with_progress(
        &mut session,
        &definition(serde_json::json!([
            {"name":"a","type":"u8"}, {"name":"b","type":"u8"}
        ])),
        &mut |value| {
            if value == 1 {
                std::fs::write(file.path(), [3, 4, 5]).unwrap();
            }
        },
    )
    .unwrap_err();
    assert_eq!(issue.code(), "source_changed");
}

#[test]
fn cached_and_direct_reads_cross_chunk_boundaries_without_losing_bytes() {
    let mut bytes = vec![0; 140000];
    bytes[65535..65539].copy_from_slice(&0x12345678u32.to_le_bytes());
    bytes[139999] = 0xAA;
    let nodes = parse(
        serde_json::json!([
            {"name":"first","type":"u8"},
            {"name":"boundary","type":"u32","placement":{"mode":"absolute","offset":"65535"}},
            {"name":"large","type":"bytes","length":70000,"placement":{"mode":"absolute","offset":"70000"}},
            {"name":"last","type":"u8","placement":{"mode":"absolute","offset":"139999"}}
        ]),
        &bytes,
    );
    assert_eq!(nodes[1].value.as_deref(), Some("305419896"));
    assert!(nodes[2].value.as_deref().unwrap().ends_with(" AA"));
    assert_eq!(nodes[3].value.as_deref(), Some("170"));
}

#[test]
fn expectations_preserve_exact_large_integers_in_both_byte_orders_and_check_bools() {
    for (endian, bytes) in [
        ("little", 9007199254740993u64.to_le_bytes()),
        ("big", 9007199254740993u64.to_be_bytes()),
    ] {
        let nodes = parse(
            serde_json::json!([
                {"name":"id","type":"u64","endianness":endian,"expect":{"kind":"equals","value":{"type":"unsigned","value":"9007199254740993"}}}
            ]),
            &bytes,
        );
        assert_eq!(nodes[0].value.as_deref(), Some("9007199254740993"));
        assert!(nodes[0].diagnostics.is_empty());
        let failed = parse(
            serde_json::json!([
                {"name":"id","type":"u64","endianness":endian,"expect":{"kind":"equals","value":{"type":"unsigned","value":"9007199254740992"}}}
            ]),
            &bytes,
        );
        assert!(failed[0].value.is_none());
    }
    let flag = parse(
        serde_json::json!([{"name":"enabled","type":"bool","expect":{"kind":"equals","value":{"type":"bool","value":true}}}]),
        &[1],
    );
    assert_eq!(flag[0].value.as_deref(), Some("true"));
}

#[test]
fn new_schema_round_trips_and_parses_sparse_edited_lengths_and_signatures() {
    let template = definition(serde_json::json!([
        {"name":"magic","type":"u8","expect":{"kind":"equals","value":{"type":"unsigned","value":"7"}}},
        {"name":"size","type":"u8"},
        {"name":"text","type":"string","encoding":"ascii","length":{"ref":"size","max":16},"placement":{"mode":"absolute","offset":"2"}}
    ]));
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("data.bin");
    std::fs::write(&source, [0, 0, b'O', b'K']).unwrap();
    let path = folder.path().join("template.json");
    let mut files = hexforge_lib::template_file::TemplateFileSession::default();
    files
        .save_as(&path, &template, false, Some(&source))
        .unwrap();
    let loaded = files.load(&path).unwrap();
    assert_eq!(loaded, template);
    assert!(serde_json::to_value(&loaded)
        .unwrap()
        .get("version")
        .is_none());
    let mut session = FileSession::open(source.clone(), 256, 2).unwrap();
    session.edit_byte(0, 7).unwrap();
    session.edit_byte(1, 2).unwrap();
    let nodes = parse_template(&mut session, &loaded).unwrap();
    assert_eq!(nodes[2].value.as_deref(), Some("OK"));
    assert_eq!(std::fs::read(source).unwrap(), [0, 0, b'O', b'K']);
    let csv = folder.path().join("result.csv");
    hexforge_lib::export::export_results_csv(&csv, &nodes).unwrap();
    assert!(std::fs::read_to_string(csv)
        .unwrap()
        .contains("text,text,2,2,string,little,OK"));
}

#[test]
fn referenced_relative_offset_is_added_to_structure_base_and_aligned() {
    let nodes = parse(
        serde_json::json!([
            {"name":"block","type":"struct","placement":{"mode":"absolute","offset":"4"},"fields":[
                {"name":"offset","type":"u8"},
                {"name":"value","type":"u16","align":2,"placement":{"mode":"relative","offset":{"ref":"offset","add":"1"}}}
            ]}
        ]),
        &[0, 0, 0, 0, 2, 0, 0, 0, 0x34, 0x12],
    );
    assert_eq!(nodes[0].children[1].offset.as_deref(), Some("8"));
    assert_eq!(nodes[0].children[1].value.as_deref(), Some("4660"));
}
