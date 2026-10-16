#[path = "support/controller.rs"]
mod support;
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, Effect},
    goto::parse_goto,
    search::{parse_search_pattern, MatchDirection, SearchState},
};
use hexforge_core::{
    dto::{DirtyState, SearchResponse},
    operations::{CoreReply, CoreRequest},
};
use support::*;

#[test]
fn clear_search_preserves_edited_markers() {
    let mut c = opened(64, false);
    let (edit, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::EditByte {
            offset: 16,
            value: 0x42,
        },
    ))));
    finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    c.handle(ControllerEvent::Action(Action::SetSearchPattern(
        "41 42".into(),
    )));
    let (search, _) = run(c.handle(ControllerEvent::Action(Action::SubmitSearch {
        force: true,
        direction: MatchDirection::Next,
    })));
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec!["16".into(), "32".into()],
            truncated: true,
        }),
    );
    assert_eq!(snapshot(&c).selection.as_ref().unwrap().end, 17);
    c.handle(ControllerEvent::Action(Action::ClearSearch));
    let s = snapshot(&c);
    assert!(s.matches.is_empty());
    assert!(s.search.raw.is_empty());
    assert!(s.search.query.is_empty());
    assert_eq!(s.search.match_length, 1);
    assert!(!s.search.truncated);
    assert!(s.file.unwrap().dirty);
    assert_eq!(s.last_edit_delta.unwrap().0, 16);
    let selection = s.selection.unwrap();
    assert_eq!(
        (selection.start, selection.end, selection.count()),
        (16, 17, 2)
    );
    assert!(s.search.warning().is_none());
}

#[test]
fn new_pattern_cannot_navigate_old_matches() {
    let mut c = opened(64, false);
    c.handle(ControllerEvent::Action(Action::SetSearchPattern(
        "41".into(),
    )));
    let (search, _) = run(c.handle(ControllerEvent::Action(Action::SubmitSearch {
        force: true,
        direction: MatchDirection::Next,
    })));
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec!["16".into(), "32".into()],
            truncated: false,
        }),
    );
    c.handle(ControllerEvent::Action(Action::SetSearchPattern(
        "42".into(),
    )));
    let effects = invoke(&mut c, Command::SearchNext);
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert_eq!(snapshot(&c).selection.unwrap().start, 16);
    assert!(!snapshot(&c).policy.has_search_matches);
}

#[test]
fn limited_search_reports_current_warning() {
    let mut search = SearchState::default();
    search.set_pattern("41 42");
    search.install("41 42", vec![16, 32], 2, true);
    assert_eq!(
        search.warning(),
        Some("Search results were limited; refine the byte pattern.")
    );
    search.set_pattern("GG");
    assert!(search.warning().is_none());
    search.set_pattern("41 42");
    assert!(search.warning().is_some());
    search.clear();
    assert!(search.warning().is_none());
}

#[test]
fn goto_accepts_decimal_and_hex_without_precision_loss() {
    assert_eq!(parse_goto("42", 100).unwrap(), 42);
    assert_eq!(parse_goto("0X2a", 100).unwrap(), 42);
    assert_eq!(
        parse_goto("9007199254740993", 9007199254740994).unwrap(),
        9007199254740993
    );
    assert_eq!(
        parse_goto("0x20000000000001", 9007199254740994).unwrap(),
        9007199254740993
    );
    for input in ["", " ", "-1", "+1", "0x", "0x2g", "12x", " 42", "42 "] {
        assert_eq!(
            parse_goto(input, 100).unwrap_err().message,
            "Offset must be a decimal or 0x-prefixed hexadecimal integer"
        );
    }
    for (input, size) in [("100", 100), ("0", 0), ("18446744073709551616", u64::MAX)] {
        assert_eq!(
            parse_goto(input, size).unwrap_err().message,
            "Offset is outside the file"
        );
    }
}

