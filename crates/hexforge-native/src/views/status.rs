use crate::style::{fonts, HexForgeStyle};
use eframe::egui::{self, Align2};
use hexforge_controller::{
    command::Command,
    controller::{ControllerEvent, InvokeContext, MainSnapshot},
    operation::OperationKind,
};
pub fn show(ui: &mut egui::Ui, style: &HexForgeStyle, s: &MainSnapshot) -> Vec<ControllerEvent> {
    let mut events = vec![];
    let r = ui.max_rect();
    let p = ui.painter().clone();
    p.rect_filled(r, 0, style.palette.surface);
    p.hline(
        r.x_range(),
        r.top() + 0.5,
        egui::Stroke::new(1.0, style.palette.border),
    );
    let size = s
        .file
        .as_ref()
        .and_then(|f| f.size.parse::<u64>().ok())
        .unwrap_or(0);
    let selected = s.selection.as_ref().map(|selection| {
        let value = s.page.as_ref().and_then(|page| {
            page.offset.parse::<u64>().ok().and_then(|base| {
                selection
                    .start
                    .checked_sub(base)
                    .and_then(|n| usize::try_from(n).ok())
                    .and_then(|n| page.bytes.get(n).copied())
            })
        });
        (selection.start, value)
    });
    let mut x = r.left();
    let values = [
        format!("Size {}", super::file_strip::format_size(size, false)),
        format!(
            "Offset {}",
            selected.map_or_else(|| "—".into(), |(offset, _)| format!("0x{offset:X}"))
        ),
        format!(
            "Byte {}",
            selected.and_then(|(_, v)| v).map_or_else(
                || "—".into(),
                |b| format!(
                    "{b:02X} '{}'",
                    if (32..=126).contains(&b) {
                        char::from(b)
                    } else {
                        '.'
                    }
                )
            )
        ),
        format!(
            "Selected {}",
            s.selection.as_ref().map_or(0, |range| range.count())
        ),
    ];
    for text in values {
        x = span(&p, r, x, &text, style, style.palette.muted);
    }
    let label = p.layout_no_wrap("Rows ".into(), fonts::regular(11.0), style.palette.muted);
    p.galley(
        egui::pos2(x + 8.0, r.center().y + 1.0 - label.size().y / 2.0),
        label.clone(),
        style.palette.muted,
    );
    x += 8.0 + label.size().x;
    for (width, command) in [(16, Command::Row16), (32, Command::Row32)] {
        let rect = egui::Rect::from_min_size(egui::pos2(x, r.top() + 5.0), egui::vec2(19.2, 16.0));
        let response = ui.interact(rect, ui.id().with(width), egui::Sense::click());
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                width.to_string(),
            );
            info.selected = Some(s.row_width.bytes() == width);
            info
        });
        if response.hovered() || s.row_width.bytes() == width {
            p.rect_filled(rect, 2, style.palette.hover);
        }
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            width.to_string(),
            fonts::regular(11.0),
            if response.hovered() || s.row_width.bytes() == width {
                style.palette.text
            } else {
                style.palette.muted
            },
        );
        if response.clicked() {
            events.push(ControllerEvent::Invoke {
                command,
                context: InvokeContext::default(),
            });
        }
        x += 19.2;
        if width == 16 {
            p.text(
                egui::pos2(x, r.center().y + 1.0),
                Align2::LEFT_CENTER,
                "/",
                fonts::regular(11.0),
                style.palette.muted,
            );
            x += 6.6;
        }
    }
    p.vline(
        x + 8.5,
        egui::Rangef::new(r.center().y - 7.0, r.center().y + 9.0),
        egui::Stroke::new(1.0, style.palette.border),
    );
    x += 9.0;
    x = span(
        &p,
        r,
        x,
        if s.edit_mode { "Edit" } else { "Read-only" },
        style,
        style.palette.muted,
    );
    x = span(
        &p,
        r,
        x,
        if s.template.default_endianness == hexforge_core::template::Endian::Little {
            "Parse: LE"
        } else {
            "Parse: BE"
        },
        style,
        style.palette.muted,
    );
    let dirty = s.file.as_ref().is_some_and(|f| f.dirty);
    let text = if dirty { "Modified" } else { "Clean" };
    let dirty_width = p
        .layout_no_wrap(text.into(), fonts::regular(11.0), style.palette.muted)
        .size()
        .x
        + 16.0;
    p.text(
        egui::pos2(r.right() - 8.0, r.center().y + 1.0),
        Align2::RIGHT_CENTER,
        text,
        fonts::regular(11.0),
        if dirty {
            style.palette.modified
        } else {
            style.palette.muted
        },
    );
    let progress = s
        .activity
        .as_ref()
        .map(|a| {
            let label = match a.operation {
                OperationKind::Open => "Opening file",
                OperationKind::Close => "Closing file",
                OperationKind::Search => "Searching bytes",
                OperationKind::Parse => "Parsing template",
                OperationKind::Save => "Saving copy",
                OperationKind::Export => "Exporting CSV",
                OperationKind::Template => "Working with template",
                OperationKind::Edit => "Applying edit",
                OperationKind::Undo => "Undoing edit",
                OperationKind::Page => "Loading bytes",
                _ => "",
            };
            a.progress.as_ref().map_or_else(
                || label.to_owned(),
                |v| {
                    if v.phase == hexforge_core::progress::OperationPhase::Parse && v.total == "0" {
                        format!("{label} · {} nodes processed", v.processed)
                    } else {
                        format!("{label} · {} / {}", v.processed, v.total)
                    }
                },
            )
        })
        .or_else(|| s.completion_notice.clone());
    if let Some(progress) = progress {
        super::results::cell(
            &p,
            egui::Rect::from_min_max(
                egui::pos2(x + 8.0, r.top()),
                egui::pos2((r.right() - dirty_width).max(x + 8.0), r.bottom()),
            ),
            &progress,
            fonts::regular(11.0),
            style.palette.text,
        );
    }
    events
}
fn span(
    p: &egui::Painter,
    r: egui::Rect,
    x: f32,
    text: &str,
    style: &HexForgeStyle,
    color: egui::Color32,
) -> f32 {
    let galley = p.layout_no_wrap(text.into(), fonts::regular(11.0), color);
    // CSS span padding and its 1px border both belong to the flow width.
    let width = galley.size().x + 17.0;
    p.galley(
        egui::pos2(x + 8.0, r.center().y + 1.0 - galley.size().y / 2.0),
        galley,
        color,
    );
    p.vline(
        x + width - 0.5,
        egui::Rangef::new(r.center().y - 6.0, r.center().y + 8.0),
        egui::Stroke::new(1.0, style.palette.border),
    );
    x + width
}
