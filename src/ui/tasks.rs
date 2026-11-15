use egui::{Align, Layout, Margin, RichText, Sense, Stroke, vec2};

use crate::app::{PendingKind, UndoItem, WorklogApp};
use crate::db::{Priority, Task, TaskSort, TaskStatus};
use crate::ui;
use crate::ui::theme::{self as t, Weight};
use crate::ui::widgets::{self as w, icon};

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    if app.pending_log.is_some() {
        bridge_banner(app, ui_);
        ui_.add_space(18.0);
    }
    composer(app, ui_);
    ui_.add_space(16.0);

    let mut action: Option<Action> = None;
    toolbar(app, ui_, &mut action);
    ui_.add_space(16.0);

    let show_panel = app.show_notes_panel;
    ui::notes::with_side_panel(
        app,
        ui_,
        "tasks/notes_panel",
        show_panel,
        |app, ui_| ui::notes::panel(app, ui_, None),
        |app, ui_| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui_, |ui_| {
                    if app.open_tasks.is_empty() {
                        ui_.label(RichText::new("No open tasks.").color(t::MUTED));
                    } else if app.task_filter_active() {
                        found_list(app, ui_, &mut action);
                    } else if app.group_tasks {
                        grouped_list(app, ui_, &mut action);
                    } else if app.task_sort == TaskSort::Manual {
                        manual_list(app, ui_, &mut action);
                    } else {
                        status_list(app, ui_, &mut action);
                    }
                    ui_.add_space(8.0);
                    done_section(app, ui_, &mut action);
                });
        },
    );
    apply(app, action);
}

/// The heart of the app: a just-completed (or stopped) task offering to
/// become a log entry. Pinned above the list, prefilled, hours focused —
/// type hours, hit Enter, done.
fn bridge_banner(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let Some(mut pending) = app.pending_log.take() else {
        return;
    };
    let mut keep = true;

    egui::Frame::new()
        .fill(t::BANNER_BG)
        .stroke(Stroke::new(1.0, t::alpha(t::ACCENT, 0.38)))
        .corner_radius(t::R_CARD)
        .inner_margin(Margin::symmetric(18, 16))
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 12.0;
                let (rect, _) = ui_.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                ui_.painter()
                    .circle_filled(rect.center(), 16.0, t::alpha(t::ACCENT, 0.18));
                ui_.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    icon::CHECK,
                    t::icon_font(16.0),
                    t::ACCENT_TEXT,
                );
                ui_.with_layout(Layout::right_to_left(Align::Min), |ui_| {
                    let tip = match pending.kind {
                        PendingKind::Midway => "Skip — log nothing (Esc)",
                        _ => "Cancel — put the task back as it was, log nothing",
                    };
                    if w::icon_button_ex(ui_, icon::X, tip, 32.0, t::MUTED, true).clicked() {
                        if matches!(pending.kind, PendingKind::Midway) {
                            keep = false;
                        } else {
                            app.undo_complete(&pending);
                            keep = false;
                        }
                    }
                    ui_.with_layout(Layout::top_down(Align::Min), |ui_| {
                        ui_.spacing_mut().item_spacing.y = 2.0;
                        let title = match pending.kind {
                            PendingKind::Completed { .. } => {
                                format!("Done: “{}” — log the time?", pending.task_title)
                            }
                            PendingKind::Midway => format!(
                                "“{}” — log the time so far? (stays open)",
                                pending.task_title
                            ),
                            PendingKind::Stopped { .. } => {
                                format!("Stopped: “{}” — log the time?", pending.task_title)
                            }
                        };
                        ui_.label(w::semibold(title, 15.0));
                        sub_line(ui_, &mut pending);
                    });
                });
            });
            ui_.add_space(10.0);
            ui_.horizontal(|ui_| {
                ui_.add_space(44.0);
                ui_.horizontal_wrapped(|ui_| {
                    ui_.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    let submit = ui::entry_form_fields(
                        app,
                        ui_,
                        &mut pending.form,
                        "bridge",
                        None,
                        Some(ui::FOCUS_BRIDGE_HOURS),
                        false,
                        270.0,
                    );
                    let valid = pending.form.is_valid();
                    if (ui_
                        .add_enabled(valid, w::primary("Log it").min_size(vec2(0.0, 36.0)))
                        .on_hover_text("Enter also saves")
                        .clicked()
                        || (submit && valid))
                        && app.save_entry_form(&pending.form)
                    {
                        app.pending_logged(&pending);
                        keep = false;
                    }
                    if ui_
                        .add(w::secondary("Skip").min_size(vec2(0.0, 36.0)))
                        .on_hover_text("Don't log time; the tracked count is kept for later")
                        .clicked()
                    {
                        keep = false;
                    }
                });
            });
            // Esc = Skip, once nothing is holding keyboard focus (a first Esc
            // just leaves the text field).
            if ui_.ctx().memory(|m| m.focused().is_none())
                && ui_.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                keep = false;
            }
        });

    if keep {
        app.pending_log = Some(pending);
    }
}

