//! Inspect Cargo's resolved shipping graph, not manifest text or the reference
//! stack retained elsewhere in this checkout. These checks never launch UI.
use std::{path::Path, process::Command};

fn cargo(args: &[&str]) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(root)
        .args(args)
        .env("CARGO_TERM_COLOR", "never");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW: diagnostic CLI only.
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "Cargo diagnostic failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() < 16 * 1024 * 1024);
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn shipping_dependency_graph_uses_native_ui_without_webview_or_javascript_runtime() {
    let graph = cargo(&[
        "tree",
        "--offline",
        "-p",
        "hexforge-native",
        "--edges",
        "normal,build",
        "--prefix",
        "none",
    ]);
    let packages: Vec<_> = graph
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for package in [
        "hexforge-core",
        "hexforge-controller",
        "eframe",
        "egui",
        "glutin",
        "rfd",
    ] {
        assert!(
            packages.contains(&package),
            "missing shipping native dependency {package}"
        );
    }
    for package in [
        "tauri",
        "tauri-runtime",
        "tauri-runtime-wry",
        "wry",
        "tao",
        "webview2-com",
        "webview2-com-sys",
        "webkit2gtk",
        "vue",
        "vite",
        "nodejs",
        "node",
        "napi",
    ] {
        assert!(
            !packages.contains(&package),
            "reference/runtime dependency leaked into native product: {package}"
        );
    }
}

#[test]
fn cargo_native_target_is_one_application_executable_with_embedded_assets() {
    let metadata: serde_json::Value = serde_json::from_str(&cargo(&[
        "metadata",
        "--offline",
        "--no-deps",
        "--format-version",
        "1",
    ]))
    .unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let native = packages
        .iter()
        .find(|package| package["name"] == "hexforge-native")
        .unwrap();
    let binaries: Vec<_> = native["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| target["kind"] == serde_json::json!(["bin"]))
        .collect();
    assert_eq!(binaries.len(), 1);
    assert_eq!(binaries[0]["name"], "hexforge");
    assert_eq!(
        packages.len(),
        3,
        "reference Tauri crate must not enter the shipping workspace"
    );
    assert_eq!(
        image::load_from_memory(hexforge_native::assets::ICON_PNG)
            .unwrap()
            .width(),
        32
    );
    for bytes in [
        hexforge_native::assets::REGULAR_FONT,
        hexforge_native::assets::SEMIBOLD_FONT,
        hexforge_native::assets::BOLD_FONT,
    ] {
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        assert!(face.glyph_index('0').is_some() && face.glyph_index('A').is_some());
    }
}
