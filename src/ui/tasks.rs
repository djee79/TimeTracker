use crate::app::{PendingKind, WorklogApp};
use crate::db::{Priority, Task, TaskSort, TaskStatus};
use crate::ui;

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    bridge_strip(app, ui_);
    add_task_row(app, ui_);
    ui_.separator();
    list_controls(app, ui_);

    let mut action: Option<Action> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            if app.open_tasks.is_empty() {
                ui_.add_space(8.0);
                ui_.label(egui::RichText::new("No open tasks.").weak());
            } else if app.group_tasks {
                grouped_list(app, ui_, &mut action);
            } else if app.task_sort == TaskSort::Manual {
                manual_list(app, ui_, &mut action);
            } else {
                status_list(app, ui_, &mut action);
            }
            ui_.add_space(12.0);
            done_section(app, ui_, &mut action);
        });
    apply(app, action);
}

/// The heart of the app: a just-completed task offering to become a log
/// entry. Pinned above the list (the task itself has already moved to
/// Completed), prefilled, hours field focused — type hours, hit Enter, done.
fn bridge_strip(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let Some(pending) = app.pending_log.take() else {
        return;
    };
    let mut pending = pending;
    let mut keep = true;

    let accent = ui_.visuals().selection.bg_fill.linear_multiply(0.15);
    egui::Frame::group(ui_.style()).fill(accent).show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        let title = match pending.kind {
            PendingKind::Completed { .. } => {
                format!("Done: “{}” — log the time?", pending.task_title)
            }
            PendingKind::Midway => {
                format!("“{}” — log the time so far? (stays open)", pending.task_title)
            }
        };
        ui_.label(egui::RichText::new(title).strong());
        if pending.tracked_secs > 0 {
            let hours = crate::report::fmt_hours(pending.tracked_secs as f64 / 3600.0);
            if ui_
                .link(
                    egui::RichText::new(format!(
                        "⏱ {} tracked (≈ {hours} h)",
                        crate::report::fmt_duration(pending.tracked_secs),
                    ))
                    .weak(),
                )
                .on_hover_text("click to put this value in the hours box — still yours to edit")
                .clicked()
            {
                pending.form.hours_text = hours;
            }
        }
        if pending.longest_stretch_secs >= crate::app::LONG_STRETCH_SECS {
            ui_.label(
                egui::RichText::new(format!(
                    "⚠ includes one unbroken stretch of {} — timer left running?",
                    crate::report::fmt_duration(pending.longest_stretch_secs),
                ))
                .color(ui_.visuals().warn_fg_color),
            );
        }
        let submit = ui::entry_form_fields(
            app,
            ui_,
            &mut pending.form,
            "bridge",
            None,
            Some(ui::FOCUS_BRIDGE_HOURS),
        );
        let valid = pending.form.is_valid();
        ui_.horizontal(|ui_| {
            if (ui_
                .add_enabled(valid, egui::Button::new("Log it"))
                .on_hover_text("Enter also saves")
                .clicked()
                || (submit && valid))
                && app.save_entry_form(&pending.form)
            {
                app.pending_logged(&pending);
                keep = false;
            }
            if ui_
                .button("Skip")
                .on_hover_text("Don't log time; the tracked count is kept for later")
                .clicked()
            {
                keep = false;
            }
            if matches!(pending.kind, PendingKind::Completed { .. })
                && ui_
                    .button("Cancel")
                    .on_hover_text("Oops — put the task back as it was, log nothing")
                    .clicked()
            {
                app.undo_complete(&pending);
                keep = false;
            }
        });
        // Esc = Skip, once nothing is holding keyboard focus (a first Esc
        // just leaves the text field).
        if ui_.ctx().memory(|m| m.focused().is_none())
            && ui_.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            keep = false;
        }
    });
    ui_.add_space(8.0);

    if keep {
        app.pending_log = Some(pending);
    }
}

/// Cycle button for a priority value. Returns Some(new) when clicked.
fn priority_button(ui_: &mut egui::Ui, priority: Priority) -> Option<Priority> {
    let (symbol, color) = match priority {
        Priority::High => ("⏶", egui::Color32::from_rgb(220, 90, 60)),
        Priority::Normal => ("–", ui_.visuals().weak_text_color()),
        Priority::Low => ("⏷", egui::Color32::from_rgb(110, 140, 180)),
    };
    let clicked = ui_
        .add(
            egui::Button::new(egui::RichText::new(symbol).color(color).strong())
                .frame(false)
                .min_size(egui::vec2(18.0, 0.0)),
        )
        .on_hover_text(format!("priority: {} — click to change", priority.label()))
        .clicked();
    clicked.then(|| priority.cycled())
}

