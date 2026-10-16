//! These races catch wrong identity/token checks, mutation ordering and activity ownership.
use hexforge_controller::{
    action::Action,
    controller::{AppController, ControllerEvent, Effect, InitialState, WindowSnapshot},
    identity::{CheckedSequence, RequestStamp},
    operation::{OperationKind, OperationTracker},
    shortcuts::WindowKind,
};
use hexforge_core::{
    dto::{DirtyState, FileInfoDto, PageResponse, SearchResponse},
    error::{AppError, ErrorCode},
    operations::{CoreReply, CoreRequest},
    progress::{OperationPhase, OperationProgress},
    template::TemplateDefinition,
};

fn run(controller: &mut AppController, request: CoreRequest) -> (RequestStamp, CoreRequest) {
    next_run(controller.handle(ControllerEvent::Action(Action::Core(request))))
}
fn next_run(effects: Vec<Effect>) -> (RequestStamp, CoreRequest) {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::RunCore { stamp, request } => Some((stamp, request)),
            _ => None,
        })
        .expect("one core operation must start")
}
fn finish(controller: &mut AppController, stamp: RequestStamp, reply: CoreReply) -> Vec<Effect> {
    controller.handle(ControllerEvent::CoreCompleted {
        stamp,
        reply: Ok(reply),
    })
}
fn file(path: &str) -> CoreReply {
    CoreReply::FileInfo(FileInfoDto {
        name: path.into(),
        path: path.into(),
        size: "1024".into(),
        revision: "0".into(),
        dirty: false,
    })
}
fn opened(controller: &mut AppController, path: &str) {
    let (stamp, _) = run(
        controller,
        CoreRequest::OpenFile {
            path: path.into(),
            discard_unsaved: false,
        },
    );
    finish(controller, stamp, file(path));
}
fn main(controller: &AppController) -> hexforge_controller::controller::MainSnapshot {
    match controller.snapshot(WindowKind::Main) {
        WindowSnapshot::Main(snapshot) => snapshot,
        _ => panic!("main snapshot"),
    }
}
fn template() -> TemplateDefinition {
    serde_json::from_str(r#"{"name":"Example","defaultEndianness":"little","fields":[]}"#).unwrap()
}

#[test]
fn same_path_reopen_rejects_previous_source() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "same.bin");
    let first_source = main(&c).source;
    let (page, _) = run(
        &mut c,
        CoreRequest::ReadPage {
            offset: 0,
            length: 16,
        },
    );
    let (search, _) = run(
        &mut c,
        CoreRequest::SearchBytes {
            pattern: "41".into(),
        },
    );
    let (parse, _) = run(
        &mut c,
        CoreRequest::ApplyTemplate {
            template: template(),
        },
    );
    opened(&mut c, "same.bin");
    assert_ne!(first_source, main(&c).source);
    assert_eq!(main(&c).file.as_ref().unwrap().revision, "0");
    finish(
        &mut c,
        page,
        CoreReply::Page(PageResponse {
            offset: "0".into(),
            bytes: vec![0x41],
            modified_offsets: vec![],
            revision: "0".into(),
        }),
    );
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec!["0".into()],
            truncated: false,
        }),
    );
    finish(&mut c, parse, CoreReply::Parsed(vec![]));
    let snapshot = main(&c);
    assert!(snapshot.page.is_none());
    assert!(snapshot.matches.is_empty());
    assert!(!snapshot.template_applied);
    assert!(snapshot.activity.is_none());
}

#[test]
fn auxiliary_source_read_errors_are_bound_to_the_current_identity_and_revision() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "same.bin");
    let original = main(&c).source.unwrap();
    let error = AppError::new(
        ErrorCode::SourceChanged,
        "The source file changed.",
        Some("sparse preview".into()),
    );
    c.handle(ControllerEvent::SourceReadFailed {
        source: original,
        revision: "0".into(),
        error: error.clone(),
    });
    assert_eq!(main(&c).error.as_ref().unwrap().code, "source_changed");
    assert_eq!(
        main(&c).error.as_ref().unwrap().detail.as_deref(),
        Some("sparse preview")
    );
    c.handle(ControllerEvent::Action(Action::ClearError));
    opened(&mut c, "same.bin");
    let replacement = main(&c).source.unwrap();
    assert_ne!(original, replacement);
    c.handle(ControllerEvent::SourceReadFailed {
        source: original,
        revision: "0".into(),
        error: error.clone(),
    });
    assert!(
        main(&c).error.is_none(),
        "An old auxiliary reader must not fail a same-path reopened file"
    );
    let (stamp, _) = run(
        &mut c,
        CoreRequest::EditByte {
            offset: 0,
            value: 0xff,
        },
    );
    finish(
        &mut c,
        stamp,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    c.handle(ControllerEvent::SourceReadFailed {
        source: replacement,
        revision: "0".into(),
        error: error.clone(),
    });
    assert!(
        main(&c).error.is_none(),
        "An earlier revision must not report over a completed edit"
    );
    c.handle(ControllerEvent::SourceReadFailed {
        source: replacement,
        revision: "1".into(),
        error,
    });
    let after = main(&c);
    assert_eq!(
        after.error.as_ref().unwrap().message,
        "The source file changed."
    );
    assert_eq!(after.source, Some(replacement));
    assert_eq!(after.file.as_ref().unwrap().path, "same.bin");
    assert_eq!(after.file.as_ref().unwrap().revision, "1");
    assert!(after.file.unwrap().dirty);
}

