//! Independent capture recognition, not an app-owned ready/title signal.
use hexforge_native::startup::{
    observer::{decode_top_down_bgra, CaptureBracket, CaptureHistory, PixelRect, SceneSignature},
    presentation::{MeasurementError, PrimaryControl, Tick},
};
use image::{Rgba, RgbaImage};

fn baseline() -> (RgbaImage, Vec<(PrimaryControl, PixelRect)>) {
    // Five deliberately different hand-checked landmarks. Changing the
    // recognizer to accept a blank, partial or translated frame must fail.
    let mut image = RgbaImage::from_pixel(60, 20, Rgba([17, 20, 24, 255]));
    let roles = PrimaryControl::REQUIRED
        .iter()
        .enumerate()
        .map(|(i, role)| {
            let x = 12 * i as u32;
            for y in 3..7 {
                for dx in 3..7 {
                    image.put_pixel(x + dx, y, Rgba([150 + i as u8, 170, 190, 255]));
                }
            }
            (
                *role,
                PixelRect {
                    x,
                    y: 0,
                    width: 10,
                    height: 10,
                },
            )
        })
        .collect();
    (image, roles)
}

#[test]
fn recognition_requires_all_five_real_pixel_landmarks_at_the_recorded_positions() {
    let (image, roles) = baseline();
    let signature = SceneSignature::from_baseline(&image, &roles).unwrap();
    assert!(signature.matches(&image));
    for (_, rect) in &roles {
        let mut missing = image.clone();
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                missing.put_pixel(x, y, Rgba([17, 20, 24, 255]));
            }
        }
        assert!(
            !signature.matches(&missing),
            "missing control at x={}",
            rect.x
        );
    }
    assert!(!signature.matches(&RgbaImage::from_pixel(60, 20, Rgba([255; 4]))));
    assert!(!signature.matches(&RgbaImage::from_pixel(60, 20, Rgba([17, 20, 24, 255]))));
    assert!(!signature.matches(&RgbaImage::new(61, 20)));
}

#[test]
fn a_title_splash_or_partial_interface_cannot_be_calibrated_as_the_main_scene() {
    let (image, roles) = baseline();
    assert!(SceneSignature::from_baseline(&image, &roles[..4]).is_err());
    let mut duplicate = roles.clone();
    duplicate[4].0 = PrimaryControl::NavigationMenus;
    assert!(SceneSignature::from_baseline(&image, &duplicate).is_err());
    let flat = RgbaImage::from_pixel(60, 20, Rgba([20, 20, 20, 255]));
    assert!(SceneSignature::from_baseline(&flat, &roles).is_err());
    let mut outside = roles.clone();
    outside[0].1.x = u32::MAX;
    assert!(SceneSignature::from_baseline(&image, &outside).is_err());
    outside[0].1 = PixelRect {
        x: 59,
        y: 0,
        width: 10,
        height: 10,
    };
    assert!(SceneSignature::from_baseline(&image, &outside).is_err());
}

#[test]
fn desktop_capture_alpha_is_not_confused_with_visible_rgb_content() {
    let (image, roles) = baseline();
    let signature = SceneSignature::from_baseline(&image, &roles).unwrap();
    let mut desktop = image.clone();
    for pixel in desktop.pixels_mut() {
        pixel[3] = 0;
    }
    assert!(signature.matches(&desktop));
    // No threshold: even one different RGB pixel in a landmark rejects it.
    desktop.put_pixel(3, 3, Rgba([149, 170, 190, 0]));
    assert!(!signature.matches(&desktop));
}

#[test]
fn capture_lower_bound_is_negative_capture_start_not_its_finish() {
    let mut history = CaptureHistory::default();
    history
        .record_negative(CaptureBracket::new(Tick::qpc(100, 1000), Tick::qpc(108, 1000)).unwrap())
        .unwrap();
    let positive = CaptureBracket::new(Tick::qpc(110, 1000), Tick::qpc(120, 1000)).unwrap();
    let evidence = history
        .qualifying_frame(42, 77, positive, "a".repeat(64))
        .unwrap();
    // Presentation can occur during a readback. Use the conservative start of
    // the last unsuccessful read and the finish of the successful read.
    assert_eq!(evidence.last_non_meaningful_at, Some(Tick::qpc(100, 1000)));
    assert_eq!(evidence.observed_at, Tick::qpc(120, 1000));
    assert_eq!(evidence.frame_sequence, 2);
    assert_eq!(evidence.primary_controls, PrimaryControl::REQUIRED);
}

