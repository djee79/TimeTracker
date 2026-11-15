use egui::{Align, Layout, Margin, RichText, Sense, Stroke, vec2};

use crate::app::{IDLE_PAUSE_CHOICES, ProjectForm, WorklogApp, today};
use crate::db::ProjectStatus;
use crate::report::{self, fmt_hours};
use crate::ui;
use crate::ui::theme::{self as t, Weight};
use crate::ui::widgets::{self as w, icon};

const WIDTH: f32 = 760.0;

/// The project manager: a modal dialog (Esc or a click outside closes it).
pub fn show_window(app: &mut WorklogApp, ctx: &egui::Context) {
    if !app.show_projects {
        return;
    }
    if app.project_form.color.is_empty() {
        app.project_form.color = suggested_color(app).to_string();
    }
    let modal = egui::Modal::new(egui::Id::new("projects_dialog"))
        .backdrop_color(egui::Color32::from_black_alpha(150))
        .frame(
            egui::Frame::new()
                .fill(t::PANEL)
                .stroke(Stroke::new(1.0, t::BORDER_STRONG))
                .corner_radius(t::R_DIALOG)
                .shadow(ctx.global_style().visuals.window_shadow),
        )
        .show(ctx, |ui_| {
            ui_.set_width(WIDTH);
            ui_.spacing_mut().item_spacing.y = 0.0;
            let mut close = false;
            header(app, ui_, &mut close);
            divider(ui_);
            let max_h = (ctx.content_rect().height() - 160.0).max(300.0);
            egui::ScrollArea::vertical()
                .max_height(max_h)
                .min_scrolled_height(max_h)
                .auto_shrink([false, true])
                .show(ui_, |ui_| {
                    ui_.spacing_mut().item_spacing.y = 0.0;
                    form(app, ui_);
                    divider(ui_);
                    list(app, ui_);
                    divider(ui_);
                    settings(app, ui_);
                });
            close
        });
    if modal.inner || modal.should_close() {
        app.show_projects = false;
    }
}

fn divider(ui_: &mut egui::Ui) {
    let (rect, _) = ui_.allocate_exact_size(vec2(ui_.available_width(), 1.0), Sense::hover());
    ui_.painter().rect_filled(rect, 0, t::DIVIDER);
}

fn header(app: &WorklogApp, ui_: &mut egui::Ui, close: &mut bool) {
    egui::Frame::new()
        .inner_margin(Margin { left: 24, right: 20, top: 18, bottom: 16 })
        .show(ui_, |ui_| {
            ui_.horizontal(|ui_| {
                ui_.vertical(|ui_| {
                    ui_.spacing_mut().item_spacing.y = 2.0;
                    ui_.label(RichText::new("Projects").font(t::font(18.0, Weight::Semibold)).color(t::TEXT));
                    let n = app.active_projects().count();
                    ui_.label(RichText::new(format!("{n} active")).size(12.5).color(t::MUTED));
                });
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    if w::icon_button_ex(ui_, icon::X, "Close (Esc)", 36.0, t::MUTED, true).clicked() {
                        *close = true;
                    }
                });
            });
        });
}

/// The palette entry used least by active projects — the default for a
/// new project, so colours stay distinct.
fn suggested_color(app: &WorklogApp) -> &'static str {
    t::PALETTE
        .iter()
        .min_by_key(|s| {
            app.active_projects()
                .filter(|p| p.color.as_deref() == Some(s.key))
                .count()
        })
        .map_or("blue", |s| s.key)
}

