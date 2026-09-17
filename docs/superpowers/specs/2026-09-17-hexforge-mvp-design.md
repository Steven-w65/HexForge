# HexForge MVP Design

## Purpose

HexForge is a modern, lightweight, fully offline desktop hex viewer and binary analysis utility for developers, firmware debuggers, reverse engineers, and game asset analysts. It uses Tauri 2, Vue 3, TypeScript, and Rust. The frontend renders the hex view with Canvas and handles presentation and interaction. Rust owns file I/O, paging, parsing, searching, exporting, and the in-memory editing buffer.

The MVP is deliberately limited to the feature set in this specification. It excludes nested templates, conditional parsing, redo, in-place saves, insert/delete editing, networking, analytics, and automatic updates.

## Non-Negotiable Constraints

- HexForge never loads an entire input file into memory. Reads are bounded, aligned, on-demand page requests around the visible viewport or bounded streaming chunks for long operations.
- The hex view is self-drawn with Canvas. No third-party hex viewer component is used.
- The original file is opened read-only and is never overwritten in place.
- Edits are byte replacements held only in Rust memory. The MVP cannot change file length.
- Modified content can be written only with Save As to a brand-new path.
- No file path, file byte, template, or derived value is sent over a network.
- Long operations do not execute on the UI thread.
- All expected failures become structured, friendly errors; application code must not panic on user input or file content.

## Technology and Platform

- Tauri 2 desktop application, configured with the minimum capabilities required for dialogs and window control.
- Vue 3 Composition API with TypeScript and Vite.
- Rust backend with Serde-based IPC and JSON template serialization.
- JetBrains Mono bundled as a local application asset.
- Cross-platform desktop structure for Windows, macOS, and Linux, with Windows as the primary development host.

## Architecture

### Rust-Owned File Session

The backend manages one active file session behind application state. Opening a file closes the prior session only after any dirty-state decision has been resolved in the frontend. A session contains:

- a read-only file handle and canonical source path;
- immutable file metadata;
- a fixed-memory LRU page cache;
- a sparse map of modified bytes keyed by 64-bit file offset;
- a LIFO undo stack containing the offset and prior effective byte for each edit;
- a monotonically increasing revision used to invalidate stale frontend data.

Opening a file reads metadata only. It does not read the file body. File offsets and sizes use unsigned 64-bit values in Rust and JSON-safe decimal strings at the IPC boundary where a JavaScript number could lose precision.

### Paged Reading

Viewport requests specify an aligned starting offset and bounded byte count. The backend reads only the pages intersecting the request, using seek-and-read operations on the read-only handle. A fixed-size LRU cache reuses recently visited pages and evicts the least recently used pages when its configured byte budget is reached. Sparse edits are overlaid on a copied response buffer; cached source pages remain unmodified.

The frontend requests the visible range plus a small bounded prefetch margin. A request generation number ensures that a slow response for an old scroll position cannot replace a newer viewport. Page size and cache budget are implementation constants, not user-facing MVP settings.

### Canvas Rendering

The central hex view renders three columns from one row model: offset address, hexadecimal bytes, and ASCII preview. Row width is switchable between 16 and 32 bytes. Bytes outside printable ASCII range `0x20..=0x7e` render as `.`.

Rendering uses cached offscreen layers:

1. A static layer for column headers, separators, and stable chrome.
2. A content layer for addresses, hex values, and ASCII characters in the current viewport.
3. A dynamic overlay layer for selection, search matches, template-field ranges, hover state, and modified-byte markers.

Scrolling within loaded data reuses content. A page change invalidates the content and overlay layers. Selection-only changes invalidate only the overlay. Device pixel ratio is accounted for so text and rules remain sharp.

The view uses a custom virtual scrollbar. It maps a finite pixel track to 64-bit row indices rather than creating a DOM element whose height represents every file row. This avoids browser maximum-scroll-height limits for multi-gigabyte files.

### Editing Model

Edit mode is opt-in. A selected byte can be replaced only by a two-digit hexadecimal value representing `00` through `FF`. The frontend validates input for immediate feedback, and Rust validates it again before applying the edit.

An edit records the previous effective value on the undo stack and updates the sparse edit map. If the new value equals the source byte, the sparse entry is removed. Undo restores the prior effective value and updates dirty state. Redo is intentionally absent. Modified offsets are included in page responses so the Canvas can mark them orange independently of blue selection highlighting.

