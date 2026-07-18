use std::time::Instant;

use chrono::{Datelike, NaiveDate};

use crate::db::{Db, LogEntryRow, Priority, Project, ProjectStatus, Task, TaskSort, TaskStatus};
use crate::report::{self, WeeklyReport};
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Journal,
    Tasks,
    Reports,
}

/// How far back the journal list looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FilterRange {
    Days7,
    Days30,
    Days90,
    All,
}

impl FilterRange {
    pub const ALL: [FilterRange; 4] = [
        FilterRange::Days7,
        FilterRange::Days30,
        FilterRange::Days90,
        FilterRange::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FilterRange::Days7 => "last 7 days",
            FilterRange::Days30 => "last 30 days",
            FilterRange::Days90 => "last 90 days",
            FilterRange::All => "all",
        }
    }

    pub fn since(self, today: NaiveDate) -> Option<NaiveDate> {
        let days = match self {
            FilterRange::Days7 => 7,
            FilterRange::Days30 => 30,
            FilterRange::Days90 => 90,
            FilterRange::All => return None,
        };
        Some(today - chrono::Duration::days(days))
    }
}

/// Shared form state for creating/editing a log entry — used by the capture
/// strip, the inline entry editor, and the task→log bridge.
#[derive(Clone)]
pub struct EntryForm {
    pub project_id: Option<i64>,
    pub description: String,
    pub hours_text: String,
    pub is_dev: bool,
    pub work_date: NaiveDate,
    /// Task this entry is logged from (set by the bridge) — stored so the
    /// journal can pull up the task's notes later.
    pub task_id: Option<i64>,
}

impl EntryForm {
    pub fn new(project_id: Option<i64>, work_date: NaiveDate) -> EntryForm {
        EntryForm {
            project_id,
            description: String::new(),
            hours_text: String::new(),
            is_dev: false,
            work_date,
            task_id: None,
        }
    }

    pub fn from_entry(row: &LogEntryRow) -> EntryForm {
        EntryForm {
            project_id: Some(row.entry.project_id),
            description: row.entry.description.clone(),
            hours_text: report::fmt_hours(row.entry.hours),
            is_dev: row.entry.is_dev,
            work_date: row.entry.work_date,
            task_id: row.entry.task_id,
        }
    }

    /// Accepts "1.5", "1,5" and "1:30".
    pub fn parse_hours(&self) -> Option<f64> {
        let s = self.hours_text.trim().replace(',', ".");
        let h = if let Some((hh, mm)) = s.split_once(':') {
            let hh: f64 = if hh.is_empty() { 0.0 } else { hh.parse().ok()? };
            let mm: f64 = mm.parse().ok()?;
            hh + mm / 60.0
        } else {
            s.parse().ok()?
        };
        (h.is_finite() && h > 0.0 && h <= 24.0).then_some(h)
    }

    pub fn is_valid(&self) -> bool {
        self.project_id.is_some()
            && !self.description.trim().is_empty()
            && self.parse_hours().is_some()
    }
}

/// What put the bridge on screen — decides the wording and the buttons.
pub enum PendingKind {
    /// Task was just completed; "Cancel" restores status, activity and,
    /// if it was running, the timer.
    Completed { prev_status: TaskStatus, was_active: bool },
    /// "Log time so far" on a still-open task; it keeps running.
    Midway,
}

/// A single unbroken timer stretch this long probably means the timer was
/// forgotten (lunch, overnight) — the bridge flags the total as suspect.
pub const LONG_STRETCH_SECS: i64 = 5 * 3600;

/// The task → prefill-log bridge, shown as a pinned strip on the Tasks
/// screen until saved or skipped.
pub struct PendingLog {
    pub task_title: String,
    /// Timer total for the task (0 = never started) — shown as a hint;
    /// the hours to log stay the user's call.
    pub tracked_secs: i64,
    /// Longest single stretch inside that total, to flag runaway timers.
    pub longest_stretch_secs: i64,
    pub task_id: i64,
    pub kind: PendingKind,
    pub form: EntryForm,
}

/// State of the task-notes window: one task's markdown details being
/// viewed or edited. Closing the window saves.
pub struct TaskDetails {
    pub task_id: i64,
    pub task_title: String,
    pub text: String,
    /// Last saved copy — drives the "unsaved" hint.
    pub saved: String,
    /// Rendered view vs the raw-markdown editor.
    pub preview: bool,
}

