use crate::app::{today, EntryForm, FilterRange, WorklogApp};
use crate::report::fmt_hours;
use crate::ui;

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    capture_strip(app, ui_);
    ui_.add_space(8.0);
    filter_row(app, ui_);
    ui_.separator();
    if app.show_notes_panel {
        egui::Panel::right(egui::Id::new("journal/notes_panel"))
            .resizable(true)
            .default_size(280.0)
            .show(ui_, |ui_| ui::tasks::notes_panel(app, ui_));
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new())
        .show(ui_, |ui_| entry_list(app, ui_));
}

/// The fast-capture form. Enter anywhere in it saves.
/// (`app.capture` is moved out for the frame because `entry_form_fields`
/// needs `&mut app` alongside the form.)
fn capture_strip(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    egui::Frame::group(ui_.style())
        .fill(ui_.visuals().faint_bg_color)
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.label(egui::RichText::new("Log work").strong());

            let mut form = std::mem::replace(&mut app.capture, EntryForm::new(None, today()));
            let submit = ui::entry_form_fields(
                app,
                ui_,
                &mut form,
                "capture",
                Some(ui::FOCUS_CAPTURE_DESC),
                None,
            );
            let valid = form.is_valid();
            if (ui_
                .add_enabled(valid, egui::Button::new("Add entry"))
                .on_hover_text("Enter also saves")
                .clicked()
                || (submit && valid))
                && app.save_entry_form(&form)
            {
                form = EntryForm::new(form.project_id, form.work_date);
                app.request_focus(ui::FOCUS_CAPTURE_DESC);
            }
            app.capture = form;
        });
}

fn filter_row(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        ui_.label("Show:");
        let projects = app.projects.clone();
        if ui::project_filter_combo(ui_, "journal/filter_proj", &projects, &mut app.filter_project)
        {
            app.reload_entries();
        }
        let mut range = app.filter_range;
        egui::ComboBox::from_id_salt("journal/filter_range")
            .selected_text(range.label())
            .show_ui(ui_, |ui_| {
                for r in FilterRange::ALL {
                    ui_.selectable_value(&mut range, r, r.label());
                }
            });
        if range != app.filter_range {
            app.filter_range = range;
            app.reload_entries();
        }
        if ui_
            .add(
                egui::TextEdit::singleline(&mut app.filter_text)
                    .hint_text("search: text or project…")
                    .desired_width(200.0),
            )
            .changed()
        {
            app.reload_entries();
        }
        if !app.filter_text.is_empty() && ui_.button("✖").on_hover_text("clear search").clicked()
        {
            app.filter_text.clear();
            app.reload_entries();
        }
        let mut panel = app.show_notes_panel;
        ui_.checkbox(&mut panel, "notes panel")
            .on_hover_text("show the selected task's notes beside the list");
        if panel != app.show_notes_panel {
            app.show_notes_panel = panel;
            let _ = app.db.set_setting("notes_panel", if panel { "1" } else { "0" });
        }
        let total: f64 = app.entries.iter().map(|e| e.entry.hours).sum();
        ui_.label(
            egui::RichText::new(format!(
                "{} entries, {} h",
                app.entries.len(),
                fmt_hours(total)
            ))
            .weak(),
        );
    });
}

enum Action {
    Edit(usize),
    SaveEdit,
    CancelEdit,
    Delete(i64),
}