#[test]
fn older_viewport_cannot_override_new_intent() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "input.bin");
    c.handle(ControllerEvent::Action(Action::Select {
        start: 21,
        end: 21,
    }));
    let (old, _) = run(
        &mut c,
        CoreRequest::ReadPage {
            offset: 0,
            length: 16,
        },
    );
    let (new, _) = run(
        &mut c,
        CoreRequest::ReadPage {
            offset: 16,
            length: 16,
        },
    );
    finish(
        &mut c,
        new,
        CoreReply::Page(PageResponse {
            offset: "16".into(),
            bytes: vec![0x42],
            modified_offsets: vec![],
            revision: "0".into(),
        }),
    );
    c.handle(ControllerEvent::CoreCompleted {
        stamp: old,
        reply: Err(AppError::new(ErrorCode::IoError, "Old page failed.", None)),
    });
    let s = main(&c);
    assert_eq!(s.page.unwrap().offset, "16");
    assert_eq!(s.selection.unwrap().start, 21);
    assert!(s.error.is_none());
    assert!(!s.busy.get(&OperationKind::Page).copied().unwrap_or(false));
}

#[test]
fn mutation_queue_preserves_edits_before_open() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (edit, _) = run(
        &mut c,
        CoreRequest::EditByte {
            offset: 0,
            value: 0x42,
        },
    );
    let queued = c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::OpenFile {
            path: "b.bin".into(),
            discard_unsaved: true,
        },
    )));
    assert!(!queued.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let effects = finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    assert!(
        main(&c).file.unwrap().dirty,
        "successful earlier edits must not be dropped as stale"
    );
    let (open, request) = next_run(effects);
    assert!(
        matches!(request, CoreRequest::OpenFile { path, .. } if path.to_string_lossy() == "b.bin")
    );
    c.handle(ControllerEvent::CoreCompleted {
        stamp: open,
        reply: Err(AppError::new(
            ErrorCode::PermissionDenied,
            "B failed.",
            None,
        )),
    });
    assert_eq!(main(&c).file.as_ref().unwrap().path, "a.bin");
    assert!(main(&c).file.unwrap().dirty);
    assert_eq!(main(&c).error.unwrap().message, "B failed.");
}

#[test]
fn busy_activity_falls_back_after_latest_finishes() {
    let mut tracker = OperationTracker::default();
    let older = tracker.begin(OperationKind::Search).unwrap();
    let newer = tracker.begin(OperationKind::Parse).unwrap();
    tracker.report(
        older,
        OperationProgress {
            operation_id: "core-older".into(),
            phase: OperationPhase::Search,
            processed: "4".into(),
            total: "10".into(),
        },
    );
    assert_eq!(
        tracker.visible_activity().unwrap().operation,
        OperationKind::Parse
    );
    tracker.finish(newer);
    let active = tracker.visible_activity().unwrap();
    assert_eq!(active.operation, OperationKind::Search);
    assert_eq!(active.progress.unwrap().processed, "4");
    tracker.finish(newer); // Duplicate completions cannot underflow busy counters.
    assert!(tracker.is_busy(OperationKind::Search));
    tracker.finish(older);
    assert!(tracker.visible_activity().is_none());
    assert!(!tracker.is_busy(OperationKind::Search));
}

