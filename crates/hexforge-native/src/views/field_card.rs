//! Native field controls are a presenter over the existing structured editor
//! model. Raw rejected input stays in its stable-ID sidecar, never in JSON.
use crate::{
    style::{fonts, HexForgeStyle},
    widgets::{
        button::{ButtonRole, ButtonSpec, StyledButton},
        input,
    },
};
use eframe::egui;
use hexforge_controller::template::{
    editing::{
        has_endian, EditOutcome, EditRejected, EditorModel, FieldId, FieldPath, TemplateEdit,
    },
    references,
    validation::{enum_order, ValidationIssue},
};
use hexforge_core::template::{FieldType, ParsedNode, TemplateDefinition, TemplateField};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn control_id(id: FieldId, control: &str, index: Option<usize>) -> egui::Id {
    egui::Id::new(("template-field", id.0, control, index))
}
#[derive(Default)]
pub struct EditorControlState {
    fields: HashMap<FieldId, CardState>,
    enum_buffers: HashMap<(FieldId, usize), String>,
    pub confirmation: Option<EditOutcome>,
    pub reveal: Option<egui::Id>,
    pub summary_collapsed: bool,
}
#[derive(Clone)]
struct CardState {
    expanded: bool,
    advanced: bool,
    labels: bool,
    flags: bool,
}
impl Default for CardState {
    fn default() -> Self {
        Self {
            expanded: true,
            advanced: false,
            labels: false,
            flags: false,
        }
    }
}
impl EditorControlState {
    pub fn pending_inputs(&self) -> bool {
        !self.enum_buffers.is_empty()
    }
    /// HTML enum keys commit on change/blur. Keep their in-progress text local
    /// so numeric-key sorting cannot retarget subsequent keystrokes to another
    /// label. Explicit save/close flushes commit the same buffers first.
    pub fn take_buffered_inputs(&mut self, model: &EditorModel) -> Vec<FieldEvent> {
        self.enum_buffers
            .drain()
            .filter_map(|((id, index), text)| {
                model.path_for_id(id).map(|path| FieldEvent::Input {
                    id,
                    path,
                    control: "enum-key".into(),
                    index: Some(index),
                    text,
                })
            })
            .collect()
    }
    pub fn reveal_control(
        &mut self,
        model: &EditorModel,
        id: FieldId,
        control: &str,
        index: Option<usize>,
    ) {
        if let Some(path) = model.path_for_id(id) {
            for n in 1..=path.0.len() {
                if let Some(ancestor) = model.id(&FieldPath(path.0[..n].to_vec())) {
                    self.fields.entry(ancestor).or_default().expanded = true;
                }
            }
        }
        let s = self.fields.entry(id).or_default();
        if ["align", "comment"].contains(&control)
            || control.starts_with("condition-")
            || control.starts_with("expect-")
            || control.starts_with("enum-")
            || control.starts_with("flag-")
        {
            s.advanced = true;
        }
        s.labels |= control.starts_with("enum-");
        s.flags |= control.starts_with("flag-");
        self.reveal = Some(control_id(id, control, index));
    }
    fn focus(&mut self, ui: &egui::Ui, r: &egui::Response) {
        if self.reveal == Some(r.id) {
            r.request_focus();
            r.scroll_to_me(Some(egui::Align::Center));
            self.reveal = None;
            ui.ctx().request_repaint();
        }
    }
    pub fn reconcile(&mut self, model: &EditorModel) {
        self.fields.retain(|id, _| model.path_for_id(*id).is_some());
        self.enum_buffers.retain(|(id, index), _| {
            model
                .named_entries()
                .iter()
                .any(|(candidate, _, _, field)| {
                    candidate == id
                        && field
                            .enum_labels
                            .as_ref()
                            .is_some_and(|labels| *index < labels.len())
                })
        });
        if !model.pending() {
            self.confirmation = None;
        }
    }
}
#[derive(Debug, Clone)]
pub enum FieldEvent {
    Edit(TemplateEdit),
    Input {
        id: FieldId,
        path: FieldPath,
        control: String,
        index: Option<usize>,
        text: String,
    },
    Navigate {
        start: u64,
        end: u64,
    },
}
pub struct FieldSnapshot {
    pub id: FieldId,
    pub locator: FieldPath,
    pub path: String,
    pub field: TemplateField,
    pub depth: usize,
    pub index: usize,
    pub siblings: usize,
    pub element: bool,
}
pub struct ScopeSnapshot<'a> {
    pub definition: &'a TemplateDefinition,
    pub model: &'a EditorModel,
    pub issues: &'a [ValidationIssue],
    pub results: &'a [ParsedNode],
}
enum Kind {
    Text,
    Number,
    Select(Vec<(String, String)>),
}
struct Control {
    key: &'static str,
    label: String,
    text: String,
    index: Option<usize>,
    hint: &'static str,
    kind: Kind,
    refs: Option<&'static str>,
}
fn str_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}
fn text(key: &'static str, label: &str, value: &Value) -> Control {
    Control {
        key,
        label: label.into(),
        text: str_value(value),
        index: None,
        hint: "",
        kind: Kind::Text,
        refs: None,
    }
}
fn num(key: &'static str, label: &str, value: &Value) -> Control {
    let mut c = text(key, label, value);
    c.kind = Kind::Number;
    c
}
fn select(key: &'static str, label: &str, value: &str, options: &[(&str, &str)]) -> Control {
    Control {
        key,
        label: label.into(),
        text: value.into(),
        index: None,
        hint: "",
        kind: Kind::Select(
            options
                .iter()
                .map(|(v, l)| (v.to_string(), l.to_string()))
                .collect(),
        ),
        refs: None,
    }
}
fn reference(
    key: &'static str,
    label: &str,
    value: &Value,
    kind: &'static str,
    hint: &'static str,
) -> Control {
    let mut c = text(key, label, value);
    c.refs = Some(kind);
    c.hint = hint;
    c
}
fn show_control(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    f: &FieldSnapshot,
    scope: &ScopeSnapshot<'_>,
    state: &mut EditorControlState,
    c: &Control,
    events: &mut Vec<FieldEvent>,
) {
    if !c.label.is_empty() {
        ui.label(egui::text::LayoutJob::single_section(
            c.label.clone(),
            egui::TextFormat {
                font_id: fonts::regular(11.0),
                color: s.palette.muted,
                line_height: Some(14.0),
                ..Default::default()
            },
        ));
    }
    let id = control_id(f.id, c.key, c.index);
    let mut value = scope
        .model
        .raw
        .get(f.id, c.key, c.index)
        .map_or_else(|| c.text.clone(), |raw| raw.text.clone());
    if c.key == "enum-key" {
        if let Some(buffer) = state.enum_buffers.get(&(f.id, c.index.unwrap_or(0))) {
            value = buffer.clone();
        }
    }
    let r = match &c.kind {
        Kind::Select(options) => {
            let options = options
                .iter()
                .map(|(v, l)| (v.clone(), l.as_str()))
                .collect::<Vec<_>>();
            let old = value.clone();
            let changed = input::combo(ui, s, id, &mut value, &options, ui.available_width());
            if changed && value != old {
                events.push(FieldEvent::Input {
                    id: f.id,
                    path: f.locator.clone(),
                    control: c.key.into(),
                    index: c.index,
                    text: value.clone(),
                });
            }
            // The combobox owns its focus; use its stable input identity for
            // validation reveal, independently of its popup's row identities.
            if state.reveal == Some(id) {
                if let Some(response) = ui.ctx().read_response(id) {
                    state.focus(ui, &response);
                }
            }
            return;
        }
        _ => input::text(ui, s, id, &mut value, ui.available_width(), c.hint),
    };
    state.focus(ui, &r);
    if c.key == "enum-key" {
        let buffer_key = (f.id, c.index.unwrap_or(0));
        if r.changed() {
            state.enum_buffers.insert(buffer_key, value.clone());
        }
        if r.lost_focus() {
            if let Some(text) = state.enum_buffers.remove(&buffer_key) {
                events.push(FieldEvent::Input {
                    id: f.id,
                    path: f.locator.clone(),
                    control: c.key.into(),
                    index: c.index,
                    text,
                });
            }
        }
    } else if r.changed() {
        events.push(FieldEvent::Input {
            id: f.id,
            path: f.locator.clone(),
            control: c.key.into(),
            index: c.index,
            text: value.clone(),
        });
    }
    if let Some(kind) = c.refs {
        if r.has_focus() && !value.is_empty() {
            let candidates = references::reference_candidates(scope.definition, &f.path, kind)
                .into_iter()
                .filter(|candidate| candidate.reference.contains(&value))
                .take(12)
                .collect::<Vec<_>>();
            if !candidates.is_empty() {
                egui::Area::new(id.with("reference-candidates"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(r.rect.left_bottom() + egui::vec2(0.0, 5.0))
                    .show(ui.ctx(), |ui| {
                        crate::widgets::menu::frame(s).show(ui, |ui| {
                            ui.set_width(r.rect.width().max(180.0));
                            for candidate in candidates {
                                if StyledButton::show(
                                    ui,
                                    s,
                                    &ButtonSpec::new(&format!(
                                        "{} · {}",
                                        candidate.path,
                                        serde_json::to_value(candidate.field_type)
                                            .unwrap()
                                            .as_str()
                                            .unwrap()
                                    ))
                                    .role(ButtonRole::Menu),
                                )
                                .clicked()
                                {
                                    events.push(FieldEvent::Input {
                                        id: f.id,
                                        path: f.locator.clone(),
                                        control: c.key.into(),
                                        index: c.index,
                                        text: candidate.reference,
                                    });
                                }
                            }
                        });
                    });
            }
        }
    }
}
fn grid(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    f: &FieldSnapshot,
    scope: &ScopeSnapshot<'_>,
    state: &mut EditorControlState,
    controls: Vec<Control>,
    events: &mut Vec<FieldEvent>,
) {
    let columns = if ui.ctx().content_rect().width() <= 620.0 {
        2
    } else {
        3
    };
    let width = ((ui.available_width() - 7.0 * (columns - 1) as f32) / columns as f32).max(1.0);
    // The CSS grid has 7px between its rows; its containing field body has
    // 8px between sections. Keep those two gaps independent.
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 7.0;
        for chunk in controls.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                for c in chunk {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 45.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            show_control(ui, s, f, scope, state, c, events);
                        },
                    );
                }
            });
        }
    });
}
fn disclosure(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    id: egui::Id,
    label: &str,
    open: &mut bool,
) -> egui::Response {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    let r = ui.interact(rect, id, egui::Sense::click());
    r.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, ui.is_enabled(), label)
    });
    if r.hovered() {
        ui.painter().rect_filled(rect, 2, s.palette.hover);
    }
    crate::widgets::disclosure::paint(ui.painter(), rect, label, *open, s.palette.muted);
    if r.clicked() {
        *open = !*open;
    }
    r
}
fn checkbox(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    id: egui::Id,
    label: &str,
    checked: &mut bool,
) -> bool {
    let text = ui
        .painter()
        .layout_no_wrap(label.into(), fonts::regular(11.0), s.palette.muted);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(text.size().x + 20.0, 18.0), egui::Sense::hover());
    let r = ui.interact(rect, id, egui::Sense::click());
    let boxrect = egui::Rect::from_center_size(
        rect.left_center() + egui::vec2(7.5, 0.0),
        egui::vec2(15.0, 15.0),
    );
    ui.painter().rect_filled(
        boxrect,
        2,
        if *checked {
            s.palette.selection
        } else {
            s.palette.input
        },
    );
    ui.painter().rect_stroke(
        boxrect,
        2,
        egui::Stroke::new(
            1.0,
            if r.has_focus() {
                s.palette.selection
            } else {
                s.palette.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    if *checked {
        ui.painter().line_segment(
            [
                boxrect.min + egui::vec2(3.0, 7.0),
                boxrect.min + egui::vec2(6.0, 10.0),
            ],
            egui::Stroke::new(1.4, s.palette.selection_text),
        );
        ui.painter().line_segment(
            [
                boxrect.min + egui::vec2(6.0, 10.0),
                boxrect.min + egui::vec2(12.0, 4.0),
            ],
            egui::Stroke::new(1.4, s.palette.selection_text),
        );
    }
    ui.painter().galley(
        rect.left_center() + egui::vec2(20.0, -text.size().y / 2.0),
        text,
        s.palette.muted,
    );
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *checked, label)
    });
    if r.clicked() {
        *checked = !*checked;
        true
    } else {
        false
    }
}
const TYPES: &[(&str, &str)] = &[
    ("u8", "u8"),
    ("u16", "u16"),
    ("u32", "u32"),
    ("u64", "u64"),
    ("i8", "i8"),
    ("i16", "i16"),
    ("i32", "i32"),
    ("i64", "i64"),
    ("f32", "f32"),
    ("f64", "f64"),
    ("bool", "bool"),
    ("string", "string"),
    ("bytes", "bytes"),
    ("struct", "struct"),
    ("array", "array"),
];
fn advanced_summary(v: &Value) -> String {
    let mut values = vec![];
    if let Some(n) = v["align"].as_u64() {
        values.push(format!("Align {n}"));
    }
    if v.get("condition").is_some() {
        values.push("Condition configured".into());
    }
    if let Some(check) = v.get("expect") {
        values.push(match check["kind"].as_str() {
            Some("equals") => format!(
                "Expected: {}",
                str_value(&check["value"]["value"])
                    .chars()
                    .take(40)
                    .collect::<String>()
            ),
            Some("oneOf") => format!(
                "{} allowed values",
                check["values"].as_array().map_or(0, Vec::len)
            ),
            _ => "Expected range".into(),
        });
    }
    if let Some(labels) = v["enumLabels"].as_object() {
        values.push(format!("{} labels", labels.len()));
    }
    if let Some(flags) = v["bitFlags"].as_array() {
        values.push(format!("{} flags", flags.len()));
    }
    if v["comment"].as_str().is_some_and(|s| !s.is_empty()) {
        values.push("Comment".into());
    }
    if values.is_empty() {
        "Advanced".into()
    } else {
        format!("Advanced · {}", values.join(" · "))
    }
}
pub fn show_field_card(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    f: &FieldSnapshot,
    scope: &ScopeSnapshot<'_>,
    state: &mut EditorControlState,
) -> Vec<FieldEvent> {
    let mut events = vec![];
    let mut card = state.fields.get(&f.id).cloned().unwrap_or_default();
    let v = serde_json::to_value(&f.field).unwrap();
    let kind = v["type"].as_str().unwrap();
    egui::Frame::new()
        .fill(s.palette.card)
        .stroke(egui::Stroke::new(1.0, s.palette.border))
        .corner_radius(5)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(7.0, 8.0);
            ui.horizontal(|ui| {
                if StyledButton::show(
                    ui,
                    s,
                    &ButtonSpec {
                        width: Some(24.0),
                        tooltip: Some(if card.expanded {
                            "Collapse field"
                        } else {
                            "Expand field"
                        }),
                        ..ButtonSpec::new(if card.expanded { "▾" } else { "▸" })
                            .role(ButtonRole::Transparent)
                    },
                )
                .clicked()
                {
                    card.expanded = !card.expanded;
                }
                let tools = if f.element {
                    0.0
                } else {
                    4.0 * 25.0 + 3.0 * 4.0 + 7.0
                };
                let w = (ui.available_width() - tools - if card.expanded { 125.0 } else { 0.0 })
                    .max(1.0);
                if card.expanded {
                    if f.element {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 28.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                // Source strong { flex:1 }: reserve the label's
                                // full slot, not just its intrinsic text width.
                                ui.set_min_width(w);
                                ui.label(
                                    egui::RichText::new("Array element")
                                        .font(fonts::bold(11.0))
                                        .color(s.palette.muted),
                                );
                            },
                        );
                    } else {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 28.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                show_control(
                                    ui,
                                    s,
                                    f,
                                    scope,
                                    state,
                                    &text("name", "", &v["name"]),
                                    &mut events,
                                );
                            },
                        );
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(118.0, 28.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            show_control(
                                ui,
                                s,
                                f,
                                scope,
                                state,
                                &select("type", "", kind, TYPES),
                                &mut events,
                            );
                        },
                    );
                } else {
                    let summary = format!(
                        "{} · {kind}{}",
                        if f.element {
                            "Array element"
                        } else {
                            f.field
                                .name
                                .as_deref()
                                .filter(|s| !s.is_empty())
                                .unwrap_or("Unnamed field")
                        },
                        if kind == "struct" {
                            let count = f.field.fields.as_ref().map_or(0, Vec::len);
                            format!(" · {count} {}", if count == 1 { "field" } else { "fields" })
                        } else if kind == "array" {
                            if v["count"].get("ref").is_some() {
                                format!(" · Count: {}", str_value(&v["count"]["ref"]))
                            } else {
                                format!(" · {} elements", v["count"]["fixed"].as_u64().unwrap_or(0))
                            }
                        } else if kind == "bytes" || kind == "string" {
                            if v["length"].is_object() {
                                format!(" · Length: {}", str_value(&v["length"]["ref"]))
                            } else {
                                format!(
                                    " · {} B",
                                    v["length"]
                                        .as_u64()
                                        .or_else(|| v["maxLength"].as_u64())
                                        .unwrap_or(0)
                                )
                            }
                        } else {
                            String::new()
                        }
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(w, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(w);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(summary).font(fonts::regular(12.0)),
                                )
                                .truncate(),
                            );
                        },
                    );
                    let count = scope
                        .issues
                        .iter()
                        .filter(|issue| issue.path == f.path)
                        .count();
                    if count > 0 {
                        ui.label(
                            egui::RichText::new(format!("{count} !"))
                                .font(fonts::regular(11.0))
                                .color(s.palette.modified),
                        )
                        .on_hover_text(format!("{count} errors"));
                    }
                }
                if !f.element {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (label, tooltip, disabled, edit) in [
                        (
                            "↑",
                            "Move up",
                            f.index == 0,
                            TemplateEdit::Move {
                                path: f.locator.clone(),
                                delta: -1,
                            },
                        ),
                        (
                            "↓",
                            "Move down",
                            f.index + 1 >= f.siblings,
                            TemplateEdit::Move {
                                path: f.locator.clone(),
                                delta: 1,
                            },
                        ),
                        (
                            "⧉",
                            "Duplicate field",
                            false,
                            TemplateEdit::Duplicate {
                                path: f.locator.clone(),
                            },
                        ),
                        (
                            "×",
                            "Remove field",
                            false,
                            TemplateEdit::Delete {
                                path: f.locator.clone(),
                            },
                        ),
                    ] {
                        if StyledButton::show(
                            ui,
                            s,
                            &ButtonSpec {
                                width: Some(25.0),
                                font_size: Some(12.0),
                                tooltip: Some(tooltip),
                                disabled_reason: disabled.then_some(tooltip),
                                ..ButtonSpec::new(label)
                            },
                        )
                        .clicked()
                        {
                            events.push(FieldEvent::Edit(edit));
                        }
                    }
                }
            });
            if !card.expanded {
                return;
            }
            let mut controls = vec![];
            if !f.element {
                controls.push(select(
                    "placement-mode",
                    "Placement",
                    v["placement"]["mode"].as_str().unwrap_or("sequential"),
                    &[
                        ("sequential", "Sequential"),
                        ("relative", "Relative to struct"),
                        ("absolute", "Absolute"),
                    ],
                ));
            }
            if v.get("placement").is_some() && v["placement"]["mode"] != "sequential" {
                let offset = &v["placement"]["offset"];
                controls.push(select(
                    "offset-source",
                    "Offset source",
                    if offset.is_object() { "ref" } else { "fixed" },
                    &[
                        ("fixed", "Decimal offset"),
                        ("ref", "Earlier unsigned field"),
                    ],
                ));
                if offset.is_object() {
                    controls.push(reference(
                        "offset-ref",
                        "Offset field",
                        &offset["ref"],
                        "unsigned",
                        "header.offset",
                    ));
                    controls.push(text("offset-add", "Add bytes", &offset["add"]));
                } else {
                    controls.push(text("placement-offset", "Offset", offset));
                }
            }
            if has_endian(f.field.field_type) {
                controls.push(select(
                    "endianness",
                    "Endianness",
                    v["endianness"].as_str().unwrap_or(""),
                    &[("", "Inherit"), ("little", "Little"), ("big", "Big")],
                ));
            }
            let length_ref = v["length"].is_object();
            let terminated = v.get("maxLength").is_some();
            if kind == "bytes" && !length_ref {
                controls.push(num("length", "Length bytes", &v["length"]));
            }
            if kind == "string" {
                controls.push(select(
                    "encoding",
                    "Encoding",
                    v["encoding"].as_str().unwrap_or("utf8"),
                    &[
                        ("ascii", "ASCII"),
                        ("utf8", "UTF-8"),
                        ("utf16le", "UTF-16LE"),
                        ("utf16be", "UTF-16BE"),
                    ],
                ));
                controls.push(select(
                    "text-mode",
                    "Text bound",
                    if terminated {
                        "nullTerminated"
                    } else {
                        "fixed"
                    },
                    &[
                        ("fixed", "Fixed length"),
                        ("nullTerminated", "Null-terminated"),
                    ],
                ));
                if !length_ref {
                    controls.push(num(
                        "text-length",
                        if terminated {
                            "Maximum bytes"
                        } else {
                            "Length bytes"
                        },
                        if terminated {
                            &v["maxLength"]
                        } else {
                            &v["length"]
                        },
                    ));
                }
            }
            if kind == "bytes" || (kind == "string" && !terminated) {
                controls.push(select(
                    "length-source",
                    "Length source",
                    if length_ref { "ref" } else { "fixed" },
                    &[("fixed", "Fixed length"), ("ref", "Earlier unsigned field")],
                ));
                if length_ref {
                    controls.push(reference(
                        "length-ref",
                        "Length field",
                        &v["length"]["ref"],
                        "unsigned",
                        "header.length",
                    ));
                    controls.push(num("length-max", "Maximum bytes", &v["length"]["max"]));
                }
            }
            if kind == "array" {
                let referenced = v["count"].get("ref").is_some();
                controls.push(select(
                    "count-mode",
                    "Count source",
                    if referenced { "ref" } else { "fixed" },
                    &[("fixed", "Fixed count"), ("ref", "Earlier unsigned field")],
                ));
                controls.push(if referenced {
                    reference(
                        "count-ref",
                        "Field path",
                        &v["count"]["ref"],
                        "unsigned",
                        "header.count",
                    )
                } else {
                    num("count-fixed", "Count", &v["count"]["fixed"])
                });
            }
            grid(ui, s, f, scope, state, controls, &mut events);
            disclosure(
                ui,
                s,
                control_id(f.id, "advanced", None),
                &advanced_summary(&v),
                &mut card.advanced,
            );
            if card.advanced {
                grid(
                    ui,
                    s,
                    f,
                    scope,
                    state,
                    vec![num("align", "Align bytes", &v["align"])],
                    &mut events,
                );
                if !f.element {
                    let mut checked = v.get("condition").is_some();
                    if checkbox(
                        ui,
                        s,
                        control_id(f.id, "condition-toggle", None),
                        "Condition",
                        &mut checked,
                    ) {
                        events.push(FieldEvent::Input {
                            id: f.id,
                            path: f.locator.clone(),
                            control: "condition-toggle".into(),
                            index: None,
                            text: checked.to_string(),
                        });
                    }
                    if checked {
                        let cond = &v["condition"];
                        let ty = cond["value"]["type"].as_str().unwrap_or("unsigned");
                        let mut controls = vec![
                            reference(
                                "condition-ref",
                                "Condition field path",
                                &cond["ref"],
                                match ty {
                                    "signed" => "signed",
                                    "bool" => "bool",
                                    "string" => "string",
                                    _ => "unsigned",
                                },
                                "Earlier field path",
                            ),
                            select(
                                "condition-op",
                                "Comparison",
                                cond["op"].as_str().unwrap_or("eq"),
                                &[
                                    ("eq", "eq"),
                                    ("ne", "ne"),
                                    ("lt", "lt"),
                                    ("lte", "lte"),
                                    ("gt", "gt"),
                                    ("gte", "gte"),
                                ],
                            ),
                            select(
                                "condition-type",
                                "Condition type",
                                ty,
                                &[
                                    ("unsigned", "Unsigned"),
                                    ("signed", "Signed"),
                                    ("bool", "Bool"),
                                    ("string", "Text"),
                                ],
                            ),
                        ];
                        controls.push(if ty == "bool" {
                            select(
                                "condition-value",
                                "Comparison value",
                                if cond["value"]["value"] == true {
                                    "true"
                                } else {
                                    "false"
                                },
                                &[("false", "false"), ("true", "true")],
                            )
                        } else {
                            text(
                                "condition-value",
                                "Comparison value",
                                &cond["value"]["value"],
                            )
                        });
                        grid(ui, s, f, scope, state, controls, &mut events);
                    }
                }
                if kind != "struct" && kind != "array" {
                    show_expectation(ui, s, f, scope, state, &v, &mut events);
                }
                if references::unsigned(f.field.field_type)
                    || references::signed(f.field.field_type)
                {
                    ui.horizontal_top(|ui| {
                        let width = (ui.available_width() - 4.0)
                            / if references::unsigned(f.field.field_type) {
                                2.0
                            } else {
                                1.0
                            };
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 17.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                disclosure(
                                    ui,
                                    s,
                                    control_id(f.id, "enum-labels", None),
                                    "Enum labels",
                                    &mut card.labels,
                                );
                                if card.labels {
                                    show_enum(ui, s, f, scope, state, &v, &mut events);
                                }
                            },
                        );
                        if references::unsigned(f.field.field_type) {
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 17.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    disclosure(
                                        ui,
                                        s,
                                        control_id(f.id, "bit-flags", None),
                                        "Bit flags",
                                        &mut card.flags,
                                    );
                                    if card.flags {
                                        show_flags(ui, s, f, scope, state, &v, &mut events);
                                    }
                                },
                            );
                        }
                    });
                }
                show_control(
                    ui,
                    s,
                    f,
                    scope,
                    state,
                    &Control {
                        hint: "Comment",
                        ..text("comment", "", &v["comment"])
                    },
                    &mut events,
                );
            }
            if kind == "struct" || kind == "array" {
                let nested = egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 11,
                        top: 8,
                        right: 0,
                        // Each source child card has margin-bottom:8px,
                        // including the final card inside this container.
                        bottom: if f.field.element.is_some()
                            || f.field
                                .fields
                                .as_ref()
                                .is_some_and(|fields| !fields.is_empty())
                        {
                            8
                        } else {
                            0
                        },
                    })
                    .show(ui, |ui| {
                        let header_min_height = ui.spacing().interact_size.y;
                        if kind == "array" {
                            // horizontal() reads its minimum height before
                            // entering the closure, so set this at the owner.
                            ui.spacing_mut().interact_size.y = 14.0;
                        }
                        ui.horizontal(|ui| {
                            ui.label(egui::text::LayoutJob::single_section(
                                if kind == "struct" {
                                    "Structure fields"
                                } else {
                                    "Array element definition"
                                }
                                .into(),
                                egui::TextFormat {
                                    font_id: fonts::regular(11.0),
                                    color: s.palette.muted,
                                    line_height: Some(14.0),
                                    ..Default::default()
                                },
                            ));
                            if kind == "struct" {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if StyledButton::show(
                                            ui,
                                            s,
                                            &ButtonSpec {
                                                font_size: Some(11.0),
                                                ..ButtonSpec::new("+ Field")
                                            },
                                        )
                                        .clicked()
                                        {
                                            events.push(FieldEvent::Edit(TemplateEdit::Add {
                                                parent: Some(f.locator.clone()),
                                            }));
                                        }
                                    },
                                );
                            }
                        });
                        ui.spacing_mut().interact_size.y = header_min_height;
                        if let Some(children) = &f.field.fields {
                            for (index, child) in children.iter().enumerate() {
                                let path = f.locator.clone().child(index);
                                if let Some(id) = scope.model.id(&path) {
                                    events.extend(show_field_card(
                                        ui,
                                        s,
                                        &FieldSnapshot {
                                            id,
                                            locator: path,
                                            path: format!(
                                                "{}.{}",
                                                f.path,
                                                child.name.as_deref().unwrap_or("")
                                            ),
                                            field: child.clone(),
                                            depth: f.depth + 1,
                                            index,
                                            siblings: children.len(),
                                            element: false,
                                        },
                                        scope,
                                        state,
                                    ));
                                }
                            }
                        }
                        if let Some(element) = &f.field.element {
                            let path = f.locator.clone().element();
                            if let Some(id) = scope.model.id(&path) {
                                events.extend(show_field_card(
                                    ui,
                                    s,
                                    &FieldSnapshot {
                                        id,
                                        locator: path,
                                        path: format!("{}[]", f.path),
                                        field: (**element).clone(),
                                        depth: f.depth + 1,
                                        index: 0,
                                        siblings: 1,
                                        element: true,
                                    },
                                    scope,
                                    state,
                                ));
                            }
                        }
                    });
                ui.painter().vline(
                    nested.response.rect.left() + 1.0,
                    nested.response.rect.y_range(),
                    egui::Stroke::new(2.0, s.palette.border_strong),
                );
            }
            for issue in scope.issues.iter().filter(|i| i.path == f.path) {
                ui.label(
                    egui::RichText::new(&issue.message)
                        .font(fonts::regular(11.0))
                        .color(s.palette.modified),
                );
            }
            if let Some(leaf) = find_leaf(scope.results, &f.path) {
                if let Some((start, end)) = hexforge_controller::results::navigable_range(leaf) {
                    if StyledButton::show(
                        ui,
                        s,
                        &ButtonSpec {
                            font_size: Some(11.0),
                            ..ButtonSpec::new(&format!(
                                "↗ {} · {start}",
                                leaf.value.as_deref().unwrap_or("")
                            ))
                            .role(ButtonRole::Transparent)
                        },
                    )
                    .clicked()
                    {
                        events.push(FieldEvent::Navigate { start, end });
                    }
                }
            }
        });
    state.fields.insert(f.id, card);
    events
}
fn find_leaf<'a>(nodes: &'a [ParsedNode], path: &str) -> Option<&'a ParsedNode> {
    for n in nodes {
        if n.path == path {
            return Some(n);
        }
        if let Some(n) = find_leaf(&n.children, path) {
            return Some(n);
        }
    }
    None
}
fn show_enum(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    f: &FieldSnapshot,
    scope: &ScopeSnapshot<'_>,
    state: &mut EditorControlState,
    v: &Value,
    events: &mut Vec<FieldEvent>,
) {
    let mut labels = v["enumLabels"]
        .as_object()
        .map(|m| m.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    labels.sort_by(|(a, _), (b, _)| enum_order(a).cmp(&enum_order(b)));
    for (i, (key, label)) in labels.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (control, value, w) in [
                ("enum-key", json!(key), 85.0),
                (
                    "enum-label",
                    (*label).clone(),
                    (ui.available_width() - 114.0).max(1.0),
                ),
            ] {
                ui.allocate_ui_with_layout(
                    egui::vec2(w, 28.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        show_control(
                            ui,
                            s,
                            f,
                            scope,
                            state,
                            &Control {
                                index: Some(i),
                                ..text(control, "", &value)
                            },
                            events,
                        )
                    },
                );
            }
            if StyledButton::show(ui, s, &ButtonSpec::new("×").width(25.0)).clicked() {
                events.push(FieldEvent::Input {
                    id: f.id,
                    path: f.locator.clone(),
                    control: "remove-enum".into(),
                    index: Some(i),
                    text: String::new(),
                });
            }
        });
    }
    if StyledButton::show(
        ui,
        s,
        &ButtonSpec {
            font_size: Some(11.0),
            ..ButtonSpec::new("+ Label")
        },
    )
    .clicked()
    {
        events.push(FieldEvent::Input {
            id: f.id,
            path: f.locator.clone(),
            control: "add-enum".into(),
            index: None,
            text: String::new(),
        });
    }
}
fn show_flags(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    f: &FieldSnapshot,
    scope: &ScopeSnapshot<'_>,
    state: &mut EditorControlState,
    v: &Value,
    events: &mut Vec<FieldEvent>,
) {
    for (i, flag) in v["bitFlags"].as_array().into_iter().flatten().enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (control, value, w) in [
                ("flag-bit", &flag["bit"], 85.0),
                (
                    "flag-name",
                    &flag["name"],
                    (ui.available_width() - 114.0).max(1.0),
                ),
            ] {
                ui.allocate_ui_with_layout(
                    egui::vec2(w, 28.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        show_control(
                            ui,
                            s,
                            f,
                            scope,
                            state,
                            &Control {
                                index: Some(i),
                                ..text(control, "", value)
                            },
                            events,
                        )
                    },
                );
            }
            if StyledButton::show(ui, s, &ButtonSpec::new("×").width(25.0)).clicked() {
                events.push(FieldEvent::Input {
                    id: f.id,
                    path: f.locator.clone(),
                    control: "remove-flag".into(),
                    index: Some(i),
                    text: String::new(),
                });
            }
        });
    }
    if StyledButton::show(
        ui,
        s,
        &ButtonSpec {
            font_size: Some(11.0),
            ..ButtonSpec::new("+ Flag")
        },
    )
    .clicked()
    {
        events.push(FieldEvent::Input {
            id: f.id,
            path: f.locator.clone(),
            control: "add-flag".into(),
            index: None,
            text: String::new(),
        });
    }
}
fn show_expectation(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    f: &FieldSnapshot,
    scope: &ScopeSnapshot<'_>,
    state: &mut EditorControlState,
    v: &Value,
    events: &mut Vec<FieldEvent>,
) {
    let mut checked = v.get("expect").is_some();
    if checkbox(
        ui,
        s,
        control_id(f.id, "expect-toggle", None),
        "Expected value check",
        &mut checked,
    ) {
        events.push(FieldEvent::Input {
            id: f.id,
            path: f.locator.clone(),
            control: "expect-toggle".into(),
            index: None,
            text: checked.to_string(),
        });
    }
    if !checked {
        return;
    }
    let check = &v["expect"];
    let kind = check["kind"].as_str().unwrap_or("equals");
    let mut choices = vec![("equals", "Equals"), ("oneOf", "One of")];
    if references::unsigned(f.field.field_type)
        || references::signed(f.field.field_type)
        || matches!(f.field.field_type, FieldType::F32 | FieldType::F64)
    {
        choices.push(("range", "Inclusive range"));
    }
    let mut c = vec![select("expect-kind", "Check", kind, &choices)];
    let literal = |key, label, value: &Value| {
        if f.field.field_type == FieldType::Bool {
            select(
                key,
                label,
                if *value == true { "true" } else { "false" },
                &[("false", "false"), ("true", "true")],
            )
        } else {
            text(key, label, value)
        }
    };
    if kind == "equals" {
        c.push(literal(
            "expect-value",
            if f.field.field_type == FieldType::Bytes {
                "Expected hex bytes"
            } else {
                "Expected value"
            },
            &check["value"]["value"],
        ));
    }
    if kind == "range" {
        c.push(text("expect-min", "Minimum", &check["min"]["value"]));
        c.push(text("expect-max", "Maximum", &check["max"]["value"]));
    }
    grid(ui, s, f, scope, state, c, events);
    if kind == "oneOf" {
        for (i, value) in check["values"].as_array().into_iter().flatten().enumerate() {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2((ui.available_width() - 32.0).max(1.0), 28.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let mut c = literal("expect-allowed", "", &value["value"]);
                        c.index = Some(i);
                        show_control(ui, s, f, scope, state, &c, events);
                    },
                );
                if StyledButton::show(ui, s, &ButtonSpec::new("×").width(25.0)).clicked() {
                    events.push(FieldEvent::Input {
                        id: f.id,
                        path: f.locator.clone(),
                        control: "remove-expected".into(),
                        index: Some(i),
                        text: String::new(),
                    });
                }
            });
        }
        if StyledButton::show(
            ui,
            s,
            &ButtonSpec {
                disabled_reason: (check["values"].as_array().map_or(0, Vec::len) >= 256)
                    .then_some("The allowed-value limit is 256."),
                font_size: Some(11.0),
                ..ButtonSpec::new("+ Value")
            },
        )
        .clicked()
        {
            events.push(FieldEvent::Input {
                id: f.id,
                path: f.locator.clone(),
                control: "add-expected".into(),
                index: None,
                text: String::new(),
            });
        }
    }
}
fn number(text: &str) -> Result<Value, EditRejected> {
    let value = if text.trim().is_empty() {
        0.0
    } else {
        text.parse::<f64>()
            .map_err(|_| EditRejected("Enter a nonnegative whole number.".into()))?
    };
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(EditRejected(
            "Enter a nonnegative safe whole number.".into(),
        ));
    }
    Ok(json!(value as u64))
}
fn expected(t: FieldType, input: Option<&str>) -> Value {
    let ty = if references::unsigned(t) {
        "unsigned"
    } else if references::signed(t) {
        "signed"
    } else {
        match t {
            FieldType::F32 | FieldType::F64 => "float",
            FieldType::Bool => "bool",
            FieldType::Bytes => "bytes",
            _ => "string",
        }
    };
    let raw = input.unwrap_or(match ty {
        "bytes" => "00",
        "string" => "",
        _ => "0",
    });
    json!({"type":ty,"value":if ty=="bool"{json!(raw=="true")}else{json!(raw)}})
}
/// Apply each input to the LATEST model, not the render-time JSON snapshot.
/// This keeps two controls edited within one frame from replacing one another.
pub fn apply_input(
    model: &mut EditorModel,
    id: FieldId,
    path: FieldPath,
    key: &str,
    index: Option<usize>,
    raw: &str,
) -> Result<EditOutcome, EditRejected> {
    let result = (|| {
        if key == "name" {
            return model.edit(TemplateEdit::Rename {
                path: path.clone(),
                name: raw.into(),
            });
        }
        if key == "type" {
            let t = serde_json::from_value(json!(raw)).map_err(|e| EditRejected(e.to_string()))?;
            return model.edit(TemplateEdit::Type {
                path: path.clone(),
                field_type: t,
            });
        }
        let field = model
            .field(&path)
            .ok_or_else(|| EditRejected("The field no longer exists.".into()))?;
        let mut v = serde_json::to_value(&field).unwrap();
        let index = index.unwrap_or(0);
        let property = match key {
            "placement-mode" => {
                if raw == "sequential" {
                    v.as_object_mut().unwrap().remove("placement");
                } else {
                    let offset = v["placement"].get("offset").cloned().unwrap_or(json!("0"));
                    v["placement"] = json!({"mode":raw,"offset":offset});
                }
                "placement"
            }
            "offset-source" => {
                v["placement"]["offset"] = if raw == "ref" {
                    json!({"ref":""})
                } else {
                    json!("0")
                };
                "placement"
            }
            "placement-offset" => {
                v["placement"]["offset"] = json!(raw);
                "placement"
            }
            "offset-ref" => {
                v["placement"]["offset"]["ref"] = json!(raw);
                "placement"
            }
            "offset-add" => {
                if raw.is_empty() {
                    v["placement"]["offset"]
                        .as_object_mut()
                        .unwrap()
                        .remove("add");
                } else {
                    v["placement"]["offset"]["add"] = json!(raw);
                }
                "placement"
            }
            "endianness" => {
                if raw.is_empty() {
                    v.as_object_mut().unwrap().remove("endianness");
                } else {
                    v["endianness"] = json!(raw);
                }
                "endianness"
            }
            "encoding" => {
                v["encoding"] = json!(raw);
                "encoding"
            }
            "length" => {
                v["length"] = number(raw)?;
                "length"
            }
            "text-length" => {
                let p = if v.get("maxLength").is_some() {
                    "maxLength"
                } else {
                    "length"
                };
                v[p] = number(raw)?;
                p
            }
            "text-mode" => {
                let bound = if v["length"].is_object() {
                    v["length"]["max"].clone()
                } else {
                    v.get("length")
                        .or_else(|| v.get("maxLength"))
                        .cloned()
                        .unwrap_or(json!(1))
                };
                // Both bound properties must change atomically: neither an
                // invalid two-bound field nor a boundless field is published.
                let remove = if raw == "fixed" {
                    "maxLength"
                } else {
                    "length"
                };
                let add = if raw == "fixed" {
                    "length"
                } else {
                    "maxLength"
                };
                let mut next = model.clone();
                next.edit(TemplateEdit::Patch {
                    path: path.clone(),
                    property: remove.into(),
                    value: None,
                })?;
                let outcome = next.edit(TemplateEdit::Patch {
                    path: path.clone(),
                    property: add.into(),
                    value: Some(bound),
                })?;
                *model = next;
                return Ok(outcome);
            }
            "length-source" => {
                v["length"] = if raw == "ref" {
                    json!({"ref":"","max":256})
                } else {
                    json!(1)
                };
                "length"
            }
            "length-ref" => {
                v["length"]["ref"] = json!(raw);
                "length"
            }
            "length-max" => {
                v["length"]["max"] = number(raw)?;
                "length"
            }
            "count-mode" => {
                v["count"] = if raw == "ref" {
                    json!({"ref":""})
                } else {
                    json!({"fixed":1})
                };
                "count"
            }
            "count-fixed" => {
                v["count"] = json!({"fixed":number(raw)?});
                "count"
            }
            "count-ref" => {
                v["count"] = json!({"ref":raw});
                "count"
            }
            "align" => {
                if raw.is_empty() {
                    v.as_object_mut().unwrap().remove("align");
                } else {
                    v["align"] = number(raw)?;
                }
                "align"
            }
            "comment" => {
                v["comment"] = json!(raw);
                "comment"
            }
            "condition-toggle" => {
                if raw == "true" {
                    v["condition"] =
                        json!({"ref":"","op":"eq","value":{"type":"unsigned","value":"0"}});
                } else {
                    v.as_object_mut().unwrap().remove("condition");
                }
                "condition"
            }
            "condition-ref" => {
                v["condition"]["ref"] = json!(raw);
                "condition"
            }
            "condition-op" => {
                v["condition"]["op"] = json!(raw);
                "condition"
            }
            "condition-type" => {
                v["condition"]["value"] =
                    json!({"type":raw,"value":if raw=="bool"{json!(false)}else{json!("0")}});
                "condition"
            }
            "condition-value" => {
                v["condition"]["value"]["value"] = if v["condition"]["value"]["type"] == "bool" {
                    json!(raw == "true")
                } else {
                    json!(raw)
                };
                "condition"
            }
            "expect-toggle" | "expect-kind" => {
                if key == "expect-toggle" && raw != "true" {
                    v.as_object_mut().unwrap().remove("expect");
                } else {
                    let kind = if key == "expect-toggle" {
                        "equals"
                    } else {
                        raw
                    };
                    let lit = expected(field.field_type, None);
                    v["expect"] = match kind {
                        "oneOf" => json!({"kind":kind,"values":[lit]}),
                        "range" => json!({"kind":kind,"min":lit,"max":lit}),
                        _ => json!({"kind":kind,"value":lit}),
                    };
                }
                "expect"
            }
            "expect-value" => {
                v["expect"]["value"] = expected(field.field_type, Some(raw));
                "expect"
            }
            "expect-min" | "expect-max" => {
                v["expect"][if key == "expect-min" { "min" } else { "max" }] =
                    expected(field.field_type, Some(raw));
                "expect"
            }
            "expect-allowed" => {
                v["expect"]["values"][index] = expected(field.field_type, Some(raw));
                "expect"
            }
            "add-expected" => {
                let values = v["expect"]["values"]
                    .as_array_mut()
                    .ok_or_else(|| EditRejected("This check has no allowed-values list.".into()))?;
                if values.len() < 256 {
                    values.push(expected(field.field_type, None));
                }
                "expect"
            }
            "remove-expected" => {
                if let Some(values) = v["expect"]["values"].as_array_mut() {
                    if index < values.len() {
                        values.remove(index);
                    }
                }
                "expect"
            }
            "add-enum" | "remove-enum" | "enum-key" | "enum-label" => {
                let mut labels = field.enum_labels.unwrap_or_default();
                let mut keys = labels.keys().cloned().collect::<Vec<_>>();
                keys.sort_by_key(|key| enum_order(key));
                if key == "add-enum" {
                    let mut n = 0u64;
                    while labels.contains_key(&n.to_string()) {
                        n += 1;
                    }
                    labels.insert(n.to_string(), format!("Label {n}"));
                } else if let Some(old) = keys.get(index) {
                    if key == "enum-key" {
                        return model.edit(TemplateEdit::EnumKey {
                            path: path.clone(),
                            old: old.clone(),
                            new: raw.into(),
                        });
                    } else if key == "enum-label" {
                        labels.insert(old.clone(), raw.into());
                    } else {
                        labels.remove(old);
                    }
                }
                if labels.is_empty() {
                    v.as_object_mut().unwrap().remove("enumLabels");
                } else {
                    v["enumLabels"] = json!(labels);
                }
                "enumLabels"
            }
            "add-flag" | "remove-flag" | "flag-bit" | "flag-name" => {
                let mut flags = v["bitFlags"].as_array().cloned().unwrap_or_default();
                if key == "add-flag" {
                    let mut bit = 0u64;
                    while flags.iter().any(|f| f["bit"] == bit) {
                        bit += 1;
                    }
                    flags.push(json!({"bit":bit,"name":format!("flag{bit}")}));
                } else if index < flags.len() {
                    match key {
                        "remove-flag" => {
                            flags.remove(index);
                        }
                        "flag-bit" => flags[index]["bit"] = number(raw)?,
                        _ => flags[index]["name"] = json!(raw),
                    }
                }
                if flags.is_empty() {
                    v.as_object_mut().unwrap().remove("bitFlags");
                } else {
                    v["bitFlags"] = json!(flags);
                }
                "bitFlags"
            }
            _ => return Err(EditRejected("The field control is unavailable.".into())),
        };
        model.edit(TemplateEdit::Patch {
            path: path.clone(),
            property: property.into(),
            value: v.get(property).cloned(),
        })
    })();
    match &result {
        Ok(_) => model.raw.clear(id, key, index),
        Err(error) => model.rejected_input(id, key, index, raw, &error.0),
    }
    result
}
