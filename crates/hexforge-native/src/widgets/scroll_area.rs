//! Source-styled thin scrollbars over egui's bounded content/wheel/reveal area.
//! File/minimap coordinates are deliberately not handled by this pixel widget.
use crate::style::HexForgeStyle;
use eframe::egui::{
    self,
    scroll_area::{ScrollAreaOutput, ScrollBarVisibility, ScrollSource},
};

#[derive(Clone, Default)]
struct BarState {
    overflow: Option<bool>,
    grab: Option<f32>,
    repeat: Option<(u8, f64)>,
}

pub fn vertical<R>(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    salt: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> ScrollAreaOutput<R> {
    vertical_viewport(ui, style, salt, |ui, _viewport| contents(ui))
}

/// A source-style bar with the unsnapped local scroll viewport. Custom drawing
/// can retain fractional CSS coordinates instead of inheriting egui's pixel-
/// rounded content Ui origin. Input ownership is identical to `vertical`.
pub fn vertical_viewport<R>(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    salt: &str,
    contents: impl FnOnce(&mut egui::Ui, egui::Rect) -> R,
) -> ScrollAreaOutput<R> {
    viewport(ui, style, salt, false, 0.0, None, None, contents)
}

/// A CSS overflow container scrolls its padding with its rows. Keep its thin
/// bar over the entire padded viewport, while painting only intersecting rows.
pub fn padded_menu_rows<R>(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    salt: &str,
    row_height: f32,
    total_rows: usize,
    contents: impl FnOnce(&mut egui::Ui, std::ops::Range<usize>) -> R,
) -> ScrollAreaOutput<R> {
    viewport(
        ui,
        style,
        salt,
        false,
        0.0,
        None,
        Some((egui::Margin::same(4), style.palette.panel)),
        |ui, visible| {
            // Same bounded row window/stable auto-IDs as egui show_rows; this
            // outer source scrollbar alone owns pointer input and wheel work.
            ui.set_height(row_height * total_rows as f32);
            let mut start = (visible.min.y / row_height).floor() as usize;
            let mut end = (visible.max.y / row_height).ceil() as usize + 1;
            if end > total_rows {
                let count = end.saturating_sub(start);
                end = total_rows;
                start = total_rows.saturating_sub(count);
            }
            let rows = egui::Rect::from_x_y_ranges(
                ui.max_rect().x_range(),
                ui.max_rect().top() + start as f32 * row_height
                    ..=ui.max_rect().top() + end as f32 * row_height,
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(rows), |ui| {
                ui.skip_ahead_auto_ids(start);
                contents(ui, start..end)
            })
            .inner
        },
    )
}

/// A result list's sticky heading is outside its row clip, but participates in
/// the same CSS scroll container. It contributes to track/content height, not
/// to row travel. Both axes retain one wheel owner and source-style controls.
pub fn both_viewport_with_header<R>(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    salt: &str,
    header_height: f32,
    offset: f32,
    contents: impl FnOnce(&mut egui::Ui, egui::Rect) -> R,
) -> ScrollAreaOutput<R> {
    viewport(
        ui,
        style,
        salt,
        true,
        header_height,
        Some(offset),
        None,
        contents,
    )
}

fn viewport<R>(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    salt: &str,
    horizontal: bool,
    header_height: f32,
    offset: Option<f32>,
    padded_frame: Option<(egui::Margin, egui::Color32)>,
    contents: impl FnOnce(&mut egui::Ui, egui::Rect) -> R,
) -> ScrollAreaOutput<R> {
    let id = ui.make_persistent_id(("source-thin-scrollbar", salt));
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<BarState>(id))
        .unwrap_or_default();
    let outer = ui.available_rect_before_wrap();
    // The built-in bar reserves the existing 10px slot, but owns no pointer
    // input. Only this source-style control may interpret that input. Wheel
    // and scroll-to/reveal remain with the original ScrollArea exactly once.
    let mut area = egui::ScrollArea::new([horizontal, true])
        .id_salt(salt)
        .auto_shrink([false, false])
        .scroll_source(ScrollSource {
            scroll_bar: false,
            ..Default::default()
        })
        .scroll_bar_visibility(match (horizontal, state.overflow) {
            (true, _) => ScrollBarVisibility::VisibleWhenNeeded,
            (false, Some(true)) => ScrollBarVisibility::AlwaysVisible,
            (false, Some(false)) => ScrollBarVisibility::AlwaysHidden,
            (false, None) => ScrollBarVisibility::VisibleWhenNeeded,
        });
    if let Some(offset) = offset {
        area = area.vertical_scroll_offset(offset);
    }
    if let Some((margin, _)) = padded_frame {
        area = area.content_margin(margin);
    }
    let mut output = area.show_viewport(ui, contents);
    if horizontal {
        let horizontal_id = id.with("horizontal");
        let horizontal_state = ui
            .ctx()
            .data_mut(|d| d.get_temp::<BarState>(horizontal_id))
            .unwrap_or_default();
        let track = egui::Rect::from_min_max(
            egui::pos2(output.inner_rect.left(), outer.bottom() - 10.0),
            egui::pos2(output.inner_rect.right(), outer.bottom()),
        );
        paint_bar(
            ui,
            style,
            horizontal_id,
            horizontal_state,
            &output,
            track,
            output.content_size.x,
            true,
            style.palette.panel,
        );
        if let Some(applied) = egui::scroll_area::State::load(ui.ctx(), output.id) {
            output.state = applied;
        }
    }
    let height = output.inner_rect.height() + header_height;
    let content_height = output.content_size.y + header_height;
    let overflow = content_height > height;
    if state.overflow != Some(overflow) {
        ui.ctx().request_repaint();
    }
    state.overflow = Some(overflow);
    if !overflow {
        state.grab = None;
        state.repeat = None;
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return output;
    }
    let track = egui::Rect::from_min_max(
        egui::pos2(
            outer.right() - 10.0,
            output.inner_rect.top() - header_height,
        ),
        egui::pos2(outer.right(), output.inner_rect.bottom()),
    );
    paint_bar(
        ui,
        style,
        id,
        state,
        &output,
        track,
        content_height,
        false,
        if let Some((_, background)) = padded_frame {
            background
        } else if horizontal {
            style.palette.panel
        } else {
            style.palette.bg
        },
    );
    // Return the post-input state. A presenter may seed the following frame
    // from this offset; returning the pre-arrow/pre-drag state would undo that
    // accepted scroll on the next frame, even though the widget stored it.
    if let Some(applied) = egui::scroll_area::State::load(ui.ctx(), output.id) {
        output.state = applied;
    }
    output
}