#[test]
fn controller_activity_and_progress_keep_one_owner_when_a_newer_save_finishes() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "source.bin");
    let (search, _) = run(
        &mut c,
        CoreRequest::SearchBytes {
            pattern: "41".into(),
        },
    );
    let progress = |id: &str, phase, processed: &str, total: &str| OperationProgress {
        operation_id: id.into(),
        phase,
        processed: processed.into(),
        total: total.into(),
    };
    c.handle(ControllerEvent::Progress {
        stamp: search.clone(),
        progress: progress("search", OperationPhase::Search, "4", "10"),
    });
    let (save, _) = run(
        &mut c,
        CoreRequest::SaveBinaryAs {
            path: "copy.bin".into(),
        },
    );
    assert_eq!(main(&c).activity.unwrap().operation, OperationKind::Save);
    assert!(main(&c).activity.unwrap().progress.is_none());
    c.handle(ControllerEvent::Progress {
        stamp: save.clone(),
        progress: progress("save", OperationPhase::Save, "2", "8"),
    });
    c.handle(ControllerEvent::Progress {
        stamp: search.clone(),
        progress: progress("search", OperationPhase::Search, "9", "10"),
    });
    let current = main(&c).activity.unwrap();
    assert_eq!(current.operation, OperationKind::Save);
    assert_eq!(current.progress.unwrap().processed, "2");
    let saved = hexforge_core::dto::SaveResponse {
        bytes_written: "1024".into(),
        destination: "copy.bin".into(),
        file: FileInfoDto {
            name: "copy.bin".into(),
            path: "copy.bin".into(),
            size: "1024".into(),
            revision: "0".into(),
            dirty: false,
        },
        state: DirtyState {
            dirty: false,
            revision: "0".into(),
        },
    };
    finish(&mut c, save.clone(), CoreReply::BinarySaved(saved));
    let previous = main(&c).activity.unwrap();
    assert_eq!(previous.operation, OperationKind::Search);
    assert_eq!(previous.progress.unwrap().processed, "9");
    c.handle(ControllerEvent::Progress {
        stamp: save,
        progress: progress("late-save", OperationPhase::Save, "8", "8"),
    });
    assert_eq!(
        main(&c).activity.unwrap().progress.unwrap().operation_id,
        "search"
    );
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec![],
            truncated: false,
        }),
    );
    assert!(main(&c).activity.is_none());
    assert!(main(&c).error.is_none());
}

#[test]
fn failed_newer_export_restores_the_pending_parse_and_its_own_progress() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "source.bin");
    let (parse, _) = run(
        &mut c,
        CoreRequest::ApplyTemplate {
            template: template(),
        },
    );
    c.handle(ControllerEvent::Progress {
        stamp: parse.clone(),
        progress: OperationProgress {
            operation_id: "parse".into(),
            phase: OperationPhase::Parse,
            processed: "1".into(),
            total: "0".into(),
        },
    });
    let (export, _) = run(
        &mut c,
        CoreRequest::ExportResultsCsv {
            path: "out.csv".into(),
            template: template(),
        },
    );
    assert_eq!(main(&c).activity.unwrap().operation, OperationKind::Export);
    assert!(main(&c).activity.unwrap().progress.is_none());
    c.handle(ControllerEvent::CoreCompleted {
        stamp: export,
        reply: Err(AppError::new(
            ErrorCode::PermissionDenied,
            "No output.",
            None,
        )),
    });
    let older = main(&c).activity.unwrap();
    assert_eq!(older.operation, OperationKind::Parse);
    assert_eq!(older.progress.unwrap().operation_id, "parse");
    assert!(main(&c).busy[&OperationKind::Parse]);
    assert_eq!(main(&c).error.unwrap().message, "No output.");
    finish(&mut c, parse, CoreReply::Parsed(vec![]));
    assert!(main(&c).activity.is_none());
    assert!(main(&c).template_applied);
    assert!(main(&c).results.is_empty());
}

#[test]
fn checked_ids_fail_without_wrapping_or_mutating() {
    let mut ids = CheckedSequence::new(u64::MAX);
    assert!(ids.next_id().is_err());
    assert!(ids.next_id().is_err());
    let mut ids = CheckedSequence::new(u64::MAX - 1);
    assert_eq!(ids.next_id().unwrap(), u64::MAX);
    assert!(ids.next_id().is_err());
}

#[test]
fn duplicate_completion_cannot_restore_old_state() {
    let mut c = AppController::new(InitialState::default());
    let (open, _) = run(
        &mut c,
        CoreRequest::OpenFile {
            path: "a.bin".into(),
            discard_unsaved: false,
        },
    );
    finish(&mut c, open.clone(), file("a.bin"));
    let identity = main(&c).source;
    finish(&mut c, open, file("wrong.bin"));
    assert_eq!(main(&c).file.unwrap().path, "a.bin");
    assert_eq!(main(&c).source, identity);
}