/// "Timer tracked 1 h 20 — click to use it", plus the runaway-timer warning.
fn sub_line(ui_: &mut egui::Ui, pending: &mut crate::app::PendingLog) {
    let muted = |s: String| RichText::new(s).size(12.5).color(t::MUTED);
    if pending.tracked_secs > 0 {
        let hours = crate::report::fmt_hours(pending.tracked_secs as f64 / 3600.0);
        ui_.horizontal_wrapped(|ui_| {
            ui_.spacing_mut().item_spacing.x = 4.0;
            ui_.label(muted(format!(
                "Timer tracked {} (≈ {hours} h).",
                crate::report::fmt_duration(pending.tracked_secs)
            )));
            if ui_
                .link(RichText::new("Use it").size(12.5))
                .on_hover_text("Put this value in the hours box — still yours to edit")
                .clicked()
            {
                pending.form.hours_text = hours;
            }
        });
    } else {
        ui_.label(muted(
            "No timer time tracked. Enter the hours you want on the record.".into(),
        ));
    }
    if pending.longest_stretch_secs >= crate::app::LONG_STRETCH_SECS {
        w::icon_label(
            ui_,
            &format!(
                "{} Includes one unbroken stretch of {} — timer left running?",
                icon::WARNING,
                crate::report::fmt_duration(pending.longest_stretch_secs),
            ),
            12.5,
            t::AMBER,
        );
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
                            // show_mut: checkboxes toggle in place; the edit
                            // lands in d.text and rides the normal save path.
                            egui_commonmark::CommonMarkViewer::new().show_mut(
                                ui_,
                                &mut app.md_cache,
                                &mut d.text,
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

/// The notes icon on a row. Hovering it shows the rendered notes
/// (truncated when very long) for a quick glance.
fn details_button(app: &mut WorklogApp, ui_: &mut egui::Ui, task: &Task, row_hovered: bool) {
    let has_notes = !task.details.trim().is_empty();
    if !has_notes && !row_hovered {
        // keep the slot so the columns don't jump on hover
        ui_.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        return;
    }
    let color = if has_notes { t::TEXT_2 } else { t::DISABLED };
    let resp = w::icon_button_ex(ui_, icon::NOTE, "", 28.0, color, true);
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
                    ui_.label(RichText::new("… click for the rest").color(t::MUTED).small());
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
        resp.on_hover_text("Add notes")
    };
    if resp.clicked() {
        app.open_task_details(task);
    }
}

/// Cycle button for a priority value. Returns Some(new) when clicked.
fn priority_button(ui_: &mut egui::Ui, priority: Priority) -> Option<Priority> {
    let (glyph, color) = match priority {
        Priority::High => (icon::ARROW_UP, t::PALETTE[2].base),
        Priority::Normal => (icon::MINUS, t::DISABLED),
        Priority::Low => (icon::ARROW_DOWN, t::PALETTE[0].base),
    };
    let tip = format!("Priority: {} — click to change", priority.label());
    let clicked = w::icon_button_ex(ui_, glyph, &tip, 24.0, color, true).clicked();
    clicked.then(|| priority.cycled())
}

/// Project + priority + "Add a task…" + Add.
fn composer(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 10.0;
        let projects = app.projects.clone();
        ui_.scope(|ui_| {
            ui::field_style(ui_);
            let v = &mut ui_.visuals_mut().widgets;
            v.inactive.weak_bg_fill = t::PANEL;
            v.hovered.weak_bg_fill = t::PANEL;
            ui::project_combo(ui_, "tasks/new_proj", &projects, &mut app.new_task_project);
        });
        if let Some(p) = priority_button(ui_, app.new_task_priority) {
            app.new_task_priority = p;
        }
        let mut title = std::mem::take(&mut app.new_task_title);
        let resp = w::input(
            ui_,
            egui::TextEdit::singleline(&mut title)
                .hint_text(RichText::new("Add a task…").color(t::FAINT))
                .background_color(t::PANEL)
                .desired_width(ui_.available_width() - 100.0),
        );
        app.apply_focus(ui::FOCUS_TASK_TITLE, &resp);
        let submit = ui::enter_pressed(&resp, ui_);
        let valid = app.new_task_project.is_some() && !title.trim().is_empty();
        let clicked = ui_
            .add_enabled(
                valid,
                w::primary(format!("{}  Add", icon::PLUS)).min_size(vec2(80.0, 36.0)),
            )
            .clicked();
        if (clicked || (submit && valid))
            && let Some(pid) = app.new_task_project
        {
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
        app.new_task_title = title;
    });
}

/// The task finder (a fuzzy filter over the open list — type the letters
/// you remember, in order, and Enter resumes the top hit) plus the view
/// options on the right.
fn toolbar(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 12.0;
        let resp = ui::journal::search_field(
            ui_,
            &mut app.task_filter,
            "Find a task — fuzzy match",
            340.0,
            Some("Ctrl K"),
        );
        app.apply_focus(ui::FOCUS_TASK_FILTER, &resp);
        // Esc clears the box (and a second one leaves it, egui's own doing).
        if resp.lost_focus() && ui_.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.task_filter.clear();
        }
        let hint = if app.task_filter_active() {
            let found = app.visible_tasks();
            if ui::enter_pressed(&resp, ui_)
                && let Some(top) = found.first()
            {
                app.selected_task_id = Some(top.id);
                app.set_status(format!("Resumed “{}”", top.title));
                *action = Some(Action::Activate(top.id));
                app.task_filter.clear();
            }
            format!(
                "{} of {} open · best match first — Enter resumes it",
                found.len(),
                app.open_tasks.len()
            )
        } else {
            "Enter resumes the best match".to_string()
        };
        ui_.label(RichText::new(hint).size(12.0).color(t::MUTED));

        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
            ui_.spacing_mut().item_spacing.x = 16.0;
            ui::journal::notes_panel_toggle(app, ui_);
            let mut group = app.group_tasks;
            w::toggle(ui_, &mut group, "Group by project");
            if group != app.group_tasks {
                app.group_tasks = group;
                let _ = app.db.set_setting("group_tasks", if group { "1" } else { "0" });
            }
            let mut sort = app.task_sort;
            let label = |s: TaskSort| match s {
                TaskSort::Auto => "Auto",
                TaskSort::Manual => "Manual",
            };
            let mut text = egui::text::LayoutJob::default();
            text.append("Order  ", 0.0, egui::TextFormat::simple(t::font(13.0, Weight::Regular), t::FAINT));
            text.append(label(sort), 0.0, egui::TextFormat::simple(t::font(13.0, Weight::Medium), t::TEXT_2));
            let hint = match (app.task_filter_active(), app.task_sort) {
                (true, _) => "Finding — order and grouping come back when the box is empty",
                (false, TaskSort::Auto) => "Sorts itself: active, in progress, priority, newest",
                (false, TaskSort::Manual) => "Drag the grip to reorder; new tasks land on top",
            };
            egui::ComboBox::from_id_salt("tasks/sort")
                .selected_text(text)
                .show_ui(ui_, |ui_| {
                    ui_.selectable_value(&mut sort, TaskSort::Auto, "Auto");
                    ui_.selectable_value(&mut sort, TaskSort::Manual, "Manual");
                })
                .response
                .on_hover_text(hint);
            if sort != app.task_sort {
                app.set_task_sort(sort);
            }
        });
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

