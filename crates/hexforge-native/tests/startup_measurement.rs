//! Measurement guards, not startup performance numbers. The real Windows
//! capture/presentation observer must supply evidence before a run is valid.
use hexforge_native::startup::{
    clock::correlate_process_creation,
    diagnostics::StartupDiagnostics,
    presentation::{
        summarize_launches, variant_order, ClockDomain, FrameEvidence, FrameKind,
        InteractionEvidence, MeasurementError, ObservationMethod, PrimaryControl, ProcessTimeline,
        StartupMeasurement, Tick,
    },
    trace::{StartupPhase, StartupTrace},
};

fn timeline() -> ProcessTimeline {
    ProcessTimeline::new(42, Tick::qpc(50_000, 1000), 2).unwrap()
}
fn frame(at: i64) -> FrameEvidence {
    FrameEvidence {
        process_id: 42,
        window_id: 77,
        frame_sequence: 1,
        observed_at: Tick::qpc(at, 1000),
        last_non_meaningful_at: Some(Tick::qpc(50_040, 1000)),
        method: ObservationMethod::IndependentWindowCapture,
        kind: FrameKind::RealMainInterface,
        visible: true,
        primary_controls: PrimaryControl::REQUIRED.to_vec(),
        artifact_hash: "a".repeat(64),
    }
}
fn interaction(at: i64) -> InteractionEvidence {
    InteractionEvidence {
        process_id: 42,
        window_id: 77,
        accepted_at: Tick::qpc(at, 1000),
        command_sequence: 1,
        reached_controller: true,
        accepted: true,
    }
}
fn launch(paint: i64, interactive: i64) -> StartupMeasurement {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    let mut f = frame(50_000 + paint);
    f.last_non_meaningful_at = Some(Tick::qpc(50_000 + paint - 1, 1000));
    m.observe_presented_frame(f).unwrap();
    m.record_interaction(interaction(50_000 + interactive))
        .unwrap();
    m
}

#[test]
fn queued_frame_is_not_meaningful_paint() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    assert!(m.presented().is_none());
    assert_eq!(m.metrics(), Err(MeasurementError::PresentationMissing));
    let mut queued = frame(50_026);
    queued.method = ObservationMethod::ApplicationFrameQueue;
    assert_eq!(
        m.observe_presented_frame(queued),
        Err(MeasurementError::IndependentPresentationRequired)
    );
    assert!(m.presented().is_none());
}

#[test]
fn clock_origins_must_be_correlated() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    let mut uncorrelated = frame(50_048);
    uncorrelated.observed_at.domain = ClockDomain::Uncorrelated("browser-navigation".into());
    assert_eq!(
        m.observe_presented_frame(uncorrelated),
        Err(MeasurementError::UncorrelatedClock)
    );
    let mut wrong_units = frame(50_048);
    wrong_units.observed_at = Tick::qpc(50_048, 10_000_000);
    assert_eq!(
        m.observe_presented_frame(wrong_units),
        Err(MeasurementError::UncorrelatedClock)
    );
    assert!(m.presented().is_none());
}

#[test]
fn blank_or_hidden_frame_is_rejected() {
    for kind in [
        FrameKind::Blank,
        FrameKind::Splash,
        FrameKind::Placeholder,
        FrameKind::WindowTitle,
    ] {
        let mut m = StartupMeasurement::new(timeline());
        m.record_scene_submission(77, Tick::qpc(50_025, 1000))
            .unwrap();
        let mut f = frame(50_048);
        f.kind = kind;
        assert_eq!(
            m.observe_presented_frame(f),
            Err(MeasurementError::RealInterfaceRequired)
        );
    }
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    let mut hidden = frame(50_048);
    hidden.visible = false;
    assert_eq!(
        m.observe_presented_frame(hidden),
        Err(MeasurementError::HiddenFrame)
    );
    let mut missing = frame(50_048);
    missing
        .primary_controls
        .retain(|c| *c != PrimaryControl::OpenFile);
    assert_eq!(
        m.observe_presented_frame(missing),
        Err(MeasurementError::PrimaryControlsMissing)
    );
}

