use hexforge_lib::export::export_results_csv;
use hexforge_lib::session::FileSession;
use hexforge_lib::template::{parse_template, validate_template, TemplateDefinition};
use hexforge_lib::template_file::TemplateFileSession;

fn parse(json: &str, bytes: &[u8]) -> Vec<hexforge_lib::template::ParsedNode> {
    let template: TemplateDefinition = serde_json::from_str(json).unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    parse_template(&mut session, &template).unwrap()
}

#[test]
fn parser_reads_sparse_edits_as_effective_bytes() {
    let template: TemplateDefinition = serde_json::from_str(
        r#"{"name":"Wide","defaultEndianness":"little","fields":[{"name":"value","type":"u64","placement":{"mode":"absolute","offset":"0"}}]}"#,
    )
    .unwrap();

    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), [0_u8; 8]).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    session.edit_byte(7, 0x20).unwrap();

    let nodes = parse_template(&mut session, &template).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].path, "value");
    assert_eq!(nodes[0].offset.as_deref(), Some("0"));
    assert_eq!(nodes[0].length.as_deref(), Some("8"));
    assert_eq!(nodes[0].value.as_deref(), Some("2305843009213693952"));
    assert!(nodes[0].diagnostics.is_empty());
}

#[test]
fn all_scalar_widths_decode_in_both_byte_orders_without_float_or_integer_precision_loss() {
    let json = r#"{"name":"Scalars","defaultEndianness":"little","fields":[
      {"name":"u8","type":"u8"},{"name":"i8","type":"i8"},
      {"name":"u16","type":"u16"},{"name":"i16","type":"i16"},
      {"name":"u32","type":"u32"},{"name":"i32","type":"i32"},
      {"name":"u64","type":"u64"},{"name":"i64","type":"i64"},
      {"name":"f32","type":"f32"},{"name":"f64","type":"f64"},
      {"name":"bool","type":"bool"},
      {"name":"be16","type":"u16","endianness":"big"},
      {"name":"be32","type":"u32","endianness":"big"},
      {"name":"be64","type":"u64","endianness":"big"},
      {"name":"beFloat","type":"f32","endianness":"big"}
    ]}"#;
    let mut bytes = vec![255, 254];
    bytes.extend(0x1234_u16.to_le_bytes());
    bytes.extend((-2_i16).to_le_bytes());
    bytes.extend(0x12345678_u32.to_le_bytes());
    bytes.extend((-2_i32).to_le_bytes());
    bytes.extend(9_007_199_254_740_993_u64.to_le_bytes());
    bytes.extend((-9_007_199_254_740_993_i64).to_le_bytes());
    bytes.extend(1.5_f32.to_le_bytes());
    bytes.extend(2.5_f64.to_le_bytes());
    bytes.push(1);
    bytes.extend(0x1234_u16.to_be_bytes());
    bytes.extend(0x12345678_u32.to_be_bytes());
    bytes.extend(9_007_199_254_740_993_u64.to_be_bytes());
    bytes.extend(1.5_f32.to_be_bytes());
    let nodes = parse(json, &bytes);
    let actual: Vec<&str> = nodes
        .iter()
        .map(|node| node.value.as_deref().unwrap())
        .collect();
    assert_eq!(
        actual,
        [
            "255",
            "-2",
            "4660",
            "-2",
            "305419896",
            "-2",
            "9007199254740993",
            "-9007199254740993",
            "1.5",
            "2.5",
            "true",
            "4660",
            "305419896",
            "9007199254740993",
            "1.5"
        ]
    );
    assert_eq!(nodes[6].offset.as_deref(), Some("14"));
}

#[test]
fn fixed_and_bounded_null_terminated_text_use_explicit_encodings_and_exact_consumed_lengths() {
    let json = r#"{"name":"Text","defaultEndianness":"little","fields":[
      {"name":"ascii","type":"string","encoding":"ascii","length":4},
      {"name":"utf8","type":"string","encoding":"utf8","maxLength":4},
      {"name":"le","type":"string","encoding":"utf16le","maxLength":4},
      {"name":"be","type":"string","encoding":"utf16be","length":4},
      {"name":"raw","type":"bytes","length":2}
    ]}"#;
    let nodes = parse(
        json,
        &[
            b'A', b'B', 0, 0, 0xC3, 0xA9, 0, b'Z', 0, 0, 0, 0, b'Q', 0, 0, 0xDE, 0xAD,
        ],
    );
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.value.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["AB", "é", "Z", "Q", "DE AD"]
    );
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.length.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["4", "3", "4", "4", "2"]
    );
    assert_eq!(nodes[2].offset.as_deref(), Some("7"));
}

