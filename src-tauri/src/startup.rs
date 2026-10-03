//! Local, opt-in startup diagnostics. Never enabled by frontend IPC alone.
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf, sync::Mutex, time::Instant};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartupPhase {
    pub name: String,
    pub at_ms: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartupTimings {
    pub phases: Vec<StartupPhase>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paints: Vec<StartupPhase>,
    #[serde(default, rename = "mainInterfaceAtMs")]
    pub main_interface_at_ms: Option<f64>,
    #[serde(default, rename = "clockSamples")]
    pub clock_samples: Vec<ClockSample>,
    #[serde(default)]
    pub resources: Vec<StartupResource>,
    #[serde(default, rename = "measurementError")]
    pub measurement_error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockSample {
    pub document_before_ms: f64,
    pub process_at_ms: f64,
    pub document_after_ms: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartupResource {
    pub path: String,
    pub initiator_type: String,
    pub start_ms: f64,
    pub duration_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupClock {
    pub at_ms: f64,
}

pub struct StartupDiagnostics {
    started: Instant,
    output: Option<PathBuf>,
    native_phases: Mutex<Vec<StartupPhase>>,
    run_entry_ms: f64,
    clock_origin: &'static str,
    anchor_uncertainty_ms: f64,
}

impl StartupDiagnostics {
    pub fn new(output: Option<PathBuf>) -> Self {
        Self {
            started: Instant::now(),
            output,
            native_phases: Mutex::new(Vec::new()),
            run_entry_ms: 0.0,
            clock_origin: "rust-run-entry",
            anchor_uncertainty_ms: 0.0,
        }
    }

    /// Native milestones share the declared process/run-entry clock, never the browser clock.
    /// Disabled diagnostics do not read the clock or lock the phase buffer.
    pub fn mark_native(&self, name: &str) {
        if self.output.is_none() {
            return;
        }
        if let Ok(mut phases) = self.native_phases.lock() {
            if phases.len() >= 32 || phases.iter().any(|phase| phase.name == name) {
                return;
            }
            if phases.is_empty() {
                phases.push(StartupPhase {
                    name: "run-entry".into(),
                    at_ms: self.run_entry_ms,
                });
            }
            phases.push(StartupPhase {
                name: name.into(),
                at_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            });
        }
    }

    pub fn from_environment() -> Self {
        let mut diagnostics = Self::new(
            std::env::var_os("HEXFORGE_STARTUP_PROFILE")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        );
        // Pair OS process creation (FILETIME) with a sampled monotonic Instant.
        // All later intervals use the monotonic clock, not Date.now()/UTC.
        #[cfg(windows)]
        if diagnostics.output.is_some() {
            if let Some((started, uncertainty)) = windows_process_origin() {
                diagnostics.run_entry_ms = diagnostics
                    .started
                    .saturating_duration_since(started)
                    .as_secs_f64()
                    * 1000.0;
                diagnostics.started = started;
                diagnostics.clock_origin = "windows-process-creation";
                diagnostics.anchor_uncertainty_ms = uncertainty;
            }
        }
        diagnostics
    }

    pub fn sample_clock(&self) -> Option<StartupClock> {
        self.output.as_ref().map(|_| StartupClock {
            at_ms: self.started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// Isolated profiles are benchmark-only; normal app preferences are untouched.
    pub fn profile_data_directory(&self) -> Option<PathBuf> {
        self.output.as_ref()?;
        std::env::var_os("HEXFORGE_STARTUP_PROFILE_DATA_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    pub fn initialization_script(&self) -> &'static str {
        if self.output.is_some() {
            "\nwindow.__HEXFORGE_STARTUP_PROFILE__ = true;"
        } else {
            ""
        }
    }

    /// The path comes only from the launching process, never from webview data.
    pub fn record(&self, window: &str, timings: Option<&StartupTimings>) -> io::Result<()> {
        let (Some(path), Some(timings)) = (&self.output, timings) else {
            return Ok(());
        };
        if window != "main" {
            return Ok(());
        }
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid startup timing report.",
            )
        };
        if timings.phases.is_empty() || timings.phases.len() > 32 {
            return Err(invalid());
        }
        let mut previous = 0.0;
        for phase in &timings.phases {
            if phase.name.is_empty()
                || phase.name.len() > 64
                || !phase
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                || !phase.at_ms.is_finite()
                || phase.at_ms < previous
                || phase.at_ms > 300_000.0
            {
                return Err(invalid());
            }
            previous = phase.at_ms;
        }
        if timings.paints.len() > 2
            || timings.paints.iter().any(|paint| {
                !matches!(
                    paint.name.as_str(),
                    "first-paint" | "first-contentful-paint"
                ) || !paint.at_ms.is_finite()
                    || !(0.0..=300_000.0).contains(&paint.at_ms)
            })
        {
            return Err(invalid());
        }
        let valid_time = |value: f64| value.is_finite() && (0.0..=300_000.0).contains(&value);
        if timings.clock_samples.len() > 5
            || timings.clock_samples.iter().any(|sample| {
                !valid_time(sample.document_before_ms)
                    || !valid_time(sample.document_after_ms)
                    || !valid_time(sample.process_at_ms)
                    || sample.document_after_ms < sample.document_before_ms
            })
            || timings.resources.len() > 128
            || timings.resources.iter().any(|resource| {
                resource.path.len() > 255
                    || resource.initiator_type.len() > 32
                    || !valid_time(resource.start_ms)
                    || !valid_time(resource.duration_ms)
            })
            || timings
                .measurement_error
                .as_ref()
                .is_some_and(|error| error.len() > 255)
        {
            return Err(invalid());
        }
        let metrics = if let Some(controls_at) = timings.main_interface_at_ms {
            if !valid_time(controls_at) || timings.measurement_error.is_some() {
                return Err(invalid());
            }
            let paint = timings
                .paints
                .iter()
                .find(|phase| phase.name == "first-contentful-paint")
                .ok_or_else(invalid)?;
            if paint.at_ms < controls_at {
                return Err(invalid());
            }
            let ready = timings
                .phases
                .iter()
                .find(|phase| phase.name == "listeners-ready")
                .ok_or_else(invalid)?;
            let sample = timings
                .clock_samples
                .iter()
                .min_by(|a, b| {
                    (a.document_after_ms - a.document_before_ms)
                        .total_cmp(&(b.document_after_ms - b.document_before_ms))
                })
                .ok_or_else(invalid)?;
            let offset =
                sample.process_at_ms - (sample.document_before_ms + sample.document_after_ms) / 2.0;
            let fmp = paint.at_ms + offset;
            let interactive = ready.at_ms.max(paint.at_ms) + offset;
            if !valid_time(fmp) || !valid_time(interactive) {
                return Err(invalid());
            }
            Some(serde_json::json!({
                "firstMeaningfulPaintMs": fmp,
                "interactiveMs": interactive,
                "listenersReadyMs": ready.at_ms + offset,
                "documentToProcessOffsetMs": offset,
                "clockRoundTripUncertaintyMs": (sample.document_after_ms - sample.document_before_ms) / 2.0,
                "processAnchorSamplingUncertaintyMs": self.anchor_uncertainty_ms,
                "clockOrigin": self.clock_origin,
                "definition": "contentful paint after all real main controls are visible; no splash or placeholder",
            }))
        } else {
            if !timings.clock_samples.is_empty() {
                return Err(invalid());
            }
            None
        };
        let native_phases = self
            .native_phases
            .lock()
            .map_err(|_| io::Error::other("Startup diagnostics lock failed."))?;
        let report = serde_json::json!({
            "window": window,
            // Native milestones use the declared origin. Browser timestamps
            // stay document-relative; only metrics apply measured correlation.
            "nativeReadyMs": self.started.elapsed().as_secs_f64() * 1000.0,
            "nativeClockOrigin": self.clock_origin,
            "phases": timings.phases,
            "paints": timings.paints,
            "nativePhases": *native_phases,
            "metrics": metrics,
            "resources": timings.resources,
            "mainInterfaceAtMs": timings.main_interface_at_ms,
            "clockSamples": timings.clock_samples,
            "measurementError": timings.measurement_error,
        });
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)
    }
}

#[cfg(windows)]
fn windows_process_origin() -> Option<(Instant, f64)> {
    use std::{ffi::c_void, time::Duration};
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    impl FileTime {
        fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn GetSystemTimePreciseAsFileTime(time: *mut FileTime);
    }
    let (mut created, mut exited, mut kernel, mut user, mut utc) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // SAFETY: the current-process pseudo-handle is valid; pointers reference
    // writable, correctly laid out FILETIMEs and remain live for these calls.
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return None;
    }
    let before = Instant::now();
    unsafe { GetSystemTimePreciseAsFileTime(&mut utc) };
    let span = before.elapsed();
    let age = Duration::from_nanos(utc.ticks().checked_sub(created.ticks())?.checked_mul(100)?);
    let midpoint = before.checked_add(span / 2)?;
    Some((
        midpoint.checked_sub(age)?,
        span.as_secs_f64() * 500.0 + 0.001,
    ))
}
