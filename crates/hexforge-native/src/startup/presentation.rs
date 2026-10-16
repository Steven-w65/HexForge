//! Clock-correlated measurement and evidence validation, independent of egui.
//!
//! A screenshot of the application's own render target proves rendering, not
//! desktop presentation. An external observer must capture the visible window
//! and identify the actual primary controls before supplying `FrameEvidence`.
//! These guards validate that evidence; they do not manufacture an observation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockDomain {
    WindowsQpc { frequency: u64 },
    Uncorrelated(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tick {
    pub ticks: i64,
    pub domain: ClockDomain,
}
impl Tick {
    pub fn qpc(ticks: i64, frequency: u64) -> Self {
        Self {
            ticks,
            domain: ClockDomain::WindowsQpc { frequency },
        }
    }
    pub(crate) fn frequency(&self) -> Result<u64, MeasurementError> {
        match self.domain {
            ClockDomain::WindowsQpc { frequency } if frequency > 0 && self.ticks >= 0 => {
                Ok(frequency)
            }
            _ => Err(MeasurementError::UncorrelatedClock),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProcessTimeline {
    process_id: u32,
    started_at: Tick,
    origin_uncertainty_ticks: u64,
}
impl ProcessTimeline {
    /// `started_at` must describe OS process creation, not entry into Rust main.
    /// A FILETIME/QPC calibration supplies the separately reported uncertainty.
    pub fn new(
        process_id: u32,
        started_at: Tick,
        origin_uncertainty_ticks: u64,
    ) -> Result<Self, MeasurementError> {
        started_at.frequency()?;
        if process_id == 0 {
            return Err(MeasurementError::WrongFrameIdentity);
        }
        Ok(Self {
            process_id,
            started_at,
            origin_uncertainty_ticks,
        })
    }
    pub fn started_at(&self) -> &Tick {
        &self.started_at
    }
    pub fn origin_uncertainty_ticks(&self) -> u64 {
        self.origin_uncertainty_ticks
    }
    fn elapsed_ms(&self, at: &Tick) -> Result<f64, MeasurementError> {
        let frequency = at.frequency()?;
        if at.domain != self.started_at.domain {
            return Err(MeasurementError::UncorrelatedClock);
        }
        // Subtract exact counter values before the bounded duration conversion.
        let elapsed = i128::from(at.ticks) - i128::from(self.started_at.ticks);
        if elapsed < 0 {
            return Err(MeasurementError::InvalidInterval);
        }
        Ok(elapsed as f64 * 1000.0 / frequency as f64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryControl {
    NavigationMenus,
    OpenFile,
    HexViewport,
    ParsedResults,
    StatusBar,
}
impl PrimaryControl {
    pub const REQUIRED: [Self; 5] = [
        Self::NavigationMenus,
        Self::OpenFile,
        Self::HexViewport,
        Self::ParsedResults,
        Self::StatusBar,
    ];
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationMethod {
    IndependentWindowCapture,
    IndependentCompositor,
    ApplicationFrameQueue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameKind {
    RealMainInterface,
    Blank,
    Splash,
    Placeholder,
    WindowTitle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameEvidence {
    pub process_id: u32,
    pub window_id: u64,
    pub frame_sequence: u64,
    pub observed_at: Tick,
    /// Last independent capture which had not yet shown the real interface.
    /// No capture before this frame means only process creation is a lower bound.
    pub last_non_meaningful_at: Option<Tick>,
    pub method: ObservationMethod,
    pub kind: FrameKind,
    pub visible: bool,
    pub primary_controls: Vec<PrimaryControl>,
    /// SHA-256 of the observer's retained lossless image/presentation artifact.
    pub artifact_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionEvidence {
    pub process_id: u32,
    pub window_id: u64,
    pub accepted_at: Tick,
    pub command_sequence: u64,
    pub reached_controller: bool,
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementError {
    UncorrelatedClock,
    InvalidInterval,
    WrongFrameIdentity,
    IndependentPresentationRequired,
    RealInterfaceRequired,
    HiddenFrame,
    PrimaryControlsMissing,
    CaptureArtifactMissing,
    LiveDispatchRequired,
    SceneSubmissionMissing,
    PresentationMissing,
    InteractionMissing,
    NoValidRuns,
}
impl std::fmt::Display for MeasurementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UncorrelatedClock => {
                "Startup timestamps have not been correlated to the same QPC clock."
            }
            Self::InvalidInterval => "Startup evidence contains an invalid or reversed interval.",
            Self::WrongFrameIdentity => {
                "Startup evidence does not identify the launched process and main window."
            }
            Self::IndependentPresentationRequired => {
                "A queued application frame is not independent presentation evidence."
            }
            Self::RealInterfaceRequired => {
                "The observed frame does not show the real main interface."
            }
            Self::HiddenFrame => "The observed main window is not visible.",
            Self::PrimaryControlsMissing => {
                "The observed interface is missing required primary controls."
            }
            Self::CaptureArtifactMissing => {
                "The independent capture artifact hash is missing or invalid."
            }
            Self::LiveDispatchRequired => {
                "Interaction evidence must describe an accepted live controller dispatch."
            }
            Self::SceneSubmissionMissing => "No main interface scene submission has been recorded.",
            Self::PresentationMissing => {
                "No independent presentation of the real interface has been recorded."
            }
            Self::InteractionMissing => "No accepted live interaction has been recorded.",
            Self::NoValidRuns => "No verified startup runs are available.",
        })
    }
}
impl std::error::Error for MeasurementError {}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LaunchMetrics {
    /// Conservative upper bound: time of the first qualifying independent frame.
    pub first_meaningful_paint_ms: f64,
    pub first_meaningful_paint_lower_bound_ms: f64,
    pub capture_interval_ms: f64,
    pub clock_origin_uncertainty_ms: f64,
    pub total_uncertainty_ms: f64,
    pub scene_submission_ms: f64,
    pub interactive_ms: f64,
}

#[derive(Clone, Debug)]
pub struct StartupMeasurement {
    timeline: ProcessTimeline,
    main_window: Option<u64>,
    scene_submission: Option<Tick>,
    presented_frame: Option<FrameEvidence>,
    interaction: Option<InteractionEvidence>,
}
impl StartupMeasurement {
    pub fn new(timeline: ProcessTimeline) -> Self {
        Self {
            timeline,
            main_window: None,
            scene_submission: None,
            presented_frame: None,
            interaction: None,
        }
    }
    pub fn record_scene_submission(
        &mut self,
        window_id: u64,
        at: Tick,
    ) -> Result<(), MeasurementError> {
        self.timeline.elapsed_ms(&at)?;
        if window_id == 0 || self.main_window.is_some_and(|w| w != window_id) {
            return Err(MeasurementError::WrongFrameIdentity);
        }
        if self.scene_submission.is_none() {
            self.main_window = Some(window_id);
            self.scene_submission = Some(at);
        }
        Ok(())
    }
    fn validate_identity(
        &self,
        pid: u32,
        window: u64,
        sequence: u64,
    ) -> Result<(), MeasurementError> {
        if pid != self.timeline.process_id || Some(window) != self.main_window || sequence == 0 {
            Err(MeasurementError::WrongFrameIdentity)
        } else {
            Ok(())
        }
    }
    pub fn observe_presented_frame(
        &mut self,
        frame: FrameEvidence,
    ) -> Result<(), MeasurementError> {
        self.validate_identity(frame.process_id, frame.window_id, frame.frame_sequence)?;
        if frame.method == ObservationMethod::ApplicationFrameQueue {
            return Err(MeasurementError::IndependentPresentationRequired);
        }
        if frame.kind != FrameKind::RealMainInterface {
            return Err(MeasurementError::RealInterfaceRequired);
        }
        if !frame.visible {
            return Err(MeasurementError::HiddenFrame);
        }
        if !PrimaryControl::REQUIRED
            .iter()
            .all(|c| frame.primary_controls.contains(c))
        {
            return Err(MeasurementError::PrimaryControlsMissing);
        }
        if frame.artifact_hash.len() != 64
            || !frame.artifact_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(MeasurementError::CaptureArtifactMissing);
        }
        self.timeline.elapsed_ms(&frame.observed_at)?;
        if let Some(lower) = &frame.last_non_meaningful_at {
            self.timeline.elapsed_ms(lower)?;
        }
        let submitted = self
            .scene_submission
            .as_ref()
            .ok_or(MeasurementError::SceneSubmissionMissing)?;
        let lower = frame
            .last_non_meaningful_at
            .as_ref()
            .unwrap_or(&self.timeline.started_at);
        // Validate interval ordering in exact ticks, never rounded milliseconds.
        if lower.ticks > frame.observed_at.ticks || submitted.ticks > frame.observed_at.ticks {
            return Err(MeasurementError::InvalidInterval);
        }
        // Validate the entire observation before changing state. Neither stale
        // input nor a later frame may replace the first qualifying observation.
        if self.presented_frame.is_none() {
            self.presented_frame = Some(frame);
        }
        Ok(())
    }
    pub fn record_interaction(
        &mut self,
        interaction: InteractionEvidence,
    ) -> Result<(), MeasurementError> {
        self.validate_identity(
            interaction.process_id,
            interaction.window_id,
            interaction.command_sequence,
        )?;
        if !interaction.reached_controller || !interaction.accepted {
            return Err(MeasurementError::LiveDispatchRequired);
        }
        self.timeline.elapsed_ms(&interaction.accepted_at)?;
        if interaction.accepted_at.ticks
            < self
                .scene_submission
                .as_ref()
                .ok_or(MeasurementError::SceneSubmissionMissing)?
                .ticks
        {
            return Err(MeasurementError::InvalidInterval);
        }
        if self.interaction.is_none() {
            self.interaction = Some(interaction);
        }
        Ok(())
    }
    pub fn presented(&self) -> Option<&FrameEvidence> {
        self.presented_frame.as_ref()
    }
    pub fn metrics(&self) -> Result<LaunchMetrics, MeasurementError> {
        let frame = self
            .presented_frame
            .as_ref()
            .ok_or(MeasurementError::PresentationMissing)?;
        let interaction = self
            .interaction
            .as_ref()
            .ok_or(MeasurementError::InteractionMissing)?;
        let upper = self.timeline.elapsed_ms(&frame.observed_at)?;
        let lower = frame
            .last_non_meaningful_at
            .as_ref()
            .map(|t| self.timeline.elapsed_ms(t))
            .transpose()?
            .unwrap_or(0.0);
        let frequency = self.timeline.started_at.frequency()?;
        let lower_tick = frame
            .last_non_meaningful_at
            .as_ref()
            .unwrap_or(&self.timeline.started_at);
        let interval_ticks = i128::from(frame.observed_at.ticks) - i128::from(lower_tick.ticks);
        let interval_ms = interval_ticks as f64 * 1000.0 / frequency as f64;
        let clock_uncertainty =
            self.timeline.origin_uncertainty_ticks as f64 * 1000.0 / frequency as f64;
        Ok(LaunchMetrics {
            first_meaningful_paint_ms: upper,
            first_meaningful_paint_lower_bound_ms: lower,
            capture_interval_ms: interval_ms,
            clock_origin_uncertainty_ms: clock_uncertainty,
            total_uncertainty_ms: interval_ms + clock_uncertainty,
            scene_submission_ms: self.timeline.elapsed_ms(
                self.scene_submission
                    .as_ref()
                    .ok_or(MeasurementError::SceneSubmissionMissing)?,
            )?,
            // A handler accepting a pre-paint command does not make an unseen UI
            // interactive. Report the later of live acceptance and presentation.
            interactive_ms: upper.max(self.timeline.elapsed_ms(&interaction.accepted_at)?),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Distribution {
    pub median_ms: f64,
    pub p95_ms: f64,
    pub fastest_ms: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LaunchSummary {
    pub count: usize,
    pub paint: Distribution,
    pub interactive: Distribution,
}
fn distribution(mut values: Vec<f64>) -> Result<Distribution, MeasurementError> {
    if values.is_empty() {
        return Err(MeasurementError::NoValidRuns);
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    let median = if n % 2 == 0 {
        values[n / 2 - 1] / 2.0 + values[n / 2] / 2.0
    } else {
        values[n / 2]
    };
    let rank = n
        .checked_mul(95)
        .and_then(|v| v.checked_add(99))
        .ok_or(MeasurementError::InvalidInterval)?
        / 100;
    Ok(Distribution {
        median_ms: median,
        p95_ms: values[rank - 1],
        fastest_ms: values[0],
    })
}
pub fn summarize_launches(runs: &[StartupMeasurement]) -> Result<LaunchSummary, MeasurementError> {
    if runs.is_empty() {
        return Err(MeasurementError::NoValidRuns);
    }
    // Do not omit failed/unverified runs, trim slow observations, or insert zeros.
    let metrics = runs
        .iter()
        .map(StartupMeasurement::metrics)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LaunchSummary {
        count: metrics.len(),
        paint: distribution(
            metrics
                .iter()
                .map(|m| m.first_meaningful_paint_ms)
                .collect(),
        )?,
        interactive: distribution(metrics.iter().map(|m| m.interactive_ms).collect())?,
    })
}
pub fn variant_order<T: Clone>(variants: &[T], pair_index: usize) -> Vec<T> {
    let mut order = variants.to_vec();
    if pair_index % 2 != 0 {
        order.reverse();
    }
    order
}
