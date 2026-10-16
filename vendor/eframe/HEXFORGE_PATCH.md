# HexForge's pinned eframe integration

Base: eframe 0.36.2 from the installed crates.io package, upstream Git revision
`49682f8baa058bf49e011035cfbd6e825f88a5ef`. The original package metadata is
retained. HexForge uses the upstream MIT license included alongside this note.

The child accessibility change is in `src/native/glow_integration.rs`: retain the
root window's AccessKit event-loop proxy and initialize a provider for each
subsequently created native viewport, once per `egui_winit::State`. Upstream's
root registration remains unchanged. Viewport destruction
also destroys the owned provider; no separate thread, polling loop, HWND
subclass, alternate controller or UI state is introduced.

Reason: physical Windows verification exposed only a title-bar tree for the
separate Template Editor. Headless egui tests produced the correct control tree
but could not catch the missing OS provider. The root window was accessible.

The app creates the editor hidden and reveals it after painting the real child
frame: AccessKit rejects registration on an already visible window. The
`framework_accessibility.rs` test runs that actual presenter and checks its
window attributes/commands. Real Windows provider/focus/actions are verified
separately through Computer Use; headless controls alone cannot prove that the
OS provider is present.

When updating eframe, check whether upstream has fixed child registration,
retest the main and editor accessibility trees, and remove this local dependency
only if both still publish their real controls. No layout, template, minimap or
window-close algorithm is changed by this patch.

## Root presentation ordering

The same Glow integration calls the small shared helper in
`src/native/frame_presentation.rs` to swap the complete root frame **before**
revealing its initially hidden window. A failed swap cannot reveal an unpainted
root. The root is then focused using the native viewport command so its actual
primary controls, not a placeholder, can become visible and interactive.
No timer, splash, browser readiness marker, screenshot-as-UI, alternative
renderer or full-file bitmap was introduced.

`crates/hexforge-native/tests/frame_presentation.rs` executes that exact helper
and verifies both success ordering and failure behavior. The bootstrap and
window-lifecycle tests verify actual emitted viewport commands. These tests do
not prove compositor presentation latency or the absence of a physical white
flash; the independent desktop-pixel probe and live Windows checks remain the
authorities for those claims.

## Immediate-child swap outcomes

The Glow child renderer records its actual surface-swap outcome through
`viewport_presentation::swap` and returns before applying viewport output on
failure. The main app consumes a fresh, viewport-owned result before revealing
the hidden Template Editor; a completed UI callback alone is not success.
Missing/failed results use the existing retryable OpenFailed path and preserve
the draft. Consuming old output before each attempt prevents a previous frame
from authorizing a failed new render. Tests substitute only the graphics swap
at this same production boundary, not the presenter or controller. Actual
compositor/driver failure is not claimed as a live injected test.
