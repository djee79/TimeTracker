use crate::app::{ProjectForm, WorklogApp};
use crate::db::ProjectStatus;
use crate::ui;

/// Project manager as a window rather than a screen — it's rarely touched.
pub fn show_window(app: &mut WorklogApp, ctx: &egui::Context) {
    if !app.show_projects {
        return;
    }
    let mut open = app.show_projects;
    egui::Window::new("Projects")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .show(ctx, |ui_| {
            if app.projects.is_empty() {
                ui_.label(
                    egui::RichText::new(
                        "Welcome! Create at least one project — every log entry \
                         and task belongs to one.",
                    )
                    .strong(),
                );
                ui_.add_space(8.0);
            }
            form(app, ui_);
            ui_.separator();
            list(app, ui_);
            ui_.add_space(8.0);
            ui_.separator();
            settings(app, ui_);
        });
    app.show_projects = open;
}

/// App-wide settings that don't deserve their own window.
fn settings(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    egui::CollapsingHeader::new(egui::RichText::new("App settings").strong())
        .default_open(false)
        .show(ui_, |ui_| {
            // Backup mirror: a second folder that receives each daily snapshot.
            ui_.horizontal(|ui_| {
                ui_.label("Backup mirror:");
                let resp = ui_.add(
                    egui::TextEdit::singleline(&mut app.settings_mirror_dir)
                        .hint_text("second folder for daily backups (e.g. a NAS share)")
                        .desired_width(260.0),
                );
                let mut save = resp.lost_focus();
                if ui_.button("Choose…").clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    app.settings_mirror_dir = dir.display().to_string();
                    save = true;
                }
                if !app.settings_mirror_dir.is_empty()
                    && ui_.button("✖").on_hover_text("stop mirroring").clicked()
                {
                    app.settings_mirror_dir.clear();
                    save = true;
                }
                if save {
                    let _ = app
                        .db
                        .set_setting("backup_mirror_dir", app.settings_mirror_dir.trim());
                }
            });
            ui_.label(
                egui::RichText::new(
                    "Each day's snapshot is copied there too — survives this disk dying.",
                )
                .weak()
                .small(),
            );
            ui_.add_space(4.0);

            ui_.horizontal(|ui_| {
                ui_.label("Name on PDF reports:");
                let resp = ui_.add(
                    egui::TextEdit::singleline(&mut app.settings_author)
                        .hint_text("shown as “Prepared by …”")
                        .desired_width(220.0),
                );
                if resp.lost_focus() {
                    let _ = app.db.set_setting("report_author", app.settings_author.trim());
                }
            });
            ui_.add_space(4.0);

            let mut splash = app.show_splash;
            ui_.checkbox(&mut splash, "show splash screen at startup");
            if splash != app.show_splash {
                app.show_splash = splash;
                let _ = app.db.set_setting("show_splash", if splash { "1" } else { "0" });
            }

            let mut idle = app.idle_pause;
            ui_.checkbox(&mut idle, "pause task timers when the machine is idle (10 min)")
                .on_hover_text(
                    "no keyboard/mouse activity pauses the stopwatch and banks time \
                     only up to when you stepped away; it resumes on activity",
                );
            if idle != app.idle_pause {
                app.idle_pause = idle;
                let _ = app.db.set_setting("idle_pause", if idle { "1" } else { "0" });
            }
        });
}

fn form(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let editing = app.project_form.id.is_some();
    ui_.label(
        egui::RichText::new(if editing { "Edit project" } else { "New project" }).strong(),
    );
    let mut submit = false;
    ui_.horizontal(|ui_| {
        let resp = ui_.add(
            egui::TextEdit::singleline(&mut app.project_form.code)
                .hint_text("code (HICA-002)")
                .desired_width(120.0),
        );
        submit |= ui::enter_pressed(&resp, ui_);
        let resp = ui_.add(
            egui::TextEdit::singleline(&mut app.project_form.name)
                .hint_text("name")
                .desired_width(180.0),
        );
        submit |= ui::enter_pressed(&resp, ui_);
        let resp = ui_.add(
            egui::TextEdit::singleline(&mut app.project_form.client)
                .hint_text("client (optional)")
                .desired_width(140.0),
        );
        submit |= ui::enter_pressed(&resp, ui_);
    });
    let valid = !app.project_form.code.trim().is_empty() && !app.project_form.name.trim().is_empty();
    ui_.horizontal(|ui_| {
        let label = if editing { "Save" } else { "Add project" };
        if ui_.add_enabled(valid, egui::Button::new(label)).clicked() || (submit && valid) {
            save(app);
        }
        if editing && ui_.button("Cancel").clicked() {
            app.project_form = ProjectForm::default();
        }
    });
}

fn save(app: &mut WorklogApp) {
    let code = app.project_form.code.trim().to_string();
    let name = app.project_form.name.trim().to_string();
    let client = app.project_form.client.trim();
    let client = (!client.is_empty()).then_some(client.to_string());
    let result = match app.project_form.id {
        Some(id) => app.db.update_project(id, &code, &name, client.as_deref()),
        None => app
            .db
            .insert_project(&code, &name, client.as_deref())
            .map(|_| ()),
    };
    match result {
        Ok(()) => {
            app.set_status(format!("Project {code} saved"));
            app.project_form = ProjectForm::default();
            app.reload_projects();
            // First project ever: make it the capture default right away.
            if app.capture.project_id.is_none() {
                let first = app.active_projects().next().map(|p| p.id);
                app.capture.project_id = first;
                app.new_task_project = first;
            }
        }
        Err(e) => app.set_status(format!("Save failed (duplicate code?): {e}")),
    }
}

fn list(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    enum Action {
        Edit(usize),
        SetStatus(i64, ProjectStatus),
    }
    let mut action = None;
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui_, |ui_| {
            egui::Grid::new("projects/grid")
                .num_columns(4)
                .striped(true)
                .show(ui_, |ui_| {
                    for (i, p) in app.projects.iter().enumerate() {
                        let archived = p.status == ProjectStatus::Archived;
                        let dim = |t: egui::RichText| if archived { t.weak() } else { t };
                        ui_.label(dim(egui::RichText::new(&p.code).monospace().strong()));
                        ui_.label(dim(egui::RichText::new(&p.name)));
                        ui_.label(dim(egui::RichText::new(p.client.as_deref().unwrap_or(""))));
                        ui_.horizontal(|ui_| {
                            if ui_.button("edit").clicked() {
                                action = Some(Action::Edit(i));
                            }
                            if archived {
                                if ui_.button("unarchive").clicked() {
                                    action = Some(Action::SetStatus(p.id, ProjectStatus::Active));
                                }
                            } else if ui_
                                .button("archive")
                                .on_hover_text("hide from pickers; history is kept")
                                .clicked()
                            {
                                action = Some(Action::SetStatus(p.id, ProjectStatus::Archived));
                            }
                        });
                        ui_.end_row();
                    }
                });
        });

    match action {
        Some(Action::Edit(i)) => {
            let p = &app.projects[i];
            app.project_form = ProjectForm {
                id: Some(p.id),
                code: p.code.clone(),
                name: p.name.clone(),
                client: p.client.clone().unwrap_or_default(),
            };
        }
        Some(Action::SetStatus(id, status)) => {
            if let Err(e) = app.db.set_project_status(id, status) {
                app.set_status(format!("Update failed: {e}"));
            }
            app.reload_projects();
        }
        None => {}
    }
}
