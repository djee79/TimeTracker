use crate::app::WorklogApp;

/// User manual as a window: one collapsible section per topic.
pub fn show_window(app: &mut WorklogApp, ctx: &egui::Context) {
    if !app.show_help {
        return;
    }
    let mut open = app.show_help;
    egui::Window::new("Help")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(600.0)
        .default_height(520.0)
        .show(ctx, |ui_| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui_, |ui_| {
                    overview(ui_);
                    section(ui_, "Projects", projects);
                    section(ui_, "Journal — logging your day", journal);
                    section(ui_, "Tasks — the to-do list", tasks);
                    section(ui_, "Time tracking", tracking);
                    section(ui_, "The “log the time?” strip", bridge);
                    section(ui_, "Completed tasks", completed);
                    section(ui_, "Reports & exports", reports);
                    section(ui_, "Calendar & days off", calendar);
                    section(ui_, "Keyboard shortcuts", shortcuts);
                    section(ui_, "Your data & backups", data);
                    footer(app, ui_);
                });
        });
    app.show_help = open;
}

/// About-footer: the splash artwork with the live version under it.
fn footer(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.add_space(12.0);
    ui_.separator();
    ui_.add_space(8.0);
    let ctx = ui_.ctx().clone();
    let splash = app.splash.get_or_insert_with(|| crate::ui::load_splash(&ctx));
    ui_.vertical_centered(|ui_| {
        let width = ui_.available_width().min(420.0);
        let size = egui::vec2(width, width * 360.0 / 560.0);
        let (rect, _) = ui_.allocate_exact_size(size, egui::Sense::hover());
        ui_.painter().image(
            splash.texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        ui_.add_space(4.0);
        ui_.label(
            egui::RichText::new(concat!("Worklog v", env!("CARGO_PKG_VERSION")))
                .weak()
                .small(),
        );
    });
}

fn section(ui_: &mut egui::Ui, title: &str, body: fn(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .default_open(false)
        .show(ui_, |ui_| {
            body(ui_);
            ui_.add_space(4.0);
        });
}

fn p(ui_: &mut egui::Ui, text: &str) {
    ui_.label(text);
    ui_.add_space(2.0);
}

fn b(ui_: &mut egui::Ui, text: &str) {
    ui_.horizontal_wrapped(|ui_| {
        ui_.label("•");
        ui_.label(text);
    });
}

fn overview(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Worklog is a work journal: you write down what you did and for how long, \
         one entry at a time, and the reports add it all up. Everything lives in a \
         single local database file — no account, no network.",
    );
    p(ui_, "The four screens work together:");
    b(ui_, "Journal — the record: one entry per slice of work (date, project, hours, description).");
    b(ui_, "Tasks — the plan: a to-do list that turns finished tasks into journal entries.");
    b(ui_, "Reports — the output: weekly summaries and the annual R&D export.");
    b(ui_, "Calendar — the month at a glance: logged days, days off, and the weekdays still missing hours.");
}

fn projects(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Every entry and task belongs to a project. Manage them with the Projects button in \
         the top bar: a code (shown everywhere), a name, an optional client and a colour.",
    );
    b(ui_, "The colour is used for the project's dots, chips, report cells and bars. New projects get the least-used colour; pick another from the six swatches.");
    b(ui_, "Each row shows the hours logged this month. Archive a project you're done with — it disappears from the pickers but its history stays in reports.");
    b(ui_, "The project you used last is preselected for new entries and tasks.");
}

fn journal(ui_: &mut egui::Ui) {
    p(
        ui_,
        "The Log work card at the top is the quickest way to log: pick a project and date, \
         type the hours and what you did, press Enter.",
    );
    b(ui_, "Hours accept 1.5, 1,5 or 1:30 — up to 24 h per entry.");
    b(ui_, "The R&D switch flags the entry for the annual SR&ED export.");
    b(ui_, "Filter the list by project, time range, or the search box (every word must match the description or project code, in any order). The “N entries · X h” counter follows the filters.");
    b(ui_, "Days can be folded by clicking their header. The pencil edits an entry in place (its creation stamp never changes); the bin asks once, then deletes — and the status bar offers Undo for 10 seconds.");
    b(ui_, "Click an entry to select it; with the Notes panel on, an entry logged from a task shows that task's notes and checklist beside the list. Tick checklist items or add new ones right there.");
    b(ui_, "Entries from a task with a checklist show its progress, e.g. ✓ 2/2.");
    b(ui_, "Double-clicking a day in the Calendar opens the journal on just that day; the date chip in the filter row goes back to the range.");
}

