use crate::{
    style::{fonts, HexForgeStyle},
    widgets::button::{ButtonRole, ButtonSpec, StyledButton},
};
use eframe::egui::{self, Align2, Color32};
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, InvokeContext},
    policy::command_unavailable_reason,
    results::{navigable_range, ResultAction, ResultKey, ResultTree, ResultViewport},
    template::status::template_status,
};
use hexforge_controller::{controller::MainSnapshot, template::status::TemplateSource};
pub struct PanelState {
    pub message: Option<String>,
    pub actions: bool,
    pub show_results: bool,
}
pub fn panel_state(s: &MainSnapshot) -> PanelState {
    let source = s.template_state.source;
    let has_file = s.file.is_some();
    let name = active_name(s);
    let show_results = has_file && source != TemplateSource::None && s.template_applied;
    let message = if show_results {
        None
    } else {
        Some(match source {
            TemplateSource::None => if has_file {
                "No template loaded. Load or create template from Template menu."
            } else {
                "No template loaded."
            }
            .into(),
            TemplateSource::Draft => format!("Unsaved template draft: \"{name}\"."),
            TemplateSource::File => {
                if has_file {
                    format!("Template: \"{name}\" loaded, pending apply.")
                } else {
                    format!("Template: \"{name}\" loaded. Open binary file to preview.")
                }
            }
        })
    };
    PanelState {
        message,
        show_results,
        actions: has_file && source == TemplateSource::File && !s.template_applied,
    }
}

fn active_name(s: &MainSnapshot) -> &str {
    let name = s.template_state.display_name.trim();
    if name.is_empty() {
        "Untitled"
    } else {
        name
    }
}

