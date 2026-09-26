<p align="center">
  <img src="src-tauri/icons/hexforge.svg" width="96" alt="HexForge logo" />
</p>

<h1 align="center">HexForge</h1>

<p align="center"><strong>See every byte. Keep the original intact.</strong></p>

<p align="center">
  A focused, offline hex viewer and binary analysis tool for developers, firmware debuggers,
  reverse engineers, and game asset analysts.
</p>

<p align="center"><code>Tauri 2</code> · <code>Vue 3</code> · <code>TypeScript</code> · <code>Rust</code> · <code>Canvas</code></p>

<p align="center">
  <a href="#what-you-can-do">Features</a> ·
  <a href="#get-started">Get started</a> ·
  <a href="#parsing-templates">Templates</a> ·
  <a href="#keyboard-shortcuts">Shortcuts</a>
</p>

---

## What you can do

| Capability | What it gives you |
| --- | --- |
| **Inspect large binaries** | Open any local binary file, including `.bin`, `.rom`, and `.raw`. Rust reads bounded pages near the viewport instead of loading even a multi-gigabyte file at once. A self-drawn, layered Canvas view shows offsets, hex bytes, and ASCII. |
| **Find your place** | Switch between 16 and 32 bytes per row, select a byte or range, jump to decimal or `0x` offsets, and highlight matches for hex searches such as `41 42 43`. Non-printable ASCII appears as `.`. |
| **Edit safely** | Enable edit mode, change a byte using a validated `00`–`FF` value, and undo the latest edits. Orange marks in-memory modifications. Save As creates a new binary file; it never overwrites the original. |
| **Understand structure** | Load or create a flat JSON template in a separate Template Editor window. Apply it when ready, inspect results below the hex view, jump to a field's byte range, and export parsed results to CSV. |

Long-running search, parsing, Save As, and export operations report progress without freezing the interface. Dark mode is the default; light mode is one toggle away, and JetBrains Mono is bundled locally. The application has no telemetry, updater, or file-upload path: files, paths, templates, and parsed results stay on your computer.

## Get started

### Portable Windows app

The [Windows build workflow](.github/workflows/build.yaml) produces a portable `hexforge.exe` and uploads it as the `HexForge-windows-x64-portable` artifact after a successful run. In [GitHub Actions](https://github.com/Steven-w65/HexForge/actions), open a successful **Build Portable Windows EXE** run, download its artifact, extract it, and launch the executable. There is no HexForge installer. Windows still needs the Microsoft Edge WebView2 runtime used by Tauri.

### Run or build from source

You will need a compatible Node.js version (`^20.19.0` or `>=22.12.0`), npm (the project specifies `11.6.3`), stable Rust, and the [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/). On Windows, development also requires Microsoft C++ Build Tools and WebView2.

```sh
npm ci
npm run tauri -- dev
```

To build the portable executable without an installer:

```sh
npm run tauri -- build --no-bundle
```

On Windows, the output is `src-tauri/target/release/hexforge.exe`. The repository's GitHub Actions workflow runs the same portable build.

## A safe editing model

HexForge opens the current binary source **read-only**. Byte edits live in a sparse in-memory buffer, and the source file is never saved over in place. `File → Save As…` writes a **new** file containing the source bytes plus your edits, then keeps that saved copy visible in the viewer. HexForge refuses the original path and an already-existing binary destination.

The status bar shows the selected offset and byte, row width, mode, endianness, and whether there are unsaved binary changes. Closing with unsaved edits asks for confirmation. Undo is available; Redo is not part of this MVP.

## Parsing templates

Template files are local JSON, separate from binary files. Loading a template does **not** open a binary or parse it automatically.

1. Open a binary with `File → Open File…` or drag and drop it into HexForge.
2. Use `Template → Load Template…`, or open `Template → Template Editor…` to create an Untitled template. The editor lives in its own window.
3. Add or edit fields in the editor, then choose **Apply Template**. An unsaved draft can be applied directly; saving the JSON first is optional.
4. Read the parsed results below the hex view. Click a result to navigate to and highlight its source bytes.

The editor can save an existing path-backed template with **Save Template** or choose a path with **Save Template As…**. Neither action automatically applies the template. **Unload Template** clears the active template. Unsaved editor changes are checked when you close the editor or main window.

Supported field types: `u8`, `u16`, `u32`, `i8`, `i16`, `i32`, `f32`, `f64`, fixed-length `string`, and fixed-length `bytes`. Numeric fields support little- or big-endian parsing. Offsets accept decimal or `0x`-prefixed hex input; `string` and `bytes` fields require a positive length. Templates are intentionally flat—no nested structures or conditional logic.

Start with a sample:

- [PNG header / IHDR template](templates/png-header.json) — a recognizable real-world format with big-endian fields.
- [Firmware header template](templates/firmware-header.json) — a compact example of little-endian fields.

For predictable memory use, template parsing is limited to 4,096 fields and a 16 MiB aggregate decoded-data budget.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Open / close binary file | `Ctrl+O` / `Ctrl+W` |
| Save binary as a new file | `Ctrl+Alt+S` |
| Export parsed results as CSV | `Ctrl+Shift+E` |
| Toggle edit mode / edit selected byte / undo | `Ctrl+Alt+E` / `F2` / `Ctrl+Z` |
| Go to offset / search bytes | `Ctrl+G` / `Ctrl+F` |
| Open Template Editor / apply template | `Ctrl+Shift+T` / `Ctrl+Enter` |
| Load / unload template | `Ctrl+Alt+O` / `Ctrl+Alt+U` |
| Save template / Save Template As | `Ctrl+S` / `Ctrl+Shift+S` |
| Toggle dark/light theme | `Ctrl+Alt+T` |
| Set 16 / 32 bytes per row | `Ctrl+1` / `Ctrl+2` |
| Close a popup, or clear selection | `Esc` |
| Exit | `Alt+F4` |

`Ctrl+S` is for the **template JSON**. It is not an in-place save command for a binary file.

## For contributors

Vue and TypeScript handle the interface, interactions, and Canvas rendering. Tauri commands call the Rust backend for paginated I/O, searching, template parsing, exports, and the in-memory edit buffer. No third-party hex-view component is used.

```sh
npm test
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Dependencies are locked in `package-lock.json`; use `npm ci` for a fresh checkout and `npm install` only when intentionally updating dependencies.
