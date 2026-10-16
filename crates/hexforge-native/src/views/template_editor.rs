//! The sole native Template Editor: unchanged metadata/cards/footer, with
//! local control state and typed events. Persistence stays in the controller.
use super::field_card::{self, EditorControlState, FieldEvent, FieldSnapshot, ScopeSnapshot};
use crate::{
    style::{fonts, HexForgeStyle},
    widgets::{
        button::{ButtonRole, ButtonSpec, StyledButton},
        dialog, inert, input, scroll_area,
    },
};
use eframe::egui::{self, Align2};
use hexforge_controller::{
    command::Command,
    controller::EditorSnapshot,
    dialog::{DialogDecision, DialogRequest},
    identity::RequestId,
    policy::{command_unavailable_reason, PolicyState},
    template::{
        coordinator::{EditorAction, EditorCloseDecision},
        editing::{ConfirmationKind, EditorModel, FieldPath, TemplateEdit},
        status::{template_status, TemplateSource},
        validation::{collect_validation, validation_target, ValidationIssue},
    },
};
use hexforge_core::template::Endian;
#[derive(Debug, Clone)]
pub enum EditorUiEvent {
    Name(String),
    Endian(Endian),
    Field(FieldEvent),
    Action(EditorAction),
    Confirm,
    CancelConfirm,
    CloseDecision(EditorCloseDecision),
    DialogDecision(RequestId, DialogDecision),
    ClearError,
}
pub fn editor_policy(s: &EditorSnapshot, model: &EditorModel) -> PolicyState {
    let mut policy = s.policy.clone();
    policy.template_active = s.template_state.active;
    policy.template_has_path = s.template_state.saved_path.is_some();
    policy.template_has_fields = !model.definition().fields.is_empty();
    let issues = collect_validation(&model.definition(), &model.raw);
    policy.template_valid = issues.is_empty();
    policy.template_issue_count = Some(u32::try_from(issues.len()).unwrap_or(u32::MAX));
    policy.operation_busy = s.busy;
    policy
}
fn reveal_issue(state: &mut EditorControlState, model: &EditorModel, issue: &ValidationIssue) {
    let definition = model.definition();
    let target = validation_target(issue, &definition);
    if target.control == "template-name" {
        state.reveal = Some(egui::Id::new("template-name"));
        return;
    }
    let entries = model.named_entries();
    let field = if issue.message.contains("unique in this structure") {
        entries.iter().rev().find(|(id, _, path, _)| {
            issue
                .field_id
                .map_or(*path == target.path, |wanted| *id == wanted)
        })
    } else {
        entries.iter().find(|(id, _, path, _)| {
            issue
                .field_id
                .map_or(*path == target.path, |wanted| *id == wanted)
        })
    };
    if let Some((id, _, _, _)) = field {
        state.reveal_control(model, *id, &target.control, target.index);
    }
}

fn validation_issue_galley(
    ui: &egui::Ui,
    style: &HexForgeStyle,
    issue: &ValidationIssue,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    // Source <strong> applies only to the path. The button overrides the
    // summary's orange color and wraps normally within its 5px side padding.
    let mut job = egui::text::LayoutJob::default();
    job.round_output_to_gui = false;
    job.wrap.max_width = (width - 10.0).max(1.0);
    let format = |font_id| egui::TextFormat {
        font_id,
        color: style.palette.text,
        line_height: Some(14.0),
        ..Default::default()
    };
    job.append(
        if issue.path.is_empty() {
            "Unnamed field"
        } else {
            &issue.path
        },
        0.0,
        format(fonts::bold(11.0)),
    );
    job.append(
        &format!(" · {}", issue.message),
        0.0,
        format(fonts::regular(11.0)),
    );
    ui.painter().layout_job(job)
}

