use hexforge_controller::command::Command;
use hexforge_controller::policy::{command_availability, PolicyState};
use serde::Deserialize;

#[derive(Deserialize)]
struct Cases {
    flags: Vec<String>,
    commands: Vec<Command>,
    reasons: Vec<Option<String>>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    mask: u16,
    issue_count: Option<u32>,
    result: Vec<usize>,
}

#[test]
fn every_command_branch_matches_reference() {
    let fixture: Cases = serde_json::from_str(include_str!("fixtures/command-cases.json")).unwrap();
    assert_eq!(fixture.flags.len(), 12);
    assert_eq!(fixture.commands.len(), 30);
    assert_eq!(fixture.cases.len(), 12_288);
    for case in fixture.cases {
        let mut json = serde_json::Map::new();
        for (bit, name) in fixture.flags.iter().enumerate() {
            json.insert(name.clone(), (case.mask & (1 << bit) != 0).into());
        }
        if let Some(count) = case.issue_count {
            json.insert("templateIssueCount".into(), count.into());
        }
        let state: PolicyState = serde_json::from_value(json.into()).unwrap();
        for (index, command) in fixture.commands.iter().enumerate() {
            let result = command_availability(*command, &state);
            assert_eq!(
                result.reason, fixture.reasons[case.result[index]],
                "{command:?}, mask {}, issues {:?}",
                case.mask, case.issue_count
            );
            assert_eq!(result.enabled, result.reason.is_none());
        }
    }
}