#[test]
fn interaction_requires_live_dispatch() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    m.observe_presented_frame(frame(50_048)).unwrap();
    let mut inert = interaction(50_049);
    inert.reached_controller = false;
    assert_eq!(
        m.record_interaction(inert),
        Err(MeasurementError::LiveDispatchRequired)
    );
    let mut disabled = interaction(50_049);
    disabled.accepted = false;
    assert_eq!(
        m.record_interaction(disabled),
        Err(MeasurementError::LiveDispatchRequired)
    );
    assert_eq!(m.metrics(), Err(MeasurementError::InteractionMissing));
    m.record_interaction(interaction(50_052)).unwrap();
    assert_eq!(m.metrics().unwrap().interactive_ms, 52.0);
}

#[test]
fn probe_reports_uncertainty() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    m.observe_presented_frame(frame(50_048)).unwrap();
    m.record_interaction(interaction(50_052)).unwrap();
    let metrics = m.metrics().unwrap();
    assert_eq!(metrics.first_meaningful_paint_ms, 48.0);
    assert_eq!(metrics.first_meaningful_paint_lower_bound_ms, 40.0);
    assert_eq!(metrics.capture_interval_ms, 8.0);
    assert_eq!(metrics.clock_origin_uncertainty_ms, 2.0);
    assert_eq!(metrics.total_uncertainty_ms, 10.0);
    assert_eq!(metrics.scene_submission_ms, 25.0);
}

#[test]
fn p95_uses_all_valid_runs() {
    let runs = [
        launch(160, 161),
        launch(130, 140),
        launch(110, 120),
        launch(170, 180),
    ];
    let summary = summarize_launches(&runs).unwrap();
    assert_eq!(summary.count, 4);
    assert_eq!(summary.paint.median_ms, 145.0);
    assert_eq!(summary.paint.p95_ms, 170.0);
    assert_eq!(summary.paint.fastest_ms, 110.0);
    assert_eq!(summary.interactive.median_ms, 150.5);
    assert_eq!(summary.interactive.p95_ms, 180.0);
    assert!(summarize_launches(&[]).is_err());
    let unverified = StartupMeasurement::new(timeline());
    assert!(summarize_launches(&[unverified]).is_err());
}

#[test]
fn benchmark_alternates_pair_order_without_dropping_a_variant() {
    assert_eq!(
        variant_order(&["baseline", "candidate"], 0),
        ["baseline", "candidate"]
    );
    assert_eq!(
        variant_order(&["baseline", "candidate"], 1),
        ["candidate", "baseline"]
    );
}

#[test]
fn presented_evidence_is_bound_to_the_launched_process_main_window_and_frame() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    for (pid, window, sequence) in [(99, 77, 1), (42, 88, 1), (42, 77, 0)] {
        let mut stale = frame(50_048);
        stale.process_id = pid;
        stale.window_id = window;
        stale.frame_sequence = sequence;
        assert_eq!(
            m.observe_presented_frame(stale),
            Err(MeasurementError::WrongFrameIdentity)
        );
    }
    let mut incomplete = frame(50_048);
    incomplete.artifact_hash.clear();
    assert_eq!(
        m.observe_presented_frame(incomplete),
        Err(MeasurementError::CaptureArtifactMissing)
    );
    m.observe_presented_frame(frame(50_048)).unwrap();
    m.observe_presented_frame(frame(50_060)).unwrap();
    assert_eq!(m.presented().unwrap().observed_at.ticks, 50_048);
}

