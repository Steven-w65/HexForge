//! Input-method ownership survives frames but is scoped to a native window.
//! A commit late in a frame must not turn an earlier composition key into an
//! application command. Text-edit controls retain the original IME events.
use eframe::egui;
use hexforge_controller::shortcuts::WindowKind;

#[derive(Clone, Default)]
struct Composition {
    frame: Option<u64>,
    active: bool,
    keys: Vec<(egui::Key, egui::Modifiers)>,
}

fn focus_visibility_id(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::new(("HexForge focus-visible", viewport))
}

/// Observe raw modality before a dialog consumes Tab for its focus trap.
/// Programmatic autofocus does not invent keyboard intent; pointer movement
/// alone does not erase the visible focus of a keyboard-operated control.
pub(crate) fn observe_focus_modality(ctx: &egui::Context, input: &egui::RawInput) {
    let id = focus_visibility_id(input.viewport_id);
    let mut keyboard = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    for event in &input.events {
        match event {
            // Like the source browser's :focus-visible heuristic, an Alt
            // access key (or Ctrl/Meta shortcut) does not invent keyboard
            // navigation intent. Preserve earlier intent until a pointer
            // press or an unmodified navigation key changes it.
            egui::Event::Key {
                pressed: true,
                modifiers,
                ..
            } if !modifiers.alt && !modifiers.ctrl && !modifiers.mac_cmd => keyboard = true,
            egui::Event::PointerButton { pressed: true, .. } => keyboard = false,
            _ => {}
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, keyboard));
}

pub fn focus_visible(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(focus_visibility_id(ctx.viewport_id())))
        .unwrap_or(false)
}

fn identity(ctx: &egui::Context, window: WindowKind) -> egui::Id {
    egui::Id::new((
        "HexForge IME",
        ctx.viewport_id(),
        window == WindowKind::TemplateEditor,
    ))
}

/// Observe each input frame once, including egui's additional layout passes.
pub fn begin_frame(ctx: &egui::Context, window: WindowKind) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("HexForge input owner"), window));
    let frame = ctx.cumulative_frame_nr();
    let id = identity(ctx, window);
    let observed = ctx.data(|d| d.get_temp::<Composition>(id));
    if observed.as_ref().is_some_and(|s| s.frame == Some(frame)) {
        return;
    }
    let (focused, events) = ctx.input(|i| (i.raw.focused, i.events.clone()));
    let mut state = observed.unwrap_or_default();
    state.frame = Some(frame);
    state.keys.clear();
    if !focused {
        state.active = false;
    } else {
        for event in events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    // Egui/winit uses an empty preedit to dismiss composition.
                    state.active = !text.is_empty();
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => state.active = false,
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } if state.active => {
                    state.keys.push((key, modifiers));
                }
                _ => {}
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, state));
}

pub fn current_window(ctx: &egui::Context) -> WindowKind {
    ctx.data(|d| d.get_temp::<WindowKind>(egui::Id::new("HexForge input owner")))
        .unwrap_or(WindowKind::Main)
}

pub fn key_is_composing(
    ctx: &egui::Context,
    window: WindowKind,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> bool {
    begin_frame(ctx, window);
    ctx.data(|d| d.get_temp::<Composition>(identity(ctx, window)))
        .is_some_and(|state| state.keys.contains(&(key, modifiers)))
}

/// Menus and other non-text controls must not capture IME navigation keys.
pub fn keyboard_owned_by_ime(ctx: &egui::Context, window: WindowKind) -> bool {
    begin_frame(ctx, window);
    ctx.data(|d| d.get_temp::<Composition>(identity(ctx, window)))
        .is_some_and(|state| state.active || !state.keys.is_empty())
}
