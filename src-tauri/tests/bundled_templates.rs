//! Verify sample definitions against independently checked format offsets and values.
use hexforge_lib::session::FileSession;
use hexforge_lib::template::{parse_template, validate_template, ParsedNode, TemplateDefinition};
use hexforge_lib::template_file::TemplateFileSession;
use std::path::PathBuf;

fn samples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../templates")
}

fn definition(stem: &str) -> TemplateDefinition {
    let path = samples().join(format!("{stem}.json"));
    assert!(
        path.is_file(),
        "Missing bundled template: {}",
        path.display()
    );
    let document = TemplateFileSession::default().load(&path).unwrap();
    validate_template(&document).unwrap();
    assert!(serde_json::to_value(&document)
        .unwrap()
        .get("version")
        .is_none());
    document
}

fn parse_bytes(stem: &str, bytes: &[u8]) -> Result<Vec<ParsedNode>, hexforge_lib::error::AppError> {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 256, 2).unwrap();
    parse_template(&mut session, &definition(stem))
}

fn find<'a>(nodes: &'a [ParsedNode], path: &str) -> Option<&'a ParsedNode> {
    nodes.iter().find_map(|node| {
        if node.path == path {
            Some(node)
        } else {
            find(&node.children, path)
        }
    })
}

fn assert_success(nodes: &[ParsedNode]) {
    for node in nodes {
        assert!(
            node.diagnostics.is_empty(),
            "{}: {:?}",
            node.path,
            node.diagnostics
        );
        assert_ne!(node.kind, "error", "{}", node.path);
        assert_success(&node.children);
    }
}

fn assert_leaf(nodes: &[ParsedNode], path: &str, offset: &str, length: &str, value: &str) {
    let node = find(nodes, path).unwrap_or_else(|| panic!("Missing result {path}"));
    assert_eq!(node.kind, "leaf", "{path}");
    assert_eq!(node.offset.as_deref(), Some(offset), "{path} offset");
    assert_eq!(node.length.as_deref(), Some(length), "{path} length");
    assert_eq!(node.value.as_deref(), Some(value), "{path} value");
}

macro_rules! sample {
    ($test:ident, $stem:literal, $expected:expr) => {
        #[test]
        fn $test() {
            let path = samples().join(concat!($stem, ".bin"));
            let document = definition($stem);
            assert!(path.is_file(), "Missing fixture: {}", path.display());
            let mut session = FileSession::open(path, 256, 2).unwrap();
            let nodes = parse_template(&mut session, &document).unwrap();
            assert_success(&nodes);
            for (path, offset, length, value) in $expected {
                assert_leaf(&nodes, path, offset, length, value);
            }
        }
    };
}

sample!(
    bmp_header,
    "bmp-header",
    [
        ("fileHeader.signature", "0", "2", "BM"),
        ("fileHeader.pixelDataOffset", "10", "4", "54"),
        ("dibHeader.info.width", "18", "4", "1"),
        ("dibHeader.info.bitsPerPixel", "28", "2", "24"),
    ]
);

sample!(
    gif_header,
    "gif-header",
    [
        ("header.signature", "0", "3", "GIF"),
        ("header.version", "3", "3", "89a"),
        ("screen.width", "6", "2", "1"),
        ("screen.packedFlags", "10", "1", "128"),
    ]
);

sample!(
    ico_directory,
    "ico-directory",
    [
        ("directory.imageCount", "4", "2", "1"),
        ("entries[0].width", "6", "1", "1"),
        ("entries[0].bitsPerPixel", "12", "2", "32"),
        ("entries[0].imageOffset", "18", "4", "22"),
    ]
);

sample!(
    dds_header,
    "dds-header",
    [
        ("magic", "0", "4", "DDS "),
        ("header.width", "16", "4", "1"),
        ("header.pixelFormat.flags", "80", "4", "65"),
        ("header.pixelFormat.redMask", "92", "4", "16711680"),
        ("header.caps", "108", "4", "4096"),
    ]
);

sample!(
    zip_local_header,
    "zip-local-header",
    [
        ("localHeader.signature", "0", "4", "50 4B 03 04"),
        ("localHeader.flags", "6", "2", "2048"),
        ("localHeader.compressionMethod", "8", "2", "0"),
        ("localHeader.fileNameLength", "26", "2", "9"),
        ("fileNameBytes[0]", "30", "1", "104"),
    ]
);

