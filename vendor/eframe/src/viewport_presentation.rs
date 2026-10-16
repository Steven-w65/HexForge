//! Actual immediate-viewport swap outcomes. Running a child UI callback only
//! draws its back buffer; it does not prove that the child frame was presented.
//! Keep this result viewport-local and consume it once to reject stale success.

#[derive(Clone, Default)]
struct SwapOutcome(Option<Result<(), String>>);

fn key(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::new(("eframe immediate viewport swap", viewport))
}

/// Execute the real surface swap and retain its outcome for this viewport's
/// caller. Failed swaps must return before applying visibility/focus output.
pub fn swap<E: std::fmt::Display>(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
    swap_buffers: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    let result = swap_buffers();
    ctx.data_mut(|data| {
        data.insert_temp(
            key(viewport),
            SwapOutcome(Some(
                result.as_ref().map(|_| ()).map_err(ToString::to_string),
            )),
        );
    });
    result
}

/// Consume the last actual swap result for a child. Call before a new immediate
/// render to clear stale output, then after it to verify this frame's outcome.
pub fn take(ctx: &egui::Context, viewport: egui::ViewportId) -> Option<Result<(), String>> {
    ctx.data_mut(|data| data.remove_temp::<SwapOutcome>(key(viewport)))
        .and_then(|outcome| outcome.0)
}
