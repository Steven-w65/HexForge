//! Real native viewport input and rendering. The controller owns file/selection
//! state; this presenter owns only local pixel geometry and captured gestures.
use crate::{
    drawing::{
        cache::HexDrawCache,
        hex::{paint_hex, HexScene},
        minimap::{MinimapOptions, MinimapScene},
    },
    style::{fonts, HexForgeStyle},
    widgets::inert,
};
use eframe::egui::{self, Color32};
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{ControllerEvent, InvokeContext, MainSnapshot, MinimapMode},
    hex::{
        layout::*, minimap_data::*, minimap_geometry::*, minimap_interaction::MinimapInteraction,
        minimap_markers::*, scroll::*, selection::*,
    },
    identity::SourceIdentity,
};
use hexforge_core::{
    error::AppError,
    operations::{CoreReply, CoreRequest},
};
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub struct HexFrame {
    pub top_row: u64,
    pub last_visible_row: Option<u64>,
    pub viewport_rect: egui::Rect,
    pub minimap_rect: Option<egui::Rect>,
    pub viewport_box: Option<egui::Rect>,
    pub minimap_projection: Option<MinimapProjection>,
    pub horizontal_px: f64,
}
impl Default for HexFrame {
    fn default() -> Self {
        Self {
            top_row: 0,
            last_visible_row: None,
            viewport_rect: egui::Rect::NOTHING,
            minimap_rect: None,
            viewport_box: None,
            minimap_projection: None,
            horizontal_px: 0.0,
        }
    }
}
struct HorizontalRepeat {
    region: u8, // left/right arrow, before/after thumb
    next_at: f64,
}
#[derive(Default)]
pub struct HexView {
    frame: HexFrame,
    source: Option<SourceIdentity>,
    row_width: Option<hexforge_controller::controller::RowWidth>,
    mini: Option<MinimapInteraction>,
    data: MinimapDataManager,
    last_data: Option<MinimapDataInput>,
    last_delta: Option<(u64, String)>,
    request: Option<(SourceIdentity, String, ByteReadPlan)>,
    selection: Selection,
    scrollbar: Option<Scrollbar>,
    horizontal_grab: Option<(f64, f64)>,
    horizontal_repeat: Option<HorizontalRepeat>,
    horizontal_px: f64,
    warning: Option<String>,
    pixels_per_point: Option<f32>,
}
impl HexView {
    pub fn frame(&self) -> &HexFrame {
        &self.frame
    }
    pub fn minimap_captured(&self) -> bool {
        self.mini.as_ref().is_some_and(|m| m.captured())
    }
    pub fn next_minimap_request(&mut self) -> Option<(MinimapTicket, CoreRequest)> {
        self.data.next_request()
    }
    pub fn complete_minimap(
        &mut self,
        ticket: MinimapTicket,
        result: Result<CoreReply, AppError>,
    ) -> Option<ControllerEvent> {
        let update = self.data.complete(ticket, result);
        if let Some(error) = update.error {
            // A source change is not an optional preview failure. Preserve the
            // accepted page and report it through the normal, identity-bound
            // app dialog. Stale tickets have already been rejected by data.
            if error.code == hexforge_core::error::ErrorCode::SourceChanged {
                let input = self.last_data.as_ref()?;
                return Some(ControllerEvent::SourceReadFailed {
                    source: input.source,
                    revision: input.revision.clone(),
                    error,
                });
            }
            self.warning = Some(error.message);
        }
        None
    }
    pub fn retry_minimap(&mut self) {
        self.warning = None;
        self.data.retry();
    }
    /// Release the active source tile while retaining the async generation
    /// counter, so a late completion cannot resurrect a closed file's preview.
    pub fn clear(&mut self, cache: &mut HexDrawCache) {
        if let Some(mut input) = self.last_data.take() {
            input.enabled = false;
            self.data.update(input);
        }
        self.mini = None;
        self.source = None;
        self.row_width = None;
        self.horizontal_px = 0.0;
        self.horizontal_grab = None;
        self.horizontal_repeat = None;
        self.selection.handle(SelectionInput::Cancel, 0);
        self.scrollbar = None;
        self.request = None;
        self.last_delta = None;
        self.warning = None;
        self.pixels_per_point = None;
        self.frame = HexFrame::default();
        cache.minimap.clear();
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        s: &MainSnapshot,
        cache: &mut HexDrawCache,
    ) -> Vec<ControllerEvent> {
        self.show_with_scrollbar_tail(ui, s, cache, 0.0)
    }
    /// Only the side track continues through the parent's resize strip. Hex
    /// and minimap row counts, drawing and backing images keep the original
    /// bounded file viewport, not this extra scrollbar height.
    pub(crate) fn show_with_scrollbar_tail(
        &mut self,
        ui: &mut egui::Ui,
        s: &MainSnapshot,
        cache: &mut HexDrawCache,
        scrollbar_tail: f32,
    ) -> Vec<ControllerEvent> {
        let mut events = vec![];
        let (whole, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let style = HexForgeStyle::for_theme(s.theme, ui.ctx().pixels_per_point());
        if s.file.is_none() {
            self.clear(cache);
            self.frame = HexFrame::default();
            ui.painter().rect_filled(whole, 0, style.palette.bg);
            return events;
        }
        let source = s.source.expect("opened file has source identity");
        let file = s.file.as_ref().unwrap();
        let size = file.size.parse::<u64>().unwrap_or(0);
        if self.source != Some(source) {
            self.clear(cache);
            self.source = Some(source);
        }
        let mini_width = if s.minimap.enabled {
            if s.row_width.bytes() == 32 {
                136.0
            } else {
                84.0
            }
        } else {
            0.0
        };
        // The retained flex columns have content-box bases and a separate 1px
        // left border each. Borders must not reduce the preview bitmap width.
        let mini_column_width = mini_width + if s.minimap.enabled { 1.0 } else { 0.0 };
        let grid_width = (whole.width() - mini_column_width - 13.0).max(1.0);
        let layout = HexLayout::new(
            HexMetrics {
                width: grid_width as f64,
                // HexCanvas uses fixed cells, not the font's glyph advance.
                // Drawing and hit testing must share that same 8px geometry.
                char_width: DEFAULT_CHAR_WIDTH,
            },
            s.row_width,
        );
        let overflow = layout.content_width() > grid_width as f64;
        let grid_height = (whole.height()
            - if overflow {
                style.metrics.horizontal_scrollbar
            } else {
                0.0
            })
        .max(1.0);
        let rect = egui::Rect::from_min_size(whole.min, egui::vec2(grid_width, grid_height));
        let total = layout.total_rows(size);
        let visible = layout.fully_visible_rows(grid_height as f64);
        let mut row = clamp_top_row(total, visible, s.viewport_offset / s.row_width.bytes());
        self.horizontal_px = self
            .horizontal_px
            .clamp(0.0, (layout.content_width() - grid_width as f64).max(0.0));
        let mini_rect = s.minimap.enabled.then(|| {
            egui::Rect::from_min_size(
                egui::pos2(rect.right() + 1.0, whole.top()),
                egui::vec2(mini_width, whole.height()),
            )
        });
        let retry_rect = self
            .warning
            .as_ref()
            .and_then(|_| mini_rect.map(minimap_retry_rect));
        let scroll_rect = egui::Rect::from_min_size(
            egui::pos2(whole.right() - 13.0, whole.top()),
            egui::vec2(13.0, whole.height() + scrollbar_tail.max(0.0)),
        );
        let geometry = MinimapGeometry::new(MinimapConfig {
            mode: s.minimap.mode,
            total_rows: total,
            height_px: whole.height().ceil().max(1.0) as u32,
            row_px: u32::from(s.minimap.scale.clamp(1, 3)) * 2,
            top_row: row,
            visible_main_rows: Some(visible),
        })
        .unwrap();
        let mut minimap_layout_changed = false;
        if s.minimap.enabled {
            let replace = self.mini.as_ref().is_none_or(|m| {
                let g = m.geometry();
                g.mode != geometry.mode
                    || g.total_rows != geometry.total_rows
                    || g.row_px != geometry.row_px
            }) || self.row_width != Some(s.row_width);
            if replace {
                self.mini = Some(MinimapInteraction::new(geometry.clone()));
            } else {
                let mini = self.mini.as_mut().unwrap();
                minimap_layout_changed = mini.geometry().height_px != geometry.height_px
                    || mini.geometry().visible_main_rows != visible;
                if minimap_layout_changed {
                    mini.replace_geometry(geometry.clone());
                } else if !mini.captured() {
                    mini.set_editor_row(row);
                }
            }
        } else {
            self.mini = None;
            if let Some(mut i) = self.last_data.take() {
                i.enabled = false;
                self.data.update(i);
            }
            cache.minimap.clear();
        }
        self.row_width = Some(s.row_width);
        let grid_response = ui.interact(
            rect,
            ui.id().with("hex-grid"),
            egui::Sense::click_and_drag(),
        );
        let mini_response = mini_rect
            .map(|r| ui.interact(r, ui.id().with("minimap"), egui::Sense::click_and_drag()));
        let scroll_response = ui.interact(
            scroll_rect,
            ui.id().with("main-scrollbar"),
            egui::Sense::click_and_drag(),
        );
        let dpi = ui.ctx().pixels_per_point();
        let dpi_changed = self.pixels_per_point.is_some_and(|old| old != dpi);
        self.pixels_per_point = Some(dpi);
        let input = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.events.clone(),
                i.raw.focused,
            )
        });
        let (pointer, pressed, down, released, raw_events, focused) = input;
        let input_enabled =
            focused && ui.is_enabled() && s.dialog.is_none() && !s.editor_close_prompt;
        if !focused {
            // A native focus loss may never deliver mouse-up. Cancel only the
            // gesture; the controller's selection and its markers are retained.
            self.selection.handle(SelectionInput::Cancel, size);
            self.scrollbar = None;
            self.horizontal_grab = None;
            self.horizontal_repeat = None;
            if let Some(m) = &mut self.mini {
                m.end_drag();
            }
        } else if (dpi_changed || minimap_layout_changed) && down {
            if let (Some(pos), Some(m)) = (pointer, self.mini.as_mut()) {
                m.rebase_drag((pos.y - whole.top()) as f64);
            }
        }
        if input_enabled {
            if let Some(pos) = pointer {
                // Retry is a separate control above the preview. Native raw
                // pointer dispatch must not let its press start fit navigation
                // or a box drag (the source button stops preview propagation).
                let over_retry = retry_rect.is_some_and(|rect| rect.contains(pos));
                if pressed && !over_retry {
                    if mini_response.as_ref().is_some_and(|r| r.hovered()) {
                        if let Some(m) = &mut self.mini {
                            row = m.pointer_down((pos.y - whole.top()) as f64).editor_row;
                        }
                    } else if scroll_response.hovered() {
                        let mut bar = main_scrollbar(
                            total,
                            visible,
                            grid_height as f64,
                            scroll_rect.height() as f64,
                            row,
                        );
                        bar.begin((pos.y - whole.top()) as f64);
                        row = bar.row;
                        self.scrollbar = Some(bar);
                    } else if grid_response.hovered() {
                        let v = HexViewState {
                            file_size: Some(size),
                            top_row: row,
                            height: grid_height as f64,
                            horizontal_px: self.horizontal_px,
                        };
                        if let Some(offset) = layout.hit_test(
                            LocalPoint {
                                x: (pos.x - rect.left()) as f64,
                                y: (pos.y - rect.top()) as f64,
                            },
                            &v,
                        ) {
                            let update = self.selection.handle(SelectionInput::Begin(offset), size);
                            if let Some(s) = update.selection {
                                events.push(ControllerEvent::Action(Action::Select {
                                    start: s.start,
                                    end: s.end,
                                }));
                            }
                        }
                    }
                }
                if down {
                    if let Some(m) = self.mini.as_mut().filter(|m| m.captured()) {
                        if m.geometry().mode == MinimapMode::Fit
                            && !mini_rect.unwrap().contains(pos)
                        {
                            m.leave();
                        } else {
                            row = m.drag((pos.y - whole.top()) as f64).editor_row;
                        }
                    } else if let Some(bar) = self.scrollbar.as_mut() {
                        row = bar.drag((pos.y - whole.top()) as f64);
                    } else if self.selection.captured() {
                        let v = HexViewState {
                            file_size: Some(size),
                            top_row: row,
                            height: grid_height as f64,
                            horizontal_px: self.horizontal_px,
                        };
                        if let Some(offset) = layout.hit_test(
                            LocalPoint {
                                x: (pos.x - rect.left()) as f64,
                                y: (pos.y - rect.top()) as f64,
                            },
                            &v,
                        ) {
                            let update = self.selection.handle(SelectionInput::Move(offset), size);
                            if let Some(s) = update.selection {
                                events.push(ControllerEvent::Action(Action::Select {
                                    start: s.start,
                                    end: s.end,
                                }));
                            }
                        }
                    }
                }
                if grid_response.double_clicked() && s.edit_mode {
                    events.push(ControllerEvent::Invoke {
                        command: Command::EditSelected,
                        context: InvokeContext::default(),
                    });
                }
                for event in &raw_events {
                    if let egui::Event::MouseWheel { delta, .. } = event {
                        if delta.y == 0.0 {
                            continue;
                        }
                        if mini_response.as_ref().is_some_and(|r| r.hovered()) || over_retry {
                            if let Some(m) = &mut self.mini {
                                row = m.wheel(-delta.y as f64).editor_row;
                            }
                        } else if grid_response.hovered() || scroll_response.hovered() {
                            row =
                                wheel_row(row, total, visible, -delta.y as f64, WheelPolicy::Main);
                            if let Some(m) = &mut self.mini {
                                m.set_editor_row(row);
                            }
                        }
                    }
                }
            }
        }
        if released || !down {
            self.selection.handle(SelectionInput::End, size);
            self.scrollbar = None;
            self.horizontal_grab = None;
            self.horizontal_repeat = None;
            if let Some(m) = &mut self.mini {
                if m.captured() {
                    m.end_drag();
                }
            }
        }
        row = clamp_top_row(total, visible, row);
        let offset = row.saturating_mul(s.row_width.bytes());
        if offset != s.viewport_offset {
            events.push(ControllerEvent::Action(Action::ScrollViewport { offset }));
        }
        if overflow {
            let track = egui::Rect::from_min_max(
                egui::pos2(whole.left(), rect.bottom()),
                egui::pos2(rect.right(), whole.bottom()),
            );
            let response = ui.interact(
                track,
                ui.id().with("hex-horizontal"),
                egui::Sense::click_and_drag(),
            );
            let travel = (layout.content_width() - grid_width as f64).max(0.0);
            // The retained themed thin bar reserves 12px end buttons in a
            // transparent 10px track. Its 2px inset is across the track only;
            // thumb rounding is shared by painting and noncentral hit/drag.
            // All horizontal coordinates are local/bounded, not file offsets.
            let button = 12.0_f64.min(grid_width as f64 / 2.0);
            let track_width = (grid_width as f64 - 2.0 * button).max(0.0);
            let thumb = (track_width * grid_width as f64 / layout.content_width())
                .round()
                .max(11.0)
                .min(track_width);
            let thumb_travel = (track_width - thumb).max(0.0);
            let thumb_top = button + self.horizontal_px / travel * thumb_travel;
            let now = ui.input(|i| i.time);
            if input_enabled {
                if let Some(pos) = pointer {
                    if pressed && response.hovered() {
                        let x = (pos.x - track.left()) as f64;
                        let mut repeat = None;
                        if x < button {
                            self.horizontal_px = (self.horizontal_px - 40.0).max(0.0);
                            repeat = Some(0);
                        } else if x >= grid_width as f64 - button {
                            self.horizontal_px = (self.horizontal_px + 40.0).min(travel);
                            repeat = Some(1);
                        } else if x >= thumb_top && x <= thumb_top + thumb {
                            self.horizontal_grab = Some((x - thumb_top, travel));
                        } else {
                            // Native scrollbar track clicks page; they do not
                            // turn into a recentered thumb drag.
                            let page = (grid_width as f64 * 0.875).floor();
                            self.horizontal_px = if x < thumb_top {
                                repeat = Some(2);
                                (self.horizontal_px - page).max(0.0)
                            } else {
                                repeat = Some(3);
                                (self.horizontal_px + page).min(travel)
                            };
                        }
                        self.horizontal_repeat = repeat.map(|region| HorizontalRepeat {
                            region,
                            next_at: now + 0.25,
                        });
                    }
                    if down {
                        if let Some((grab, _)) = self.horizontal_grab {
                            self.horizontal_px = if thumb_travel > 0.0 {
                                ((pos.x - track.left()) as f64 - button - grab)
                                    .clamp(0.0, thumb_travel)
                                    / thumb_travel
                                    * travel
                            } else {
                                0.0
                            };
                        }
                        if let Some(repeat) = &mut self.horizontal_repeat {
                            // Hold timers wake only this bounded presenter;
                            // they never advance rows or wait on core locks.
                            if now >= repeat.next_at {
                                let x = (pos.x - track.left()) as f64;
                                let top = button + self.horizontal_px / travel * thumb_travel;
                                let step = match repeat.region {
                                    0 if track.contains(pos) && x < button => -40.0,
                                    1 if track.contains(pos) && x >= grid_width as f64 - button => {
                                        40.0
                                    }
                                    2 if track.contains(pos) && x >= button && x < top => {
                                        -(grid_width as f64 * 0.875).floor()
                                    }
                                    3 if track.contains(pos)
                                        && x < grid_width as f64 - button
                                        && x > top + thumb =>
                                    {
                                        (grid_width as f64 * 0.875).floor()
                                    }
                                    _ => 0.0,
                                };
                                self.horizontal_px = (self.horizontal_px + step).clamp(0.0, travel);
                                repeat.next_at = now + 0.05;
                            }
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_secs_f64(
                                    (repeat.next_at - now).max(0.0),
                                ));
                        }
                    }
                }
            } else {
                self.horizontal_grab = None;
                self.horizontal_repeat = None;
            }
            let pointer_x = pointer.map(|p| (p.x - track.left()) as f64);
            let hovered = input_enabled && response.hovered();
            let drawn_top = button + self.horizontal_px / travel * thumb_travel;
            let thumb_hover = self.horizontal_grab.is_some()
                || (hovered && pointer_x.is_some_and(|x| x >= drawn_top && x <= drawn_top + thumb));
            ui.painter().rect_filled(track, 0, style.palette.bg);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    track.min
                        + egui::vec2(
                            (button + self.horizontal_px / travel * thumb_travel) as f32,
                            2.0,
                        ),
                    egui::vec2(thumb as f32, 6.0),
                ),
                3,
                if thumb_hover {
                    style.palette.scrollbar_hover
                } else {
                    style.palette.border_strong
                },
            );
            if button >= 8.0 {
                for (x, direction) in [(track.left() + 6.0, -1.0), (track.right() - 6.0, 1.0)] {
                    let arrow_hover = hovered
                        && pointer_x.is_some_and(|x| {
                            if direction < 0.0 {
                                x < button
                            } else {
                                x >= grid_width as f64 - button
                            }
                        });
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(x - direction * 2.0, track.top() + 2.0),
                            egui::pos2(x + direction * 2.0, track.top() + 5.0),
                            egui::pos2(x - direction * 2.0, track.top() + 8.0),
                        ],
                        if arrow_hover {
                            style.palette.scrollbar_hover
                        } else {
                            style.palette.border_strong
                        },
                        egui::Stroke::NONE,
                    ));
                }
            }
        }
        paint_hex(
            ui.painter(),
            &HexScene {
                rect,
                layout: &layout,
                snapshot: s,
                top_row: row,
                horizontal_px: self.horizontal_px,
            },
            &style,
        );
        let v = HexViewState {
            file_size: Some(size),
            top_row: row,
            height: grid_height as f64,
            horizontal_px: self.horizontal_px,
        };
        if let Some(plan) = visible_range(&layout, &v, (grid_height as f64 / 5.0).ceil()) {
            let needed_start = u128::from(offset);
            let needed_end = (needed_start
                + u128::from(layout.visible_rows(grid_height as f64))
                    * u128::from(s.row_width.bytes()))
            .min(u128::from(size));
            let covers = s.page.as_ref().is_some_and(|p| {
                p.revision == file.revision
                    && p.offset.parse::<u64>().ok().is_some_and(|start| {
                        u128::from(start) <= needed_start
                            && u128::from(start) + p.bytes.len() as u128 >= needed_end
                    })
            });
            let key = (source, file.revision.clone(), plan.clone());
            if covers {
                self.request = None;
            } else if self.request.as_ref() != Some(&key) {
                events.push(ControllerEvent::Action(Action::ReadViewport {
                    offset: plan.byte_start,
                    length: plan.byte_length,
                }));
                self.request = Some(key);
            }
        }
        let ranges = s
            .template_range
            .as_ref()
            .filter(|r| contains_parsed_leaf_range(&s.results, r.start, r.end))
            .map(|r| vec![(r.start, r.count().min(u128::from(u64::MAX)) as u64)])
            .unwrap_or_default();
        let markers = collect_markers(
            &s.matches,
            s.search.match_length,
            &ranges,
            &s.modified_overview,
            size,
            s.row_width,
        );
        let mut box_rect = None;
        let mut projection = None;
        if let (Some(mini_rect), Some(m)) = (mini_rect, self.mini.as_ref()) {
            let g = m.geometry().clone();
            let p = m.projection();
            let viewport = viewport_box(&g, &p, row, row.saturating_add(visible));
            if self.last_delta != s.last_edit_delta {
                if let Some((at, rev)) = &s.last_edit_delta {
                    self.data.apply_edit_delta(*at, rev);
                }
                self.last_delta = s.last_edit_delta.clone();
            }
            let accepted = s
                .page
                .as_ref()
                .filter(|p| p.revision == file.revision)
                .map(|p| p.data.clone());
            let input = MinimapDataInput {
                enabled: true,
                source,
                source_key: file.path.clone(),
                revision: file.revision.clone(),
                file_size: size,
                row_width: s.row_width,
                geometry: g.clone(),
                projection: Some(p.clone()),
                accepted_page: accepted,
            };
            self.data.update(input.clone());
            self.last_data = Some(input);
            let rows: BTreeMap<_, _> = sample_plan(&g, &p)
                .iter()
                .filter_map(|pos| self.data.get_row(pos.row).map(|r| (r.row, r.clone())))
                .collect();
            ui.painter().rect_filled(mini_rect, 0, style.palette.panel);
            let mini_painter = ui.painter().with_clip_rect(mini_rect);
            cache.minimap.paint(
                &mini_painter,
                mini_rect,
                &MinimapScene {
                    geometry: &g,
                    projection: &p,
                    rows: &rows,
                    width_px: mini_width as u32,
                    dpr: ui.ctx().pixels_per_point() as f64,
                    options: MinimapOptions {
                        row_width: s.row_width,
                        render: s.minimap.render,
                        theme: s.theme,
                    },
                },
            );
            for run in project_minimap(&markers, &g, &p) {
                paint_marker(&mini_painter, mini_rect, run, false);
            }
            let painted_box = viewport.and_then(|b| {
                let residual = if g.mode == MinimapMode::Proportional {
                    // p.box_top_sub is container-bounded; this restores only
                    // the pointer residual, not a large logical canvas offset.
                    b.top - p.box_top_sub.0 as f64 / SUBPIXELS as f64
                } else {
                    0.0
                };
                crate::drawing::minimap_overlay::viewport_rect(
                    mini_rect,
                    b.canvas_top,
                    canvas_offset_y(&g, &p),
                    b.height,
                    residual,
                )
            });
            if let Some(box_r) = painted_box {
                box_rect = Some(box_r);
                let hovered = pointer.is_some_and(|pos| box_r.contains(pos));
                let active = m.captured() && g.mode == MinimapMode::Proportional;
                crate::drawing::minimap_overlay::paint_viewport(
                    &mini_painter,
                    mini_rect,
                    box_r,
                    s.theme,
                    hovered,
                    active,
                );
                if mini_response.as_ref().is_some_and(|r| r.hovered()) {
                    ui.ctx().set_cursor_icon(if active {
                        egui::CursorIcon::Grabbing
                    } else if hovered {
                        egui::CursorIcon::Grab
                    } else {
                        egui::CursorIcon::PointingHand
                    });
                }
            }
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(mini_rect.left() - 1.0, mini_rect.top()),
                    egui::pos2(mini_rect.left(), mini_rect.bottom()),
                ),
                0,
                style.palette.border,
            );
            projection = Some(p);
            if let Some(warning) = self.warning.clone() {
                if show_minimap_retry(ui, minimap_retry_rect(mini_rect), &style, &warning) {
                    self.retry_minimap();
                }
            }
        }
        let scrollbar = main_scrollbar(
            total,
            visible,
            grid_height as f64,
            scroll_rect.height() as f64,
            row,
        );
        let color = style.palette.text;
        paint_main_scrollbar_track(ui.painter(), scroll_rect, &style);
        let scroll_content =
            egui::Rect::from_min_max(scroll_rect.min + egui::vec2(1.0, 0.0), scroll_rect.max);
        let overview_runs = project_overview(&markers, total, scroll_rect.height().ceil() as u32);
        // The retained ruler groups DOM elements as search, template, modified.
        // Its CSS minimum of 2px can overlap adjacent logical 1px runs, so keep
        // that drawing order independently of the exact projection dataset.
        for kind in [
            MarkerKind::Search,
            MarkerKind::Template,
            MarkerKind::Modified,
        ] {
            for run in overview_runs.iter().filter(|run| run.kind == kind) {
                paint_marker(ui.painter(), scroll_content, run.clone(), true);
            }
        }
        let thumb = egui::Rect::from_min_size(
            scroll_content.min + egui::vec2(1.0, scrollbar.top() as f32),
            // Source CSS sets a content-box height with two 1px borders.
            // Keep scrollbar.thumb_height unchanged for exact hit/travel math.
            egui::vec2(10.0, scrollbar.thumb_height as f32 + 2.0),
        );
        let thumb_painter = ui.painter().with_clip_rect(scroll_content);
        thumb_painter.rect_filled(thumb, 2, color.gamma_multiply(0.17));
        thumb_painter.rect_stroke(
            thumb,
            2,
            egui::Stroke::new(1.0, color.gamma_multiply(0.38)),
            egui::StrokeKind::Inside,
        );
        self.frame = HexFrame {
            top_row: row,
            last_visible_row: (size > 0).then(|| {
                row.saturating_add(layout.visible_rows(grid_height as f64) - 1)
                    .min(total - 1)
            }),
            viewport_rect: rect,
            minimap_rect: mini_rect,
            viewport_box: box_rect,
            minimap_projection: projection,
            horizontal_px: self.horizontal_px,
        };
        events
    }
}

