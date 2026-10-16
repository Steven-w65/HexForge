//! Same menu/file strip, hex stage, bottom results, splitter and status layout as
//! the reference. Presenter state is local geometry/focus, not file authority.
use super::{file_strip::FileStrip, results::ResultView, top_menu::MenuView};
use crate::{
    style::{fonts, HexForgeStyle},
    widgets::{
        button::{ButtonSpec, StyledButton},
        dialog, input, splitter,
    },
};
use eframe::egui::{self, Align2};
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, InvokeContext, MainSnapshot},
    dialog::{DialogDecision, DialogKind},
    policy::command_unavailable_reason,
    preferences::PanelPreferences,
    shortcuts::WindowKind,
};
#[derive(Default)]
pub struct MainControls {
    pub menu: MenuView,
    pub results: ResultView,
    pub file: FileStrip,
    pub panel: PanelPreferences,
    pub search: super::search_bar::SearchBar,
    pub goto: super::goto_bar::GotoBar,
}
pub struct EmptyLayout {
    pub icon: egui::Rect,
    pub prompt: egui::Rect,
    pub button: egui::Rect,
}
/// CSS grid's centered 44px border-box, 7px gaps, 16px line box and 4px
/// button margin. Kept independent of glyph rasterization or egui spacing.
pub fn empty_layout(stage: egui::Rect) -> EmptyLayout {
    let center = stage.center();
    EmptyLayout {
        icon: egui::Rect::from_center_size(center - egui::vec2(0.0, 31.0), egui::vec2(44.0, 44.0)),
        prompt: egui::Rect::from_center_size(
            center + egui::vec2(0.0, 6.0),
            egui::vec2(129.6, 16.0),
        ),
        button: egui::Rect::from_center_size(
            center + egui::vec2(0.0, 39.0),
            egui::vec2(88.8, 28.0),
        ),
    }
}
pub fn show(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    s: &MainSnapshot,
    hex: &mut super::hex_view::HexView,
    cache: &mut crate::drawing::cache::HexDrawCache,
    controls: &mut MainControls,
) -> Vec<ControllerEvent> {
    let mut events = vec![];
    let whole = ui.max_rect();
    ui.painter().rect_filled(whole, 0, style.palette.bg);
    let top = egui::Rect::from_min_size(whole.min, egui::vec2(whole.width(), 34.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(top), |ui| {
        events.extend(controls.menu.show(ui, style, s))
    });
    let file_height = controls.file.height(ui, s.file.as_ref());
    let file = egui::Rect::from_min_size(top.left_bottom(), egui::vec2(whole.width(), file_height));
    ui.scope_builder(egui::UiBuilder::new().max_rect(file), |ui| {
        controls.file.show(ui, style, s.file.as_ref())
    });
    ui.painter().hline(
        top.x_range(),
        top.bottom() + 0.5,
        egui::Stroke::new(1.0, style.palette.border),
    );
    let status = egui::Rect::from_min_size(
        whole.left_bottom() - egui::vec2(0.0, 24.0),
        egui::vec2(whole.width(), 24.0),
    );
    let workspace = egui::Rect::from_min_max(file.left_bottom(), status.right_top());
    let panel_height = controls.panel.effective_height(workspace.height());
    let splitter_height = if controls.panel.collapsed { 0.0 } else { 4.0 };
    let results = egui::Rect::from_min_max(
        workspace.left_bottom() - egui::vec2(0.0, panel_height),
        workspace.max,
    );
    let stage = egui::Rect::from_min_max(
        workspace.min,
        results.right_top() - egui::vec2(0.0, splitter_height),
    );
    if s.file.is_some() {
        ui.scope_builder(egui::UiBuilder::new().max_rect(stage), |ui| {
            events.extend(hex.show_with_scrollbar_tail(ui, s, cache, splitter_height))
        });
    } else {
        hex.clear(cache);
        let center = stage.center();
        let layout = empty_layout(stage);
        let plus = layout.icon;
        dashed_icon(ui.painter(), plus, style.palette.border_strong);
        ui.painter().line_segment(
            [
                plus.center() - egui::vec2(7.0, 0.0),
                plus.center() + egui::vec2(7.0, 0.0),
            ],
            egui::Stroke::new(1.0, style.palette.address),
        );
        ui.painter().line_segment(
            [
                plus.center() - egui::vec2(0.0, 7.0),
                plus.center() + egui::vec2(0.0, 7.0),
            ],
            egui::Stroke::new(1.0, style.palette.address),
        );
        ui.painter().text(
            center + egui::vec2(0.0, 6.0),
            Align2::CENTER_CENTER,
            "Drop a binary file",
            fonts::regular(12.0),
            style.palette.muted,
        );
        let reason = command_unavailable_reason(Command::Open, &s.policy);
        let rect = layout.button;
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            if StyledButton::show(
                ui,
                style,
                &ButtonSpec {
                    width: Some(rect.width()),
                    height: Some(28.0),
                    disabled_reason: reason.as_deref(),
                    tooltip: Some("Open File (Ctrl+O)"),
                    ..ButtonSpec::new("Open File")
                },
            )
            .clicked()
            {
                events.push(ControllerEvent::Invoke {
                    command: Command::Open,
                    context: InvokeContext::default(),
                });
            }
        });
    }
    if s.file.as_ref().is_some_and(|f| f.size == "0") {
        let area = egui::Rect::from_min_max(
            stage.min,
            stage.max - egui::vec2(if s.minimap.enabled { 96.0 } else { 12.0 }, 0.0),
        );
        ui.painter().rect_filled(area, 0, style.palette.bg);
        ui.painter().text(
            area.center(),
            Align2::CENTER_CENTER,
            "This binary file is empty.",
            fonts::regular(12.0),
            style.palette.muted,
        );
        // The browser's address-gutter canvas is z-index:1 and remains above
        // this noninteractive overlay even for an empty file. Reuse the same
        // native renderer with a gutter-only clip, not a second header style.
        let rect = hex.frame().viewport_rect;
        let layout = hexforge_controller::hex::layout::HexLayout::new(
            hexforge_controller::hex::layout::HexMetrics {
                width: f64::from(rect.width()),
                char_width: hexforge_controller::hex::layout::DEFAULT_CHAR_WIDTH,
            },
            s.row_width,
        );
        let gutter = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(layout.gutter_width() as f32, rect.height()),
        );
        crate::drawing::hex::paint_hex(
            &ui.painter().with_clip_rect(gutter),
            &crate::drawing::hex::HexScene {
                rect,
                layout: &layout,
                snapshot: s,
                top_row: 0,
                horizontal_px: 0.0,
            },
            style,
        );
    }
    if !controls.panel.collapsed {
        // The full-height side scrollbar owns the rightmost 13px. Keep the
        // resize handle over the remaining divider, without a competing drag
        // target beneath the scrollbar's EOF thumb.
        let strip_width = (stage.width() - if s.file.is_some() { 13.0 } else { 0.0 }).max(0.0);
        let strip = egui::Rect::from_min_size(stage.left_bottom(), egui::vec2(strip_width, 4.0));
        let (min, max) = controls.panel.bounds(workspace.height());
        let mut height = panel_height;
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
            let r = splitter::show(ui, style, &mut height, min, max, strip.width());
            if r.dragged() || r.has_focus() {
                controls.panel.preferred_height = height;
            }
        });
    }
    ui.scope_builder(egui::UiBuilder::new().max_rect(results), |ui| {
        events.extend(
            controls
                .results
                .show(ui, style, s, controls.panel.collapsed),
        )
    });
    ui.scope_builder(egui::UiBuilder::new().max_rect(status), |ui| {
        events.extend(super::status::show(ui, style, s))
    });
    events.extend(controls.search.show(ui, style, s, stage));
    events.extend(controls.goto.show(ui, style, s, stage));
    if s.search.truncated && !s.search.open {
        let rect =
            egui::Rect::from_min_size(stage.min + egui::vec2(12.0, 10.0), egui::vec2(390.0, 27.0));
        ui.painter().rect_filled(rect, 4, style.palette.surface);
        ui.painter().rect_stroke(
            rect,
            4,
            egui::Stroke::new(1.0, style.palette.border),
            egui::StrokeKind::Inside,
        );
        super::results::cell(
            ui.painter(),
            rect.shrink2(egui::vec2(8.0, 5.0)),
            "Search results were limited; refine the byte pattern.",
            fonts::regular(11.0),
            style.palette.modified,
        );
    }
    events.extend(main_dialog(ui.ctx(), style, s));
    events
}
fn dashed_icon(p: &egui::Painter, r: egui::Rect, color: egui::Color32) {
    // Trace the same 1px dashed rounded border, including the corners. Keep
    // dashes in local pixels rather than replacing it with an egui stroke.
    let r = r.shrink(0.5);
    let radius = 7.5;
    let mut path = Vec::new();
    for (center, start) in [
        (
            r.right_top() + egui::vec2(-radius, radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (r.right_bottom() - egui::vec2(radius, radius), 0.0),
        (
            r.left_bottom() + egui::vec2(radius, -radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            r.left_top() + egui::vec2(radius, radius),
            std::f32::consts::PI,
        ),
    ] {
        for step in 0..=8 {
            let angle = start + step as f32 * std::f32::consts::FRAC_PI_2 / 8.0;
            path.push(center + radius * egui::vec2(angle.cos(), angle.sin()));
        }
    }
    path.push(path[0]);
    let mut travelled = 0.0;
    for points in path.windows(2) {
        let delta = points[1] - points[0];
        let length = delta.length();
        let direction = delta / length;
        let mut distance = 0.0;
        while distance < length {
            let phase: f32 = (travelled + distance) % 4.0;
            let portion = (2.0 - phase % 2.0).min(length - distance);
            if phase < 2.0 {
                p.line_segment(
                    [
                        points[0] + direction * distance,
                        points[0] + direction * (distance + portion),
                    ],
                    egui::Stroke::new(1.0, color),
                );
            }
            distance += portion.max(0.0001);
        }
        travelled += length;
    }
}
pub fn main_dialog(
    ctx: &egui::Context,
    style: &HexForgeStyle,
    s: &MainSnapshot,
) -> Vec<ControllerEvent> {
    let mut events = vec![];
    if let Some((id, request)) = s
        .dialog
        .as_ref()
        .filter(|(_, d)| d.owner == WindowKind::Main)
    {
        let mut decision = None;
        let response = dialog::show(
            ctx,
            style,
            egui::Id::new("main-workflow-dialog"),
            &request.title,
            &request.message,
            |ui| {
                ui.add_space(15.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    let options = if request.kind == DialogKind::BinaryUnsaved {
                        vec![
                            ("Cancel", DialogDecision::Cancel),
                            ("Discard", DialogDecision::Discard),
                            ("Save Copy and Continue", DialogDecision::Save),
                        ]
                    } else {
                        vec![
                            ("Cancel", DialogDecision::Cancel),
                            ("OK", DialogDecision::Confirm),
                        ]
                    };
                    for (label, d) in options {
                        let mut action_style = style.clone();
                        let spec = if request.kind == DialogKind::BinaryUnsaved {
                            // App.vue's binary-actions are 28px high with 14px
                            // padding, not the editor's 27px/9px buttons.
                            action_style.metrics.button_radius = 4;
                            let text_width = ui
                                .painter()
                                .layout_no_wrap(
                                    label.into(),
                                    fonts::regular(style.metrics.font_body),
                                    style.palette.text,
                                )
                                .size()
                                .x;
                            ButtonSpec {
                                width: Some(text_width + 28.0),
                                height: Some(28.0),
                                ..ButtonSpec::new(label)
                            }
                        } else {
                            ButtonSpec::new(label)
                        };
                        let response = StyledButton::show(ui, &action_style, &spec);
                        if request.kind == DialogKind::BinaryUnsaved && d == DialogDecision::Cancel
                        {
                            dialog::autofocus(ui, &response);
                        }
                        if response.clicked() {
                            decision = Some(d);
                        }
                    }
                });
            },
        );
        if response.cancel {
            decision = Some(DialogDecision::Cancel);
        }
        if let Some(decision) = decision {
            events.push(ControllerEvent::DialogCompleted {
                request_id: *id,
                decision,
            });
        }
    } else if let Some(edit) = &s.edit_dialog {
        let mut value = edit.value.clone();
        let mut apply = false;
        let mut cancel = false;
        let response = dialog::show(
            ctx,
            style,
            egui::Id::new("edit-byte-dialog"),
            "Edit byte",
            edit.error.as_deref().unwrap_or(""),
            |ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new("Hex byte value").font(fonts::regular(11.0)));
                let r = input::text(
                    ui,
                    style,
                    egui::Id::new("edit-byte-value"),
                    &mut value,
                    ui.available_width(),
                    "FF",
                );
                dialog::autofocus(ui, &r);
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && r.has_focus() && !edit.pending {
                    apply = true;
                }
                if !r.has_focus() && edit.value.is_empty() && ctx.memory(|m| m.focused().is_none())
                {
                    r.request_focus();
                }
                ui.label(
                    egui::RichText::new("Enter one hexadecimal byte from 00 to FF.")
                        .font(fonts::regular(11.0))
                        .color(style.palette.muted),
                );
                ui.add_space(5.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    apply |= StyledButton::show(
                        ui,
                        style,
                        &ButtonSpec {
                            disabled_reason: edit
                                .pending
                                .then_some("Wait for the current operation to finish."),
                            height: Some(28.0),
                            ..ButtonSpec::new("Apply")
                        },
                    )
                    .clicked();
                    cancel |= StyledButton::show(
                        ui,
                        style,
                        &ButtonSpec::new("Cancel")
                            .role(crate::widgets::button::ButtonRole::Transparent),
                    )
                    .clicked();
                });
            },
        );
        if value != edit.value {
            events.push(ControllerEvent::Action(Action::SetEditValue(value.clone())));
        }
        if apply {
            events.push(ControllerEvent::Action(Action::EditSelectedByte(value)));
        }
        if cancel || response.cancel {
            events.push(ControllerEvent::Action(Action::CancelEdit));
        }
    } else if let Some(error) = &s.error {
        let response = dialog::show(
            ctx,
            style,
            egui::Id::new("main-error-dialog"),
            "Operation failed",
            &error.message,
            |_| {},
        );
        if response.cancel {
            events.push(ControllerEvent::Action(Action::ClearError));
        }
    }
    events
}
