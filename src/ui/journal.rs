use egui::{Align, Layout, Margin, RichText, Sense, vec2};

use crate::app::{EntryForm, FilterRange, UndoItem, WorklogApp, today};
use crate::db::LogEntryRow;
use crate::report::fmt_hours;
use crate::ui;
use crate::ui::theme::{self as t, Weight};
use crate::ui::widgets::{self as w, icon};

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    capture_card(app, ui_);
    ui_.add_space(18.0);
    filter_row(app, ui_);
    ui_.add_space(16.0);
    let show_panel = app.show_notes_panel;
    ui::notes::with_side_panel(
        app,
        ui_,
        "journal/notes_panel",
        show_panel,
        ui::notes::PANEL_WIDTH,
        |app, ui_| {
            let entry = app
                .selected_entry_id
                .and_then(|id| app.entries.iter().find(|r| r.entry.id == id).cloned());
            ui::notes::panel(app, ui_, entry);
        },
        entry_list,
    );
}

/// The fast-capture card. Enter anywhere in it saves.
/// (`app.capture` is moved out for the frame because `entry_form_fields`
/// needs `&mut app` alongside the form.)
fn capture_card(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    w::card_frame()
        .inner_margin(Margin::symmetric(20, 18))
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.horizontal(|ui_| {
                ui_.label(w::semibold("Log work", 15.0));
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    ui_.spacing_mut().item_spacing.x = 5.0;
                    ui_.label(RichText::new("to add").size(12.0).color(t::MUTED));
                    w::kbd(ui_, "Enter");
                    ui_.label(RichText::new("Press").size(12.0).color(t::MUTED));
                });
            });
            ui_.add_space(6.0);

            let mut form = std::mem::replace(&mut app.capture, EntryForm::new(None, today()));
            ui_.horizontal_wrapped(|ui_| {
                ui_.spacing_mut().item_spacing = vec2(12.0, 12.0);
                let submit = ui::entry_form_fields(
                    app,
                    ui_,
                    &mut form,
                    "capture",
                    Some(ui::FOCUS_CAPTURE_DESC),
                    None,
                    true,
                    250.0,
                );
                let valid = form.is_valid();
                let clicked = w::labeled(ui_, " ", |ui_| {
                    ui_.add_enabled(
                        valid,
                        w::primary(format!("{}  Add entry", icon::PLUS)).min_size(vec2(0.0, 36.0)),
                    )
                    .on_hover_text("Enter also saves")
                    .clicked()
                });
                if (clicked || (submit && valid)) && app.save_entry_form(&form) {
                    form = EntryForm::new(form.project_id, form.work_date);
                    app.request_focus(ui::FOCUS_CAPTURE_DESC);
                }
            });
            app.capture = form;
        });
}

fn filter_row(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 10.0;
        let projects = app.projects.clone();
        if ui::project_filter_combo(ui_, "journal/filter_proj", &projects, &mut app.filter_project)
        {
            app.reload_entries();
        }
        if let Some(day) = app.filter_day {
            let text = format!("{}  {}", day.format("%a, %b %-d"), icon::X);
            if ui_
                .add(
                    egui::Button::new(w::rich(&text, t::font(13.0, Weight::Medium), t::ACCENT_TEXT))
                        .fill(t::accent_tint())
                        .stroke(egui::Stroke::new(1.0, t::alpha(t::ACCENT, 0.4)))
                        .corner_radius(t::R_INPUT)
                        .min_size(vec2(0.0, 30.0)),
                )
                .on_hover_text("Showing one day (from the calendar) — click to go back to the range")
                .clicked()
            {
                app.filter_day = None;
                app.reload_entries();
            }
        }
        let mut range = app.filter_range;
        egui::ComboBox::from_id_salt("journal/filter_range")
            .selected_text(capitalized(range.label()))
            .show_ui(ui_, |ui_| {
                for r in FilterRange::ALL {
                    ui_.selectable_value(&mut range, r, capitalized(r.label()));
                }
            });
        if range != app.filter_range {
            app.filter_range = range;
            app.filter_day = None;
            app.reload_entries();
        }
        if search_field(
            ui_,
            &mut app.filter_text,
            "Search text or project…",
            300.0,
            None,
        )
        .changed()
        {
            app.reload_entries();
        }
        ui_.add_space(4.0);
        notes_panel_toggle(app, ui_);

        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
            let total: f64 = app.entries.iter().map(|e| e.entry.hours).sum();
            ui_.spacing_mut().item_spacing.x = 4.0;
            ui_.label(w::mono_semi(format!("{} h", fmt_hours(total)), 13.0).color(t::TEXT));
            ui_.label(
                RichText::new(format!("{} entries ·", app.entries.len()))
                    .size(13.0)
                    .color(t::MUTED),
            );
        });
    });
}

