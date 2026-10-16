//! Development-only recognition of independently captured desktop pixels.
//! This does not capture the app's render target, read a ready/title flag, or
//! inject input. Calibrate against an inspected, fully presented real window at
//! the same size, DPI and theme. All five landmarks must match exactly in RGB.
use super::presentation::{
    FrameEvidence, FrameKind, MeasurementError, ObservationMethod, PrimaryControl, Tick,
};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

pub const MAX_CAPTURE_PIXELS: u64 = 32_000_000;
const MAX_LANDMARK_PIXELS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl PixelRect {
    fn within(self, image: &RgbaImage) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|x| x <= image.width())
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|y| y <= image.height())
    }
}

#[derive(Debug)]
pub struct InvalidScene;
impl std::fmt::Display for InvalidScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The inspected baseline must contain five distinct, bounded primary-control landmarks with visible content.")
    }
}
impl std::error::Error for InvalidScene {}

/// A 32-bit top-down DIB has BGRA storage. Desktop alpha is undefined; do not
/// turn displayed pixels transparent or reverse their rows. Reject malformed
/// lengths before allocating the bounded RGBA copy.
pub fn decode_top_down_bgra(
    width: u32,
    height: u32,
    bytes: &[u8],
) -> Result<RgbaImage, InvalidScene> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || pixels > MAX_CAPTURE_PIXELS
        || usize::try_from(pixels * 4).ok() != Some(bytes.len())
    {
        return Err(InvalidScene);
    }
    let mut rgba = Vec::with_capacity(bytes.len());
    for pixel in bytes.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    RgbaImage::from_raw(width, height, rgba).ok_or(InvalidScene)
}

pub struct SceneSignature {
    size: (u32, u32),
    landmarks: Vec<(PixelRect, Box<[u8]>)>,
}
impl SceneSignature {
    /// The caller must visually inspect the retained independent baseline and
    /// its role rectangles first. A pure recognizer cannot infer what words or
    /// functions arbitrary pixels represent. Do not calibrate on a splash.
    pub fn from_baseline(
        image: &RgbaImage,
        regions: &[(PrimaryControl, PixelRect)],
    ) -> Result<Self, InvalidScene> {
        if u64::from(image.width()) * u64::from(image.height()) > MAX_CAPTURE_PIXELS
            || regions.len() != PrimaryControl::REQUIRED.len()
            || !PrimaryControl::REQUIRED
                .iter()
                .all(|role| regions.iter().filter(|(r, _)| r == role).count() == 1)
            || regions.iter().any(|(_, r)| !r.within(image))
            || regions
                .iter()
                .map(|(_, r)| u64::from(r.width) * u64::from(r.height))
                .sum::<u64>()
                > MAX_LANDMARK_PIXELS
        {
            return Err(InvalidScene);
        }
        let mut landmarks = Vec::with_capacity(regions.len());
        for (_, rect) in regions {
            let mut rgb = Vec::with_capacity(rect.width as usize * rect.height as usize * 3);
            let background = image.get_pixel(rect.x, rect.y);
            let mut visible_pixels = 0;
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    let pixel = image.get_pixel(x, y);
                    // Reject uniform/near-uniform background calibration. Exact
                    // matching below deliberately has no similarity threshold.
                    if (0..3).any(|i| pixel[i].abs_diff(background[i]) >= 24) {
                        visible_pixels += 1;
                    }
                    rgb.extend_from_slice(&pixel.0[..3]);
                }
            }
            if visible_pixels < 8 {
                return Err(InvalidScene);
            }
            landmarks.push((*rect, rgb.into_boxed_slice()));
        }
        Ok(Self {
            size: image.dimensions(),
            landmarks,
        })
    }
    pub fn matches(&self, image: &RgbaImage) -> bool {
        if image.dimensions() != self.size {
            return false;
        }
        self.landmarks.iter().all(|(rect, rgb)| {
            let mut offset = 0;
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    if image.get_pixel(x, y).0[..3] != rgb[offset..offset + 3] {
                        return false;
                    }
                    offset += 3;
                }
            }
            true
        })
    }
}

#[derive(Clone, Debug)]
pub struct CaptureBracket {
    started: Tick,
    finished: Tick,
}
impl CaptureBracket {
    pub fn new(started: Tick, finished: Tick) -> Result<Self, MeasurementError> {
        started.frequency()?;
        finished.frequency()?;
        if started.domain != finished.domain {
            return Err(MeasurementError::UncorrelatedClock);
        }
        if started.ticks > finished.ticks {
            return Err(MeasurementError::InvalidInterval);
        }
        Ok(Self { started, finished })
    }
}

#[derive(Default)]
pub struct CaptureHistory {
    negative: Option<CaptureBracket>,
    sequence: u64,
}
impl CaptureHistory {
    fn validate_next(&self, next: &CaptureBracket) -> Result<(), MeasurementError> {
        if let Some(previous) = &self.negative {
            if previous.finished.domain != next.started.domain {
                return Err(MeasurementError::UncorrelatedClock);
            }
            if previous.finished.ticks > next.started.ticks {
                return Err(MeasurementError::InvalidInterval);
            }
        }
        Ok(())
    }
    pub fn record_negative(&mut self, capture: CaptureBracket) -> Result<(), MeasurementError> {
        self.validate_next(&capture)?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(MeasurementError::InvalidInterval)?;
        self.negative = Some(capture);
        self.sequence = sequence;
        Ok(())
    }
    /// Only call after the external window's identity, visibility/unocclusion
    /// and five real pixel landmarks have been verified. The last negative
    /// capture's *start* is the safe lower bound: pixels can change mid-read.
    pub fn qualifying_frame(
        &self,
        process_id: u32,
        window_id: u64,
        capture: CaptureBracket,
        artifact_hash: String,
    ) -> Result<FrameEvidence, MeasurementError> {
        self.validate_next(&capture)?;
        if process_id == 0 || window_id == 0 {
            return Err(MeasurementError::WrongFrameIdentity);
        }
        if artifact_hash.len() != 64 || !artifact_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(MeasurementError::CaptureArtifactMissing);
        }
        Ok(FrameEvidence {
            process_id,
            window_id,
            frame_sequence: self
                .sequence
                .checked_add(1)
                .ok_or(MeasurementError::InvalidInterval)?,
            observed_at: capture.finished,
            last_non_meaningful_at: self.negative.as_ref().map(|c| c.started.clone()),
            method: ObservationMethod::IndependentWindowCapture,
            kind: FrameKind::RealMainInterface,
            visible: true,
            primary_controls: PrimaryControl::REQUIRED.to_vec(),
            artifact_hash,
        })
    }
}
