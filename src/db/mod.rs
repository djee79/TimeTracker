pub mod log_entry;
pub mod project;
pub mod task;

use std::path::PathBuf;

use rusqlite::Connection;

pub use log_entry::{LogEntry, LogEntryRow, SavedEntry, JOURNAL_LIMIT};
pub use project::{Project, ProjectStatus};
pub use task::{Priority, Task, TaskSort, TaskStatus};

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

/// A `%term%` LIKE pattern with the user's `%`, `_` and `\` escaped so they
/// match literally. Every use must add `ESCAPE '\'` to its LIKE clause.
pub(crate) fn like_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Schema migrations, applied in order. `PRAGMA user_version` tracks how many
/// have run, so appending a new SQL block here is all a future migration needs.
const MIGRATIONS: &[&str] = &[
    // v1: initial schema
    "
    CREATE TABLE projects (
        id      INTEGER PRIMARY KEY,
        code    TEXT NOT NULL UNIQUE,
        name    TEXT NOT NULL,
        client  TEXT,
        status  TEXT NOT NULL DEFAULT 'active'
                CHECK (status IN ('active', 'archived'))
    );

    CREATE TABLE log_entries (
        id          INTEGER PRIMARY KEY,
        work_date   TEXT NOT NULL,              -- YYYY-MM-DD, day the work happened
        project_id  INTEGER NOT NULL REFERENCES projects(id),
        description TEXT NOT NULL,
        hours       REAL NOT NULL,
        is_dev      INTEGER NOT NULL DEFAULT 0, -- R&D flag for the annual export
        created_at  TEXT NOT NULL               -- RFC3339 UTC, immutable; contemporaneity evidence
    );
    CREATE INDEX idx_log_entries_work_date ON log_entries(work_date);
    CREATE INDEX idx_log_entries_project ON log_entries(project_id);

    CREATE TABLE tasks (
        id           INTEGER PRIMARY KEY,
        project_id   INTEGER NOT NULL REFERENCES projects(id),
        title        TEXT NOT NULL,
        status       TEXT NOT NULL DEFAULT 'todo'
                     CHECK (status IN ('todo', 'doing', 'done')),
        created_at   TEXT NOT NULL,
        completed_at TEXT
    );
    CREATE INDEX idx_tasks_status ON tasks(status);

    CREATE TABLE app_settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    ",
    // v2: task priority (0 = low, 1 = normal, 2 = high) — feeds the auto-sort
    "ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 1;",
    // v3: manual ordering (drag-to-reorder). Existing tasks start newest-first.
    "ALTER TABLE tasks ADD COLUMN sort_order REAL NOT NULL DEFAULT 0;
     UPDATE tasks SET sort_order = -id;",
    // v4: time tracking — started_at is set while the timer runs (RFC3339 UTC),
    // spent_secs accumulates finished stretches across pause/resume.
    "ALTER TABLE tasks ADD COLUMN started_at TEXT;
     ALTER TABLE tasks ADD COLUMN spent_secs INTEGER NOT NULL DEFAULT 0;",
    // v5: longest unbroken timer stretch — flags forgot-the-timer totals
    "ALTER TABLE tasks ADD COLUMN max_stretch_secs INTEGER NOT NULL DEFAULT 0;",
    // v6: freeform markdown notes per task
    "ALTER TABLE tasks ADD COLUMN details TEXT NOT NULL DEFAULT '';",
    // v7: the task a log entry came from (bridge-created entries only).
    // Plain INTEGER, no FK: deleting a task must not orphan-block its entries.
    "ALTER TABLE log_entries ADD COLUMN task_id INTEGER;",
];

pub struct Db {
    conn: Connection,
    path: PathBuf,
}

impl Db {
    /// Open (creating if needed) the database in the OS data dir,
    /// e.g. `~/.local/share/worklog/worklog.db` on Linux.
    pub fn open_default() -> std::result::Result<Db, String> {
        let dirs = directories::ProjectDirs::from("", "", "worklog")
            .ok_or("could not determine OS data directory")?;
        let dir = dirs.data_dir();
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        Self::open(dir.join("worklog.db")).map_err(|e| format!("opening database: {e}"))
    }

