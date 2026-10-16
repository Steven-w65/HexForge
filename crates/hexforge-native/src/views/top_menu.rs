//! Application menus own their navigation keys; modified keys continue to the
//! shared window shortcut policy. Every item uses that same command policy.
use crate::{
    style::{fonts, HexForgeStyle},
    widgets::{
        button::{ButtonRole, ButtonSpec, StyledButton},
        menu, scroll_area,
    },
};
use eframe::egui::{self, Align2};
use hexforge_controller::{
    action::Action,
    command::Command,
    controller::{
        ControllerEvent, InvokeContext, MainSnapshot, MinimapMode, MinimapRender, RowWidth,
    },
    policy::command_unavailable_reason,
    results::{navigable_range, ResultTree},
    shortcuts::{shortcut_label, WindowKind},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuId {
    File,
    Edit,
    Navigate,
    Template,
    View,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Submenu {
    Results,
    Minimap,
}
#[derive(Clone)]
enum Entry {
    Command(Command),
    Branch(Submenu),
    Leaf(String, u64, u64),
    Separator,
}
const ORDER: [MenuId; 5] = [
    MenuId::File,
    MenuId::Edit,
    MenuId::Navigate,
    MenuId::Template,
    MenuId::View,
];
fn entries(id: MenuId) -> Vec<Entry> {
    use Command::*;
    let commands: &[Command] = match id {
        MenuId::File => &[Open, CloseFile, SaveAs, Export, Exit],
        MenuId::Edit => &[EditSelected, ToggleEdit, Undo],
        MenuId::Navigate => &[Goto, Search, SearchNext, SearchPrevious],
        MenuId::Template => &[
            TemplateEditor,
            ApplyTemplate,
            LoadTemplate,
            UnloadTemplate,
            SaveTemplate,
            SaveTemplateAs,
        ],
        MenuId::View => &[ThemeToggle, Row16, Row32],
    };
    let mut entries: Vec<_> = commands.iter().copied().map(Entry::Command).collect();
    if matches!(id, MenuId::Navigate | MenuId::View) {
        entries.push(Entry::Separator);
        entries.push(Entry::Branch(if id == MenuId::View {
            Submenu::Minimap
        } else {
            Submenu::Results
        }));
    }
    entries
}
fn submenu_entries(sub: Submenu, s: &MainSnapshot) -> Vec<Entry> {
    if sub == Submenu::Minimap {
        use Command::*;
        [
            MinimapToggle,
            MinimapFit,
            MinimapProportional,
            MinimapCharacters,
            MinimapBlocks,
            MinimapScale1,
            MinimapScale2,
            MinimapScale3,
        ]
        .into_iter()
        .map(Entry::Command)
        .collect()
    } else {
        let mut tree = ResultTree::default();
        tree.replace(s.results.clone());
        tree.navigable_leaves()
            .into_iter()
            .filter_map(|n| {
                navigable_range(n).map(|(start, end)| Entry::Leaf(n.path.clone(), start, end))
            })
            .collect()
    }
}
fn reason(e: &Entry, s: &MainSnapshot) -> Option<String> {
    match e {
        Entry::Command(c) => command_unavailable_reason(*c, &s.policy),
        Entry::Branch(Submenu::Results)
            if s.file.is_none() || submenu_entries(Submenu::Results, s).is_empty() =>
        {
            Some("No parsed results to navigate.".into())
        }
        Entry::Separator => Some(String::new()),
        _ => None,
    }
}
fn checked(c: Command, s: &MainSnapshot) -> Option<bool> {
    use Command::*;
    Some(match c {
        ToggleEdit => s.edit_mode,
        Row16 => s.row_width == RowWidth::Sixteen,
        Row32 => s.row_width == RowWidth::ThirtyTwo,
        MinimapToggle => s.minimap.enabled,
        MinimapFit => s.minimap.mode == MinimapMode::Fit,
        MinimapProportional => s.minimap.mode == MinimapMode::Proportional,
        MinimapCharacters => s.minimap.render == MinimapRender::Characters,
        MinimapBlocks => s.minimap.render == MinimapRender::Blocks,
        MinimapScale1 => s.minimap.scale == 1,
        MinimapScale2 => s.minimap.scale == 2,
        MinimapScale3 => s.minimap.scale == 3,
        _ => return None,
    })
}
pub struct MenuView {
    open: Option<MenuId>,
    submenu: Option<Submenu>,
    anchor: egui::Rect,
    submenu_anchor: egui::Rect,
    focus: Option<usize>,
    request_focus: bool,
    popup: Option<egui::Rect>,
    flyout: Option<egui::Rect>,
}
impl Default for MenuView {
    fn default() -> Self {
        Self {
            open: None,
            submenu: None,
            anchor: egui::Rect::NOTHING,
            submenu_anchor: egui::Rect::NOTHING,
            focus: None,
            request_focus: false,
            popup: None,
            flyout: None,
        }
    }
}
impl MenuView {
    fn close(&mut self) {
        self.open = None;
        self.submenu = None;
        self.focus = None;
        self.popup = None;
        self.flyout = None;
    }
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
    fn activate(&mut self, e: Entry, events: &mut Vec<ControllerEvent>) {
        match e {
            Entry::Command(command) => {
                events.push(ControllerEvent::Invoke {
                    command,
                    context: InvokeContext::default(),
                });
                self.close();
            }
            Entry::Leaf(_, start, end) => {
                events.push(ControllerEvent::Action(Action::NavigateTemplate {
                    start,
                    end,
                }));
                self.close();
            }
            Entry::Branch(sub) => {
                self.submenu = Some(sub);
                self.focus = Some(0);
                self.request_focus = true;
            }
            Entry::Separator => {}
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        style: &HexForgeStyle,
        s: &MainSnapshot,
    ) -> Vec<ControllerEvent> {
        let mut events = vec![];
        let ctx = ui.ctx().clone();
        let whole = ui.max_rect();
        let modal = s.dialog.is_some()
            || s.edit_dialog.is_some()
            || s.error.is_some()
            || s.editor_close_prompt;
        let mods = ctx.input(|i| i.modifiers);
        let composing = crate::platform::input::keyboard_owned_by_ime(
            &ctx,
            hexforge_controller::shortcuts::WindowKind::Main,
        );
        if !modal && !composing {
            if mods.alt && !mods.ctrl && !mods.shift && !mods.mac_cmd {
                for (key, id) in [
                    (egui::Key::F, MenuId::File),
                    (egui::Key::E, MenuId::Edit),
                    (egui::Key::N, MenuId::Navigate),
                    (egui::Key::T, MenuId::Template),
                    (egui::Key::V, MenuId::View),
                ] {
                    if ctx.input_mut(|i| i.consume_key(mods, key)) {
                        self.open = Some(id);
                        self.submenu = None;
                        self.focus = entries(id).iter().position(|e| reason(e, s).is_none());
                        self.request_focus = true;
                    }
                }
            }
            if let Some(id) = self.open {
                if mods.is_none() {
                    let options = self
                        .submenu
                        .map_or_else(|| entries(id), |sub| submenu_entries(sub, s));
                    for (key, delta) in [(egui::Key::ArrowDown, 1isize), (egui::Key::ArrowUp, -1)] {
                        if consume_navigation_key(&ctx, key) && !options.is_empty() {
                            let enabled: Vec<_> = options
                                .iter()
                                .enumerate()
                                .filter(|(_, e)| reason(e, s).is_none())
                                .map(|(i, _)| i)
                                .collect();
                            if !enabled.is_empty() {
                                let current = self
                                    .focus
                                    .and_then(|f| enabled.iter().position(|&e| e == f));
                                let next = current.map_or(
                                    if delta > 0 { 0 } else { enabled.len() - 1 },
                                    |i| {
                                        (i as isize + delta).rem_euclid(enabled.len() as isize)
                                            as usize
                                    },
                                );
                                self.focus = Some(enabled[next]);
                                self.request_focus = true;
                            }
                        }
                    }
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        self.close();
                    } else if ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            || i.consume_key(egui::Modifiers::NONE, egui::Key::Space)
                    }) {
                        if let Some(e) = self
                            .focus
                            .and_then(|i| options.get(i))
                            .filter(|e| reason(e, s).is_none())
                        {
                            self.activate(e.clone(), &mut events);
                        }
                    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::R))
                        && id == MenuId::Navigate
                        && !submenu_entries(Submenu::Results, s).is_empty()
                    {
                        self.submenu = Some(Submenu::Results);
                        self.focus = Some(0);
                        self.request_focus = true;
                    } else {
                        for (key, delta) in
                            [(egui::Key::ArrowRight, 1isize), (egui::Key::ArrowLeft, -1)]
                        {
                            if consume_navigation_key(&ctx, key) {
                                if delta < 0 && self.submenu.is_some() {
                                    self.submenu = None;
                                    self.focus = entries(id)
                                        .iter()
                                        .position(|e| matches!(e, Entry::Branch(_)));
                                } else if delta > 0
                                    && self.submenu.is_none()
                                    && self
                                        .focus
                                        .and_then(|i| options.get(i))
                                        .is_some_and(|e| matches!(e, Entry::Branch(_)))
                                {
                                    self.activate(
                                        options[self.focus.unwrap()].clone(),
                                        &mut events,
                                    );
                                } else if self.submenu.is_none() {
                                    let n = ORDER.iter().position(|&e| e == id).unwrap();
                                    let id = ORDER[(n as isize + delta).rem_euclid(5) as usize];
                                    self.open = Some(id);
                                    self.focus =
                                        entries(id).iter().position(|e| reason(e, s).is_none());
                                }
                                self.request_focus = true;
                            }
                        }
                    }
                }
            }
        }
        ui.painter().rect_filled(whole, 0, style.palette.surface);
        // The positioned source nav's bottom border overlays the file strip.
        // MainWindow paints it after that normal-flow background, without
        // changing the 34px menu content height or workspace coordinates.
        let brand =
            egui::Rect::from_min_size(whole.min + egui::vec2(10.0, 5.5), egui::vec2(23.0, 23.0));
        ui.painter().rect_filled(brand, 4, style.palette.selection);
        ui.painter().text(
            brand.center(),
            Align2::CENTER_CENTER,
            "HF",
            fonts::bold(10.0),
            style.palette.selection_text,
        );
        let title = ui.painter().layout_job(fonts::spaced(
            "HexForge",
            fonts::bold(14.0),
            style.palette.text,
            0.28,
        ));
        // CSS letter-spacing also reserves the final character's trailing gap;
        // epaint inserts it only between glyphs, so reserve that 0.28px in flow.
        let title_width = title.size().x + 0.28;
        ui.painter().galley(
            brand.right_center() + egui::vec2(8.0, -title.size().y / 2.0),
            title,
            style.palette.text,
        );
        let mut x = brand.right() + 8.0 + title_width + 19.0;
        ui.ctx().accesskit_node_builder(ui.id(), |node| {
            node.set_role(egui::accesskit::Role::MenuBar);
            node.set_label("Application menu");
        });
        let mut buttons = vec![];
        for (id, label) in ORDER
            .into_iter()
            .zip(["File", "Edit", "Navigate", "Template", "View"])
        {
            let galley = ui.painter().layout_no_wrap(
                label.into(),
                fonts::regular(13.0),
                style.palette.muted,
            );
            let rect = egui::Rect::from_min_size(
                egui::pos2(x, whole.top() + 2.5),
                egui::vec2(galley.size().x + 22.0, 29.0),
            );
            let response = ui.interact(
                rect,
                ui.id().with(label),
                if modal {
                    egui::Sense::hover()
                } else {
                    egui::Sense::click()
                },
            );
            let active = self.open == Some(id);
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !modal, label));
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_role(egui::accesskit::Role::MenuItem);
                node.set_expanded(active);
                node.set_has_popup(egui::accesskit::HasPopup::Menu);
            });
            if response.hovered() || active {
                ui.painter().rect_filled(rect, 3, style.palette.hover);
            }
            ui.painter().galley_with_override_text_color(
                rect.center() - galley.size() / 2.0,
                galley,
                if response.hovered() || active {
                    style.palette.text
                } else {
                    style.palette.muted
                },
            );
            if active {
                self.anchor = rect;
            }
            if response.clicked() {
                if active {
                    self.close();
                } else {
                    self.open = Some(id);
                    self.submenu = None;
                    self.focus = None;
                    self.anchor = rect;
                }
            }
            buttons.push(rect);
            x = rect.right();
        }
        if let Some(id) = self.open {
            let options = entries(id);
            let mut selected = None;
            let popup = egui::Area::new(egui::Id::new("HexForge menu popup"))
                .fade_in(false)
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(self.anchor.left(), whole.bottom() - 1.0))
                .show(&ctx, |ui| {
                    if modal {
                        ui.disable();
                    }
                    menu::frame(style).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                        ui.set_width(menu_width(ui, style, &options, 280.0));
                        for (i, e) in options.iter().enumerate() {
                            if matches!(e, Entry::Separator) {
                                // TopMenu.vue: 1px line, 4px vertical and 3px
                                // horizontal margins. egui's Separator adds
                                // its own allocation height and theme stroke.
                                let (_, interval) =
                                    ui.allocate_space(egui::vec2(ui.available_width(), 9.0));
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        interval.min + egui::vec2(3.0, 4.0),
                                        egui::pos2(interval.right() - 3.0, interval.top() + 5.0),
                                    ),
                                    0,
                                    style.palette.border,
                                );
                                continue;
                            }
                            let r = paint_entry(ui, style, e, s);
                            if matches!(e, Entry::Branch(_)) {
                                self.submenu_anchor = r.rect;
                            }
                            if self.request_focus && self.submenu.is_none() && self.focus == Some(i)
                            {
                                r.request_focus();
                            }
                            if r.clicked() && reason(e, s).is_none() {
                                selected = Some(e.clone());
                            }
                        }
                    });
                });
            self.popup = Some(popup.response.rect);
            if let Some(e) = selected {
                self.activate(e, &mut events);
            }
            if let Some(sub) = self.submenu {
                let options = submenu_entries(sub, s);
                let width = menu_width(ui, style, &options, 230.0);
                let mut selected = None;
                let r = egui::Area::new(egui::Id::new("HexForge submenu flyout"))
                    .fade_in(false)
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(
                        popup.response.rect.right() + 4.0,
                        self.submenu_anchor.top(),
                    ))
                    .show(&ctx, |ui| {
                        if modal {
                            ui.disable();
                        }
                        menu::frame(style).inner_margin(0).show(ui, |ui| {
                            let height = (options.len() as f32 * 33.0).min(280.0) + 8.0;
                            ui.set_width(width + 8.0);
                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                            ui.allocate_ui_with_layout(
                                egui::vec2(width + 8.0, height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.spacing_mut().scroll.floating = false;
                                    ui.spacing_mut().scroll.bar_width = 6.0;
                                    ui.spacing_mut().scroll.bar_inner_margin = 4.0;
                                    ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                                    ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
                                    scroll_area::padded_menu_rows(
                                        ui,
                                        style,
                                        "template-result-menu",
                                        33.0,
                                        options.len(),
                                        |ui, rows| {
                                            for i in rows {
                                                let e = &options[i];
                                                let r = paint_entry(ui, style, e, s);
                                                if self.request_focus && self.focus == Some(i) {
                                                    r.request_focus();
                                                    r.scroll_to_me(Some(egui::Align::Center));
                                                }
                                                if r.clicked() && reason(e, s).is_none() {
                                                    selected = Some(e.clone());
                                                }
                                            }
                                        },
                                    );
                                },
                            );
                        });
                    });
                self.flyout = Some(r.response.rect);
                if let Some(e) = selected {
                    self.activate(e, &mut events);
                }
            } else {
                self.flyout = None;
            }
            self.request_focus = false;
            let outside = ctx.input(|i| {
                i.pointer.any_click()
                    && i.pointer.interact_pos().is_some_and(|p| {
                        !buttons.iter().any(|r| r.contains(p))
                            && !self.popup.is_some_and(|r| r.contains(p))
                            && !self.flyout.is_some_and(|r| r.contains(p))
                    })
            });
            if outside && !modal {
                self.close();
            }
        }
        events
    }
}
/// egui queues spatial arrow navigation before presenters consume the event.
/// The application menu owns these arrows and explicitly chooses the next
/// item/parent; a second spatial move at frame end must not replace that focus.
fn consume_navigation_key(ctx: &egui::Context, key: egui::Key) -> bool {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
        ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        true
    } else {
        false
    }
}
fn paint_entry(
    ui: &mut egui::Ui,
    style: &HexForgeStyle,
    e: &Entry,
    s: &MainSnapshot,
) -> egui::Response {
    let reason = reason(e, s);
    match e {
        Entry::Command(c) => {
            let shortcut = shortcut_label(WindowKind::Main, *c);
            let r = menu::item(
                ui,
                style,
                c.label(),
                shortcut.as_deref(),
                reason.as_deref(),
                checked(*c, s),
            );
            if reason.is_none() {
                r.on_hover_text(shortcut.map_or_else(
                    || c.label().to_owned(),
                    |key| format!("{} ({key})", c.label()),
                ))
            } else {
                r
            }
        }
        Entry::Branch(sub) => {
            let r = StyledButton::show(
                ui,
                style,
                &ButtonSpec {
                    label_left_inset: Some(9.0),
                    width: Some(ui.available_width()),
                    height: Some(33.0),
                    disabled_reason: reason.as_deref(),
                    ..ButtonSpec::new(if *sub == Submenu::Results {
                        "Parsed Results"
                    } else {
                        "Minimap"
                    })
                    .role(ButtonRole::Menu)
                },
            );
            ui.painter().text(
                r.rect.right_center() - egui::vec2(9.0, 0.0),
                Align2::RIGHT_CENTER,
                "›",
                fonts::regular(12.0),
                style
                    .palette
                    .text
                    .gamma_multiply(if reason.is_some() { 0.38 } else { 1.0 }),
            );
            r
        }
        Entry::Leaf(path, start, _) => {
            menu::result_item(ui, style, path, &format!("0x{start:08X}"))
        }
        Entry::Separator => unreachable!(),
    }
}
fn menu_width(ui: &egui::Ui, style: &HexForgeStyle, entries: &[Entry], minimum: f32) -> f32 {
    entries
        .iter()
        .map(|e| {
            let (label, hint) = match e {
                Entry::Command(c) => (c.label().to_owned(), shortcut_label(WindowKind::Main, *c)),
                Entry::Leaf(path, start, _) => (path.clone(), Some(format!("0x{start:08X}"))),
                Entry::Branch(s) => (
                    if *s == Submenu::Results {
                        "Parsed Results"
                    } else {
                        "Minimap"
                    }
                    .into(),
                    None,
                ),
                Entry::Separator => return 0.0,
            };
            let l = ui
                .painter()
                .layout_job(fonts::spaced(
                    &label,
                    fonts::regular(12.0),
                    style.palette.text,
                    0.0,
                ))
                .size()
                .x;
            let h = hint.map_or(0.0, |hint| {
                ui.painter()
                    .layout_job(fonts::spaced(
                        &hint,
                        if matches!(e, Entry::Command(_)) {
                            fonts::generic_monospace(12.0)
                        } else {
                            fonts::regular(11.0)
                        },
                        style.palette.text,
                        0.0,
                    ))
                    .size()
                    .x
            });
            // Source grid: 18px horizontal padding + 14px check column +
            // two 7px gaps. No additional generic menu shortcut reservation.
            l.max(120.0)
                + h
                + if matches!(e, Entry::Command(_)) {
                    46.0
                } else {
                    25.0
                }
        })
        .fold(minimum, f32::max)
}
