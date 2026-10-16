use hexforge_core::operations::{CoreReply, CoreRequest, CoreService};
use hexforge_core::template::TemplateDefinition;

fn definition() -> TemplateDefinition {
    serde_json::from_value(
        serde_json::json!({"name":"Untitled","defaultEndianness":"little","fields":[]}),
    )
    .unwrap()
}

#[test]
fn operation_adapter_preserves_command_results() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.bin");
    std::fs::write(&path, [1, 2, 3]).unwrap();
    let core = CoreService::default();
    let opened = core
        .execute(
            CoreRequest::OpenFile {
                path: path.clone(),
                discard_unsaved: false,
            },
            &mut |_| {},
        )
        .unwrap();
    let CoreReply::FileInfo(info) = opened else {
        panic!("wrong open reply")
    };
    assert_eq!(info.size, "3");
    assert_eq!(info.revision, "0");
    core.execute(
        CoreRequest::EditByte {
            offset: 1,
            value: 9,
        },
        &mut |_| {},
    )
    .unwrap();
    let error = core
        .execute(
            CoreRequest::CloseFile {
                discard_unsaved: false,
            },
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(error.code(), "unsaved_changes");
    let CoreReply::Page(page) = core
        .execute(
            CoreRequest::ReadPage {
                offset: 0,
                length: 3,
            },
            &mut |_| {},
        )
        .unwrap()
    else {
        panic!("wrong page reply")
    };
    assert_eq!(page.bytes, [1, 9, 3]);
    assert_eq!(page.modified_offsets, ["1"]);
    assert_eq!(std::fs::read(&path).unwrap(), [1, 2, 3]);
}

#[test]
fn progress_sink_failure_is_nonfatal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.bin");
    let copy = directory.path().join("copy.bin");
    std::fs::write(&path, [1, 2, 3]).unwrap();
    let core = CoreService::default();
    core.execute(
        CoreRequest::OpenFile {
            path: path.clone(),
            discard_unsaved: false,
        },
        &mut |_| {},
    )
    .unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    drop(receive);
    let result = core
        .execute(
            CoreRequest::SaveBinaryAs { path: copy.clone() },
            &mut |progress| {
                let _ = send.send(progress);
            },
        )
        .unwrap();
    assert!(matches!(result, CoreReply::BinarySaved(_)));
    assert_eq!(std::fs::read(copy).unwrap(), [1, 2, 3]);
    assert_eq!(std::fs::read(path).unwrap(), [1, 2, 3]);
}

#[test]
fn template_alias_cannot_overwrite_binary() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    let sub = directory.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(&source, [1, 2, 3]).unwrap();
    let core = CoreService::default();
    core.execute(
        CoreRequest::OpenFile {
            path: source.clone(),
            discard_unsaved: false,
        },
        &mut |_| {},
    )
    .unwrap();
    let error = core
        .execute(
            CoreRequest::SaveTemplateAs {
                path: sub.join("../source.bin"),
                template: definition(),
                overwrite: true,
            },
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(error.code(), "destination_is_source");
    assert_eq!(std::fs::read(source).unwrap(), [1, 2, 3]);
}
