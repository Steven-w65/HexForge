//! Opt-in passive startup probe; never part of the portable distribution.
use hexforge_native::startup::{
    observer::{PixelRect, SceneSignature},
    presentation::PrimaryControl,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
#[cfg(windows)]
#[path = "support/probe_discovery.rs"]
mod discovery;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineReview {
    image_sha256: String,
    primary_controls: Vec<PrimaryControl>,
    note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Calibration {
    executable_sha256: String,
    image: PathBuf,
    image_sha256: String,
    size: [u32; 2],
    dpi: u32,
    regions: Vec<(PrimaryControl, PixelRect)>,
    review: Option<BaselineReview>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn publish_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn validate_destination(path: &Path, executable: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Err("Use a new absolute JSON report path.".into());
    }
    let parent = path
        .parent()
        .ok_or("Missing report directory.")?
        .canonicalize()?;
    let exe_parent = executable
        .canonicalize()?
        .parent()
        .ok_or("Missing executable directory.")?
        .to_path_buf();
    if parent == exe_parent {
        return Err("Report artifacts must stay outside the portable executable folder.".into());
    }
    let destination = parent.join(path.file_name().ok_or("Missing report filename.")?);
    for sibling in [
        destination.clone(),
        destination.with_extension("png"),
        destination.with_extension("phases.json"),
        destination.with_extension("first-visible.png"),
    ] {
        if sibling.try_exists()? {
            return Err(
                format!("Will not replace existing artifact: {}", sibling.display()).into(),
            );
        }
    }
    Ok(destination)
}
fn validate_review(calibration: &Calibration) -> Result<()> {
    let review = calibration.review.as_ref().ok_or("Inspect the independent baseline PNG and its five landmark rectangles before adding a review.")?;
    if review.image_sha256 != calibration.image_sha256
        || review.note.trim().is_empty()
        || review.primary_controls.len() != PrimaryControl::REQUIRED.len()
        || !PrimaryControl::REQUIRED
            .iter()
            .all(|r| review.primary_controls.iter().filter(|c| *c == r).count() == 1)
    {
        return Err("The baseline review does not bind all real primary controls to the unchanged capture hash.".into());
    }
    Ok(())
}
fn empty_scene_regions(width: u32, height: u32) -> Vec<(PrimaryControl, PixelRect)> {
    // Calibrated 1280x800/96DPI source layout only. Other DPI/preferences must
    // be calibrated separately, not stretched or passed through a loose mask.
    if [width, height] != [1280, 800] {
        return Vec::new();
    }
    vec![
        (
            PrimaryControl::NavigationMenus,
            PixelRect {
                x: 0,
                y: 0,
                width: 460,
                height: 34,
            },
        ),
        (
            PrimaryControl::OpenFile,
            PixelRect {
                x: 592,
                y: 338,
                width: 96,
                height: 32,
            },
        ),
        (
            PrimaryControl::HexViewport,
            PixelRect {
                x: 570,
                y: 257,
                width: 143,
                height: 72,
            },
        ),
        (
            PrimaryControl::ParsedResults,
            PixelRect {
                x: 0,
                y: 555,
                width: 1280,
                height: 30,
            },
        ),
        (
            PrimaryControl::StatusBar,
            PixelRect {
                x: 0,
                y: 776,
                width: 1280,
                height: 24,
            },
        ),
    ]
}
fn png(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(bytes.into_inner())
}
fn record_baseline(
    output: &Path,
    executable: &Path,
    image: &image::RgbaImage,
    dpi: u32,
) -> Result<()> {
    let bytes = png(image)?;
    let calibration = Calibration {
        executable_sha256: hash(&fs::read(executable)?),
        image: output.with_extension("png"),
        image_sha256: hash(&bytes),
        size: [image.width(), image.height()],
        dpi,
        regions: empty_scene_regions(image.width(), image.height()),
        review: None,
    };
    // Keep the independent pixels even when the observer rejects them. A
    // rejected / unreviewed calibration cannot qualify a launch measurement.
    publish_new(&calibration.image, &bytes)?;
    publish_new(output, &serde_json::to_vec_pretty(&calibration)?)?;
    SceneSignature::from_baseline(image, &calibration.regions).map_err(|e| {
        format!(
            "Rejected calibration {}x{} at {} DPI: {e}. Actual pixels retained at {}.",
            image.width(),
            image.height(),
            dpi,
            calibration.image.display()
        )
    })?;
    Ok(())
}
fn read_calibration(path: &Path, executable: &Path) -> Result<(Calibration, SceneSignature)> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err("Unbounded calibration metadata.".into());
    }
    let calibration: Calibration = serde_json::from_slice(&bytes)?;
    validate_review(&calibration)?;
    if hash(&fs::read(executable)?) != calibration.executable_sha256 {
        return Err("Executable differs from the reviewed calibration build.".into());
    }
    let image_bytes = fs::read(&calibration.image)?;
    if hash(&image_bytes) != calibration.image_sha256 {
        return Err("Baseline capture hash changed.".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(image_bytes));
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128_000_000);
    reader.limits(limits);
    let image = reader.decode()?.into_rgba8();
    if [image.width(), image.height()] != calibration.size {
        return Err("Baseline dimensions changed.".into());
    }
    let signature = SceneSignature::from_baseline(&image, &calibration.regions)?;
    Ok((calibration, signature))
}