fn tasks(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Add a task with a project, a priority (click the arrow / dash to cycle) and a title. \
         A task is either to-do, in progress, or done.",
    );
    b(ui_, "Start — marks it in progress, starts its timer and makes it the active task, shown as a pill in the top bar on every screen. Click the pill's title to jump back to it.");
    b(ui_, "Pause — back to to-do (timer stops). The crosshair makes an in-progress task the active one again after an interruption; the clock logs the time tracked so far without closing it.");
    b(ui_, "The checkbox marks it done and opens the “log the time?” banner.");
    b(ui_, "The pencil (or double-clicking the title) renames it in place — Enter saves, Esc cancels.");
    b(ui_, "The notes icon opens the notes editor (hover it for a quick peek). It shows on rows that have notes, and on any row you hover.");
    b(ui_, "Notes are Markdown. The editor has Edit / Split / Preview: Split shows the source with line numbers next to the rendered notes. The toolbar formats the selection (Ctrl+B, Ctrl+I), lights up for the line you're on, and Ctrl+Enter ticks the checkbox on the current line. Ctrl+S saves; ✕ saves and closes; Discard or Esc throw changes away (Esc asks first).");
    b(ui_, "Checklists (- [ ] / - [x]) show their progress on the row; in the preview and the notes panel the boxes are clickable.");
    b(ui_, "Find a task (Ctrl+K) — type the letters you remember, in order: “wsp” finds “Website proposal”. The list narrows as you type, best match first; Enter starts the top hit. Several words must all match, in any order, and the project code counts. Esc clears the box. Nothing open matches? “Look in Completed instead” retries over finished tasks.");
    b(ui_, "The search icon in the top bar (or Ctrl+F) searches journal, tasks and notes in one place.");
    b(ui_, "Order Auto sorts itself: active, in progress, priority, newest. Order Manual lets you drag the grip. “Group by project” splits the list per project.");
}

fn tracking(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Each task carries a stopwatch, meant as a memory aid — you always decide \
         what to log yourself.",
    );
    b(ui_, "It runs while the task is in progress and the app is open: start/focus starts it, pause banks it, quitting the app banks it, launching resumes it.");
    b(ui_, "The counter means “tracked but not yet logged”: it resets each time you log the task's time, so it always shows what remains unaccounted for.");
    b(ui_, "Hover a task's title to see its tracked time so far.");
    b(ui_, "If the total includes an unbroken stretch of 5 h or more, the strip warns you — the timer was probably left running over lunch or overnight.");
    b(ui_, "The top-bar pill shows the active task's running time, with pause and stop & log (stop pauses the task and opens the “log the time?” banner).");
    b(ui_, "After a while without keyboard/mouse (10 minutes by default; 5, 15 or 30 in Projects → App settings) the timers pause, banking only up to when you stepped away; they resume on activity. Works on Windows, X11, and Wayland compositors with idle-notify (Hyprland, KDE, sway…); where unsupported it turns itself off.");
    b(ui_, "For multi-day tasks, log the time so far (the clock icon) at the end of each day so every slice lands on the right date.");
}

fn bridge(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Completing a task, stopping its timer from the top bar, or logging the time so far \
         opens a prefilled banner above the list: project and description are filled in, \
         the hours box is focused.",
    );
    b(ui_, "The sub-line shows the tracked time; “Use it” copies the value into the hours box — still yours to edit.");
    b(ui_, "Log it (or Enter) — writes the journal entry and resets the task's tracked counter.");
    b(ui_, "Skip (or Esc) — no entry; the tracked count is kept for later.");
    b(ui_, "✕ — oops-undo: the task returns exactly as it was, including status, active pill and running timer.");
}

fn completed(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Finished tasks collect in the “Completed” section at the bottom of the Tasks screen, \
         newest first.",
    );
    b(ui_, "Find by text and by completion date — the date defaults to the last 30 days; switch to “all” to dig further back.");
    b(ui_, "The text box matches like the task finder above: letters in order in the title or project code (“wsp” → “Website proposal”), or the whole word inside the notes. Results come back best match first.");
    b(ui_, "↺ reopens a task as to-do; its previously tracked time comes back with it.");
    b(ui_, "The section keeps its open state, so you can reopen several tasks in a row.");
}

