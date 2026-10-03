# Startup performance

Measured 2026-10-03. **The <100 ms process-to-main-interface-paint target is not met.** The optimization reduces optional JavaScript loading and font discovery latency; it does not eliminate the native WebView startup cost.

## Reference machine

- Lenovo 83DE; Intel Core i9-14900HX (24 cores, 32 logical processors), 32 GB RAM.
- Samsung MZVL21T0HCLR-00BL2 NVMe SSD.
- Windows 11 Home Chinese, build 26200; installed WebView2 154.0.4258.53.
- Release executable built with `npm run tauri -- build --no-bundle`; all frontend assets remain embedded. No dev server, sidecar, resident preloader, security-option changes, or process-priority changes.
- Normal desktop applications remained running. No reboot or system cache clearing was performed.

## What is measured

The main interface is the actual File/Edit/Navigate/Template/View menu, file strip, centered Open File control, and status/16–32 row controls, in the real application shell. The HTML body initially contains only an empty mount target. There is no splash or replacement interface. A DOM-only presence check precedes paint; geometry/style/visibility checks follow the browser paint notification to avoid injecting a forced prepaint layout.

First meaningful paint is the browser's `first-contentful-paint` timestamp after those controls have mounted and are visible. Prefer its presentation timestamp when the engine exposes one; otherwise use the Paint Timing start timestamp. This is a browser paint/presentation measurement, **not a camera measurement of physical panel photons** or independent proof against another window occluding the app. Chromium timestamp quantization and compositor reporting add uncertainty beyond IPC calibration; displayed numbers are rounded, not claims of submillisecond display accuracy.