Dirty state is true whenever the sparse edit map is non-empty. Closing a dirty window is intercepted and requires explicit confirmation to discard changes.

### Save As

Save As accepts only a destination different from the source and refuses an existing path. Rust opens the destination with create-new semantics, streams the source through a bounded buffer, applies sparse edits to each chunk, flushes and synchronizes the output, and then reports success. It never constructs the full output in memory.

If writing fails, including due to insufficient disk space, Rust closes and removes only the incomplete destination it created. The original remains untouched and the session remains dirty. A successful Save As does not silently replace the active source session; it clears the current edit buffer only after the completed output is confirmed.

### Search

Search input is a whitespace-separated hexadecimal byte sequence. Empty input, odd hex digits, and bytes outside `00..FF` are rejected. Rust searches the effective content using bounded streaming chunks and retains `pattern_length - 1` overlap so matches crossing chunk boundaries are found. Sparse edits are applied before comparison.

Search is asynchronous and emits bounded result batches or progress updates. Match offsets are stored as 64-bit values. The frontend highlights only matches intersecting the visible page, preventing a large match set from becoming a rendering bottleneck.

### Templates

Templates are independent local JSON files with this versioned shape:

```json
{
  "version": 1,
  "name": "Firmware Header",
  "defaultEndianness": "little",
  "fields": [
    {
      "name": "magic",
      "offset": 0,
      "type": "bytes",
      "length": 4,
      "endianness": "little",
      "comment": "Header signature"
    }
  ]
}
```

Supported field types are `u8`, `u16`, `u32`, `i8`, `i16`, `i32`, `f32`, `f64`, `string`, and `bytes`. Numeric sizes are fixed. `length` is required for `string` and `bytes` and rejected for fixed-width types. Endianness is `little` or `big`; field endianness overrides the template default. Endianness has no effect on one-byte, string, or raw byte fields but is retained for schema consistency.

Template loading and saving occur in Rust. Parsing validates the version, names, offsets, lengths, field types, arithmetic overflow, and file bounds before reading. Parsing reads only ranges needed by fields and overlays edits. Strings are fixed-length UTF-8-lossy values with trailing NUL bytes removed. Raw bytes are displayed as uppercase, space-separated hex. Floating-point results preserve a concise display value while the parsed result also identifies type, offset, length, endianness, and comment.

The built-in editor supports adding, updating, and removing flat fields with name, start offset, type, endianness, optional length, and comment. Selecting either a template field or a parsed-result row scrolls to and highlights its exact byte range.

### CSV Export

CSV export is implemented in Rust and writes the current parsed-result rows with headers for name, offset, length, type, endianness, value, and comment. Values are correctly quoted and embedded quotes are escaped. The destination uses create-new semantics and never overwrites an existing file.

## Backend Modules and Commands

Rust modules have one primary responsibility:

- `session`: active session lifecycle and metadata.
- `page_cache`: aligned pages and LRU eviction.
- `edit_buffer`: sparse modifications, dirty state, and undo.
- `search`: streaming effective-byte search.
- `template`: schema validation, local load/save, and primitive decoding.
- `export`: Save As streaming and CSV writing.
- `error`: serializable error codes and friendly messages.
- `commands`: thin asynchronous Tauri command adapters.

The Tauri command surface is:

- `open_file`, `close_file`, `get_file_info`, `read_page`;
- `edit_byte`, `undo_edit`, `get_dirty_state`, `save_as`;
- `search_bytes`, `apply_template`;
- `load_template`, `save_template`;
- `export_results_csv`.

Long-running commands are asynchronous. Blocking filesystem loops run outside the UI thread. Operations carry a request or revision identifier, and progress is sent through Tauri events or channels without exposing unbounded buffers.

## Frontend Structure

- `AppShell.vue`: overall layout, active session orchestration, and dialogs.
- `TopToolbar.vue`: app title and actions in the specified order.
- `SidebarPanel.vue`: collapsible file information and template panel.
- `TemplateEditor.vue`: flat template editing.
- `HexCanvas.vue`: Canvas lifecycle, viewport requests, input, and scrolling.
- `ParsedResultsPanel.vue`: parsed field table and navigation.
- `StatusBar.vue`: file size, selection, row width, mode, endianness, and dirty state.
- `hex/renderer.ts`: layered drawing.
- `hex/layout.ts`: row geometry, visible ranges, and hit-testing.
- `hex/virtualScroll.ts`: 64-bit row-to-track mapping.
- `api/backend.ts`: typed IPC boundary.
- Composables for file state, selection, hotkeys, theme, and dialogs.

