# worklog — UI redesign handoff

Instructions for Claude Code. Implement the refreshed interface for the `worklog` Rust desktop time tracker, plus four small features that came out of the redesign.

The HTML mockups in `docs/design/` (`Journal.dc.html`, `Tasks.dc.html`, `Reports.dc.html`, `Calendar.dc.html`, `Projects.dc.html`, `NotesEditor.dc.html`, `DatePicker.dc.html`) are the **visual reference**. Open them to read exact colours, sizes and spacing. They are mockups, not code to port: rebuild the look with the app's existing GUI toolkit.

---

## 0. Before you start

1. Explore the repo first. Identify the GUI framework (the current UI looks like **egui/eframe**; if it's something else, map the guidance below to that toolkit), the module layout, how the DB is accessed (`~/.local/share/worklog/worklog.db`, probably SQLite) and how migrations are done.
2. Summarise what you found and propose a plan **before** writing code. Wait for my OK.
3. Work in the phases in §6, one commit per phase, and run `cargo build`, `cargo clippy` and the tests after each one.
4. Do not change existing behaviour unless this doc says so. Every existing feature (journal, tasks with timers, notes, weekly/monthly/SR&ED reports, CSV/PDF export, backup mirror, idle pause, splash) must keep working.
5. The DB must stay backward compatible. Use additive migrations only, with no destructive changes to existing tables or data.

---

## 1. Design tokens

Put these in one `theme` module (e.g. `src/ui/theme.rs`) as consts and apply them once at startup (egui: `ctx.set_visuals` / `ctx.set_style`). Don't scatter hex literals through the UI code.

### Colours

| Token | Hex | Use |
|---|---|---|
| `BG` | `#111113` | window background |
| `BAR` | `#16161a` | top bar, status bar |
| `PANEL` | `#18181c` | cards, lists, dialogs |
| `PANEL_ALT` | `#151518` | toolbars, settings area, weekend cells |
| `INSET` | `#0f0f12` | text inputs, code/markdown editor |
| `TABLE_HEAD` | `#141417` | table header / totals rows |
| `CONTROL` | `#1d1d22` | secondary buttons, segmented-control track |
| `HOVER` | `#1f1f25` | row hover |
| `CONTROL_HOVER` | `#2a2a32` | button hover |
| `ACTIVE_TAB` | `#2c2c35` | selected segment/tab |
| `BORDER` | `#26262d` | card borders |
| `BORDER_STRONG` | `#2c2c34` | input borders |
| `DIVIDER` | `#24242a` | separators, row lines (`#222228` inside lists) |
| `TEXT` | `#ececf1` | primary text |
| `TEXT_2` | `#c9c9d2` | secondary text |
| `MUTED` | `#9a9aa6` | labels, meta |
| `FAINT` | `#85858f` | placeholders, line numbers, dim dates (lowest contrast allowed for text) |
| `DISABLED` | `#4a4a52` | empty cells "·", disabled arrows |
| `ACCENT` | `#8b9cff` | primary buttons, running timer, focus, selection, today |
| `ACCENT_INK` | `#0f1330` | text/icons on `ACCENT` |
| `ACCENT_TEXT` | `#c8d0ff` | accent-coloured text on dark |
| `ACCENT_TINT` | `ACCENT` @ 10–16 % alpha | running pill, "Today" badge, banner bg (`#1a1c2a`) |
| `SELECTED_ROW` | `#1e1e27` + 3 px `ACCENT` bar on the left edge | selected journal entry / running task |
| `TODAY_COL` | `#1c1d29` | today's column in the weekly grid |
| `AMBER` | `#f2b04c` | R&D tag, "No hours" warnings, unsaved state |
| `AMBER_BG` | `#1b1915` | weekday-with-no-hours cell |

**Project palette.** Each project picks one. The dot or fill uses the base colour; text in project colour uses the light variant.

| Name | Base | Light text |
|---|---|---|
| Blue | `#7aa2f7` | `#c9d7fb` |
| Lime | `#b8d46a` | `#dbe9b0` |
| Orange | `#f08a5d` | lighten ~35 % |
| Pink | `#e48ac6` | lighten ~35 % |
| Teal | `#5ccfc0` | `#8fe0d5` |
| Violet | `#b294f5` | `#d9c9fb` |

**Day-off types:** Holiday = teal, Vacation = violet, Sick = pink, Other = `#8e8e9a`.

### Typography

- **Inter** for the UI and **JetBrains Mono** for project codes, hours, timers, dates in the text preview, keyboard hints and file paths. Both are SIL OFL: embed the `.ttf`s via `include_bytes!` and register them in `FontDefinitions` (Proportional = Inter, Monospace = JetBrains Mono). Check the licence files into `assets/fonts/`.
- Sizes: body 14, controls 13, labels/meta 12, uppercase eyebrow labels 11 with letter-spacing, card titles 15–17, dialog titles 18, month heading 24, KPI numbers 28 (mono).
- Weights: 400 body, 500 controls, 600 titles, codes and hours.

### Shape and spacing

- Corner radius: dialogs 16, cards 14, lists/tables 12, inputs/buttons 9, small icon buttons 7–8, pills/badges fully round.
- Heights: standard input/button 38, compact 34, small 32, icon buttons 32–36, list rows 48–52, top bar ≈ 56.
- Page padding 24; card padding 18–22; gaps 8 / 12 / 16 / 20.
- Dialog shadow: soft, large blur, ~55 % black.

### Icons

Use an icon font crate (e.g. `egui-phosphor`) instead of text glyphs: play, pause, stop, pencil, trash, folder, search, help-circle, calendar, chevrons, grip (6 dots), notes/file, copy, download, archive, check, x, warning triangle, plus, and the markdown toolbar set. Icon-only buttons need a tooltip (`on_hover_text`).

---

## 2. Shared components

Build these once (e.g. `src/ui/widgets.rs`) and reuse them everywhere.

- **Button variants:**
  - `primary`: `ACCENT` fill, `ACCENT_INK` text, weight 600.
  - `secondary`: `CONTROL` fill, `BORDER_STRONG` stroke.
  - `ghost`: transparent; hover shows `CONTROL_HOVER`.
  - `icon`: square ghost button with a tooltip.
- **Segmented control.** A rounded track (`CONTROL` or `INSET`, 1 px border, 3 px padding) with `ACTIVE_TAB` behind the selected segment. Used for main tabs, Edit/Split/Preview and Full/Half day.
- **Toggle switch.** 34×20 pill with a 14 px knob; `ACCENT` when on and `#33333b` when off. Replaces the plain checkboxes for settings and view options ("Notes panel", "Group by project", "R&D", "Splash screen", "Pause timers when idle"). Keep real checkboxes for task completion and checklist items. egui's demo has a `toggle_switch` you can adapt.
- **Project dot / code.** An 8 px filled circle in the project colour, then the code in mono 600 using the project's light text colour.
- **Badges:**
  - R&D: amber text on amber 12 % fill with an amber 32 % border, radius 6.
  - Client tag: uppercase 11 px with a `#33333b` border.
  - Count pill: `#26262d` fill.
  - "Today": `ACCENT_TINT` fill with `ACCENT_TEXT` text.
- **Card.** `PANEL` fill, 1 px `BORDER`, radius 14.
- **List group.** A card whose rows are separated by `DIVIDER`. Hovered rows use `HOVER`; the selected row uses `SELECTED_ROW`.
- **Labelled field.** A 12 px `MUTED` label above an `INSET` input with a `BORDER_STRONG` stroke. When focused it gets an `ACCENT` stroke plus a 3 px soft accent glow.

---

## 3. App shell (all tabs)

**Top bar** (`BAR` background, bottom divider), from left to right:

1. Logo: a 28 px `ACCENT` rounded square with a clock icon, then **worklog** in 15 px, weight 700.
2. Main tabs as a segmented control: `Journal 1`, `Tasks 2`, `Reports 3`, `Calendar 4`. The small mono number is the keyboard shortcut: keys 1–4 switch tabs when no text field has focus.
3. Spacer.
4. **Running-timer pill**, shown only while a timer runs:
   - `ACCENT_TINT` background with an accent border, fully round.
   - Contents: a pulsing accent dot, the project code (mono, project colour), the task name, the elapsed time `H:MM:SS` (mono, tabular figures, updates every second via `request_repaint_after`), a round **pause** button (accent fill) and a round **stop & log** button (outline).
   - Stop opens the existing "log the time?" flow.
5. `Projects` secondary button with a folder icon, then the search icon button and the help icon button.

**Status bar** (`BAR`, 12 px `MUTED` text): `Today 4 h` · `This week 6 h` on the left, the DB path in mono on the right.

---

## 4. Screens

### 4.1 Journal

- **Log work card.** Title "Log work" with a right-aligned hint showing the `Enter` key. Then one wrapping row of labelled fields:
  - Project (dropdown showing the dot and code)
  - Date (with a calendar icon; shows "Today, Nov 15" when it's today)
  - Hours (80 px, mono, right-aligned)
  - "What did you do?" (fills the remaining width)
  - an R&D switch inside a bordered box
  - the **Add entry** primary button with a plus icon
- **Filter row.** Project filter and range filter as secondary dropdowns, a search field with an icon, the "Notes panel" switch, and right-aligned `N entries · X h`.
- **Two columns.** The entry list takes the remaining width; the notes panel is ~340 px and wraps below the list on narrow windows.
- **Day groups:**
  - Header: weekday (600) and `· Mon DD` in muted, a "Today" badge if applicable, a hairline that fills the space, and the day total (mono 600) on the right.
  - Rows (50 px) in a list-group card: project dot, project code (fixed 86 px column), description (ellipsised), a checklist count `✓ 2/2` if the entry has a checklist, the R&D badge, hours (mono, right-aligned, 48 px column), then edit and delete icon buttons.
  - The selected entry gets `SELECTED_ROW`.
- **Notes panel (card):**
  - An uppercase "NOTES" eyebrow, the task title (18 px), and a meta line: dot, code, date and hours.
  - An edit icon button on the right.
  - A checklist progress label (`2 / 2`) and a 4 px progress bar.
  - Checklist items as checkbox rows; checked items are struck through in `MUTED`.
  - A dashed "+ Add an item" input.

### 4.2 Tasks

- **"Log the time?" banner**, shown after completing or stopping a task:
  - `#1a1c2a` background with a 1 px accent border at 38 % alpha.
  - A round check icon, the title `Done: "task 01" — log the time?` and a muted sub-line about the tracked time.
  - A dismiss ✕ in the top-right corner.
  - Fields row, indented under the title: project, date, hours (auto-focused, with accent focus ring), description (pre-filled), R&D switch, **Log it** (primary) and **Skip** (secondary). Cancel is the ✕.
- **Composer.** Project dropdown, an "Add a task…" input and an **Add** primary button.
- **Toolbar:**
  - A fuzzy search field with a `Ctrl K` key hint inside it, then muted text "Enter resumes the best match".
  - Right side: an "Order: Manual" dropdown and the "Group by project" and "Notes panel" switches.
- **Project groups:**
  - Header: chevron, dot, code (mono, project colour), name in muted, the client tag, and a count pill.
  - Rows (52 px): drag grip, completion checkbox, title, a checklist mini-bar with `2/2`, the notes icon (only if the task has notes), edit, delete, and a fixed-width (84 px) **▶ Start** button.
  - The **running task** row uses `SELECTED_ROW`, shows "● Running" plus the live elapsed time, and its Start button becomes a primary **❚❚ Pause**.
- **Completed (n)** sits at the bottom as a collapsible ghost row.
- **Notes panel empty state.** A dashed-border card with an icon tile, "No task selected" and a one-line hint.

### 4.3 Reports

**Weekly summary card:**

- **Header row:**
  - The title.
  - A week picker: ‹ · `📅 Nov 9 – 15, 2026` · › in an `INSET` track.
  - A "This week" badge, plus an amber pill `⚠ N weekdays with no hours` when N > 0 (see §5.2).
  - Right side: an "Include task notes in PDF" checkbox, then Copy week (secondary), CSV (secondary) and PDF (primary).
- **KPI cards.** A row of three or more small cards:
  - Total this week: big mono number and a stacked bar split by project colour.
  - One card per project with hours this week: dot, code, hours, and `NN% of week · N entries`.
- **Reorder hint row.** An up/down icon, the text "Drag or use the arrows to match your office timesheet. Row order is saved and used by Copy, CSV and PDF.", and a right-aligned **Reset to A–Z** button. See §5.1.
- **Hours grid** (table in a radius-12 bordered box; scrolls horizontally if narrow):
  - Columns: Project (≥ 240 px) | Mon … Sun | Total.
  - **Project cell:** drag grip, dot, code (+ name in muted), then ↑ / ↓ icon buttons on the right. ↑ is disabled on the first row and ↓ on the last.
  - **Hours cell:** a rounded fill in the project colour (~25–30 % alpha) with the number in mono 600. Shade intensity may scale with hours. Clicking it copies that day's text, as now; the hover shows an accent outline.
  - **Empty cell:** a `DISABLED` "·".
  - **Today column:** `TODAY_COL` background and an accent header label.
  - **Weekday with no hours** (§5.2): `AMBER` header text with "!", the column tinted `AMBER_BG`, and the daily-total cell showing an amber `0`.
  - **Daily total row:** `TABLE_HEAD` background; the bottom-right grand total is in `ACCENT_TEXT`.
- **Plain-text preview.** A collapsible header row with the hint "Click any hours cell to copy that day" and a Copy button. The body is mono 13 on `#0d0d10`, line-height 1.7. Its project order follows §5.1.

**Monthly summary card.** Month picker, Copy, CSV and PDF, a big total with "logged in <Month> across N projects", then one row per project: dot, code, name, a 10 px rounded bar (share of the month, in the project colour) and the hours. The plain-text preview is collapsible as now.

**Annual dev export card (SR&ED).**

- Title with an amber `SR&ED` badge and the explanatory paragraph.
- An inset box holding the year picker (› disabled for the current year) and `N R&D entries`.
- The "Include task notes in PDF" checkbox.
- Export CSV (secondary) and Export PDF (primary) as full-width buttons.

The monthly and SR&ED cards sit side by side, about 3 : 2, and wrap on narrow windows.

### 4.4 Calendar (new tab, see §5.3)

- **Header:** "November **2026**" at 24 px (year muted), then a ‹ Today › control. Right side: the legend (Holiday, Vacation, Sick, and No hours as a dashed amber square) and a **+ Mark day off** primary button.
- **Month grid**, Monday-first, 6 rows, with a 1 px `DIVIDER` gap between cells and the whole grid rounded to 12:
  - A narrow left column shows the ISO week number (`W46`, mono, faint). Clicking it jumps to that week in Reports.
  - **Out-of-month days:** `#131316` background, faint number.
  - **Weekend:** `PANEL_ALT`.
  - **Logged day:** the number top-left and the day total (mono) top-right, then one chip per project: tinted project colour, dot, code, hours.
  - **Weekday with no hours** (past only): `AMBER_BG`, a 1 px amber inner outline at 28 % alpha, and "⚠ No hours" in amber at the bottom.
  - **Day off:** a chip in the type colour with the type name or label. Half days show "½".
  - **Today:** `TODAY_COL`, a 1.5 px accent inner outline, and the day number in a filled accent circle.
  - **Selected day:** an outline in the colour of the chosen type.
  - Clicking a day selects it and opens the "Mark day" panel. Double-clicking opens the Journal filtered to that date.
- **Side panel** (~320 px, wraps below the grid on narrow windows):
  1. **Mark day.** Date title, then four type toggles (Holiday, Vacation, Sick, Other), an optional label field, Full day / Half day, a **Range…** button for multi-day spans, a one-line hint, then Cancel and **Save day off**. When editing an existing day off, also offer Delete.
  2. **Upcoming days off.** The next 5: a date tile in the type colour (month abbreviation over the day number), the name and "Weekday · Type". Below that, **Import Québec holidays** and **Office closure…** (a range of type Other with a label).
  3. **Month at a glance.** Three stats: Logged `X h`, No hours `N d` (amber), Days off `N`.

### 4.5 Projects dialog

- **Header:** title "Projects" with a muted "N active" line below it, and a ✕ on the right.
- **New project:** an uppercase eyebrow, then a 3-column grid of labelled fields: Code (mono, placeholder `HICA-002`), Name, Client (optional). Below it, a **Colour** row of six round swatches from the project palette (the selected one gets a double ring), then **+ Add project** on the right. New feature: projects need a colour column (§5.4).
- **Project list rows:** a 12 px rounded colour square, the code (mono 600, project colour), the name and client tag, muted `X h this month`, then edit and archive icon buttons.
- **App settings** (`PANEL_ALT` background), laid out as a two-column form (label and help text on the left, control on the right):
  - Backup mirror: path input (mono) and a **Choose…** button, with help text below.
  - Name on PDF reports.
  - Splash screen at startup: a switch row with help text.
  - Pause timers when idle: an idle-minutes dropdown (default 10) and a switch. The minutes value is new; store it in settings.

### 4.6 Notes editor window

- **Header:** a "NOTES" eyebrow, the task title, a project dot and code, and the R&D badge if applicable. Collapse and ✕ buttons on the right.
- **Toolbar** (`PANEL_ALT`):
  - Edit / **Split** / Preview segmented control. Split is new and should be the default when the window is ≥ 800 px wide.
  - A divider, then the icon buttons with tooltips and shortcuts, in groups:
    - Bold (Ctrl B), Italic (Ctrl I), Strikethrough, Inline code
    - Heading, Bulleted list, Numbered list, Checklist
    - Toggle checkbox on the current line (Ctrl Enter), Code block, Table, Divider
  - A button is highlighted when the cursor is in that kind of block (e.g. Checklist on a `- [ ]` line).
  - Right side: muted "Markdown".
- **Body.** In Split mode there are two equal panes:
  - **Left: source.** `INSET` background, mono 13.5, line-height ~26 px, line numbers in a gutter (`FAINT`, the current line brighter), and the current line highlighted with `#17171c`.
  - Syntax colouring via a `TextEdit` layouter: list markers `- ` in `FAINT`, `[x]` / `[ ]` in `ACCENT`, `#` headings bold, `` `code` `` in `ACCENT_TEXT`, `**bold**` bold.
  - **Right: rendered preview.** Checklist items are real clickable checkboxes; toggling one rewrites the matching source line.
- **Footer** (`PANEL_ALT`):
  - Markdown cheat-sheet chips (mono 11.5, bordered): `**bold**` `# heading` `- list` `- [ ] todo` `` `code` ``.
  - An amber "● Unsaved changes" indicator when the text differs from what's saved.
  - The checklist count `2 / 3` in `ACCENT_TEXT`.
  - **Discard** (secondary) and **Save** (primary, with a `Ctrl S` hint). Ctrl S saves; Esc with unsaved changes asks for confirmation.

### 4.7 Date picker (replaces `egui_extras::DatePickerButton`)

Mockup: `docs/design/DatePicker.dc.html`. The stock egui_extras picker can't be themed to match: its weekend colour is hard-coded red and its layout (year/month/day combo boxes, `<<< << < > >> >>>`, Save/Cancel) doesn't fit the app. Write our own `date_picker(ui, &mut NaiveDate, &DayMarkers) -> Response` widget and use it everywhere a date is picked: Log work, the "log the time?" banner, edit entry, week/month pickers and Calendar "Mark day". Check the egui_extras source to confirm where the red comes from before replacing it.

- **Trigger:** the existing date button (calendar icon + "Today, Nov 15" / "Thu, Nov 12" / "Nov 12, 2025" when not this year). While the picker is open it gets the accent focus ring.
- **Popover:**
  - Opens below the trigger: 316 px wide, `PANEL` background, `BORDER_STRONG` stroke, radius 14, soft shadow, 12 px padding.
  - Closes on outside click or Esc.
  - Use an `egui::Area` / `popup_below_widget` and keep it inside the window.
- **Header:** a ghost title button "November **2026**" (year muted) with a ▾ that switches to the month view, then ‹ › ghost icon buttons (32 px). No `<<<` / `>>>` buttons and no combo boxes.
- **Quick picks:** small secondary buttons (28 px, radius 7) for **Today**, **Yesterday** and **Last Friday**. One click selects and closes.
- **Day grid:**
  - A narrow ISO week-number column (mono 10.5, `FAINT`), then Mo–Su headers (11.5, `MUTED`; Sa/Su in `FAINT`).
  - Cells are 36 px tall, radius 8, transparent, with `CONTROL_HOVER` on hover.
  - Text colour: `TEXT` for weekdays, `#a9a9b5` for weekend days and `FAINT` for days outside the month. **No red and no filled backgrounds for weekends.**
  - Selected: `ACCENT` fill, `ACCENT_INK` text, weight 700.
  - Today, when not selected: 1.5 px `ACCENT` ring.
  - A 4 px marker dot under the number:
    - project colour if the day has hours (the project with the most hours)
    - `AMBER` for a weekday with no hours (§5.2)
    - the day-off type colour (§5.3)
    - none otherwise
- **Month view:** ‹ 2026 › header and a 3×4 grid of 44 px month buttons. The current selection uses an `ACCENT` fill. Picking a month returns to the day view; clicking the year title is optional.
- **Footer:** a hairline divider, then a legend (● logged, ● no hours) and the key hints `↵ pick`, `Esc close`.
- **Behaviour:**
  - **A single click selects the date and closes the popover**, so there's no Save/Cancel.
  - Keyboard: arrows move the focused day, PgUp/PgDn change month, Home/End go to the start/end of the week, Enter picks, Esc closes without changing anything.
  - The markers come from the same query as the Calendar (`DayMarkers` = map of date → marker). Compute them once per visible month and cache.

---

## 5. New behaviour

### 5.1 Custom project row order (weekly summary)

Purpose: the weekly grid should list projects in the same order as the office timesheet, so hours can be typed straight across.

- **Migration:** `projects.report_order INTEGER`. Back-fill it with the current alphabetical order by code.
- **Order everywhere:** the weekly grid, the weekly plain-text preview, Copy week, the weekly CSV and the weekly PDF all sort by `report_order`, then by code. Apply the same order to the monthly summary for consistency.
- **↑ / ↓** swaps `report_order` with the neighbouring row. The neighbour is in the global project order, so moving a row past a project with no hours this week still works.
- **Dragging** a row by its grip reorders it (egui: `dnd_drag_source` / `dnd_drop_zone`, or a manual drag with a drop indicator line). Write the new order in one transaction.
- **Reset to A–Z** rewrites `report_order` alphabetically, after a confirm.
- New projects get `max(report_order) + 1`. Archived projects keep their value.

### 5.2 Weekdays with no hours

- A day is **missing** if all of these are true: it's Monday–Friday; it's before today (today never counts, nor future days); it has zero logged hours across all projects; and it has no day off (§5.3). A half-day off also clears it.
- Write one pure function, e.g. `fn missing_weekdays(range: DateRange, entries, days_off, today) -> Vec<NaiveDate>`, and unit-test it (weekends, today, half days, days off, partial weeks, month and year boundaries).
- **Weekly summary:** amber column header with "!", tinted column, an amber `0` in the daily total, and the pill `⚠ N weekdays with no hours` (hidden when N = 0).
- **Calendar:** the "No hours" cell state and the "No hours N d" stat.
- **Status bar** (optional, nice to have): when the current week has missing days, show an amber `⚠ N` before "This week".

### 5.3 Calendar tab and days off

- **Migration:** a new table

  ```sql
  CREATE TABLE days_off (
    id     INTEGER PRIMARY KEY,
    date   TEXT NOT NULL,           -- ISO yyyy-mm-dd, one row per day
    kind   TEXT NOT NULL,           -- 'holiday' | 'vacation' | 'sick' | 'other'
    label  TEXT,
    half   INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'manual'  -- 'manual' | 'import:qc'
  );
  CREATE UNIQUE INDEX days_off_date ON days_off(date);
  ```

  Ranges ("Range…", "Office closure…") expand to one row per weekday in the range.
- **Calendar data:** one query per visible month for the hours per day per project, plus the `days_off` rows for those six weeks.
- **Import Québec holidays:**
  - Generate the provincial statutory holidays for a chosen year and insert them with `kind = 'holiday'` and `source = 'import:qc'`, skipping dates that already have a day off.
  - The holidays: New Year's Day (Jan 1); Good Friday **or** Easter Monday (ask which, since the employer chooses); National Patriots' Day (the Monday before May 25); Fête nationale (June 24); Canada Day (July 1); Labour Day (first Monday of September); Thanksgiving (second Monday of October); Christmas Day (Dec 25).
  - Compute Easter with a standard computus function and unit-test it.
  - Show a preview list with checkboxes before inserting. Imported days stay editable and deletable.
- **Reports:** a day off cancels the "missing" state. In the weekly plain-text preview and Copy week, add a line per day off, e.g. `  - Fri 11-27: Vacation (long weekend)`. Days off carry no hours unless I later add a setting for that.

### 5.4 Project colour

- **Migration:** `projects.color TEXT`. Back-fill existing projects with distinct palette entries (keep the current blue and lime if the app already stores a colour).
- **Used by:** project dots, chips, heatmap cells, the stacked bar, monthly bars, the timer pill code and calendar chips. Editable in the Projects dialog (new and edit).

### 5.5 Small extras from the mockups

These are optional; ask before skipping any:

- Keyboard: `1`–`4` switch tabs (only when no text field has focus); `Ctrl K` focuses task search (already exists).
- Journal entry rows show `✓ done/total` when the entry's task has a checklist.
- Projects list shows `X h this month`.
- Idle pause minutes become configurable (5 / 10 / 15 / 30).

---

## 6. Suggested phases

1. **Theme and widgets.** Fonts, tokens, visuals, and the shared widgets in §2. The app should already look different with no layout changes.
2. **Shell.** Top bar with tabs, timer pill and shortcuts; status bar.
3. **Journal and Tasks** layouts (§4.1, §4.2).
4. **Reports** layout (§4.3) plus §5.1 row order (with migration) and §5.2 missing weekdays.
5. **Projects dialog** (§4.5) plus §5.4 colour.
6. **Notes editor** (§4.6) and the **date picker** (§4.7). If phases 1–5 are already done, do the date picker on its own as the next step.
7. **Calendar** (§4.4) plus §5.3 days off and the Québec import.

After each phase, list anything you couldn't match from the mockups and why.

## 7. Done when

- Every screen matches its mockup in `docs/design/` in colours, type, spacing, states and empty states.
- Existing data opens unchanged after migration, and an older DB upgrades cleanly (test against a copy).
- `missing_weekdays`, the row ordering, Easter/holiday generation and day-off range expansion all have unit tests.
- `cargo clippy` is clean and there are no new `unwrap()` calls on DB or IO paths.
