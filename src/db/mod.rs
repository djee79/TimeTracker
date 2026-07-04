pub mod log_entry;
pub mod project;
pub mod task;

use std::path::PathBuf;

use rusqlite::Connection;

pub use log_entry::LogEntryRow;
pub use project::{Project, ProjectStatus};
pub use task::{Priority, Task, TaskSort, TaskStatus};

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

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
    pub fn backup_daily(&self, keep: usize) -> std::result::Result<Option<PathBuf>, String> {
        let Some(parent) = self.path.parent() else {
            return Ok(None); // in-memory db
        };
        let dir = parent.join("backups");
        std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let today = chrono::Local::now().date_naive();
        let target = dir.join(format!("worklog-{today}.db"));
        if target.exists() {
            return Ok(None); // already backed up today
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
        Ok(Some(target))
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

        db.insert_log_entry(d("2026-06-29"), p1, "alpha work", 2.5, true)
            .unwrap();
        db.insert_log_entry(d("2026-06-30"), p2, "beta work", 1.0, false)
            .unwrap();
        let id = db
            .insert_log_entry(d("2026-05-01"), p1, "old work", 4.0, false)
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
    fn task_lifecycle_and_sorting() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let older = db.insert_task(p, "older todo", Priority::Normal).unwrap();
        let newer = db.insert_task(p, "newer doing", Priority::Normal).unwrap();
        db.set_task_status(newer, TaskStatus::Doing).unwrap();

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
        db.insert_log_entry(d("2026-07-04"), p1, "fixed authentication bug", 2.0, true)
            .unwrap();
        db.insert_log_entry(d("2026-07-04"), p2, "wrote auth docs", 1.5, false)
            .unwrap();
        db.insert_log_entry(d("2026-07-03"), p2, "design review", 3.0, false)
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

        let first = db.backup_daily(10).unwrap();
        let path = first.expect("first call should produce a backup");
        assert!(path.exists());
        // the snapshot is a valid database with the data in it
        let copy = Db::open(path).unwrap();
        assert_eq!(copy.list_projects().unwrap().len(), 1);
        // second call the same day is a no-op
        assert_eq!(db.backup_daily(10).unwrap(), None);

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
        let kept = db.backup_daily(2).unwrap().unwrap();
        let mut names: Vec<String> = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".db")) // the opened copy grows -wal/-shm sidecars
            .collect();
        names.sort();
        assert_eq!(names.len(), 2);
        assert_eq!(names[1], kept.file_name().unwrap().to_string_lossy());

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
        db.insert_log_entry(d("2026-03-02"), p1, "b later", 1.0, true)
            .unwrap();
        db.insert_log_entry(d("2026-03-01"), p1, "b earlier", 1.0, true)
            .unwrap();
        db.insert_log_entry(d("2026-03-01"), p2, "a not dev", 1.0, false)
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
