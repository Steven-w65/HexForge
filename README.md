# HexForge

HexForge is a modern, lightweight desktop hex viewer and flat binary-template analysis utility built with Tauri 2, Vue 3, TypeScript, Rust, and a hand-written Canvas renderer. It is designed for developers, firmware debuggers, reverse engineers, and game asset analysts.

HexForge is fully offline. File paths, file bytes, templates, and parsed results stay on the local computer; the application contains no HTTP client, updater, analytics, telemetry, remote font, or network upload path.

## Prerequisites

- Node.js `^20.19.0` or `>=22.12.0` and npm 11.6.3 (the version pinned in `package.json`).
- A current stable Rust toolchain installed with `rustup`.
- The [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/) for the target OS. On Windows this includes Microsoft C++ Build Tools and WebView2; Linux and macOS require their platform-specific webview and build packages.

Install the exact JavaScript dependency graph from `package-lock.json` on a fresh checkout:

```sh
npm ci
```

Use `npm install` only when intentionally updating dependency declarations and the lockfile.

## Development and verification

Run the desktop application in development mode:

```sh
npm run tauri dev
```

Run the complete frontend test, type, and production-build checks:

```sh
npm test
npm run typecheck
npm run build
```

Run the Rust backend tests:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
```

Build an installable desktop bundle:

```sh
npm run tauri build
```

## MVP behavior

- Open any local binary file, including `.bin`, `.rom`, and `.raw`, with the toolbar, `Ctrl+O`, or drag-and-drop.
- View 16 or 32 bytes per row in Offset, Hex Bytes, and ASCII columns. Non-printable ASCII bytes appear as `.`.
- Select one byte or drag an inclusive range, jump to decimal or `0x`-prefixed offsets, and search hexadecimal byte sequences.
- Toggle dark and light themes in one click. Dark is the default; the bundled JetBrains Mono font is local and never fetched from a remote service.
- Enable edit mode to replace bytes with values from `00` through `FF`. Modified bytes are orange.
- Apply flat JSON templates, inspect parsed results, click fields to navigate to their byte ranges, and export parsed results as CSV.

## Data safety and large files

The source file is opened read-only and is never overwritten in place. HexForge reads only bounded pages near the visible viewport, so opening a multi-gigabyte file does not load the whole file into memory. Edits are sparse and remain only in memory until exported.

`Ctrl+Z` provides a basic Undo stack; Redo is intentionally not part of the MVP. **Save As always creates a brand-new file** containing the source plus current in-memory edits. It refuses the source path and any destination that already exists. A modified indicator and close confirmation protect unsaved edits.

Search, parsing, Save As, and CSV export run through bounded backend operations so the UI remains responsive. Friendly dialogs report missing files, invalid templates, permission errors, corrupt input, and insufficient disk space.

## Parsing templates

Templates are independent local JSON files. The MVP supports flat fields only—no nested structures or conditional logic—with these types:

- `u8`, `u16`, `u32`
- `i8`, `i16`, `i32`
- `f32`, `f64`
- fixed-length `string`
- fixed-length `bytes`

Numeric fields may use little-endian or big-endian byte order. `string` and `bytes` fields require a positive `length`. Offsets are decimal strings so large-file addresses remain exact across the frontend/backend boundary.

See [`templates/firmware-header.json`](templates/firmware-header.json) for a complete sample.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+O` | Open file |
| `Ctrl+F` | Open byte search |
| `Ctrl+G` | Go to offset |
| `Ctrl+S` | Save template |
| `Ctrl+Z` | Undo the most recent edit |
| `Esc` | Close the active popup, otherwise clear the selection |

Save As is deliberately a toolbar action rather than `Ctrl+S`; `Ctrl+S` belongs to template saving in this MVP.
