#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, Effect},
    dialog::PickerPurpose,
};
use hexforge_core::{
    dto::DirtyState,
    operations::{CoreReply, CoreRequest},
    template::{FieldType, ParsedNode},
};
use support::*;
#[test]
fn invalid_template_disabled_reason_includes_all_issue_count() {
    use hexforge_controller::policy::command_unavailable_reason;
    let mut c = opened(16, false);
    let template=serde_json::from_value(serde_json::json!({"name":"Bad","defaultEndianness":"little","fields":[{"name":"","type":"u8","placement":{"mode":"sequential"}}]})).unwrap();
    c.handle(ControllerEvent::Action(Action::TemplateChanged(template)));
    assert_eq!(
        command_unavailable_reason(Command::ApplyTemplate, &snapshot(&c).policy).as_deref(),
        Some("Correct 1 template error first.")
    );
}
#[test]
fn edit_dialog_blocks_background_selection_and_navigation_gestures() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::Select { start: 3, end: 3 }));
    invoke(&mut c, Command::ToggleEdit);
    invoke(&mut c, Command::EditSelected);
    for action in [
        Action::Select { start: 5, end: 5 },
        Action::Navigate { start: 7, end: 7 },
        Action::NavigateTemplate { start: 9, end: 9 },
        Action::ScrollViewport { offset: 8 },
    ] {
        c.handle(ControllerEvent::Action(action));
        assert_eq!(snapshot(&c).selection.unwrap().start, 3);
        assert_eq!(snapshot(&c).viewport_offset, 0);
    }
    assert!(snapshot(&c).template_range.is_none());
}
#[test]
fn byte_edit_dialog_validates_and_closes_only_after_success() {
    let mut c = opened(16, false);
    c.handle(ControllerEvent::Action(Action::Select { start: 3, end: 3 }));
    invoke(&mut c, Command::EditSelected);
    assert!(snapshot(&c).edit_dialog.is_none());
    invoke(&mut c, Command::ToggleEdit);
    invoke(&mut c, Command::EditSelected);
    assert_eq!(snapshot(&c).edit_dialog.unwrap().offset, 3);
    c.handle(ControllerEvent::Action(Action::EditSelectedByte(
        "XX".into(),
    )));
    assert!(snapshot(&c).edit_dialog.unwrap().error.is_some());
    let (stamp, request) = run(c.handle(ControllerEvent::Action(Action::EditSelectedByte(
        "FF".into(),
    ))));
    assert!(matches!(
        request,
        CoreRequest::EditByte {
            offset: 3,
            value: 255
        }
    ));
    assert!(snapshot(&c).edit_dialog.is_some());
    assert!(!c
        .handle(ControllerEvent::Action(Action::EditSelectedByte(
            "11".into()
        )))
        .iter()
        .any(|e| matches!(e, Effect::RunCore { .. })));
    finish(
        &mut c,
        stamp,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    assert!(snapshot(&c).edit_dialog.is_none());
    assert!(snapshot(&c).file.unwrap().dirty);
}
fn parsed_controller() -> hexforge_controller::controller::AppController {
    let mut c = opened(16, false);
    let template=serde_json::from_value(serde_json::json!({"name":"Header","defaultEndianness":"little","fields":[{"name":"id","type":"u8","placement":{"mode":"sequential"}}]})).unwrap();
    c.handle(ControllerEvent::Action(Action::TemplateChanged(template)));
    let (stamp, _) = run(invoke(&mut c, Command::ApplyTemplate));
    finish(
        &mut c,
        stamp,
        CoreReply::Parsed(vec![ParsedNode {
            kind: "leaf".into(),
            name: "id".into(),
            path: "id".into(),
            field_type: FieldType::U8,
            offset: Some("0".into()),
            length: Some("1".into()),
            value: Some("0".into()),
            endianness: None,
            comment: String::new(),
            enum_label: None,
            flags: vec![],
            diagnostics: vec![],
            children: vec![],
        }]),
    );
    c
}
#[test]
fn csv_export_is_a_single_owned_picker_and_stamped_operation() {
    let mut c = parsed_controller();
    let effects = invoke(&mut c, Command::Export);
    let (id, picker) = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, picker } = e {
                Some((request_id, picker))
            } else {
                None
            }
        })
        .expect("CSV picker");
    assert_eq!(picker.purpose, PickerPurpose::Csv);
    assert_eq!(picker.default_path.unwrap().to_str(), Some("Header.csv"));
    assert!(!invoke(&mut c, Command::Export)
        .iter()
        .any(|e| matches!(e, Effect::OpenPicker { .. })));
    let (stamp, request) = run(c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: Some("out.csv".into()),
    }));
    assert!(
        matches!(request,CoreRequest::ExportResultsCsv{path,..} if path.to_str()==Some("out.csv"))
    );
    finish(&mut c, stamp, CoreReply::Unit);
    assert_eq!(
        snapshot(&c).completion_notice.as_deref(),
        Some("CSV exported: out.csv.")
    );
    assert!(!snapshot(&c).file.unwrap().dirty);
    assert!(snapshot(&c).template_applied);
}
#[test]
fn cancelled_csv_picker_preserves_results_and_releases_busy() {
    let mut c = parsed_controller();
    let before = snapshot(&c);
    let id = invoke(&mut c, Command::Export)
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenPicker { request_id, .. } = e {
                Some(request_id)
            } else {
                None
            }
        })
        .expect("CSV picker");
    c.handle(ControllerEvent::PickerCompleted {
        request_id: id,
        path: None,
    });
    let after = snapshot(&c);
    assert_eq!(after.results, before.results);
    assert!(after.template_applied);
    assert!(!after.policy.operation_busy);
}
#[test]
fn already_open_goto_bar_can_navigate_during_background_scan() {
    let mut c = opened(32, false);
    invoke(&mut c, Command::Goto);
    run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::SearchBytes {
            pattern: "00".into(),
        },
    ))));
    c.handle(ControllerEvent::Action(Action::SetGotoValue("15".into())));
    c.handle(ControllerEvent::Action(Action::SubmitGoto));
    assert_eq!(snapshot(&c).selection.unwrap().start, 15);
    assert!(!snapshot(&c).goto.open);
}
#[test]
fn expiring_old_notice_never_clears_a_newer_identical_notice() {
    let mut c = parsed_controller();
    let mut revisions = vec![];
    for _ in 0..2 {
        let id = invoke(&mut c, Command::Export)
            .into_iter()
            .find_map(|e| {
                if let Effect::OpenPicker { request_id, .. } = e {
                    Some(request_id)
                } else {
                    None
                }
            })
            .unwrap();
        let (stamp, _) = run(c.handle(ControllerEvent::PickerCompleted {
            request_id: id,
            path: Some("out.csv".into()),
        }));
        finish(&mut c, stamp, CoreReply::Unit);
        revisions.push(snapshot(&c).notice_revision);
    }
    c.handle(ControllerEvent::Action(Action::ExpireNotice {
        revision: revisions[0],
    }));
    assert_eq!(
        snapshot(&c).completion_notice.as_deref(),
        Some("CSV exported: out.csv.")
    );
    c.handle(ControllerEvent::Action(Action::ExpireNotice {
        revision: revisions[1],
    }));
    assert!(snapshot(&c).completion_notice.is_none());
}
