use hexforge_controller::results::{ResultAction, ResultKey, ResultTree, ResultViewport};
use hexforge_core::template::{Diagnostic, Endian, FieldType, ParsedNode};

fn leaf(path: &str, offset: Option<&str>, length: Option<&str>) -> ParsedNode {
    ParsedNode {
        kind: "leaf".into(),
        path: path.into(),
        name: path.into(),
        field_type: FieldType::U8,
        offset: offset.map(str::to_owned),
        length: length.map(str::to_owned),
        value: Some("1".into()),
        endianness: Some(Endian::Little),
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    }
}
#[test]
fn parsed_error_and_zero_length_never_navigate() {
    let mut bad = leaf("bad", Some("10"), Some("1"));
    bad.diagnostics = vec![Diagnostic {
        code: "truncated".into(),
        message: "missing byte".into(),
    }];
    let mut tree = ResultTree::default();
    tree.replace(vec![
        bad,
        leaf("zero", Some("2"), Some("0")),
        leaf("overflow", Some("18446744073709551615"), Some("2")),
    ]);
    for path in ["bad", "zero", "overflow"] {
        assert_eq!(tree.activate(path), None);
    }
    assert!(tree.navigable_leaves().is_empty());
    tree.activate("bad");
    assert_eq!(tree.diagnostic_node().unwrap().path, "bad");
}
#[test]
fn leaf_navigation_highlights_exact_range() {
    let mut tree = ResultTree::default();
    tree.replace(vec![leaf("header.id", Some("9007199254740993"), Some("8"))]);
    assert_eq!(
        tree.activate("header.id"),
        Some(ResultAction::Navigate {
            start: 9_007_199_254_740_993,
            end: 9_007_199_254_741_000
        })
    );
}
#[test]
fn result_focus_and_expansion_survive_virtualization() {
    let mut tree = ResultTree::default();
    let nodes: Vec<_> = (0..200)
        .map(|i| leaf(&format!("item{i}"), Some("0"), Some("1")))
        .collect();
    tree.replace(nodes.clone());
    tree.handle_key(ResultKey::End);
    assert_eq!(tree.active_path(), "item199");
    let visible = tree.visible_nodes(ResultViewport {
        scroll_top: 5_432.0,
        height: 168.0,
    });
    assert!(visible.len() <= 18);
    assert!(visible.iter().any(|r| r.node.path == "item199"));
    tree.replace(nodes);
    assert_eq!(tree.active_path(), "item199");
    tree.handle_key(ResultKey::Home);
    assert_eq!(tree.active_path(), "item0");
}
#[test]
fn large_arrays_start_collapsed_and_structs_expand() {
    let mut structure = leaf("header", Some("0"), Some("1"));
    structure.kind = "struct".into();
    structure.children = vec![leaf("header.id", Some("0"), Some("1"))];
    let mut array = leaf("entries", Some("0"), Some("33"));
    array.kind = "array".into();
    array.children = (0..33)
        .map(|i| leaf(&format!("entries[{i}]"), Some("0"), Some("1")))
        .collect();
    let mut tree = ResultTree::default();
    tree.replace(vec![structure.clone(), array.clone()]);
    assert_eq!(tree.row_count(), 3);
    tree.activate("entries");
    assert_eq!(tree.row_count(), 36);
    tree.replace(vec![structure, array]);
    assert_eq!(tree.row_count(), 36);
    tree.activate("header");
    assert_eq!(tree.active_path(), "header");
}
#[test]
fn tree_keys_preserve_parent_child_and_activation_rules() {
    let mut parent = leaf("header", Some("0"), Some("1"));
    parent.kind = "struct".into();
    parent.children = vec![leaf("header.id", Some("0"), Some("1"))];
    let mut tree = ResultTree::default();
    tree.replace(vec![parent]);
    tree.handle_key(ResultKey::Right);
    assert_eq!(tree.active_path(), "header.id");
    assert_eq!(
        tree.handle_key(ResultKey::Enter),
        Some(ResultAction::Navigate { start: 0, end: 0 })
    );
    tree.handle_key(ResultKey::Left);
    assert_eq!(tree.active_path(), "header");
    tree.handle_key(ResultKey::Left);
    assert_eq!(tree.row_count(), 1);
    tree.handle_key(ResultKey::Right);
    assert_eq!(tree.row_count(), 2);
}

#[test]
fn nested_errors_never_become_complete_navigation_leaves() {
    use hexforge_controller::template::workspace::has_diagnostics;
    let mut id = leaf("items[2].id", Some("9007199254740993"), Some("8"));
    id.name = "id".into();
    id.field_type = FieldType::U64;
    id.endianness = Some(Endian::Big);
    let mut root = id.clone();
    root.kind = "array".into();
    root.name = "items".into();
    root.path = "items".into();
    root.field_type = FieldType::Array;
    root.value = None;
    root.children = vec![id.clone()];
    assert!(!has_diagnostics(&[root.clone()]));
    let mut tree = ResultTree::default();
    tree.replace(vec![root.clone()]);
    assert_eq!(tree.navigable_leaves(), [&id]);
    assert_eq!(
        tree.activate("items[2].id"),
        Some(ResultAction::Navigate {
            start: 9_007_199_254_740_993,
            end: 9_007_199_254_741_000
        })
    );
    let mut error = id.clone();
    error.kind = "error".into();
    error.path = "items[3].id".into();
    error.value = None;
    error.diagnostics = vec![Diagnostic {
        code: "template_out_of_bounds".into(),
        message: "items[3].id: beyond file".into(),
    }];
    root.children.push(error);
    assert!(has_diagnostics(&[root.clone()]));
    tree.replace(vec![root]);
    assert_eq!(tree.navigable_leaves(), [&id]);
    assert_eq!(tree.activate("items[3].id"), None);
    assert_eq!(
        tree.diagnostic_node().unwrap().diagnostics[0].message,
        "items[3].id: beyond file"
    );
}
