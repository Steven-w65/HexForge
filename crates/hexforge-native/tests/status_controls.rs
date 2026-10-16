mod support;
use eframe::egui;
use hexforge_controller::{
    command::Command,
    controller::{
        AppController, ByteSelection, ControllerEvent, InitialState, MainSnapshot, ViewportPage,
        WindowSnapshot,
    },
    operation::{OperationActivity, OperationKind},
    shortcuts::WindowKind,
};
use hexforge_core::{
    dto::{FileInfoDto, PageResponse},
    progress::{OperationPhase, OperationProgress},
    template::Endian,
};
use hexforge_native::{style::HexForgeStyle, views::status};
use support::ui_harness::{CapturedFrame, NativeUiHarness};

fn snapshot() -> MainSnapshot {
    let c = AppController::new(InitialState::default());
    let WindowSnapshot::Main(mut s) = c.snapshot(WindowKind::Main) else {
        unreachable!()
    };
    s.file = Some(FileInfoDto {
        name: "sample.bin".into(),
        path: "sample.bin".into(),
        size: "4096".into(),
        revision: "0".into(),
        dirty: false,
    });
    s
}
fn frame(
    h: &NativeUiHarness,
    s: &MainSnapshot,
    events: Vec<egui::Event>,
) -> (CapturedFrame, Vec<ControllerEvent>) {
    let style = HexForgeStyle::for_theme(s.theme, 1.0);
    let mut actions = vec![];
    let f = h.frame(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 24.0),
            )),
            events,
            ..Default::default()
        },
        |ui| actions.extend(status::show(ui, &style, s)),
    );
    (f, actions)
}
fn text(f: &CapturedFrame) -> Vec<String> {
    f.output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => Some(t.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn selected_byte_status_uses_the_actual_page_and_printable_ascii() {
    let h = NativeUiHarness::new();
    let mut s = snapshot();
    s.selection = Some(ByteSelection { start: 1, end: 1 });
    s.page = Some(ViewportPage {
        data: PageResponse {
            offset: "1".into(),
            bytes: vec![0xff, 0x41],
            modified_offsets: vec![],
            revision: "0".into(),
        },
        generation: 1,
    });
    let (f, _) = frame(&h, &s, vec![]);
    let values = text(&f);
    assert!(values.iter().any(|v| v == "Byte FF '.'"));
    assert!(values.iter().any(|v| v == "Offset 0x1"));
    assert!(values.iter().any(|v| v == "Selected 1"));
    s.selection = Some(ByteSelection { start: 2, end: 2 });
    let (f, _) = frame(&h, &s, vec![]);
    assert!(text(&f).iter().any(|v| v == "Byte 41 'A'"));
}
#[test]
fn status_keeps_exact_offset_while_its_byte_page_is_missing_or_elsewhere() {
    let h = NativeUiHarness::new();
    let mut s = snapshot();
    for offset in [100, 9_007_199_254_740_993] {
        s.selection = Some(ByteSelection {
            start: offset,
            end: offset,
        });
        for elsewhere in [false, true] {
            s.page = elsewhere.then(|| ViewportPage {
                data: PageResponse {
                    offset: "0".into(),
                    bytes: vec![0x41],
                    modified_offsets: vec![],
                    revision: "0".into(),
                },
                generation: 1,
            });
            let (f, _) = frame(&h, &s, vec![]);
            let values = text(&f);
            assert!(values.iter().any(|v| v
                == if offset == 100 {
                    "Offset 0x64"
                } else {
                    "Offset 0x20000000000001"
                }));
            assert!(values.iter().any(|v| v == "Byte —"));
            assert!(values.iter().any(|v| v == "Selected 1"));
        }
    }
}
#[test]
fn status_paints_dirty_endianness_progress_and_dispatches_only_row_width() {
    let h = NativeUiHarness::new();
    let mut s = snapshot();
    s.file.as_mut().unwrap().dirty = true;
    s.edit_mode = true;
    s.template.default_endianness = Endian::Big;
    s.activity = Some(OperationActivity {
        operation: OperationKind::Save,
        progress: Some(OperationProgress {
            operation_id: "1".into(),
            phase: OperationPhase::Save,
            processed: "64".into(),
            total: "128".into(),
        }),
    });
    let (f, _) = frame(&h, &s, vec![]);
    let values = text(&f);
    for wanted in [
        "Size 4 KiB",
        "Edit",
        "Parse: BE",
        "Modified",
        "Saving copy · 64 / 128",
    ] {
        assert!(
            values.iter().any(|v| v == wanted),
            "missing {wanted}: {values:?}"
        );
    }
    let point = f
        .widgets
        .iter()
        .find(|w| w.label == "32")
        .unwrap()
        .rect
        .center();
    frame(&h, &s, vec![egui::Event::PointerMoved(point)]);
    let mut all = vec![];
    for pressed in [true, false] {
        let (_, actions) = frame(
            &h,
            &s,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        all.extend(actions);
    }
    assert!(matches!(
        all.as_slice(),
        [ControllerEvent::Invoke {
            command: Command::Row32,
            ..
        }]
    ));
    s.activity = None;
    let (f, _) = frame(&h, &s, vec![]);
    assert!(!text(&f).iter().any(|v| v.contains("Saving copy")));
}

#[test]
fn native_status_uses_nested_node_progress_without_a_false_zero_denominator() {
    let h = NativeUiHarness::new();
    let mut s = snapshot();
    s.activity = Some(OperationActivity {
        operation: OperationKind::Parse,
        progress: Some(OperationProgress {
            operation_id: "nested-parse".into(),
            phase: OperationPhase::Parse,
            processed: "128".into(),
            total: "0".into(),
        }),
    });
    let (f, actions) = frame(&h, &s, vec![]);
    let values = text(&f);
    assert!(values
        .iter()
        .any(|v| v == "Parsing template · 128 nodes processed"));
    assert!(!values.iter().any(|v| v.contains("/ 0")));
    assert!(actions.is_empty());
    s.activity = Some(OperationActivity {
        operation: OperationKind::Search,
        progress: None,
    });
    let (f, _) = frame(&h, &s, vec![]);
    assert!(text(&f).iter().any(|v| v == "Searching bytes"));
    s.activity = None;
    let (f, _) = frame(&h, &s, vec![]);
    assert!(!text(&f)
        .iter()
        .any(|v| v.contains("Searching bytes") || v.contains("nodes processed")));
}