#[test]
fn nested_struct_array_reference_condition_alignment_and_labels_have_stable_paths() {
    let json = r#"{"name":"Records","defaultEndianness":"little","fields":[
      {"name":"header","type":"struct","fields":[
        {"name":"count","type":"u8"},
        {"name":"kind","type":"u8","enumLabels":{"1":"Active"},"bitFlags":[{"bit":0,"name":"enabled"}]}
      ]},
      {"name":"entries","type":"array","align":2,"count":{"ref":"header.count"},"element":{"type":"struct","align":2,"fields":[
        {"name":"id","type":"u16","endianness":"big"},
        {"name":"ok","type":"bool"}
      ]}},
      {"name":"tail","type":"u8","condition":{"ref":"header.kind","op":"eq","value":{"type":"unsigned","value":"1"}}}
    ]}"#;
    let nodes = parse(json, &[2, 1, 0x12, 0x34, 1, 0, 0xAB, 0xCD, 0, 0x77]);
    assert_eq!(nodes[0].path, "header");
    assert_eq!(nodes[0].children[1].enum_label.as_deref(), Some("Active"));
    assert_eq!(nodes[0].children[1].flags, ["enabled"]);
    assert_eq!(nodes[1].children[1].children[0].path, "entries[1].id");
    assert_eq!(
        nodes[1].children[1].children[0].offset.as_deref(),
        Some("6")
    );
    assert_eq!(
        nodes[1].children[1].children[0].value.as_deref(),
        Some("43981")
    );
    assert_eq!(nodes[2].offset.as_deref(), Some("9"));
    assert_eq!(nodes[2].value.as_deref(), Some("119"));
}

#[test]
fn missing_and_wrong_type_references_fail_preflight_and_dynamic_missing_values_are_error_nodes() {
    for json in [
        r#"{"name":"Bad","defaultEndianness":"little","fields":[{"name":"items","type":"array","count":{"ref":"count"},"element":{"type":"u8"}}]}"#,
        r#"{"name":"Bad","defaultEndianness":"little","fields":[{"name":"count","type":"i8"},{"name":"items","type":"array","count":{"ref":"count"},"element":{"type":"u8"}}]}"#,
    ] {
        let template: TemplateDefinition = serde_json::from_str(json).unwrap();
        let issue = validate_template(&template).unwrap_err();
        assert_eq!(issue.detail.as_deref(), Some("items"));
    }
    // Static scope is valid, but the count does not exist for this binary:
    // its definition is conditionally skipped. Runtime still returns a
    // path-specific diagnostic rather than silently assuming count zero.
    let missing = parse(
        r#"{"name":"Conditional","defaultEndianness":"little","fields":[
        {"name":"enabled","type":"bool"},
        {"name":"count","type":"u8","condition":{"ref":"enabled","op":"eq","value":{"type":"bool","value":true}}},
        {"name":"items","type":"array","count":{"ref":"count"},"element":{"type":"u8"}}
    ]}"#,
        &[0],
    );
    assert_eq!(missing[1].path, "items");
    assert_eq!(missing[1].kind, "error");
    assert!(missing[1].value.is_none());
}

#[test]
fn truncation_alignment_overflow_and_invalid_bool_never_return_successful_values() {
    let truncated = parse(
        r#"{"name":"Bounds","defaultEndianness":"little","fields":[{"name":"x","type":"u32"}]}"#,
        &[1, 2],
    );
    assert_eq!(truncated[0].kind, "error");
    assert_eq!(truncated[0].diagnostics[0].code, "template_out_of_bounds");
    assert!(truncated[0].value.is_none());
    let overflow = parse(
        r#"{"name":"Bounds","defaultEndianness":"little","fields":[{"name":"x","type":"u8","placement":{"mode":"absolute","offset":"18446744073709551615"},"align":2}]}"#,
        &[1],
    );
    assert!(overflow[0].diagnostics[0]
        .message
        .contains("x: Alignment overflows"));
    let invalid_bool = parse(
        r#"{"name":"Bounds","defaultEndianness":"little","fields":[{"name":"x","type":"bool"}]}"#,
        &[2],
    );
    assert!(invalid_bool[0].diagnostics[0].message.contains("Bool byte"));
}

