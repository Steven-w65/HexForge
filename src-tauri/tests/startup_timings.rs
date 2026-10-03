use hexforge_lib::startup::{StartupDiagnostics, StartupPhase, StartupTimings};

fn timings() -> StartupTimings {
    serde_json::from_value(serde_json::json!({ "phases": [
        { "name": "entry", "atMs": 20.0 },
        { "name": "listeners-ready", "atMs": 80.0 }
    ] }))
    .unwrap()
}

fn painted_timings() -> StartupTimings {
    let value = serde_json::json!({
        "phases": [{ "name": "listeners-ready", "atMs": 165.0 }],
        "paints": [{ "name": "first-contentful-paint", "atMs": 160.0 }],
        "mainInterfaceAtMs": 150.0,
        "clockSamples": [
            { "documentBeforeMs": 170.0, "processAtMs": 400.0, "documentAfterMs": 174.0 },
            { "documentBeforeMs": 210.0, "processAtMs": 440.0, "documentAfterMs": 212.0 }
        ],
        "resources": [{ "path": "/assets/main.js", "initiatorType": "script", "startMs": 10.0, "durationMs": 5.0 }]
    });
    serde_json::from_value(value)
        .expect("The opt-in report must accept correlated real-paint measurements")
}

#[test]
fn real_paint_is_correlated_using_the_smallest_clock_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paint.json");
    let diagnostics = StartupDiagnostics::new(Some(path.clone()));
    diagnostics
        .record("main", Some(&painted_timings()))
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    // The second sample has offset 440 - (210 + 212)/2 = 229 ms.
    assert_eq!(value["metrics"]["firstMeaningfulPaintMs"], 389.0);
    assert_eq!(value["metrics"]["interactiveMs"], 394.0);
    assert_eq!(value["metrics"]["clockRoundTripUncertaintyMs"], 1.0);
    assert_eq!(value["resources"][0]["path"], "/assets/main.js");
}

#[test]
fn mount_or_title_readiness_cannot_be_reported_as_meaningful_paint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paint.json");
    let diagnostics = StartupDiagnostics::new(Some(path.clone()));
    diagnostics.record("main", Some(&timings())).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(value["metrics"].is_null());
    let mut report = painted_timings();
    report.paints.clear();
    assert!(diagnostics.record("main", Some(&report)).is_err());
}

#[test]
fn pre_interface_paint_and_invalid_clock_samples_leave_output_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paint.json");
    std::fs::write(&path, "previous report").unwrap();
    let diagnostics = StartupDiagnostics::new(Some(path.clone()));
    let mut report = painted_timings();
    report.paints[0].at_ms = 140.0;
    assert!(diagnostics.record("main", Some(&report)).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous report");
    for invalid in ["backward", "negative", "infinite", "too-many"] {
        let mut report = painted_timings();
        match invalid {
            "backward" => report.clock_samples[0].document_after_ms = 169.0,
            "negative" => report.clock_samples[0].process_at_ms = -1.0,
            "infinite" => report.clock_samples[0].process_at_ms = f64::INFINITY,
            "too-many" => {
                report.clock_samples = (0..6)
                    .map(|_| hexforge_lib::startup::ClockSample {
                        document_before_ms: 170.0,
                        document_after_ms: 172.0,
                        process_at_ms: 400.0,
                    })
                    .collect()
            }
            _ => unreachable!(),
        }
        assert!(
            diagnostics.record("main", Some(&report)).is_err(),
            "{invalid}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous report");
    }
}

#[test]
fn ordinary_startup_has_no_profiling_script_or_file_work() {
    let diagnostics = StartupDiagnostics::new(None);
    assert_eq!(diagnostics.initialization_script(), "");
    let mut invalid = timings();
    invalid.phases[0].at_ms = f64::NAN;
    // Disabled profiling doesn't even validate or serialize the payload.
    diagnostics.record("main", Some(&invalid)).unwrap();
}

#[test]
fn opted_in_startup_writes_bounded_document_and_native_milestones() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("startup.json");
    let diagnostics = StartupDiagnostics::new(Some(path.clone()));
    assert!(!diagnostics.initialization_script().is_empty());
    diagnostics.mark_native("app-built");
    diagnostics.mark_native("page-started");
    diagnostics.mark_native("page-started");
    diagnostics.record("main", Some(&timings())).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value["window"], "main");
    assert_eq!(
        value["phases"][0],
        serde_json::json!({ "name": "entry", "atMs": 20.0 })
    );
    assert_eq!(value["phases"][1]["atMs"], 80.0);
    assert!(value["nativeReadyMs"].as_f64().unwrap() >= 0.0);
    let phases = value["nativePhases"].as_array().unwrap();
    assert_eq!(phases.len(), 3);
    assert_eq!(phases[0]["name"], "run-entry");
    assert_eq!(phases[1]["name"], "app-built");
    assert_eq!(phases[2]["name"], "page-started");
    assert!(phases
        .windows(2)
        .all(|p| p[0]["atMs"].as_f64() <= p[1]["atMs"].as_f64()));
}

#[test]
fn editor_or_missing_timings_cannot_overwrite_the_main_startup_report() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("startup.json");
    let diagnostics = StartupDiagnostics::new(Some(path.clone()));
    diagnostics
        .record("template-editor", Some(&timings()))
        .unwrap();
    diagnostics.record("main", None).unwrap();
    assert!(!path.exists());
}

#[test]
fn invalid_phases_leave_existing_output_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("startup.json");
    std::fs::write(&path, "previous report").unwrap();
    let diagnostics = StartupDiagnostics::new(Some(path.clone()));
    for case in [
        "negative",
        "backward",
        "infinite",
        "oversized",
        "long-name",
        "unknown-paint",
        "invalid-paint",
        "too-many-paints",
    ] {
        let mut report = timings();
        match case {
            "negative" => report.phases[0].at_ms = -1.0,
            "backward" => report.phases[1].at_ms = 10.0,
            "infinite" => report.phases[0].at_ms = f64::INFINITY,
            "oversized" => {
                report.phases = (0..33)
                    .map(|_| StartupPhase {
                        name: "phase".into(),
                        at_ms: 0.0,
                    })
                    .collect()
            }
            "long-name" => report.phases[0].name = "x".repeat(65),
            "unknown-paint" => report.paints.push(StartupPhase {
                name: "anything".into(),
                at_ms: 10.0,
            }),
            "invalid-paint" => report.paints.push(StartupPhase {
                name: "first-paint".into(),
                at_ms: f64::INFINITY,
            }),
            "too-many-paints" => {
                report.paints = (0..3)
                    .map(|_| StartupPhase {
                        name: "first-paint".into(),
                        at_ms: 10.0,
                    })
                    .collect()
            }
            _ => unreachable!(),
        }
        assert!(diagnostics.record("main", Some(&report)).is_err(), "{case}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous report");
    }
}