/// A list-group card around `rows`, divided by hairlines.
fn task_group(
    app: &mut WorklogApp,
    ui_: &mut egui::Ui,
    tasks: &[Task],
    show_code: bool,
    draggable: bool,
    action: &mut Option<Action>,
) {
    w::list_frame().show(ui_, |ui_| {
        ui_.spacing_mut().item_spacing.y = 0.0;
        for (n, task) in tasks.iter().enumerate() {
            if n > 0 {
                w::list_divider(ui_);
            }
            task_row(app, ui_, task, show_code, draggable, action);
        }
    });
}

/// One open-task row. The active task gets `SELECTED_ROW`. When
/// `draggable`, the row grows a grip (drag source) and doubles as a drop
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
    let editing = app.editing_task.as_ref().is_some_and(|(id, _)| *id == task.id);
    let sw = app.project_swatch(task.project_id);
    let (_, resp) = w::list_row(
        ui_,
        ("task", task.id),
        52.0,
        is_active || is_selected,
        |ui_| {
            ui_.spacing_mut().item_spacing.x = 10.0;
            let row_hovered = ui_.rect_contains_pointer(ui_.max_rect().expand2(vec2(14.0, 0.0)));
            if draggable {
                ui_.dnd_drag_source(egui::Id::new(("task_drag", task.id)), task.id, |ui_| {
                    ui_.label(RichText::new(icon::DOTS_SIX_VERTICAL).font(t::icon_font(16.0)).color(t::DISABLED))
                        .on_hover_text("Drag to reorder");
                });
            }
            // Renaming: the row becomes a text field. Enter saves, Esc cancels.
            if editing {
                let (_, mut title) = app.editing_task.take().unwrap_or((task.id, String::new()));
                let resp = w::input(
                    ui_,
                    egui::TextEdit::singleline(&mut title)
                        .desired_width(ui_.available_width() - 80.0),
                );
                app.apply_focus(ui::FOCUS_TASK_EDIT, &resp);
                let submit = ui::enter_pressed(&resp, ui_);
                let valid = !title.trim().is_empty();
                let save = w::icon_button_ex(ui_, icon::CHECK, "Save (Enter)", 28.0, t::ACCENT, valid)
                    .clicked()
                    || (submit && valid);
                let cancel = w::icon_button(ui_, icon::X, "Cancel (Esc)").clicked()
                    || (resp.lost_focus() && ui_.input(|i| i.key_pressed(egui::Key::Escape)));
                if save {
                    *action = Some(Action::Rename(task.id, title.trim().to_string()));
                } else if !cancel {
                    app.editing_task = Some((task.id, title));
                }
                return;
            }
            // done checkbox → triggers the bridge
            if w::checkbox(ui_, false, "Mark done + log time").clicked() {
                *action = Some(Action::Complete(task.clone()));
            }
            if let Some(p) = priority_button(ui_, task.priority) {
                *action = Some(Action::SetPriority(task.id, p));
            }
            if show_code {
                ui_.label(w::mono_semi(&task.project_code, 12.5).color(sw.light));
            }
            ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                ui_.spacing_mut().item_spacing.x = 2.0;
                start_pause_button(ui_, task, action);
                ui_.add_space(6.0);
                if ui::confirm_delete_button(ui_, &mut app.confirm_delete_task, task.id) {
                    *action = Some(Action::Delete(task.id));
                }
                if w::icon_button(ui_, icon::PENCIL_SIMPLE, "Edit title").clicked() {
                    app.start_task_edit(task);
                }
                if task.status == TaskStatus::Doing {
                    if w::icon_button(ui_, icon::CLOCK, "Log the time tracked so far — the task stays open")
                        .clicked()
                    {
                        *action = Some(Action::LogSoFar(task.clone()));
                    }
                    if !is_active
                        && w::icon_button(ui_, icon::CROSSHAIR, "Make this the active task").clicked()
                    {
                        *action = Some(Action::Activate(task.id));
                    }
                }
                details_button(app, ui_, task, row_hovered);
                ui_.spacing_mut().item_spacing.x = 10.0;
                if task.status == TaskStatus::Doing && task.started_at.is_some() {
                    let secs = task.tracked_secs(chrono::Utc::now());
                    ui_.label(w::mono_semi(ui::shell::clock(secs), 13.0).color(egui::Color32::WHITE));
                    ui_.label(
                        RichText::new("● Running")
                            .font(t::font(12.0, Weight::Semibold))
                            .color(t::ACCENT_TEXT),
                    );
                    ui_.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                }
                if let Some((done, total)) = task.checklist() {
                    ui_.label(w::mono(format!("{done}/{total}"), 12.0).color(t::ACCENT_TEXT))
                        .on_hover_text("Checklist in the notes");
                    w::progress(ui_, done as f32 / total as f32, 36.0, 4.0, t::ACCENT);
                }
                ui_.with_layout(Layout::left_to_right(Align::Center), |ui_| {
                    let title = RichText::new(&task.title).size(14.0).color(t::TEXT);
                    let title = if task.status == TaskStatus::Doing {
                        title.font(t::font(14.0, Weight::Medium))
                    } else {
                        title
                    };
                    let mut hover = format!(
                        "Added {}",
                        task.created_at.with_timezone(&chrono::Local).format("%Y-%m-%d")
                    );
                    let tracked = task.tracked_secs(chrono::Utc::now());
                    if tracked > 0 {
                        hover.push_str(&format!("\n{} tracked", crate::report::fmt_duration(tracked)));
                    }
                    hover.push_str("\nDouble-click to rename");
                    let title_resp = ui_
                        .add(egui::Label::new(title).truncate().sense(Sense::click()))
                        .on_hover_text(hover);
                    if title_resp.double_clicked() {
                        app.start_task_edit(task);
                    } else if title_resp.clicked() {
                        app.selected_task_id = Some(task.id);
                    }
                });
            });
        },
    );
    if resp.clicked() {
        app.selected_task_id = Some(task.id);
    }
    if resp.double_clicked() {
        app.start_task_edit(task);
    }

    // Drop target: while another row is being dragged over this one, show an
    // insertion line in the nearer half; dropping moves the dragged task there.
    if draggable && egui::DragAndDrop::has_any_payload(ui_.ctx()) {
        let rect = resp.rect;
        let Some(pointer) = ui_.ctx().pointer_interact_pos() else {
            return;
        };
        if !rect.contains(pointer) {
            return;
        }
        let before = pointer.y < rect.center().y;
        let y = if before { rect.top() } else { rect.bottom() };
        ui_.painter().hline(rect.x_range(), y, Stroke::new(2.0, t::ACCENT));
        if ui_.input(|i| i.pointer.any_released())
            && let Some(src_id) = egui::DragAndDrop::take_payload::<i64>(ui_.ctx())
        {
            *action = Some(Action::Reorder(*src_id, task.id, before));
        }
    }
}