#[test]
fn unknown_properties_and_top_level_format_keys_are_rejected() {
    for json in [
        r#"{"version":3,"name":"Bad","defaultEndianness":"little","fields":[]}"#,
        r#"{"name":"Bad","defaultEndianness":"little","fields":[],"script":"x"}"#,
        r#"{"name":"Bad","defaultEndianness":"little","fields":[{"name":"x","type":"u8","script":"x"}]}"#,
    ] {
        assert!(serde_json::from_str::<TemplateDefinition>(json).is_err());
    }
}

#[test]
fn fixed_expansion_and_total_decoded_byte_limits_fail_before_binary_reads() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(20_000_000).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    for json in [
        r#"{"name":"Huge","defaultEndianness":"little","fields":[{"name":"items","type":"array","count":{"fixed":10001},"element":{"type":"u8"}}]}"#,
        r#"{"name":"Huge","defaultEndianness":"little","fields":[{"name":"items","type":"array","count":{"fixed":5},"element":{"type":"bytes","length":1048576}}]}"#,
    ] {
        let template: TemplateDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(
            parse_template(&mut session, &template).unwrap_err().code(),
            "invalid_template"
        );
    }
}

#[test]
fn template_file_binding_saves_in_place_and_as_without_adding_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("template.json");
    let copy = dir.path().join("copy.json");
    std::fs::write(&original, r#"{"name":"Header","defaultEndianness":"big","fields":[{"name":"magic","type":"bytes","length":4}]}"#).unwrap();
    let mut files = TemplateFileSession::default();
    let mut document = files.load(&original).unwrap();
    document.name = "Changed".into();
    files.save(&document, false, None).unwrap();
    assert_eq!(files.load(&original).unwrap().name, "Changed");
    files.save_as(&copy, &document, false, None).unwrap();
    let persisted: TemplateDefinition =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(persisted, document);

    assert!(serde_json::to_value(&persisted)
        .unwrap()
        .get("version")
        .is_none());
}

#[test]
fn csv_exports_only_successful_leaves_with_paths_and_never_exports_partial_errors() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, [2, 0x12, 0x34, 0x56, 0x78]).unwrap();
    let mut session = FileSession::open(file, 256, 2).unwrap();
    let definition: TemplateDefinition = serde_json::from_str(r#"{"name":"Export","defaultEndianness":"big","fields":[{"name":"header","type":"struct","fields":[{"name":"count","type":"u8"},{"name":"records","type":"array","count":{"ref":"header.count"},"element":{"type":"u16"}}]}]}"#).unwrap();
    let results = parse_template(&mut session, &definition).unwrap();
    let output = dir.path().join("results.csv");
    export_results_csv(&output, &results).unwrap();
    assert_eq!(std::fs::read_to_string(&output).unwrap(),
        "path,name,offset,length,type,endianness,value,comment,enumLabel,flags\nheader.count,count,0,1,u8,big,2,,,\nheader.records[0],[0],1,2,u16,big,4660,,,\nheader.records[1],[1],3,2,u16,big,22136,,,\n");

    let invalid: TemplateDefinition = serde_json::from_str(r#"{"name":"Broken","defaultEndianness":"big","fields":[{"name":"missing","type":"u32","placement":{"mode":"absolute","offset":"10"}}]}"#).unwrap();
    let failed = parse_template(&mut session, &invalid).unwrap();
    let rejected = dir.path().join("rejected.csv");
    assert_eq!(
        export_results_csv(&rejected, &failed).unwrap_err().code(),
        "invalid_template"
    );
    assert!(!rejected.exists());
}

#[test]
fn documented_samples_parse_their_binary_fixtures() {
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../templates");
    for (stem, expected_path, expected_value) in [
        ("png-ihdr", "ihdr.width", "1"),
        ("riff-wav", "riff.data.samples[1]", "-1000"),
        ("elf64-header", "header.entry", "4198400"),
    ] {
        let document: TemplateDefinition =
            serde_json::from_slice(&std::fs::read(samples.join(format!("{stem}.json"))).unwrap())
                .unwrap();
        let mut session = FileSession::open(samples.join(format!("{stem}.bin")), 256, 2).unwrap();
        let nodes = parse_template(&mut session, &document).unwrap();
        fn leaves<'a>(
            nodes: &'a [hexforge_lib::template::ParsedNode],
            out: &mut Vec<&'a hexforge_lib::template::ParsedNode>,
        ) {
            for node in nodes {
                assert!(
                    node.diagnostics.is_empty(),
                    "{}: {:?}",
                    node.path,
                    node.diagnostics
                );
                if node.kind == "leaf" {
                    out.push(node);
                }
                leaves(&node.children, out);
            }
        }
        let mut parsed = Vec::new();
        leaves(&nodes, &mut parsed);
        assert_eq!(
            parsed
                .iter()
                .find(|node| node.path == expected_path)
                .and_then(|node| node.value.as_deref()),
            Some(expected_value),
            "{stem}"
        );
    }
}