/// Add/edit form state for the Projects window.
#[derive(Default)]
pub struct ProjectForm {
    pub id: Option<i64>, // None = creating
    pub code: String,
    pub name: String,
    pub client: String,
}

pub struct WorklogApp {
    pub db: Db,
    pub tab: Tab,
    pub projects: Vec<Project>,

    // Journal
    pub capture: EntryForm,
    pub entries: Vec<LogEntryRow>,
    pub filter_project: Option<i64>,
    pub filter_range: FilterRange,
    /// Live text search over the journal (description/project, any order).
    pub filter_text: String,
    pub editing_entry: Option<(i64, EntryForm)>,
    pub confirm_delete_entry: Option<i64>,

    // Tasks
    pub new_task_title: String,
    pub new_task_project: Option<i64>,
    pub new_task_priority: Priority,
    pub open_tasks: Vec<Task>,
    pub done_tasks: Vec<Task>,
    pub pending_log: Option<PendingLog>,
    pub confirm_delete_task: Option<i64>,
    /// Task being renamed in place: (id, edit buffer).
    pub editing_task: Option<(i64, String)>,
    /// The task-notes window, when open.
    pub task_details: Option<TaskDetails>,
    /// Task whose notes the side panel shows (falls back to the active task).
    pub selected_task_id: Option<i64>,
    /// Show the notes side panel on the Tasks screen. Persisted.
    pub show_notes_panel: bool,
    /// Layout cache for rendered markdown (egui_commonmark).
    pub md_cache: egui_commonmark::CommonMarkCache,
    /// Live text filter for the Completed section (title/project, any order).
    pub done_filter: String,
    /// How far back the Completed section looks.
    pub done_range: FilterRange,
    /// The one task currently being worked on. Highlighted in the list and
    /// pinned in the top bar so it survives interruptions. Persisted.
    pub active_task_id: Option<i64>,
    /// Group the open-task list by project instead of status sections. Persisted.
    pub group_tasks: bool,
    /// Auto-sort vs drag-to-reorder. Persisted.
    pub task_sort: TaskSort,

    // Reports
    pub week_start: NaiveDate,
    weekly_cache: Option<(NaiveDate, u64, WeeklyReport)>,
    pub report_year: i32,
    /// PDF exports include the linked task's notes under each entry. Persisted.
    pub pdf_include_notes: bool,

    // Projects window
    pub show_projects: bool,
    pub project_form: ProjectForm,

    // Help window
    pub show_help: bool,

    // Misc
    /// Persistent red flag in the status bar (integrity check failed) —
    /// unlike `status`, it doesn't fade after 5 s.
    pub db_warning: Option<String>,
    data_version: u64,
    /// Hours logged today, cached per (day, data_version) for the status bar.
    today_hours: Option<(NaiveDate, u64, f64)>,
    pub status: Option<(String, Instant)>,
    focus_id: Option<egui::Id>,
}

