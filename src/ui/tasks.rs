use crate::app::{PendingKind, UndoItem, WorklogApp};
use crate::db::{Priority, Task, TaskSort, TaskStatus};
use crate::ui;

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    bridge_strip(app, ui_);
    add_task_row(app, ui_);
    ui_.separator();
    list_controls(app, ui_);

    let mut action: Option<Action> = None;
    if app.show_notes_panel {
        egui::Panel::right(egui::Id::new("tasks/notes_panel"))
            .resizable(true)
            .default_size(280.0)
            .show(ui_, |ui_| notes_panel(app, ui_));
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new())
        .show(ui_, |ui_| {
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

/// A formatting action from the notes toolbar.
enum Md {
    /// Wrap the selection in a symmetric marker (`**`, `*`, `~~`, `` ` ``).
    Wrap(&'static str),
    /// Toggle a prefix on every selected line (`# `, `- `, `> `, `- [ ] `).
    LinePrefix(&'static str),
    /// Number the selected lines `1. 2. 3.` (or strip the numbers).
    Numbered,
    CodeBlock,
    Rule,
    Table,
}

fn byte_of(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

/// Byte length of a `N. ` list prefix, if the line has one.
fn numbered_prefix(line: &str) -> Option<usize> {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    (digits > 0 && line[digits..].starts_with(". ")).then_some(digits + 2)
}

/// Apply a toolbar action to `text` at the (sorted, char-indexed) selection.
/// Wrap toggles off when the markers are already there; line prefixes toggle
/// per block, and a heading replaces whatever heading level was in place.
/// Returns the new selection, again in chars.
fn apply_md(text: &mut String, sel: (usize, usize), md: &Md) -> (usize, usize) {
    let (a, b) = sel;
    let (ab, bb) = (byte_of(text, a), byte_of(text, b));
    // The whole lines the selection touches — what line-based actions edit.
    let line_start = text[..ab].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[bb..].find('\n').map(|i| bb + i).unwrap_or(text.len());
    let select_block = |text: &String, new_block: &str| {
        let s = text[..line_start].chars().count();
        (s, s + new_block.chars().count())
    };
    match md {
        Md::Wrap(m) => {
            let ml = m.chars().count();
            let selected = text[ab..bb].to_string();
            if selected.len() >= 2 * m.len() && selected.starts_with(m) && selected.ends_with(m) {
                let inner = selected[m.len()..selected.len() - m.len()].to_string();
                text.replace_range(ab..bb, &inner);
                (a, b - 2 * ml)
            } else if text[..ab].ends_with(m) && text[bb..].starts_with(m) {
                text.replace_range(bb..bb + m.len(), "");
                text.replace_range(ab - m.len()..ab, "");
                (a - ml, b - ml)
            } else {
                text.insert_str(bb, m);
                text.insert_str(ab, m);
                (a + ml, b + ml)
            }
        }
        Md::LinePrefix(p) => {
            let lines: Vec<String> =
                text[line_start..line_end].split('\n').map(String::from).collect();
            let heading = p.starts_with('#');
            let all_have = lines.iter().all(|l| l.starts_with(p));
            let new_block = lines
                .iter()
                .map(|l| {
                    if all_have {
                        l[p.len()..].to_string()
                    } else if heading {
                        let bare = l.trim_start_matches('#');
                        let bare = bare.strip_prefix(' ').unwrap_or(bare);
                        format!("{p}{bare}")
                    } else {
                        format!("{p}{l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            text.replace_range(line_start..line_end, &new_block);
            select_block(text, &new_block)
        }
        Md::Numbered => {
            let lines: Vec<String> =
                text[line_start..line_end].split('\n').map(String::from).collect();
            let all_have = lines.iter().all(|l| numbered_prefix(l).is_some());
            let new_block = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    if all_have {
                        l[numbered_prefix(l).unwrap()..].to_string()
                    } else {
                        format!("{}. {l}", i + 1)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            text.replace_range(line_start..line_end, &new_block);
            select_block(text, &new_block)
        }
        Md::CodeBlock => {
            let new_block = format!("```\n{}\n```", &text[line_start..line_end]);
            text.replace_range(line_start..line_end, &new_block);
            select_block(text, &new_block)
        }
        Md::Rule => {
            text.insert_str(line_end, "\n\n---\n");
            let p = text[..line_end].chars().count() + 6;
            (p, p)
        }
        Md::Table => {
            let t = "\n\n| Col 1 | Col 2 |\n| --- | --- |\n|  |  |\n";
            text.insert_str(line_end, t);
            let p = text[..line_end].chars().count() + t.chars().count();
            (p, p)
        }
    }
}

/// The formatting toolbar shown above the notes editor.
fn md_toolbar(ui_: &mut egui::Ui, md: &mut Option<Md>) {
    fn tb(ui_: &mut egui::Ui, label: impl Into<egui::WidgetText>, hint: &str) -> bool {
        ui_.button(label).on_hover_text(hint).clicked()
    }
    use egui::RichText as R;
    ui_.horizontal_wrapped(|ui_| {
        if tb(ui_, R::new("B").strong(), "bold — **text** (Ctrl+B)") {
            *md = Some(Md::Wrap("**"));
        }
        if tb(ui_, R::new("I").italics(), "italic — *text* (Ctrl+I)") {
            *md = Some(Md::Wrap("*"));
        }
        if tb(ui_, R::new("S").strikethrough(), "strikethrough — ~~text~~") {
            *md = Some(Md::Wrap("~~"));
        }
        if tb(ui_, R::new("c").monospace(), "inline code — `text`") {
            *md = Some(Md::Wrap("`"));
        }
        ui_.separator();
        ui_.menu_button("H", |ui_| {
            for (label, p) in [("Heading 1", "# "), ("Heading 2", "## "), ("Heading 3", "### ")] {
                if ui_.button(label).clicked() {
                    *md = Some(Md::LinePrefix(p));
                    ui_.close();
                }
            }
        })
        .response
        .on_hover_text("heading");
        if tb(ui_, "•", "bullet list") {
            *md = Some(Md::LinePrefix("- "));
        }
        if tb(ui_, "1.", "numbered list") {
            *md = Some(Md::Numbered);
        }
        if tb(ui_, "☑", "task list — - [ ]") {
            *md = Some(Md::LinePrefix("- [ ] "));
        }
        ui_.separator();
        if tb(ui_, "❝", "quote") {
            *md = Some(Md::LinePrefix("> "));
        }
        if tb(ui_, R::new("</>").monospace(), "code block") {
            *md = Some(Md::CodeBlock);
        }
        if tb(ui_, "⊞", "table") {
            *md = Some(Md::Table);
        }
        if tb(ui_, "—", "horizontal rule") {
            *md = Some(Md::Rule);
        }
    });
}

/// The notes window: one task's markdown details, with an Edit/Preview
/// toggle. Closing the window (or the Save button) saves — notes are too
/// long to lose to a misclick, so there is no discard path.
pub fn details_window(app: &mut WorklogApp, ctx: &egui::Context) {
    let Some(mut d) = app.task_details.take() else {
        return;
    };
    let mut open = true;
    let mut save = false;
    egui::Window::new(format!("Notes — {}", d.task_title))
        .id(egui::Id::new("task_details"))
        .open(&mut open)
        .resizable(true)
        .default_width(540.0)
        .default_height(420.0)
        .show(ctx, |ui_| {
            ui_.horizontal(|ui_| {
                ui_.selectable_value(&mut d.preview, false, "Edit");
                ui_.selectable_value(&mut d.preview, true, "Preview");
                if ui_.button("Save").clicked() {
                    save = true;
                }
                if d.text != d.saved {
                    ui_.label(egui::RichText::new("unsaved").weak().small());
                }
                ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                    ui_.label(
                        egui::RichText::new("markdown: **bold**  # heading  - list  `code`")
                            .weak()
                            .small(),
                    );
                });
            });
            let editor_id = egui::Id::new("task_details_editor");
            if !d.preview {
                let mut md: Option<Md> = None;
                md_toolbar(ui_, &mut md);
                if ui_.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::B)) {
                    md = Some(Md::Wrap("**"));
                }
                if ui_.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::I)) {
                    md = Some(Md::Wrap("*"));
                }
                if let Some(md) = md {
                    // Last frame's cursor tells us what to format; fall back
                    // to the end of the text if the editor was never focused.
                    let ctx_ = ui_.ctx().clone();
                    let sel = egui::TextEdit::load_state(&ctx_, editor_id)
                        .and_then(|s| s.cursor.char_range())
                        .map(|r| {
                            let (x, y) = (r.primary.index.0, r.secondary.index.0);
                            (x.min(y), x.max(y))
                        })
                        .unwrap_or_else(|| {
                            let n = d.text.chars().count();
                            (n, n)
                        });
                    let (na, nb) = apply_md(&mut d.text, sel, &md);
                    let mut state =
                        egui::TextEdit::load_state(&ctx_, editor_id).unwrap_or_default();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(na),
                        egui::text::CCursor::new(nb),
                    )));
                    state.store(&ctx_, editor_id);
                    ctx_.memory_mut(|m| m.request_focus(editor_id));
                }
            }
            ui_.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui_, |ui_| {
                    if d.preview {
                        if d.text.trim().is_empty() {
                            ui_.label(
                                egui::RichText::new("Nothing here yet — switch to Edit.").weak(),
                            );
                        } else {
                            egui_commonmark::CommonMarkViewer::new().show(
                                ui_,
                                &mut app.md_cache,
                                &d.text,
                            );
                        }
                    } else {
                        ui_.add(
                            egui::TextEdit::multiline(&mut d.text)
                                .id(editor_id)
                                .hint_text("Anything worth remembering about this task…")
                                .desired_width(f32::INFINITY)
                                .desired_rows(16),
                        );
                    }
                });
        });
    if (save || !open) && d.text != d.saved {
        match app.db.set_task_details(d.task_id, d.text.trim()) {
            Ok(()) => {
                d.saved = d.text.clone();
                app.set_status(format!("Notes saved — {}", d.task_title));
            }
            Err(e) => app.set_status(format!("Save failed: {e}")),
        }
        app.touch();
    }
    if open {
        app.task_details = Some(d);
    }
}