#[test]
fn capture_history_rejects_mixed_clocks_reverse_or_overlapping_observations() {
    assert_eq!(
        CaptureBracket::new(Tick::qpc(10, 1000), Tick::qpc(9, 1000)).unwrap_err(),
        MeasurementError::InvalidInterval
    );
    assert_eq!(
        CaptureBracket::new(Tick::qpc(10, 1000), Tick::qpc(12, 2000)).unwrap_err(),
        MeasurementError::UncorrelatedClock
    );
    let mut history = CaptureHistory::default();
    history
        .record_negative(CaptureBracket::new(Tick::qpc(10, 1000), Tick::qpc(20, 1000)).unwrap())
        .unwrap();
    assert_eq!(
        history.record_negative(
            CaptureBracket::new(Tick::qpc(19, 1000), Tick::qpc(25, 1000)).unwrap()
        ),
        Err(MeasurementError::InvalidInterval)
    );
    assert_eq!(
        history
            .qualifying_frame(
                42,
                77,
                CaptureBracket::new(Tick::qpc(21, 2000), Tick::qpc(25, 2000)).unwrap(),
                "a".repeat(64)
            )
            .unwrap_err(),
        MeasurementError::UncorrelatedClock
    );
    // Rejected input cannot alter the previous conservative lower bound.
    let ok = history
        .qualifying_frame(
            42,
            77,
            CaptureBracket::new(Tick::qpc(21, 1000), Tick::qpc(25, 1000)).unwrap(),
            "a".repeat(64),
        )
        .unwrap();
    assert_eq!(ok.last_non_meaningful_at, Some(Tick::qpc(10, 1000)));
}

#[test]
fn desktop_pixels_keep_top_down_row_order_and_bgr_channel_order_without_alpha() {
    let image =
        decode_top_down_bgra(2, 2, &[3, 2, 1, 0, 6, 5, 4, 0, 9, 8, 7, 0, 12, 11, 10, 0]).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 255]);
    assert_eq!(image.get_pixel(1, 1).0, [10, 11, 12, 255]);
    assert!(decode_top_down_bgra(2, 2, &[0; 15]).is_err());
    assert!(decode_top_down_bgra(0, 2, &[]).is_err());
    assert!(decode_top_down_bgra(u32::MAX, u32::MAX, &[]).is_err());
}

#[test]
#[cfg(windows)]
fn external_observer_correlates_the_real_process_creation_not_its_attach_time() {
    use hexforge_native::startup::clock::NativeClock;
    let clock = NativeClock::new().unwrap();
    let before = clock.current_process_timeline().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let attached = clock.process_timeline(std::process::id()).unwrap();
    let after = clock.now().unwrap();
    assert_eq!(before.started_at().domain, attached.started_at().domain);
    // Independent precise FILETIME samples, same OS creation, exact QPC domain.
    // One-ms tolerance includes OS wall clock adjustment and sample uncertainty.
    let frequency = match after.domain {
        hexforge_native::startup::presentation::ClockDomain::WindowsQpc { frequency } => frequency,
        _ => unreachable!(),
    };
    assert!(
        before
            .started_at()
            .ticks
            .abs_diff(attached.started_at().ticks)
            < frequency / 1000
    );
    assert!(after.ticks - attached.started_at().ticks >= (frequency / 50) as i64);
    assert!(clock.process_timeline(0).is_err());
}

#[test]
#[cfg(windows)]
fn desktop_observer_rejects_missing_or_foreign_window_identity_before_capture() {
    use hexforge_native::startup::desktop::DesktopCapture;
    assert!(DesktopCapture::new(std::process::id(), 0).is_err());
    assert!(DesktopCapture::new(0, 1).is_err());
    assert!(DesktopCapture::new(std::process::id(), u64::MAX).is_err());
}
