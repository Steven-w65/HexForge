# Task 3 Report: Bounded Effective-Byte Search

## Result

Implemented bounded, streaming search over effective file bytes. The implementation adds hex-pattern parsing, capped search results, overlap-safe chunk-boundary matching, sparse-edit overlays, a bounded session chunk reader, and `u64` match offsets.

## RED/GREEN evidence

Tests were written before the search implementation in `src-tauri/src/search.rs`, covering the required parser, boundary, sparse-edit, overlap, long-pattern, EOF, cap, validation, and session-effective-byte behaviors. The first focused test command was attempted immediately after adding those tests:

```text
cargo test --manifest-path D:\steve\Documents\GitHub\HexForge\src-tauri\Cargo.toml search::tests
cargo: The term 'cargo' is not recognized as a name of a cmdlet, function, script file, or executable program.
```

Cargo was then invoked by its installed absolute path after implementation. Dependency resolution prevented compilation/test execution:

```text
C:\Users\steve\.cargo\bin\cargo.exe test --manifest-path D:\steve\Documents\GitHub\HexForge\src-tauri\Cargo.toml search::tests
...
error: failed to download from `https://static.crates.io/crates/tao/0.35.3/download`
Caused by: [7] Could not connect to server (Failed to connect to static.crates.io:443 ...)
```

Per the task brief, one offline attempt was made without retrying:

```text
C:\Users\steve\.cargo\bin\cargo.exe test --offline --manifest-path D:\steve\Documents\GitHub\HexForge\src-tauri\Cargo.toml search::tests
error: failed to download `aho-corasick v1.1.5`
Caused by: attempting to make an HTTP request, but --offline was specified
```

Therefore no Cargo RED or GREEN test result could be observed; both focused and full Rust tests remain blocked by missing registry dependencies. The test suite is left in place for immediate execution once dependencies are available.

Formatting was verified successfully after applying `cargo fmt`:

```text
C:\Users\steve\.cargo\bin\cargo.exe fmt --manifest-path D:\steve\Documents\GitHub\HexForge\src-tauri\Cargo.toml -- --check
<no output; exit code 0>
```

## Files changed

- `src-tauri/src/search.rs`
  - Added `SearchResult` with `Vec<u64>` matches and truncation state.
  - Added strict ASCII-whitespace hex parsing with exact two-digit tokens and a 4096-byte cap.
  - Added bounded generic `search_reader` using only the current chunk plus the prior `pattern.len() - 1` effective bytes.
  - Added overlap-safe matching, chunk-boundary matching, immediate result-cap truncation, and validation errors.
  - Added `search_session` through an adapter backed by `FileSession::read_effective_chunk`.
  - Added focused unit tests for all required behaviors.
- `src-tauri/src/session.rs`
  - Added bounded `FileSession::read_effective_chunk(offset, buffer)` with sparse overlay and EOF clamping.
  - Added a direct helper test verifying bounded effective reads.
- `src-tauri/src/lib.rs`
  - Registered the `search` module.

## Self-review

- Search never allocates a file-sized buffer; memory is bounded by the requested chunk, the pattern/tail, and the result cap.
- Source reads and match offsets use `u64` offsets; conversions to `usize` are limited to bounded chunk/pattern lengths.
- The session path reads through the existing page cache and overlays sparse edits; it does not call `read_range` for the whole file.
- Matches are overlapping and are emitted in ascending offset order. Previously examined starts are suppressed when tails are prepended.
- Result truncation is set only after detecting an additional match beyond the cap.
- No Tauri commands, frontend behavior, templates, or export behavior were added.

## Concerns

The Rust test suite could not compile because the environment could not reach `static.crates.io` and the local cache lacks `aho-corasick v1.1.5` (and the online attempt also reported `tao v0.35.3`). `cargo fmt --check` passes. The generated application lockfile is included in the review-fix commit.

## Review fix: cache-independent streaming reads

### Root cause

`search_session` already streamed by chunk, but `FileSession::read_effective_chunk` delegated to `read_source_range`. That helper calls `source_page`, which inserts each touched page into `PageCache`; with a large cache this retained pages from the entire search and violated the intended O(chunk + pattern + result-cap) memory bound.

### RED test

Added `streaming_search_does_not_populate_the_page_cache` in `src-tauri/src/search.rs`. It searches a 128-byte multi-page file with a 64-page cache and asserts `session.cache_len()` remains zero. The focused test was attempted before the fix with:

```text
C:\Users\steve\.cargo\bin\cargo.exe test --offline --manifest-path D:\steve\Documents\GitHub\HexForge\src-tauri\Cargo.toml search::tests::streaming_search_does_not_populate_the_page_cache
error: failed to download `brotli v8.0.4`
Caused by: attempting to make an HTTP request, but --offline was specified
```

The registry dependency failure prevented observing the test's runtime RED result.

### Fix and verification

`read_effective_chunk` now seeks and reads directly into the caller-provided bounded buffer, overlays sparse edits in place, and never calls `read_source_range`, `source_page`, or `PageCache`. EOF clamping and existing offset/length validation are unchanged.

Commands run after the fix:

```text
C:\Users\steve\.cargo\bin\cargo.exe fmt --manifest-path D:\steve\Documents\GitHub\HexForge\src-tauri\Cargo.toml -- --check
<no output; exit code 0>
```

Focused/full Rust tests remain blocked by the same unavailable registry dependencies; no retry loop was performed. `src-tauri/Cargo.lock` is included in the fix commit for reproducible application dependency resolution.
