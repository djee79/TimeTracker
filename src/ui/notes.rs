//! The notes side panel shared by Journal and Tasks: the selected task's
//! checklist as real checkboxes, an "add an item" box, the rest of its
//! markdown rendered — plus the two-column layout that hosts it.

use egui::{Align, Frame, Layout, Margin, RichText, Sense, Vec2, vec2};

use super::theme::{self as t, Weight};
use super::widgets::{self as w, icon};
use crate::app::WorklogApp;
use crate::db::{LogEntryRow, Task, TaskStatus};
use crate::report::fmt_hours;

pub const PANEL_WIDTH: f32 = 340.0;
/// Below this the panel moves under the list instead of beside it.
const NARROW: f32 = 900.0;

/// Main content beside (or, on narrow windows, above) the notes panel.
pub fn with_side_panel(
    app: &mut WorklogApp,
    ui: &mut egui::Ui,
    id: &str,
    show_panel: bool,
    width: f32,
    side: impl FnOnce(&mut WorklogApp, &mut egui::Ui),
    main: impl FnOnce(&mut WorklogApp, &mut egui::Ui),
) {
    if show_panel {
        if ui.available_width() >= NARROW {
            egui::Panel::right(egui::Id::new(id))
                .exact_size(width + 20.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().inner_margin(Margin { left: 20, ..Default::default() }))
                .show(ui, |ui| side(app, ui));
        } else {
            egui::Panel::bottom(egui::Id::new(id).with("narrow"))
                .default_size(300.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::new().inner_margin(Margin { top: 16, ..Default::default() }))
                .show(ui, |ui| side(app, ui));
        }
    }
    egui::CentralPanel::default()
        .frame(Frame::new())
        .show(ui, |ui| main(app, ui));
}

// ---- checklist parsing ----

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistItem {
    /// Line index in the notes.
    pub line: usize,
    pub checked: bool,
    pub text: String,
}

/// Byte offset of the `[ ]`/`[x]` marker on a checklist line, if it is one
/// (`- [ ]`, `* [x]`, `+ [ ]`, `1. [ ]`, any indentation).
fn marker_offset(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let l = &line[indent..];
    let after_bullet = if let Some(r) = l
        .strip_prefix("- ")
        .or_else(|| l.strip_prefix("* "))
        .or_else(|| l.strip_prefix("+ "))
    {
        r
    } else {
        let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        l[digits..].strip_prefix(". ")?
    };
    let pad = after_bullet.len() - after_bullet.trim_start().len();
    let rest = &after_bullet[pad..];
    let is_marker = rest.starts_with("[ ]") || rest.starts_with("[x]") || rest.starts_with("[X]");
    is_marker.then(|| line.len() - rest.len())
}

pub fn checklist_items(text: &str) -> Vec<ChecklistItem> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let at = marker_offset(line)?;
            let checked = &line[at + 1..at + 2] != " ";
            Some(ChecklistItem {
                line: i,
                checked,
                text: line[at + 3..].trim().to_string(),
            })
        })
        .collect()
}