impl WorklogApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Result<WorklogApp, String> {
        let db = Db::open_default()?;
        let today = today();
        let mut app = WorklogApp {
            db,
            tab: Tab::Journal,
            projects: Vec::new(),
            capture: EntryForm::new(None, today),
            entries: Vec::new(),
            filter_project: None,
            filter_range: FilterRange::Days30,
            filter_text: String::new(),
            editing_entry: None,
            confirm_delete_entry: None,
            new_task_title: String::new(),
            new_task_project: None,
            new_task_priority: Priority::Normal,
            open_tasks: Vec::new(),
            done_tasks: Vec::new(),
            pending_log: None,
            confirm_delete_task: None,
            editing_task: None,
            task_details: None,
            selected_task_id: None,
            show_notes_panel: false,
            md_cache: egui_commonmark::CommonMarkCache::default(),
            done_filter: String::new(),
            done_range: FilterRange::Days30,
            active_task_id: None,
            group_tasks: false,
            task_sort: TaskSort::Auto,
            week_start: report::week_start_of(today),
            weekly_cache: None,
            report_year: today.year(),
            pdf_include_notes: true,
            show_projects: false,
            project_form: ProjectForm::default(),
            show_help: false,
            db_warning: None,
            data_version: 0,
            today_hours: None,
            status: None,
            focus_id: None,
        };
        // Daily safety net: a consistent snapshot next to the live file.
        match app.db.backup_daily(10) {
            Ok(Some(path)) => app.set_status(format!("Backed up to {}", path.display())),
            Ok(None) => {}
            Err(e) => app.set_status(format!("Backup failed: {e}")),
        }
        // Maintenance: corruption is caught while the backups still hold good
        // data; the monthly vacuum keeps the file compact after deletions.
        match app.db.integrity_check() {
            Ok(None) => {
                let _ = app.db.vacuum_monthly();
            }
            Ok(Some(report)) => {
                app.db_warning = Some(format!(
                    "⚠ database check failed: {report} — restore a backup soon"
                ));
            }
            Err(e) => app.db_warning = Some(format!("⚠ database check failed: {e}")),
        }
        app.reload_projects();
        app.reload_entries();
        app.group_tasks = app.db.setting("group_tasks").as_deref() == Some("1");
        app.show_notes_panel = app.db.setting("notes_panel").as_deref() == Some("1");
        app.pdf_include_notes = app.db.setting("pdf_notes").as_deref() != Some("0");
        if app.db.setting("task_sort").as_deref() == Some("manual") {
            app.task_sort = TaskSort::Manual;
        }
        // Restore the active task, but only if it's still open and in progress.
        app.active_task_id = app
            .db
            .setting("active_task_id")
            .and_then(|v| v.parse().ok());
        // Timers only run while the app is open (Drop banks them on exit);
        // resume them for whatever is still in progress.
        let _ = app.db.restart_doing_timers(chrono::Utc::now());
        app.reload_tasks();
        if let Some(id) = app.active_task_id {
            let still_doing = app
                .open_tasks
                .iter()
                .any(|t| t.id == id && t.status == crate::db::TaskStatus::Doing);
            if !still_doing {
                app.set_active_task(None);
            }
        }

        // Default project = last used, else first active.
        let last: Option<i64> = app
            .db
            .setting("last_project_id")
            .and_then(|v| v.parse().ok());
        app.capture.project_id = last
            .filter(|id| app.active_projects().any(|p| p.id == *id))
            .or_else(|| app.active_projects().next().map(|p| p.id));
        app.new_task_project = app.capture.project_id;

