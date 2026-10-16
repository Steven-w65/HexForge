//! Consumer-visible contracts transferred from the reference IPC boundary.
//! These use real files and the shared operation adapter, not a mock transport.
use hexforge_core::{
    operations::{CoreReply, CoreRequest, CoreService},
    progress::OperationPhase,
    template::TemplateDefinition,
};

fn field_template() -> TemplateDefinition {
    serde_json::from_value(
        serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[
            {"name":"first","type":"u8","placement":{"mode":"absolute","offset":"0"}},
            {"name":"second","type":"u8","placement":{"mode":"absolute","offset":"1"}}
        ]}),
    )
    .unwrap()
}
fn execute(core: &CoreService, request: CoreRequest) -> CoreReply {
    core.execute(request, &mut |_| {}).unwrap()
}

#[test]
fn every_long_operation_returns_its_real_result_and_same_operation_progress() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    let csv = directory.path().join("fields.csv");
    let copy = directory.path().join("copy.bin");
    std::fs::write(&source, [1, 2, 3, 4, 5]).unwrap();
    let core = CoreService::default();
    execute(
        &core,
        CoreRequest::OpenFile {
            path: source.clone(),
            discard_unsaved: false,
        },
    );
    execute(
        &core,
        CoreRequest::EditByte {
            offset: 0,
            value: 9,
        },
    );
    let cases = [
        (
            CoreRequest::SearchBytes {
                pattern: "09 02".into(),
            },
            OperationPhase::Search,
            "5",
        ),
        (
            CoreRequest::ApplyTemplate {
                template: field_template(),
            },
            OperationPhase::Parse,
            "2",
        ),
        (
            CoreRequest::ExportResultsCsv {
                path: csv.clone(),
                template: field_template(),
            },
            OperationPhase::Parse,
            "2",
        ),
        (
            CoreRequest::SaveBinaryAs { path: copy.clone() },
            OperationPhase::Save,
            "5",
        ),
    ];
    let mut ids = std::collections::BTreeSet::new();
    for (index, (request, phase, completed)) in cases.into_iter().enumerate() {
        let mut progress = Vec::new();
        let reply = core.execute(request, &mut |p| progress.push(p)).unwrap();
        assert_eq!(progress[0].phase, phase);
        assert_eq!(progress[0].processed, "0");
        assert_eq!(progress.last().unwrap().phase, OperationPhase::Complete);
        assert_eq!(progress.last().unwrap().processed, completed);
        assert_eq!(progress.last().unwrap().total, completed);
        assert!(ids.insert(progress[0].operation_id.clone()));
        assert!(progress
            .iter()
            .all(|p| p.operation_id == progress[0].operation_id));
        let encoded = serde_json::to_value(&progress).unwrap();
        assert!(encoded
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["operationId"].is_string()
                && p["processed"].is_string()
                && p["total"].is_string()));
        match (index, reply) {
            (0, CoreReply::Search(found)) => assert_eq!(found.matches, ["0"]),
            (1, CoreReply::Parsed(nodes)) => {
                assert_eq!(nodes[0].path, "first");
                assert_eq!(nodes[0].value.as_deref(), Some("9"));
                assert_eq!(nodes[1].value.as_deref(), Some("2"));
            }
            (2, CoreReply::Unit) => assert!(progress
                .iter()
                .any(|p| p.phase == OperationPhase::Csv && p.total == "2")),
            (3, CoreReply::BinarySaved(saved)) => {
                assert_eq!(saved.bytes_written, "5");
                assert!(!saved.state.dirty);
            }
            (_, reply) => panic!("Wrong operation reply: {reply:?}"),
        }
    }
    assert_eq!(std::fs::read(source).unwrap(), [1, 2, 3, 4, 5]);
    assert_eq!(std::fs::read(copy).unwrap(), [9, 2, 3, 4, 5]);
    assert_eq!(std::fs::read_to_string(csv).unwrap(), "path,name,offset,length,type,endianness,value,comment,enumLabel,flags\nfirst,first,0,1,u8,little,9,,,\nsecond,second,1,1,u8,little,2,,,\n");
}

#[test]
fn adapter_overview_and_minimap_are_bounded_and_revision_guarded() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    std::fs::write(&source, [0x41; 1024]).unwrap();
    let core = CoreService::default();
    let CoreReply::FileInfo(info) = execute(
        &core,
        CoreRequest::OpenFile {
            path: source,
            discard_unsaved: false,
        },
    ) else {
        panic!("open")
    };
    for offset in [0, 512, 1023] {
        execute(
            &core,
            CoreRequest::EditByte {
                offset,
                value: 0x42,
            },
        );
    }
    let CoreReply::ModifiedOverview(overview) = execute(&core, CoreRequest::GetModifiedOverview)
    else {
        panic!("overview")
    };
    assert_eq!(
        serde_json::to_value(overview).unwrap(),
        serde_json::json!({"binCount":1024,"bins":[0,512,1023]})
    );
    let CoreReply::MinimapSamples(samples) = execute(
        &core,
        CoreRequest::ReadMinimapSamples {
            rows: vec![0, 32, 63],
            bytes_per_row: 16,
            source_key: info.path.clone(),
            expected_revision: 3,
        },
    ) else {
        panic!("samples")
    };
    assert_eq!(samples.revision, "3");
    assert_eq!(
        samples
            .samples
            .iter()
            .map(|s| s.row.as_str())
            .collect::<Vec<_>>(),
        ["0", "32", "63"]
    );
    assert_eq!(samples.samples[2].modified_offsets, ["1023"]);
    assert_eq!(samples.samples[2].bytes.len(), 16);
    assert_eq!(samples.samples[2].bytes[15], 0x42);
    for (rows, revision, code) in [
        (vec![0; 33], 3, "invalid_length"),
        (vec![0], 2, "operation_failed"),
        (vec![u64::MAX], 3, "invalid_offset"),
    ] {
        assert_eq!(
            core.execute(
                CoreRequest::ReadMinimapSamples {
                    rows,
                    bytes_per_row: 16,
                    source_key: info.path.clone(),
                    expected_revision: revision
                },
                &mut |_| {}
            )
            .unwrap_err()
            .code(),
            code
        );
    }
    execute(&core, CoreRequest::UndoEdit);
    let CoreReply::ModifiedOverview(overview) = execute(&core, CoreRequest::GetModifiedOverview)
    else {
        panic!("overview")
    };
    assert_eq!(overview.bins, [0, 512]);
}

#[test]
fn typed_boundary_preserves_known_errors_and_sanitizes_a_failed_core_lock() {
    let core = CoreService::default();
    let known = core
        .execute(
            CoreRequest::ReadPage {
                offset: u64::MAX,
                length: 1,
            },
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(known).unwrap(),
        serde_json::json!({"code":"no_active_file","message":"No file is open.","detail":null})
    );
    let failing = core.clone();
    assert!(std::thread::spawn(move || {
        let _guard = failing.session.lock().unwrap();
        panic!("private failure payload")
    })
    .join()
    .is_err());
    let sanitized = core
        .execute(CoreRequest::GetFileInfo, &mut |_| {})
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(sanitized).unwrap(),
        serde_json::json!({"code":"operation_failed","message":"The operation could not be completed.","detail":null})
    );
}