fn add_task_row(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        let projects = app.projects.clone();
        ui::project_combo(ui_, "tasks/new_proj", &projects, &mut app.new_task_project);
        if let Some(p) = priority_button(ui_, app.new_task_priority) {
            app.new_task_priority = p;
        }
        let mut title = std::mem::take(&mut app.new_task_title);
        let resp = ui_.add(
            egui::TextEdit::singleline(&mut title)
                .hint_text("new task…")
                .desired_width(ui_.available_width() - 60.0),
        );
        app.apply_focus(ui::FOCUS_TASK_TITLE, &resp);
        let submit = ui::enter_pressed(&resp, ui_);
        let valid = app.new_task_project.is_some() && !title.trim().is_empty();
        if ui_.add_enabled(valid, egui::Button::new("Add")).clicked() || (submit && valid) {
            if let Some(pid) = app.new_task_project {
                match app.db.insert_task(pid, title.trim(), app.new_task_priority) {
                    Ok(_) => {
                        title.clear();
                        app.new_task_priority = Priority::Normal;
                        app.request_focus(ui::FOCUS_TASK_TITLE);
                    }
                    Err(e) => app.set_status(format!("Add failed: {e}")),
                }
                app.touch();
            }
        }
        app.new_task_title = title;
    });
}

fn list_controls(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        ui_.label("order:");
        let mut sort = app.task_sort;
        egui::ComboBox::from_id_salt("tasks/sort")
            .selected_text(match sort {
                TaskSort::Auto => "auto",
                TaskSort::Manual => "manual",
            })
            .show_ui(ui_, |ui_| {
                ui_.selectable_value(&mut sort, TaskSort::Auto, "auto");
                ui_.selectable_value(&mut sort, TaskSort::Manual, "manual");
            });
        if sort != app.task_sort {
            app.set_task_sort(sort);
        }

        let mut group = app.group_tasks;
        ui_.checkbox(&mut group, "group by project");
        if group != app.group_tasks {
            app.group_tasks = group;
            let _ = app.db.set_setting("group_tasks", if group { "1" } else { "0" });
        }

        let hint = match app.task_sort {
            TaskSort::Auto => "sorts itself: active, in progress, priority, newest",
            TaskSort::Manual => "drag ☰ to reorder; new tasks land on top",
        };
        ui_.label(egui::RichText::new(hint).weak().small());
    });
}

enum Action {
    Complete(Task),
    LogSoFar(Task),
    SetStatus(i64, TaskStatus),
    SetPriority(i64, Priority),
    Activate(i64),
    Delete(i64),
    /// Move task .0 above (.2 = true) or below task .1 in the manual order.
    Reorder(i64, i64, bool),
}

