# Worklog

A single-binary work journal that also does to-dos. Built around two outputs:

1. **Weekly timesheet summary** — hours per project + what was done, copy-paste ready.
2. **Annual dev export** — CSV of all entries flagged `dev (R&D)`, grouped by project,
   chronological, for the SR&ED claim.

Everything else (tasks, time tracking, UI) exists to feed those two reports.

## Install

Grab a prebuilt binary from the [Releases page](https://github.com/djee79/TimeTracker/releases):

- `worklog-windows-x86_64.exe` — fully self-contained (static CRT, SQLite compiled in);
  copy it anywhere on a Windows 10/11 machine and run it. No installer.
- `worklog-linux-x86_64` — built on the latest Ubuntu LTS runner; needs a comparably
  recent glibc. `chmod +x` and run. If your distro is older, build from source instead.

### Linux desktop icon (optional)

The window icon is baked into the binary, but Wayland desktops (Hyprland, KDE,
GNOME…) show the icon matching the app's `.desktop` entry. To install it:

```sh
install -Dm644 assets/timetrackerIcons/worklog-256.png \
  ~/.local/share/icons/hicolor/256x256/apps/worklog.png
install -Dm644 assets/worklog.desktop ~/.local/share/applications/worklog.desktop
# point Exec= at wherever your worklog binary lives, then:
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

Releases are cut by pushing a version tag (`git tag v0.1.1 && git push origin v0.1.1`) —
GitHub Actions tests, builds both binaries, and attaches them
(`.github/workflows/release.yml`).

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
  `Today X h · This week Y h` as an end-of-day "did I log everything?" check, with an
  amber `⚠ N` when past weekdays this week have no hours.
- **Tasks** (Ctrl+2): add tasks inline; the list auto-sorts (active task → in-progress →
  priority → newest). Checking a task off pins a prefilled "log the time?" strip at the
  top: hours field is already focused, description is the task title — type hours,
  Enter, done. `Skip` logs nothing; `✕` is a true undo (status, active pill and
  timer come back); `Esc` skips.
  - **Find a task** (`Ctrl+K`): the search box above the list is a fuzzy ("lazy") filter —
    type the letters you remember, in order, and they needn't be adjacent: `wsp` finds
    *Website proposal*, `lgnbtn` finds *Fix the login button*. Every word must match
    (any order) and the project code counts as one, so `web login` narrows by project.
    The list becomes one flat ranked list, best match first; **Enter starts the top hit
    and makes it active** — `Ctrl+K`, a few letters, Enter is the whole resume-a-task
    flow. Esc clears it; if nothing open matches, one link retries over Completed.
  - **Time tracking**: every task carries a stopwatch — a memory aid, never a
    timesheet entry by itself. It runs while the task is in progress *and* the app is
    open (quitting banks it, relaunching resumes it). The "log the time?" strip shows
    the tracked total (click it to copy into the hours box — still yours to edit) and
    warns when it contains a 5 h+ unbroken stretch, i.e. the timer was probably left
    running. The counter means *tracked but not yet logged*: it resets whenever you
    log, so it always shows the unaccounted remainder.
  - **Log so far** (clock icon on a running task): log the time tracked so far *without* closing the task — for multi-day
    tasks, log each day's slice on the right date; the final checkbox then only shows
    what's left.
  - **Priority**: the small arrow / dash on each row cycles high → low → normal
    on click and feeds the auto-sort.
  - **Manual order**: switch Order to *Manual* and each row grows a grip —
    drag rows to arrange them yourself (top = next up). New tasks land on top.
    The arrangement persists; switching back to *auto* keeps it for later.
  - **Active task**: `▶ Start` marks a task in-progress *and* active — its row is
    highlighted and a timer pill in the top bar shows it on every screen, with the
    live elapsed time, pause, and stop & log (pauses it and opens the "log the time?"
    banner). Got interrupted? Start the interruption, then the crosshair on the
    original task makes it active again. Survives restarts.
  - **Notes**: Markdown per task, in an editor with Edit / Split / Preview (Split shows
    line-numbered, syntax-coloured source beside the rendered notes), a formatting
    toolbar, `Ctrl+S` to save and `Ctrl+Enter` to tick the checkbox on the current
    line. The Notes panel (Journal and Tasks) shows the selected task's checklist as
    clickable checkboxes, with an "add an item" box.
  - **Group by project**: switch above the list swaps status sections for one
    section per project. Persisted.
  - **Completed**: finished tasks collapse at the bottom, searchable by text (same
    fuzzy matching, ranked best-first) and completion date (defaults to the last 30
    days). SQLite does the narrowing — a subsequence `LIKE` prefilter plus the date
    bound — so years of history stay cheap; the scorer only ranks what survives.
    `↺` reopens one — its tracked time comes back with it.
- **Reports** (Ctrl+3): week picker (any date snaps to its Monday) → per-project totals
  and a **timesheet grid**, projects down, Mon–Sun across. Everything is click-to-copy at
  the granularity you need: an hours **cell** copies that project-day's descriptions, a
  **project code** copies its whole week, a **day total** copies that day across all
  projects, and Copy week / CSV / PDF cover the rest. **Rows follow your office
  timesheet**: drag them or use ↑ / ↓; the order is saved and used by Copy, CSV, PDF and
  the monthly summary (Reset to A–Z undoes it). Past weekdays with no hours and no day off
  turn amber. Below: the monthly summary and the annual dev export (pick a year, see the
  R&D-entry count, export CSV `date, project_code, project_name, description, hours`, or PDF).
- **Calendar** (Ctrl+4): the month at a glance — hours per project per day, days off,
  and missing weekdays. Click a day to mark it off (holiday / vacation / sick / other,
  optional label, half days, ranges); double-click opens the journal on that day; a week
  number opens that week in Reports. **Import Québec holidays** previews the statutory
  holidays for a year (Good Friday *or* Easter Monday — your employer's choice) before
  adding them; **Office closure…** marks a range. Days off clear the "no hours" warning
  and are listed in the weekly copy text.
- **Projects** (top right): code, name, optional client and a colour (used for dots,
  chips and report cells), hours this month. Archive instead of delete; history stays
  intact. App settings live here too (backup mirror, PDF name, splash, idle pause delay).

Shortcuts: `Ctrl+N` jump to capture, `Ctrl+T` new task, `Ctrl+K` find a task,
`Ctrl+F` search everything, `Ctrl+1/2/3/4` (or just `1`–`4` when not typing) switch screens.

Tip for a "global" shortcut: the window sets Wayland `app_id = worklog`, so bind a
compositor keybinding that launches/focuses it.

## Layout

```
src/
  main.rs        eframe wiring
  app.rs         WorklogApp state + actions + the eframe::App impl
  db/            rusqlite: schema/migrations (mod.rs), one file per entity
  fuzzy.rs       fzf-style scorer behind the task searches
  holidays.rs    Easter, Québec holidays, weekday ranges (days off)
  report/        weekly + monthly + annual roll-ups, missing weekdays, text/CSV/PDF
  ui/            one file per screen (journal, tasks, reports, calendar) + projects/
                 help/search windows and the notes editor; theme.rs holds the design
                 tokens and fonts, widgets.rs the shared components, shell.rs the
                 top and status bars
```

Migrations are append-only SQL blocks in `src/db/mod.rs::MIGRATIONS`, tracked via
`PRAGMA user_version`.

`cargo test` covers the DB layer (entries, tasks, timers, filters, backups, report
order, days off, upgrading an older database), report grouping/formatting, missing
weekdays, Easter/holiday generation, checklist and markdown helpers, and hours parsing.

UI fonts are Inter and JetBrains Mono (SIL OFL, licences in `assets/fonts/`), icons
from Phosphor. In debug builds, `WORKLOG_SHOT=/path.png WORKLOG_SHOT_VIEW=reports`
renders one screen to a PNG and exits — point `XDG_DATA_HOME` at a scratch copy of
the data first.

## License

MIT — see [LICENSE](LICENSE).
