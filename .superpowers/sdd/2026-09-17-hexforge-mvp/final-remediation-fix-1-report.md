# Final Remediation Review Fix — Round 1

## Result

Closed all three follow-up review findings with a separate TDD cycle: a mounted configured-width layout regression, complete Save As search-state assertions, and owner-paired concurrent operation activity.

## RED evidence

- The first focused run failed because `HexSession.activity` and the shared compact shell geometry module did not exist.
- The activity tests reproduced the mismatch directly: an older search could retain global progress while a newer Save As/export owned the visible busy label.
- The mounted AppShell test was written against the real Tauri window width and required behavior that was not yet represented by a shared shell contract or conditional rendered overflow state.
- The first test implementation used a Node filesystem URL under jsdom and failed as a fixture error; it was corrected to import the real JSON config through the project's existing typed JSON module path before GREEN was accepted.

## GREEN implementation

- Added a bounded in-memory activity registry keyed by the existing monotonically increasing operation issue number. Each record owns both its operation name and latest progress value.
- The newest pending non-page operation is displayed. A waiting owner shows no unrelated progress; when it completes or fails, the next-newest pending operation and its own stored progress are restored.
- Existing per-operation busy counters, current-ticket checks, stale-result suppression, global progress compatibility, and session epoch safety remain unchanged.
- App labels and progress now derive from the same structured activity object.
- Added shared 200px/260px compact panel constants used by AppShell CSS variables and layout tests.
- HexCanvas now renders `overflow-x: hidden` when its Canvas plus scrollbar fits, and `overflow-x: auto` only when the computed full Offset/Hex/ASCII extent exceeds the viewport.
- The mounted AppShell→HexCanvas test imports `src-tauri/tauri.conf.json`, proves the configured width is 1280px and center width is 820px, drives the real ResizeObserver boundary, proves 16-byte content fits, then proves 32-byte content exceeds the viewport and the actual rendered overflow mode is `auto`. Removing that overflow behavior fails the test.
- The Save As regression now also proves `searchMatchLength` returns to 1 and `searchTruncated` returns to false.

## Checks

| Gate | Result |
| --- | --- |
| Focused activity/layout tests | PASS — 35 tests |
| Full `vitest run` | PASS — 17 files, 140 tests |
| `vue-tsc --noEmit` | PASS |
| `vite build` | PASS — 55 modules |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo test` | PASS — 58 unit, 8 integration, 0 doc failures |
| `cargo check` | PASS |
| `npm ci --ignore-scripts --no-audit --no-fund` | PASS — 141 packages |
| `npm ls --all` | PASS — expected optional platform/peer omissions only |
| `cargo tree` | PASS |

## Self-review

- Activity state is bounded by the number of currently pending frontend operations and is cleared in `finally` on success or failure.
- Page requests are intentionally excluded so viewport prefetch cannot replace a long-running search/parse/export/save status.
- Progress callbacks update only current tickets, preserving stale-operation suppression.
- Reverse completion and failure fallback are both covered.
- Layout testing exercises the mounted real components, backing Canvas width, configured desktop width, compact panel widths, and rendered overflow behavior rather than checking only a CSS custom property.
- No backend, dependency, capability, network, or product-scope change was introduced; the progress ledger was not edited.