#[test]
fn completing_a_read_never_advances_the_mutation_queue() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (edit, _) = run(
        &mut c,
        CoreRequest::EditByte {
            offset: 0,
            value: 0x42,
        },
    );
    let (page, _) = run(
        &mut c,
        CoreRequest::ReadPage {
            offset: 0,
            length: 1,
        },
    );
    c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::OpenFile {
            path: "b.bin".into(),
            discard_unsaved: true,
        },
    )));
    let effects = finish(
        &mut c,
        page,
        CoreReply::Page(PageResponse {
            offset: "0".into(),
            bytes: vec![0x41],
            modified_offsets: vec![],
            revision: "0".into(),
        }),
    );
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::RunCore {
            request: CoreRequest::OpenFile { .. },
            ..
        }
    )));
    let effects = finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    assert!(matches!(next_run(effects).1, CoreRequest::OpenFile { .. }));
}

#[test]
fn successful_earlier_mutation_invalidates_search_even_when_later_mutation_fails() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (search, _) = run(
        &mut c,
        CoreRequest::SearchBytes {
            pattern: "41".into(),
        },
    );
    let (edit, _) = run(
        &mut c,
        CoreRequest::EditByte {
            offset: 0,
            value: 0x42,
        },
    );
    c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::EditByte {
            offset: 0,
            value: 0x43,
        },
    )));
    let (later, _) = next_run(finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    ));
    c.handle(ControllerEvent::CoreCompleted {
        stamp: later,
        reply: Err(AppError::new(
            ErrorCode::IoError,
            "Newer edit failed.",
            None,
        )),
    });
    finish(
        &mut c,
        search,
        CoreReply::Search(SearchResponse {
            matches: vec!["0".into()],
            truncated: false,
        }),
    );
    assert!(main(&c).matches.is_empty());
    assert_eq!(main(&c).error.unwrap().message, "Newer edit failed.");
    assert!(main(&c).file.unwrap().dirty);
}

#[test]
fn template_change_rejects_old_parse_progress_and_diagnostics() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (parse, _) = run(
        &mut c,
        CoreRequest::ApplyTemplate {
            template: template(),
        },
    );
    let mut changed = template();
    changed.name = "Changed".into();
    c.handle(ControllerEvent::Action(Action::TemplateChanged(changed)));
    c.handle(ControllerEvent::Progress {
        stamp: parse.clone(),
        progress: OperationProgress {
            operation_id: "stale".into(),
            phase: OperationPhase::Parse,
            processed: "1".into(),
            total: "1".into(),
        },
    });
    assert!(main(&c).activity.unwrap().progress.is_none());
    c.handle(ControllerEvent::CoreCompleted {
        stamp: parse,
        reply: Err(AppError::new(
            ErrorCode::TemplateOutOfBounds,
            "Old template.",
            None,
        )),
    });
    assert!(!main(&c).template_applied);
    assert!(main(&c).error.is_none());
    assert_eq!(main(&c).template.name, "Changed");
}

#[test]
fn newest_search_controls_results_progress_and_error_ownership() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (older, _) = run(
        &mut c,
        CoreRequest::SearchBytes {
            pattern: "41".into(),
        },
    );
    let (newer, _) = run(
        &mut c,
        CoreRequest::SearchBytes {
            pattern: "42".into(),
        },
    );
    c.handle(ControllerEvent::Progress {
        stamp: newer.clone(),
        progress: OperationProgress {
            operation_id: "new".into(),
            phase: OperationPhase::Search,
            processed: "5".into(),
            total: "10".into(),
        },
    });
    c.handle(ControllerEvent::Progress {
        stamp: older.clone(),
        progress: OperationProgress {
            operation_id: "old".into(),
            phase: OperationPhase::Search,
            processed: "9".into(),
            total: "10".into(),
        },
    });
    assert_eq!(
        main(&c).activity.unwrap().progress.unwrap().operation_id,
        "new"
    );
    finish(
        &mut c,
        newer,
        CoreReply::Search(SearchResponse {
            matches: vec!["32".into()],
            truncated: false,
        }),
    );
    assert!(main(&c).busy[&OperationKind::Search]);
    c.handle(ControllerEvent::CoreCompleted {
        stamp: older,
        reply: Err(AppError::new(ErrorCode::IoError, "Old failure.", None)),
    });
    assert_eq!(main(&c).matches, [32]);
    assert!(main(&c).error.is_none());
    assert!(main(&c).activity.is_none());
}