/// The 📄 row button; weak while the task has no notes yet. Hovering it
/// shows the rendered notes (truncated when very long) for a quick glance.
fn details_button(app: &mut WorklogApp, ui_: &mut egui::Ui, task: &Task) {
    let has_notes = !task.details.trim().is_empty();
    let icon = if has_notes {
        egui::RichText::new("📄")
    } else {
        egui::RichText::new("📄").weak()
    };
    let resp = ui_.button(icon);
    let resp = if has_notes {
        resp.on_hover_ui(|ui_| {
            ui_.set_max_width(420.0);
            match task.details.char_indices().nth(1200) {
                Some((cut, _)) => {
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui_,
                        &mut app.md_cache,
                        &task.details[..cut],
                    );
                    ui_.label(egui::RichText::new("… click 📄 for the rest").weak().small());
                }
                None => {
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui_,
                        &mut app.md_cache,
                        &task.details,
                    );
                }
            }
        })
    } else {
        resp.on_hover_text("add notes")
    };
    if resp.clicked() {
        app.open_task_details(task);
    }
}

/// Right-hand glance panel: rendered notes for the selected (or active)
/// task. Shown on the Tasks and Journal screens (same toggle).
pub fn notes_panel(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    // DB lookup, not the open list: completed tasks (clicked in the
    // Completed section or via a journal entry) resolve too.
    let task = app
        .selected_task_id
        .and_then(|id| app.db.task(id).ok().flatten())
        .or_else(|| app.active_task().cloned());
    let Some(task) = task else {
        ui_.add_space(8.0);
        ui_.label(
            egui::RichText::new(
                "Click a task (or a journal entry logged from one) to show its notes here.",
            )
            .weak(),
        );
        return;
    };
    ui_.horizontal(|ui_| {
        ui_.label(egui::RichText::new(&task.title).strong());
        if task.status == TaskStatus::Done {
            ui_.label(egui::RichText::new("(completed)").weak().small());
        }
        ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
            if ui_.button("✏").on_hover_text("edit notes").clicked() {
                app.open_task_details(&task);
            }
        });
    });
    ui_.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            if task.details.trim().is_empty() {
                ui_.label(egui::RichText::new("No notes yet — ✏ to add some.").weak());
            } else {
                egui_commonmark::CommonMarkViewer::new().show(ui_, &mut app.md_cache, &task.details);
            }
        });
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

        let mut panel = app.show_notes_panel;
        ui_.checkbox(&mut panel, "notes panel")
            .on_hover_text("show the selected task's notes beside the list");
        if panel != app.show_notes_panel {
            app.show_notes_panel = panel;
            let _ = app.db.set_setting("notes_panel", if panel { "1" } else { "0" });
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
    Rename(i64, String),
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
    let is_selected = app.show_notes_panel && app.selected_task_id == Some(task.id);
    let frame = if is_active {
        egui::Frame::new()
            .fill(ui_.visuals().selection.bg_fill.linear_multiply(0.22))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 3))
    } else if is_selected {
        egui::Frame::new()
            .fill(ui_.visuals().selection.bg_fill.linear_multiply(0.10))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 3))
    } else {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 3))
    };
    let row = frame.show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.horizontal(|ui_| {
            // Renaming: the row becomes a text field. Enter saves, Esc cancels.
            if app.editing_task.as_ref().is_some_and(|(id, _)| *id == task.id) {
                let (_, mut title) = app.editing_task.take().unwrap();
                let resp = ui_.add(
                    egui::TextEdit::singleline(&mut title)
                        .desired_width(ui_.available_width() - 64.0),
                );
                app.apply_focus(ui::FOCUS_TASK_EDIT, &resp);
                let submit = ui::enter_pressed(&resp, ui_);
                let valid = !title.trim().is_empty();
                let save = ui_
                    .add_enabled(valid, egui::Button::new("✔"))
                    .on_hover_text("save (Enter)")
                    .clicked()
                    || (submit && valid);
                let cancel = ui_.button("✖").on_hover_text("cancel (Esc)").clicked()
                    || (resp.lost_focus() && ui_.input(|i| i.key_pressed(egui::Key::Escape)));
                if save {
                    *action = Some(Action::Rename(task.id, title.trim().to_string()));
                } else if !cancel {
                    app.editing_task = Some((task.id, title));
                }
                return;
            }
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
            hover.push_str("\ndouble-click to edit");
            let title_resp = ui_
                .add(egui::Label::new(title).sense(egui::Sense::click()))
                .on_hover_text(hover);
            if title_resp.double_clicked() {
                app.start_task_edit(task);
            } else if title_resp.clicked() {
                app.selected_task_id = Some(task.id);
            }
            ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                if ui::confirm_delete_button(ui_, &mut app.confirm_delete_task, task.id) {
                    *action = Some(Action::Delete(task.id));
                }
                if ui_.button("✏").on_hover_text("edit title").clicked() {
                    app.start_task_edit(task);
                }
                details_button(app, ui_, task);
                if let Some((done, total)) = task.checklist() {
                    let text = egui::RichText::new(format!("{done}/{total}")).small();
                    let text = if done == total {
                        text.color(egui::Color32::from_rgb(80, 160, 95))
                    } else {
                        text.weak()
                    };
                    ui_.label(text).on_hover_text("checklist in the notes");
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
                    let title_resp = ui_.add(
                        egui::Label::new(egui::RichText::new(&task.title).weak())
                            .sense(egui::Sense::click()),
                    );
                    if title_resp
                        .on_hover_text("click to show notes in the side panel")
                        .clicked()
                    {
                        app.selected_task_id = Some(task.id);
                    }
                    ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                        if ui::confirm_delete_button(ui_, &mut app.confirm_delete_task, task.id) {
                            *action = Some(Action::Delete(task.id));
                        }
                        if ui_.button("↺").on_hover_text("reopen").clicked() {
                            *action = Some(Action::SetStatus(task.id, TaskStatus::Todo));
                        }
                        if !task.details.trim().is_empty() {
                            details_button(app, ui_, task);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_adds_and_toggles() {
        let mut t = String::from("hello world");
        assert_eq!(apply_md(&mut t, (6, 11), &Md::Wrap("**")), (8, 13));
        assert_eq!(t, "hello **world**");
        // same selection (the inner word) toggles it back off
        assert_eq!(apply_md(&mut t, (8, 13), &Md::Wrap("**")), (6, 11));
        assert_eq!(t, "hello world");
        // selecting the markers too also toggles off
        apply_md(&mut t, (6, 11), &Md::Wrap("~~"));
        assert_eq!(apply_md(&mut t, (6, 15), &Md::Wrap("~~")), (6, 11));
        assert_eq!(t, "hello world");
    }

    #[test]
    fn empty_selection_wrap_places_cursor_between_markers() {
        let mut t = String::from("ab");
        assert_eq!(apply_md(&mut t, (2, 2), &Md::Wrap("**")), (4, 4));
        assert_eq!(t, "ab****");
    }

    #[test]
    fn heading_swaps_level_and_toggles() {
        let mut t = String::from("title");
        apply_md(&mut t, (0, 0), &Md::LinePrefix("# "));
        assert_eq!(t, "# title");
        apply_md(&mut t, (3, 3), &Md::LinePrefix("## "));
        assert_eq!(t, "## title");
        apply_md(&mut t, (0, 0), &Md::LinePrefix("## "));
        assert_eq!(t, "title");
    }

    #[test]
    fn lists_cover_every_selected_line() {
        let mut t = String::from("a\nb\nc");
        apply_md(&mut t, (0, 5), &Md::LinePrefix("- "));
        assert_eq!(t, "- a\n- b\n- c");
        let mut t = String::from("a\nb");
        apply_md(&mut t, (0, 3), &Md::Numbered);
        assert_eq!(t, "1. a\n2. b");
        let len = t.chars().count();
        apply_md(&mut t, (0, len), &Md::Numbered);
        assert_eq!(t, "a\nb");
    }

    #[test]
    fn code_block_wraps_selected_lines() {
        let mut t = String::from("let x = 1;");
        apply_md(&mut t, (2, 5), &Md::CodeBlock);
        assert_eq!(t, "```\nlet x = 1;\n```");
    }

    #[test]
    fn wrap_survives_multibyte_text() {
        let mut t = String::from("café été");
        assert_eq!(apply_md(&mut t, (5, 8), &Md::Wrap("**")), (7, 10));
        assert_eq!(t, "café **été**");
    }
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
        Some(Action::Rename(id, title)) => {
            match app.db.set_task_title(id, &title) {
                Ok(()) => app.set_status("Task updated"),
                Err(e) => app.set_status(format!("Update failed: {e}")),
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
            let deleted = app
                .open_tasks
                .iter()
                .chain(app.done_tasks.iter())
                .find(|t| t.id == id)
                .cloned();
            match app.db.delete_task(id) {
                Ok(()) => {
                    if let Some(task) = deleted {
                        app.offer_undo(UndoItem::Task(task));
                    }
                }
                Err(e) => app.set_status(format!("Delete failed: {e}")),
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
