//! Bounded, opt-in first-milestone tracing. This never samples clocks when off
//! and never treats a renderer's queued frame as presentation evidence.
use super::presentation::{FrameEvidence, MeasurementError};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum StartupPhase {
    RustEntry,
    ThemeReady,
    NativeWindowCreated,
    RendererReady,
    FontAssetsReady,
    ControllerReady,
    PreferencesReady,
    MainSceneBuilt,
    FrameQueued,
    MainDispatchAccepted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PhaseMark {
    pub phase: StartupPhase,
    pub qpc: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct TraceReport {
    pub marks: Vec<PhaseMark>,
    pub presented_frame: Option<FrameEvidence>,
}
#[derive(Default)]
pub struct StartupTrace {
    report: Option<TraceReport>,
}
impl StartupTrace {
    pub fn new(enabled: bool) -> Self {
        Self {
            report: enabled.then(|| TraceReport {
                marks: Vec::new(),
                presented_frame: None,
            }),
        }
    }
    pub fn mark_with(
        &mut self,
        phase: StartupPhase,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), MeasurementError> {
        // Short-circuit both the clock and duplicate work, not just log output.
        if self
            .report
            .as_ref()
            .is_none_or(|r| r.marks.iter().any(|m| m.phase == phase))
        {
            return Ok(());
        }
        self.mark(phase, clock())
    }
    pub fn mark(&mut self, phase: StartupPhase, qpc: i64) -> Result<(), MeasurementError> {
        let Some(report) = self.report.as_mut() else {
            return Ok(());
        };
        if report.marks.iter().any(|m| m.phase == phase) {
            return Ok(());
        }
        if qpc < 0 || report.marks.last().is_some_and(|m| m.qpc > qpc) {
            return Err(MeasurementError::InvalidInterval);
        }
        // The closed phase enum puts a strict bound on retained milestones.
        report.marks.push(PhaseMark { phase, qpc });
        Ok(())
    }
    pub fn snapshot(&self) -> Option<TraceReport> {
        self.report.clone()
    }
}