fn form(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let editing = app.project_form.id.is_some();
    egui::Frame::new()
        .inner_margin(Margin::symmetric(24, 20))
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.spacing_mut().item_spacing.y = 14.0;
            if app.projects.is_empty() {
                ui_.label(
                    RichText::new(
                        "Welcome! Create at least one project — every log entry and task belongs to one.",
                    )
                    .color(t::ACCENT_TEXT),
                );
            }
            w::eyebrow(ui_, if editing { "Edit project" } else { "New project" });
            let mut submit = false;
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 12.0;
                let total = ui_.available_width();
                let name_w = total - 150.0 - 180.0 - 24.0;
                w::labeled(ui_, "Code", |ui_| {
                    let r = w::input(
                        ui_,
                        egui::TextEdit::singleline(&mut app.project_form.code)
                            .hint_text(RichText::new("HICA-002").color(t::FAINT))
                            .font(t::mono(13.5))
                            .desired_width(150.0),
                    );
                    submit |= ui::enter_pressed(&r, ui_);
                });
                w::labeled(ui_, "Name", |ui_| {
                    let r = w::input(
                        ui_,
                        egui::TextEdit::singleline(&mut app.project_form.name)
                            .hint_text(RichText::new("Project name").color(t::FAINT))
                            .desired_width(name_w),
                    );
                    submit |= ui::enter_pressed(&r, ui_);
                });
                w::labeled(ui_, "Client (optional)", |ui_| {
                    let r = w::input(
                        ui_,
                        egui::TextEdit::singleline(&mut app.project_form.client)
                            .hint_text(RichText::new("Client").color(t::FAINT))
                            .desired_width(180.0),
                    );
                    submit |= ui::enter_pressed(&r, ui_);
                });
            });
            let valid =
                !app.project_form.code.trim().is_empty() && !app.project_form.name.trim().is_empty();
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 14.0;
                ui_.label(RichText::new("Colour").font(t::font(12.0, Weight::Medium)).color(t::MUTED));
                ui_.horizontal(|ui_| {
                    ui_.spacing_mut().item_spacing.x = 8.0;
                    for sw in t::PALETTE {
                        if swatch_button(ui_, sw, app.project_form.color == sw.key).clicked() {
                            app.project_form.color = sw.key.to_string();
                        }
                    }
                });
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    let label = if editing {
                        format!("{}  Save", icon::CHECK)
                    } else {
                        format!("{}  Add project", icon::PLUS)
                    };
                    if ui_
                        .add_enabled(valid, w::primary(label).min_size(vec2(0.0, 36.0)))
                        .clicked()
                        || (submit && valid)
                    {
                        save(app);
                    }
                    if editing && ui_.add(w::secondary("Cancel").min_size(vec2(0.0, 36.0))).clicked() {
                        app.project_form = ProjectForm::default();
                    }
                });
            });
        });
}

/// 22 px round swatch; the selected one gets a double ring.
fn swatch_button(ui_: &mut egui::Ui, sw: t::Swatch, selected: bool) -> egui::Response {
    let (rect, resp) = ui_.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let c = rect.center();
    if selected {
        ui_.painter().circle_filled(c, 13.0, t::TEXT);
        ui_.painter().circle_filled(c, 11.0, t::PANEL);
    } else if resp.hovered() {
        ui_.painter().circle_filled(c, 13.0, t::BORDER_STRONG);
    }
    ui_.painter().circle_filled(c, 9.0, sw.base);
    resp.on_hover_text(sw.name)
}