#[cfg(windows)]
fn run() -> Result<()> {
    use hexforge_native::startup::{
        clock::NativeClock,
        desktop::{find_main_window, DesktopCapture},
        observer::{CaptureBracket, CaptureHistory},
    };
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let usage = "Usage: hexforge_startup_probe baseline EXE PID HWND OUT.json | observe|watch EXE CALIBRATION.json OUT.json native|reference";
    if args.len() != 5 {
        return Err(usage.into());
    }
    let executable = Path::new(&args[1]).canonicalize()?;
    match args[0].as_str() {
        "baseline" => {
            let output = validate_destination(Path::new(&args[4]), &executable)?;
            let pid = args[2].parse()?;
            let window = args[3].parse()?;
            let mut capture = DesktopCapture::new(pid, window)?;
            let image = capture.capture()?.ok_or_else(|| {
                format!(
                    "The main window is not yet visible, active and unobscured: {}",
                    capture
                        .diagnostic_status()
                        .unwrap_or_else(|e| e.to_string())
                )
            })?;
            record_baseline(&output, &executable, &image, capture.dpi())?;
            println!("Captured independent desktop baseline: {}. Review is pending; it cannot yet measure a launch.",output.display());
            Ok(())
        }
        "observe" | "watch" => {
            let output = validate_destination(Path::new(&args[3]), &executable)?;
            if !["native", "reference"].contains(&args[4].as_str()) {
                return Err(usage.into());
            }
            let (calibration, signature) = read_calibration(Path::new(&args[2]), &executable)?;
            let clock = NativeClock::new()?;
            let mut command = std::process::Command::new(&executable);
            command.env_remove("HEXFORGE_STARTUP_PROFILE");
            let phases = output.with_extension("phases.json");
            if args[4] == "native" {
                command.env("HEXFORGE_STARTUP_PROFILE", &phases);
            }
            // No GUI automation or auto-termination: the task's separate live
            // UI verification sends a real command and closes this clean app.
            let externally_launched = args[0] == "watch";
            let (mut child, pid, process) = if externally_launched {
                let armed = clock.now()?;
                println!("Observer armed; launch the exact executable through Computer Use. No input or activation is sent by this probe.");
                let limit = std::time::Instant::now() + std::time::Duration::from_secs(60);
                let (pid, _, process) = loop {
                    if let Some(found) = discovery::find_new_root(&executable, &armed, &clock)? {
                        break found;
                    }
                    if std::time::Instant::now() > limit {
                        return Err(
                            "No new matching root window appeared after arming the observer."
                                .into(),
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                };
                (None, pid, process)
            } else {
                let child = command.spawn()?;
                let pid = child.id();
                let process = clock.process_timeline(pid)?;
                (Some(child), pid, process)
            };
            let mut capture: Option<DesktopCapture> = None;
            let mut window = None;
            let mut history = CaptureHistory::default();
            let mut first_visible = None;
            let mut frame = None;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let observation = (|| -> Result<()> {
                loop {
                    if let Some(child) = child.as_mut() {
                        if child.try_wait()?.is_some() {
                            return Err("Launched process exited before a real presented interface was observed.".into());
                        }
                    }
                    if std::time::Instant::now() > deadline {
                        return Err(format!("No matching presented interface within ten seconds. Preserve this failed run. {}",
                            capture.as_ref().map(|c| c.diagnostic_status().unwrap_or_else(|e| e.to_string())).unwrap_or_else(|| "No root window discovered.".into())).into());
                    }
                    let started = clock.now()?;
                    if capture.is_none() {
                        if let Some(id) = find_main_window(pid)? {
                            window = Some(id);
                            capture = Some(DesktopCapture::new(pid, id)?);
                        }
                    }
                    let image = match capture.as_mut() {
                        Some(c) => c.capture()?,
                        None => None,
                    };
                    let finished = clock.now()?;
                    let bracket = CaptureBracket::new(started, finished)?;
                    if let Some(image) = image {
                        if capture.as_ref().expect("window capture").dpi() != calibration.dpi {
                            return Err("Launch DPI differs from the reviewed baseline.".into());
                        }
                        if signature.matches(&image) {
                            let bytes = png(&image)?;
                            let evidence = history.qualifying_frame(
                                pid,
                                window.expect("identified window"),
                                bracket,
                                hash(&bytes),
                            )?;
                            publish_new(&output.with_extension("png"), &bytes)?;
                            frame = Some(evidence);
                            break;
                        }
                        if first_visible.is_none() {
                            first_visible = Some(image);
                        }
                        history.record_negative(bracket)?;
                    }
                    // No pixels while waiting for creation/reveal/activation:
                    // keep the conservative lower bound at process launch,
                    // rather than inventing a negative desktop observation.
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Ok(())
            })();
            if let Some(image) = first_visible {
                publish_new(&output.with_extension("first-visible.png"), &png(&image)?)?;
            }
            let milliseconds = |at: i64| {
                (i128::from(at) - i128::from(process.started_at().ticks)) as f64 * 1000.0
                    / match process.started_at().domain {
                        hexforge_native::startup::presentation::ClockDomain::WindowsQpc {
                            frequency,
                        } => frequency as f64,
                        _ => unreachable!(),
                    }
            };
            let upper = frame.as_ref().map(|f| milliseconds(f.observed_at.ticks));
            let lower = frame.as_ref().map(|f| {
                f.last_non_meaningful_at
                    .as_ref()
                    .map(|t| milliseconds(t.ticks))
                    .unwrap_or(0.0)
            });
            let report = serde_json::json!({
                "status": if observation.is_ok() { "presentation-observed; interaction-pending" } else { "failed" },
                "error": observation.as_ref().err().map(|e|e.to_string()),
                "process": process,"frame":frame,"calibration":Path::new(&args[2]).canonicalize()?,
                "baselineSha256":calibration.image_sha256,"executableSha256":calibration.executable_sha256,
                "paintUpperMs":upper,"paintLowerMs":lower,"captureIntervalMs":upper.zip(lower).map(|(u,l)|u-l),
                "clockOriginUncertaintyTicks":process.origin_uncertainty_ticks(),
                "phaseReport":if args[4]=="native" && !externally_launched { Some(phases) } else { None },
                "launchMethod":if externally_launched { "External Computer Use launch; passive new-process discovery; diagnostics off" } else { "Child launched by passive observer" },
                "method":"Independent client pixels from desktop DC; QPC brackets; strict five-landmark RGB match; no app readiness signal",
                "interaction":"Not measured. Requires a real live input and its accepted response; no automatic readiness claim.",
                "coldClassification":"Not OS-cold. Caller must document warm or labeled fresh-state/profile conditions.",
            });
            publish_new(&output, &serde_json::to_vec_pretty(&report)?)?;
            println!("Process {pid}; report {}. Presented interval {lower:?}..{upper:?} ms. Real input/clean close remains separate.",output.display());
            observation
        }
        _ => Err(usage.into()),
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Independent desktop startup measurement requires Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("Startup probe failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejected_calibration_retains_the_actual_capture_without_approving_it() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("hexforge.exe");
        std::fs::write(&exe, b"owned executable fixture").unwrap();
        let output = root.path().join("baseline.json");
        let image = image::RgbaImage::from_pixel(1280, 800, image::Rgba([0, 0, 0, 255]));
        let error = super::record_baseline(&output, &exe, &image, 96).unwrap_err();
        assert!(error.to_string().contains("1280x800 at 96 DPI"));
        let calibration: super::Calibration =
            serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert!(calibration.review.is_none());
        assert!(super::validate_review(&calibration).is_err());
        let retained = image::open(&calibration.image).unwrap().into_rgba8();
        assert_eq!(retained, image);
        assert_eq!(calibration.size, [1280, 800]);
        assert_eq!(
            calibration.image_sha256,
            super::hash(&std::fs::read(&calibration.image).unwrap())
        );
    }

    #[test]
    fn reports_require_new_absolute_destinations_outside_the_portable_folder() {
        let root = tempfile::tempdir().unwrap();
        let portable = root.path().join("portable");
        std::fs::create_dir(&portable).unwrap();
        let exe = portable.join("hexforge.exe");
        std::fs::write(&exe, b"owned fixture").unwrap();
        assert!(super::validate_destination(&portable.join("report.json"), &exe).is_err());
        assert!(super::validate_destination(std::path::Path::new("relative.json"), &exe).is_err());
        let destination = root.path().join("report.json");
        assert!(super::validate_destination(&destination, &exe).is_ok());
        super::publish_new(&destination, b"first").unwrap();
        assert!(super::validate_destination(&destination, &exe).is_err());
        assert!(super::publish_new(&destination, b"second").is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"first");
        let sibling = root.path().join("other.json");
        std::fs::write(sibling.with_extension("png"), b"retained capture").unwrap();
        assert!(super::validate_destination(&sibling, &exe).is_err());
    }

    #[test]
    fn unreviewed_or_hash_changed_baselines_cannot_supply_a_paint_measurement() {
        let mut calibration = super::Calibration {
            executable_sha256: "a".repeat(64),
            image: "unused.png".into(),
            image_sha256: "b".repeat(64),
            size: [1280, 800],
            dpi: 96,
            regions: super::empty_scene_regions(1280, 800),
            review: None,
        };
        assert!(super::validate_review(&calibration).is_err());
        calibration.review = Some(super::BaselineReview {
            image_sha256: "b".repeat(64),
            primary_controls: hexforge_native::startup::presentation::PrimaryControl::REQUIRED
                .to_vec(),
            note: "Manually inspected actual primary controls.".into(),
        });
        assert!(super::validate_review(&calibration).is_ok());
        calibration.image_sha256 = "c".repeat(64);
        assert!(super::validate_review(&calibration).is_err());
        calibration.image_sha256 = "b".repeat(64);
        calibration.review.as_mut().unwrap().primary_controls.pop();
        assert!(super::validate_review(&calibration).is_err());
    }
}