    pub fn open(path: PathBuf) -> Result<Db> {
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db { conn, path };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        let version: usize =
            self.conn
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            self.conn.execute_batch(sql)?;
            self.conn
                .pragma_update(None, "user_version", (i + 1) as i64)?;
        }
        Ok(())
    }

    /// Snapshot the database into `backups/worklog-YYYY-MM-DD.db` next to the
    /// live file (at most once per day) and prune to the `keep` newest.
    /// Uses VACUUM INTO, so the copy is a consistent snapshot even in WAL mode.
    /// Returns today's snapshot path and whether this call created it (false =
    /// it already existed), so the caller can mirror it elsewhere either way.
    pub fn backup_daily(
        &self,
        keep: usize,
    ) -> std::result::Result<Option<(PathBuf, bool)>, String> {
        let Some(parent) = self.path.parent() else {
            return Ok(None); // in-memory db
        };
        let dir = parent.join("backups");
        std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let today = chrono::Local::now().date_naive();
        let target = dir.join(format!("worklog-{today}.db"));
        if target.exists() {
            return Ok(Some((target, false))); // already backed up today
        }
        self.conn
            .execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])
            .map_err(|e| format!("backup failed: {e}"))?;

        let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("worklog-") && n.ends_with(".db"))
            })
            .collect();
        old.sort(); // date-named, so lexicographic = chronological
        while old.len() > keep {
            let _ = std::fs::remove_file(old.remove(0));
        }
        Ok(Some((target, true)))
    }

    // -- maintenance --

    /// SQLite's fast self-check. None = healthy; Some = what it reported.
    /// Cheap at this database's size, so it runs on every launch to catch
    /// silent file corruption (disk issues, bad copies) while the daily
    /// backups still hold good data.
    pub fn integrity_check(&self) -> Result<Option<String>> {
        let report: String =
            self.conn
                .query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        Ok((report != "ok").then_some(report))
    }

    /// Compact the file (reclaims space freed by deleted rows) at most once
    /// per month. Returns true when a vacuum actually ran.
    pub fn vacuum_monthly(&self) -> Result<bool> {
        let this_month = chrono::Local::now().format("%Y-%m").to_string();
        if self.setting("last_vacuum").as_deref() == Some(this_month.as_str()) {
            return Ok(false);
        }
        self.conn.execute_batch("VACUUM")?;
        self.set_setting("last_vacuum", &this_month)?;
        Ok(true)
    }

    /// SQLite's own tune-up (refreshes query-planner statistics) — the
    /// documented best practice is to run it when closing the connection.
    pub fn optimize(&self) {
        let _ = self.conn.execute_batch("PRAGMA optimize");
    }

    // -- app_settings: tiny key/value store (last-used project, UI prefs) --

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db {
            conn,
            path: PathBuf::from(":memory:"),
        };
        db.migrate()?;
        Ok(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn migrations_apply_once() {
        let db = Db::open_in_memory().unwrap();
        let v: i64 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        db.migrate().unwrap(); // idempotent
    }

    #[test]
    fn settings_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting("x"), None);
        db.set_setting("x", "1").unwrap();
        db.set_setting("x", "2").unwrap();
        assert_eq!(db.setting("x").as_deref(), Some("2"));
    }

    #[test]
    fn log_entry_crud_and_filters() {
        let db = Db::open_in_memory().unwrap();
        let p1 = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let p2 = db.insert_project("BBB-002", "Beta", Some("ACME")).unwrap();

        db.insert_log_entry(d("2026-06-29"), p1, "alpha work", 2.5, true, None)
            .unwrap();
        db.insert_log_entry(d("2026-06-30"), p2, "beta work", 1.0, false, None)
            .unwrap();
        let id = db
            .insert_log_entry(d("2026-05-01"), p1, "old work", 4.0, false, None)
            .unwrap();

        assert_eq!(db.list_log_entries(None, None, "").unwrap().len(), 3);
        assert_eq!(db.list_log_entries(Some(p1), None, "").unwrap().len(), 2);
        assert_eq!(
            db.list_log_entries(None, Some(d("2026-06-01")), "").unwrap().len(),
            2
        );
        // newest work first
        let all = db.list_log_entries(None, None, "").unwrap();
        assert_eq!(all[0].entry.work_date, d("2026-06-30"));

        db.update_log_entry(id, d("2026-05-02"), p2, "moved", 3.0, true)
            .unwrap();
        let moved = &db.list_log_entries(Some(p2), None, "").unwrap();
        assert_eq!(moved.len(), 2);

        db.delete_log_entry(id).unwrap();
        assert_eq!(db.list_log_entries(None, None, "").unwrap().len(), 2);
    }

    #[test]
    fn merge_folds_same_day_same_description() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let first = db
            .merge_or_insert_log_entry(d("2026-07-20"), p, "Update AVEVA", 2.0, false, None)
            .unwrap();
        let SavedEntry::Inserted(id) = first else {
            panic!("first save must insert");
        };

        // same day, case/whitespace differ → hours fold into the first entry
        let second = db
            .merge_or_insert_log_entry(d("2026-07-20"), p, "  update aveva ", 1.5, false, Some(7))
            .unwrap();
        assert_eq!(
            second,
            SavedEntry::Merged { id, total_hours: 3.5, prev_hours: 2.0, prev_task_id: None }
        );
        let all = db.list_log_entries(None, None, "").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].entry.hours, 3.5);
        assert_eq!(all[0].entry.description, "Update AVEVA"); // first casing wins
        assert_eq!(all[0].entry.task_id, Some(7)); // adopted from the merge

        // undoing the merge restores the pre-merge hours and task link
        db.unmerge_log_entry(id, 2.0, None).unwrap();
        let all = db.list_log_entries(None, None, "").unwrap();
        assert_eq!(all[0].entry.hours, 2.0);
        assert_eq!(all[0].entry.task_id, None);
        db.unmerge_log_entry(id, 3.5, Some(7)).unwrap(); // put it back for the rest

        // different day, project, dev flag or description → separate entries
        let p2 = db.insert_project("BBB-002", "Beta", None).unwrap();
        for (date, proj, desc, dev) in [
            (d("2026-07-21"), p, "Update AVEVA", false),
            (d("2026-07-20"), p2, "Update AVEVA", false),
            (d("2026-07-20"), p, "Update AVEVA", true),
            (d("2026-07-20"), p, "Update Citect", false),
        ] {
            let saved = db
                .merge_or_insert_log_entry(date, proj, desc, 1.0, dev, None)
                .unwrap();
            assert!(matches!(saved, SavedEntry::Inserted(_)));
        }
        assert_eq!(db.list_log_entries(None, None, "").unwrap().len(), 5);
    }

    #[test]
    fn search_matches_like_wildcards_literally() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        db.insert_log_entry(d("2026-07-20"), p, "meeting 50% done", 1.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-20"), p, "meeting 50x done", 1.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-20"), p, "wrote spec_v2", 1.0, false, None)
            .unwrap();

        // % and _ are literal characters, not wildcards
        let hits = db.list_log_entries(None, None, "50%").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.description, "meeting 50% done");
        let hits = db.list_log_entries(None, None, "spec_v2").unwrap();
        assert_eq!(hits.len(), 1);

        // same contract for the task searches
        let t = db.insert_task(p, "review 100% coverage", Priority::Normal).unwrap();
        assert_eq!(db.search_tasks("100%", 10).unwrap().len(), 1);
        assert!(db.search_tasks("100_", 10).unwrap().is_empty());
        db.set_task_status(t, TaskStatus::Done).unwrap();
        assert_eq!(db.list_done_tasks(10, None, "100%").unwrap().len(), 1);
    }

    #[test]
    fn dedupe_folds_legacy_duplicates() {
        let db = Db::open_in_memory().unwrap();
        let p1 = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let p2 = db.insert_project("BBB-002", "Beta", None).unwrap();

        // three-way duplicate: keeper is the first, hours sum, task link adopted
        let keeper = db
            .insert_log_entry(d("2026-07-20"), p1, "Update AVEVA", 2.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-20"), p1, "update aveva", 1.5, false, Some(4))
            .unwrap();
        db.insert_log_entry(d("2026-07-20"), p1, " UPDATE AVEVA ", 0.5, false, None)
            .unwrap();
        // near-misses that must survive untouched
        db.insert_log_entry(d("2026-07-21"), p1, "Update AVEVA", 1.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-20"), p2, "Update AVEVA", 1.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-20"), p1, "Update AVEVA", 1.0, true, None)
            .unwrap();

        assert_eq!(db.merge_duplicate_log_entries().unwrap(), 2);
        let all = db.list_log_entries(None, None, "").unwrap();
        assert_eq!(all.len(), 4);
        let kept = all.iter().find(|r| r.entry.id == keeper).unwrap();
        assert_eq!(kept.entry.hours, 4.0);
        assert_eq!(kept.entry.description, "Update AVEVA");
        assert_eq!(kept.entry.task_id, Some(4));

        // second pass finds nothing — the cleanup is idempotent
        assert_eq!(db.merge_duplicate_log_entries().unwrap(), 0);
    }

    #[test]
    fn task_lifecycle_and_sorting() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let older = db.insert_task(p, "older todo", Priority::Normal).unwrap();
        let newer = db.insert_task(p, "newer doing", Priority::Normal).unwrap();
        db.set_task_status(newer, TaskStatus::Doing).unwrap();

        db.set_task_title(older, "older todo, refined").unwrap();
        let open = db.list_open_tasks(TaskSort::Auto).unwrap();
        assert_eq!(
            open.iter().find(|t| t.id == older).unwrap().title,
            "older todo, refined"
        );

        let open = db.list_open_tasks(TaskSort::Auto).unwrap();
        assert_eq!(open.len(), 2);
        // doing sorts above todo regardless of age
        assert_eq!(open[0].id, newer);
        assert_eq!(open[0].status, TaskStatus::Doing);

        db.set_task_status(older, TaskStatus::Done).unwrap();
        let done = db.list_done_tasks(10, None, "").unwrap();
        assert_eq!(done.len(), 1);
        assert!(done[0].completed_at.is_some());

        // reopening clears completed_at
        db.set_task_status(older, TaskStatus::Todo).unwrap();
        assert!(db.list_done_tasks(10, None, "").unwrap().is_empty());
        let reopened = db.list_open_tasks(TaskSort::Auto).unwrap();
        assert!(reopened.iter().all(|t| t.completed_at.is_none()));
    }

    #[test]
    fn undo_reinserts_entries_and_tasks_faithfully() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let t = db.insert_task(p, "with notes", Priority::High).unwrap();
        db.set_task_details(t, "- [x] kept").unwrap();
        db.set_task_status(t, TaskStatus::Doing).unwrap();
        db.insert_log_entry(d("2026-07-01"), p, "logged", 2.0, true, Some(t))
            .unwrap();

        // entry: delete, reinsert, original created_at and task link survive
        let row = db.list_log_entries(None, None, "").unwrap().remove(0);
        db.delete_log_entry(row.entry.id).unwrap();
        db.reinsert_log_entry(&row.entry).unwrap();
        let back = db.list_log_entries(None, None, "").unwrap().remove(0);
        assert_eq!(back.entry.created_at, row.entry.created_at);
        assert_eq!(back.entry.task_id, Some(t));
        assert_eq!(back.entry.hours, 2.0);

        // task: delete, reinsert — status, priority, notes survive; the
        // in-progress timer is running again
        let task = db.list_open_tasks(TaskSort::Auto).unwrap().remove(0);
        db.delete_task(task.id).unwrap();
        db.reinsert_task(&task, chrono::Utc::now()).unwrap();
        let back = db.list_open_tasks(TaskSort::Auto).unwrap().remove(0);
        assert_eq!(back.title, "with notes");
        assert_eq!(back.status, TaskStatus::Doing);
        assert_eq!(back.priority, Priority::High);
        assert_eq!(back.details, "- [x] kept");
        assert!(back.started_at.is_some());
    }

    #[test]
    fn search_tasks_all_statuses_and_notes() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let open = db.insert_task(p, "fix the parser", Priority::Normal).unwrap();
        let done = db.insert_task(p, "old cleanup", Priority::Normal).unwrap();
        db.set_task_details(done, "remember the parser edge case").unwrap();
        db.set_task_status(done, TaskStatus::Done).unwrap();

        let hits = db.search_tasks("parser", 10).unwrap();
        assert_eq!(hits.len(), 2); // title match + notes match
        assert_eq!(hits[0].id, open); // open tasks sort first
        assert!(db.search_tasks("nothing here", 10).unwrap().is_empty());
    }

    #[test]
    fn checklist_counts_markdown_task_items() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let t = db.insert_task(p, "task", Priority::Normal).unwrap();
        let task = |db: &Db| db.list_open_tasks(TaskSort::Auto).unwrap().remove(0);
        assert_eq!(task(&db).checklist(), None);
        db.set_task_details(t, "## plan\n- [x] one\n- [ ] two\n  1. [X] nested numbered\n* [ ] star\nplain - [ ] not a list? no: dash needed at start")
            .unwrap();
        assert_eq!(task(&db).checklist(), Some((2, 4)));
    }

    #[test]
    fn maintenance_check_and_monthly_vacuum() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.integrity_check().unwrap(), None);
        assert!(db.vacuum_monthly().unwrap()); // first run this month
        assert!(!db.vacuum_monthly().unwrap()); // second is a no-op
        db.optimize(); // must not error on a healthy db
    }

    #[test]
    fn log_entry_remembers_its_task() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let t = db.insert_task(p, "the task", Priority::Normal).unwrap();
        db.insert_log_entry(d("2026-07-18"), p, "from the bridge", 2.0, false, Some(t))
            .unwrap();
        db.insert_log_entry(d("2026-07-18"), p, "typed by hand", 1.0, false, None)
            .unwrap();
        let entries = db.list_log_entries(None, None, "").unwrap();
        assert_eq!(entries.len(), 2);
        let by_desc = |s: &str| {
            entries.iter().find(|r| r.entry.description == s).unwrap().entry.task_id
        };
        assert_eq!(by_desc("from the bridge"), Some(t));
        assert_eq!(by_desc("typed by hand"), None);
        // the lookup the notes panel uses works for done tasks too
        db.set_task_status(t, TaskStatus::Done).unwrap();
        assert_eq!(db.task(t).unwrap().unwrap().title, "the task");
        assert!(db.task(9999).unwrap().is_none());
    }

    #[test]
    fn task_details_roundtrip_and_search() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let t = db.insert_task(p, "titled", Priority::Normal).unwrap();
        assert_eq!(db.list_open_tasks(TaskSort::Auto).unwrap()[0].details, "");

        db.set_task_details(t, "## plan\nwatch the **edge case**").unwrap();
        assert_eq!(
            db.list_open_tasks(TaskSort::Auto).unwrap()[0].details,
            "## plan\nwatch the **edge case**"
        );

        // the completed-section search also looks inside the notes
        db.set_task_status(t, TaskStatus::Done).unwrap();
        assert_eq!(db.list_done_tasks(10, None, "edge case").unwrap().len(), 1);
        assert!(db.list_done_tasks(10, None, "unrelated").unwrap().is_empty());
    }

    #[test]
    fn task_timer_accumulates_across_pauses() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let t = db.insert_task(p, "timed", Priority::Normal).unwrap();
        let t0: chrono::DateTime<chrono::Utc> = "2026-07-04T09:00:00Z".parse().unwrap();
        let at = |mins: i64| t0 + chrono::Duration::minutes(mins);

        // stop without start is a no-op
        db.stop_task_timer(t, at(1)).unwrap();
        assert_eq!(db.task_timer_totals(t).unwrap().0, 0);

        db.start_task_timer(t, t0).unwrap();
        // starting again while running must not reset the stretch
        db.start_task_timer(t, at(5)).unwrap();
        db.stop_task_timer(t, at(10)).unwrap();
        assert_eq!(db.task_timer_totals(t).unwrap().0, 600);

        // a second stretch accumulates
        db.start_task_timer(t, at(60)).unwrap();
        db.stop_task_timer(t, at(75)).unwrap();
        assert_eq!(db.task_timer_totals(t).unwrap().0, 1500);

        // stopping twice doesn't double-count
        db.stop_task_timer(t, at(90)).unwrap();
        assert_eq!(db.task_timer_totals(t).unwrap().0, 1500);

        // longest single stretch is remembered (10 min, then 15 min)
        assert_eq!(db.task_timer_totals(t).unwrap().1, 900);

        // once logged, both counters restart from zero
        db.reset_task_spent(t).unwrap();
        assert_eq!(db.task_timer_totals(t).unwrap(), (0, 0));
    }

    #[test]
    fn log_entry_search_and_daily_total() {
        let db = Db::open_in_memory().unwrap();
        let p1 = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let p2 = db.insert_project("BBB-002", "Beta", None).unwrap();
        db.insert_log_entry(d("2026-07-04"), p1, "fixed authentication bug", 2.0, true, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-04"), p2, "wrote auth docs", 1.5, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-03"), p2, "design review", 3.0, false, None)
            .unwrap();

        // words match description or project code, any order, case-insensitive
        assert_eq!(db.list_log_entries(None, None, "auth").unwrap().len(), 2);
        assert_eq!(db.list_log_entries(None, None, "AUTH bbb").unwrap().len(), 1);
        assert_eq!(db.list_log_entries(None, None, "nope").unwrap().len(), 0);
        // composes with the other filters
        assert_eq!(
            db.list_log_entries(Some(p1), None, "auth").unwrap().len(),
            1
        );

        assert_eq!(db.hours_on(d("2026-07-04")).unwrap(), 3.5);
        assert_eq!(db.hours_on(d("2026-01-01")).unwrap(), 0.0);
    }

    #[test]
    fn backup_snapshots_once_a_day_and_prunes() {
        let dir = std::env::temp_dir().join(format!("worklog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(dir.join("worklog.db")).unwrap();
        db.insert_project("AAA-001", "Alpha", None).unwrap();

        let (path, created) = db.backup_daily(10).unwrap().expect("first call backs up");
        assert!(created);
        assert!(path.exists());
        // the snapshot is a valid database with the data in it
        let copy = Db::open(path.clone()).unwrap();
        assert_eq!(copy.list_projects().unwrap().len(), 1);
        drop(copy); // Windows can't delete a file something still has open
        // second call the same day reports the existing snapshot, creates nothing
        assert_eq!(db.backup_daily(10).unwrap(), Some((path, false)));

        // pruning keeps only the newest N (seed some fake older backups)
        for day in ["2020-01-01", "2020-01-02", "2020-01-03"] {
            std::fs::write(
                dir.join("backups").join(format!("worklog-{day}.db")),
                b"old",
            )
            .unwrap();
        }
        std::fs::remove_file(dir.join("backups").join(format!(
            "worklog-{}.db",
            chrono::Local::now().date_naive()
        )))
        .unwrap();
        let (kept, _) = db.backup_daily(2).unwrap().unwrap();
        let mut names: Vec<String> = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".db")) // the opened copy grows -wal/-shm sidecars
            .collect();
        names.sort();
        assert_eq!(names.len(), 2);
        assert_eq!(names[1], kept.file_name().unwrap().to_string_lossy());

        drop(db); // close the live database before deleting the directory
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn done_tasks_filter_by_date_and_search() {
        let db = Db::open_in_memory().unwrap();
        let p1 = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let p2 = db.insert_project("BBB-002", "Beta", None).unwrap();
        let old = db.insert_task(p1, "fix api endpoint", Priority::Normal).unwrap();
        let recent = db.insert_task(p2, "update api docs", Priority::Normal).unwrap();
        let other = db.insert_task(p2, "review design", Priority::Normal).unwrap();
        for id in [old, recent, other] {
            db.set_task_status(id, TaskStatus::Done).unwrap();
        }
        // backdate one completion well past any range window
        db.conn
            .execute(
                "UPDATE tasks SET completed_at = '2020-01-15T12:00:00+00:00' WHERE id = ?1",
                [old],
            )
            .unwrap();

        let ids = |v: Vec<crate::db::Task>| v.iter().map(|t| t.id).collect::<Vec<_>>();

        // no filters: everything, newest completion first
        assert_eq!(db.list_done_tasks(10, None, "").unwrap().len(), 3);
        assert_eq!(*ids(db.list_done_tasks(10, None, "").unwrap()).last().unwrap(), old);

        // date range excludes the backdated task
        let since = Some(d("2026-01-01"));
        assert_eq!(ids(db.list_done_tasks(10, since, "").unwrap()).len(), 2);

        // search: words match title or project code, any order
        assert_eq!(ids(db.list_done_tasks(10, None, "api").unwrap()), vec![recent, old]);
        assert_eq!(ids(db.list_done_tasks(10, None, "api BBB").unwrap()), vec![recent]);
        assert_eq!(ids(db.list_done_tasks(10, None, "API").unwrap()).len(), 2); // case-insensitive
        assert!(db.list_done_tasks(10, None, "nope").unwrap().is_empty());

        // both combined
        assert_eq!(ids(db.list_done_tasks(10, since, "api").unwrap()), vec![recent]);
    }

    #[test]
    fn timers_bank_on_shutdown_and_resume_for_doing() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let doing = db.insert_task(p, "doing", Priority::Normal).unwrap();
        let todo = db.insert_task(p, "todo", Priority::Normal).unwrap();
        db.set_task_status(doing, TaskStatus::Doing).unwrap();
        let t0: chrono::DateTime<chrono::Utc> = "2026-07-04T09:00:00Z".parse().unwrap();
        let at = |mins: i64| t0 + chrono::Duration::minutes(mins);

        db.start_task_timer(doing, t0).unwrap();
        db.stop_all_task_timers(at(30)).unwrap(); // app exit
        assert_eq!(db.task_timer_totals(doing).unwrap().0, 1800);

        // next launch: only the doing task resumes; the overnight gap
        // between stop and restart is not counted
        db.restart_doing_timers(at(600)).unwrap();
        let open = db.list_open_tasks(TaskSort::Auto).unwrap();
        let d = open.iter().find(|t| t.id == doing).unwrap();
        let td = open.iter().find(|t| t.id == todo).unwrap();
        assert_eq!(d.started_at, Some(at(600)));
        assert_eq!(td.started_at, None);
        assert_eq!(d.tracked_secs(at(660)), 1800 + 3600);
    }

    #[test]
    fn priority_orders_within_status() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let low = db.insert_task(p, "low", Priority::Low).unwrap();
        let normal = db.insert_task(p, "normal", Priority::Normal).unwrap();
        let high = db.insert_task(p, "high", Priority::High).unwrap();
        // a doing task still outranks a high-priority todo
        let doing = db.insert_task(p, "doing low", Priority::Low).unwrap();
        db.set_task_status(doing, TaskStatus::Doing).unwrap();

        let ids: Vec<i64> = db.list_open_tasks(TaskSort::Auto).unwrap().iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![doing, high, normal, low]);

        // bump `low` to high: both high-priority todos now outrank `normal`,
        // newest-first between them
        db.set_task_priority(low, Priority::High).unwrap();
        let ids: Vec<i64> = db.list_open_tasks(TaskSort::Auto).unwrap().iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![doing, high, low, normal]);
    }

    #[test]
    fn manual_order_and_new_tasks_on_top() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let a = db.insert_task(p, "a", Priority::Normal).unwrap();
        let b = db.insert_task(p, "b", Priority::High).unwrap();
        let c = db.insert_task(p, "c", Priority::Low).unwrap();

        // newest insert lands on top of the manual order, priority ignored
        let ids: Vec<i64> = db
            .list_open_tasks(TaskSort::Manual)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec![c, b, a]);

        // persist an arbitrary drag result and read it back
        db.save_task_order(&[a, c, b]).unwrap();
        let ids: Vec<i64> = db
            .list_open_tasks(TaskSort::Manual)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec![a, c, b]);

        // auto sort is unaffected by manual order
        let auto: Vec<i64> = db
            .list_open_tasks(TaskSort::Auto)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(auto, vec![b, a, c]); // high, normal, low
    }

    #[test]
    fn range_query_dev_filter_and_order() {
        let db = Db::open_in_memory().unwrap();
        let p1 = db.insert_project("BBB-002", "Beta", None).unwrap();
        let p2 = db.insert_project("AAA-001", "Alpha", None).unwrap();
        db.insert_log_entry(d("2026-03-02"), p1, "b later", 1.0, true, None)
            .unwrap();
        db.insert_log_entry(d("2026-03-01"), p1, "b earlier", 1.0, true, None)
            .unwrap();
        db.insert_log_entry(d("2026-03-01"), p2, "a not dev", 1.0, false, None)
            .unwrap();

        let rows = db
            .log_entries_in_range(d("2026-01-01"), d("2026-12-31"), true)
            .unwrap();
        // dev only, grouped by project code, chronological within project
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].entry.description, "b earlier");
        assert_eq!(rows[1].entry.description, "b later");
    }
}
