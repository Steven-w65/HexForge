//! Parsed navigation owns only the cyan range, not unsaved edit markers.
#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    controller::{AppController, ControllerEvent, Effect},
};
use hexforge_core::{
    dto::{ModifiedOverviewDto, PageResponse, SearchResponse},
    operations::{CoreReply, CoreRequest},
    template::TemplateDefinition,
};
use support::*;

fn marked() -> AppController {
    let mut c = opened(8192, true);
    let (page, _) = run(c.handle(ControllerEvent::Action(Action::ReadViewport {
        offset: 0,
        length: 32,
    })));
    finish(
        &mut c,
        page,
        CoreReply::Page(PageResponse {
            offset: "0".into(),
            bytes: vec![0x42; 32],
            modified_offsets: vec!["0".into()],
            revision: "0".into(),
        }),
    );
    let (overview, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::GetModifiedOverview,
    ))));
    finish(
        &mut c,
        overview,
        CoreReply::ModifiedOverview(ModifiedOverviewDto {
            bin_count: 1024,
            bins: vec![0],
        }),
    );
    c.handle(ControllerEvent::Action(Action::NavigateTemplate {
        start: 4,
        end: 7,
    }));
    let s = snapshot(&c);
    let range = s.template_range.unwrap();
    assert_eq!((range.start, range.end, range.count()), (4, 7, 4));
    assert_eq!(s.selection.unwrap().start, 4);
    c
}
fn assert_edits(c: &AppController) {
    let s = snapshot(c);
    assert!(s.file.unwrap().dirty);
    assert_eq!(s.modified_overview.bins, [0]);
    assert_eq!(s.page.unwrap().modified_offsets, ["0"]);
}
fn definition(name: &str) -> TemplateDefinition {
    serde_json::from_value(
        serde_json::json!({ "name": name, "defaultEndianness": "little", "fields": [] }),
    )
    .unwrap()
}

#[test]
fn clearing_or_replacing_a_parsed_selection_preserves_all_unsaved_byte_markers() {
    for clear in [true, false] {
        let mut c = marked();
        let effects = c.handle(ControllerEvent::Action(if clear {
            Action::ClearSelection
        } else {
            Action::Select { start: 8, end: 8 }
        }));
        assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
        let s = snapshot(&c);
        assert!(s.template_range.is_none());
        if clear {
            assert!(s.selection.is_none());
        } else {
            let selection = s.selection.unwrap();
            assert_eq!(
                (selection.start, selection.end, selection.count()),
                (8, 8, 1)
            );
        }
        assert_edits(&c);
    }
}

#[test]
fn goto_and_successful_search_remove_only_the_template_range() {
    for search in [false, true] {
        let mut c = marked();
        if search {
            let (stamp, _) = run(c.handle(ControllerEvent::Action(Action::Core(
                CoreRequest::SearchBytes {
                    pattern: "42".into(),
                },
            ))));
            finish(
                &mut c,
                stamp,
                CoreReply::Search(SearchResponse {
                    matches: vec!["16".into()],
                    truncated: false,
                }),
            );
        } else {
            c.handle(ControllerEvent::Action(Action::SetGotoValue("0x10".into())));
            c.handle(ControllerEvent::Action(Action::SubmitGoto));
        }
        let s = snapshot(&c);
        assert!(s.template_range.is_none());
        let selection = s.selection.unwrap();
        assert_eq!(
            (selection.start, selection.end, selection.count()),
            (16, 16, 1)
        );
        assert_eq!(s.viewport_offset, 16);
        assert_edits(&c);
    }
}

#[test]
fn scrolling_invalid_goto_and_a_search_without_matches_preserve_the_parsed_range() {
    let mut c = marked();
    c.handle(ControllerEvent::Action(Action::ScrollViewport {
        offset: 16,
    }));
    c.handle(ControllerEvent::Action(Action::SetGotoValue(
        "invalid".into(),
    )));
    let effects = c.handle(ControllerEvent::Action(Action::SubmitGoto));
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert!(snapshot(&c).goto.error.is_some());
    let (stamp, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::SearchBytes {
            pattern: "FF".into(),
        },
    ))));
    finish(
        &mut c,
        stamp,
        CoreReply::Search(SearchResponse {
            matches: vec![],
            truncated: false,
        }),
    );
    let s = snapshot(&c);
    let range = s.template_range.unwrap();
    assert_eq!((range.start, range.end, range.count()), (4, 7, 4));
    assert_eq!(s.selection.unwrap().start, 4);
    assert_eq!(s.viewport_offset, 16);
    assert_edits(&c);
}

#[test]
fn template_changes_clear_the_owned_selection_but_never_an_independent_byte_selection() {
    for load in [false, true] {
        for ordinary in [false, true] {
            let mut c = marked();
            if ordinary {
                c.handle(ControllerEvent::Action(Action::Select { start: 8, end: 8 }));
            }
            if load {
                let (stamp, _) = run(c.handle(ControllerEvent::Action(Action::Core(
                    CoreRequest::LoadTemplate {
                        path: "new.json".into(),
                    },
                ))));
                finish(&mut c, stamp, CoreReply::Template(definition("New")));
            } else {
                c.handle(ControllerEvent::Action(Action::TemplateChanged(
                    definition("Changed"),
                )));
            }
            let s = snapshot(&c);
            assert!(s.template_range.is_none());
            assert_eq!(
                s.selection.map(|selection| selection.start),
                ordinary.then_some(8)
            );
            assert_edits(&c);
        }
    }
}