fn save(app: &mut WorklogApp) {
    let code = app.project_form.code.trim().to_string();
    let name = app.project_form.name.trim().to_string();
    let client = app.project_form.client.trim();
    let client = (!client.is_empty()).then_some(client.to_string());
    let color = app.project_form.color.clone();
    let result = match app.project_form.id {
        Some(id) => app
            .db
            .update_project(id, &code, &name, client.as_deref())
            .map(|_| id),
        None => app.db.insert_project(&code, &name, client.as_deref()),
    }
    .and_then(|id| app.db.set_project_color(id, &color));
    match result {
        Ok(()) => {
            app.set_status(format!("Project {code} saved"));
            app.project_form = ProjectForm::default();
            app.reload_projects();
            app.touch();
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
        Edit(i64),
        SetStatus(i64, ProjectStatus),
    }
    let mut action = None;
    let month = report::month_start_of(today());
    let hours = app
        .db
        .hours_by_project(month, report::month_end_of(month))
        .unwrap_or_default();
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.spacing_mut().item_spacing.y = 2.0;
            if app.projects.is_empty() {
                ui_.label(RichText::new("No projects yet.").color(t::MUTED));
            }
            for p in app.projects.clone() {
                let archived = p.status == ProjectStatus::Archived;
                let sw = ui::swatch_of(&p);
                let editing = app.project_form.id == Some(p.id);
                let (rect, _) =
                    ui_.allocate_exact_size(vec2(ui_.available_width(), 52.0), Sense::hover());
                let bg = ui_.painter().add(egui::Shape::Noop);
                ui_.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(vec2(8.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui_| {
                        ui_.spacing_mut().item_spacing.x = 14.0;
                        let dim = |c: egui::Color32| if archived { t::alpha(c, 0.45) } else { c };
                        let (sq, _) = ui_.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui_.painter().rect_filled(sq, 4, dim(sw.base));
                        ui_.allocate_ui_with_layout(vec2(110.0, 20.0), Layout::left_to_right(Align::Center), |ui_| {
                            ui_.set_width(110.0);
                            ui_.add(egui::Label::new(w::mono_semi(&p.code, 14.0).color(dim(sw.light))).truncate());
                        });
                        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                            ui_.spacing_mut().item_spacing.x = 2.0;
                            if archived {
                                if w::icon_button_ex(ui_, icon::ARROW_COUNTER_CLOCKWISE, "Unarchive", 34.0, t::FAINT, true)
                                    .clicked()
                                {
                                    action = Some(Action::SetStatus(p.id, ProjectStatus::Active));
                                }
                            } else if w::icon_button_ex(
                                ui_,
                                icon::ARCHIVE,
                                "Archive — hide from pickers; history is kept",
                                34.0,
                                t::FAINT,
                                true,
                            )
                            .clicked()
                            {
                                action = Some(Action::SetStatus(p.id, ProjectStatus::Archived));
                            }
                            if w::icon_button_ex(ui_, icon::PENCIL_SIMPLE, "Edit", 34.0, t::FAINT, true).clicked() {
                                action = Some(Action::Edit(p.id));
                            }
                            ui_.add_space(10.0);
                            let h = hours.get(&p.id).copied().unwrap_or(0.0);
                            ui_.label(
                                w::mono(format!("{} h this month", fmt_hours(h)), 12.5).color(t::MUTED),
                            );
                            ui_.with_layout(Layout::left_to_right(Align::Center), |ui_| {
                                ui_.spacing_mut().item_spacing.x = 10.0;
                                ui_.add(
                                    egui::Label::new(RichText::new(&p.name).size(14.0).color(dim(t::TEXT_2)))
                                        .truncate(),
                                );
                                if let Some(c) = p.client.as_deref().filter(|c| !c.trim().is_empty()) {
                                    w::client_tag(ui_, c);
                                }
                                if archived {
                                    ui_.label(RichText::new("archived").size(12.0).color(t::FAINT));
                                }
                            });
                        });
                    },
                );
                if editing {
                    ui_.painter().set(bg, egui::Shape::rect_filled(rect, 10, t::SELECTED_ROW));
                } else if ui_.rect_contains_pointer(rect) {
                    ui_.painter().set(bg, egui::Shape::rect_filled(rect, 10, t::HOVER));
                }
            }
        });

    match action {
        Some(Action::Edit(id)) => {
            if let Some(p) = app.project(id) {
                app.project_form = ProjectForm {
                    id: Some(p.id),
                    code: p.code.clone(),
                    name: p.name.clone(),
                    client: p.client.clone().unwrap_or_default(),
                    color: ui::swatch_of(p).key.to_string(),
                };
            }
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

/// App-wide settings, as a two-column form.
fn settings(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    egui::Frame::new()
        .fill(t::PANEL_ALT)
        .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: t::R_DIALOG, se: t::R_DIALOG })
        .inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 24 })
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.spacing_mut().item_spacing.y = 0.0;
            w::eyebrow(ui_, "App settings");
            ui_.add_space(8.0);

            setting_row(ui_, "Backup mirror", "Daily snapshots, copied here too", true, |ui_| {
                ui_.spacing_mut().item_spacing.y = 6.0;
                ui_.horizontal(|ui_| {
                    ui_.spacing_mut().item_spacing.x = 8.0;
                    let field_w = ui_.available_width() - 120.0;
                    let resp = w::input(
                        ui_,
                        egui::TextEdit::singleline(&mut app.settings_mirror_dir)
                            .hint_text(RichText::new("/mnt/nas/worklog-backups").color(t::FAINT))
                            .font(t::mono(13.0))
                            .desired_width(field_w),
                    );
                    let mut save = resp.lost_focus();
                    if ui_
                        .add(w::secondary(format!("{}  Choose…", icon::FOLDER)).min_size(vec2(0.0, 36.0)))
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        app.settings_mirror_dir = dir.display().to_string();
                        save = true;
                    }
                    if save {
                        let _ = app
                            .db
                            .set_setting("backup_mirror_dir", app.settings_mirror_dir.trim());
                    }
                });
                ui_.label(
                    RichText::new("Use a second disk or a NAS so the log survives this disk dying. Clear the field to stop mirroring.")
                        .size(12.0)
                        .color(t::MUTED),
                );
            });

            setting_row(ui_, "Name on PDF reports", "Shown as “Prepared by …”", true, |ui_| {
                let resp = w::input(
                    ui_,
                    egui::TextEdit::singleline(&mut app.settings_author)
                        .hint_text(RichText::new("Your name").color(t::FAINT))
                        .desired_width(ui_.available_width()),
                );
                if resp.lost_focus() {
                    let _ = app.db.set_setting("report_author", app.settings_author.trim());
                }
            });

            setting_row(
                ui_,
                "Splash screen at startup",
                "The artwork while the database opens",
                true,
                |ui_| {
                    ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                        let mut splash = app.show_splash;
                        w::toggle(ui_, &mut splash, "");
                        if splash != app.show_splash {
                            app.show_splash = splash;
                            let _ = app.db.set_setting("show_splash", if splash { "1" } else { "0" });
                        }
                    });
                },
            );

            setting_row(
                ui_,
                "Pause timers when idle",
                "No keyboard or mouse for this long banks the time up to when you stepped away",
                false,
                |ui_| {
                    ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                        ui_.spacing_mut().item_spacing.x = 12.0;
                        let mut idle = app.idle_pause;
                        w::toggle(ui_, &mut idle, "");
                        if idle != app.idle_pause {
                            app.idle_pause = idle;
                            let _ = app.db.set_setting("idle_pause", if idle { "1" } else { "0" });
                        }
                        let mut mins = app.idle_pause_mins;
                        ui_.add_enabled_ui(app.idle_pause, |ui_| {
                            egui::ComboBox::from_id_salt("settings/idle_mins")
                                .selected_text(format!("{mins} min"))
                                .show_ui(ui_, |ui_| {
                                    for m in IDLE_PAUSE_CHOICES {
                                        ui_.selectable_value(&mut mins, m, format!("{m} min"));
                                    }
                                });
                        });
                        if mins != app.idle_pause_mins {
                            app.idle_pause_mins = mins;
                            let _ = app.db.set_setting("idle_pause_minutes", &mins.to_string());
                        }
                    });
                },
            );
        });
}

/// Label + help on the left (200 px), control on the right.
fn setting_row(
    ui_: &mut egui::Ui,
    label: &str,
    help: &str,
    line_below: bool,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui_.add_space(10.0);
    ui_.horizontal_top(|ui_| {
        ui_.spacing_mut().item_spacing.x = 16.0;
        ui_.allocate_ui_with_layout(vec2(200.0, 0.0), Layout::top_down(Align::Min), |ui_| {
            ui_.set_width(200.0);
            ui_.spacing_mut().item_spacing.y = 3.0;
            ui_.add_space(6.0);
            ui_.label(RichText::new(label).font(t::font(14.0, Weight::Medium)).color(t::TEXT));
            ui_.add(egui::Label::new(RichText::new(help).size(12.0).color(t::MUTED)).wrap());
        });
        ui_.allocate_ui_with_layout(
            vec2(ui_.available_width(), 0.0),
            Layout::top_down(Align::Min),
            control,
        );
    });
    ui_.add_space(10.0);
    if line_below {
        divider_color(ui_, t::LIST_DIVIDER);
    }
}

fn divider_color(ui_: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui_.allocate_exact_size(vec2(ui_.available_width(), 1.0), Sense::hover());
    ui_.painter().rect_filled(rect, 0, color);
}
