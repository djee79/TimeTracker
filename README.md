# Worklog

A single-binary work journal that also does to-dos. Built around two outputs:

1. **Weekly timesheet summary** — hours per project + what was done, copy-paste ready.
2. **Annual dev export** — CSV of all entries flagged `dev (R&D)`, grouped by project,
   chronological, for the SR&ED claim.

Everything else (tasks, time tracking, UI) exists to feed those two reports.

## Build & run

```sh
cargo run --release        # binary lands in target/release/worklog
```

### Windows

1. Install Rust from <https://rustup.rs>. When `rustup-init.exe` offers to install the
   **Visual Studio C++ Build Tools**, accept — the linker and the bundled SQLite are
   compiled with them. (Already have Visual Studio? The "Desktop development with C++"
   workload is what's needed.)
2. Build:

   ```bat
   git clone https://github.com/djee79/TimeTracker.git
   cd TimeTracker
   cargo build --release
   ```

3. The result is `target\release\worklog.exe` — fully self-contained: SQLite is
   compiled in and the C runtime is statically linked (see `.cargo/config.toml`),
   so you can copy the single exe to any Windows 10/11 machine, no installer or
   redistributable required. Pin it to the taskbar or drop a shortcut in
   `shell:startup` to launch it at login.
4. The release build opens no console window, and the database lands in
   `%APPDATA%\worklog\data\worklog.db` (with daily snapshots in `backups\` beside it) —
   the exact path is always shown in the app's status bar.

Data lives in a single SQLite file (WAL mode, foreign keys on):
`~/.local/share/worklog/worklog.db` — snapshot that file and you've backed up everything.
The path is always visible in the app's status bar. The app also snapshots it for you:
on the first launch of each day, a consistent copy (`VACUUM INTO`) lands in the
`backups/` folder next to it, and the 10 newest are kept.

## Using it

The `?` button (top right) opens the built-in help; the short version:

- **Journal** (Ctrl+1): the capture strip at the top is the fast path — project
  (defaults to last used), date, hours, dev checkbox, description, **Enter saves**.
  Hours accept `1.5`, `1,5` or `1:30`. Below it, recent entries: filter by project,
  range, or full-text search (every word must match the description or project code,
  any order — the `N entries, X h` counter follows, so searching a topic totals it).
  Hovering an entry's project code shows when it was recorded (`created_at` is
  immutable — the contemporaneous-record evidence). The status bar always shows
  `today: X h` as an end-of-day "did I log everything?" check.
- **Tasks** (Ctrl+2): add tasks inline; the list auto-sorts (active task → in-progress →
  priority → newest). Checking a task off pins a prefilled "log the time?" strip at the
  top: hours field is already focused, description is the task title — type hours,
  Enter, done. `Skip` logs nothing; `Cancel` is a true undo (status, active chip and
  timer come back); `Esc` skips.
  - **Time tracking**: every task carries a stopwatch — a memory aid, never a
    timesheet entry by itself. It runs while the task is in progress *and* the app is
    open (quitting banks it, relaunching resumes it). The "log the time?" strip shows
    the tracked total (click it to copy into the hours box — still yours to edit) and
    warns when it contains a 5 h+ unbroken stretch, i.e. the timer was probably left
    running. The counter means *tracked but not yet logged*: it resets whenever you
    log, so it always shows the unaccounted remainder.
  - **⏱ log**: log the time tracked so far *without* closing the task — for multi-day
    tasks, log each day's slice on the right date; the final checkbox then only shows
    what's left.
  - **Priority**: the small `⏶ / – / ⏷` marker on each row cycles high → low → normal
    on click and feeds the auto-sort.
  - **Manual order**: switch "order:" to *manual* and each row grows a `☰` grip —
    drag rows to arrange them yourself (top = next up). New tasks land on top.
    The arrangement persists; switching back to *auto* keeps it for later.
  - **Active task**: `▶ start` marks a task in-progress *and* active — its row is
    highlighted and it's pinned in the top bar on every screen (click the chip to jump
    back). Got interrupted? `▶ start` the interruption, then `● focus` the original
    doing-task to make it active again. Survives restarts.
  - **Group by project**: checkbox above the list swaps status sections for one
    section per project. Persisted.
  - **Completed**: finished tasks collapse at the bottom, searchable by text and
    completion date (defaults to the last 30 days), filtered in SQL so years of
    history stay cheap. `↺` reopens one — its tracked time comes back with it.
- **Reports** (Ctrl+3): week picker (any date snaps to its Monday) → a **timesheet grid**,
  projects down, Mon–Sun across. Everything is click-to-copy at the granularity you need:
  an hours **cell** copies that project-day's descriptions, a **project code** copies its
  whole week, a **day total** copies that day across all projects, and "Copy full week" /
  "Export CSV…" cover the rest (plain-text preview collapsed below). Underneath, the
  annual dev export: pick a year, see the dev-entry count, export CSV
  (`date, project_code, project_name, description, hours`).
- **Projects…** (top right): small managed list — code, name, optional client.
  Archive instead of delete; history stays intact.

Shortcuts: `Ctrl+N` jump to capture, `Ctrl+T` new task, `Ctrl+1/2/3` switch screens.

Tip for a "global" shortcut: the window sets Wayland `app_id = worklog`, so bind a
compositor keybinding that launches/focuses it.

## Layout

```
src/
  main.rs        eframe wiring
  app.rs         WorklogApp state + actions + the eframe::App impl
  db/            rusqlite: schema/migrations (mod.rs), one file per entity
  report/        weekly + annual roll-ups, text/CSV rendering
  ui/            one file per screen (journal, tasks, reports) + projects/help
                 windows + shared widgets in mod.rs
```

Migrations are append-only SQL blocks in `src/db/mod.rs::MIGRATIONS`, tracked via
`PRAGMA user_version`.

`cargo test` covers the DB layer (entries, tasks, timers, filters, backups),
report grouping/formatting, and hours parsing.

## License

MIT — see [LICENSE](LICENSE).
