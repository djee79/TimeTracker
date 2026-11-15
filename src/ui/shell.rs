//! The app shell around every screen: top bar (logo, tabs, running-timer
//! pill, Projects / search / help) and the status bar.

use egui::{Align, Align2, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2, vec2};

use super::theme::{self as t, Weight};
use super::widgets::{self as w, Segment, icon};
use crate::app::{Tab, UNDO_WINDOW_SECS, UndoItem, WorklogApp};
use crate::report;

pub fn top_bar(app: &mut WorklogApp, root: &mut egui::Ui) {
    egui::Panel::top(egui::Id::new("top_bar"))
        .frame(
            Frame::new()
                .fill(t::BAR)
                .inner_margin(Margin::symmetric(20, 10)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                logo(ui);
                ui.add_space(6.0);
                let tabs = [
                    Segment::new(Tab::Journal, "Journal").hint("1"),
                    Segment::new(Tab::Tasks, "Tasks").hint("2"),
                    Segment::new(Tab::Reports, "Reports").hint("3"),
                ];
                w::segmented(ui, &mut app.tab, &tabs, t::CONTROL, 30.0);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if framed_icon(ui, icon::QUESTION, "Help and shortcuts").clicked() {
                        app.show_help = true;
                    }
                    if framed_icon(ui, icon::MAGNIFYING_GLASS, "Search everything (Ctrl+F)")
                        .clicked()
                    {
                        app.show_search = true;
                        app.request_focus(super::FOCUS_SEARCH);
                    }
                    if ui
                        .add(
                            w::secondary(format!("{}  Projects", icon::FOLDER))
                                .min_size(vec2(0.0, 36.0)),
                        )
                        .clicked()
                    {
                        app.show_projects = true;
                    }
                    ui.add_space(6.0);
                    timer_pill(app, ui);
                });
            });
        });
}

fn logo(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    ui.painter().rect_filled(rect, 8, t::ACCENT);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon::CLOCK,
        t::icon_font(18.0),
        t::ACCENT_INK,
    );
    ui.label(RichText::new("worklog").font(t::font(15.0, Weight::Bold)).color(t::TEXT));
}

fn framed_icon(ui: &mut egui::Ui, glyph: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(glyph).font(t::icon_font(18.0)).color(t::MUTED))
            .corner_radius(t::R_INPUT)
            .min_size(Vec2::splat(36.0)),
    )
    .on_hover_text(tip)
}

/// `H:MM:SS` for the live timer.
pub fn clock(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

/// The running-timer pill — only while the active task's timer runs. Laid
/// out right-to-left (the bar's right side), so items go in reverse.
fn timer_pill(app: &mut WorklogApp, ui: &mut egui::Ui) {
    let Some(task) = app.active_task().cloned() else {
        return;
    };
    if task.started_at.is_none() {
        return; // idle-paused: nothing is ticking
    }
    let sw = app.project_swatch(task.project_id);
    let elapsed = task.tracked_secs(chrono::Utc::now());
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(1000));
    Frame::new()
        .fill(t::alpha(t::ACCENT, 0.10))
        .stroke(Stroke::new(1.0, t::alpha(t::ACCENT, 0.34)))
        .corner_radius(t::R_PILL)
        .inner_margin(Margin { left: 14, right: 3, top: 3, bottom: 3 })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if w::round_button(ui, icon::STOP, "Stop & log the time", false).clicked() {
                app.stop_and_log(&task);
            }
            if w::round_button(ui, icon::PAUSE, "Pause timer", true).clicked() {
                app.pause_task(task.id);
            }
            ui.label(w::mono_semi(clock(elapsed), 13.0).color(egui::Color32::WHITE));
            let mut title = task.title.clone();
            if title.chars().count() > 32 {
                title = title.chars().take(31).collect::<String>() + "…";
            }
            if ui
                .add(
                    egui::Label::new(RichText::new(title).size(13.0).color(t::TEXT_2))
                        .sense(Sense::click()),
                )
                .on_hover_text(format!("Active task: {} — click to open Tasks", task.title))
                .clicked()
            {
                app.tab = Tab::Tasks;
            }
            ui.label(w::mono_semi(&task.project_code, 12.0).color(sw.light));
            // pulsing dot: the halo breathes once a second
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            let on = elapsed % 2 == 0;
            ui.painter().circle_filled(
                rect.center(),
                8.0,
                t::alpha(t::ACCENT, if on { 0.22 } else { 0.08 }),
            );
            ui.painter().circle_filled(rect.center(), 4.0, t::ACCENT);
        });
}

pub fn status_bar(app: &mut WorklogApp, root: &mut egui::Ui) {
    let ctx = root.ctx().clone();
    egui::Panel::bottom(egui::Id::new("status_bar"))
        .frame(
            Frame::new()
                .fill(t::BAR)
                .inner_margin(Margin::symmetric(20, 7)),
        )
        .show(root, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui.label(w::mono(app.db.path().display().to_string(), 11.5).color(t::FAINT));
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    if let Some(warning) = &app.db_warning {
                        ui.label(RichText::new(warning).color(t::DANGER).strong());
                    }
                    let today_h = app.today_hours();
                    let week_h = app.week_hours();
                    let missing = app.missing_this_week();
                    stat(ui, "Today", today_h);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if missing > 0 {
                            w::icon_label(ui, &format!("{} {missing}", icon::WARNING), 12.0, t::AMBER)
                                .on_hover_text(format!(
                                    "{missing} weekday(s) this week with no hours logged"
                                ));
                        }
                        stat(ui, "This week", week_h);
                    });
                    match &app.undo_delete {
                        Some((item, at)) if at.elapsed().as_secs() < UNDO_WINDOW_SECS => {
                            let label = match item {
                                UndoItem::Merge { .. } => "Undo merge",
                                _ => "Undo delete",
                            };
                            if ui
                                .add(
                                    w::secondary(format!("{} {label}", icon::ARROW_COUNTER_CLOCKWISE))
                                        .min_size(vec2(0.0, 24.0)),
                                )
                                .clicked()
                            {
                                app.undo_delete_now();
                            }
                            ctx.request_repaint_after(std::time::Duration::from_millis(500));
                        }
                        Some(_) => app.undo_delete = None,
                        None => {}
                    }
                    if let Some((text, at)) = &app.status {
                        if at.elapsed().as_secs_f32() < 5.0 {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).size(12.0).color(t::TEXT_2),
                                )
                                .truncate(),
                            );
                            ctx.request_repaint_after(std::time::Duration::from_millis(500));
                        } else {
                            app.status = None;
                        }
                    }
                });
            });
        });
}

fn stat(ui: &mut egui::Ui, label: &str, hours: f64) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(label).size(12.0).color(t::MUTED));
        ui.label(w::mono_semi(format!("{} h", report::fmt_hours(hours)), 12.0).color(t::TEXT));
    });
}
