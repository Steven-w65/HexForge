#[path = "support/fixtures.rs"]
mod fixtures;

use std::path::PathBuf;

fn samples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates")
}

#[test]
fn fixture_regeneration_is_byte_identical() {
    let generated = fixtures::generated_fixtures();
    let mut actual_names: Vec<_> = generated.iter().map(|(name, _)| *name).collect();
    actual_names.sort();
    assert_eq!(
        actual_names,
        [
            "bmp-header",
            "dds-header",
            "elf64-header",
            "gif-header",
            "glb-header",
            "gzip-header",
            "ico-directory",
            "pcap-big-header",
            "pcap-little-header",
            "png-ihdr",
            "riff-wav",
            "sqlite-header",
            "uf2-block",
            "zip-local-header",
        ]
    );
    // Original checked-in bytes are the independent baseline. No generator
    // reads them as inputs, and an ordinary test never writes sample files.
    for (name, bytes) in generated {
        let original = std::fs::read(samples().join(format!("{name}.bin"))).unwrap();
        assert_eq!(bytes, original, "Native generator changed {name}.bin");
    }
}

#[test]
#[ignore = "explicit fixture regeneration writes the bundled .bin samples"]
fn regenerate_bundled_fixtures() {
    // Run only after the default byte-identical test passed. Files are the
    // fourteen explicitly named bundled binaries, never user-saved JSON.
    let generated = fixtures::generated_fixtures();
    assert_eq!(
        generated.len(),
        14,
        "Do not regenerate a partial fixture set"
    );
    for (name, bytes) in generated {
        std::fs::write(samples().join(format!("{name}.bin")), bytes).unwrap();
    }
}
