# Parsing templates

HexForge uses one local, unversioned JSON format. A template is independent of the binary it describes. The root has exactly `name`, `defaultEndianness`, and `fields`:

```json
{
  "name": "Example",
  "defaultEndianness": "little",
  "fields": [
    { "name": "signature", "type": "bytes", "length": 4 },
    { "name": "recordCount", "type": "u16" }
  ]
}
```

`defaultEndianness` is `little` or `big`; an individual numeric field may override it with `endianness`. Fields parse in their listed order. Their names must be unique within their containing structure and cannot contain `.`, `[` or `]`. The parser rejects unknown JSON properties and invalid combinations rather than ignoring them. In particular, **any top-level `version` property is rejected**, whether its value is 1, 2, or something else. Old saved templates must be recreated manually; loading does not migrate them. A field within a binary format may still be named `version` (see the ELF example).

## Fields and placement

| Field kind | Required properties | Notes |
| --- | --- | --- |
| `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `bool` | `name`, `type` | `bool` uses one byte. Integers support `enumLabels`; unsigned integers also support `bitFlags`. |
| `string` | `name`, `type`, `encoding`, exactly one of `length` or `maxLength` | `encoding` is `ascii`, `utf8`, `utf16le`, or `utf16be`. `length` is a fixed byte count or bounded unsigned reference; numeric `maxLength` bounds a null-terminated read. UTF-16 bounds and actual lengths must be even. |
| `bytes` | `name`, `type`, `length` | Displays a fixed or reference-length byte sequence. |
| `struct` | `name`, `type`, `fields` | Children parse in order and appear under an expandable result node. |
| `array` | `name`, `type`, `count`, `element` | `count` is `{ "fixed": 3 }` or `{ "ref": "header.count" }`. The element is an unnamed, sequential, unconditional field definition. |

An omitted `placement` means sequential placement after the preceding field. `{"mode":"absolute","offset":"64"}` places a field at file byte 64; `{"mode":"relative","offset":"4"}` places it four bytes from its containing structure's base. `{"mode":"sequential"}` is the explicit sequential form. Literal offsets are **unsigned decimal strings**, not JSON numbers or hexadecimal strings, so large values remain exact across Rust and TypeScript. `align` optionally advances the start to a power-of-two byte boundary. Field byte offsets and lengths in parsed results are also decimal strings.

Fields may have a `comment`. Integer labels use decimal-string keys, for example `"enumLabels": {"1":"PCM"}`. Named bits use `"bitFlags": [{"bit":0,"name":"enabled"}]` on unsigned integer fields.

### Reference lengths and offsets

Existing numeric lengths and literal decimal offsets remain valid. A `length` may also be `{"ref":"header.size","max":256}`: the earlier unsigned field supplies the actual byte count, while `max` is a required positive bound up to 1 MiB. An actual count of zero is valid for referenced bytes/text and consumes no bytes. A fixed numeric length must still be positive. `maxLength` for null-terminated text remains a fixed positive bound.

A non-sequential placement's `offset` may be `{"ref":"header.payloadOffset","add":"2"}`. `add` is optional and is an unsigned decimal string (default zero); all arithmetic is checked u64 arithmetic. Absolute references are file-relative; relative references are added to the containing structure's base, then alignment is applied. All reads must remain inside the file. References resolve with the same scope and earlier-value rules as counts. No general expressions are evaluated.

This complete example reads a header, checks its marker, and follows a bounded payload pointer:

```json
{
  "name": "Pointer-based record",
  "defaultEndianness": "little",
  "fields": [
    {
      "name": "header", "type": "struct",
      "fields": [
        { "name": "marker", "type": "bytes", "length": 4,
          "expect": { "kind": "equals", "value": { "type": "bytes", "value": "44 41 54 41" } } },
        { "name": "payloadOffset", "type": "u64" },
        { "name": "size", "type": "u16" }
      ]
    },
    {
      "name": "payload", "type": "string", "encoding": "utf8",
      "length": { "ref": "header.size", "max": 256 },
      "placement": { "mode": "absolute", "offset": { "ref": "header.payloadOffset" } }
    }
  ]
}
```

### Expected-value checks

Optional `expect` is available on primitive, text, and bytes fields, not containers. There are exactly three shapes:

```json
{ "kind": "equals", "value": { "type": "string", "value": "IHDR" } }
```

```json
{ "kind": "oneOf", "values": [
  { "type": "unsigned", "value": "1" },
  { "type": "unsigned", "value": "2" }
] }
```

```json
{ "kind": "range", "min": { "type": "signed", "value": "-10" },
  "max": { "type": "signed", "value": "10" } }
