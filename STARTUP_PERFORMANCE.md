# Native startup performance

Updated 2026-10-16. **The under-100 ms process-to-real-interface target was not achieved.** The native release no longer uses a WebView, a development server, or JavaScript bundles. Removing those dependencies does not make native window, graphics-driver, or compositor initialization instantaneous.

The user chose to finish with the verified samples and defer the full 20-pair warm/fresh-profile benchmark. Consequently, no native before/after median, p95, profile-cold distribution, or earliest-interaction claim is made. Older browser timing results use a different observer and must not be compared directly with these results.

## Reference machine

- Lenovo 83DE; Intel Core i9-14900HX, 24 cores / 32 logical processors, approximately 32 GB RAM.
- Windows 11 Home Chinese, build 26200.
- NVIDIA RTX 4090 Laptop GPU, driver 32.0.16.1074; AskLink virtual display driver 18.36.3.757.
- Observed client: 2240 × 1400 physical pixels, 168 DPI / 175% scaling, dark application theme.
- Final release SHA-256: `CD4BCB0FE60B2E7F63FA6BD0110CD6EBA2D4E6ED571DFDB60F49222E9B19E2E2`, 10,876,928 bytes.
- Other desktop applications remained running. No reboot, cache flush, security change, priority change, resident preloader, or system animation change was used.

## What qualifies as meaningful paint

The real main interface must actually be presented with its navigation menus, Open File action, hex viewport, parsed-results panel, and status bar. A title change, blank window, placeholder, constructed scene, queued frame, or application-owned framebuffer cannot qualify.

The opt-in development probe captures the Windows desktop client pixels independently and matches five reviewed, hash-bound regions. It rejects obscured, hidden, minimized, wrong-build, wrong-DPI, and unreviewed frames. OS process creation is correlated to QPC using a bracketed precise FILETIME sample. Capture brackets use that same QPC domain; no uncorrelated browser and Rust timestamps are combined.

Each result is an interval from the last negative capture to the first qualifying capture. The upper bound is conservative. The capture interval and the separate process-origin calibration uncertainty are retained. Failed runs remain failures, not zero-millisecond results or silently discarded outliers.

## Verified native observations

The final release was independently observed on Oct16 with diagnostics off:
**511.5068–555.7582 ms**, with a 44.2514 ms capture bracket and process-origin
uncertainty of 4 ticks at 10 MHz. This is one warm launch, not a distribution.
It used an external Computer Use launch with no post-launch activation/input;
the observer matched all five real primary-control regions in actual presented
desktop pixels. Its exact-hash baseline was inspected independently before the
launch. Evidence: `oct16-final-native-warm-01.json` in the parity workspace.
The final EXE also opened its real main and separate editor controls from an
EXE-only folder outside the repository. Earliest interaction was not measured.

The following earlier observations bind the preceding release
`23AB07B1ED043D78D78639FBCCD13542C1F6D97E0841CCB7A718696059324A40`
(10,871,296 bytes). They are retained verified samples, **not final-build
statistics or a before/after speedup**.

| Observation | Launch / diagnostics | Presented interval | Capture interval |
| --- | --- | ---: | ---: |
| Oct13 warm02 | External app launch, diagnostics off | 466.5451–514.4752 ms | 47.9301 ms |
| Oct13 warm03 | External app launch, diagnostics off | 492.6285–531.8296 ms | 39.2011 ms |
| Oct13 paired warm01 native retry | External app launch, diagnostics off | 491.1940–532.8128 ms | 41.6188 ms |
| Oct13 paired warm02 native | External app launch, diagnostics off | 523.0838–578.4040 ms | 55.3202 ms |
| Oct15 diagnostic01 | External app launch, diagnostics off | 492.6627–531.5290 ms | 38.8663 ms |
| Oct15 final phased01 | Passive observer child launch, diagnostics on | 538.4201–581.6619 ms | 43.2418 ms |

These earlier samples all bind the preceding release hash above. The fastest qualifying upper bound in that set is **514.4752 ms**. Sparse samples with differing diagnostic settings are not the deferred repeated comparison. Process-origin uncertainty is 2–5 ticks at 10 MHz, much smaller than the capture brackets but not concealed.

## Preceding-release phase sample

The Oct15 phased sample of the preceding release records the following milestones relative to its own correlated OS process origin. The final Oct16 release has a new real presentation sample but was not separately phase-profiled; do not silently transfer this breakdown to its new hash.

| Milestone | Elapsed |
| --- | ---: |
| First Rust diagnostic mark | 49.4938 ms |
| System theme selected | 49.7953 ms |
| Native window / renderer available | 251.1919 / 251.1920 ms |
| Controller / preferences ready | 251.2233 / 251.2839 ms |
| Embedded font assets installed | 267.4309 ms |
| Real main scene constructed | 270.6135 ms |
| First actual Glow draw callback | 274.0142 ms |
| Independently observed presentation | 538.4201–581.6619 ms |

The theme-to-window/renderer interval is about **201.4 ms**, already longer than the target. This is a combined framework/native-window/graphics initialization region, not a measurement of each internal driver step. The later gap between draw submission and desktop presentation also includes reveal/compositor/scheduling effects; its cause has not been isolated. Scene construction and draw submission are not presentation.

A real Ctrl+Alt+T command was accepted and the light interface was observed. Its diagnostic mark was approximately 60.07 seconds after launch because the verification input was sent much later. This demonstrates eventual interaction only, **not earliest time to interaction**. No early interaction measurement is invented.

## Reproduce without changing packaging

Build the app and the development-only probe separately:

```powershell
cargo build --release -p hexforge-native --bin hexforge
cargo build --release -p hexforge-native --example hexforge_startup_probe
```

The probe supports:

```text
hexforge_startup_probe baseline EXE PID HWND OUT.json
hexforge_startup_probe watch EXE CALIBRATION.json OUT.json native
hexforge_startup_probe observe EXE CALIBRATION.json OUT.json native
```

Use explicit absolute paths and new output names outside the EXE-only folder. A baseline must be independently inspected and reviewed for the exact build, real controls, client dimensions, DPI, and theme before it can measure a launch. The default automatic rectangles apply only to 1280 × 800 at 96 DPI; other viewports require their own reviewed regions, not screenshot scaling or relaxed thresholds.

`watch` passively waits for an externally launched matching executable. It does not activate windows or send input. `observe` launches a clean test child with native phase diagnostics; it never closes the app automatically. Close only that clean test instance through its normal UI. The probe executable and its reports are development artifacts and are not distributed alongside HexForge.

For phase-only diagnostics, set `HEXFORGE_STARTUP_PROFILE` for the child process to a new absolute report filename outside the app folder. Normal launches do no diagnostic clock/report work. Reports are published on clean shutdown and never replace earlier files.

## Explicitly deferred or skipped

- The full paired warm/fresh-state benchmark and early interaction measurement are deferred by user direction. Native fresh state must never be called WebView-profile-cold or OS-cold.
- Physical drag-and-drop was manually verified by the user.
- Live Windows system-theme and per-monitor-DPI transition checks were skipped at the user's request. Automated theme/DPI/layout tests remain enabled; skipped checks are not counted as passes.
- A qualifying final frame does not prove there was no earlier white flash. The root is kept hidden until a successful real buffer swap, but complete independent white-flash coverage remains a stated measurement limitation.

For a future true OS-cold series, build before rebooting, use a predefined idle interval, perform no warmup, capture the first launch, and repeat across separately labeled reboots. Do not clear global caches or alter Windows settings automatically.