fn entry_list(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let mut action: Option<Action> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            let entries = app.entries.clone();
            // Column widths in characters: code and hours are padded to the
            // widest in the list (monospace), so descriptions line up no
            // matter the project, the decimals, or the dev badge.
            let code_w = entries.iter().map(|r| r.project_code.chars().count()).max().unwrap_or(0);
            let hours_w = entries
                .iter()
                .map(|r| fmt_hours(r.entry.hours).chars().count())
                .max()
                .unwrap_or(0);
            let mut last_date = None;
            for (i, row) in entries.iter().enumerate() {
                // date subheader whenever the day changes
                if last_date != Some(row.entry.work_date) {
                    last_date = Some(row.entry.work_date);
                    ui_.add_space(6.0);
                    ui_.label(
                        egui::RichText::new(row.entry.work_date.format("%A %Y-%m-%d").to_string())
                            .weak()
                            .small(),
                    );
                }

                if app.editing_entry.as_ref().is_some_and(|(id, _)| *id == row.entry.id) {
                    // Inline editor
                    egui::Frame::group(ui_.style()).show(ui_, |ui_| {
                        ui_.set_width(ui_.available_width());
                        let mut form = app.editing_entry.as_ref().unwrap().1.clone();
                        let submit =
                            ui::entry_form_fields(app, ui_, &mut form, "edit_entry", None, None);
                        let valid = form.is_valid();
                        app.editing_entry = Some((row.entry.id, form));
                        ui_.horizontal(|ui_| {
                            if ui_.add_enabled(valid, egui::Button::new("Save")).clicked()
                                || (submit && valid)
                            {
                                action = Some(Action::SaveEdit);
                            }
                            if ui_.button("Cancel").clicked() {
                                action = Some(Action::CancelEdit);
                            }
                        });
                    });
                    continue;
                }

                ui_.horizontal(|ui_| {
                    ui_.label(
                        egui::RichText::new(format!("{:<code_w$}", row.project_code))
                            .strong()
                            .monospace(),
                    )
                    .on_hover_text(format!(
                        "recorded {}",
                        row.entry
                            .created_at
                            .with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M")
                    ));
                    ui_.label(
                        egui::RichText::new(format!(
                            "{:>hours_w$} h",
                            fmt_hours(row.entry.hours)
                        ))
                        .monospace(),
                    );
                    // fixed slot whether or not the badge is there
                    ui_.allocate_ui_with_layout(
                        egui::vec2(26.0, 16.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui_| {
                            if row.entry.is_dev {
                                ui::dev_badge(ui_);
                            }
                        },
                    );
                    ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                        if ui::confirm_delete_button(ui_, &mut app.confirm_delete_entry, row.entry.id)
                        {
                            action = Some(Action::Delete(row.entry.id));
                        }
                        if ui_.button("edit").clicked() {
                            action = Some(Action::Edit(i));
                        }
                        ui_.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui_| {
                                let from_task = row.entry.task_id.is_some();
                                let mut label =
                                    egui::Label::new(&row.entry.description).truncate();
                                if from_task {
                                    label = label.sense(egui::Sense::click());
                                }
                                let hover = if from_task {
                                    format!(
                                        "{}\n\nclick to show the task's notes",
                                        row.entry.description
                                    )
                                } else {
                                    row.entry.description.clone()
                                };
                                if ui_.add(label).on_hover_text(hover).clicked() {
                                    app.selected_task_id = row.entry.task_id;
                                }
                            },
                        );
                    });
                });
            }
            if entries.is_empty() {
                ui_.add_space(16.0);
                ui_.label(egui::RichText::new("No entries in this range yet.").weak());
            }
        });

    match action {
        Some(Action::Edit(i)) => {
            let row = &app.entries[i];
            app.editing_entry = Some((row.entry.id, EntryForm::from_entry(row)));
            app.confirm_delete_entry = None;
        }
        Some(Action::SaveEdit) => {
            if let Some((id, form)) = app.editing_entry.take() {
                if let (Some(pid), Some(hours)) = (form.project_id, form.parse_hours()) {
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
        }
        Some(Action::CancelEdit) => app.editing_entry = None,
        Some(Action::Delete(id)) => {
            match app.db.delete_log_entry(id) {
                Ok(()) => app.set_status("Entry deleted"),
                Err(e) => app.set_status(format!("Delete failed: {e}")),
            }
            app.touch();
        }
        None => {}
    }
}