pub fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The "Notes panel" switch (Journal and Tasks share the setting).
pub fn notes_panel_toggle(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let mut panel = app.show_notes_panel;
    w::toggle(ui_, &mut panel, "Notes panel")
        .on_hover_text("Show the selected task's notes beside the list");
    if panel != app.show_notes_panel {
        app.show_notes_panel = panel;
        let _ = app.db.set_setting("notes_panel", if panel { "1" } else { "0" });
    }
}

/// A search box with a magnifier on the left, a clear ✕ while it has text
/// and an optional key hint (`Ctrl K`) on the right.
pub fn search_field(
    ui_: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    width: f32,
    key_hint: Option<&str>,
) -> egui::Response {
    let right_pad: i8 = if key_hint.is_some() { 64 } else { 30 };
    let mut resp = ui_.add(
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(t::FAINT))
            .background_color(t::PANEL)
            .margin(Margin { left: 34, right: right_pad, top: 8, bottom: 8 })
            .desired_width(width),
    );
    w::focus_glow(ui_, &resp);
    let r = resp.rect;
    ui_.painter().text(
        egui::pos2(r.left() + 12.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        icon::MAGNIFYING_GLASS,
        t::icon_font(15.0),
        t::FAINT,
    );
    if !text.is_empty() {
        let x_rect = egui::Rect::from_center_size(
            egui::pos2(r.right() - 16.0, r.center().y),
            egui::Vec2::splat(22.0),
        );
        let x = ui_.interact(x_rect, resp.id.with("clear"), Sense::click());
        ui_.painter().text(
            x_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon::X,
            t::icon_font(13.0),
            if x.hovered() { t::TEXT } else { t::FAINT },
        );
        if x.on_hover_text("Clear (Esc)").clicked() {
            text.clear();
            resp.mark_changed();
        }
    } else if let Some(k) = key_hint {
        let galley = ui_
            .painter()
            .layout_no_wrap(k.to_string(), t::mono(11.0), t::MUTED);
        let size = galley.size() + vec2(12.0, 4.0);
        let kr = egui::Rect::from_min_size(
            egui::pos2(r.right() - 8.0 - size.x, r.center().y - size.y / 2.0),
            size,
        );
        ui_.painter().rect_stroke(
            kr,
            5,
            egui::Stroke::new(1.0, t::SWITCH_OFF),
            egui::StrokeKind::Inside,
        );
        ui_.painter().galley(kr.min + vec2(6.0, 2.0), galley, t::MUTED);
    }
    resp
}

enum Action {
    Edit(i64),
    SaveEdit,
    CancelEdit,
    Delete(i64),
    Select(i64, Option<i64>),
}

