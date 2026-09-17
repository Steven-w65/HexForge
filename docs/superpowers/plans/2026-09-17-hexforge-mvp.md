# HexForge MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete HexForge MVP: an offline Tauri 2 desktop hex viewer with bounded paged reads, a layered Canvas viewport, sparse in-memory edits and undo, flat binary templates, search, CSV export, and create-new Save As.

**Architecture:** Rust owns a single active read-only file session, fixed-budget LRU pages, sparse edits, parsing, searching, and streaming output. Vue owns only presentation and user interaction, using a typed Tauri IPC adapter; the hex view uses static, content, and overlay Canvas layers plus a virtual 64-bit scrollbar.

**Tech Stack:** Tauri 2, Rust 2021, Vue 3 Composition API, TypeScript, Vite, Vitest, Vue Test Utils, HTML Canvas, CSS variables.

**Spec:** `docs/superpowers/specs/2026-09-17-hexforge-mvp-design.md`

## Global Constraints

- Never read the entire input file into memory; viewport operations are paged and long operations use bounded chunks.
- Never open the source file for writing and never overwrite it in place.
- Save As and CSV/template saves use create-new semantics for user output paths.
- Keep the edit buffer, undo stack, binary parsing, template file I/O, search, and output streaming in Rust.
- Use a hand-written Canvas hex renderer; do not add a third-party hex component.
- Keep all operation paths offline; do not add HTTP, analytics, telemetry, updater, or remote font dependencies.
- Support only the MVP field types and flat structures listed in the spec; no redo, insertion, deletion, nesting, or conditions.
- All user/file failures return `AppError`; no `unwrap`, `expect`, or panic on runtime input paths.
- Use 64-bit Rust offsets and decimal strings at the TypeScript IPC boundary.
- Bundle JetBrains Mono locally and make dark theme the default.

## File Map

```text
HexForge/
├─ package.json                         frontend scripts and dependencies
├─ vite.config.ts                       Vue/Vitest build configuration
├─ tsconfig.json                        strict TypeScript configuration
├─ index.html                           Vite entry document
├─ src/
│  ├─ main.ts                           Vue bootstrap
│  ├─ App.vue                           application root
│  ├─ api/backend.ts                    typed Tauri command wrapper
│  ├─ types.ts                          shared frontend domain types
│  ├─ assets/fonts/JetBrainsMono.woff2  bundled local font
│  ├─ styles/theme.css                  theme variables and global reset
│  ├─ components/AppShell.vue           orchestration and application layout
│  ├─ components/TopToolbar.vue         required toolbar actions
│  ├─ components/SidebarPanel.vue       file info and template container
│  ├─ components/TemplateEditor.vue     flat field editor
│  ├─ components/HexCanvas.vue          Canvas lifecycle and input
│  ├─ components/ParsedResultsPanel.vue parsed result table
│  ├─ components/StatusBar.vue          live session/selection status
│  ├─ components/AppDialog.vue          Go To, search, edit, and errors
│  ├─ composables/useHexSession.ts       session and async operation state
│  ├─ composables/useHotkeys.ts          specified keyboard routing
│  ├─ composables/useTheme.ts            dark/light state
│  └─ hex/
│     ├─ input.ts                        offset and byte-sequence validation
│     ├─ layout.ts                       row geometry and hit-testing
│     ├─ renderer.ts                     three-layer Canvas rendering
│     ├─ selection.ts                    inclusive range normalization
│     └─ virtualScroll.ts                bigint row/track mapping
├─ src/**/*.test.ts                     colocated frontend unit tests
├─ src/components/*.test.ts             component tests
├─ src-tauri/
│  ├─ Cargo.toml                         Rust/Tauri dependencies
│  ├─ build.rs                           Tauri build hook
│  ├─ tauri.conf.json                    desktop/window/build configuration
│  ├─ capabilities/default.json          minimum Tauri 2 permissions
│  ├─ icons/                             generated application icons
│  ├─ src/main.rs                        desktop entry point
│  ├─ src/lib.rs                         Tauri builder and command registration
│  ├─ src/error.rs                       structured errors
│  ├─ src/edit_buffer.rs                 sparse edits and undo
│  ├─ src/page_cache.rs                  fixed-budget LRU byte pages
│  ├─ src/session.rs                     active read-only file session
│  ├─ src/search.rs                      bounded streaming search
│  ├─ src/template.rs                    schema, JSON I/O, and decoding
│  ├─ src/export.rs                      Save As and CSV streaming
│  └─ src/commands.rs                    asynchronous IPC adapters
├─ src-tauri/tests/session_integration.rs source-preservation/bounded-I/O tests
└─ templates/firmware-header.json        sample local template
```

---

### Task 1: Tauri 2, Vue 3, and Test Harness Scaffold

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`
- Create: `src/main.ts`, `src/App.vue`, `src/styles/theme.css`, `src/types.ts`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Modify: `.gitignore`, `README.md`
- Test: `src/App.test.ts`, Rust smoke test in `src-tauri/src/lib.rs`

**Interfaces:**
- Produces npm scripts `dev`, `build`, `test`, `typecheck`, `tauri`.
- Produces Rust crate `hexforge_lib::run()` and Tauri app identifier `dev.hexforge.app`.
- Produces frontend types `Endian`, `FieldType`, `FileInfo`, `PageResponse`, `TemplateDefinition`, `ParsedField`, and `AppError`.

- [ ] **Step 1: Add the frontend smoke test before the application entry exists**

```ts
// src/App.test.ts
import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import App from './App.vue'