fn paint_bar<R>(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    id: egui::Id,
    mut state: BarState,
    output: &ScrollAreaOutput<R>,
    track: egui::Rect,
    content_height: f32,
    horizontal: bool,
    background: egui::Color32,
) {
    let height = if horizontal {
        track.width()
    } else {
        track.height()
    };
    if content_height <= height {
        state.grab = None;
        state.repeat = None;
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return;
    }
    let response = ui.interact(track, id, egui::Sense::click_and_drag());
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_role(egui::accesskit::Role::ScrollBar);
        node.set_label(if horizontal {
            "Horizontal scroll bar"
        } else {
            "Vertical scroll bar"
        });
        node.set_bounds(egui::accesskit::Rect {
            x0: track.left() as f64,
            y0: track.top() as f64,
            x1: track.right() as f64,
            y1: track.bottom() as f64,
        });
        if !ui.is_enabled() {
            node.set_disabled();
        }
    });
    // The source's themed thin bar is 10px across, but its end buttons are
    // 12px along the scrolling axis. Thumb rounding and a cross-axis-only
    // inset must also be shared by paint and hit testing; using square 10px
    // buttons plus axial insets happened to put the top edge at the same
    // pixel while shortening the thumb and changing noncentral drag travel.
    let button = 12.0_f32.min(height / 2.0);
    let axis = (height - 2.0 * button).max(0.0);
    let thumb = (axis * height / content_height).round().max(11.0).min(axis);
    let travel = (axis - thumb).max(0.0);
    let maximum = content_height - height;
    let coordinate = |point: egui::Pos2| {
        if horizontal {
            point.x - track.left()
        } else {
            point.y - track.top()
        }
    };
    let current_offset = if horizontal {
        output.state.offset.x
    } else {
        output.state.offset.y
    };
    let mut offset = current_offset.clamp(0.0, maximum);
    let (pointer, pressed, down, released, focused, now) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.focused,
            i.time,
        )
    });
    if !ui.is_enabled() || !focused || released || !down {
        state.grab = None;
        state.repeat = None;
    }
    let top = button + offset / maximum * travel;
    if ui.is_enabled() && focused {
        if let Some(pos) = pointer {
            let y = coordinate(pos);
            if pressed && response.hovered() {
                if y >= top && y <= top + thumb {
                    state.grab = Some(y - top);
                } else {
                    let part = if y < button {
                        0
                    } else if y >= height - button {
                        1
                    } else if y < top {
                        2
                    } else {
                        3
                    };
                    offset = (offset + step(part, height)).clamp(0.0, maximum);
                    state.repeat = Some((part, now + 0.25));
                }
            }
            if down {
                if let Some(grab) = state.grab {
                    // Keep the exact local pointer grab offset even outside
                    // the area; clamp at its ends, rather than terminating.
                    offset = if travel > 0.0 {
                        (y - button - grab).clamp(0.0, travel) / travel * maximum
                    } else {
                        0.0
                    };
                }
                if let Some((part, next)) = &mut state.repeat {
                    if now >= *next {
                        let top = button + offset / maximum * travel;
                        let inside = track.contains(pos)
                            && match *part {
                                0 => y < button,
                                1 => y >= height - button,
                                2 => y >= button && y < top,
                                _ => y < height - button && y > top + thumb,
                            };
                        if inside {
                            offset = (offset + step(*part, height)).clamp(0.0, maximum);
                        }
                        *next = now + 0.05;
                    }
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs_f64(
                            (*next - now).max(0.0),
                        ));
                }
            }
        }
    }
    if offset != current_offset {
        // An explicit scrollbar action cancels prior reveal/kinetic motion.
        // Rebuild only ScrollArea's transient mechanics, retaining its exact
        // bounded offset and our overflow allocation; no workspace is changed.
        let mut scroll = egui::scroll_area::State::default();
        scroll.offset = if horizontal {
            egui::vec2(offset, output.state.offset.y)
        } else {
            egui::vec2(output.state.offset.x, offset)
        };
        scroll.store(ui.ctx(), output.id);
        ui.ctx().request_repaint();
    }
    let top = button + offset / maximum * travel;
    let hovered = ui.is_enabled() && response.hovered();
    let y = pointer.map(coordinate);
    let thumb_hover =
        state.grab.is_some() || (hovered && y.is_some_and(|y| y >= top && y <= top + thumb));
    ui.painter().rect_filled(track, 0, background);
    ui.painter().rect_filled(
        egui::Rect::from_min_size(
            track.min
                + if horizontal {
                    egui::vec2(top, 2.0)
                } else {
                    egui::vec2(2.0, top)
                },
            if horizontal {
                egui::vec2(thumb, 6.0)
            } else {
                egui::vec2(6.0, thumb)
            },
        ),
        3,
        if thumb_hover {
            style.palette.scrollbar_hover
        } else {
            style.palette.border_strong
        },
    );
    if button >= 8.0 {
        for (cy, direction) in [(6.0, -1.0), (height - 6.0, 1.0)] {
            let arrow_hover = hovered
                && y.is_some_and(|y| {
                    if direction < 0.0 {
                        y < button
                    } else {
                        y >= height - button
                    }
                });
            ui.painter().add(egui::Shape::convex_polygon(
                [
                    (2.0, cy - direction * 2.0),
                    (5.0, cy + direction * 2.0),
                    (8.0, cy - direction * 2.0),
                ]
                .map(|(cross, axis)| {
                    track.min
                        + if horizontal {
                            egui::vec2(axis, cross)
                        } else {
                            egui::vec2(cross, axis)
                        }
                })
                .to_vec(),
                if arrow_hover {
                    style.palette.scrollbar_hover
                } else {
                    style.palette.border_strong
                },
                egui::Stroke::NONE,
            ));
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}

fn step(part: u8, height: f32) -> f32 {
    match part {
        0 => -40.0,
        1 => 40.0,
        2 => -(height * 0.875).floor(),
        _ => (height * 0.875).floor(),
    }
}