#[test]
fn nested_arrays_relative_offsets_and_false_conditions_follow_deterministic_order() {
    let json = r#"{"name":"Nested","defaultEndianness":"little","fields":[
      {"name":"prefix","type":"u8"},
      {"name":"header","type":"struct","placement":{"mode":"absolute","offset":"4"},"fields":[
        {"name":"count","type":"u8"},
        {"name":"marker","type":"u16","placement":{"mode":"relative","offset":"2"},"endianness":"big"}
      ]},
      {"name":"skipped","type":"u8","condition":{"ref":"header.count","op":"eq","value":{"type":"unsigned","value":"9"}}},
      {"name":"matrix","type":"array","count":{"ref":"header.count"},"element":{"type":"array","count":{"fixed":2},"element":{"type":"u16"}}}
    ]}"#;
    let nodes = parse(
        json,
        &[0x77, 0, 0, 0, 2, 0, 0x12, 0x34, 1, 0, 2, 0, 3, 0, 4, 0],
    );
    assert_eq!(nodes.len(), 3, "a false condition must omit the field");
    assert_eq!(nodes[1].children[1].offset.as_deref(), Some("6"));
    assert_eq!(nodes[1].children[1].value.as_deref(), Some("4660"));
    assert_eq!(nodes[2].children[1].children[1].path, "matrix[1][1]");
    assert_eq!(nodes[2].children[1].children[1].value.as_deref(), Some("4"));
    assert_eq!(
        nodes[2].children[1].children[1].offset.as_deref(),
        Some("14")
    );
}

#[test]
fn typed_conditions_compare_signed_bool_and_text_values_from_earlier_leaves() {
    let json = r#"{"name":"Conditions","defaultEndianness":"little","fields":[
      {"name":"signed","type":"i8"},
      {"name":"enabled","type":"bool"},
      {"name":"tag","type":"string","encoding":"ascii","length":2},
      {"name":"signedCase","type":"u8","condition":{"ref":"signed","op":"lt","value":{"type":"signed","value":"-1"}}},
      {"name":"boolCase","type":"u8","condition":{"ref":"enabled","op":"eq","value":{"type":"bool","value":true}}},
      {"name":"textCase","type":"u8","condition":{"ref":"tag","op":"eq","value":{"type":"string","value":"A"}}},
      {"name":"skipped","type":"u8","condition":{"ref":"tag","op":"ne","value":{"type":"string","value":"A"}}},
      {"name":"tail","type":"u8"}
    ]}"#;
    let nodes = parse(json, &[0xFE, 1, b'A', 0, 0x11, 0x22, 0x33, 0x44]);
    assert_eq!(nodes.len(), 7);
    assert_eq!(nodes[3].value.as_deref(), Some("17"));
    assert_eq!(nodes[4].value.as_deref(), Some("34"));
    assert_eq!(nodes[5].value.as_deref(), Some("51"));
    assert_eq!(nodes[6].path, "tail");
    assert_eq!(nodes[6].offset.as_deref(), Some("7"));
}

