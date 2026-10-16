use crate::{
    style::{fonts, HexForgeStyle},
    widgets::button::{ButtonRole, ButtonSpec, StyledButton},
};
use eframe::egui::{self, Ui};

#[derive(Clone)]
struct FocusControl {
    id: egui::Id,
    rect: egui::Rect,
}
#[derive(Clone)]
struct FocusEntry {
    id: egui::Id,
    return_focus: Option<egui::Id>,
    controls: Vec<FocusControl>,
    preferred: Option<egui::Id>,
}
#[derive(Clone, Default)]
struct ModalFocusState {
    entries: Vec<FocusEntry>,
    previous: Vec<egui::Id>,
    shown: Vec<egui::Id>,
    building: Vec<egui::Id>,
    last_focus: Option<egui::Id>,
    tabs: Vec<bool>,
}
fn focus_state_id(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::new(("HexForge modal focus", viewport))
}
struct ModalFocus;
impl egui::Plugin for ModalFocus {
    fn debug_name(&self) -> &'static str {
        "HexForge modal focus"
    }
    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        crate::platform::input::observe_focus_modality(ctx, input);
        let id = focus_state_id(input.viewport_id);
        let active = ctx
            .data(|d| d.get_temp::<ModalFocusState>(id))
            .is_some_and(|s| !s.previous.is_empty());
        if active {
            // Intercept Tab before egui queues its own next/previous traversal.
            // This preserves the DOM's immediate boundary wrap instead of an
            // unfocused frame or visiting controls behind the modal.
            input.events.retain(|event| {
                if let egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if modifiers.is_none() || modifiers.shift_only() {
                        ctx.data_mut(|d| {
                            d.get_temp_mut_or_default::<ModalFocusState>(id)
                                .tabs
                                .push(modifiers.shift);
                        });
                        return false;
                    }
                }
                true
            });
        }
    }
    fn on_begin_pass(&mut self, ui: &mut Ui) {
        let id = focus_state_id(ui.ctx().viewport_id());
        ui.ctx().data_mut(|d| {
            let state = d.get_temp_mut_or_default::<ModalFocusState>(id);
            state.shown.clear();
            state.building.clear();
        });
    }
    fn on_end_pass(&mut self, ui: &mut Ui) {
        finish_focus(ui.ctx());
    }
}
/// One plugin, with separate focus checkpoints for each native viewport.
pub fn install_focus(ctx: &egui::Context) {
    ctx.add_plugin(ModalFocus);
}
fn begin_focus(ctx: &egui::Context, id: egui::Id) {
    let focused = ctx.memory(|m| m.focused());
    let state_id = focus_state_id(ctx.viewport_id());
    ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<ModalFocusState>(state_id);
        if let Some(entry) = state.entries.iter_mut().find(|e| e.id == id) {
            entry.controls.clear();
            entry.preferred = None;
        } else {
            state.entries.push(FocusEntry {
                id,
                return_focus: focused.or(state.last_focus),
                controls: vec![],
                preferred: None,
            });
        }
        state.shown.push(id);
        state.building.push(id);
    });
}
/// Register only enabled, actually painted controls inside the current dialog.
/// Geometry orders wrapped action rows left-to-right, matching the source DOM
/// even when egui's right-aligned layout allocates buttons in reverse order.
pub(crate) fn register_focus(ui: &Ui, response: &egui::Response, enabled: bool) {
    if enabled {
        let state_id = focus_state_id(ui.ctx().viewport_id());
        ui.ctx().data_mut(|d| {
            let state = d.get_temp_mut_or_default::<ModalFocusState>(state_id);
            if let Some(id) = state.building.last().copied() {
                if let Some(entry) = state.entries.iter_mut().find(|e| e.id == id) {
                    entry.controls.push(FocusControl {
                        id: response.id,
                        rect: response.rect,
                    });
                }
            }
        });
    }
}
/// Equivalent to the reference dialog's enabled [autofocus] control.
pub(crate) fn autofocus(ui: &Ui, response: &egui::Response) {
    if response.enabled() {
        let state_id = focus_state_id(ui.ctx().viewport_id());
        ui.ctx().data_mut(|d| {
            let state = d.get_temp_mut_or_default::<ModalFocusState>(state_id);
            if let Some(id) = state.building.last().copied() {
                if let Some(entry) = state.entries.iter_mut().find(|e| e.id == id) {
                    entry.preferred = Some(response.id);
                }
            }
        });
    }
}
fn finish_focus(ctx: &egui::Context) {
    let id = focus_state_id(ctx.viewport_id());
    let mut state = ctx
        .data(|d| d.get_temp::<ModalFocusState>(id))
        .unwrap_or_default();
    let focused = ctx.memory(|m| m.focused());
    let old_top = state.previous.last().copied();
    let new_top = state.shown.last().copied();
    let returning = old_top
        .filter(|old| !state.shown.contains(old))
        .and_then(|old| state.entries.iter().find(|e| e.id == old))
        .and_then(|entry| entry.return_focus);
    let mut target = None;
    if let Some(top) = new_top {
        let entry = state.entries.iter_mut().find(|e| e.id == top).unwrap();
        entry.controls.sort_by(|a, b| {
            a.rect
                .center()
                .y
                .total_cmp(&b.rect.center().y)
                .then_with(|| a.rect.left().total_cmp(&b.rect.left()))
        });
        entry.controls.dedup_by_key(|control| control.id);
        let contains = |id| entry.controls.iter().any(|control| control.id == id);
        let current = returning
            .filter(|id| contains(*id))
            .or_else(|| focused.filter(|id| contains(*id)))
            .or_else(|| entry.preferred)
            .or_else(|| entry.controls.first().map(|control| control.id));
        target = current;
        if !state.tabs.is_empty() {
            let start = state.last_focus.filter(|id| contains(*id)).or(current);
            if let Some(mut index) =
                start.and_then(|id| entry.controls.iter().position(|c| c.id == id))
            {
                let length = entry.controls.len();
                for backwards in state.tabs.drain(..) {
                    index = if backwards {
                        (index + length - 1) % length
                    } else {
                        (index + 1) % length
                    };
                }
                target = Some(entry.controls[index].id);
            }
        } else if !state.previous.contains(&top) {
            target = entry.preferred.or(current);
        }
    } else {
        if let Some(returning) = returning {
            if ctx.read_response(returning).is_some_and(|r| r.enabled()) {
                target = Some(returning);
            }
        }
        // Even a dialog without an invoking control ends its focus session.
        // A later opening must take a new checkpoint, not reuse a stale None.
        state.entries.clear();
        state.tabs.clear();
    }
    if let Some(target) = target {
        ctx.memory_mut(|m| {
            m.move_focus(egui::FocusDirection::None);
            if !m.has_focus(target) {
                m.request_focus(target);
            }
        });
    }
    state.last_focus = target.or(focused);
    state.previous = state.shown.clone();
    ctx.data_mut(|d| d.insert_temp(id, state));
}
pub struct DialogResponse<T> {
    pub inner: T,
    pub cancel: bool,
}
pub fn show<T>(
    ctx: &egui::Context,
    s: &HexForgeStyle,
    id: egui::Id,
    title: &str,
    message: &str,
    content: impl FnOnce(&mut Ui) -> T,
) -> DialogResponse<T> {
    show_in_stack(ctx, s, id, title, message, true, content)
}
/// Paint a covered dialog in the same stack without treating its controls as
/// policy-disabled. DOM `inert` leaves the reference's visuals unchanged.
/// Covered layers cannot consume Escape or backdrop clicks; their caller also
/// ignores decisions while the higher dialog is present.
pub fn show_in_stack<T>(
    ctx: &egui::Context,
    s: &HexForgeStyle,
    id: egui::Id,
    title: &str,
    message: &str,
    interactive: bool,
    content: impl FnOnce(&mut Ui) -> T,
) -> DialogResponse<T> {
    begin_focus(ctx, id);
    let mut close = false;
    // Area rounds its origin to device pixels. A viewport-sized modal layer
    // keeps that origin stable while the actual CSS-centered frame can retain
    // half-pixel coordinates (e.g. a 145px dialog in a 760px client).
    let viewport = ctx.content_rect();
    let size_id = id.with("dialog measured size");
    let previous_height = ctx.data(|d| d.get_temp::<f32>(size_id)).unwrap_or(0.0);
    let width = s
        .metrics
        .dialog_width
        .min((viewport.width() - 40.0).max(280.0));
    let dialog_rect =
        egui::Rect::from_center_size(viewport.center(), egui::vec2(width, previous_height));
    let mut dialog_response = None;
    let area = egui::Modal::default_area(id)
        .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
        .interactable(interactive);
    let is_top_modal = ctx.memory_mut(|memory| {
        memory.set_modal_layer(area.layer());
        memory.top_modal_layer() == Some(area.layer())
    });
    let any_popup_open = egui::Popup::is_any_open(ctx);
    let shown = area.show(ctx, |layer_ui| {
        // Separate the full-client backdrop hit target from the dialog's
        // click blocker. A fullscreen content frame would swallow genuine
        // backdrop clicks as well as rounding the centered dialog.
        let mut backdrop = layer_ui.new_child(
            egui::UiBuilder::new()
                .sense(egui::Sense::CLICK | egui::Sense::DRAG)
                .max_rect(viewport),
        );
        backdrop.set_min_size(viewport.size());
        layer_ui
            .painter()
            .rect_filled(viewport, 0, egui::Color32::from_black_alpha(107));
        let backdrop_response = backdrop.response();
        let inner = layer_ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(dialog_rect)
                    .sense(egui::Sense::CLICK | egui::Sense::DRAG),
                |ui| {
                    let frame = egui::Frame::new()
                        .fill(s.palette.panel)
                        .stroke(egui::Stroke::new(1.0, s.palette.border_strong))
                        .corner_radius(7)
                        .inner_margin(15)
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 8],
                            blur: 24,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(41),
                        })
                        .show(ui, |ui| {
                            // AppDialog uses CSS margins, not an implicit gap between every
                            // row. Its header is 26px and message line-height is exactly 18px.
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.spacing_mut().interact_size.y = 26.0;
                            ui.set_width(width - 32.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(title)
                                        .font(fonts::bold(13.0))
                                        .color(s.palette.text),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let spec = ButtonSpec {
                                            width: Some(26.0),
                                            height: Some(26.0),
                                            font_size: Some(18.0),
                                            accessible_name: Some("Close dialog"),
                                            ..ButtonSpec::new("×").role(ButtonRole::DialogClose)
                                        };
                                        close |= StyledButton::show(ui, s, &spec).clicked();
                                    },
                                );
                            });
                            if !message.is_empty() {
                                ui.add_space(10.0);
                                ui.label(egui::text::LayoutJob::single_section(
                                    message.into(),
                                    egui::TextFormat {
                                        font_id: fonts::regular(12.0),
                                        color: s.palette.muted,
                                        line_height: Some(18.0),
                                        ..Default::default()
                                    },
                                ));
                            }
                            content(ui)
                        });
                    let height = frame.response.rect.height();
                    if (height - previous_height).abs() > 0.01 {
                        ctx.data_mut(|d| d.insert_temp(size_id, height));
                        ctx.request_discard("center measured real dialog, without pixel rounding");
                    }
                    dialog_response = Some(frame.response);
                    frame.inner
                },
            )
            .inner;
        (inner, backdrop_response)
    });
    let r = egui::ModalResponse {
        response: shown.response,
        backdrop_response: shown.inner.1,
        inner: shown.inner.0,
        is_top_modal,
        any_popup_open,
    };
    // Modal supplies input layering; name its Area explicitly for screen readers.
    ctx.accesskit_node_builder(dialog_response.as_ref().unwrap().id, |node| {
        node.set_role(egui::accesskit::Role::Dialog);
        node.set_label(title);
        node.set_modal();
        if !interactive {
            node.set_disabled();
        }
    });
    let state_id = focus_state_id(ctx.viewport_id());
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<ModalFocusState>(state_id)
            .building
            .pop();
    });
    let composing_escape = crate::platform::input::key_is_composing(
        ctx,
        crate::platform::input::current_window(ctx),
        egui::Key::Escape,
        egui::Modifiers::NONE,
    );
    // Keep pointer dismissals unchanged, but do not let egui's default modal
    // Escape handler consume the IME's candidate-cancellation key.
    let cancel = interactive
        && (close
            || if composing_escape {
                r.backdrop_response.clicked() || r.response.should_close()
            } else {
                r.should_close()
            });
    DialogResponse {
        inner: r.inner,
        cancel,
    }
}