/// Fixed-width ▶ Start (secondary) / ❚❚ Pause (primary).
fn start_pause_button(ui_: &mut egui::Ui, task: &Task, action: &mut Option<Action>) {
    let size = vec2(84.0, 32.0);
    match task.status {
        TaskStatus::Doing => {
            if ui_
                .add(w::primary(format!("{} Pause", icon::PAUSE)).min_size(size))
                .on_hover_text("Back to to-do (timer stops)")
                .clicked()
            {
                *action = Some(Action::SetStatus(task.id, TaskStatus::Todo));
            }
        }
        _ => {
            if ui_
                .add(w::secondary(format!("{} Start", icon::PLAY)).min_size(size))
                .on_hover_text("Mark in progress, start the timer and make it the active task")
                .clicked()
            {
                *action = Some(Action::Activate(task.id));
            }
        }
    }
}

/// Finder mode: one flat list, best match first. Sections, grouping and the
/// drag grip step aside — while you're searching, row one is the answer.
fn found_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let tasks = app.visible_tasks();
    if tasks.is_empty() {
        ui_.label(RichText::new("No open task matches.").color(t::MUTED));
        let query = app.task_filter.clone();
        if ui_
            .link("Look in Completed instead")
            .on_hover_text("Same search over finished tasks, all the way back")
            .clicked()
        {
            app.search_completed(query);
        }
        return;
    }
    task_group(app, ui_, &tasks, true, false, action);
    ui_.add_space(10.0);
}

