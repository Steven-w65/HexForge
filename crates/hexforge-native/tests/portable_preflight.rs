#![cfg(windows)]
use std::{fs, path::Path, process::Command};

fn run(executable: &Path, destination: &Path, report: &Path) -> std::process::Output {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/verify_portable_native.ps1");
    // Use the available PowerShell7 development runtime; never bypass or
    // change the machine's script execution policy to run this verifier.
    Command::new("pwsh.exe")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(script)
        .arg("-Executable")
        .arg(executable)
        .arg("-Destination")
        .arg(destination)
        .arg("-Report")
        .arg(report)
        .arg("-PrepareOnly")
        .output()
        .unwrap()
}

#[test]
fn portable_directory_contains_only_the_exact_executable_without_a_live_pass() {
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("portable-测试");
    let report = folder.path().join("preflight.json");
    let executable = std::env::current_exe().unwrap();
    let output = run(&executable, &destination, &report);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let files: Vec<_> = fs::read_dir(&destination)
        .unwrap()
        .map(|f| f.unwrap().file_name())
        .collect();
    assert_eq!(files, ["hexforge.exe"]);
    assert_eq!(
        fs::read(destination.join("hexforge.exe")).unwrap(),
        fs::read(&executable).unwrap()
    );
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(
        evidence["status"],
        "preflight-passed; live-verification-pending"
    );
    assert_eq!(evidence["liveVerified"], false);
    assert_eq!(evidence["startupMeasured"], false);
    assert_eq!(evidence["files"], serde_json::json!(["hexforge.exe"]));
    assert_eq!(evidence["sourceSha256"], evidence["portableSha256"]);
}

#[test]
fn portable_preflight_refuses_existing_destination_without_touching_its_files() {
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("already-present");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep.txt"), "existing user content").unwrap();
    let report = folder.path().join("preflight.json");
    let output = run(&std::env::current_exe().unwrap(), &destination, &report);
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(destination.join("keep.txt")).unwrap(),
        "existing user content"
    );
    assert_eq!(fs::read_dir(destination).unwrap().count(), 1);
    assert!(!report.exists());
}

#[test]
fn portable_preflight_refuses_report_inside_distribution_before_creating_it() {
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("portable");
    let output = run(
        &std::env::current_exe().unwrap(),
        &destination,
        &destination.join("report.json"),
    );
    assert!(!output.status.success());
    assert!(
        !destination.exists(),
        "Do not create a sidecar-bearing distribution"
    );
}

#[test]
fn portable_preflight_preserves_existing_report_and_rejects_non_executables() {
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("portable");
    let report = folder.path().join("report.json");
    fs::write(&report, "earlier independent evidence").unwrap();
    let output = run(&std::env::current_exe().unwrap(), &destination, &report);
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(&report).unwrap(),
        "earlier independent evidence"
    );
    assert!(!destination.exists());
    let invalid = folder.path().join("invalid.exe");
    fs::write(&invalid, "not a PE executable").unwrap();
    let new_report = folder.path().join("new.json");
    let output = run(&invalid, &destination, &new_report);
    assert!(!output.status.success());
    assert!(!destination.exists());
    assert!(!new_report.exists());
}