#[test]
fn invalid_intervals_cannot_become_negative_fast_launches() {
    let mut m = StartupMeasurement::new(timeline());
    assert!(m
        .record_scene_submission(77, Tick::qpc(49_000, 1000))
        .is_err());
    assert!(ProcessTimeline::new(42, Tick::qpc(50_000, 0), 0).is_err());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    let mut reversed = frame(50_048);
    reversed.last_non_meaningful_at = Some(Tick::qpc(50_049, 1000));
    assert_eq!(
        m.observe_presented_frame(reversed),
        Err(MeasurementError::InvalidInterval)
    );
    assert!(m.presented().is_none());
}

#[test]
fn first_paint_is_not_backdated_to_a_prepaint_dispatch() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    m.record_interaction(interaction(50_030)).unwrap();
    m.observe_presented_frame(frame(50_048)).unwrap();
    assert_eq!(m.metrics().unwrap().interactive_ms, 48.0);
}

#[test]
fn disabled_startup_profiling_does_no_clock_work_and_returns_no_report() {
    let mut trace = StartupTrace::new(false);
    trace
        .mark_with(StartupPhase::RustEntry, || {
            panic!("ordinary launch must not sample clocks")
        })
        .unwrap();
    assert!(trace.snapshot().is_none());
}

#[test]
fn phase_trace_records_first_milestones_once_and_snapshots_are_independent() {
    let mut trace = StartupTrace::new(true);
    trace.mark(StartupPhase::RustEntry, 50_020).unwrap();
    trace.mark(StartupPhase::RendererReady, 50_030).unwrap();
    trace.mark(StartupPhase::RendererReady, 50_040).unwrap();
    let mut report = trace.snapshot().unwrap();
    assert_eq!(report.marks.len(), 2);
    assert_eq!(report.marks[1].qpc, 50_030);
    report.marks[0].qpc = 999;
    assert_eq!(trace.snapshot().unwrap().marks[0].qpc, 50_020);
}

#[test]
fn diagnostic_trace_is_bounded_and_never_promotes_a_frame_queue_to_presentation() {
    let mut trace = StartupTrace::new(true);
    for _ in 0..100 {
        trace.mark(StartupPhase::MainSceneBuilt, 50_030).unwrap();
    }
    trace.mark(StartupPhase::FrameQueued, 50_031).unwrap();
    let report = trace.snapshot().unwrap();
    assert_eq!(report.marks.len(), 2);
    assert!(report.presented_frame.is_none());
}

#[test]
fn process_creation_uses_a_bracketed_exact_filetime_qpc_correlation() {
    // OS creation was 25ms before the wall-clock sample. The sample occurred
    // somewhere inside the 4-tick QPC bracket, not at Rust main's timestamp.
    let t = correlate_process_creation(
        42,
        10_000_000,
        10_250_000,
        Tick::qpc(50_098, 1000),
        Tick::qpc(50_102, 1000),
    )
    .unwrap();
    assert_eq!(t.started_at(), &Tick::qpc(50_075, 1000));
    assert_eq!(t.origin_uncertainty_ticks(), 3); // half bracket + conversion tick
}

#[test]
fn process_correlation_subtracts_large_absolute_clock_values_before_conversion() {
    let filetime = (1u64 << 60) + 19;
    let qpc = (1i64 << 56) + 17;
    let t = correlate_process_creation(
        42,
        filetime,
        filetime + 100_000,
        Tick::qpc(qpc, 10_000_000),
        Tick::qpc(qpc + 4, 10_000_000),
    )
    .unwrap();
    assert_eq!(t.started_at().ticks, qpc + 2 - 100_000);
    assert_eq!(t.origin_uncertainty_ticks(), 3);
}

#[test]
fn process_correlation_rejects_clock_mismatch_future_creation_and_impossible_origin() {
    assert!(
        correlate_process_creation(42, 101, 100, Tick::qpc(20, 1000), Tick::qpc(22, 1000)).is_err()
    );
    assert!(correlate_process_creation(
        42,
        0,
        u64::MAX,
        Tick::qpc(20, u64::MAX),
        Tick::qpc(22, u64::MAX)
    )
    .is_err());
    assert_eq!(
        correlate_process_creation(42, 100, 100, Tick::qpc(20, 1000), Tick::qpc(22, 2000))
            .unwrap_err(),
        MeasurementError::UncorrelatedClock
    );
    assert!(
        correlate_process_creation(42, 100, 100, Tick::qpc(22, 1000), Tick::qpc(20, 1000)).is_err()
    );
}

