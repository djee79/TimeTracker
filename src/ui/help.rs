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
                    section(ui_, "Keyboard shortcuts", shortcuts);
                    section(ui_, "Your data & backups", data);
                });
        });
    app.show_help = open;
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
    p(ui_, "The three screens work together:");
    b(ui_, "Journal — the record: one entry per slice of work (date, project, hours, description).");
    b(ui_, "Tasks — the plan: a to-do list that turns finished tasks into journal entries.");
    b(ui_, "Reports — the output: weekly summaries and the annual R&D export.");
}

fn projects(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Every entry and task belongs to a project. Manage them via the “Projects…” \
         button in the top bar: a code (shown everywhere), a name, and an optional client.",
    );
    b(ui_, "Archive a project you're done with — it disappears from the pickers but its history stays in reports.");
    b(ui_, "The project you used last is preselected for new entries and tasks.");
}

fn journal(ui_: &mut egui::Ui) {
    p(
        ui_,
        "The capture strip at the top is the quickest way to log: pick a project and date, \
         type the hours and what you did, press Enter.",
    );
    b(ui_, "Hours accept 1.5, 1,5 or 1:30 — up to 24 h per entry.");
    b(ui_, "The “dev” checkbox marks the entry as R&D; only those go in the annual export.");
    b(ui_, "Filter the list by project, time range, or the search box (every word must match the description or project code, in any order).");
    b(ui_, "The “N entries, X h” counter follows the filters — search a topic to see its total hours.");
    b(ui_, "✏ edits an entry in place (its creation stamp never changes); 🗑 asks once, then deletes.");
    b(ui_, "The status bar always shows “today: X h” — an end-of-day check that nothing was forgotten.");
}

fn tasks(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Add a task with a project, a priority (click the –/⏶/⏷ symbol to cycle), and a title. \
         A task is either to-do, in progress, or done.",
    );
    b(ui_, "⏵ start — marks it in progress and makes it the active task, shown as a chip in the top bar on every screen. Click the chip to jump back to it.");
    b(ui_, "⏸ pause — back to to-do; ⏺ focus — make an in-progress task the active one again after an interruption.");
    b(ui_, "⏱ log — log the time tracked so far without closing the task (see Time tracking).");
    b(ui_, "The checkbox marks it done and opens the “log the time?” strip.");
    b(ui_, "Order “auto” sorts itself: active, in progress, priority, newest. Order “manual” lets you drag the ☰ grip. “Group by project” splits the list per project.");
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
    b(ui_, "For multi-day tasks, use ⏱ log at the end of each day so every slice lands on the right date.");
}

fn bridge(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Completing a task (or clicking ⏱ log) opens a prefilled strip above the list: \
         project and description are filled in, the hours box is focused and empty.",
    );
    b(ui_, "The ⏱ hint shows the tracked time; click it to copy the value into the hours box — still yours to edit.");
    b(ui_, "Log it (or Enter) — writes the journal entry and resets the task's tracked counter.");
    b(ui_, "Skip (or Esc) — no entry; the tracked count is kept for later.");
    b(ui_, "Cancel (completions only) — oops-undo: the task returns exactly as it was, including status, active chip and running timer.");
}

fn completed(ui_: &mut egui::Ui) {
    p(
        ui_,
        "Finished tasks collect in the “Completed” section at the bottom of the Tasks screen, \
         newest first.",
    );
    b(ui_, "Filter by text (title or project code) and by completion date — it defaults to the last 30 days; switch to “all” to dig further back.");
    b(ui_, "↺ reopens a task as to-do; its previously tracked time comes back with it.");
    b(ui_, "The section keeps its open state, so you can reopen several tasks in a row.");
}

fn reports(ui_: &mut egui::Ui) {
    p(ui_, "Weekly summary — one week at a time, grouped by project with totals.");
    b(ui_, "Click any cell to copy it; “Copy full week” and “Export CSV…” take the whole week.");
    p(ui_, "Annual dev export (SR&ED) — every entry marked “dev” for a year, as CSV: date, project code, project name, description, hours.");
    b(ui_, "Entries keep an immutable creation stamp, so the export doubles as contemporaneity evidence.");
}

fn shortcuts(ui_: &mut egui::Ui) {
    egui::Grid::new("help/shortcuts").num_columns(2).spacing([24.0, 4.0]).show(ui_, |ui_| {
        for (keys, what) in [
            ("Ctrl+1 / 2 / 3", "switch to Journal / Tasks / Reports"),
            ("Ctrl+N", "new journal entry (jumps to the capture strip)"),
            ("Ctrl+T", "new task (jumps to the task title)"),
            ("Enter", "submits the form you're typing in"),
            ("Esc", "dismisses the “log the time?” strip (first Esc leaves the text field)"),
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
}