describe('App', () => {
  it('renders the HexForge application landmark', () => {
    const wrapper = mount(App)
    expect(wrapper.get('[data-testid="hexforge-app"]').attributes('role')).toBe('application')
  })
})
```

- [ ] **Step 2: Create only the package and Vitest configuration, then verify RED**

```json
{
  "name": "hexforge",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "typecheck": "vue-tsc --noEmit",
    "test": "vitest run",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tauri-apps/plugin-dialog": "^2.0.0",
    "vue": "^3.5.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@vitejs/plugin-vue": "^6.0.0",
    "@vue/test-utils": "^2.4.0",
    "jsdom": "^26.0.0",
    "typescript": "^5.8.0",
    "vite": "^7.0.0",
    "vitest": "^3.2.0",
    "vue-tsc": "^3.0.0"
  }
}
```

Run: `npm install` then `npm test -- src/App.test.ts`

Expected: FAIL because `src/App.vue` does not exist.

- [ ] **Step 3: Add the minimal Vue shell and strict configuration**

```vue
<!-- src/App.vue -->
<template><main data-testid="hexforge-app" role="application"><h1>HexForge</h1></main></template>
<style src="./styles/theme.css"></style>
```

```ts
// src/main.ts
import { createApp } from 'vue'
import App from './App.vue'
createApp(App).mount('#app')
```

Configure `vite.config.ts` with `vue()` and `test: { environment: 'jsdom' }`. Configure `tsconfig.json` with `strict`, `noUncheckedIndexedAccess`, `target: ES2022`, and Vue types. Add `index.html` with only `<div id="app"></div>` and the module entry. Define the domain types with decimal-string offsets:

```ts
export type OffsetString = string
export type Endian = 'little' | 'big'
export type FieldType = 'u8'|'u16'|'u32'|'i8'|'i16'|'i32'|'f32'|'f64'|'string'|'bytes'
export interface FileInfo { name: string; path: string; size: OffsetString; revision: string; dirty: boolean }
export interface PageResponse { offset: OffsetString; bytes: number[]; modifiedOffsets: OffsetString[]; revision: string }
export interface TemplateField { name: string; offset: OffsetString; type: FieldType; length?: number; endianness: Endian; comment: string }
export interface TemplateDefinition { version: 1; name: string; defaultEndianness: Endian; fields: TemplateField[] }
export interface ParsedField extends TemplateField { length: number; value: string }
export interface AppError { code: string; message: string; detail?: string }
```

- [ ] **Step 4: Verify the Vue smoke test passes**

Run: `npm test -- src/App.test.ts`

Expected: PASS.

- [ ] **Step 5: Add a failing Rust bootstrap smoke test**

```rust
// src-tauri/src/lib.rs
#[cfg(test)]
mod tests {
    #[test]
    fn application_name_is_stable() {
        assert_eq!(super::APP_NAME, "HexForge");
    }
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml application_name_is_stable`

Expected: FAIL because `APP_NAME` is not defined.

- [ ] **Step 6: Add the minimal Tauri 2 bootstrap**

Use Rust 2021 with `tauri = { version = "2", features = [] }`, `tauri-plugin-dialog = "2"`, `serde` with derive, `serde_json`, `csv`, `thiserror`, and dev dependency `tempfile`. Define `pub const APP_NAME: &str = "HexForge"`; register the dialog plugin in `run()`. `main.rs` calls `hexforge_lib::run()`. Set `frontendDist` to `../dist`, `beforeDevCommand` to `npm run dev`, `beforeBuildCommand` to `npm run build`, and a 1280×800 minimum 960×640 window. Grant only `core:default`, `core:window:allow-close`, and `dialog:default` in `capabilities/default.json`.

- [ ] **Step 7: Verify both scaffolds and commit**

Run: `npm test -- src/App.test.ts && npm run typecheck && cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all PASS with no warnings introduced by project code.

```bash
git add package.json package-lock.json vite.config.ts tsconfig.json index.html src src-tauri .gitignore README.md
git commit -m "build: scaffold Tauri 2 and Vue application"
```

---

### Task 2: Read-Only Session, Bounded Page Cache, Sparse Edits, and Undo

**Files:**
- Create: `src-tauri/src/error.rs`, `src-tauri/src/edit_buffer.rs`, `src-tauri/src/page_cache.rs`, `src-tauri/src/session.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: unit tests colocated in those modules

**Interfaces:**
- Produces `AppError { code: ErrorCode, message: String, detail: Option<String> }` serializable to camelCase.
- Produces `EditBuffer::{apply, undo, overlay, is_dirty, modified_offsets, clear}`.
- Produces `PageCache::new(page_size: usize, max_pages: usize)` and `FileSession::{open, info, read_range, edit_byte, undo, clear_edits}`.
- `FileSession` uses `u64` internally and never opens the source with write permission.

- [ ] **Step 1: Write failing edit-buffer tests**

```rust
#[test]
fn applying_source_value_removes_dirty_edit() {
    let mut edits = EditBuffer::default();
    edits.apply(7, 0xaa, 0xbb);
    assert!(edits.is_dirty());
    edits.apply(7, 0xaa, 0xaa);
    assert!(!edits.is_dirty());
}

#[test]
fn undo_restores_prior_effective_value_in_lifo_order() {
    let mut edits = EditBuffer::default();
    edits.apply(2, 0x10, 0x20);
    edits.apply(2, 0x10, 0x30);
    assert_eq!(edits.undo(0x10), Some((2, 0x20)));
    assert_eq!(edits.effective_byte(2, 0x10), 0x20);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml edit_buffer`

Expected: FAIL because the module and methods do not exist.

- [ ] **Step 2: Implement the minimal sparse edit model**

```rust
#[derive(Debug, Default)]
pub struct EditBuffer {
    edits: std::collections::BTreeMap<u64, u8>,
    undo: Vec<EditAction>,
}

#[derive(Debug)]
struct EditAction { offset: u64, previous_effective: u8 }

impl EditBuffer {
    pub fn apply(&mut self, offset: u64, source: u8, value: u8) {
        let previous_effective = self.effective_byte(offset, source);
        if previous_effective == value { return; }
        self.undo.push(EditAction { offset, previous_effective });
        if value == source { self.edits.remove(&offset); } else { self.edits.insert(offset, value); }
    }
    pub fn effective_byte(&self, offset: u64, source: u8) -> u8 { self.edits.get(&offset).copied().unwrap_or(source) }
    pub fn is_dirty(&self) -> bool { !self.edits.is_empty() }
}
```

Complete `undo`, `overlay(start, &mut [u8])`, ordered `modified_offsets(start, len)`, and `clear` directly from the tested semantics.

- [ ] **Step 3: Verify edit tests pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml edit_buffer`

Expected: PASS.

- [ ] **Step 4: Write failing page-cache and session tests**

```rust
#[test]
fn cache_evicts_least_recent_page() {
    let mut cache = PageCache::new(4, 2);
    cache.insert(0, vec![0; 4]);
    cache.insert(4, vec![1; 4]);
    assert!(cache.get(0).is_some());
    cache.insert(8, vec![2; 4]);
    assert!(cache.get(4).is_none());
}

#[test]
fn session_reads_only_requested_range_and_overlays_edits() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), (0u8..32).collect::<Vec<_>>()).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 8, 2).unwrap();
    session.edit_byte(9, 0xfe).unwrap();
    let page = session.read_range(8, 4).unwrap();
    assert_eq!(page.bytes, vec![8, 0xfe, 10, 11]);
    assert_eq!(page.modified_offsets, vec![9]);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml page_cache && cargo test --manifest-path src-tauri/Cargo.toml session`

Expected: FAIL because cache/session types do not exist.

- [ ] **Step 5: Implement LRU pages, structured errors, and the read-only session**

Use a `HashMap<u64, CacheEntry { bytes, last_used }>` with a monotonic access counter; page keys are aligned offsets. `read_range` rejects requests above a 1 MiB command maximum, uses checked arithmetic, clamps only at EOF, fills missing cache pages with `File::seek` plus `read`, copies the requested slice, then overlays edits. `edit_byte` first reads one source byte and rejects `offset >= size`.

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageData { pub offset: u64, pub bytes: Vec<u8>, pub modified_offsets: Vec<u64>, pub revision: u64 }

pub struct FileSession {
    path: PathBuf,
    file: File,
    size: u64,
    cache: PageCache,
    edits: EditBuffer,
    revision: u64,
}
```

Map `NotFound`, `PermissionDenied`, `AlreadyExists`, `WriteZero`, and platform disk-full raw codes to stable error codes; all other I/O becomes `io_error` with a friendly message.

- [ ] **Step 6: Verify the backend core and commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check && cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

```bash
git add src-tauri/src
git commit -m "feat: add bounded read-only file sessions"
```

---

### Task 3: Bounded Effective-Byte Search

**Files:**
- Create: `src-tauri/src/search.rs`
- Modify: `src-tauri/src/session.rs`, `src-tauri/src/lib.rs`
- Test: `src-tauri/src/search.rs`

**Interfaces:**
- Consumes `FileSession::read_effective_chunk(offset, &mut [u8]) -> Result<usize, AppError>`.
- Produces `parse_hex_pattern(&str) -> Result<Vec<u8>, AppError>`.
- Produces `search_session(&mut FileSession, pattern: &[u8], chunk_size: usize, max_results: usize) -> Result<SearchResult, AppError>` where offsets are ordered and `truncated` marks the cap.

- [ ] **Step 1: Write failing parser and boundary tests**

```rust
#[test]
fn rejects_empty_and_partial_hex_tokens() {
    assert_eq!(parse_hex_pattern(" ").unwrap_err().code(), "invalid_search");
    assert_eq!(parse_hex_pattern("4 42").unwrap_err().code(), "invalid_search");
}

#[test]
fn finds_match_crossing_chunk_boundary() {
    let mut source = Cursor::new(b"xxxxABCxxxx".to_vec());
    let offsets = search_reader(&mut source, 11, b"ABC", 5, 100, &EditBuffer::default()).unwrap();
    assert_eq!(offsets.matches, vec![4]);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml search`

Expected: FAIL because the search API does not exist.

- [ ] **Step 2: Implement strict parsing and overlapped chunk search**

Split on ASCII whitespace, require exactly two ASCII hex digits per token, and cap patterns at 4096 bytes. For each chunk, prepend only the final `pattern.len() - 1` effective bytes from the prior chunk; suppress duplicate offsets below the current logical start. Stop at `max_results` and set `truncated = true`. Keep memory at `chunk_size + pattern.len()`.

- [ ] **Step 3: Add and pass the edited-byte search test**

```rust
#[test]
fn search_uses_sparse_edits() {
    let mut edits = EditBuffer::default();
    edits.apply(1, b'X', b'B');
    let mut source = Cursor::new(b"AXC".to_vec());
    let result = search_reader(&mut source, 3, b"ABC", 2, 10, &edits).unwrap();
    assert_eq!(result.matches, vec![0]);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml search`

Expected: PASS.

- [ ] **Step 4: Commit the bounded search slice**

```bash
git add src-tauri/src/search.rs src-tauri/src/session.rs src-tauri/src/lib.rs
git commit -m "feat: add streaming byte search"
```

---

### Task 4: Versioned Template JSON and Primitive Parsing

**Files:**
- Create: `src-tauri/src/template.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `templates/firmware-header.json`
- Test: `src-tauri/src/template.rs`

**Interfaces:**
- Produces Serde types `TemplateDefinition`, `TemplateField`, `FieldType`, `Endian`, and `ParsedField` using camelCase JSON.
- Produces `validate_template(&TemplateDefinition, file_size: u64) -> Result<Vec<ValidatedField>, AppError>`.
- Produces `parse_template(session: &mut FileSession, template: &TemplateDefinition) -> Result<Vec<ParsedField>, AppError>`.
- Produces `load_template_file(path)` and `save_template_file_create_new(path, template)`.

- [ ] **Step 1: Write failing schema and validation tests**

```rust
#[test]
fn bytes_requires_positive_length_and_numeric_rejects_length() {
    let bytes = field("blob", 0, FieldType::Bytes, None);
    assert_eq!(validate_field(&bytes, Endian::Little, 16).unwrap_err().code(), "invalid_template");
    let number = field("count", 0, FieldType::U32, Some(4));
    assert_eq!(validate_field(&number, Endian::Little, 16).unwrap_err().code(), "invalid_template");
}

#[test]
fn rejects_field_past_end_without_clamping() {
    let field = field("tail", 14, FieldType::U32, None);
    assert_eq!(validate_field(&field, Endian::Little, 16).unwrap_err().code(), "template_out_of_bounds");
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml template`

Expected: FAIL because template types and validation do not exist.

- [ ] **Step 2: Implement the schema and complete validation**

Use `#[serde(rename_all = "camelCase")]` for structs and lowercase enum names. Require version `1`, non-empty trimmed template and field names, unique field names, positive string/bytes length, and checked `offset + length <= file_size`. Resolve per-field `endianness: Option<Endian>` to the template default.

- [ ] **Step 3: Write failing decode tests for every field family**

```rust
#[test]
fn decodes_signed_unsigned_float_string_and_bytes() {
    assert_eq!(decode(FieldType::U16, &[0x34, 0x12], Endian::Little).unwrap(), "4660");
    assert_eq!(decode(FieldType::I16, &[0xff, 0xfe], Endian::Big).unwrap(), "-2");
    assert_eq!(decode(FieldType::F32, &1.5f32.to_be_bytes(), Endian::Big).unwrap(), "1.5");
    assert_eq!(decode(FieldType::String, b"ROM\0\0", Endian::Little).unwrap(), "ROM");
    assert_eq!(decode(FieldType::Bytes, &[0xde, 0xad], Endian::Little).unwrap(), "DE AD");
}
```

Extend the table test to cover `u8/u16/u32/i8/i16/i32/f32/f64` with both endian modes where size exceeds one byte.

Run: `cargo test --manifest-path src-tauri/Cargo.toml decodes_`

Expected: FAIL because `decode` does not exist.

- [ ] **Step 4: Implement exact-width decoding and effective-range parsing**

Use `from_le_bytes`/`from_be_bytes` with fixed-size arrays, `String::from_utf8_lossy(...).trim_end_matches('\0')`, and uppercase two-digit byte formatting. `parse_template` validates all fields first, reads each exact range from `FileSession::read_range`, and preserves field order.

- [ ] **Step 5: Add JSON I/O tests and the sample file**

Add these concrete JSON I/O cases, then create `templates/firmware-header.json` with fields `magic: bytes[4]`, `version: u16`, `flags: u16`, and `payloadSize: u32`, all at non-overlapping offsets:

```rust
#[test]
fn template_file_io_reports_invalid_json_and_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("invalid.json");
    std::fs::write(&invalid, "{not json}").unwrap();
    assert_eq!(load_template_file(&invalid).unwrap_err().code(), "invalid_template");

    let existing = dir.path().join("existing.json");
    std::fs::write(&existing, "keep").unwrap();
    let definition = valid_template();
    assert_eq!(save_template_file_create_new(&existing, &definition).unwrap_err().code(), "destination_exists");
    assert_eq!(std::fs::read_to_string(existing).unwrap(), "keep");
}

#[test]
fn template_rejects_unsupported_version() {
    let error = serde_json::from_str::<TemplateDefinition>(r#"{"version":2,"name":"bad","defaultEndianness":"little","fields":[]}"#)
        .unwrap_err();
    assert!(error.to_string().contains("version"));
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml template`

Expected: PASS.

- [ ] **Step 6: Commit template support**

```bash
git add src-tauri/src/template.rs src-tauri/src/lib.rs templates
git commit -m "feat: add flat binary parsing templates"
```

---

### Task 5: Streaming Save As and CSV Export

**Files:**
- Create: `src-tauri/src/export.rs`
- Modify: `src-tauri/src/session.rs`, `src-tauri/src/lib.rs`
- Test: `src-tauri/src/export.rs`, `src-tauri/tests/session_integration.rs`

**Interfaces:**
- Produces `save_session_as(session: &mut FileSession, destination: &Path, chunk_size: usize) -> Result<SaveSummary, AppError>`.
- Produces `export_csv_create_new(destination: &Path, fields: &[ParsedField]) -> Result<(), AppError>`.
- `SaveSummary` contains decimal-string-friendly `bytes_written: u64` and `destination: PathBuf`.

- [ ] **Step 1: Write failing source-preservation and path-safety tests**

```rust
#[test]
fn save_as_applies_edits_without_changing_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let output = dir.path().join("copy.bin");
    std::fs::write(&source, [1, 2, 3, 4]).unwrap();
    let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
    session.edit_byte(1, 9).unwrap();
    save_session_as(&mut session, &output, 2).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), [1, 2, 3, 4]);
    assert_eq!(std::fs::read(&output).unwrap(), [1, 9, 3, 4]);
    assert!(!session.is_dirty());
}

#[test]
fn save_as_rejects_source_and_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let existing = dir.path().join("existing.bin");
    std::fs::write(&source, [1, 2]).unwrap();
    std::fs::write(&existing, [7, 7]).unwrap();
    let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
    assert_eq!(save_session_as(&mut session, &source, 2).unwrap_err().code(), "destination_is_source");
    assert_eq!(save_session_as(&mut session, &existing, 2).unwrap_err().code(), "destination_exists");
    assert_eq!(std::fs::read(existing).unwrap(), [7, 7]);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test session_integration`

Expected: FAIL because the export module does not exist.

- [ ] **Step 2: Implement bounded Save As with cleanup**

Canonicalize the source and destination parent, reject equal normalized paths, then open with `OpenOptions::new().write(true).create_new(true)`. Loop over a reusable buffer no larger than the supplied chunk size, overlay edits for each source offset, use `write_all`, `flush`, and `sync_all`. On any post-create error, drop the handle and call `remove_file` only on that destination. Clear edits only after sync succeeds.

- [ ] **Step 3: Write failing CSV escaping test**

```rust
#[test]
fn csv_quotes_commas_quotes_and_newlines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("result.csv");
    export_csv_create_new(&path, &[parsed("name,one", "a\"b\nc")]).unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    assert!(csv.contains("\"name,one\""));
    assert!(csv.contains("\"a\"\"b\nc\""));
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml csv_quotes`

Expected: FAIL until CSV export is present.

- [ ] **Step 4: Implement CSV export and verify all export tests**

Use `csv::WriterBuilder` on a create-new file and serialize headers `name,offset,length,type,endianness,value,comment` followed by field rows. Flush and remove only the newly created incomplete CSV on error.

Exercise the bounded copy loop with a writer that fails as disk-full after two bytes:

```rust
struct FailingWriter { remaining: usize, error_code: i32 }
impl FailingWriter {
    fn new(remaining: usize, error_code: i32) -> Self { Self { remaining, error_code } }
}
impl std::io::Write for FailingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.remaining == 0 { return Err(std::io::Error::from_raw_os_error(self.error_code)); }
        let written = buffer.len().min(self.remaining);
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
#[cfg(windows)] fn disk_full_test_code() -> i32 { 112 }
#[cfg(unix)] fn disk_full_test_code() -> i32 { 28 }

#[test]
fn copy_failure_is_reported_as_disk_full_without_clearing_edits() {
    let mut writer = FailingWriter::new(2, disk_full_test_code());
    let mut source = Cursor::new(vec![1, 2, 3, 4]);
    let mut edits = EditBuffer::default();
    edits.apply(1, 2, 9);
    let error = copy_effective(&mut source, &mut writer, 4, 2, &edits, |_| {}).unwrap_err();
    assert_eq!(error.code(), "disk_full");
    assert!(edits.is_dirty());
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml export && cargo test --manifest-path src-tauri/Cargo.toml --test session_integration`

Expected: PASS.

- [ ] **Step 5: Commit output safety**

```bash
git add src-tauri/src/export.rs src-tauri/src/session.rs src-tauri/src/lib.rs src-tauri/tests
git commit -m "feat: add safe streaming exports"
```

---

### Task 6: Async Tauri Commands and Typed Frontend IPC

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/session.rs`
- Create: `src/api/backend.ts`, `src/api/backend.test.ts`
- Modify: `src/types.ts`

**Interfaces:**
- Produces `AppState { session: Mutex<Option<FileSession>> }` managed by Tauri.
- Produces all command names in the spec with camelCase arguments and camelCase responses.
- Produces `backend` methods with `bigint`/decimal-string conversion and normalized `AppError` rejection.
- Produces `OperationProgress { operationId, phase, processed, total }` updates through Tauri IPC channels for search, template parsing, Save As, and CSV export.

- [ ] **Step 1: Write a failing frontend IPC contract test**

```ts
import { beforeEach, expect, it, vi } from 'vitest'
const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

it('serializes bigint offsets as decimal strings', async () => {
  invoke.mockResolvedValue({ offset: '9007199254740993', bytes: [1], modifiedOffsets: [], revision: '1' })
  const { backend } = await import('./backend')
  await backend.readPage(9007199254740993n, 1)
  expect(invoke).toHaveBeenCalledWith('read_page', { offset: '9007199254740993', length: 1 })
})
```

Run: `npm test -- src/api/backend.test.ts`

Expected: FAIL because the adapter does not exist.

- [ ] **Step 2: Implement the typed IPC adapter**

Expose methods matching the command list exactly. Convert outgoing `bigint` offsets to base-10 strings and incoming offset/size strings to `bigint` only in view-model code. Catch unknown rejections and normalize them to `{ code: 'unexpected', message: 'The operation could not be completed.' }`. For long operations, construct a `Channel<OperationProgress>` from `@tauri-apps/api/core`, assign its `onmessage` callback, and pass it as `onProgress`:

```ts
async function withProgress<T>(command: string, args: object, onProgress: (value: OperationProgress) => void): Promise<T> {
  const channel = new Channel<OperationProgress>()
  channel.onmessage = onProgress
  return invoke<T>(command, { ...args, onProgress: channel })
}
```

- [ ] **Step 3: Add backend command tests before registering commands**

Test pure helpers `parse_offset_arg`, `with_session`, and error serialization without a Tauri runtime:

```rust
#[test]
fn offset_argument_accepts_u64_decimal_and_rejects_negative_text() {
    assert_eq!(parse_offset_arg("18446744073709551615").unwrap(), u64::MAX);
    assert_eq!(parse_offset_arg("-1").unwrap_err().code(), "invalid_offset");
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands`

Expected: FAIL because `commands.rs` does not exist.

- [ ] **Step 4: Implement and register all async command adapters**

Use `tauri::State<'_, AppState>` and `Result<T, AppError>`. Every command that performs filesystem I/O executes that work through `tauri::async_runtime::spawn_blocking`; return join failures as `operation_failed`. Search, parsing, Save As, and CSV functions accept `tauri::ipc::Channel<OperationProgress>` and send bounded progress records after each chunk or field, never file bytes. Register one `generate_handler!` containing `open_file`, `close_file`, `get_file_info`, `read_page`, `edit_byte`, `undo_edit`, `get_dirty_state`, `save_as`, `search_bytes`, `apply_template`, `load_template`, `save_template`, and `export_results_csv`.

For the MVP, search returns at most 100,000 offsets plus `truncated`; reads remain capped at 1 MiB. Opening a file creates a session with 64 KiB pages and a 256-page cache (16 MiB maximum source-page cache).

- [ ] **Step 5: Verify IPC types and commands, then commit**

Run: `npm test -- src/api/backend.test.ts && npm run typecheck && cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

```bash
git add src/api src/types.ts src-tauri/src
git commit -m "feat: expose typed HexForge commands"
```

---

### Task 7: Hex Input, Geometry, Selection, and Virtual Scrolling

**Files:**
- Create: `src/hex/input.ts`, `src/hex/input.test.ts`
- Create: `src/hex/layout.ts`, `src/hex/layout.test.ts`
- Create: `src/hex/selection.ts`, `src/hex/selection.test.ts`
- Create: `src/hex/virtualScroll.ts`, `src/hex/virtualScroll.test.ts`

**Interfaces:**
- Produces `parseOffset(text, fileSize) -> bigint` and `parseHexBytes(text) -> number[]`.
- Produces `createLayout(width, bytesPerRow)`, `visibleRange`, and `hitTestByte`.
- Produces inclusive `normalizeSelection(anchor, focus)`.
- Produces `rowToThumb` and `thumbToRow` using bigint-safe integer ratios.

- [ ] **Step 1: Write failing validation tests**

```ts
it.each([['42', 42n], ['0x2A', 42n], ['0X2a', 42n]])('parses offsets', (text, expected) => {
  expect(parseOffset(text, 100n)).toBe(expected)
})
it.each(['', '-1', '0x', '12x', '100'])('rejects invalid or out-of-range offsets', text => {
  expect(() => parseOffset(text, 100n)).toThrow()
})
it('accepts only complete byte tokens', () => {
  expect(parseHexBytes('41 42 ff')).toEqual([0x41, 0x42, 0xff])
  expect(() => parseHexBytes('4')).toThrow()
})
```

Run: `npm test -- src/hex/input.test.ts`

Expected: FAIL because parsing functions do not exist.

- [ ] **Step 2: Implement exact decimal/hex and byte-sequence parsing**

Use anchored regexes (`/^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/` and `/^[0-9a-fA-F]{2}$/`) before `BigInt` or `Number.parseInt`. Require `0 <= offset < fileSize` and at least one byte token.

- [ ] **Step 3: Write failing geometry, selection, and virtual-scroll tests**

```ts
it('maps hex and ASCII cells to the same byte', () => {
  const layout = createLayout(900, 16)
  expect(hitTestByte(layout, layout.hexX + layout.byteStride * 3 + 2, layout.headerHeight + 4, 32n)).toBe(35n)
  expect(hitTestByte(layout, layout.asciiX + layout.charWidth * 3 + 2, layout.headerHeight + 4, 32n)).toBe(35n)
})
it('normalizes reverse drag selection inclusively', () => {
  expect(normalizeSelection(12n, 8n)).toEqual({ start: 8n, end: 12n, count: 5n })
})
it('maps the final bigint row exactly to the final track position', () => {
  expect(rowToThumb(9_000_000_000n, 9_000_000_001n, 1000)).toBe(1000)
  expect(thumbToRow(1000, 9_000_000_001n, 1000)).toBe(9_000_000_000n)
})
```

Run: `npm test -- src/hex/layout.test.ts src/hex/selection.test.ts src/hex/virtualScroll.test.ts`

Expected: FAIL because the geometry modules do not exist.

- [ ] **Step 4: Implement deterministic geometry and bigint-safe mapping**

Use a 22 px row height, 32 px header, measured monospace character width with a deterministic test fallback, grouped byte spacing every eight bytes, and explicit column origins. `visibleRange` returns first row, row count plus two prefetch rows, byte start, and byte length capped at 1 MiB. Convert ratios only after bigint multiplication/division so large row counts never pass through imprecise JavaScript numbers.

- [ ] **Step 5: Run the frontend logic suite and commit**

Run: `npm test -- src/hex && npm run typecheck`

Expected: PASS.

```bash
git add src/hex
git commit -m "feat: add hex viewport geometry"
```

---

### Task 8: Layered Canvas Renderer and Interactive Hex View

**Files:**
- Create: `src/hex/renderer.ts`, `src/hex/renderer.test.ts`
- Create: `src/components/HexCanvas.vue`, `src/components/HexCanvas.test.ts`
- Modify: `src/types.ts`

**Interfaces:**
- Produces `HexRenderer` with `resize`, `setTheme`, `drawStatic`, `drawContent`, `drawOverlay`, and `composite`.
- `HexCanvas` props: `fileSize`, `page`, `bytesPerRow`, `selection`, `matches`, `templateRange`, `editMode`.
- `HexCanvas` emits `request-page`, `select`, `edit-request`, and `viewport-offset`.

- [ ] **Step 1: Write failing renderer tests with recording contexts**

```ts
it('renders non-printable ASCII as dots and modified bytes in orange', () => {
  const ctx = recordingContext()
  const renderer = new HexRenderer(ctx.layers, testMetrics)
  renderer.drawContent(page([0x41, 0x00], ['1']), 16, 0n)
  expect(ctx.text).toContain('A.')
  renderer.drawOverlay({ modifiedOffsets: new Set(['1']), selection: null, matches: [], templateRange: null })
  expect(ctx.fills).toContain('#f0883e')
})
```

The recording context is a test utility that records `fillText`, `fillRect`, `strokeRect`, colors, and clears; it does not mock renderer decisions.

Run: `npm test -- src/hex/renderer.test.ts`

Expected: FAIL because `HexRenderer` does not exist.

- [ ] **Step 2: Implement three offscreen layers and invalidation boundaries**

Create offscreen canvases for static, content, and overlay. Draw addresses with enough uppercase hex digits for file size, two-digit uppercase bytes, and printable ASCII or `.`. Draw selection blue, search matches with a muted amber fill, template ranges with a cyan outline, and modified-byte underlines/markers in `#f0883e`. `composite` clears the visible canvas once and draws layers in order. Scale backing stores by device pixel ratio while layout remains in CSS pixels.

- [ ] **Step 3: Write failing component interaction tests**

```ts
it('emits an inclusive selection while dragging', async () => {
  const wrapper = mount(HexCanvas, { props: readyProps })
  await fireCanvasPointer(wrapper, 'pointerdown', cellPoint(2))
  await fireCanvasPointer(wrapper, 'pointermove', cellPoint(5))
  await fireCanvasPointer(wrapper, 'pointerup', cellPoint(5))
  expect(wrapper.emitted('select')?.at(-1)).toEqual([{ start: 2n, end: 5n, count: 4n }])
})
```

Add the remaining interaction assertions explicitly:

```ts
it('requests a bounded page after resize', async () => {
  const wrapper = mount(HexCanvas, { props: readyProps })
  triggerResize(wrapper, { width: 900, height: 500 })
  await nextFrame()
  const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number }
  expect(request.length).toBeGreaterThan(0)
  expect(request.length).toBeLessThanOrEqual(1024 * 1024)
})

it('recalculates hit geometry when row width changes', async () => {
  const wrapper = mount(HexCanvas, { props: readyProps })
  await wrapper.setProps({ bytesPerRow: 32 })
  expect(wrapper.emitted('request-page')?.length).toBeGreaterThan(0)
})

it('emits edit request only in edit mode', async () => {
  const wrapper = mount(HexCanvas, { props: { ...readyProps, editMode: true } })
  await fireCanvasPointer(wrapper, 'dblclick', cellPoint(4))
  expect(wrapper.emitted('edit-request')?.at(-1)).toEqual([4n])
})
```

Run: `npm test -- src/components/HexCanvas.test.ts`

Expected: FAIL because the component does not exist.

- [ ] **Step 4: Implement Canvas lifecycle and interactions**

Use `ResizeObserver`, pointer capture, `requestAnimationFrame`, and a narrow scrollbar element adjacent to the Canvas. Re-render static only on size/theme/row-width changes, content only on page/viewport changes, and overlay only on selection/match/template/edit marker changes. Emit generation-tagged page requests and ignore drawing data whose revision or requested range is stale.

- [ ] **Step 5: Verify rendering logic and commit**

Run: `npm test -- src/hex src/components/HexCanvas.test.ts && npm run typecheck`

Expected: PASS.

```bash
git add src/hex/renderer.ts src/hex/renderer.test.ts src/components/HexCanvas.vue src/components/HexCanvas.test.ts src/types.ts
git commit -m "feat: add layered Canvas hex view"
```

---

### Task 9: Application Shell, Template UI, Results, Status, and Themes

**Files:**
- Create: `src/components/TopToolbar.vue`, `src/components/TopToolbar.test.ts`
- Create: `src/components/SidebarPanel.vue`, `src/components/TemplateEditor.vue`, `src/components/TemplateEditor.test.ts`
- Create: `src/components/ParsedResultsPanel.vue`, `src/components/ParsedResultsPanel.test.ts`
- Create: `src/components/StatusBar.vue`, `src/components/StatusBar.test.ts`
- Create: `src/components/AppDialog.vue`
- Create: `src/components/AppShell.vue`, `src/components/AppShell.test.ts`
- Create: `src/composables/useTheme.ts`, `src/composables/useTheme.test.ts`
- Modify: `src/App.vue`, `src/styles/theme.css`
- Add: `src/assets/fonts/JetBrainsMono.woff2`

**Interfaces:**
- Toolbar emits the eight specified actions in the specified order.
- Template editor emits immutable `update:modelValue`, `save`, `load`, and `navigate` events.
- Result rows emit `navigate({ start, end })`.
- Status bar consumes file, selection, row width, mode, endian, and dirty props.

- [ ] **Step 1: Write failing toolbar and theme tests**

```ts
it('keeps the required action order', () => {
  const wrapper = mount(TopToolbar, { props: { hasFile: true, dirty: true, editMode: false, theme: 'dark' } })
  expect(wrapper.findAll('[data-action]').map(x => x.attributes('data-action'))).toEqual([
    'open','goto','search','template','export','theme','edit','save-as'
  ])
})

it('defaults to dark and toggles the root data attribute', () => {
  const theme = useTheme(document.documentElement)
  expect(theme.value.value).toBe('dark')
  theme.toggle()
  expect(document.documentElement.dataset.theme).toBe('light')
})
```

Run: `npm test -- src/components/TopToolbar.test.ts src/composables/useTheme.test.ts`

Expected: FAIL because components/composable do not exist.

- [ ] **Step 2: Implement toolbar and offline themes**

Use small inline SVG line icons with `currentColor`; do not add an icon package. Define dark variables exactly for `--bg: #111418`, `--text: #c9d1d9`, `--address: #8b949e`, `--selection: #1f6feb`, and `--modified: #f0883e`. Define the light equivalents in `[data-theme='light']`. Add local `@font-face` for the bundled WOFF2 and no external URL.

- [ ] **Step 3: Write failing editor, results, and status tests**

Use explicit component assertions for editor defaults, navigation, and live status:

```ts
it('adds a valid numeric field and reveals length only for variable-width types', async () => {
  const wrapper = mount(TemplateEditor, { props: { modelValue: emptyTemplate() } })
  await wrapper.get('[data-action="add-field"]').trigger('click')
  const template = wrapper.emitted('update:modelValue')?.at(-1)?.[0] as TemplateDefinition
  expect(template.fields[0]).toEqual({ name: 'field1', offset: '0', type: 'u8', endianness: 'little', comment: '' })
  await wrapper.setProps({ modelValue: { ...template, fields: [{ ...template.fields[0]!, type: 'bytes', length: 4 }] } })
  expect(wrapper.find('input[data-field="length"]').exists()).toBe(true)
})

it('emits the exact parsed-field byte range', async () => {
  const wrapper = mount(ParsedResultsPanel, { props: { results: [parsedAt('16', 4)] } })
  await wrapper.get('tbody tr').trigger('click')
  expect(wrapper.emitted('navigate')).toEqual([[{ start: 16n, end: 19n }]])
})

it('renders printable and non-printable selected bytes', async () => {
  const wrapper = mount(StatusBar, { props: statusProps({ offset: 1n, value: 0xff }) })
  expect(wrapper.text()).toContain('FF')
  expect(wrapper.text()).toContain('.')
  await wrapper.setProps(statusProps({ offset: 2n, value: 0x41 }))
  expect(wrapper.text()).toContain('A')
})
```

Run: `npm test -- src/components/TemplateEditor.test.ts src/components/ParsedResultsPanel.test.ts src/components/StatusBar.test.ts`

Expected: FAIL because these components do not exist.

- [ ] **Step 4: Implement the panels and status bar**

Keep field rows flat and keyed by stable local IDs not serialized into JSON. Show the file name, path, exact formatted size, and modification badge. The right table columns are Name, Offset, Type, Value, and Comment. Both side panels expose one compact collapse button. Status items are file size, selected offset, byte value/ASCII, selected count, 16/32 bytes per row, read-only/edit, LE/BE, and clean/modified.

- [ ] **Step 5: Implement and test the responsive application shell**

Mount `TopToolbar`, `SidebarPanel`, `HexCanvas`, `ParsedResultsPanel`, `StatusBar`, and one `AppDialog` outlet in a CSS grid with top/content/status rows and left/center/right columns. Test panel collapse, 16/32 toggle, action forwarding, and that no-file state shows a centered drop prompt while keeping Open enabled.

Run: `npm test -- src/components src/composables/useTheme.test.ts && npm run typecheck`

Expected: PASS.

- [ ] **Step 6: Commit the complete visual shell**

```bash
git add src/App.vue src/assets src/components src/composables/useTheme.ts src/composables/useTheme.test.ts src/styles/theme.css
git commit -m "feat: build the HexForge desktop workspace"
```

---

### Task 10: Session Orchestration, Dialogs, Drag-and-Drop, Hotkeys, and Dirty Close

**Files:**
- Create: `src/composables/useHexSession.ts`, `src/composables/useHexSession.test.ts`
- Create: `src/composables/useHotkeys.ts`, `src/composables/useHotkeys.test.ts`
- Modify: `src/components/AppShell.vue`, `src/components/AppShell.test.ts`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`

**Interfaces:**
- `useHexSession` exposes reactive `file`, `page`, `selection`, `template`, `results`, `matches`, `busy`, `progress`, `error`, and all toolbar operations.
- `useHotkeys` receives action callbacks and returns a cleanup function.
- Window drag-drop and close listeners are registered once and unregistered on component unmount.

- [ ] **Step 1: Write failing session stale-response and edit tests**

```ts
it('ignores a page response older than the newest generation', async () => {
  const first = deferred<PageResponse>()
  const second = deferred<PageResponse>()
  backend.readPage.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
  const session = useHexSession(backend)
  const a = session.requestPage(0n, 256)
  const b = session.requestPage(4096n, 256)
  second.resolve(pageAt(4096n)); await b
  first.resolve(pageAt(0n)); await a
  expect(session.page.value?.offset).toBe('4096')
})

it('validates edit text before invoking Rust and refreshes dirty state', async () => {
  const session = useHexSession(backend)
  await expect(session.editSelectedByte('GG')).rejects.toThrow()
  expect(backend.editByte).not.toHaveBeenCalled()
  await session.editSelectedByte('ff')
  expect(backend.editByte).toHaveBeenCalledWith(session.selection.value!.start, 0xff)
})
```

Run: `npm test -- src/composables/useHexSession.test.ts`

Expected: FAIL because the composable does not exist.

- [ ] **Step 2: Implement orchestration with friendly failure state**

Every operation sets a named busy flag in `try/finally`, stores normalized errors for `AppDialog`, and updates only relevant state. Opening resets selection/results/matches and reads the first page. Go To selects the offset and updates viewport. Search parses locally for fast feedback, invokes Rust, and selects the first match if present. Template apply stores results. Save As clears dirty state only from the backend response. Undo refreshes the current page and dirty state.

- [ ] **Step 3: Write failing hotkey tests**

```ts
it('dispatches specified shortcuts and lets Escape close a popup before clearing selection', () => {
  const actions = actionSpies({ popupOpen: true })
  const dispose = useHotkeys(actions)
  keydown('Control', 'o'); keydown('Control', 'f'); keydown('Escape')
  expect(actions.open).toHaveBeenCalledOnce()
  expect(actions.search).toHaveBeenCalledOnce()
  expect(actions.closePopup).toHaveBeenCalledOnce()
  expect(actions.clearSelection).not.toHaveBeenCalled()
  dispose()
})
```

Cover Ctrl+G, Ctrl+S, Ctrl+Z and input-field behavior.

Run: `npm test -- src/composables/useHotkeys.test.ts`

Expected: FAIL because hotkey routing does not exist.

- [ ] **Step 4: Implement hotkeys, native dialogs, and drag-and-drop**

Use `@tauri-apps/plugin-dialog` `open`, `save`, `message`, and `confirm`. File open allows one file and accepts any extension. Save As and CSV/template destinations return paths only; Rust performs all writes. Listen to Tauri `onDragDropEvent`; accept exactly one dropped path and route it through the same dirty guard and `openFile` action. Dialog cancellations are silent and do not become errors.

- [ ] **Step 5: Add dirty-close interception test before wiring the listener**

Test the pure decision function:

```ts
it('prevents close until dirty discard is confirmed', async () => {
  const event = { preventDefault: vi.fn() }
  await handleCloseRequest(event, true, vi.fn().mockResolvedValue(false), vi.fn())
  expect(event.preventDefault).toHaveBeenCalledOnce()
})
```

Implement `getCurrentWindow().onCloseRequested`: call `preventDefault()` when dirty, await native confirmation, set a one-shot `allowClose` guard, then call `window.close()` only after confirmation. Escape closes the topmost in-app dialog before clearing selection.

- [ ] **Step 6: Verify the full frontend interaction suite and commit**

Run: `npm test && npm run typecheck && npm run build`

Expected: PASS with no unhandled promise rejections.

```bash
git add src src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "feat: wire HexForge desktop interactions"
```

---

### Task 11: End-to-End Safety Verification, Packaging Checks, and Documentation

**Files:**
- Modify: `src-tauri/tests/session_integration.rs`
- Modify: `README.md`
- Modify: `src-tauri/tauri.conf.json` and `src-tauri/icons/*` only if packaging checks identify missing required metadata/assets

**Interfaces:**
- Produces a documented development/build workflow and verified MVP acceptance evidence.
- Does not add product features beyond the approved spec.

- [ ] **Step 1: Add an integration test proving bounded sparse-file viewport reads**

```rust
#[test]
fn multi_gigabyte_sparse_file_opens_and_reads_a_small_page() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(5 * 1024 * 1024 * 1024).unwrap();
    let mut session = FileSession::open(file.path().to_path_buf(), 64 * 1024, 4).unwrap();
    let page = session.read_range(4_000_000_000, 4096).unwrap();
    assert_eq!(page.bytes.len(), 4096);
    assert!(session.cached_bytes() <= 4 * 64 * 1024);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test session_integration multi_gigabyte`

Expected: PASS without allocating relative to file size.

- [ ] **Step 2: Add explicit regression tests for source immutability and create-new output**

Add a regression that directly compares all relevant paths:

```rust
#[test]
fn source_and_existing_destinations_remain_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let output = dir.path().join("output.bin");
    let existing = dir.path().join("existing.bin");
    let original = vec![0x10, 0x20, 0x30];
    std::fs::write(&source, &original).unwrap();
    std::fs::write(&existing, [0xaa]).unwrap();
    let mut session = FileSession::open(source.clone(), 2, 2).unwrap();
    session.edit_byte(1, 0xff).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), original);
    session.undo().unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), original);
    session.edit_byte(1, 0xff).unwrap();
    save_session_as(&mut session, &output, 2).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(&output).unwrap(), [0x10, 0xff, 0x30]);
    assert_eq!(save_session_as(&mut session, &existing, 2).unwrap_err().code(), "destination_exists");
    assert_eq!(std::fs::read(existing).unwrap(), [0xaa]);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test session_integration`

Expected: PASS.

- [ ] **Step 3: Document exact local workflows and safety guarantees**

Update README with prerequisites (Node, Rust, Tauri OS dependencies), `npm install`, `npm run tauri dev`, `npm test`, `cargo test --manifest-path src-tauri/Cargo.toml`, and `npm run tauri build`. Document supported template fields, sample location, 16/32-byte rows, hotkeys, read-only source behavior, sparse edits, Undo-only behavior, and create-new Save As.

- [ ] **Step 4: Run the complete quality gate**

Run:

```bash
npm test
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: every command exits zero; tests show all specified backend, frontend, and integration cases passing.

- [ ] **Step 5: Inspect dependency and capability boundaries**

Run `cargo tree --manifest-path src-tauri/Cargo.toml` and `npm ls --all`. Confirm the project declares no HTTP client, updater, telemetry, analytics, remote font, or hex-view dependency. Inspect `capabilities/default.json` and confirm it contains only the window/dialog/core permissions used by the app.

- [ ] **Step 6: Commit the verified MVP**

```bash
git add README.md src-tauri/tests src-tauri/tauri.conf.json src-tauri/icons
git commit -m "test: verify HexForge MVP safety guarantees"
```

## Final Acceptance Walkthrough

After the automated quality gate, run `npm run tauri dev` and perform this single manual desktop walkthrough:

1. Open a binary through Ctrl+O and confirm only the viewport appears.
2. Drop a different binary and confirm the same open flow.
3. Toggle 16/32 bytes per row and both themes.
4. Click and drag selections; verify offset, value, ASCII, and count in the status bar.
5. Use decimal and hexadecimal Go To values.
6. Search a byte sequence spanning a Rust search chunk boundary fixture.
7. Enable edit mode, replace one byte, observe orange and modified indicators, then undo.
8. Edit again, attempt to close, decline discard, and verify the window remains open.
9. Save As to a new path; compare source and output; verify an existing destination is refused.
10. Load, edit, save, and apply `templates/firmware-header.json`; click fields in both panels and verify navigation/highlight.
11. Export parsed results to CSV and inspect quoted values.
12. Trigger missing-file, invalid-template, and permission-denied cases and verify friendly dialogs without a crash.