#[test]
fn evidence_interval_checks_use_exact_ticks_even_after_an_extremely_long_clock_range() {
    let mut m = StartupMeasurement::new(ProcessTimeline::new(42, Tick::qpc(0, 1000), 0).unwrap());
    let at = (1i64 << 56) + 17;
    m.record_scene_submission(77, Tick::qpc(at - 1, 1000))
        .unwrap();
    let mut f = frame(at);
    f.last_non_meaningful_at = Some(Tick::qpc(at + 1, 1000));
    assert_eq!(
        m.observe_presented_frame(f),
        Err(MeasurementError::InvalidInterval)
    );
    assert!(m.presented().is_none());
}

#[test]
fn live_interaction_cannot_precede_the_submitted_interface() {
    let mut m = StartupMeasurement::new(timeline());
    m.record_scene_submission(77, Tick::qpc(50_025, 1000))
        .unwrap();
    assert_eq!(
        m.record_interaction(interaction(50_024)),
        Err(MeasurementError::InvalidInterval)
    );
}

#[test]
fn ordinary_native_application_does_not_initialize_or_publish_startup_diagnostics() {
    use hexforge_native::app::NativeApp;
    let app = NativeApp::new(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
    );
    assert!(app.startup_report().is_none());
    assert!(StartupDiagnostics::from_output(None)
        .unwrap()
        .report()
        .is_none());
}

#[cfg(windows)]
#[test]
fn opt_in_native_clock_is_monotonic_and_process_creation_precedes_the_sample() {
    use hexforge_native::startup::clock::NativeClock;
    let clock = NativeClock::new().unwrap();
    let timeline = clock.current_process_timeline().unwrap();
    let first = clock.now().unwrap();
    let next = clock.now().unwrap();
    assert_eq!(first.domain, next.domain);
    assert_eq!(timeline.started_at().domain, first.domain);
    assert!(timeline.started_at().ticks <= first.ticks);
    assert!(first.ticks <= next.ticks);
}

#[cfg(windows)]
#[test]
fn native_phase_trace_observes_the_real_scene_without_reporting_its_presentation() {
    use eframe::egui;
    use hexforge_native::app::NativeApp;
    let profile = StartupDiagnostics::in_memory().unwrap();
    let mut app = NativeApp::with_startup_diagnostics(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        profile,
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| app.paint_main(ui),
    );
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
    let report = app.startup_report().unwrap();
    assert!(report
        .trace
        .marks
        .iter()
        .any(|m| m.phase == StartupPhase::ControllerReady));
    assert!(report
        .trace
        .marks
        .iter()
        .any(|m| m.phase == StartupPhase::FontAssetsReady));
    assert!(report
        .trace
        .marks
        .iter()
        .any(|m| m.phase == StartupPhase::MainSceneBuilt));
    assert!(!report
        .trace
        .marks
        .iter()
        .any(|m| m.phase == StartupPhase::FrameQueued));
    assert!(report.trace.presented_frame.is_none());
    assert!(
        report.main_window.is_none(),
        "a headless scene is not a Windows presentation identity"
    );
    assert!(report.measurement_error.is_none());
    assert!(app.main_snapshot().file.is_none());
}

