use hexforge_core::template::TemplateDefinition;
use hexforge_core::template_file::TemplateFileSession;

#[test]
fn unversioned_template_round_trips_without_a_version_property() {
    let source = r#"{"name":"Example","defaultEndianness":"little","fields":[]}"#;
    let template: TemplateDefinition = serde_json::from_str(source).unwrap();
    let saved = serde_json::to_value(template).unwrap();
    assert_eq!(saved["name"], "Example");
    assert!(saved.get("version").is_none());
}

#[test]
fn versioned_template_files_are_rejected() {
    for version in [1, 2] {
        let source = format!(
            r#"{{"version":{version},"name":"Old","defaultEndianness":"little","fields":[]}}"#
        );
        let issue = serde_json::from_str::<TemplateDefinition>(&source).unwrap_err();
        assert!(issue.to_string().contains("version"), "{issue}");
    }
}

#[test]
fn loading_versioned_json_does_not_replace_the_current_file_binding() {
    let folder = tempfile::tempdir().unwrap();
    let current = folder.path().join("current.json");
    let old = folder.path().join("old.json");
    std::fs::write(
        &current,
        r#"{"name":"Current","defaultEndianness":"little","fields":[]}"#,
    )
    .unwrap();
    std::fs::write(
        &old,
        r#"{"version":2,"name":"Old","defaultEndianness":"little","fields":[]}"#,
    )
    .unwrap();
    let mut files = TemplateFileSession::default();
    assert_eq!(files.load(&current).unwrap().name, "Current");
    let original_path = files.path().unwrap().to_path_buf();
    let original_bytes = std::fs::read(&current).unwrap();
    let issue = files.load(&old).unwrap_err();
    assert!(issue.message.contains("version"));
    assert_eq!(files.path(), Some(original_path.as_path()));
    assert_eq!(std::fs::read(&current).unwrap(), original_bytes);
}

#[test]
fn saving_an_unversioned_template_round_trips_without_adding_a_format_key() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("template.json");
    let mut files = TemplateFileSession::default();
    let template: TemplateDefinition = serde_json::from_str(
        r#"{"name":"Round trip","defaultEndianness":"big","fields":[{"name":"header","type":"struct","fields":[{"name":"count","type":"u64"}]}]}"#,
    ).unwrap();

    files.save_as(&path, &template, false, None).unwrap();
    assert_eq!(
        files.path(),
        Some(std::fs::canonicalize(&path).unwrap().as_path())
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(persisted.get("version").is_none());
    assert_eq!(files.load(&path).unwrap(), template);

    let changed = TemplateDefinition {
        name: "Updated".into(),
        ..template
    };
    files.save(&changed, false, None).unwrap();
    assert_eq!(files.load(&path).unwrap().name, "Updated");
}

#[test]
fn persistence_detects_external_changes_and_refuses_binary_source_overwrite() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("template.json");
    let binary = folder.path().join("source.bin");
    std::fs::write(
        &path,
        r#"{"name":"Current","defaultEndianness":"little","fields":[]}"#,
    )
    .unwrap();
    std::fs::write(&binary, [0_u8, 1, 2]).unwrap();
    let mut files = TemplateFileSession::default();
    let template = files.load(&path).unwrap();

    let changed_disk = r#"{"name":"External","defaultEndianness":"little","fields":[]}"#;
    std::fs::write(&path, changed_disk).unwrap();
    assert!(files
        .save(&template, false, None)
        .unwrap_err()
        .message
        .contains("modified"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), changed_disk);
    files.save(&template, true, None).unwrap();
    assert_eq!(files.load(&path).unwrap(), template);

    let failed = files
        .save_as(&binary, &template, true, Some(&binary))
        .unwrap_err();
    assert!(failed.message.contains("binary"));
    assert_eq!(std::fs::read(&binary).unwrap(), [0_u8, 1, 2]);
    assert_eq!(
        files.path(),
        Some(std::fs::canonicalize(&path).unwrap().as_path())
    );
    std::fs::remove_file(&path).unwrap();
    files.save(&template, false, Some(&binary)).unwrap();
    assert_eq!(files.load(&path).unwrap(), template);
}

#[test]
fn template_saves_reject_hard_link_aliases_without_changing_state_or_files() {
    for in_place in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let binary = folder.path().join("source.bin");
        let alias = folder.path().join("alias.json");
        let current = folder.path().join("current.json");
        let original = br#"{"name":"Current","defaultEndianness":"little","fields":[]}"#;
        std::fs::write(&binary, original).unwrap();
        std::fs::hard_link(&binary, &alias).unwrap();
        std::fs::write(&current, original).unwrap();
        let mut files = TemplateFileSession::default();
        let mut edited = files
            .load(if in_place { &alias } else { &current })
            .unwrap();
        let bound = files.path().unwrap().to_path_buf();
        edited.name = "Unsaved edit".into();

        let result = if in_place {
            files.save(&edited, true, Some(&binary))
        } else {
            files.save_as(&alias, &edited, true, Some(&binary))
        };
        assert_eq!(
            result
                .expect_err("An alias of the open binary must not be a template destination")
                .code,
            hexforge_core::error::ErrorCode::DestinationIsSource,
        );
        assert_eq!(files.path(), Some(bound.as_path()));
        assert_eq!(std::fs::read(&binary).unwrap(), original);
        assert_eq!(std::fs::read(&alias).unwrap(), original);
        assert_eq!(std::fs::read(&current).unwrap(), original);
        assert!(same_file::is_same_file(&binary, &alias).unwrap());
        // Without a protected open binary the unchanged checkpoint must still
        // permit an ordinary save, rather than inventing an external edit.
        files.save(&edited, false, None).unwrap();
    }
}