```

Ranges are inclusive and numeric only. Literal types are `unsigned`, `signed`, `float`, `bool`, `string`, and `bytes` and must match the field type. Integers and finite floats use strings; booleans use JSON booleans; byte values use space-separated two-digit hex pairs, case-insensitively. `oneOf` accepts 1–256 values. A failed check produces `template_expectation_failed` at the affected range, with no successful value; dependent sequential parsing stops and CSV export is refused. Invalid text or boolean binary data is distinguished as `template_data_invalid`. The editor provides controls for all three checks and reference ranges.

## References and conditions

An array count reference must identify an earlier parsed **unsigned integer**. Conditions can compare an earlier value with a typed literal:

```json
{
  "name": "OptionalPayload",
  "defaultEndianness": "little",
  "fields": [
    { "name": "kind", "type": "u8", "enumLabels": { "1": "Data" } },
    { "name": "count", "type": "u8" },
    {
      "name": "payload",
      "type": "array",
      "condition": { "ref": "kind", "op": "eq", "value": { "type": "unsigned", "value": "1" } },
      "count": { "ref": "count" },
      "element": { "type": "u16" }
    }
  ]
}
```

Operators are `eq`, `ne`, `lt`, `lte`, `gt`, and `gte`. Literal types are `unsigned` or `signed` with a decimal string, `bool` with a JSON boolean, and `string` with JSON text. Boolean and text conditions support only `eq`/`ne`. References resolve from the current structure outward and must already have been parsed; there is no scripting or expression evaluation. Result leaves have stable paths such as `header.entries[2].id`. Clicking a successful leaf navigates to its exact byte range; errors appear as diagnostics without a misleading decoded value.

## Safety and workflow

The template file is limited to 1 MiB. Parsing is limited to eight nesting levels, 4,096 field definitions, 10,000 expanded result nodes, a 16 MiB decoded-data budget, and 1 MiB per individual read. Invalid templates and out-of-bounds or truncated data are reported without loading the entire binary. Applying a template reads the effective bytes, including unsaved in-memory hex edits.

The node budget includes array/structure containers. The decoded-data budget conservatively reserves four bytes per input byte for formatted output, plus result overhead, paths, repeated comments, enum labels, and bit-flag names. Fixed expansion is checked before reading, reference counts are checked during interpretation, and reference-length maxima are included in static validation. Known reference/type problems are rejected before Load/Save/Apply. Conditional and concrete indexed references are additionally checked at runtime. Parser reads use a disposable 64 KiB cache, with source checks around refills and before publishing results. Large nested/root parses report work in batches of 128 nodes (small top-level lists also report each field); the status bar shows nodes processed rather than an inaccurate percentage when work is data-dependent.

Load, Save, and Save As operate on the template JSON only. None automatically applies it to the open binary. You can also Apply an unsaved editor draft. Template saves are atomic, detect external changes to a bound template file, and refuse to overwrite the open binary. The Template Editor asks before discarding an unsaved draft.

## Bundled examples

Open a matching binary through **File → Open File…**, load its `.json` through **Template → Load Template…**, then choose **Apply Template**. To try a sample immediately, open the `.bin` fixture in the same row. The fixtures contain small example files, not executable programs. Their `.bin` extension does not change the binary format inside.

| Template | Matching binary fixture | Parsed portion |
| --- | --- | --- |
| [PNG / IHDR](png-ihdr.json) | [png-ihdr.bin](png-ihdr.bin) | PNG signature and the mandatory first IHDR chunk, including dimensions and CRC; not IDAT/IEND. |
| [BMP](bmp-header.json) | [bmp-header.bin](bmp-header.bin) | 14-byte file header and DIB size; expands dimensions, bit depth, compression, and resolution only for a 40-byte BITMAPINFOHEADER. Other DIB layouts are skipped; no palette or pixel decoding. |
| [GIF87a / GIF89a](gif-header.json) | [gif-header.bin](gif-header.bin) | First 13 bytes: signature, format revision, logical dimensions, packed flags, background index, and aspect-ratio code. No palettes, animation frames, extensions, or LZW decoding. |
| [ICO directory](ico-directory.json) | [ico-directory.bin](ico-directory.bin) | Six-byte directory header and every 16-byte image entry through a count reference. Image offsets and sizes are shown without following the PNG/DIB payloads. ICO only; CUR entry words have different meanings. |
| [DDS texture](dds-header.json) | [dds-header.bin](dds-header.bin) | 128-byte magic/header/pixel-format layout, flag labels, channel masks, and the optional 20-byte DX10 extension. Standard ASCII FourCCs only; no pixel or mipmap decoding. Fixture: one uncompressed RGBA pixel. |
| [RIFF/WAV](riff-wav.json) | [riff-wav.bin](riff-wav.bin) | RIFF/WAVE header, PCM `fmt ` chunk, `data` header, and the first 16-bit stereo sample frame; not the complete audio stream. |
| [ELF64](elf64-header.json) | [elf64-header.bin](elf64-header.bin) | The 64-byte little-endian ELF64 file header; not program or section table entries. |
| [ZIP local header](zip-local-header.json) | [zip-local-header.bin](zip-local-header.bin) | First 30-byte local file header, filename bytes, and raw extra bytes. Requires a local header at offset 0: no empty ZIP or leading self-extracting executable. No central-directory, ZIP64-extra, data-descriptor, or compressed-payload decoding. |
| [GZIP header](gzip-header.json) | [gzip-header.bin](gzip-header.bin) | Ten-byte fixed member header, including optional-field flags and timestamp. No optional name/comment/extra decoding, decompression, CRC validation, or member traversal. |
| [SQLite database](sqlite-header.json) | [sqlite-header.bin](sqlite-header.bin) | Complete 100-byte big-endian main-database header. Page-size value 1 is labeled as 65536 bytes. No b-tree records, SQL values, journal, or WAL parsing. Fixture: a valid empty 512-byte database. |
| [PCAP, little-endian](pcap-little-header.json) | [pcap-little-header.bin](pcap-little-header.bin) | Classic 24-byte global header. Choose for magic `D4 C3 B2 A1` (microseconds) or `4D 3C B2 A1` (nanoseconds). No packet records or PCAPNG decoding. |
| [PCAP, big-endian](pcap-big-header.json) | [pcap-big-header.bin](pcap-big-header.bin) | Same global header in big-endian order. Choose for magic `A1 B2 C3 D4` (microseconds) or `A1 B2 3C 4D` (nanoseconds). Each PCAP fixture contains one experimental Ethernet frame. |
| [GLB / binary glTF](glb-header.json) | [glb-header.bin](glb-header.bin) | 12-byte container header and, for GLB 2, the first eight-byte JSON chunk header. No asset-JSON, mesh, buffer, or later chunk decoding. Fixture: a minimal GLB 2 scene. |
| [UF2 firmware block](uf2-block.json) | [uf2-block.bin](uf2-block.bin) | First complete 512-byte block: magic values, flags, target address, block counts, and the full 476-byte data area including padding. No later blocks, extension/checksum decoding, or device operations. |

These templates do not automatically detect file formats. PNG/IHDR, classic PCM WAV, little-endian ELF64, and ZIP local-header examples now actively check their identifying values with `expect`. WAV requires a 16-byte PCM format chunk immediately followed by `data`; ELF requires the declared little-endian ELF64 layout. Checksums are not verified. Other examples still rely on the “Expected” comments for manual inspection; fields without checks can decode a mismatched format, so select the correct template and byte order. ICO directory counts and ZIP byte arrays remain subject to the existing 10,000-node expansion limit, so unusually large metadata is rejected rather than expanded without a bound. Use a bounded reference-length `bytes` field instead of a byte-by-byte array when per-byte result nodes are unnecessary.

The tiny fixtures can be regenerated from the repository root:

```sh
node templates/generate-fixtures.mjs
```

Pass one or more template stems to regenerate only selected fixtures:

```sh
node templates/generate-fixtures.mjs bmp-header sqlite-header
```

Rust integration tests load the new templates through the application's template-file loader and verify decoded values, exact offsets, conditional DIB/DX10 headers, and bounded ICO expansion:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test bundled_templates
```

### Format references

The additional layouts follow these primary specifications and implementation references:

- BMP: Microsoft [BITMAPFILEHEADER](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapfileheader) and [BITMAPINFOHEADER](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapinfoheader).
- GIF: the [GIF89a specification](https://www.w3.org/Graphics/GIF/spec-gif89a.txt).
- ICO: Microsoft's [ICO file-format description](https://devblogs.microsoft.com/oldnewthing/20101018-00/?p=12513).
- DDS: Microsoft [DDS_HEADER](https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header), [DDS_PIXELFORMAT](https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-pixelformat), and [DDS_HEADER_DXT10](https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header-dxt10).
- ZIP: PKWARE [APPNOTE, section 4.3.7](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT).
- GZIP: [RFC 1952, section 2.3](https://www.rfc-editor.org/rfc/rfc1952.html).
- SQLite: [Database File Format](https://www.sqlite.org/fileformat.html#the_database_header).
- PCAP: the libpcap project's [savefile format manual](https://github.com/the-tcpdump-group/libpcap/blob/master/pcap-savefile.manfile.in).
- GLB: Khronos [glTF specification, Binary glTF Layout](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#glb-file-format-specification).
- UF2: Microsoft's [UF2 specification](https://github.com/microsoft/uf2#file-format).