#[cfg(windows)]
#[test]
fn submission_probe_runs_at_the_gl_boundary_not_during_ui_construction_and_is_opt_in() {
    use eframe::egui;
    use hexforge_native::app::NativeApp;
    fn callbacks(shape: &egui::epaint::Shape) -> usize {
        match shape {
            egui::epaint::Shape::Callback(_) => 1,
            egui::epaint::Shape::Vec(shapes) => shapes.iter().map(callbacks).sum(),
            _ => 0,
        }
    }
    for enabled in [false, true] {
        let profile = if enabled {
            StartupDiagnostics::in_memory().unwrap()
        } else {
            StartupDiagnostics::disabled()
        };
        let mut app = NativeApp::with_startup_diagnostics(
            Default::default(),
            std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
            profile,
        );
        let ctx = egui::Context::default();
        app.attach(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| app.paint_main(ui),
        );
        output.textures_delta.clear();
        assert_eq!(
            output
                .shapes
                .iter()
                .map(|s| callbacks(&s.shape))
                .sum::<usize>(),
            usize::from(enabled)
        );
        if let Some(report) = app.startup_report() {
            assert!(!report
                .trace
                .marks
                .iter()
                .any(|m| m.phase == StartupPhase::FrameQueued));
            assert!(report.trace.presented_frame.is_none());
        }
    }
}

#[cfg(windows)]
#[test]
fn native_interaction_marker_requires_a_real_accepted_controller_change() {
    use hexforge_controller::{
        command::Command,
        controller::{InvokeContext, Theme},
    };
    use hexforge_native::app::NativeApp;
    let profile = StartupDiagnostics::in_memory().unwrap();
    let mut app = NativeApp::with_startup_diagnostics(
        Default::default(),
        std::sync::Arc::new(hexforge_core::operations::CoreService::default()),
        profile,
    );
    // Disabled Undo and a modal-blocked theme command are not interaction proof.
    app.invoke(Command::Undo, Default::default());
    app.invoke(
        Command::ThemeToggle,
        InvokeContext {
            modal: true,
            ..Default::default()
        },
    );
    assert!(!app
        .startup_report()
        .unwrap()
        .trace
        .marks
        .iter()
        .any(|m| m.phase == StartupPhase::MainDispatchAccepted));
    assert_eq!(app.main_snapshot().theme, Theme::Dark);
    app.invoke(Command::ThemeToggle, Default::default());
    assert_eq!(app.main_snapshot().theme, Theme::Light);
    let report = app.startup_report().unwrap();
    assert!(report
        .trace
        .marks
        .iter()
        .any(|m| m.phase == StartupPhase::MainDispatchAccepted));
    assert!(report.trace.presented_frame.is_none());
}

#[cfg(windows)]
#[test]
fn opt_in_phase_report_is_created_only_after_shutdown_outside_the_executable_folder() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("startup.json");
    let mut diagnostics = StartupDiagnostics::from_output(Some(output.clone())).unwrap();
    diagnostics.mark(StartupPhase::MainSceneBuilt);
    assert!(
        !output.exists(),
        "report I/O must not block the first frame"
    );
    drop(diagnostics);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(
        json["observation_status"],
        "presentation-not-independently-observed"
    );
    assert!(json["trace"]["presented_frame"].is_null());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);

    let executable = std::env::current_exe().unwrap();
    let sidecar = executable
        .parent()
        .unwrap()
        .join(format!("native-startup-{}.json", std::process::id()));
    assert!(StartupDiagnostics::from_output(Some(sidecar.clone())).is_err());
    assert!(!sidecar.exists());
}

#[cfg(windows)]
#[test]
fn profiling_never_replaces_an_existing_file_or_a_racing_new_destination() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.json");
    std::fs::write(&output, b"keep existing user content").unwrap();
    assert!(StartupDiagnostics::from_output(Some(output.clone())).is_err());
    assert_eq!(
        std::fs::read(&output).unwrap(),
        b"keep existing user content"
    );
    let racing = directory.path().join("racing.json");
    let diagnostics = StartupDiagnostics::from_output(Some(racing.clone())).unwrap();
    std::fs::write(&racing, b"created after validation").unwrap();
    drop(diagnostics);
    assert_eq!(std::fs::read(&racing).unwrap(), b"created after validation");
}