sample!(
    gzip_header,
    "gzip-header",
    [
        ("header.magic", "0", "2", "1F 8B"),
        ("header.compressionMethod", "2", "1", "8"),
        ("header.flags", "3", "1", "0"),
        ("header.modifiedTime", "4", "4", "0"),
    ]
);

sample!(
    sqlite_header,
    "sqlite-header",
    [
        ("header.magic", "0", "16", "SQLite format 3"),
        ("header.pageSize", "16", "2", "512"),
        ("header.pageCount", "28", "4", "1"),
        ("header.textEncoding", "56", "4", "1"),
        ("header.sqliteVersion", "96", "4", "3046000"),
    ]
);

sample!(
    pcap_little_endian,
    "pcap-little-header",
    [
        ("header.magic", "0", "4", "D4 C3 B2 A1"),
        ("header.majorVersion", "4", "2", "2"),
        ("header.minorVersion", "6", "2", "4"),
        ("header.snapshotLength", "16", "4", "65535"),
        ("header.linkTypeAndFlags", "20", "4", "1"),
    ]
);

sample!(
    pcap_big_endian,
    "pcap-big-header",
    [
        ("header.magic", "0", "4", "A1 B2 C3 D4"),
        ("header.majorVersion", "4", "2", "2"),
        ("header.minorVersion", "6", "2", "4"),
        ("header.snapshotLength", "16", "4", "65535"),
        ("header.linkTypeAndFlags", "20", "4", "1"),
    ]
);

sample!(
    glb_header,
    "glb-header",
    [
        ("header.magic", "0", "4", "glTF"),
        ("header.version", "4", "4", "2"),
        ("firstChunk.chunkType", "16", "4", "JSON"),
    ]
);

sample!(
    uf2_block,
    "uf2-block",
    [
        ("block.startMagic0", "0", "4", "55 46 32 0A"),
        ("block.targetAddress", "12", "4", "268443648"),
        ("block.payloadSize", "16", "4", "256"),
        ("block.totalBlocks", "24", "4", "1"),
        ("block.endMagic", "508", "4", "30 6F B1 0A"),
    ]
);

#[test]
fn bmp_skips_other_dib_layouts_instead_of_misreading_them_as_bitmapinfoheader() {
    let mut bytes = std::fs::read(samples().join("bmp-header.bin")).unwrap();
    bytes[14..18].copy_from_slice(&12_u32.to_le_bytes());
    bytes.truncate(26); // A BITMAPCOREHEADER is only 12 bytes.
    let nodes = parse_bytes("bmp-header", &bytes).unwrap();
    assert_success(&nodes);
    assert!(find(&nodes, "dibHeader.info").is_none());
    assert_leaf(&nodes, "dibHeader.size", "14", "4", "12");
}

#[test]
fn dds_reads_dx10_extension_only_when_the_fourcc_requests_it() {
    let mut bytes = std::fs::read(samples().join("dds-header.bin")).unwrap();
    let legacy = parse_bytes("dds-header", &bytes).unwrap();
    assert!(find(&legacy, "dx10").is_none());
    bytes.truncate(128);
    bytes[80..84].copy_from_slice(&4_u32.to_le_bytes());
    bytes[84..88].copy_from_slice(b"DX10");
    bytes[88..108].fill(0);
    for value in [28_u32, 3, 0, 1, 1] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend([255, 0, 0, 255]);
    let nodes = parse_bytes("dds-header", &bytes).unwrap();
    assert_success(&nodes);
    assert_leaf(&nodes, "dx10.dxgiFormat", "128", "4", "28");
    assert_leaf(&nodes, "dx10.resourceDimension", "132", "4", "3");
    assert_leaf(&nodes, "dx10.arraySize", "140", "4", "1");
}

#[test]
fn ico_directory_handles_zero_entries_and_rejects_excessive_expansion() {
    let nodes = parse_bytes("ico-directory", &[0, 0, 1, 0, 0, 0]).unwrap();
    assert_success(&nodes);
    assert!(find(&nodes, "entries").unwrap().children.is_empty());
    let error = parse_bytes("ico-directory", &[0, 0, 1, 0, 255, 255]).unwrap_err();
    assert_eq!(error.code(), "invalid_template");
    assert_eq!(error.detail.as_deref(), Some("entries"));
    assert!(error.message.contains("expanded-result limit"));
}