        // First run: nothing works without a project, so open the manager.
        if app.projects.is_empty() {
            app.show_projects = true;
        }
        Ok(app)
    }

    // ---- data loading ----

    pub fn active_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects
            .iter()
            .filter(|p| p.status == ProjectStatus::Active)
    }

    pub fn project(&self, id: i64) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn reload_projects(&mut self) {
        self.projects = self.db.list_projects().unwrap_or_default();
    }

    pub fn reload_entries(&mut self) {
        self.entries = self
            .db
            .list_log_entries(
                self.filter_project,
                self.filter_range.since(today()),
                &self.filter_text,
            )
            .unwrap_or_default();
    }

    /// Hours logged today (all projects, ignores the journal filters).
    pub fn today_hours(&mut self) -> f64 {
        let today = today();
        match self.today_hours {
            Some((d, v, h)) if d == today && v == self.data_version => h,
            _ => {
                let h = self.db.hours_on(today).unwrap_or(0.0);
                self.today_hours = Some((today, self.data_version, h));
                h
            }
        }
    }

    pub fn reload_tasks(&mut self) {
        self.open_tasks = self.db.list_open_tasks(self.task_sort).unwrap_or_default();
        // In auto mode the active task jumps to the very top (stable sort
        // keeps the doing → priority → recency order for everything else).
        // Manual order is the user's — never rearrange it.
        if self.task_sort == TaskSort::Auto {
            if let Some(id) = self.active_task_id {
                self.open_tasks.sort_by_key(|t| t.id != id);
            }
        }
        self.done_tasks = self
            .db
            .list_done_tasks(200, self.done_range.since(today()), &self.done_filter)
            .unwrap_or_default();
    }

    pub fn set_task_sort(&mut self, sort: TaskSort) {
        self.task_sort = sort;
        let value = match sort {
            TaskSort::Auto => "auto",
            TaskSort::Manual => "manual",
        };
        let _ = self.db.set_setting("task_sort", value);
        self.reload_tasks();
    }

    /// Move `src_id` next to `target_id` (above it if `before`) in the manual
    /// order and persist the whole arrangement.
    pub fn reorder_task(&mut self, src_id: i64, target_id: i64, before: bool) {
        if src_id == target_id {
            return;
        }
        let mut ids: Vec<i64> = self.open_tasks.iter().map(|t| t.id).collect();
        let Some(from) = ids.iter().position(|&id| id == src_id) else {
            return;
        };
        ids.remove(from);
        let Some(target) = ids.iter().position(|&id| id == target_id) else {
            return;
        };
        ids.insert(if before { target } else { target + 1 }, src_id);
        if let Err(e) = self.db.save_task_order(&ids) {
            self.set_status(format!("Reorder failed: {e}"));
        }
        self.reload_tasks();
    }

    /// Open the inline title editor on a task row.
    pub fn start_task_edit(&mut self, task: &Task) {
        self.editing_task = Some((task.id, task.title.clone()));
        self.confirm_delete_task = None;
        self.request_focus(ui::FOCUS_TASK_EDIT);
    }

    /// Open the notes window on a task. Starts in the rendered view when
    /// there's something to read, in the editor when the notes are empty.
    pub fn open_task_details(&mut self, task: &Task) {
        self.task_details = Some(TaskDetails {
            task_id: task.id,
            task_title: task.title.clone(),
            text: task.details.clone(),
            saved: task.details.clone(),
            preview: !task.details.trim().is_empty(),
        });
    }

    pub fn set_active_task(&mut self, id: Option<i64>) {
        self.active_task_id = id;
        let value = id.map(|i| i.to_string()).unwrap_or_default();
        let _ = self.db.set_setting("active_task_id", &value);
        self.reload_tasks();
    }

    pub fn active_task(&self) -> Option<&Task> {
        let id = self.active_task_id?;
        self.open_tasks.iter().find(|t| t.id == id)
    }

    /// Call after any mutation: bumps the version (invalidating report caches)
    /// and refreshes the visible lists.
    pub fn touch(&mut self) {
        self.data_version += 1;
        self.reload_entries();
        self.reload_tasks();
    }

    // ---- status + focus plumbing ----

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some((text.into(), Instant::now()));
    }

    pub fn request_focus(&mut self, marker: &'static str) {
        self.focus_id = Some(egui::Id::new(marker));
    }

    /// In render code, attach a pending focus request to a widget response.
    pub fn apply_focus(&mut self, marker: &'static str, resp: &egui::Response) {
        if self.focus_id == Some(egui::Id::new(marker)) {
            resp.request_focus();
            self.focus_id = None;
        }
    }

    // ---- actions ----

    /// Insert a log entry from a validated form. Returns false if invalid.
    pub fn save_entry_form(&mut self, form: &EntryForm) -> bool {
        let (Some(project_id), Some(hours)) = (form.project_id, form.parse_hours()) else {
            return false;
        };
        let description = form.description.trim();
        if description.is_empty() {
            return false;
        }
        match self.db.insert_log_entry(
            form.work_date,
            project_id,
            description,
            hours,
            form.is_dev,
            form.task_id,
        ) {
            Ok(_) => {
                let _ = self
                    .db
                    .set_setting("last_project_id", &project_id.to_string());
                let code = self
                    .project(project_id)
                    .map(|p| p.code.clone())
                    .unwrap_or_default();
                self.set_status(format!(
                    "Logged {} h on {} — {}",
                    report::fmt_hours(hours),
                    code,
                    form.work_date
                ));
                self.touch();
                true
            }
            Err(e) => {
                self.set_status(format!("Save failed: {e}"));
                false
            }
        }
    }

    /// Mark a task done and open the prefilled log bridge.
    pub fn complete_task(&mut self, task: &Task) {
        if let Err(e) = self.db.set_task_status(task.id, crate::db::TaskStatus::Done) {
            self.set_status(format!("Update failed: {e}"));
            return;
        }
        let _ = self.db.stop_task_timer(task.id, chrono::Utc::now());
        let was_active = self.active_task_id == Some(task.id);
        if was_active {
            self.set_active_task(None);
        }
        self.open_bridge(
            task,
            PendingKind::Completed {
                prev_status: task.status,
                was_active,
            },
        );
    }

    /// "Log time so far" on an in-progress task: bank the running stretch,
    /// offer it in the bridge, and keep the timer going for the next slice.
    pub fn log_midway(&mut self, task: &Task) {
        let now = chrono::Utc::now();
        let _ = self.db.stop_task_timer(task.id, now);
        let _ = self.db.start_task_timer(task.id, now);
        self.open_bridge(task, PendingKind::Midway);
    }

    fn open_bridge(&mut self, task: &Task, kind: PendingKind) {
        let (tracked_secs, longest_stretch_secs) =
            self.db.task_timer_totals(task.id).unwrap_or((0, 0));
        let mut form = EntryForm::new(Some(task.project_id), today());
        form.description = task.title.clone();
        form.task_id = Some(task.id);
        self.pending_log = Some(PendingLog {
            task_title: task.title.clone(),
            tracked_secs,
            longest_stretch_secs,
            task_id: task.id,
            kind,
            form,
        });
        self.request_focus(ui::FOCUS_BRIDGE_HOURS);
        self.touch();
    }

    /// Called when the bridge's entry was saved: the tracked counter has been
    /// logged, so it starts over from zero (any still-running stretch counts
    /// from where the bridge opened).
    pub fn pending_logged(&mut self, pending: &PendingLog) {
        let _ = self.db.reset_task_spent(pending.task_id);
        self.touch();
    }

    /// Undo a completion from the bridge ("Cancel"): the task goes back to
    /// the exact state it was in — status, active flag, running timer.
    pub fn undo_complete(&mut self, pending: &PendingLog) {
        let PendingKind::Completed { prev_status, was_active } = pending.kind else {
            return;
        };
        if let Err(e) = self.db.set_task_status(pending.task_id, prev_status) {
            self.set_status(format!("Reopen failed: {e}"));
            return;
        }
        if prev_status == TaskStatus::Doing {
            let _ = self.db.start_task_timer(pending.task_id, chrono::Utc::now());
        }
        self.touch();
        if was_active {
            self.set_active_task(Some(pending.task_id));
        }
        self.set_status(format!("Put back “{}”", pending.task_title));
    }

    pub fn weekly_report(&mut self) -> &WeeklyReport {
        let stale = !matches!(
            &self.weekly_cache,
            Some((ws, v, _)) if *ws == self.week_start && *v == self.data_version
        );
        if stale {
            let rep = report::weekly(&self.db, self.week_start).unwrap_or(WeeklyReport {
                week_start: self.week_start,
                groups: Vec::new(),
                total_hours: 0.0,
            });
            self.weekly_cache = Some((self.week_start, self.data_version, rep));
        }
        &self.weekly_cache.as_ref().unwrap().2
    }

    /// Save-dialog + render for a built PDF document (or report the build
    /// error), with a status message either way.
    pub fn export_pdf(
        &mut self,
        suggested_name: &str,
        doc: Result<genpdf::Document, String>,
    ) {
        let doc = match doc {
            Ok(doc) => doc,
            Err(e) => {
                self.set_status(format!("Export failed: {e}"));
                return;
            }
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(suggested_name)
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        match doc.render_to_file(&path) {
            Ok(()) => self.set_status(format!("Exported {}", path.display())),
            Err(e) => self.set_status(format!("Export failed: {e}")),
        }
    }

    /// Save-dialog + write, with a status message either way.
    pub fn export_csv(&mut self, suggested_name: &str, contents: &str) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(suggested_name)
            .add_filter("CSV", &["csv"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, contents) {
            Ok(()) => self.set_status(format!("Exported {}", path.display())),
            Err(e) => self.set_status(format!("Export failed: {e}")),
        }
    }
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

impl Drop for WorklogApp {
    /// Bank running task timers so time with the app closed isn't tracked,
    /// and let SQLite run its close-time tune-up.
    fn drop(&mut self) {
        let _ = self.db.stop_all_task_timers(chrono::Utc::now());
        self.db.optimize();
    }
}

impl eframe::App for WorklogApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        // Keyboard shortcuts: Ctrl+1/2/3 switch tabs, Ctrl+N quick-capture,
        // Ctrl+T new task.
        ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            if i.consume_key(Modifiers::CTRL, Key::Num1) {
                self.tab = Tab::Journal;
            }
            if i.consume_key(Modifiers::CTRL, Key::Num2) {
                self.tab = Tab::Tasks;
            }
            if i.consume_key(Modifiers::CTRL, Key::Num3) {
                self.tab = Tab::Reports;
            }
            if i.consume_key(Modifiers::CTRL, Key::N) {
                self.tab = Tab::Journal;
                self.focus_id = Some(egui::Id::new(ui::FOCUS_CAPTURE_DESC));
            }
            if i.consume_key(Modifiers::CTRL, Key::T) {
                self.tab = Tab::Tasks;
                self.focus_id = Some(egui::Id::new(ui::FOCUS_TASK_TITLE));
            }
        });

        egui::Panel::top(egui::Id::new("tabs")).show(root, |ui_| {
            ui_.add_space(4.0);
            ui_.horizontal(|ui_| {
                ui_.selectable_value(&mut self.tab, Tab::Journal, "  Journal  ");
                ui_.selectable_value(&mut self.tab, Tab::Tasks, "  Tasks  ");
                ui_.selectable_value(&mut self.tab, Tab::Reports, "  Reports  ");
                ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                    if ui_.button("?").on_hover_text("help").clicked() {
                        self.show_help = true;
                    }
                    if ui_.button("Projects…").clicked() {
                        self.show_projects = true;
                    }
                    // The active task follows you across screens; click to
                    // jump back to it after an interruption.
                    if let Some(task) = self.active_task() {
                        let mut title = task.title.clone();
                        if title.chars().count() > 40 {
                            title = title.chars().take(39).collect::<String>() + "…";
                        }
                        let chip = egui::RichText::new(format!(
                            "⏵ {}  {}",
                            task.project_code, title
                        ))
                        .color(ui_.visuals().selection.stroke.color);
                        if ui_
                            .button(chip)
                            .on_hover_text(format!("Active task: {}", task.title))
                            .clicked()
                        {
                            self.tab = Tab::Tasks;
                        }
                    }
                });
            });
            ui_.add_space(4.0);
        });

        // Status bar: message for 5 s, plus the DB path for backup peace of mind.
        egui::Panel::bottom(egui::Id::new("status")).show(root, |ui_| {
            ui_.horizontal(|ui_| {
                if let Some(warning) = &self.db_warning {
                    ui_.label(
                        egui::RichText::new(warning)
                            .color(ui_.visuals().error_fg_color)
                            .strong(),
                    );
                }
                if let Some((text, at)) = &self.status {
                    if at.elapsed().as_secs_f32() < 5.0 {
                        ui_.label(egui::RichText::new(text).strong());
                        ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    } else {
                        self.status = None;
                    }
                }
                ui_.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_| {
                    ui_.label(
                        egui::RichText::new(self.db.path().display().to_string())
                            .weak()
                            .small(),
                    );
                    ui_.separator();
                    // end-of-day sanity check: did I log what I worked on?
                    let today_h = self.today_hours();
                    let text = if today_h > 0.0 {
                        format!("today: {} h", report::fmt_hours(today_h))
                    } else {
                        "today: nothing logged".to_string()
                    };
                    ui_.label(egui::RichText::new(text).weak());
                });
            });
        });

        egui::CentralPanel::default().show(root, |ui_| match self.tab {
            Tab::Journal => ui::journal::show(self, ui_),
            Tab::Tasks => ui::tasks::show(self, ui_),
            Tab::Reports => ui::reports::show(self, ui_),
        });

        ui::projects::show_window(self, &ctx);
        ui::tasks::details_window(self, &ctx);
        ui::help::show_window(self, &ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(hours: &str) -> EntryForm {
        let mut f = EntryForm::new(Some(1), today());
        f.hours_text = hours.into();
        f
    }

    #[test]
    fn hours_parsing_formats() {
        assert_eq!(form("1.5").parse_hours(), Some(1.5));
        assert_eq!(form("1,5").parse_hours(), Some(1.5));
        assert_eq!(form("1:30").parse_hours(), Some(1.5));
        assert_eq!(form(":45").parse_hours(), Some(0.75));
        assert_eq!(form("8").parse_hours(), Some(8.0));
        assert_eq!(form("").parse_hours(), None);
        assert_eq!(form("0").parse_hours(), None);
        assert_eq!(form("25").parse_hours(), None);
        assert_eq!(form("abc").parse_hours(), None);
    }

    #[test]
    fn form_validation() {
        let mut f = form("2");
        assert!(!f.is_valid()); // no description
        f.description = "did things".into();
        assert!(f.is_valid());
        f.project_id = None;
        assert!(!f.is_valid());
    }
}