The process origin uses Windows [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes). [GetSystemTimePreciseAsFileTime](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getsystemtimepreciseasfiletime) is sampled once against a monotonic Rust `Instant` to anchor that origin. All subsequent native intervals are monotonic. Five IPC samples taken **after measured paint** correlate `performance.now()` with native elapsed time. The minimum-round-trip sample provides the offset; its half-round-trip is reported as calibration uncertainty (median 0.30 ms in the final paired warm series). Raw native and document timestamps are never subtracted without this offset. See [W3C Paint Timing](https://w3c.github.io/paint-timing/) for the browser metric's semantics.

Interaction readiness is reported separately: the later of verified paint and completion of required native bridge, close, and drop listeners; keyboard handlers and command handlers are already installed. All measured samples had those listeners ready before paint, so interaction and paint numbers coincide. This measures readiness of the real controls, not completion of every optional module. Functional suites check the actions separately.

The title acknowledgment is only the smoke-test completion signal. Its externally polled time is not the paint metric. A blank window, Vue mount, missing paint, stale report, calibration failure, or minimized/hidden window cannot satisfy the paint probe.

## Before and after

The pre-optimization executable includes the exact same paint instrumentation as the candidate. Twenty launches per executable per category were tested in a paired comparison, alternating baseline/candidate order each iteration. One predefined warmup per executable is excluded; **no measured outliers are removed**. p95 is nearest rank (`ceil(0.95 × n)`).

| Category | Before paint median / p95 | After paint median / p95 | After interaction median / p95 | After fastest |
| --- | ---: | ---: | ---: | ---: |
| Warm OS and reused private WebView profile | 416.0 / 442.1 ms | 407.1 / 438.5 ms | 407.1 / 438.5 ms | 397.1 ms |
| Fresh private WebView profile; OS cache warm | 411.2 / 443.8 ms | 406.1 / 433.6 ms | 406.1 / 433.6 ms | 393.6 ms |

Warm median improved 8.9 ms (2.1%); profile-cold median improved 5.1 ms (1.2%). p95 improved 3.6 ms warm and 10.2 ms profile-cold in this final series. This is a modest, noisy gain, not a 100 ms launch claim or a guarantee of tail improvement under all loads. The fastest verified final-executable sample is **393.6 ms**. All final results include the close-protection failure-path hardening found during review.

Before editing product code, an initial sequential baseline produced warm 385.7 ms median / 410.6 ms p95 and profile-cold 387.5 / 393.9 ms. Its fastest sample was 374.9 ms. A later renderer-split-only candidate series measured 422.9 / 449.3 ms warm and 409.9 / 464.4 ms profile-cold. The first paired comparison (before the review safety fix) measured warm 450.9 / 478.0 → 435.3 / 468.0 ms, and profile-cold 453.3 / 464.3 → 446.8 / 467.6 ms; that series' profile-cold p95 worsened slightly. Because timings drifted across successive series, the final alternating comparison above is the primary evidence; comparing its candidate directly to the earlier 385.7 ms baseline would be misleading. The renderer split alone did **not** prove a startup gain. An earlier exploratory measurement forced layout before paint and is excluded from the reported comparison.

Raw reports remain in the ignored local build directory `src-tauri/target/startup-benchmark-2026-10-03/` (`clean-probe` for initial series, `paired` for the first comparison, `final-paired` for the final executable comparison). They contain native/browser phases, resource requests, clock samples, and full-precision summary values. These are opt-in benchmark artifacts, not distribution files.

## Critical-path evidence

Median phase durations from the paired warm series (individual medians do not necessarily sum to the median total):

| Phase | Before | After |
| --- | ---: | ---: |
| Process creation → Rust run entry | 44.5 ms | 43.8 ms |
| Generate native context | 0.5 ms | 0.5 ms |
| Build app/configuration | 3.2 ms | 3.1 ms |
| Native window and WebView creation | 241.9 ms | 240.6 ms |
| Window/WebView created → JavaScript entry | 16.3 ms | 17.2 ms |
| Main root asset load and evaluation | 6.0 ms | 4.6 ms |
| Root available → Vue root mounted | 9.3 ms | 9.7 ms |
| Root mounted → reported contentful paint | 92.4 ms | 86.1 ms |
| Concurrent required-listener registration (overlaps painting) | 5.6 ms | 6.0 ms |

The largest measured interval is **native window/WebView creation**, already about 241 ms before frontend optimization can help. This probe measures that interval as a combined phase, not separate internal WebView browser/GPU/native-window subphases. The post-mount paint gap includes browser layout/raster/compositor presentation and scheduling; it must not be attributed entirely to JavaScript.

Asset resource timings show that the font request moves from median document time **93.2 ms to 16.9 ms**, with median load duration 4.1 → 3.4 ms. The main App JavaScript chunk shrinks roughly **112 KB → 77 KB**; the 37 KB hex renderer/minimap chunk is requested after paint (or immediately on file opening). All backing canvases and renderer behavior are unchanged. The empty shell's layout is already final, so prewarm introduces no layout jump. Failed optional prewarm stays silent and retryable; an actual viewer-load failure reports an error and offers Retry.

## Reproduce

Build from the repository root:

```powershell
npm run tauri -- build --no-bundle
powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/tests/verify_portable_startup.ps1 `
  -ExecutablePath src-tauri/target/release/hexforge.exe -Measure
node scripts/startup-benchmark.mjs --runs 20 --label current `
  --output-dir src-tauri/target/startup-new-measurement
```

For a controlled before/after comparison, preserve a baseline executable **built with the same measurement instrumentation**, then use:

```powershell
node scripts/startup-benchmark.mjs --runs 20 --label comparison `
  --baseline-exe path/to/baseline.exe --exe src-tauri/target/release/hexforge.exe `
  --output-dir src-tauri/target/startup-new-comparison
```

Use a new output directory/label each time. The benchmark rejects reused fresh-profile directories rather than mislabeling them cold. The reused warm profile is private to the benchmark; normal preferences and user files are untouched. The probe restores child environment flags and closes only its own application. Normal launches have no profiling report, clock sampling, or profiling file writes. A paint observer is used for optional-work scheduling; it is not a readiness assertion or diagnostic logger.

### True OS-cold procedure (not run here)

1. Build and save the release executable before testing. Record Windows/WebView2 versions, power mode, hardware, and background application conditions.
2. Reboot manually; allow the same predefined desktop idle interval each time. Do not launch HexForge or its benchmark warmup before the measured run.
3. Run the single-launch `-Measure` probe once with a new `-WebViewProfilePath` and `-ProfileOutputPath` in an existing dedicated report directory.
4. Repeat across at least 20 reboots and compute median/p95 from the valid first-launch reports. Keep failed measurements visible; do not turn them into zero or silently discard slow runs.
5. Label these reboot-cold results separately. Warm and fresh-profile results above are not a substitute.

## Verification

Frontend: 606 tests; TypeScript typecheck; production frontend/portable build. Rust: `cargo fmt --check`, `cargo check`, 130 tests. Real Edge Canvas/editor styling regression and portable startup smoke also run. No commit is created by this work.

Final review also identified a safety issue in the earlier concurrent-listener work: a drop/bridge failure could roll back a successful close guard while leaving editing available. Main-window safety listeners now remain owned until teardown; file opening/edit commands are gated until close protection is installed. Atomic editor-bridge registration/cleanup remains unchanged. Regression tests reproduce failed and delayed registration, guarded dirty close, and late cleanup.

### Files updated for this task

- Initial loading: `index.html`, `src/main.ts`, `src/App.vue`, `src/components/AppShell.vue`, `src/startup/afterPaint.ts`, `src/startup/hexCanvasLoader.ts`.
- Profiling and native hooks: `src/startup/timings.ts`, `src/startup/listenerScope.ts`, `src/api/backend.ts`, `src-tauri/src/startup.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`.
- Probes: `src-tauri/tests/verify_portable_startup.ps1`, `scripts/startup-benchmark.mjs`.
- Tests: `src/startup/timings.test.ts`, `src/startup/afterPaint.test.ts`, `src/startup/canvasLoading.test.ts`, `src/App.test.ts`, `src/components/AppShell.test.ts`, `src/test/hexCanvasStub.ts`, `scripts/startup-assets.test.mjs`, `scripts/startup-benchmark.test.mjs`, `src-tauri/tests/startup_timings.rs`.
- Documentation: `README.md`, `STARTUP_PERFORMANCE.md`; a local ignored plan/ledger records implementation and review.

Other existing uncommitted work, including the shortcut changes and Template Editor behavior, is preserved. The release executable matches the final benchmark executable byte-for-byte (SHA-256 `D78019BCD31C94F6CD899E063D53AC1262A8207E533125BA3888FBEE14A9F298`).

Remaining limitation: achieving <100 ms would require addressing the measured native WebView/compositor startup cost. That has not been achieved or hidden by changing the endpoint, adding a splash, keeping a background helper alive, or changing the single-executable distribution.