fn reports(ui_: &mut egui::Ui) {
    p(ui_, "Weekly summary — one week at a time: totals per project, an hours grid (projects × days) and a plain-text preview.");
    b(ui_, "Click an hours cell to copy that day's text for that project; the daily total copies the whole day, a project code its whole week. Copy week, CSV and PDF take everything.");
    b(ui_, "Rows follow your office timesheet: drag the grip or use ↑ / ↓ (they skip past projects with no hours that week). The order is saved and used by Copy, CSV and PDF, and by the monthly summary. Reset to A–Z puts it back.");
    b(ui_, "Past weekdays with nothing logged and no day off turn amber, with a “N weekdays with no hours” warning; the status bar shows ⚠ N for the current week.");
    b(ui_, "Days off from the Calendar are listed at the end of the copy text, e.g. “Fri 11-27: Vacation (long weekend)”.");
    p(ui_, "Monthly summary — a whole month per project with share bars, plus copy, CSV and PDF.");
    p(ui_, "Annual dev export (SR&ED) — every R&D entry for a year, as CSV: date, project code, project name, description, hours.");
    b(ui_, "Entries keep an immutable creation stamp, so the export doubles as contemporaneity evidence.");
    p(ui_, "PDFs are US letter, grouped by project, with page numbers and a “Prepared by …” line (set your name in Projects → App settings). “Include task notes in PDF” prints each entry's task notes under it, once per task.");
}

fn calendar(ui_: &mut egui::Ui) {
    p(ui_, "A month at a glance: hours per project on each day, days off, and the weekdays you forgot to log.");
    b(ui_, "Click a day to mark it off — Holiday, Vacation, Sick or Other, an optional label, full or half day. Range… covers every weekday between two dates. Click a marked day to edit or delete it.");
    b(ui_, "Double-click a day to open the journal on that date; click a week number to open that week in Reports.");
    b(ui_, "Import Québec holidays fills in the statutory holidays for a year — choose Good Friday or Easter Monday, untick what doesn't apply. Days already marked are left alone; imported days stay editable.");
    b(ui_, "Office closure… marks a range of weekdays as Other with a label.");
    b(ui_, "Days off (even half days) clear the amber “no hours” warning and carry no hours.");
}

fn shortcuts(ui_: &mut egui::Ui) {
    egui::Grid::new("help/shortcuts").num_columns(2).spacing([24.0, 4.0]).show(ui_, |ui_| {
        for (keys, what) in [
            ("1 / 2 / 3 / 4", "switch to Journal / Tasks / Reports / Calendar (when not typing; Ctrl+1–4 work anywhere)"),
            ("Ctrl+N", "new journal entry (jumps to the capture strip)"),
            ("Ctrl+T", "new task (jumps to the task title)"),
            ("Ctrl+K", "find an open task (letters in order; Enter resumes the top hit)"),
            ("Ctrl+F", "search everything (journal, tasks, notes)"),
            ("Enter", "submits the form you're typing in"),
            ("Esc", "dismisses the “log the time?” banner (first Esc leaves the text field)"),
            ("Ctrl+S", "save in the notes editor"),
            ("Ctrl+B / Ctrl+I", "bold / italic in the notes editor"),
            ("Ctrl+Enter", "tick the checkbox on the current line (notes editor)"),
        ] {
            ui_.label(egui::RichText::new(keys).monospace());
            ui_.label(what);
            ui_.end_row();
        }
    });
}

fn data(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Everything is one SQLite file; its full path is shown at the bottom right. \
         Copy that file and you've backed up everything.",
    );
    b(ui_, "The app also does it for you: on the first launch of each day it snapshots the database into the “backups” folder next to it, keeping the 10 most recent.");
    b(ui_, "To restore, close the app and copy a snapshot over worklog.db.");
    b(ui_, "Set a backup mirror (Projects → App settings) — a second folder, ideally on another disk or a NAS, that receives each daily snapshot too.");
    b(ui_, "Maintenance is automatic: every launch runs a quick integrity check (a red warning appears in the status bar if it ever fails — restore a backup), the file is compacted monthly, and SQLite tunes itself up on exit. One database holds everything, for all years — at this app's scale SQLite is nowhere near its limits, and keeping history together is what makes search and the annual export work.");
}