The frontend never directly reads or writes user files. Native paths selected by dialogs or received from drag-and-drop are passed to Rust commands.

## User Interface

The workspace uses a dense VS Code-inspired layout with minimal borders, small radii, lightweight cards, soft hover fills, and no heavy shadows. The top bar contains the title followed by compact linear icon actions in this order: Open File, Go To Offset, Search Bytes, Apply Template, Export, Theme Toggle, Edit Mode Toggle, and Save As.

The left sidebar contains file information and the template editor. The Canvas fills the central area. The right panel contains parsed results. Both side panels are independently collapsible. The status bar remains fixed at the bottom.

Dark theme is the default with background `#111418`, text `#c9d1d9`, addresses `#8b949e`, and selection `#1f6feb`. Modified bytes use orange. Light theme uses corresponding CSS variables and is toggled in one click. JetBrains Mono is bundled locally and used for hexadecimal data, addresses, ASCII, and numeric values.

Selection supports click for one byte and drag for an inclusive range. Blue shows selection, a restrained secondary fill shows search matches, an outline shows template ranges, and an orange marker remains visible for modified bytes even when selected. The status bar updates the selected offset, byte value, printable ASCII value or `.`, and selected byte count in real time.

Go To accepts decimal or `0x`-prefixed hexadecimal offsets and rejects negative, malformed, or out-of-range values. It positions the target row in view and selects the byte. Drag-and-drop opens one local file and uses the same guarded open flow as the toolbar.

## Hotkeys

- `Ctrl+O`: open a binary file.
- `Ctrl+F`: open byte search.
- `Ctrl+G`: open Go To Offset.
- `Ctrl+S`: save the current template.
- `Ctrl+Z`: undo the latest edit when available.
- `Esc`: close the active popup first; otherwise clear selection.

Hotkeys are handled at application level and do not fire when incompatible with current state. Ordinary text editing behavior is preserved inside input controls except for the explicit application commands above.

## Error Handling and Safety

Backend failures use a serializable structure containing a stable code, friendly message, and optional non-sensitive detail. Expected categories include missing file, permission denied, invalid path, invalid offset, invalid byte input, invalid or unsupported template, unexpected end of file, disk full, destination exists, destination equals source, no active file, and operation cancelled.

Reads use checked arithmetic. Viewport requests may be clamped at end of file; template ranges are rejected rather than silently clamped. Lock poisoning and unexpected I/O errors become command errors. Partial Save As and CSV files created by the current operation are removed on failure when possible. No broad directory deletion or unrelated file mutation occurs.

The application includes no HTTP dependency, remote font, telemetry, updater, analytics, or external-content view. Tauri capabilities are kept to the minimum needed for IPC, native dialogs, drag-and-drop events, and controlled window close.

## Testing Strategy

Rust unit tests cover:

- page alignment, end-of-file reads, overlay behavior, and cache eviction;
- sparse edit insertion/removal, dirty state, and undo order;
- search validation, cross-chunk matches, and edited-byte matches;
- every template field type in both meaningful endian modes;
- malformed JSON, unsupported versions/types, overflows, and out-of-bounds fields;
- CSV quoting and escaping;
- Save As streaming, source preservation, destination rejection, and failure cleanup.

Rust integration tests use sparse temporary files and instrumented/bounded readers where appropriate to prove that operations use bounded buffers rather than whole-file reads. Tests verify that editing and Save As never alter the source.

TypeScript unit tests cover offset parsing, byte input parsing, visible row calculation, Canvas hit-testing, selection normalization, virtual-scroll mapping, and hotkey dispatch. Vue component tests cover action enablement, dirty indicators, template/result navigation events, status values, theme switching, and close-confirmation state.

Completion verification runs the Rust test suite, frontend tests, TypeScript type checking, production frontend build, Rust formatting/linting, and Tauri `cargo check`.

## Acceptance Criteria

The MVP is complete when a user can open or drop a multi-gigabyte file without whole-file allocation; smoothly inspect paged bytes in a layered Canvas view; select, navigate, and search bytes; edit byte values in memory; undo edits; receive a discard warning on close; save effective content only to a new file; create, save, load, and apply flat JSON templates; navigate from parsed fields to bytes; export parsed results to a new CSV; switch 16/32-byte rows and light/dark themes; use all specified hotkeys; and receive friendly errors without a crash or network access.