#[test]
fn pattern_validation_preserves_ascii_whitespace_and_exact_edit_rules() {
    assert_eq!(
        parse_search_pattern("41 42\tff\n00").unwrap(),
        [0x41, 0x42, 0xff, 0]
    );
    for input in ["", "   "] {
        assert_eq!(
            parse_search_pattern(input).unwrap_err().message,
            "At least one byte is required"
        );
    }
    for input in ["4", "GG", "000", "41-42", "41\u{00a0}42"] {
        assert_eq!(
            parse_search_pattern(input).unwrap_err().message,
            "Bytes must be two hexadecimal digits separated by ASCII whitespace"
        );
    }
}
#[test]
fn every_reference_offset_pattern_and_single_byte_literal_retains_strict_parsing() {
    use hexforge_controller::hex::input::{parse_hex_bytes, parse_offset, parse_single_byte};
    for text in ["42", "0x2A", "0X2a"] {
        assert_eq!(parse_offset(text, 100).unwrap(), 42);
    }
    for text in ["", "   ", "-1", "+1", "0x", "0x2g", "12x", "100"] {
        assert!(parse_offset(text, 100).is_err(), "{text:?}");
    }
    assert!(parse_offset("0", 0).is_err());
    for text in ["9007199254740993", "0x20000000000001"] {
        assert_eq!(
            parse_offset(text, 9007199254740994).unwrap(),
            9007199254740993
        );
    }
    assert_eq!(
        parse_hex_bytes("41 42\tff\n00").unwrap(),
        [0x41, 0x42, 0xff, 0]
    );
    assert_eq!(parse_hex_bytes("  41\n42  ").unwrap(), [0x41, 0x42]);
    for text in ["", "   ", "4", "GG", "0", "000", "41-42"] {
        assert!(parse_hex_bytes(text).is_err(), "{text:?}");
    }
    for (text, byte) in [("00", 0), ("aF", 0xaf), ("FF", 255)] {
        assert_eq!(parse_single_byte(text).unwrap(), byte);
    }
    for text in ["", "F", "FFF", "GG", "0xFF", " FF"] {
        assert!(parse_single_byte(text).is_err(), "{text:?}");
    }
}

#[test]
fn navigation_wraps_from_manual_selection_and_stays_exact_above_two_to_53() {
    let mut search = SearchState::default();
    search.set_pattern("41 42 43");
    search.install(
        "41 42 43",
        vec![9007199254740993, 9007199254741001],
        3,
        true,
    );
    search.set_anchor(None, 0);
    assert_eq!(
        search.next(MatchDirection::Next).unwrap().start,
        9007199254740993
    );
    assert_eq!(
        search.next(MatchDirection::Next).unwrap().end,
        9007199254741003
    );
    assert_eq!(
        search.next(MatchDirection::Next).unwrap().start,
        9007199254740993
    );
    assert_eq!(
        search.next(MatchDirection::Previous).unwrap().start,
        9007199254741001
    );
    let first = search.next(MatchDirection::Previous).unwrap();
    assert_eq!(
        (first.start, first.end),
        (9007199254740993, 9007199254740995)
    );
    assert!(search.truncated);
}

#[test]
fn controller_search_navigation_wraps_without_scanning_and_keeps_manual_anchors() {
    let mut c = opened(8192, false);
    c.handle(ControllerEvent::Action(Action::SetSearchPattern(
        "41 42".into(),
    )));
    let (search, _) = run(c.handle(ControllerEvent::Action(Action::SubmitSearch {
        force: true,
        direction: MatchDirection::Next,
    })));
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec!["16".into(), "32".into()],
            truncated: false,
        }),
    );
    let assert_match = |c: &hexforge_controller::controller::AppController, start, end, index| {
        let s = snapshot(c);
        let selection = s.selection.as_ref().unwrap();
        assert_eq!(
            (selection.start, selection.end, selection.count()),
            (start, end, 2)
        );
        assert_eq!(s.search.active_index(s.selection.as_ref()), Some(index));
        assert_eq!(s.viewport_offset, start);
    };
    assert_match(&c, 16, 17, 0);
    for (command, start, end, index) in [
        (Command::SearchNext, 32, 33, 1),
        (Command::SearchNext, 16, 17, 0),
        (Command::SearchPrevious, 32, 33, 1),
    ] {
        let effects = invoke(&mut c, command);
        assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
        assert_match(&c, start, end, index);
    }
    c.handle(ControllerEvent::Action(Action::Select {
        start: 24,
        end: 26,
    }));
    let s = snapshot(&c);
    assert_eq!(s.search.active_index(s.selection.as_ref()), None);
    invoke(&mut c, Command::SearchPrevious);
    assert_match(&c, 16, 17, 0);
    c.handle(ControllerEvent::Action(Action::ScrollViewport {
        offset: 4096,
    }));
    let s = snapshot(&c);
    assert_eq!(s.search.active_index(s.selection.as_ref()), Some(0));
    invoke(&mut c, Command::SearchNext);
    assert_match(&c, 32, 33, 1);
    c.handle(ControllerEvent::Action(Action::ClearSearch));
    let effects = invoke(&mut c, Command::SearchNext);
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let s = snapshot(&c);
    assert_eq!(s.search.active_index(s.selection.as_ref()), None);
    assert_eq!(s.selection.unwrap().start, 32);
}

