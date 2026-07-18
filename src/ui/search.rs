use crate::app::{FilterRange, Tab, WorklogApp};
use crate::db::TaskStatus;
use crate::report::fmt_hours;
use crate::ui;

/// Global search (Ctrl+F): one query over journal entries, tasks (any
/// status) and task notes. Clicking a result jumps to it.
pub fn show_window(app: &mut WorklogApp, ctx: &egui::Context) {
    if !app.show_search {
        return;
    }
    let mut open = app.show_search;
    let mut close_after = false;
    egui::Window::new("Search everything")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(420.0)
        .show(ctx, |ui_| {
            let resp = ui_.add(
                egui::TextEdit::singleline(&mut app.search_query)
                    .hint_text("search journal, tasks and notes — every word must match…")
                    .desired_width(f32::INFINITY),
            );
            app.apply_focus(ui::FOCUS_SEARCH, &resp);
            ui_.separator();
            let query = app.search_query.trim().to_string();
            if query.chars().count() < 2 {
                ui_.label(egui::RichText::new("Type at least two characters.").weak());
                return;
            }
            let tasks = app.db.search_tasks(&query, 30).unwrap_or_default();
            let entries = app.db.list_log_entries(None, None, &query).unwrap_or_default();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui_, |ui_| {
                    ui_.label(
                        egui::RichText::new(format!("Tasks ({})", tasks.len())).weak().small(),
                    );
                    for task in &tasks {
                        let status = match task.status {
                            TaskStatus::Doing => "in progress",
                            TaskStatus::Done => "done",
                            TaskStatus::Todo => "to-do",
                        };
                        let label = format!("{}  {}   ({status})", task.project_code, task.title);
                        if ui_
                            .add(egui::Button::new(label).frame(false))
                            .on_hover_text("open this task's notes")
                            .clicked()
                        {
                            app.tab = Tab::Tasks;
                            app.selected_task_id = Some(task.id);
                            app.open_task_details(task);
                            close_after = true;
                        }
                    }
                    if tasks.is_empty() {
                        ui_.label(egui::RichText::new("no matching tasks").weak());
                    }
                    ui_.add_space(8.0);
                    ui_.label(
                        egui::RichText::new(format!("Journal entries ({})", entries.len()))
                            .weak()
                            .small(),
                    );
                    for row in entries.iter().take(50) {
                        let mut desc = row.entry.description.clone();
                        if desc.chars().count() > 80 {
                            desc = desc.chars().take(79).collect::<String>() + "…";
                        }
                        let label = format!(
                            "{}  {}  {} h — {desc}",
                            row.entry.work_date,
                            row.project_code,
                            fmt_hours(row.entry.hours),
                        );
                        if ui_
                            .add(egui::Button::new(label).frame(false))
                            .on_hover_text("show in the journal, filtered to this search")
                            .clicked()
                        {
                            app.tab = Tab::Journal;
                            app.filter_project = None;
                            app.filter_range = FilterRange::All;
                            app.filter_text = query.clone();
                            app.reload_entries();
                            close_after = true;
                        }
                    }
                    if entries.is_empty() {
                        ui_.label(egui::RichText::new("no matching entries").weak());
                    }
                });
        });
    if close_after {
        open = false;
    }
    // Esc closes, once nothing is holding keyboard focus.
    if open
        && ctx.memory(|m| m.focused().is_none())
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        open = false;
    }
    app.show_search = open;
}