/// Manual mode: one flat list in exactly the user's order.
fn manual_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let tasks = app.open_tasks.clone();
    task_group(app, ui_, &tasks, true, true, action);
    ui_.add_space(10.0);
}

/// Default view: status sections (in progress, then to-do).
fn status_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let tasks = app.open_tasks.clone();
    for (label, doing) in [("In progress", true), ("To do", false)] {
        let section: Vec<Task> = tasks
            .iter()
            .filter(|t| (t.status == TaskStatus::Doing) == doing)
            .cloned()
            .collect();
        if section.is_empty() {
            continue;
        }
        ui_.horizontal(|ui_| {
            ui_.add_space(4.0);
            w::eyebrow(ui_, label);
            w::count_pill(ui_, section.len());
        });
        ui_.add_space(4.0);
        task_group(app, ui_, &section, true, false, action);
        ui_.add_space(18.0);
    }
}

/// Alternative view: one collapsible section per project (alphabetical by
/// code), same ordering rules within each (rows stay draggable in manual mode).
fn grouped_list(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let draggable = app.task_sort == TaskSort::Manual;
    let tasks = app.open_tasks.clone();
    let mut codes: Vec<&str> = tasks.iter().map(|t| t.project_code.as_str()).collect();
    codes.sort_unstable();
    codes.dedup();
    for code in codes {
        let group: Vec<Task> = tasks
            .iter()
            .filter(|t| t.project_code == code)
            .cloned()
            .collect();
        let project_id = group.first().map(|t| t.project_id).unwrap_or_default();
        let (name, client) = app
            .project(project_id)
            .map(|p| (p.name.clone(), p.client.clone()))
            .unwrap_or_default();
        let sw = app.project_swatch(project_id);
        // Stable id: keyed on the code so the collapsed state survives adds.
        let open = w::collapsible_header(
            ui_,
            egui::Id::new(("tasks/group", code)),
            true,
            false,
            |ui_| {
                w::dot(ui_, sw.base, 8.0);
                ui_.label(w::mono_semi(code, 13.0).color(sw.light));
                ui_.label(RichText::new(&name).size(13.0).color(t::MUTED));
                if let Some(c) = client.as_deref().filter(|c| !c.trim().is_empty()) {
                    w::client_tag(ui_, c);
                }
                w::count_pill(ui_, group.len());
            },
        );
        ui_.add_space(4.0);
        if open {
            task_group(app, ui_, &group, false, draggable, action);
        }
        ui_.add_space(18.0);
    }
}

