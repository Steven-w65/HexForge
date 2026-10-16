//! Passive development-probe selection: an already-running window must never
//! become a fast "new launch", and a same-named executable is not its identity.
#[path = "../examples/support/probe_discovery.rs"]
mod discovery;

use hexforge_native::startup::presentation::{ProcessTimeline, Tick};
use std::path::Path;

#[test]
fn watch_selects_only_the_exact_executable_created_after_arming() {
    let armed = Tick::qpc(100, 10_000_000);
    let old = ProcessTimeline::new(7, Tick::qpc(99, 10_000_000), 1).unwrap();
    let fresh = ProcessTimeline::new(8, Tick::qpc(101, 10_000_000), 1).unwrap();
    let expected = Path::new("D:/test/portable/hexforge.exe");
    assert!(!discovery::is_new_identity(expected, expected, &armed, &old).unwrap());
    assert!(discovery::is_new_identity(expected, expected, &armed, &fresh).unwrap());
    assert!(!discovery::is_new_identity(
        expected,
        Path::new("D:/different/hexforge.exe"),
        &armed,
        &fresh,
    )
    .unwrap());
}

#[test]
fn watch_never_combines_different_clock_domains() {
    let process = ProcessTimeline::new(8, Tick::qpc(101, 10_000_000), 1).unwrap();
    let exe = Path::new("D:/test/hexforge.exe");
    assert!(discovery::is_new_identity(exe, exe, &Tick::qpc(100, 1_000), &process).is_err());
}

#[test]
fn watch_rejects_launches_whose_creation_uncertainty_crosses_the_arming_boundary() {
    let armed = Tick::qpc(100, 10_000_000);
    let exe = Path::new("D:/test/hexforge.exe");
    // An estimated start just after arming is not sufficient if its lower
    // bound includes an already-running process. Do not claim a fast launch.
    let ambiguous = ProcessTimeline::new(8, Tick::qpc(101, 10_000_000), 2).unwrap();
    let definite = ProcessTimeline::new(9, Tick::qpc(103, 10_000_000), 2).unwrap();
    assert!(!discovery::is_new_identity(exe, exe, &armed, &ambiguous).unwrap());
    assert!(discovery::is_new_identity(exe, exe, &armed, &definite).unwrap());
}

#[test]
#[cfg(windows)]
fn passive_process_image_lookup_reads_the_real_process_and_rejects_missing_identity() {
    let actual = discovery::process_image(std::process::id()).unwrap();
    assert_eq!(
        actual.canonicalize().unwrap(),
        std::env::current_exe().unwrap().canonicalize().unwrap()
    );
    assert!(discovery::process_image(0).is_err());
    assert!(discovery::process_image(u32::MAX).is_err());
}

#[test]
#[cfg(windows)]
fn armed_watcher_cannot_reuse_the_existing_test_process_as_a_new_launch() {
    let clock = hexforge_native::startup::clock::NativeClock::new().unwrap();
    let armed = clock.now().unwrap();
    assert!(discovery::find_new_root(
        &std::env::current_exe().unwrap().canonicalize().unwrap(),
        &armed,
        &clock,
    )
    .unwrap()
    .is_none());
}
