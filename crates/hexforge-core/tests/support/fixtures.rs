//! Deterministic bundled-fixture generation, kept outside the shipping parser.
use flate2::{write::ZlibEncoder, Compression, GzBuilder};
use std::io::Write;

fn put(out: &mut [u8], offset: usize, bytes: &[u8]) {
    out[offset..offset + bytes.len()].copy_from_slice(bytes);
}
fn le16(out: &mut [u8], offset: usize, value: u16) {
    put(out, offset, &value.to_le_bytes());
}
fn le32(out: &mut [u8], offset: usize, value: u32) {
    put(out, offset, &value.to_le_bytes());
}
fn be16(out: &mut [u8], offset: usize, value: u16) {
    put(out, offset, &value.to_be_bytes());
}
fn be32(out: &mut [u8], offset: usize, value: u32) {
    put(out, offset, &value.to_be_bytes());
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}
fn png_chunk(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut body = name.to_vec();
    body.extend_from_slice(data);
    let mut out = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

pub fn generated_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let mut fixtures = Vec::with_capacity(14);
    let mut ihdr = vec![0; 13];
    be32(&mut ihdr, 0, 1);
    be32(&mut ihdr, 4, 1);
    ihdr[8] = 8;
    ihdr[9] = 6; // A single red RGBA pixel, filter0.
    let mut compressed = ZlibEncoder::new(Vec::new(), Compression::default());
    compressed.write_all(&[0, 255, 0, 0, 255]).unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &compressed.finish().unwrap()));
    png.extend(png_chunk(b"IEND", &[]));
    fixtures.push(("png-ihdr", png.clone()));

    let mut wav = vec![0; 48];
    put(&mut wav, 0, b"RIFF");
    le32(&mut wav, 4, 40);
    put(&mut wav, 8, b"WAVEfmt ");
    le32(&mut wav, 16, 16);
    le16(&mut wav, 20, 1);
    le16(&mut wav, 22, 2);
    le32(&mut wav, 24, 8000);
    le32(&mut wav, 28, 32000);
    le16(&mut wav, 32, 4);
    le16(&mut wav, 34, 16);
    put(&mut wav, 36, b"data");
    le32(&mut wav, 40, 4);
    put(&mut wav, 44, &1000_i16.to_le_bytes());
    put(&mut wav, 46, &(-1000_i16).to_le_bytes());
    fixtures.push(("riff-wav", wav));

    let mut elf = vec![0; 64];
    put(&mut elf, 0, b"\x7fELF");
    put(&mut elf, 4, &[2, 1, 1]);
    le16(&mut elf, 16, 2);
    le16(&mut elf, 18, 62);
    le32(&mut elf, 20, 1);
    put(&mut elf, 24, &0x401000_u64.to_le_bytes());
    for offset in [52, 54, 58] {
        le16(&mut elf, offset, 64);
    }
    fixtures.push(("elf64-header", elf));

    let mut bmp = vec![0; 58];
    put(&mut bmp, 0, b"BM");
    for (offset, value) in [
        (2, 58),
        (10, 54),
        (14, 40),
        (18, 1),
        (22, 1),
        (34, 4),
        (38, 2835),
        (42, 2835),
    ] {
        le32(&mut bmp, offset, value);
    }
    le16(&mut bmp, 26, 1);
    le16(&mut bmp, 28, 24);
    bmp[56] = 255; // 1x1 bottom-up24bit BGR, four-byte row alignment.
    fixtures.push(("bmp-header", bmp));

    let mut gif = vec![0; 13];
    put(&mut gif, 0, b"GIF89a");
    le16(&mut gif, 6, 1);
    le16(&mut gif, 8, 1);
    gif[10] = 0x80;
    gif.extend([255, 0, 0, 0, 0, 0]);
    gif.extend([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
    gif.extend([2, 2, 0x44, 0x01, 0, 0x3b]);
    fixtures.push(("gif-header", gif));

    let mut ico = vec![0; 22];
    le16(&mut ico, 2, 1);
    le16(&mut ico, 4, 1);
    ico[6] = 1;
    ico[7] = 1;
    le16(&mut ico, 10, 1);
    le16(&mut ico, 12, 32);
    le32(&mut ico, 14, u32::try_from(png.len()).unwrap());
    le32(&mut ico, 18, 22);
    ico.extend(png);
    fixtures.push(("ico-directory", ico));

    let mut dds = vec![0; 132];
    put(&mut dds, 0, b"DDS ");
    for (offset, value) in [
        (4, 124),
        (8, 0x100f),
        (12, 1),
        (16, 1),
        (20, 4),
        (76, 32),
        (80, 0x41),
        (88, 32),
        (92, 0x00ff0000),
        (96, 0x0000ff00),
        (100, 0x000000ff),
        (104, 0xff000000),
        (108, 0x1000),
        (128, 0xffff0000),
    ] {
        le32(&mut dds, offset, value);
    }
    fixtures.push(("dds-header", dds));

    let name = b"hello.txt";
    let text = b"HexForge\n";
    let checksum = crc32(text);
    let mut zip = vec![0; 30];
    le32(&mut zip, 0, 0x04034b50);
    le16(&mut zip, 4, 20);
    le16(&mut zip, 6, 0x0800);
    le16(&mut zip, 12, 0x0021);
    le32(&mut zip, 14, checksum);
    le32(&mut zip, 18, text.len() as u32);
    le32(&mut zip, 22, text.len() as u32);
    le16(&mut zip, 26, name.len() as u16);
    zip.extend_from_slice(name);
    zip.extend_from_slice(text);
    let central_offset = zip.len();
    let mut central = vec![0; 46];
    le32(&mut central, 0, 0x02014b50);
    le16(&mut central, 4, 20);
    le16(&mut central, 6, 20);
    le16(&mut central, 8, 0x0800);
    le16(&mut central, 14, 0x0021);
    le32(&mut central, 16, checksum);
    le32(&mut central, 20, text.len() as u32);
    le32(&mut central, 24, text.len() as u32);
    le16(&mut central, 28, name.len() as u16);
    zip.extend(central);
    zip.extend_from_slice(name);
    let mut end = vec![0; 22];
    le32(&mut end, 0, 0x06054b50);
    le16(&mut end, 8, 1);
    le16(&mut end, 10, 1);
    le32(&mut end, 12, (46 + name.len()) as u32);
    le32(&mut end, 16, central_offset as u32);
    zip.extend(end);
    fixtures.push(("zip-local-header", zip));

    // Fixed mtime/OS/level match the original host-independent Node fixture.
    let mut gzip = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), Compression::best());
    gzip.write_all(text).unwrap();
    fixtures.push(("gzip-header", gzip.finish().unwrap()));

    let mut sqlite = vec![0; 512];
    put(&mut sqlite, 0, b"SQLite format 3\0");
    be16(&mut sqlite, 16, 512);
    put(&mut sqlite, 18, &[1, 1]);
    put(&mut sqlite, 21, &[64, 32, 32]);
    for (offset, value) in [(24, 1), (28, 1), (44, 4), (56, 1), (92, 1), (96, 3046000)] {
        be32(&mut sqlite, offset, value);
    }
    sqlite[100] = 0x0d;
    be16(&mut sqlite, 105, 512);
    fixtures.push(("sqlite-header", sqlite));

    for little in [true, false] {
        let mut pcap = vec![0; 24];
        let u32 = if little { le32 } else { be32 };
        let u16 = if little { le16 } else { be16 };
        u32(&mut pcap, 0, 0xa1b2c3d4);
        u16(&mut pcap, 4, 2);
        u16(&mut pcap, 6, 4);
        u32(&mut pcap, 16, 65535);
        u32(&mut pcap, 20, 1);
        let mut header = vec![0; 16];
        for (offset, value) in [(0, 1700000000), (4, 123456), (8, 60), (12, 60)] {
            u32(&mut header, offset, value);
        }
        let mut packet = vec![0; 60];
        packet[..6].fill(255);
        packet[6] = 2;
        packet[11] = 1;
        be16(&mut packet, 12, 0x88b5);
        put(&mut packet, 14, text);
        pcap.extend(header);
        pcap.extend(packet);
        fixtures.push((
            if little {
                "pcap-little-header"
            } else {
                "pcap-big-header"
            },
            pcap,
        ));
    }

    let json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{}]}"#;
    let mut chunk = vec![0x20; (json.len() + 3) & !3];
    put(&mut chunk, 0, json);
    let mut glb = vec![0; 20];
    put(&mut glb, 0, b"glTF");
    le32(&mut glb, 4, 2);
    le32(&mut glb, 8, (20 + chunk.len()) as u32);
    le32(&mut glb, 12, chunk.len() as u32);
    put(&mut glb, 16, b"JSON");
    glb.extend(chunk);
    fixtures.push(("glb-header", glb));

    let mut uf2 = vec![0; 512];
    for (offset, value) in [
        (0, 0x0a324655),
        (4, 0x9e5d5157),
        (12, 0x10002000),
        (16, 256),
        (24, 1),
        (508, 0x0ab16f30),
    ] {
        le32(&mut uf2, offset, value);
    }
    put(&mut uf2, 32, &[0xde, 0xad, 0xbe, 0xef]);
    fixtures.push(("uf2-block", uf2));
    fixtures
}