/// One open-task row. The active task gets an accent background. When
/// `draggable`, the row grows a ☰ grip (drag source) and doubles as a drop
/// target for other rows.
fn task_row(
    app: &mut WorklogApp,
    ui_: &mut egui::Ui,
    task: &Task,
    show_code: bool,
    draggable: bool,
    action: &mut Option<Action>,
) {
    let is_active = app.active_task_id == Some(task.id);
    let frame = if is_active {
        egui::Frame::new()
            .fill(ui_.visuals().selection.bg_fill.linear_multiply(0.22))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 3))
    } else {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 3))
    };
    let row = frame.show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.horizontal(|ui_| {
            if draggable {
                ui_.dnd_drag_source(egui::Id::new(("task_drag", task.id)), task.id, |ui_| {
                    ui_.label(egui::RichText::new("☰").weak())
                        .on_hover_text("drag to reorder");
                });
            }
            // done checkbox → triggers the bridge
            let mut done = false;
            if ui_.checkbox(&mut done, "").on_hover_text("mark done + log time").clicked() {
                *action = Some(Action::Complete(task.clone()));
            }
            if let Some(p) = priority_button(ui_, task.priority) {
                *action = Some(Action::SetPriority(task.id, p));
            }
            if show_code {
                ui_.label(egui::RichText::new(&task.project_code).monospace().weak());
            }
            if is_active {
                ui_.label(
                    egui::RichText::new("⏵")
                        .color(ui_.visuals().selection.stroke.color)
                        .strong(),
                );
            }
            let title = if task.status == TaskStatus::Doing {
                egui::RichText::new(&task.title).strong()
            } else {
                egui::RichText::new(&task.title)
            };
            let mut hover = format!(
                "added {}",
                task.created_at
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d")
            );
            let tracked = task.tracked_secs(chrono::Utc::now());
            if tracked > 0 {
                hover.push_str(&format!(
                    "\n⏱ {} tracked",
                    crate::report::fmt_duration(tracked)
                ));
            }
            ui_.label(title).on_hover_text(hover);
            ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                if ui::confirm_delete_button(ui_, &mut app.confirm_delete_task, task.id) {
                    *action = Some(Action::Delete(task.id));
                }
                match task.status {
                    TaskStatus::Todo => {
                        if ui_
                            .button("⏵ start")
                            .on_hover_text("mark in progress and make it the active task")
                            .clicked()
                        {
                            *action = Some(Action::Activate(task.id));
                        }
                    }
                    TaskStatus::Doing => {
                        if ui_.button("⏸ pause").on_hover_text("back to to-do").clicked() {
                            *action = Some(Action::SetStatus(task.id, TaskStatus::Todo));
                        }
                        if ui_
                            .button("⏱ log")
                            .on_hover_text("log the time tracked so far — the task stays open")
                            .clicked()
                        {
                            *action = Some(Action::LogSoFar(task.clone()));
                        }
                        if !is_active
                            && ui_
                                .button("⏺ focus")
                                .on_hover_text("make this the active task")
                                .clicked()
                        {
                            *action = Some(Action::Activate(task.id));
                        }
                    }
                    TaskStatus::Done => {}
                }
            });
        });
    });

    // Drop target: while another row is being dragged over this one, show an
    // insertion line in the nearer half; dropping moves the dragged task there.
    if draggable && egui::DragAndDrop::has_any_payload(ui_.ctx()) {
        let rect = row.response.rect;
        let Some(pointer) = ui_.ctx().pointer_interact_pos() else {
            return;
        };
        if !rect.contains(pointer) {
            return;
        }
        let before = pointer.y < rect.center().y;
        let y = if before { rect.top() } else { rect.bottom() };
        ui_.painter().hline(
            rect.x_range(),
            y,
            egui::Stroke::new(2.0, ui_.visuals().selection.stroke.color),
        );
        if ui_.input(|i| i.pointer.any_released()) {
            if let Some(src_id) = egui::DragAndDrop::take_payload::<i64>(ui_.ctx()) {
                *action = Some(Action::Reorder(*src_id, task.id, before));
            }
        }
    }
}

/// Manual mode: one flat list in exactly the user's order.
fn manual_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let tasks = app.open_tasks.clone();
    ui_.add_space(4.0);
    for task in &tasks {
        task_row(app, ui_, task, true, true, action);
    }
}

/// Default view: status sections (in progress, then to-do).
fn status_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let tasks = app.open_tasks.clone();
    let mut current_status = None;
    for task in &tasks {
        if current_status != Some(task.status) {
            current_status = Some(task.status);
            ui_.add_space(4.0);
            let label = match task.status {
                TaskStatus::Doing => "In progress",
                _ => "To do",
            };
            ui_.label(egui::RichText::new(label).weak().small());
        }
        task_row(app, ui_, task, true, false, action);
    }
}

/// Alternative view: one section per project (alphabetical by code),
/// same ordering rules within each (rows stay draggable in manual mode).
fn grouped_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let draggable = app.task_sort == TaskSort::Manual;
    let tasks = app.open_tasks.clone();
    let mut codes: Vec<&str> = tasks.iter().map(|t| t.project_code.as_str()).collect();
    codes.sort_unstable();
    codes.dedup();
    for code in codes {
        let group: Vec<&Task> = tasks.iter().filter(|t| t.project_code == code).collect();
        let name = group
            .first()
            .and_then(|t| app.project(t.project_id))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        ui_.add_space(4.0);
        ui_.label(
            egui::RichText::new(format!("{code} — {name} ({})", group.len()))
                .weak()
                .small(),
        );
        for task in group {
            task_row(app, ui_, task, false, draggable, action);
        }
    }
}