fn entry_list(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let mut action: Option<Action> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            let entries = app.entries.clone();
            // One collapsible section per day (entries arrive date-sorted):
            // fold days you're done reading. State is keyed on the date, so
            // it survives edits and reloads within the session.
            let mut day_start = 0;
            while day_start < entries.len() {
                let date = entries[day_start].entry.work_date;
                let day_end = entries[day_start..]
                    .iter()
                    .position(|r| r.entry.work_date != date)
                    .map(|off| day_start + off)
                    .unwrap_or(entries.len());
                let day = &entries[day_start..day_end];
                let day_hours: f64 = day.iter().map(|r| r.entry.hours).sum();
                let open = w::collapsible_header(
                    ui_,
                    egui::Id::new(("journal/day", date)),
                    true,
                    false,
                    |ui_| {
                        ui_.spacing_mut().item_spacing.x = 6.0;
                        ui_.label(
                            RichText::new(date.format("%A").to_string())
                                .font(t::font(13.0, Weight::Semibold))
                                .color(t::TEXT),
                        );
                        ui_.label(
                            RichText::new(format!("· {}", date.format("%b %-d")))
                                .font(t::font(13.0, Weight::Medium))
                                .color(t::MUTED),
                        );
                        ui_.add_space(6.0);
                        if date == today() {
                            w::today_badge(ui_);
                            ui_.add_space(6.0);
                        }
                        // total right-aligned, hairline filling the gap
                        let total = w::mono_semi(format!("{} h", fmt_hours(day_hours)), 13.0)
                            .color(t::TEXT);
                        let galley = egui::WidgetText::from(total.clone()).into_galley(
                            ui_,
                            Some(egui::TextWrapMode::Extend),
                            f32::INFINITY,
                            egui::TextStyle::Body,
                        );
                        w::hairline(ui_, galley.size().x + 16.0);
                        ui_.add_space(6.0);
                        ui_.label(total);
                    },
                );
                ui_.add_space(4.0);
                if open {
                    w::list_frame().show(ui_, |ui_| {
                        ui_.spacing_mut().item_spacing.y = 0.0;
                        for (n, row) in day.iter().enumerate() {
                            if n > 0 {
                                w::list_divider(ui_);
                            }
                            entry_row(app, ui_, row, &mut action);
                        }
                    });
                }
                ui_.add_space(18.0);
                day_start = day_end;
            }
            if entries.is_empty() {
                ui_.add_space(16.0);
                ui_.label(RichText::new("No entries in this range yet.").color(t::MUTED));
            } else if entries.len() >= crate::db::JOURNAL_LIMIT {
                ui_.label(
                    RichText::new(format!(
                        "Showing the {} most recent entries — narrow the range or search \
                         to see older ones.",
                        crate::db::JOURNAL_LIMIT
                    ))
                    .color(t::MUTED),
                );
            }
        });

    match action {
        Some(Action::Edit(id)) => {
            if let Some(row) = app.entries.iter().find(|r| r.entry.id == id) {
                app.editing_entry = Some((id, EntryForm::from_entry(row)));
                app.confirm_delete_entry = None;
            }
        }
        Some(Action::SaveEdit) => {
            if let Some((id, form)) = app.editing_entry.take()
                && let (Some(pid), Some(hours)) = (form.project_id, form.parse_hours())
            {
                match app.db.update_log_entry(
                    id,
                    form.work_date,
                    pid,
                    form.description.trim(),
                    hours,
                    form.is_dev,
                ) {
                    Ok(()) => app.set_status("Entry updated"),
                    Err(e) => app.set_status(format!("Update failed: {e}")),
                }
                app.touch();
            }
        }
        Some(Action::CancelEdit) => app.editing_entry = None,
        Some(Action::Delete(id)) => {
            let deleted = app
                .entries
                .iter()
                .find(|r| r.entry.id == id)
                .map(|r| r.entry.clone());
            match app.db.delete_log_entry(id) {
                Ok(()) => {
                    app.set_status("Entry deleted");
                    if let Some(entry) = deleted {
                        app.offer_undo(UndoItem::Entry(entry));
                    }
                }
                Err(e) => app.set_status(format!("Delete failed: {e}")),
            }
            app.touch();
        }
        Some(Action::Select(id, task_id)) => {
            app.selected_entry_id = Some(id);
            if task_id.is_some() {
                app.selected_task_id = task_id;
            }
        }
        None => {}
    }
}