pub fn show_template_editor(
    ui: &mut egui::Ui,
    s: &HexForgeStyle,
    snapshot: &EditorSnapshot,
    model: &EditorModel,
    state: &mut EditorControlState,
    error: Option<&str>,
    workflow: Option<(RequestId, &DialogRequest)>,
) -> Vec<EditorUiEvent> {
    let mut events = vec![];
    let whole = ui.max_rect();
    ui.painter().rect_filled(whole, 0, s.palette.bg);
    let content = whole.shrink(12.0);
    // CSS: 17px heading + 2px gap + 14px context + 9px padding + 1px border.
    let header = egui::Rect::from_min_size(content.min, egui::vec2(content.width(), 43.0));
    let p = ui.painter().clone();
    p.text(
        header.min + egui::vec2(0.0, 8.5),
        Align2::LEFT_CENTER,
        "Template Editor",
        fonts::bold(13.0),
        s.palette.text,
    );
    let definition = model.definition();
    let source = if !snapshot.template_state.active {
        TemplateSource::None
    } else if snapshot.template_state.saved_path.is_some() {
        TemplateSource::File
    } else {
        TemplateSource::Draft
    };
    let status = template_status(
        source,
        snapshot.template_state.dirty,
        snapshot.template_state.applied,
        snapshot.template_state.needs_refresh,
        snapshot.policy.has_file,
        has_diagnostics(&snapshot.results),
    );
    let context_name = format!(
        "{}{}",
        if definition.name.is_empty() {
            "Untitled"
        } else {
            &definition.name
        },
        snapshot
            .template_state
            .saved_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or(String::new(), |name| format!(
                " · {}",
                name.to_string_lossy()
            ))
    );
    let status = format!(" · {status}");
    let status_width = p
        .layout_job(fonts::spaced(
            &status,
            fonts::regular(11.0),
            s.palette.muted,
            0.0,
        ))
        .size()
        .x;
    let name_width = p
        .layout_job(fonts::spaced(
            &context_name,
            fonts::regular(11.0),
            s.palette.muted,
            0.0,
        ))
        .size()
        .x;
    let contextrect = egui::Rect::from_min_size(
        header.min + egui::vec2(0.0, 19.0),
        // The source's context-name flex item shrinks but does not grow: the
        // state label follows the actual name, not the far edge of the window.
        egui::vec2(
            name_width.min((header.width() - status_width).max(0.0)),
            14.0,
        ),
    );
    super::results::cell(
        &p,
        contextrect,
        &context_name,
        fonts::regular(11.0),
        s.palette.muted,
    );
    p.text(
        egui::pos2(contextrect.right(), contextrect.center().y),
        Align2::LEFT_CENTER,
        status,
        fonts::regular(11.0),
        if snapshot.template_state.dirty {
            s.palette.modified
        } else {
            s.palette.muted
        },
    );
    p.hline(
        header.x_range(),
        header.bottom() - 0.5,
        egui::Stroke::new(1.0, s.palette.border),
    );
    let issues = collect_validation(&definition, &model.raw);
    let policy = editor_policy(snapshot, model);
    let modal = snapshot.close_prompt
        || error.is_some()
        || state.confirmation.is_some()
        || workflow.is_some();
    let meta = egui::Rect::from_min_size(
        header.left_bottom() + egui::vec2(0.0, 12.0),
        egui::vec2(content.width(), 45.0),
    );
    ui.scope_builder(
        inert::builder(egui::UiBuilder::new().max_rect(meta), modal),
        |ui| {
            inert::block(ui);
            ui.spacing_mut().item_spacing = egui::vec2(9.0, 3.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2((meta.width() - 159.0).max(1.0), 45.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(egui::text::LayoutJob::single_section(
                            "Name".into(),
                            egui::TextFormat {
                                font_id: fonts::regular(11.0),
                                color: s.palette.muted,
                                line_height: Some(14.0),
                                ..Default::default()
                            },
                        ));
                        let mut name = definition.name.clone();
                        let r = input::text(
                            ui,
                            s,
                            egui::Id::new("template-name"),
                            &mut name,
                            ui.available_width(),
                            "",
                        );
                        if state.reveal == Some(r.id) {
                            r.request_focus();
                            state.reveal = None;
                        }
                        if r.changed() {
                            events.push(EditorUiEvent::Name(name));
                        }
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(150.0, 45.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(egui::text::LayoutJob::single_section(
                            "Default endian".into(),
                            egui::TextFormat {
                                font_id: fonts::regular(11.0),
                                color: s.palette.muted,
                                line_height: Some(14.0),
                                ..Default::default()
                            },
                        ));
                        let mut endian = definition.default_endianness;
                        if input::combo(
                            ui,
                            s,
                            egui::Id::new("template-endian"),
                            &mut endian,
                            &[(Endian::Little, "Little"), (Endian::Big, "Big")],
                            150.0,
                        ) {
                            events.push(EditorUiEvent::Endian(endian));
                        }
                    },
                );
            });
        },
    );
    let note = egui::Rect::from_min_size(
        meta.left_bottom() + egui::vec2(0.0, 9.0),
        egui::vec2(content.width(), 14.0),
    );
    p.text(
        note.left_center(),
        Align2::LEFT_CENTER,
        "Fields parse in order. References use earlier result paths.",
        fonts::regular(11.0),
        s.palette.address,
    );
    let mut list_top = note.bottom() + 9.0;
    if !issues.is_empty() {
        let issue_width = (content.width() - 20.0).max(1.0);
        let layout_issues = |width| {
            issues
                .iter()
                .map(|issue| validation_issue_galley(ui, s, issue, width))
                .collect::<Vec<_>>()
        };
        let mut issue_galleys = layout_issues(issue_width);
        let body_height = |galleys: &[std::sync::Arc<egui::Galley>]| {
            galleys.iter().map(|g| g.size().y + 8.0).sum::<f32>()
                + issues.len().saturating_sub(1) as f32 * 3.0
        };
        let overflow = body_height(&issue_galleys) > 110.0;
        let issue_width = issue_width - if overflow { 10.0 } else { 0.0 };
        if overflow {
            issue_galleys = layout_issues(issue_width);
        }
        let body_height = body_height(&issue_galleys).min(110.0);
        // CSS border + padding is 10px horizontally and 8px vertically.
        // A 14px summary line and naturally wrapped issue rows determine the
        // box height; only its issue body is capped at 110px and scrolls.
        let height = 30.0
            + if state.summary_collapsed {
                0.0
            } else {
                5.0 + body_height
            };
        let bounds = egui::Rect::from_min_size(
            egui::pos2(content.left(), list_top),
            egui::vec2(content.width(), height),
        );
        p.rect_stroke(
            bounds,
            5,
            egui::Stroke::new(1.0, s.palette.border),
            egui::StrokeKind::Inside,
        );
        ui.scope_builder(
            inert::builder(
                egui::UiBuilder::new().max_rect(bounds.shrink2(egui::vec2(10.0, 8.0))),
                modal,
            ),
            |ui| {
                inert::block(ui);
                let label_text = format!(
                    "{} template {}",
                    issues.len(),
                    if issues.len() == 1 { "error" } else { "errors" }
                );
                let rect = egui::Rect::from_min_size(
                    ui.max_rect().min,
                    egui::vec2(ui.available_width(), 14.0),
                );
                let r = ui.interact(
                    rect,
                    ui.id().with("template-validation-summary-toggle"),
                    egui::Sense::click(),
                );
                r.widget_info(|| {
                    let mut info = egui::WidgetInfo::labeled(
                        egui::WidgetType::CollapsingHeader,
                        ui.is_enabled(),
                        &label_text,
                    );
                    info.selected = Some(!state.summary_collapsed);
                    info
                });
                crate::widgets::disclosure::paint(
                    &p,
                    rect,
                    &label_text,
                    !state.summary_collapsed,
                    s.palette.modified,
                );
                if r.clicked() {
                    state.summary_collapsed = !state.summary_collapsed;
                }
                if !state.summary_collapsed {
                    let body = egui::Rect::from_min_size(
                        rect.left_bottom() + egui::vec2(0.0, 5.0),
                        egui::vec2(rect.width(), body_height),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.spacing_mut().scroll.floating = false;
                        ui.spacing_mut().scroll.bar_width = 6.0;
                        ui.spacing_mut().scroll.bar_inner_margin = 4.0;
                        ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                        ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
                        scroll_area::vertical(ui, s, "template-validation-summary", |ui| {
                            for (index, (issue, galley)) in
                                issues.iter().zip(issue_galleys).enumerate()
                            {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(issue_width, galley.size().y + 8.0),
                                    egui::Sense::hover(),
                                );
                                let r = ui.interact(
                                    rect,
                                    ui.id().with(("template-validation-issue", index)),
                                    egui::Sense::click(),
                                );
                                r.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        ui.is_enabled(),
                                        galley.text(),
                                    )
                                });
                                dialog::register_focus(ui, &r, ui.is_enabled());
                                if r.hovered() {
                                    ui.painter().rect_filled(rect, 3, s.palette.hover);
                                }
                                if r.has_focus() && crate::platform::input::focus_visible(ui.ctx())
                                {
                                    ui.painter().rect_stroke(
                                        rect.expand(2.0),
                                        3,
                                        egui::Stroke::new(1.0, s.palette.selection),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                ui.painter().galley(
                                    rect.min + egui::vec2(5.0, 4.0),
                                    galley,
                                    s.palette.text,
                                );
                                if r.clicked() {
                                    reveal_issue(state, model, issue);
                                }
                            }
                        });
                    });
                }
            },
        );
        list_top += height + 9.0;
    }
    let notice_height = if snapshot.notice.is_some() { 28.5 } else { 0.0 };
    let footer = egui::Rect::from_min_size(
        content.left_bottom() - egui::vec2(0.0, 38.0 + notice_height),
        egui::vec2(content.width(), 38.0),
    );
    let list = egui::Rect::from_min_max(
        egui::pos2(content.left(), list_top),
        egui::pos2(content.right(), (footer.top() - 9.0).max(list_top)),
    );
    ui.scope_builder(
        inert::builder(egui::UiBuilder::new().max_rect(list), modal),
        |ui| {
            inert::block(ui);
            ui.spacing_mut().item_spacing.y = 8.0;
            // CSS overflow:auto reserves the Windows thin scrollbar's 10px slot
            // only when needed. A floating egui bar must not overlap field cards,
            // and the 3px list padding is additional to that slot.
            ui.spacing_mut().scroll.floating = false;
            ui.spacing_mut().scroll.bar_width = 6.0;
            ui.spacing_mut().scroll.bar_inner_margin = 4.0;
            ui.spacing_mut().scroll.bar_outer_margin = 0.0;
            ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
            scroll_area::vertical(ui, s, "template-field-list", |ui| {
                ui.set_width((ui.available_width() - 3.0).max(1.0));
                let scope = ScopeSnapshot {
                    definition: &definition,
                    model,
                    issues: &issues,
                    results: &snapshot.results,
                };
                for (index, field) in definition.fields.iter().enumerate() {
                    let locator = FieldPath::root(index);
                    if let Some(id) = model.id(&locator) {
                        events.extend(
                            field_card::show_field_card(
                                ui,
                                s,
                                &FieldSnapshot {
                                    id,
                                    locator,
                                    path: field.name.clone().unwrap_or_default(),
                                    field: field.clone(),
                                    depth: 0,
                                    index,
                                    siblings: definition.fields.len(),
                                    element: false,
                                },
                                &scope,
                                state,
                            )
                            .into_iter()
                            .map(EditorUiEvent::Field),
                        );
                    }
                }
                if !definition.fields.is_empty() {
                    // Materialize the pending 8px sibling gap as the last
                    // card's CSS margin; add_space(8) would count it twice.
                    ui.add_space(0.0);
                }
                if definition.fields.is_empty() {
                    ui.add_space(12.0);
                    ui.label(egui::text::LayoutJob::single_section(
                        "No fields defined. Add a field to start a template.".into(),
                        egui::TextFormat {
                            font_id: fonts::regular(12.0),
                            color: s.palette.muted,
                            line_height: Some(16.0),
                            ..Default::default()
                        },
                    ));
                }
            });
        },
    );
    p.hline(
        footer.x_range(),
        footer.top() + 0.5,
        egui::Stroke::new(1.0, s.palette.border),
    );
    ui.scope_builder(
        inert::builder(egui::UiBuilder::new().max_rect(footer), modal),
        |ui| {
            inert::block(ui);
            ui.add_space(11.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if StyledButton::show(ui, s, &ButtonSpec::new("+ Field")).clicked() {
                    events.push(EditorUiEvent::Field(FieldEvent::Edit(TemplateEdit::Add {
                        parent: None,
                    })));
                }
                for (label, command) in [
                    ("Load…", Command::LoadTemplate),
                    ("Unload", Command::UnloadTemplate),
                    ("Save", Command::SaveTemplate),
                    ("Save As…", Command::SaveTemplateAs),
                ] {
                    let reason = command_unavailable_reason(command, &policy);
                    if StyledButton::show(
                        ui,
                        s,
                        &ButtonSpec {
                            disabled_reason: reason.as_deref(),
                            ..ButtonSpec::new(label)
                        },
                    )
                    .clicked()
                    {
                        events.push(EditorUiEvent::Action(EditorAction::Command(command)));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let reason = command_unavailable_reason(Command::ApplyTemplate, &policy);
                    if StyledButton::show(
                        ui,
                        s,
                        &ButtonSpec {
                            disabled_reason: reason.as_deref(),
                            ..ButtonSpec::new("Apply Template").role(ButtonRole::Primary)
                        },
                    )
                    .clicked()
                    {
                        events.push(EditorUiEvent::Action(EditorAction::Command(
                            Command::ApplyTemplate,
                        )));
                    }
                });
            });
        },
    );
    if let Some(notice) = &snapshot.notice {
        p.text(
            content.left_bottom() - egui::vec2(0.0, 8.25),
            Align2::LEFT_CENTER,
            notice,
            fonts::regular(11.0),
            s.palette.text,
        );
    }
    // The reference renders the parent prompt and a later error together.
    // Keep its controls painted, but never accept a covered-layer decision,
    // including on the first frame before egui has settled its area ordering.
    let parent_interactive = error.is_none();
    if let Some((id, request)) = workflow {
        let mut decision = None;
        let r = dialog::show_in_stack(
            ui.ctx(),
            s,
            egui::Id::new("editor-workflow-dialog"),
            &request.title,
            &request.message,
            parent_interactive,
            |ui| {
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (label, d) in [
                        ("Cancel", DialogDecision::Cancel),
                        ("OK", DialogDecision::Confirm),
                    ] {
                        if StyledButton::show(ui, s, &ButtonSpec::new(label)).clicked() {
                            decision = Some(d);
                        }
                    }
                });
            },
        );
        if r.cancel {
            decision = Some(DialogDecision::Cancel);
        }
        if let Some(decision) = decision.filter(|_| parent_interactive) {
            events.push(EditorUiEvent::DialogDecision(id, decision));
        }
    } else if snapshot.close_prompt {
        let mut decision = None;
        let r = dialog::show_in_stack(
            ui.ctx(),
            s,
            egui::Id::new("editor-unsaved-close"),
            "Unsaved template edits",
            "Save changes made in this Template Editor before closing?",
            parent_interactive,
            |ui| {
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (label, d, command) in [
                        ("Cancel", EditorCloseDecision::Cancel, None),
                        ("Don't Save", EditorCloseDecision::Discard, None),
                        (
                            "Save As",
                            EditorCloseDecision::SaveAs,
                            Some(Command::SaveTemplateAs),
                        ),
                        (
                            "Save",
                            EditorCloseDecision::Save,
                            Some(Command::SaveTemplate),
                        ),
                    ] {
                        let reason = command.and_then(|c| command_unavailable_reason(c, &policy));
                        let response = StyledButton::show(
                            ui,
                            s,
                            &ButtonSpec {
                                disabled_reason: reason.as_deref(),
                                ..ButtonSpec::new(label)
                            },
                        );
                        if matches!(d, EditorCloseDecision::Cancel) {
                            dialog::autofocus(ui, &response);
                        }
                        if response.clicked() {
                            decision = Some(d);
                        }
                    }
                });
            },
        );
        if r.cancel {
            decision = Some(EditorCloseDecision::Cancel);
        }
        if let Some(decision) = decision.filter(|_| parent_interactive) {
            events.push(EditorUiEvent::CloseDecision(decision));
        }
    } else if let Some(outcome) = &state.confirmation {
        let Some(confirm) = &outcome.confirmation else {
            return events;
        };
        let references = confirm.kind == ConfirmationKind::References;
        let title = match confirm.kind {
            ConfirmationKind::Remove => "Remove field?",
            ConfirmationKind::Type => "Change field type?",
            ConfirmationKind::References => "References will be affected",
        };
        let message = if references {
            format!("This change removes or changes an earlier reference used by:\n{}\n\nContinue only if you intend to repair these references before saving or applying.",outcome.affected_references.join("\n"))
        } else {
            format!("This will remove {} for '{}', including nested fields or rules. This cannot be undone.",if confirm.kind==ConfirmationKind::Remove{"the field"}else{"type-specific configuration"},confirm.field_name)
        };
        let mut approved = false;
        let mut cancelled = false;
        let r = dialog::show_in_stack(
            ui.ctx(),
            s,
            egui::Id::new("editor-field-confirmation"),
            title,
            &message,
            parent_interactive,
            |ui| {
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    cancelled = StyledButton::show(ui, s, &ButtonSpec::new("Cancel")).clicked();
                    approved = StyledButton::show(
                        ui,
                        s,
                        &ButtonSpec::new(if references {
                            "Continue"
                        } else if confirm.kind == ConfirmationKind::Remove {
                            "Remove Field"
                        } else {
                            "Change Type"
                        }),
                    )
                    .clicked();
                });
            },
        );
        if approved && parent_interactive {
            events.push(EditorUiEvent::Confirm);
        }
        if parent_interactive && (cancelled || r.cancel) {
            events.push(EditorUiEvent::CancelConfirm);
        }
    }
    if let Some(error) = error {
        if dialog::show(
            ui.ctx(),
            s,
            egui::Id::new("editor-action-error"),
            "Template action failed",
            error,
            |_| {},
        )
        .cancel
        {
            events.push(EditorUiEvent::ClearError);
        }
    }
    events
}
fn has_diagnostics(nodes: &[hexforge_core::template::ParsedNode]) -> bool {
    nodes
        .iter()
        .any(|node| !node.diagnostics.is_empty() || has_diagnostics(&node.children))
}