/// Flip the checkbox on `line` (`[ ]` ↔ `[x]`); other lines untouched.
pub fn toggle_item(text: &str, line: usize) -> String {
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    if let Some(l) = out.get_mut(line)
        && let Some(at) = marker_offset(l)
    {
        let new = if &l[at + 1..at + 2] == " " { "[x]" } else { "[ ]" };
        l.replace_range(at..at + 3, new);
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Append `- [ ] item` as a new last line.
pub fn append_item(text: &str, item: &str) -> String {
    let mut out = text.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str("- [ ] ");
    out.push_str(item.trim());
    out
}

/// The notes without their checklist lines — rendered as markdown below
/// the checkbox rows.
pub fn without_checklist(text: &str) -> String {
    text.lines()
        .filter(|l| marker_offset(l).is_none())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- the panel ----

/// Notes for the selected task (else the active one). `entry` is the
/// journal entry it was reached from, for the date/hours meta line.
pub fn panel(app: &mut WorklogApp, ui: &mut egui::Ui, entry: Option<LogEntryRow>) {
    let task = app
        .selected_task_id
        .and_then(|id| app.db.task(id).ok().flatten())
        .or_else(|| app.active_task().cloned());
    let Some(task) = task else {
        empty_state(ui);
        return;
    };
    let entry = entry.filter(|e| e.entry.task_id == Some(task.id));
    w::card_frame()
        .inner_margin(Margin::symmetric(20, 18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| body(app, ui, &task, entry.as_ref()));
        });
}

fn body(app: &mut WorklogApp, ui: &mut egui::Ui, task: &Task, entry: Option<&LogEntryRow>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    let sw = app.project_swatch(task.project_id);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if w::icon_button(ui, icon::PENCIL_SIMPLE, "Edit notes").clicked() {
                app.open_task_details(task);
            }
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                w::eyebrow(ui, "Notes");
                ui.add(
                    egui::Label::new(
                        RichText::new(&task.title).font(t::font(18.0, Weight::Semibold)).color(t::TEXT),
                    )
                    .wrap(),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    w::dot(ui, sw.base, 7.0);
                    ui.label(w::mono(&task.project_code, 12.5).color(sw.light));
                    for part in meta(task, entry) {
                        ui.label(RichText::new("·").size(12.5).color(t::MUTED));
                        ui.label(RichText::new(part).size(12.5).color(t::MUTED));
                    }
                });
            });
        });
    });
    ui.add_space(12.0);

    // the editor window holds its own (possibly unsaved) copy — stay
    // read-only so a click here can't be clobbered by its save
    let editing = app.task_details.as_ref().is_some_and(|d| d.task_id == task.id);
    let items = checklist_items(&task.details);
    if !items.is_empty() {
        let done = items.iter().filter(|i| i.checked).count();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Checklist").size(12.0).color(t::MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(w::mono(format!("{done} / {}", items.len()), 12.0).color(t::ACCENT_TEXT));
            });
        });
        let width = ui.available_width();
        w::progress(ui, done as f32 / items.len() as f32, width, 4.0, t::ACCENT);
        ui.add_space(6.0);
        for item in &items {
            if checklist_row(ui, item, !editing) {
                let text = toggle_item(&task.details, item.line);
                save_notes(app, task.id, &text);
            }
        }
    }
    if !editing {
        add_item_box(app, ui, task);
    } else {
        ui.label(faint_small("Open in the editor — changes there are saved on close."));
    }

    let rest = without_checklist(&task.details);
    if !rest.trim().is_empty() {
        ui.add_space(10.0);
        egui_commonmark::CommonMarkViewer::new().show(ui, &mut app.md_cache, rest.trim());
    } else if items.is_empty() {
        ui.add_space(4.0);
        ui.label(faint_small("No notes yet — use the pencil to write some."));
    }
}

fn faint_small(text: &str) -> RichText {
    RichText::new(text).size(12.5).color(t::FAINT)
}

/// "Sun, Nov 15 · 2 h" for a journal entry; status and tracked time for a task.
fn meta(task: &Task, entry: Option<&LogEntryRow>) -> Vec<String> {
    if let Some(e) = entry {
        return vec![
            e.entry.work_date.format("%a, %b %-d").to_string(),
            format!("{} h", fmt_hours(e.entry.hours)),
        ];
    }
    let mut out = vec![match task.status {
        TaskStatus::Doing => "In progress".to_string(),
        TaskStatus::Todo => "To do".to_string(),
        TaskStatus::Done => task
            .completed_at
            .map(|c| format!("Completed {}", c.with_timezone(&chrono::Local).format("%b %-d")))
            .unwrap_or_else(|| "Completed".into()),
    }];
    let tracked = task.tracked_secs(chrono::Utc::now());
    if tracked > 0 {
        out.push(format!("{} tracked", crate::report::fmt_duration(tracked)));
    }
    out
}