/// One journal row: dot, code, description, checklist count, R&D badge,
/// hours, edit/delete. Becomes the inline editor while being edited.
fn entry_row(
    app: &mut WorklogApp,
    ui_: &mut egui::Ui,
    row: &LogEntryRow,
    action: &mut Option<Action>,
) {
    if let Some((_, form)) = app
        .editing_entry
        .as_ref()
        .filter(|(id, _)| *id == row.entry.id)
    {
        let mut form = form.clone();
        egui::Frame::new()
            .fill(t::SELECTED_ROW)
            .inner_margin(Margin::symmetric(14, 12))
            .show(ui_, |ui_| {
                ui_.set_width(ui_.available_width());
                ui_.horizontal_wrapped(|ui_| {
                    ui_.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    let submit =
                        ui::entry_form_fields(app, ui_, &mut form, "edit_entry", None, None, false, 200.0);
                    let valid = form.is_valid();
                    if ui_
                        .add_enabled(valid, w::primary("Save").min_size(vec2(0.0, 36.0)))
                        .clicked()
                        || (submit && valid)
                    {
                        *action = Some(Action::SaveEdit);
                    }
                    if ui_
                        .add(w::secondary("Cancel").min_size(vec2(0.0, 36.0)))
                        .clicked()
                        || ui_.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        *action = Some(Action::CancelEdit);
                    }
                });
            });
        if !matches!(action, Some(Action::SaveEdit | Action::CancelEdit)) {
            app.editing_entry = Some((row.entry.id, form));
        }
        return;
    }

    let sw = app.project_swatch(row.entry.project_id);
    let selected = app.show_notes_panel && app.selected_entry_id == Some(row.entry.id);
    let checklist = row
        .entry
        .task_id
        .and_then(|id| app.entry_checklists.get(&id).copied());
    let (_, resp) = w::list_row(ui_, ("entry", row.entry.id), 50.0, selected, |ui_| {
        ui_.spacing_mut().item_spacing.x = 12.0;
        w::dot(ui_, sw.base, 8.0);
        ui_.allocate_ui_with_layout(
            vec2(86.0, 20.0),
            Layout::left_to_right(Align::Center),
            |ui_| {
                ui_.add(
                    egui::Label::new(w::mono_semi(&row.project_code, 12.5).color(sw.light))
                        .truncate(),
                )
                .on_hover_text(format!(
                    "{} — recorded {}",
                    row.project_name,
                    row.entry
                        .created_at
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                ));
            },
        );
        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
            ui_.spacing_mut().item_spacing.x = 2.0;
            if ui::confirm_delete_button(ui_, &mut app.confirm_delete_entry, row.entry.id) {
                *action = Some(Action::Delete(row.entry.id));
            }
            if w::icon_button(ui_, icon::PENCIL_SIMPLE, "Edit entry").clicked() {
                *action = Some(Action::Edit(row.entry.id));
            }
            ui_.spacing_mut().item_spacing.x = 12.0;
            ui_.allocate_ui_with_layout(
                vec2(48.0, 20.0),
                Layout::right_to_left(Align::Center),
                |ui_| {
                    ui_.label(
                        w::mono_semi(format!("{} h", fmt_hours(row.entry.hours)), 14.0)
                            .color(t::TEXT),
                    );
                },
            );
            if row.entry.is_dev {
                w::rd_badge(ui_);
            }
            if let Some((done, total)) = checklist {
                w::icon_label(
                    ui_,
                    &format!("{} {done}/{total}", icon::CHECK_SQUARE),
                    12.0,
                    if done == total { t::ACCENT_TEXT } else { t::MUTED },
                )
                .on_hover_text("Checklist in the task's notes");
            }
            ui_.with_layout(Layout::left_to_right(Align::Center), |ui_| {
                let hover = if row.entry.task_id.is_some() {
                    format!("{}\n\nClick to show the task's notes", row.entry.description)
                } else {
                    row.entry.description.clone()
                };
                ui_.add(
                    egui::Label::new(RichText::new(&row.entry.description).size(14.0).color(t::TEXT))
                        .truncate(),
                )
                .on_hover_text(hover);
            });
        });
    });
    if resp.clicked() {
        *action = Some(Action::Select(row.entry.id, row.entry.task_id));
    }
    if resp.double_clicked() {
        *action = Some(Action::Edit(row.entry.id));
    }
}
