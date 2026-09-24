use hexforge_lib::{export, search, session::FileSession, template};

#[test]
fn operations_report_each_chunk_field_and_row() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    std::fs::write(&source, [1, 2, 3, 4, 5]).unwrap();
    let mut session = FileSession::open(source, 2, 2).unwrap();
    let mut searched = Vec::new();
    let result = search::search_session_with_progress(&mut session, &[9], 2, 100, &mut |value| {
        searched.push(value)
    })
    .unwrap();
    assert!(result.matches.is_empty());
    assert_eq!(searched, [2, 4, 5]);

    let definition: template::TemplateDefinition = serde_json::from_value(serde_json::json!({
        "version":1,"name":"Header","defaultEndianness":"little","fields":[
            {"name":"first","offset":"0","type":"u8","comment":""},
            {"name":"second","offset":"1","type":"u8","comment":""}
        ]
    }))
    .unwrap();
    let mut parsed = Vec::new();
    let fields = template::parse_template_with_progress(&mut session, &definition, &mut |value| {
        parsed.push(value)
    })
    .unwrap();
    assert_eq!(parsed, [1, 2]);
    assert_eq!(fields[1].value, "2");
    let mut rows = Vec::new();
    export::export_csv_create_new_with_progress(
        &directory.path().join("fields.csv"),
        &fields,
        &mut |value| rows.push(value),
    )
    .unwrap();
    assert_eq!(rows, [1, 2]);
    session.edit_byte(0, 9).unwrap();
    let mut saved = Vec::new();
    export::save_session_as_with_progress(
        &mut session,
        &directory.path().join("copy.bin"),
        2,
        &mut |value| saved.push(value),
    )
    .unwrap();
    assert_eq!(saved, [2, 4, 5]);
    assert!(session.is_dirty());
    assert_eq!(
        std::fs::read(directory.path().join("copy.bin")).unwrap(),
        [9, 2, 3, 4, 5]
    );
}