/// A horizontal scrollbar shortens the hex content, not the side track. Use
/// one model for thumb paint and pointer capture so EOF and noncentral grabs
/// cannot disagree. Long-file thumb density retains the hex content height;
/// a file fitting on one page fills the track instead of leaving a dead tail.
fn main_scrollbar(
    total: u64,
    visible: u64,
    content_height: f64,
    track_height: f64,
    row: u64,
) -> Scrollbar {
    let mut bar = Scrollbar::new(
        total,
        visible,
        if total <= visible.max(1) {
            track_height
        } else {
            content_height
        },
        row,
    );
    bar.height = track_height;
    bar
}

/// Paint the complete track used for pointer input, including its left border.
/// The parent can extend it through the resize strip without extending any
/// file rows or the minimap's bounded backing image.
fn paint_main_scrollbar_track(painter: &egui::Painter, rect: egui::Rect, style: &HexForgeStyle) {
    painter.rect_filled(rect, 0, style.palette.text.gamma_multiply(0.08));
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(1.0, rect.height())),
        0,
        style.palette.border,
    );
}

fn minimap_retry_rect(minimap: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        minimap.min + egui::vec2(minimap.width() - 30.0, 6.0),
        egui::vec2(24.0, 24.0),
    )
}
/// HexCanvas's compact retry is a 24px panel-colored bordered control, not a
/// normal footer button. Share exactly one rectangle for paint and hit testing.
fn show_minimap_retry(
    ui: &egui::Ui,
    rect: egui::Rect,
    style: &HexForgeStyle,
    warning: &str,
) -> bool {
    let response = ui.interact(rect, ui.id().with("minimap-retry"), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Retry minimap preview",
        )
    });
    let opacity = if inert::visually_enabled(ui) {
        1.0
    } else {
        0.4
    };
    let fill = if response.hovered() || response.is_pointer_button_down_on() {
        style.palette.hover
    } else {
        style.palette.panel
    };
    ui.painter()
        .rect_filled(rect, 4, fill.gamma_multiply(opacity));
    ui.painter().rect_stroke(
        rect,
        4,
        egui::Stroke::new(1.0, style.palette.border_strong.gamma_multiply(opacity)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "↻",
        fonts::regular(style.metrics.font_body),
        style.palette.text.gamma_multiply(opacity),
    );
    if response.has_focus() && crate::platform::input::focus_visible(ui.ctx()) {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            4,
            egui::Stroke::new(1.0, style.palette.selection),
            egui::StrokeKind::Outside,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{warning} Click to retry."))
        .clicked()
}
/// Like AppShell's minimapFields, only an exact successful result leaf owns a
/// parsed marker. The grid's selection and unsaved edit/search layers stay
/// independent. Compare integer ranges before projecting nearby pixels.
fn contains_parsed_leaf_range(
    nodes: &[hexforge_core::template::ParsedNode],
    start: u64,
    end: u64,
) -> bool {
    nodes.iter().any(|node| {
        hexforge_controller::results::navigable_range(node) == Some((start, end))
            || contains_parsed_leaf_range(&node.children, start, end)
    })
}

fn paint_marker(p: &egui::Painter, rect: egui::Rect, run: PixelRun, overview: bool) {
    let (r, g, b, a) = match run.kind {
        MarkerKind::Search => (189, 166, 74, 166),
        MarkerKind::Template => (57, 197, 207, if overview { 140 } else { 166 }),
        MarkerKind::Modified => (240, 136, 62, 230),
    };
    p.with_clip_rect(rect).rect_filled(
        egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, run.top as f32),
            egui::vec2(
                rect.width(),
                if overview {
                    run.height.max(2)
                } else {
                    run.height
                } as f32,
            ),
        ),
        if overview { 1 } else { 0 },
        Color32::from_rgba_unmultiplied(r, g, b, a),
    );
}