#[test]
fn close_barrier_waits_for_started_mutations_and_blocks_new_mutations() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (edit, _) = run(
        &mut c,
        CoreRequest::EditByte {
            offset: 0,
            value: 0x42,
        },
    );
    let before = c.handle(ControllerEvent::Action(Action::PrepareClose));
    assert!(!before.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    let blocked = c.handle(ControllerEvent::Action(Action::Core(CoreRequest::UndoEdit)));
    assert!(!blocked.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert_eq!(
        main(&c).error.unwrap().message,
        "Window close is in progress."
    );
    let (query, request) = next_run(finish(
        &mut c,
        edit,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    ));
    assert!(matches!(request, CoreRequest::GetDirtyState));
    finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: true,
            revision: "1".into(),
        }),
    );
    assert!(main(&c).file.unwrap().dirty);
    assert!(main(&c).policy.operation_busy);
    c.handle(ControllerEvent::Action(Action::ReleaseCloseBarrier));
    assert!(!main(&c).policy.operation_busy);
}

#[test]
fn a_clean_close_query_keeps_the_mutation_barrier_until_explicit_release() {
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "clean.bin");
    let (query, request) = next_run(c.handle(ControllerEvent::Action(Action::PrepareClose)));
    assert!(matches!(request, CoreRequest::GetDirtyState));
    finish(
        &mut c,
        query,
        CoreReply::Dirty(DirtyState {
            dirty: false,
            revision: "0".into(),
        }),
    );
    assert!(!main(&c).file.as_ref().unwrap().dirty);
    assert!(main(&c).policy.operation_busy);
    let rejected = c.handle(ControllerEvent::Action(Action::Core(
        CoreRequest::SaveBinaryAs {
            path: "not-approved.bin".into(),
        },
    )));
    assert!(!rejected.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    assert_eq!(
        main(&c).error.as_ref().unwrap().message,
        "Window close is in progress."
    );
    assert_eq!(main(&c).file.as_ref().unwrap().path, "clean.bin");
    c.handle(ControllerEvent::Action(Action::ClearError));
    c.handle(ControllerEvent::Action(Action::ReleaseCloseBarrier));
    assert!(!main(&c).policy.operation_busy);
    let (_, resumed) = run(&mut c, CoreRequest::UndoEdit);
    assert!(matches!(resumed, CoreRequest::UndoEdit));
}

#[test]
fn disabled_command_never_starts_a_core_effect() {
    use hexforge_controller::{command::Command, controller::InvokeContext};
    let mut c = AppController::new(InitialState::default());
    let effects = c.handle(ControllerEvent::Invoke {
        command: Command::Undo,
        context: InvokeContext::default(),
    });
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
    opened(&mut c, "a.bin");
    run(
        &mut c,
        CoreRequest::SearchBytes {
            pattern: "41".into(),
        },
    );
    let effects = c.handle(ControllerEvent::Invoke {
        command: Command::ApplyTemplate,
        context: InvokeContext::default(),
    });
    assert!(!effects.iter().any(|e| matches!(e, Effect::RunCore { .. })));
}

#[test]
fn export_availability_uses_successful_leaves_and_rejects_diagnostics() {
    use hexforge_core::template::{Diagnostic, FieldType, ParsedNode};
    let leaf = ParsedNode {
        kind: "leaf".into(),
        name: "id".into(),
        path: "header.id".into(),
        field_type: FieldType::U8,
        offset: Some("0".into()),
        length: Some("1".into()),
        value: Some("41".into()),
        endianness: None,
        comment: String::new(),
        enum_label: None,
        flags: vec![],
        diagnostics: vec![],
        children: vec![],
    };
    let mut c = AppController::new(InitialState::default());
    opened(&mut c, "a.bin");
    let (parse, _) = run(
        &mut c,
        CoreRequest::ApplyTemplate {
            template: template(),
        },
    );
    finish(&mut c, parse, CoreReply::Parsed(vec![leaf.clone()]));
    assert!(main(&c).policy.has_parsed_results);
    let mut invalid = leaf;
    invalid.diagnostics.push(Diagnostic {
        code: "template_data_invalid".into(),
        message: "Invalid data.".into(),
    });
    let (parse, _) = run(
        &mut c,
        CoreRequest::ApplyTemplate {
            template: template(),
        },
    );
    finish(&mut c, parse, CoreReply::Parsed(vec![invalid]));
    assert!(!main(&c).policy.has_parsed_results);
}
