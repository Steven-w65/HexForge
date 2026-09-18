use hexforge_lib::export::save_session_as;
use hexforge_lib::session::FileSession;

#[test]
fn save_as_applies_edits_without_changing_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let output = dir.path().join("copy.bin");
    std::fs::write(&source, [1, 2, 3, 4]).unwrap();
    let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
    session.edit_byte(1, 9).unwrap();

    let summary = save_session_as(&mut session, &output, 2).unwrap();

    assert_eq!(summary.bytes_written, 4);
    assert_eq!(summary.destination, output);
    assert_eq!(std::fs::read(&source).unwrap(), [1, 2, 3, 4]);
    assert_eq!(std::fs::read(&output).unwrap(), [1, 9, 3, 4]);
    assert!(!session.is_dirty());
}

#[test]
fn save_as_rejects_source_and_preserves_pending_edits() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, [1, 2]).unwrap();
    let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
    session.edit_byte(0, 9).unwrap();

    let error = save_session_as(&mut session, &source, 2).unwrap_err();

    assert_eq!(error.code(), "destination_is_source");
    assert_eq!(std::fs::read(&source).unwrap(), [1, 2]);
    assert!(session.is_dirty());
}

#[test]
fn save_as_preserves_existing_destination_and_dirty_edits() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let output = dir.path().join("existing.bin");
    std::fs::write(&source, [1, 2]).unwrap();
    std::fs::write(&output, [7, 7]).unwrap();
    let mut session = FileSession::open(source, 2, 2).unwrap();
    session.edit_byte(0, 9).unwrap();

    let error = save_session_as(&mut session, &output, 2).unwrap_err();

    assert_eq!(error.code(), "destination_exists");
    assert_eq!(std::fs::read(output).unwrap(), [7, 7]);
    assert!(session.is_dirty());
}

#[test]
fn save_as_handles_empty_file_and_caps_oversized_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("empty.bin");
    let output = dir.path().join("copy.bin");
    std::fs::write(&source, []).unwrap();
    let mut session = FileSession::open(source, 1, 1).unwrap();

    let summary = save_session_as(&mut session, &output, 2 * 1_048_576).unwrap();

    assert_eq!(summary.bytes_written, 0);
    assert_eq!(std::fs::read(output).unwrap(), Vec::<u8>::new());
    assert!(!session.is_dirty());
}

#[test]
fn save_as_rejects_zero_chunk_size_without_creating_output() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let output = dir.path().join("copy.bin");
    std::fs::write(&source, [1]).unwrap();
    let mut session = FileSession::open(source, 1, 1).unwrap();
    session.edit_byte(0, 9).unwrap();

    let error = save_session_as(&mut session, &output, 0).unwrap_err();

    assert_eq!(error.code(), "invalid_length");
    assert!(!output.exists());
    assert!(session.is_dirty());
}