fn done_section(app: &mut WorklogApp, ui_: &mut egui::Ui, action: &mut Option<Action>) {
    let done = app.done_tasks.clone();
    // One-shot force-open: the finder handing its query over here.
    let force_open = std::mem::take(&mut app.expand_completed);
    let open = w::collapsible_header(
        ui_,
        egui::Id::new("tasks/completed"),
        false,
        force_open,
        |ui_| {
            ui_.label(RichText::new("Completed").font(t::font(13.0, Weight::Medium)).color(t::TEXT_2));
            w::count_pill(ui_, done.len());
        },
    );
    if !open {
        return;
    }
    ui_.add_space(8.0);
    // Both filters run in SQL (reload on change) so the section stays fast
    // once years of completed tasks pile up.
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 10.0;
        if ui::journal::search_field(ui_, &mut app.done_filter, "Find: any letters, in order…", 260.0, None)
            .changed()
        {
            app.reload_tasks();
        }
        let mut range = app.done_range;
        egui::ComboBox::from_id_salt("tasks/done_range")
            .selected_text(ui::journal::capitalized(range.label()))
            .show_ui(ui_, |ui_| {
                for r in crate::app::FilterRange::ALL {
                    ui_.selectable_value(&mut range, r, ui::journal::capitalized(r.label()));
                }
            });
        if range != app.done_range {
            app.done_range = range;
            app.reload_tasks();
        }
    });
    ui_.add_space(8.0);
    if done.is_empty() {
        let filtered =
            !app.done_filter.trim().is_empty() || app.done_range != crate::app::FilterRange::All;
        let msg = if filtered { "No match." } else { "Nothing completed yet." };
        ui_.label(RichText::new(msg).color(t::MUTED));
        return;
    }
    w::list_frame().show(ui_, |ui_| {
        ui_.spacing_mut().item_spacing.y = 0.0;
        for (n, task) in done.iter().enumerate() {
            if n > 0 {
                w::list_divider(ui_);
            }
            let selected = app.show_notes_panel && app.selected_task_id == Some(task.id);
            let sw = app.project_swatch(task.project_id);
            let (_, resp) = w::list_row(ui_, ("done", task.id), 44.0, selected, |ui_| {
                ui_.spacing_mut().item_spacing.x = 12.0;
                let when = task
                    .completed_at
                    .map(|c| c.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                ui_.label(w::mono(when, 12.0).color(t::FAINT));
                ui_.label(w::mono_semi(&task.project_code, 12.5).color(sw.light));
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    ui_.spacing_mut().item_spacing.x = 2.0;
                    if ui::confirm_delete_button(ui_, &mut app.confirm_delete_task, task.id) {
                        *action = Some(Action::Delete(task.id));
                    }
                    if w::icon_button(ui_, icon::ARROW_COUNTER_CLOCKWISE, "Reopen").clicked() {
                        *action = Some(Action::SetStatus(task.id, TaskStatus::Todo));
                    }
                    if !task.details.trim().is_empty() {
                        details_button(app, ui_, task, true);
                    }
                    ui_.with_layout(Layout::left_to_right(Align::Center), |ui_| {
                        ui_.add(
                            egui::Label::new(RichText::new(&task.title).size(14.0).color(t::MUTED))
                                .truncate(),
                        );
                    });
                });
            });
            if resp.clicked() {
                app.selected_task_id = Some(task.id);
            }
        }
    });
}

fn apply(app: &mut WorklogApp, action: Option<Action>) {
    match action {
        Some(Action::Complete(task)) => app.complete_task(&task),
        Some(Action::LogSoFar(task)) => app.log_midway(&task),
        Some(Action::SetStatus(id, status)) => app.change_task_status(id, status),
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
        Some(Action::Activate(id)) => app.activate_task(id),
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