/// One checklist item. Returns true when its box was clicked.
fn checklist_row(ui: &mut egui::Ui, item: &ChecklistItem, enabled: bool) -> bool {
    let width = ui.available_width();
    let galley_text = if item.checked {
        RichText::new(&item.text).size(14.0).color(t::MUTED).strikethrough()
    } else {
        RichText::new(&item.text).size(14.0).color(t::TEXT)
    };
    let frame = Frame::new().corner_radius(8).inner_margin(Margin::symmetric(10, 7));
    let mut prepared = frame.begin(ui);
    let mut clicked = false;
    {
        let ui = &mut prepared.content_ui;
        ui.set_width(width - 20.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            clicked |= w::checkbox(ui, item.checked, "").clicked();
            ui.add(egui::Label::new(galley_text).wrap());
        });
    }
    let rect = prepared.content_ui.min_rect().expand2(vec2(10.0, 7.0));
    let resp = ui.interact(rect, ui.id().with(("check", item.line)), Sense::click());
    if enabled && resp.hovered() {
        prepared.frame = prepared.frame.fill(t::HOVER);
    }
    prepared.end(ui);
    enabled && (clicked || resp.clicked())
}

fn add_item_box(app: &mut WorklogApp, ui: &mut egui::Ui, task: &Task) {
    let id = egui::Id::new(("notes/add_item", task.id));
    let mut buf: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    ui.add_space(4.0);
    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .hint_text(RichText::new("+ Add an item").color(t::FAINT))
            .background_color(egui::Color32::TRANSPARENT)
            .frame(Frame::new().inner_margin(Margin::symmetric(12, 10)))
            .desired_width(f32::INFINITY),
    );
    let color = if resp.has_focus() { t::ACCENT } else { t::SWITCH_OFF };
    w::dashed_rect(ui, resp.rect, color);
    if super::enter_pressed(&resp, ui) && !buf.trim().is_empty() {
        let text = append_item(&task.details, &buf);
        save_notes(app, task.id, &text);
        buf.clear();
        resp.request_focus();
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, buf));
}

fn save_notes(app: &mut WorklogApp, task_id: i64, text: &str) {
    match app.db.set_task_details(task_id, text.trim_end()) {
        Ok(()) => app.touch(),
        Err(e) => app.set_status(format!("Save failed: {e}")),
    }
}

/// Dashed card shown when no task is selected.
pub fn empty_state(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 320.0), Sense::hover());
    ui.painter().rect_filled(rect, t::R_CARD, t::PANEL);
    w::dashed_rect(ui, rect.shrink(0.5), t::BORDER_STRONG);
    let c = rect.center();
    let tile = egui::Rect::from_center_size(c - vec2(0.0, 44.0), Vec2::splat(44.0));
    ui.painter().rect_filled(tile, 12, t::CONTROL);
    ui.painter().text(
        tile.center(),
        egui::Align2::CENTER_CENTER,
        icon::NOTE,
        t::icon_font(22.0),
        t::FAINT,
    );
    ui.painter().text(
        c + vec2(0.0, 2.0),
        egui::Align2::CENTER_CENTER,
        "No task selected",
        t::font(14.0, Weight::Semibold),
        t::TEXT,
    );
    let mut job = egui::text::LayoutJob::simple(
        "Click a task, or a journal entry logged from one, to see its notes and checklist here."
            .into(),
        t::font(13.0, Weight::Regular),
        t::MUTED,
        260.0_f32.min(width - 40.0),
    );
    job.halign = Align::Center;
    let hint = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(c + vec2(0.0, 18.0), hint, t::MUTED);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checklist_items() {
        let text = "intro\n- [ ] one\n  * [x] two\n3. [X] three\n- plain\n+ [ ]four";
        let items = checklist_items(text);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], ChecklistItem { line: 1, checked: false, text: "one".into() });
        assert!(items[1].checked && items[1].line == 2);
        assert!(items[2].checked && items[2].text == "three");
        assert_eq!(items[3].text, "four");
    }

    #[test]
    fn toggles_one_line_only() {
        let text = "- [ ] a\n- [x] b\n";
        assert_eq!(toggle_item(text, 0), "- [x] a\n- [x] b\n");
        assert_eq!(toggle_item(text, 1), "- [ ] a\n- [ ] b\n");
        assert_eq!(toggle_item(text, 5), text);
        // keeps indentation and other bullets
        assert_eq!(toggle_item("  1. [ ] x", 0), "  1. [x] x");
    }

    #[test]
    fn appends_and_strips() {
        assert_eq!(append_item("", "new"), "- [ ] new");
        assert_eq!(append_item("notes\n", " new "), "notes\n- [ ] new");
        assert_eq!(without_checklist("a\n- [ ] b\nc"), "a\nc");
    }
}