fn done_section(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let done = app.done_tasks.clone();
    // Stable id: the default (title-derived) id changes with the count, which
    // collapsed the section every time a task was reopened from inside it.
    egui::CollapsingHeader::new(format!("Completed ({})", done.len()))
        .id_salt("tasks/completed")
        .default_open(false)
        .show(ui_, |ui_| {
            // Both filters run in SQL (reload on change) so the section stays
            // fast once years of completed tasks pile up.
            ui_.horizontal(|ui_| {
                if ui_
                    .add(
                        egui::TextEdit::singleline(&mut app.done_filter)
                            .hint_text("filter: title or project…")
                            .desired_width(220.0),
                    )
                    .changed()
                {
                    app.reload_tasks();
                }
                if !app.done_filter.is_empty()
                    && ui_.button("✖").on_hover_text("clear filter").clicked()
                {
                    app.done_filter.clear();
                    app.reload_tasks();
                }
                let mut range = app.done_range;
                egui::ComboBox::from_id_salt("tasks/done_range")
                    .selected_text(range.label())
                    .show_ui(ui_, |ui_| {
                        for r in crate::app::FilterRange::ALL {
                            ui_.selectable_value(&mut range, r, r.label());
                        }
                    });
                if range != app.done_range {
                    app.done_range = range;
                    app.reload_tasks();
                }
            });
            for task in &done {
                ui_.horizontal(|ui_| {
                    let when = task
                        .completed_at
                        .map(|t| {
                            t.with_timezone(&chrono::Local)
                                .format("%Y-%m-%d")
                                .to_string()
                        })
                        .unwrap_or_default();
                    ui_.label(egui::RichText::new(when).weak().small().monospace());
                    ui_.label(egui::RichText::new(&task.project_code).monospace().weak());
                    ui_.label(egui::RichText::new(&task.title).weak());
                    ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                        if ui::confirm_delete_button(ui_, &mut app.confirm_delete_task, task.id) {
                            *action = Some(Action::Delete(task.id));
                        }
                        if ui_.button("↺").on_hover_text("reopen").clicked() {
                            *action = Some(Action::SetStatus(task.id, TaskStatus::Todo));
                        }
                    });
                });
            }
            if done.is_empty() {
                let filtered =
                    !app.done_filter.trim().is_empty() || app.done_range != crate::app::FilterRange::All;
                let msg = if filtered { "No match." } else { "Nothing completed yet." };
                ui_.label(egui::RichText::new(msg).weak());
            }
        });
}

fn apply(app: &mut WorklogApp, action: Option<Action>) {
    match action {
        Some(Action::Complete(task)) => app.complete_task(&task),
        Some(Action::LogSoFar(task)) => app.log_midway(&task),
        Some(Action::SetStatus(id, status)) => {
            if let Err(e) = app.db.set_task_status(id, status) {
                app.set_status(format!("Update failed: {e}"));
            }
            // pausing (or reopening to todo) the active task deactivates it
            if status != TaskStatus::Doing && app.active_task_id == Some(id) {
                app.set_active_task(None);
            }
            if status != TaskStatus::Doing {
                let _ = app.db.stop_task_timer(id, chrono::Utc::now());
            }
            app.touch();
        }
        Some(Action::SetPriority(id, priority)) => {
            if let Err(e) = app.db.set_task_priority(id, priority) {
                app.set_status(format!("Update failed: {e}"));
            }
            app.touch();
        }
        Some(Action::Activate(id)) => {
            if let Err(e) = app.db.set_task_status(id, TaskStatus::Doing) {
                app.set_status(format!("Update failed: {e}"));
            }
            let _ = app.db.start_task_timer(id, chrono::Utc::now());
            app.touch();
            app.set_active_task(Some(id));
        }
        Some(Action::Delete(id)) => {
            if let Err(e) = app.db.delete_task(id) {
                app.set_status(format!("Delete failed: {e}"));
            }
            if app.active_task_id == Some(id) {
                app.set_active_task(None);
            }
            app.touch();
        }
        Some(Action::Reorder(src, target, before)) => {
            app.reorder_task(src, target, before);
        }
        None => {}
    }
}