fn detail_galley(
    painter: &egui::Painter,
    text: String,
    width: f32,
    font: egui::FontId,
    color: Color32,
    line_height: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text,
        egui::TextFormat {
            font_id: font,
            color,
            line_height: Some(line_height),
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(1.0);
    job.wrap.break_anywhere = true;
    painter.layout_job(job)
}

#[derive(Default)]
pub struct ResultView {
    pub tree: ResultTree,
    scroll: f32,
    focus_requested: bool,
}
pub fn offset_label(value: Option<&str>) -> String {
    value.map_or_else(
        || "—".into(),
        |raw| {
            raw.parse::<u64>()
                .map_or_else(|_| raw.into(), |n| format!("0x{n:08X}"))
        },
    )
}
fn has_diagnostics(nodes: &[hexforge_core::template::ParsedNode]) -> bool {
    nodes
        .iter()
        .any(|n| !n.diagnostics.is_empty() || has_diagnostics(&n.children))
}
pub(crate) fn cell(
    p: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    font: egui::FontId,
    color: Color32,
) {
    let mut job = egui::text::LayoutJob::simple(text.into(), font, color, rect.width());
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = p.layout_job(job);
    p.with_clip_rect(rect).galley(
        rect.left_center() - egui::vec2(0.0, galley.size().y / 2.0),
        galley,
        color,
    );
}
impl ResultView {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        style: &HexForgeStyle,
        s: &MainSnapshot,
        collapsed: bool,
    ) -> Vec<ControllerEvent> {
        let mut events = vec![];
        self.tree.replace(s.results.clone());
        let whole = ui.max_rect();
        ui.painter().rect_filled(whole, 0, style.palette.panel);
        // The source's panel has a 1px top border before its 28px header.
        let header = egui::Rect::from_min_size(
            whole.min + egui::vec2(0.0, 1.0),
            egui::vec2(whole.width(), 28.0),
        );
        ui.painter().hline(
            header.x_range(),
            whole.top() + 0.5,
            egui::Stroke::new(1.0, style.palette.border),
        );
        let p = ui.painter().clone();
        let heading = p.layout_job(fonts::spaced(
            "PARSED RESULTS",
            fonts::regular(11.0),
            style.palette.muted,
            0.88,
        ));
        let mut caption_right = header.left() + 12.0 + heading.size().x + 0.88;
        p.galley(
            egui::pos2(
                header.left() + 12.0,
                header.center().y - 0.5 - heading.size().y / 2.0,
            ),
            heading,
            style.palette.muted,
        );
        let count = self.tree.navigable_leaves().len();
        if count > 0 {
            p.text(
                egui::pos2(caption_right + 6.6 + 0.88 + 5.0, header.center().y - 0.5),
                Align2::LEFT_CENTER,
                count.to_string(),
                fonts::regular(11.0),
                style.palette.address,
            );
            caption_right += 6.6 + 0.88 + 5.0 + count.to_string().len() as f32 * 6.6;
        }
        let status = template_status(
            s.template_state.source,
            s.template_state.dirty,
            s.template_applied,
            s.results_need_refresh,
            s.file.is_some(),
            has_diagnostics(&s.results),
        );
        let status = if s.template_state.source == TemplateSource::None {
            status
        } else {
            format!("{} · {status}", active_name(s))
        };
        let status_rect = egui::Rect::from_min_max(
            egui::pos2(caption_right + 10.0, header.top()),
            header.max - egui::vec2(40.0, 1.0),
        );
        let galley = p.layout_no_wrap(status.clone(), fonts::regular(11.0), style.palette.address);
        p.with_clip_rect(status_rect).galley(
            egui::pos2(
                status_rect.right() - galley.size().x,
                status_rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            style.palette.address,
        );
        let collapse_rect = egui::Rect::from_min_size(
            header.right_top() + egui::vec2(-30.0, 2.5),
            egui::vec2(22.0, 22.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(collapse_rect), |ui| {
            if StyledButton::show(
                ui,
                style,
                &ButtonSpec {
                    accessible_name: Some(if collapsed {
                        "Expand parsed results"
                    } else {
                        "Collapse parsed results"
                    }),
                    width: Some(22.0),
                    height: Some(22.0),
                    font_size: Some(16.0),
                    icon_only: true,
                    tooltip: Some(if collapsed {
                        "Expand results"
                    } else {
                        "Collapse results"
                    }),
                    ..ButtonSpec::new(if collapsed { "⌃" } else { "⌄" })
                        .role(ButtonRole::Transparent)
                },
            )
            .clicked()
            {
                events.push(ControllerEvent::Invoke {
                    command: Command::ToggleRightPanel,
                    context: InvokeContext::default(),
                });
            }
        });
        if collapsed {
            return events;
        }
        p.hline(
            header.x_range(),
            header.bottom() - 0.5,
            egui::Stroke::new(1.0, style.palette.border),
        );
        let content = egui::Rect::from_min_max(header.left_bottom(), whole.max);
        let state = panel_state(s);
        if let Some(message) = state.message {
            if !state.actions {
                let mut job = egui::text::LayoutJob::single_section(
                    message,
                    egui::TextFormat {
                        font_id: fonts::regular(12.0),
                        color: style.palette.muted,
                        line_height: Some(18.0),
                        ..Default::default()
                    },
                );
                job.wrap.max_width = (content.width() - 36.0).max(1.0);
                job.halign = egui::Align::Center;
                let galley = p.layout_job(job);
                p.with_clip_rect(content).galley(
                    // Center the actual local bounds. With halign=Center their
                    // X origin is negative; subtracting size()/2 centers twice.
                    content.center() - galley.rect.center().to_vec2(),
                    galley,
                    style.palette.muted,
                );
                return events;
            }
            // Source grid: 18px text line, 7px grid gap + 4px action margin,
            // then 27px buttons. Center that intrinsic 56px group rather than
            // relying on egui's horizontal layout to center an expanding Ui.
            let mut job = egui::text::LayoutJob::single_section(
                message,
                egui::TextFormat {
                    font_id: fonts::regular(12.0),
                    color: style.palette.muted,
                    line_height: Some(18.0),
                    ..Default::default()
                },
            );
            job.wrap.max_width = (content.width() - 36.0).max(1.0);
            job.halign = egui::Align::Center;
            let galley = p.layout_job(job);
            let line_height = galley.rect.height();
            let group_top = content.center().y - (line_height + 11.0 + 27.0) / 2.0;
            p.with_clip_rect(content).galley(
                egui::pos2(content.center().x, group_top + line_height / 2.0)
                    - galley.rect.center().to_vec2(),
                galley,
                style.palette.muted,
            );
            let actions = [
                (Command::TemplateEditor, "Template Editor"),
                (Command::ApplyTemplate, "Apply Template"),
            ];
            let widths = actions.map(|(_, label)| {
                p.layout_no_wrap(label.into(), fonts::regular(12.0), style.palette.text)
                    .size()
                    .x
                    + 20.0
            });
            let action_rect = egui::Rect::from_min_size(
                egui::pos2(
                    content.center().x - (widths[0] + 8.0 + widths[1]) / 2.0,
                    group_top + line_height + 11.0,
                ),
                egui::vec2(widths[0] + 8.0 + widths[1], 27.0),
            );
            let mut action_style = style.clone();
            action_style.metrics.button_radius = 4;
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(action_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for ((command, label), width) in actions.into_iter().zip(widths) {
                        let reason = command_unavailable_reason(command, &s.policy);
                        if StyledButton::show(
                            ui,
                            &action_style,
                            &ButtonSpec {
                                width: Some(width),
                                disabled_reason: reason.as_deref(),
                                ..ButtonSpec::new(label)
                            },
                        )
                        .clicked()
                        {
                            events.push(ControllerEvent::Invoke {
                                command,
                                context: InvokeContext::default(),
                            });
                        }
                    }
                },
            );
            return events;
        }
        let detail = self.tree.diagnostic_node().cloned();
        let detail_layout = detail.as_ref().map(|d| {
            let layout = |width| {
                let heading = detail_galley(
                    &p,
                    d.path.clone(),
                    width,
                    fonts::bold(11.0),
                    style.palette.modified,
                    14.0,
                );
                let paragraphs: Vec<_> = d
                    .diagnostics
                    .iter()
                    .map(|issue| {
                        detail_galley(
                            &p,
                            issue.message.clone(),
                            width,
                            fonts::regular(11.0),
                            style.palette.modified,
                            16.5,
                        )
                    })
                    .collect();
                // Galley bounds round outward to physical pixels. CSS line
                // boxes do not: preserve the 16.5px paragraph advance for
                // layout instead of letting raster bounds grow the pane.
                let natural = heading.rows.len() as f32 * 14.0
                    + paragraphs
                        .iter()
                        .map(|galley| 5.0 + galley.rows.len() as f32 * 16.5)
                        .sum::<f32>();
                (heading, paragraphs, natural)
            };
            let width = content.width() - 24.0;
            let (mut heading, mut paragraphs, mut natural) = layout(width);
            if natural > 110.0 {
                (heading, paragraphs, natural) = layout(width - 10.0);
            }
            // Source CSS max-height bounds the CONTENT box. The 8px top/
            // bottom padding and 1px border add 17px; paragraph height depends
            // on wrapping, not on how many diagnostic objects were returned.
            (heading, paragraphs, natural.min(110.0) + 17.0)
        });
        let detail_height = detail_layout.as_ref().map_or(0.0, |(_, _, height)| *height);
        let list = egui::Rect::from_min_max(
            content.min,
            egui::pos2(
                content.right(),
                (content.bottom() - detail_height).max(content.top()),
            ),
        );
        // Native thin scrollbars are non-floating, exactly like the source's
        // overflow:auto list. The sticky heading reserves the same gutter as
        // its rows; its own CSS grid has no 2px row selection border.
        let scrollbar_width = if (self.tree.row_count() + 1) as f32 * 28.0 > list.height() {
            10.0
        } else {
            0.0
        };
        let columns = egui::Rect::from_min_size(
            list.min,
            egui::vec2((list.width() - scrollbar_width).max(1.0), 28.0),
        );
        let mut column_origin_x = columns.left();
        let mut column_grid_width = columns.width().max(620.0);
        let body = egui::Rect::from_min_max(egui::pos2(list.left(), columns.bottom()), list.max);
        if s.results.is_empty() {
            p.text(
                body.min + egui::vec2(16.0, 16.0),
                Align2::LEFT_TOP,
                "No parsed fields.",
                fonts::regular(12.0),
                style.palette.muted,
            );
        }
        let focus_id = ui.id().with("result-tree-focus");
        // Keep a registered focus owner even when the active row is offscreen.
        ui.interact(body, focus_id, egui::Sense::click())
            .widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Other,
                    ui.is_enabled(),
                    "Parsed template fields",
                )
            });
        // A just-clicked tree has not had a frame to install its event filter.
        // Cancel the framework's queued directional focus traversal as well
        // as consuming the key; traversal otherwise runs at the END of this
        // pass and moves focus to the status controls after our row changes.
        let owns_arrow = ui.memory(|m| m.had_focus_last_frame(focus_id))
            && ui.input(|i| {
                !i.pointer.any_pressed()
                    && [
                        egui::Key::ArrowDown,
                        egui::Key::ArrowUp,
                        egui::Key::ArrowLeft,
                        egui::Key::ArrowRight,
                    ]
                    .iter()
                    .any(|k| i.key_pressed(*k))
            });
        if owns_arrow {
            ui.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        let focused = ui.memory(|m| m.has_focus(focus_id));
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    focus_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        let enabled = ui.is_enabled()
            && s.dialog.is_none()
            && s.edit_dialog.is_none()
            && !s.editor_close_prompt
            && s.error.is_none();
        if focused && enabled {
            for (key, resultkey) in [
                (egui::Key::ArrowDown, ResultKey::Down),
                (egui::Key::ArrowUp, ResultKey::Up),
                (egui::Key::ArrowLeft, ResultKey::Left),
                (egui::Key::ArrowRight, ResultKey::Right),
                (egui::Key::Home, ResultKey::Home),
                (egui::Key::End, ResultKey::End),
                (egui::Key::Enter, ResultKey::Enter),
                (egui::Key::Space, ResultKey::Space),
            ] {
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                    if let Some(ResultAction::Navigate { start, end }) =
                        self.tree.handle_key(resultkey)
                    {
                        events.push(ControllerEvent::Action(Action::NavigateTemplate {
                            start,
                            end,
                        }));
                    }
                    self.focus_requested = true;
                }
            }
        }
        if self.focus_requested {
            if let Some(index) = self.tree.active_index() {
                let y = index as f32 * 28.0;
                if y < self.scroll {
                    self.scroll = y;
                } else if y + 28.0 > self.scroll + body.height() {
                    self.scroll = (y + 28.0 - body.height()).max(0.0);
                }
            }
            self.focus_requested = false;
        }
        let mut rendered_rows = Vec::new();
        // Use the keyboard owner as the row scope's accessibility parent.
        // Keep a distinct widget ID: reusing focus_id for a layout-only Ui
        // would replace the registered focusable tree with a hover-only Ui.
        // AccessKit
        // then exposes only this bounded tile as children of the same tree;
        // virtualizing the drawing must not hide its per-row semantics.
        ui.scope_builder(
            egui::UiBuilder::new()
                .accessibility_parent(focus_id)
                .max_rect(body),
            |ui| {
                ui.spacing_mut().scroll.bar_width = 6.0;
                ui.spacing_mut().scroll.bar_inner_margin = 4.0;
                ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
                // Allow the source bar to extend upward beside the sticky
                // heading without permitting rows to paint over that heading.
                let output = crate::widgets::scroll_area::both_viewport_with_header(ui, style, "result-tree-scroll", 28.0, self.scroll, |ui, viewport| {
                    let width = ui.available_width().max(620.0);
                    let count = self.tree.row_count();
                    // At the CSS grid's 620px minimum, fixed/minmax cells
                    // extend 4px beyond a row (2px for the borderless heading).
                    // Reserve that real overflow without widening the row or
                    // changing its independent grid sizing.
                    let minimum_cells = column_widths(width - if count > 0 { 2.0 } else { 0.0 });
                    let content_width = width.max(if count > 0 { 12.0 } else { 10.0 }
                        + minimum_cells.iter().sum::<f32>() + 32.0);
                    let (all, _) = ui.allocate_exact_size(
                        egui::vec2(content_width, count as f32 * 28.0),
                        egui::Sense::hover(),
                    );
                    column_origin_x = all.left();
                    column_grid_width = width;
                    let rows = self.tree.visible_nodes(ResultViewport {
                        scroll_top: viewport.top() as f64,
                        height: body.height() as f64,
                    });
                    let mut activate = None;
                    let mut accessibility_focus = None;
                    for row in rows {
                        let rect = egui::Rect::from_min_size(
                            all.min + egui::vec2(0.0, row.index as f32 * 28.0),
                            egui::vec2(width, 28.0),
                        );
                        let response = ui.interact(
                            rect,
                            ui.id().with(&row.node.path),
                            if enabled {
                                // The tree is one Tab stop, like the source's
                                // roving tabindex. Row clicks still activate,
                                // but virtual rows must not each enter Tab order.
                                egui::Sense::CLICK
                            } else {
                                egui::Sense::hover()
                            },
                        );
                        let active = navigable_range(row.node).is_some_and(|(start, end)| {
                            s.template_range
                                .as_ref()
                                .is_some_and(|r| r.start == start && r.end == end)
                        });
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Other,
                                enabled,
                                &row.node.name,
                            )
                        });
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_role(egui::accesskit::Role::TreeItem);
                            node.set_value(row.node.path.as_str());
                            node.set_level(row.depth + 1);
                            node.set_position_in_set(row.position);
                            node.set_size_of_set(row.siblings);
                            node.set_selected(active);
                            if active {
                                node.set_aria_current(egui::accesskit::AriaCurrent::Location);
                            }
                            if !row.node.children.is_empty() {
                                node.set_expanded(self.tree.expanded(&row.node.path));
                            }
                            if !row.node.comment.is_empty() {
                                node.set_tooltip(row.node.comment.as_str());
                            }
                            if enabled {
                                node.add_action(egui::accesskit::Action::Focus);
                            }
                        });
                        if enabled
                            && ui.input(|input| {
                                input.events.iter().any(|event| {
                                    matches!(event,
                                        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                                            action: egui::accesskit::Action::Focus,
                                            target_node,
                                            target_tree: egui::accesskit::TreeId::ROOT,
                                            data: None,
                                        }) if *target_node == response.id.accesskit_id()
                                    )
                                })
                            })
                        {
                            accessibility_focus = Some(row.node.path.clone());
                        }
                        rendered_rows.push((row.node.path.clone(), response.id));
                        let p = ui.painter();
                        if active {
                            p.rect_filled(
                                rect,
                                0,
                                blend(style.palette.selection, style.palette.panel, 0.16),
                            );
                        } else if response.hovered() {
                            p.rect_filled(rect, 0, style.palette.hover);
                        }
                        p.hline(
                            rect.x_range(),
                            rect.bottom() - 0.5,
                            egui::Stroke::new(1.0, style.palette.border),
                        );
                        if active || !row.node.diagnostics.is_empty() {
                            p.rect_filled(
                                egui::Rect::from_min_size(rect.min, egui::vec2(2.0, 28.0)),
                                0,
                                if !row.node.diagnostics.is_empty() {
                                    style.palette.modified
                                } else {
                                    style.palette.selection
                                },
                            );
                        }
                        if focused && row.node.path == self.tree.active_path() {
                            p.rect_stroke(
                                rect,
                                0,
                                egui::Stroke::new(1.0, style.palette.selection),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let mut x = rect.left() + 12.0;
                        let widths = column_widths(width - 2.0);
                        let indent = row.depth as f32 * 14.0;
                        let name_rect = egui::Rect::from_min_size(
                            egui::pos2(x + indent, rect.top()),
                            egui::vec2((widths[0] - indent).max(0.0), 28.0),
                        );
                        let name_inset = if row.node.children.is_empty() {
                            0.0
                        } else {
                            // The browser's fallback chevron advances 11px,
                            // followed by one collapsed space. Keep its layout
                            // independent of the narrower native fallback glyph.
                            cell(
                                &p.with_clip_rect(name_rect),
                                egui::Rect::from_min_size(name_rect.min, egui::vec2(11.0, 28.0)),
                                if self.tree.expanded(&row.node.path) { "▾" } else { "▸" },
                                fonts::semibold(12.0),
                                style.palette.text,
                            );
                            18.1875
                        };
                        cell(
                            p,
                            egui::Rect::from_min_size(
                                name_rect.min + egui::vec2(name_inset, 0.0),
                                egui::vec2((name_rect.width() - name_inset).max(0.0), 28.0),
                            ),
                            &row.node.name,
                            fonts::semibold(12.0),
                            style.palette.text,
                        );
                        x += widths[0] + 8.0;
                        let value = row.node.diagnostics.first().map_or_else(
                            || {
                                [row.node.value.clone(), row.node.enum_label.clone()]
                                    .into_iter()
                                    .flatten()
                                    .chain(row.node.flags.clone())
                                    .filter(|s| !s.is_empty())
                                    .collect::<Vec<_>>()
                                    .join(" · ")
                            },
                            |d| d.message.clone(),
                        );
                        let kind = serde_json::to_value(row.node.field_type)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned();
                        let endian = row.node.endianness.map_or("", |e| {
                            if e == hexforge_core::template::Endian::Little {
                                " · LE"
                            } else {
                                " · BE"
                            }
                        });
                        let cells = [
                            value,
                            offset_label(row.node.offset.as_deref()),
                            format!("{} B", row.node.length.as_deref().unwrap_or("—")),
                            format!("{kind}{endian}"),
                        ];
                        // The reference tree item's accessible name includes
                        // its columns, not only the field name. Reuse the exact
                        // decoded cells (including full diagnostic text) while
                        // the stable path remains its identity/value.
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label(format!("{} {}", row.node.name, cells.join(" ")));
                        });
                        for (i, text) in cells.iter().enumerate() {
                            cell(
                                p,
                                egui::Rect::from_min_size(
                                    egui::pos2(x, rect.top()),
                                    egui::vec2(widths[i + 1], 28.0),
                                ),
                                text,
                                fonts::regular(12.0),
                                if i == 0 {
                                    if row.node.diagnostics.is_empty() {
                                        style.palette.text
                                    } else {
                                        style.palette.modified
                                    }
                                } else {
                                    style.palette.address
                                },
                            );
                            x += widths[i + 1] + 8.0;
                        }
                        if response.clicked() {
                            activate = Some(row.node.path.clone());
                            ui.memory_mut(|m| m.request_focus(focus_id));
                        }
                        if !row.node.comment.is_empty() {
                            response.on_hover_text(&row.node.comment);
                        }
                    }
                    if let Some(path) = accessibility_focus {
                        // Native accessibility focus has the same effect as
                        // the source row's focus event, not its click event:
                        // update the keyboard row without selecting any bytes.
                        self.tree.focus(&path);
                        self.focus_requested = true;
                        ui.memory_mut(|m| m.request_focus(focus_id));
                    }
                    if let Some(path) = activate {
                        if let Some(ResultAction::Navigate { start, end }) =
                            self.tree.activate(&path)
                        {
                            events.push(ControllerEvent::Action(Action::NavigateTemplate {
                                start,
                                end,
                            }));
                        }
                    }
                });
                self.scroll = output.state.offset.y;
            },
        );
        // Sticky means fixed vertically only. Share the row tile's actual
        // local X origin, including same-frame wheel motion, then clip the
        // virtual-width heading to its viewport and scrollbar gutter.
        let header_painter = p.with_clip_rect(columns);
        header_painter.rect_filled(columns, 0, style.palette.surface);
        let mut x = column_origin_x + 10.0;
        for (text, width) in ["Path", "Value / Diagnostic", "Offset", "Size", "Type"]
            .into_iter()
            .zip(column_widths(column_grid_width))
        {
            cell(
                &header_painter,
                egui::Rect::from_min_size(egui::pos2(x, columns.top()), egui::vec2(width, 28.0)),
                text,
                fonts::regular(11.0),
                style.palette.muted,
            );
            x += width + 8.0;
        }
        header_painter.hline(
            columns.x_range(),
            columns.bottom() - 0.5,
            egui::Stroke::new(1.0, style.palette.border),
        );
        // The bounded tree keeps one keyboard owner even when its active row
        // is offscreen. Expose the final row identity after key/click handling,
        // independently of the template-owned byte selection.
        ui.ctx().accesskit_node_builder(focus_id, |node| {
            node.set_role(egui::accesskit::Role::Tree);
            node.set_value(self.tree.active_path());
            if let Some((_, id)) = rendered_rows
                .iter()
                .find(|(path, _)| path == self.tree.active_path())
            {
                node.set_active_descendant(id.accesskit_id());
            }
        });
        if let Some((heading, paragraphs, height)) = detail_layout {
            let r =
                egui::Rect::from_min_size(list.left_bottom(), egui::vec2(content.width(), height));
            p.hline(
                r.x_range(),
                r.top(),
                egui::Stroke::new(1.0, style.palette.border),
            );
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(
                    r.min + egui::vec2(0.0, 1.0),
                    r.max,
                )),
                |ui| {
                    // Clip to the actual results area even if the source's
                    // flex:none details exceed a very short resized panel.
                    ui.set_clip_rect(ui.clip_rect().intersect(content));
                    ui.spacing_mut().scroll.bar_width = 6.0;
                    ui.spacing_mut().scroll.bar_inner_margin = 4.0;
                    ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                    ui.spacing_mut().scroll.floating = false;
                    ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
                    crate::widgets::scroll_area::vertical_viewport(
                        ui,
                        style,
                        "result-diagnostic-details",
                        |ui, viewport| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let width = ui.available_width();
                            // Egui rounds the content Ui origin to device
                            // pixels. Preserve the source's continuous local
                            // origin and line advances for actual drawing.
                            let origin = r.min + egui::vec2(0.0, 1.0) - viewport.min.to_vec2();
                            let mut y = origin.y + 8.0;
                            let mut content_height = 16.0;
                            for (index, galley) in
                                std::iter::once(heading).chain(paragraphs).enumerate()
                            {
                                if index > 0 {
                                    y += 5.0;
                                    content_height += 5.0;
                                }
                                let line_height = if index == 0 { 14.0 } else { 16.5 };
                                let advance = galley.rows.len() as f32 * line_height;
                                let rect = egui::Rect::from_min_size(
                                    egui::pos2(origin.x + 12.0, y),
                                    egui::vec2((width - 24.0).max(1.0), advance),
                                );
                                let response = ui.interact(
                                    rect,
                                    ui.id().with(("diagnostic-text", index)),
                                    egui::Sense::hover(),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Label,
                                        ui.is_enabled(),
                                        &galley.job.text,
                                    )
                                });
                                ui.painter()
                                    .galley(rect.min, galley, style.palette.modified);
                                y += advance;
                                content_height += advance;
                            }
                            ui.set_min_height(content_height);
                            ui.set_min_width(width);
                        },
                    );
                },
            );
        }
        events
    }
}
fn column_widths(width: f32) -> [f32; 5] {
    let flexible = (width - 352.0).max(280.0);
    let name = (flexible * 1.2 / 2.9).max(120.0);
    [name, (flexible - name).max(160.0), 130.0, 70.0, 100.0]
}
fn blend(a: Color32, b: Color32, amount: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 * amount + b.r() as f32 * (1.0 - amount)).round() as u8,
        (a.g() as f32 * amount + b.g() as f32 * (1.0 - amount)).round() as u8,
        (a.b() as f32 * amount + b.b() as f32 * (1.0 - amount)).round() as u8,
    )
}
