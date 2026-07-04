use chrono::{DateTime, Utc};
use rusqlite::Row;

use super::{Db, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Todo,
    Doing,
    Done,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::Doing => "doing",
            TaskStatus::Done => "done",
        }
    }

    fn parse(s: &str) -> TaskStatus {
        match s {
            "doing" => TaskStatus::Doing,
            "done" => TaskStatus::Done,
            _ => TaskStatus::Todo,
        }
    }
}

/// How the open-task list orders itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSort {
    /// Status, then priority, then recency — the list sorts itself.
    Auto,
    /// User-dragged order (`sort_order` column).
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
}

impl Priority {
    pub fn from_i64(v: i64) -> Priority {
        match v {
            0 => Priority::Low,
            2 => Priority::High,
            _ => Priority::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
        }
    }

    /// Click-to-cycle order: normal → high → low → normal.
    pub fn cycled(self) -> Priority {
        match self {
            Priority::Normal => Priority::High,
            Priority::High => Priority::Low,
            Priority::Low => Priority::Normal,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Timer running since this instant (while the task is `doing`).
    pub started_at: Option<DateTime<Utc>>,
    /// Tracked seconds from previous start→pause stretches.
    pub spent_secs: i64,
    // denormalized for display
    pub project_code: String,
}

impl Task {
    /// Total tracked time: banked stretches plus the running one, if any.
    pub fn tracked_secs(&self, now: DateTime<Utc>) -> i64 {
        let running = self
            .started_at
            .map_or(0, |s| (now - s).num_seconds().max(0));
        self.spent_secs + running
    }
}

fn from_row(row: &Row) -> rusqlite::Result<Task> {
    let status: String = row.get(3)?;
    let created_at: String = row.get(4)?;
    let completed_at: Option<String> = row.get(5)?;
    let started_at: Option<String> = row.get(8)?;
    Ok(Task {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        status: TaskStatus::parse(&status),
        created_at: created_at
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now()),
        completed_at: completed_at.and_then(|s| s.parse().ok()),
        project_code: row.get(6)?,
        priority: Priority::from_i64(row.get(7)?),
        started_at: started_at.and_then(|s| s.parse().ok()),
        spent_secs: row.get(9)?,
    })
}

const SELECT: &str = "SELECT t.id, t.project_id, t.title, t.status, t.created_at, t.completed_at, p.code, t.priority, t.started_at, t.spent_secs
 FROM tasks t JOIN projects p ON p.id = t.project_id";

/// Second-precision RFC3339 — what SQLite's strftime('%s', …) parses cleanly.
fn ts(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Db {
    /// New tasks land at the top of the manual order.
    pub fn insert_task(&self, project_id: i64, title: &str, priority: Priority) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tasks (project_id, title, created_at, priority, sort_order)
             VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT MIN(sort_order) FROM tasks), 0.0) - 1.0)",
            rusqlite::params![project_id, title, Utc::now().to_rfc3339(), priority as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_task_priority(&self, id: i64, priority: Priority) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET priority = ?2 WHERE id = ?1",
            rusqlite::params![id, priority as i64],
        )?;
        Ok(())
    }

    /// Sets status, maintaining `completed_at` (stamped when entering `done`,
    /// cleared when leaving it).
    pub fn set_task_status(&self, id: i64, status: TaskStatus) -> Result<()> {
        let completed_at = (status == TaskStatus::Done).then(|| Utc::now().to_rfc3339());
        self.conn.execute(
            "UPDATE tasks SET status = ?2, completed_at = ?3 WHERE id = ?1",
            rusqlite::params![id, status.as_str(), completed_at],
        )?;
        Ok(())
    }

    pub fn delete_task(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Open tasks. Auto: doing first, then priority (high → low), then newest
    /// first. Manual: whatever order the user dragged them into.
    pub fn list_open_tasks(&self, sort: TaskSort) -> Result<Vec<Task>> {
        let order = match sort {
            TaskSort::Auto => "t.status = 'doing' DESC, t.priority DESC, t.created_at DESC",
            TaskSort::Manual => "t.sort_order ASC",
        };
        let mut stmt = self
            .conn
            .prepare(&format!("{SELECT} WHERE t.status != 'done' ORDER BY {order}"))?;
        let rows = stmt.query_map([], from_row)?;
        rows.collect()
    }

    /// Persist a full manual ordering: `ids` in top-to-bottom display order.
    pub fn save_task_order(&self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE tasks SET sort_order = ?2 WHERE id = ?1",
                rusqlite::params![id, i as f64],
            )?;
        }
        tx.commit()
    }

    /// Start the timer if it isn't already running (so re-focusing a task
    /// that's already in progress never resets the current stretch).
    pub fn start_task_timer(&self, id: i64, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET started_at = ?2 WHERE id = ?1 AND started_at IS NULL",
            rusqlite::params![id, ts(now)],
        )?;
        Ok(())
    }

    /// Fold the running stretch into `spent_secs`, remember it if it's the
    /// longest so far, and stop the timer. No-op when the timer isn't running.
    pub fn stop_task_timer(&self, id: i64, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET
                 spent_secs = spent_secs + MAX(0, strftime('%s', ?2) - strftime('%s', started_at)),
                 max_stretch_secs = MAX(max_stretch_secs, strftime('%s', ?2) - strftime('%s', started_at)),
                 started_at = NULL
             WHERE id = ?1 AND started_at IS NOT NULL",
            rusqlite::params![id, ts(now)],
        )?;
        Ok(())
    }

    /// Bank every running timer — called on app shutdown so closed-app time
    /// doesn't count as work.
    pub fn stop_all_task_timers(&self, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET
                 spent_secs = spent_secs + MAX(0, strftime('%s', ?1) - strftime('%s', started_at)),
                 max_stretch_secs = MAX(max_stretch_secs, strftime('%s', ?1) - strftime('%s', started_at)),
                 started_at = NULL
             WHERE started_at IS NOT NULL",
            rusqlite::params![ts(now)],
        )?;
        Ok(())
    }

    /// (Re)start timers for every in-progress task — called on app startup.
    /// The timer runs whenever a task is `doing` and the app is open; any
    /// stale `started_at` left by a crash is reset rather than counted.
    pub fn restart_doing_timers(&self, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET started_at = ?1 WHERE status = 'doing'",
            rusqlite::params![ts(now)],
        )?;
        Ok(())
    }

    /// Banked tracked seconds and the longest single stretch (call after
    /// `stop_task_timer` for up-to-date totals).
    pub fn task_timer_totals(&self, id: i64) -> Result<(i64, i64)> {
        self.conn.query_row(
            "SELECT spent_secs, max_stretch_secs FROM tasks WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// Zero the tracked counters — called once tracked time has been logged,
    /// so the counter always means "tracked but not yet logged".
    pub fn reset_task_spent(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET spent_secs = 0, max_stretch_secs = 0 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Completed tasks, most recently finished first. `since` bounds the
    /// completion date (local time); every whitespace-separated word of
    /// `search` must appear in the title or project code, in any order.
    /// Filtering happens in SQL so a years-old backlog stays cheap to show.
    pub fn list_done_tasks(
        &self,
        limit: usize,
        since: Option<chrono::NaiveDate>,
        search: &str,
    ) -> Result<Vec<Task>> {
        let mut sql = format!("{SELECT} WHERE t.status = 'done'");
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(since) = since {
            sql.push_str(" AND date(t.completed_at, 'localtime') >= ?");
            params.push(Box::new(since.to_string()));
        }
        for term in search.split_whitespace() {
            sql.push_str(" AND (t.title LIKE ? OR p.code LIKE ?)");
            let pattern = format!("%{term}%");
            params.push(Box::new(pattern.clone()));
            params.push(Box::new(pattern));
        }
        sql.push_str(" ORDER BY t.completed_at DESC LIMIT ?");
        params.push(Box::new(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), from_row)?;
        rows.collect()
    }
}