#[test]
fn references_inside_repeated_structures_resolve_to_each_elements_earlier_values() {
    let json = r#"{"name":"Scoped","defaultEndianness":"little","fields":[
      {"name":"recordCount","type":"u8"},
      {"name":"records","type":"array","count":{"ref":"recordCount"},"element":{"type":"struct","fields":[
        {"name":"length","type":"u8"},
        {"name":"payload","type":"array","count":{"ref":"length"},"element":{"type":"u8"}},
        {"name":"tail","type":"u8","condition":{"ref":"length","op":"gt","value":{"type":"unsigned","value":"1"}}}
      ]}}
    ]}"#;
    let nodes = parse(json, &[2, 1, 0xAA, 2, 0xBB, 0xCC, 0xDD]);
    assert!(nodes.iter().all(|node| node.diagnostics.is_empty()));
    assert_eq!(
        nodes[1].children[0].children.len(),
        2,
        "first record's tail is skipped"
    );
    assert_eq!(
        nodes[1].children[0].children[1].children[0]
            .value
            .as_deref(),
        Some("170")
    );
    assert_eq!(
        nodes[1].children[1].children[1].children[1].path,
        "records[1].payload[1]"
    );
    assert_eq!(
        nodes[1].children[1].children[1].children[1]
            .value
            .as_deref(),
        Some("204")
    );
    assert_eq!(
        nodes[1].children[1].children[2].value.as_deref(),
        Some("221")
    );
}

#[test]
fn dynamic_expansion_and_decoded_data_limits_are_enforced_after_references_resolve() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(6_000_000).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    for (json, expected) in [
        (
            r#"{"name":"Nodes","defaultEndianness":"little","fields":[{"name":"count","type":"u16"},{"name":"items","type":"array","count":{"ref":"count"},"element":{"type":"u8"}}]}"#,
            "expanded-result limit",
        ),
        (
            r#"{"name":"Bytes","defaultEndianness":"little","fields":[{"name":"count","type":"u8"},{"name":"items","type":"array","count":{"ref":"count"},"element":{"type":"bytes","length":1048576}}]}"#,
            "decoded-data limit",
        ),
    ] {
        // Sparse in-memory edits set the count without changing the file on disk.
        session
            .edit_byte(
                0,
                if expected == "expanded-result limit" {
                    0x11
                } else {
                    5
                },
            )
            .unwrap();
        if expected == "expanded-result limit" {
            session.edit_byte(1, 0x27).unwrap();
        } // 10001 LE
        let template: TemplateDefinition = serde_json::from_str(json).unwrap();
        let error = parse_template(&mut session, &template).unwrap_err();
        assert_eq!(error.code(), "invalid_template");
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn unknown_nested_properties_are_not_silently_accepted() {
    for fragment in [
        r#""placement":{"mode":"absolute","offset":"0","script":"x"}"#,
        r#""condition":{"ref":"a","op":"eq","value":{"type":"unsigned","value":"1","script":"x"}}"#,
        r#""count":{"fixed":1,"script":"x"}"#,
        r#""bitFlags":[{"bit":0,"name":"x","script":"x"}]"#,
    ] {
        let field = if fragment.starts_with("\"count\"") {
            format!(
                "{{\"name\":\"b\",\"type\":\"array\",{fragment},\"element\":{{\"type\":\"u8\"}}}}"
            )
        } else {
            format!("{{\"name\":\"b\",\"type\":\"u8\",{fragment}}}")
        };
        let json = format!("{{\"name\":\"Bad\",\"defaultEndianness\":\"little\",\"fields\":[{{\"name\":\"a\",\"type\":\"u8\"}},{field}]}}");
        assert!(
            serde_json::from_str::<TemplateDefinition>(&json).is_err(),
            "accepted {fragment}"
        );
    }
}

#[test]
fn enum_keys_must_fit_their_integer_field_width() {
    for (kind, key) in [("u8", "256"), ("i8", "128"), ("i16", "-32769")] {
        let json = format!(
            r#"{{"name":"Labels","defaultEndianness":"little","fields":[{{"name":"value","type":"{kind}","enumLabels":{{"{key}":"impossible"}}}}]}}"#
        );
        let template: TemplateDefinition = serde_json::from_str(&json).unwrap();
        let issue = validate_template(&template).unwrap_err();
        assert!(issue.message.contains("value"), "{}", issue.message);
        assert!(issue.message.contains("width"), "{}", issue.message);
    }
}
