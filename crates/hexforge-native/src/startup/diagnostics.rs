//! Optional phase-only reports. There is no first-paint/ready signal here.
//! Independent presentation and accepted interaction evidence remain necessary
//! to use a run in `StartupMeasurement`. No diagnostics run on ordinary startup.
use super::{
    clock::NativeClock,
    presentation::ProcessTimeline,
    trace::{StartupPhase, StartupTrace, TraceReport},
};
use serde::Serialize;
use std::{
    io,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

struct SubmissionProbe {
    clock: NativeClock,
    first: OnceLock<Result<super::presentation::Tick, String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeStartupReport {
    pub process: ProcessTimeline,
    pub main_window: Option<u64>,
    pub trace: TraceReport,
    pub measurement_error: Option<String>,
    pub observation_status: &'static str,
}

pub struct StartupDiagnostics {
    clock: Option<NativeClock>,
    timeline: Option<ProcessTimeline>,
    trace: StartupTrace,
    output: Option<PathBuf>,
    main_window: Option<u64>,
    measurement_error: Option<String>,
    submission: Option<Arc<SubmissionProbe>>,
}
impl Default for StartupDiagnostics {
    fn default() -> Self {
        Self::disabled()
    }
}
impl StartupDiagnostics {
    pub fn disabled() -> Self {
        Self {
            clock: None,
            timeline: None,
            trace: StartupTrace::new(false),
            output: None,
            main_window: None,
            measurement_error: None,
            submission: None,
        }
    }
    pub fn enabled(&self) -> bool {
        self.clock.is_some()
    }
    pub fn in_memory() -> Result<Self, super::clock::ClockError> {
        let clock = NativeClock::new()?;
        let entry = clock.now()?;
        let timeline = clock.current_process_timeline()?;
        let mut trace = StartupTrace::new(true);
        trace.mark(StartupPhase::RustEntry, entry.ticks)?;
        let submission = Arc::new(SubmissionProbe {
            clock: clock.clone(),
            first: OnceLock::new(),
        });
        Ok(Self {
            clock: Some(clock),
            timeline: Some(timeline),
            trace,
            output: None,
            main_window: None,
            measurement_error: None,
            submission: Some(submission),
        })
    }
    pub fn from_environment() -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_output(
            std::env::var_os("HEXFORGE_STARTUP_PROFILE")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
        )
    }
    pub fn from_output(output: Option<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let Some(output) = output else {
            return Ok(Self::disabled());
        };
        let mut diagnostics = Self::in_memory()?;
        // Development artifacts must not turn the one-executable distribution
        // into a sidecar-based runtime or overwrite a previous/user report.
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Use an absolute startup report path outside the executable folder.",
                )
            })?;
        if !output.is_absolute() || output.file_name().is_none() || output.try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The startup report must have a new absolute filename.",
            )
            .into());
        }
        let parent = parent.canonicalize()?;
        let executable = std::env::current_exe()?;
        let executable_parent = executable
            .parent()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot identify the executable folder.",
                )
            })?
            .canonicalize()?;
        if parent == executable_parent {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Write startup reports outside the portable executable folder.",
            )
            .into());
        }
        diagnostics.output = Some(parent.join(output.file_name().expect("validated filename")));
        Ok(diagnostics)
    }
    /// The production launcher supplies its actual CreationContext HWND. A
    /// headless egui run leaves this unset and cannot become window evidence.
    pub fn bind_main_window(&mut self, window: u64) {
        if !self.enabled() {
            return;
        }
        if window == 0 || self.main_window.is_some_and(|w| w != window) {
            self.measurement_error
                .get_or_insert_with(|| "Invalid main window identity.".into());
            return;
        }
        self.main_window = Some(window);
        self.mark(StartupPhase::NativeWindowCreated);
    }
    pub fn mark(&mut self, phase: StartupPhase) {
        let Some(clock) = &self.clock else {
            return;
        };
        if self.measurement_error.is_some() {
            return;
        }
        let mut clock_error = None;
        let result = self.trace.mark_with(phase, || match clock.now() {
            Ok(tick) => tick.ticks,
            Err(error) => {
                clock_error = Some(error.to_string());
                -1
            }
        });
        if let Some(error) = clock_error {
            self.measurement_error = Some(error);
        } else if let Err(error) = result {
            self.measurement_error = Some(error.to_string());
        }
    }
    pub fn report(&self) -> Option<NativeStartupReport> {
        let mut trace = self.trace.snapshot()?;
        let mut error = self.measurement_error.clone();
        if let Some(result) = self.submission.as_ref().and_then(|s| s.first.get()) {
            match result {
                Ok(at) => {
                    // The callback may run before later live input. Merge its
                    // exact first timestamp, rather than retiming it on report.
                    trace.marks.push(super::trace::PhaseMark {
                        phase: StartupPhase::FrameQueued,
                        qpc: at.ticks,
                    });
                    trace.marks.sort_by_key(|m| m.qpc);
                }
                Err(message) => {
                    error.get_or_insert_with(|| message.clone());
                }
            }
        }
        Some(NativeStartupReport {
            process: self.timeline.clone()?,
            main_window: self.main_window,
            trace,
            measurement_error: error,
            observation_status: "presentation-not-independently-observed",
        })
    }
    /// Root-only, opt-in marker at the real Glow draw callback boundary. UI
    /// construction/tessellation cannot execute it. This records submission of
    /// draw commands, NOT a swap, desktop presentation, or interaction readiness.
    pub fn paint_submission_probe(
        &self,
        painter: &eframe::egui::Painter,
        rect: eframe::egui::Rect,
    ) {
        let Some(probe) = &self.submission else {
            return;
        };
        if probe.first.get().is_some() {
            return;
        }
        let probe = probe.clone();
        painter.add(eframe::egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_, _| {
                probe
                    .first
                    .get_or_init(|| probe.clock.now().map_err(|e| e.to_string()));
            })),
        });
    }
}
impl Drop for StartupDiagnostics {
    fn drop(&mut self) {
        let (Some(path), Some(report)) = (&self.output, self.report()) else {
            return;
        };
        // Report after shutdown, not on the first-frame critical path. The probe
        // must end its controlled clean launch before consuming this artifact.
        // create_new also closes the check/write race without replacing a file.
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            serde_json::to_writer_pretty(file, &report)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Startup phase report was not written: {error}");
        }
    }
}