#[test]
fn selection_changed_during_search_is_not_replaced() {
    let mut c = opened(64, false);
    c.handle(ControllerEvent::Action(Action::SetSearchPattern(
        "41".into(),
    )));
    let (search, _) = run(c.handle(ControllerEvent::Action(Action::SubmitSearch {
        force: true,
        direction: MatchDirection::Next,
    })));
    c.handle(ControllerEvent::Action(Action::Select {
        start: 48,
        end: 49,
    }));
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec!["16".into()],
            truncated: false,
        }),
    );
    assert_eq!(snapshot(&c).matches, [16]);
    let selected = snapshot(&c).selection.unwrap();
    assert_eq!(
        (selected.start, selected.end, selected.count()),
        (48, 49, 2)
    );
}

#[test]
fn pending_search_blocks_repeated_submit_and_match_navigation_in_every_input_path() {
    let mut c = opened(64, false);
    c.handle(ControllerEvent::Action(Action::SetSearchPattern(
        "41 42".into(),
    )));
    let (stamp, _) = run(c.handle(ControllerEvent::Action(Action::SubmitSearch {
        force: true,
        direction: MatchDirection::Next,
    })));
    for (force, direction) in [
        (false, MatchDirection::Next),
        (false, MatchDirection::Previous),
        (true, MatchDirection::Next),
    ] {
        let effects = c.handle(ControllerEvent::Action(Action::SubmitSearch {
            force,
            direction,
        }));
        assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    }
    for command in [Command::SearchNext, Command::SearchPrevious] {
        let effects = invoke(&mut c, command);
        assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
        assert!(!hexforge_controller::policy::command_enabled(
            command,
            &snapshot(&c).policy
        ));
    }
    finish(
        &mut c,
        stamp,
        CoreReply::Search(SearchResponse {
            matches: vec!["4".into()],
            truncated: false,
        }),
    );
    let selected = snapshot(&c).selection.unwrap();
    assert_eq!((selected.start, selected.end), (4, 5));
}

#[test]
fn edit_refresh_keeps_current_page_generation_and_modified_overview() {
    use hexforge_core::dto::{ModifiedOverviewDto, PageResponse};
    let mut c = opened(8192, false);
    let (page, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 0,
            length: 32,
        },
    ))));
    finish(
        &mut c,
        page,
        CoreReply::Page(PageResponse {
            offset: "0".into(),
            bytes: vec![0x41; 32],
            modified_offsets: vec![],
            revision: "0".into(),
        }),
    );
    let generation = snapshot(&c).page.unwrap().generation;
    let (edit, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::EditByte {
            offset: 0,
            value: 0xff,
        },
    ))));
    let effects = finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    let mut read = None;
    let mut overview = None;
    for effect in effects {
        if let Effect::RunCore { stamp, request } = effect {
            match request {
                CoreRequest::ReadPage {
                    offset: 0,
                    length: 32,
                } => read = Some(stamp),
                CoreRequest::GetModifiedOverview => overview = Some(stamp),
                _ => {}
            }
        }
    }
    let read = read.expect("refresh the currently displayed page");
    finish(
        &mut c,
        read,
        CoreReply::Page(PageResponse {
            offset: "0".into(),
            bytes: vec![0xff; 32],
            modified_offsets: vec!["0".into()],
            revision: "1".into(),
        }),
    );
    finish(
        &mut c,
        overview.expect("file-wide markers refresh"),
        CoreReply::ModifiedOverview(ModifiedOverviewDto {
            bin_count: 1024,
            bins: vec![0],
        }),
    );
    assert_eq!(snapshot(&c).page.as_ref().unwrap().generation, generation);
    assert_eq!(snapshot(&c).page.unwrap().bytes[0], 0xff);
    assert_eq!(snapshot(&c).modified_overview.bins, [0]);
}

#[test]
fn edit_completion_cannot_refresh_a_superseded_viewport() {
    use hexforge_core::dto::PageResponse;
    let mut c = opened(8192, false);
    let (page, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 0,
            length: 32,
        },
    ))));
    finish(
        &mut c,
        page,
        CoreReply::Page(PageResponse {
            offset: "0".into(),
            bytes: vec![0x41; 32],
            modified_offsets: vec![],
            revision: "0".into(),
        }),
    );
    let (edit, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::EditByte {
            offset: 0,
            value: 0xff,
        },
    ))));
    let (new_page, _) = run(c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::ReadPage {
            offset: 4096,
            length: 32,
        },
    ))));
    let effects = finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::RunCore {
            request: CoreRequest::ReadPage { .. },
            ..
        }
    )));
    finish(
        &mut c,
        new_page,
        CoreReply::Page(PageResponse {
            offset: "4096".into(),
            bytes: vec![0x42; 32],
            modified_offsets: vec![],
            revision: "1".into(),
        }),
    );
    assert_eq!(snapshot(&c).page.unwrap().offset, "4096");
}
