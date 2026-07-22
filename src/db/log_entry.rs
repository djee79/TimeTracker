use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::Row;

use super::{Db, Result};

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub id: i64,
    pub work_date: NaiveDate,
    pub project_id: i64,
    pub description: String,
    pub hours: f64,
    pub is_dev: bool,
    pub created_at: DateTime<Utc>,
    /// The task this entry was logged from (via the bridge), if any.
    pub task_id: Option<i64>,
}

/// A log entry joined with its project's code/name, for lists and reports.
#[derive(Debug, Clone)]
pub struct LogEntryRow {
    pub entry: LogEntry,
    pub project_code: String,
    pub project_name: String,
}

/// Outcome of a merge-aware save (see `merge_or_insert_log_entry`). A merge
/// carries the entry's pre-merge hours and task link so it can be undone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SavedEntry {
    Inserted(i64),
    Merged {
        id: i64,
        total_hours: f64,
        prev_hours: f64,
        prev_task_id: Option<i64>,
    },
}

/// The journal list is capped at this many rows; the UI shows a hint when
/// the cap is hit so old entries don't look deleted.
pub const JOURNAL_LIMIT: usize = 500;

const SELECT: &str = "SELECT e.id, e.work_date, e.project_id, e.description, e.hours,
        e.is_dev, e.created_at, p.code, p.name, e.task_id
 FROM log_entries e JOIN projects p ON p.id = e.project_id";

fn row_to_entry(row: &Row) -> rusqlite::Result<LogEntryRow> {
    let work_date: String = row.get(1)?;
    let created_at: String = row.get(6)?;
    Ok(LogEntryRow {
        entry: LogEntry {
            id: row.get(0)?,
            work_date: work_date.parse().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
            })?,
            project_id: row.get(2)?,
            description: row.get(3)?,
            hours: row.get(4)?,
            is_dev: row.get(5)?,
            created_at: created_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            task_id: row.get(9)?,
        },
        project_code: row.get(7)?,
        project_name: row.get(8)?,
    })
}

impl Db {
    pub fn insert_log_entry(
        &self,
        work_date: NaiveDate,
        project_id: i64,
        description: &str,
        hours: f64,
        is_dev: bool,
        task_id: Option<i64>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO log_entries (work_date, project_id, description, hours, is_dev, created_at, task_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                work_date.to_string(),
                project_id,
                description,
                hours,
                is_dev,
                Utc::now().to_rfc3339(),
                task_id,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Save an entry, folding it into an existing one when the same work was
    /// already logged that day: same date, project and dev flag, and the same
    /// description ignoring case and surrounding whitespace. Hours add up; the
    /// first entry's description casing and `created_at` (the contemporaneity
    /// stamp) are kept.
    pub fn merge_or_insert_log_entry(
        &self,
        work_date: NaiveDate,
        project_id: i64,
        description: &str,
        hours: f64,
        is_dev: bool,
        task_id: Option<i64>,
    ) -> Result<SavedEntry> {
        let wanted = description.trim().to_lowercase();
        // Case folding is done in Rust, not SQL: SQLite's lower() is
        // ASCII-only, and descriptions may carry accents.
        let existing = {
            let mut stmt = self.conn.prepare(
                "SELECT id, hours, description FROM log_entries
                 WHERE work_date = ?1 AND project_id = ?2 AND is_dev = ?3
                 ORDER BY id",
            )?;
            stmt.query_map(
                rusqlite::params![work_date.to_string(), project_id, is_dev],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?, r.get::<_, String>(2)?)),
            )?
            .filter_map(|r| r.ok())
            .find(|(_, _, desc)| desc.trim().to_lowercase() == wanted)
        };
        match existing {
            Some((id, old_hours, _)) => {
                let prev_task_id: Option<i64> = self.conn.query_row(
                    "SELECT task_id FROM log_entries WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )?;
                let total_hours = old_hours + hours;
                self.conn.execute(
                    "UPDATE log_entries SET hours = ?2, task_id = COALESCE(task_id, ?3)
                     WHERE id = ?1",
                    rusqlite::params![id, total_hours, task_id],
                )?;
                Ok(SavedEntry::Merged {
                    id,
                    total_hours,
                    prev_hours: old_hours,
                    prev_task_id,
                })
            }
            None => self
                .insert_log_entry(work_date, project_id, description, hours, is_dev, task_id)
                .map(SavedEntry::Inserted),
        }
    }

    /// Undo a merge: put back the hours and task link the entry had before
    /// the merge folded new time into it.
    pub fn unmerge_log_entry(&self, id: i64, hours: f64, task_id: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE log_entries SET hours = ?2, task_id = ?3 WHERE id = ?1",
            rusqlite::params![id, hours, task_id],
        )?;
        Ok(())
    }

    /// One-time cleanup for databases from before merge-aware logging: fold
    /// duplicates (same date, project, dev flag and case-insensitive
    /// description) into the earliest entry of each group, summing hours and
    /// keeping the first task link found. Returns how many rows were folded
    /// away. Runs in a transaction so a crash can't double-count hours.
    pub fn merge_duplicate_log_entries(&self) -> Result<usize> {
        // (work_date, project_id, is_dev, folded description) → (id, hours, task_id)
        type DupGroups = std::collections::HashMap<(String, i64, bool, String), Vec<(i64, f64, Option<i64>)>>;
        let mut groups = DupGroups::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT id, work_date, project_id, is_dev, description, hours, task_id
                 FROM log_entries ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, f64>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                ))
            })?;
            for row in rows {
                let (id, date, pid, dev, desc, hours, task_id) = row?;
                groups
                    .entry((date, pid, dev, desc.trim().to_lowercase()))
                    .or_default()
                    .push((id, hours, task_id));
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut folded = 0;
        for group in groups.values().filter(|g| g.len() > 1) {
            let keeper_id = group[0].0; // lowest id: first logged, casing + created_at kept
            let total: f64 = group.iter().map(|(_, h, _)| h).sum();
            let task_id = group.iter().find_map(|(_, _, t)| *t);
            tx.execute(
                "UPDATE log_entries SET hours = ?2, task_id = ?3 WHERE id = ?1",
                rusqlite::params![keeper_id, total, task_id],
            )?;
            for (dup_id, _, _) in &group[1..] {
                tx.execute("DELETE FROM log_entries WHERE id = ?1", [dup_id])?;
                folded += 1;
            }
        }
        tx.commit()?;
        Ok(folded)
    }

    /// Updates the work fields only — `created_at` is deliberately immutable.
    pub fn update_log_entry(
        &self,
        id: i64,
        work_date: NaiveDate,
        project_id: i64,
        description: &str,
        hours: f64,
        is_dev: bool,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE log_entries
             SET work_date = ?2, project_id = ?3, description = ?4, hours = ?5, is_dev = ?6
             WHERE id = ?1",
            rusqlite::params![id, work_date.to_string(), project_id, description, hours, is_dev],
        )?;
        Ok(())
    }

    /// Put a deleted entry back exactly as it was — original `created_at`
    /// (the immutable contemporaneity stamp) and task link, fresh id.
    pub fn reinsert_log_entry(&self, e: &LogEntry) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO log_entries (work_date, project_id, description, hours, is_dev, created_at, task_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                e.work_date.to_string(),
                e.project_id,
                e.description,
                e.hours,
                e.is_dev,
                e.created_at.to_rfc3339(),
                e.task_id,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_log_entry(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM log_entries WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Journal list: newest work first, optionally filtered by project and/or
    /// a minimum work_date. Every whitespace-separated word of `search` must
    /// appear in the description or project code, in any order.
    pub fn list_log_entries(
        &self,
        project_id: Option<i64>,
        since: Option<NaiveDate>,
        search: &str,
    ) -> Result<Vec<LogEntryRow>> {
        let mut sql = format!("{SELECT} WHERE 1=1");
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(pid) = project_id {
            params.push(Box::new(pid));
            sql.push_str(&format!(" AND e.project_id = ?{}", params.len()));
        }
        if let Some(d) = since {
            params.push(Box::new(d.to_string()));
            sql.push_str(&format!(" AND e.work_date >= ?{}", params.len()));
        }
        for term in search.split_whitespace() {
            let pattern = super::like_pattern(term);
            params.push(Box::new(pattern.clone()));
            sql.push_str(&format!(" AND (e.description LIKE ?{} ESCAPE '\\'", params.len()));
            params.push(Box::new(pattern));
            sql.push_str(&format!(" OR p.code LIKE ?{} ESCAPE '\\')", params.len()));
        }
        sql.push_str(&format!(
            " ORDER BY e.work_date DESC, e.id DESC LIMIT {JOURNAL_LIMIT}"
        ));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_entry)?;
        rows.collect()
    }

    /// Total hours logged on one day — the status bar's "did I log today?".
    pub fn hours_on(&self, date: NaiveDate) -> Result<f64> {
        self.conn.query_row(
            "SELECT COALESCE(SUM(hours), 0) FROM log_entries WHERE work_date = ?1",
            [date.to_string()],
            |r| r.get(0),
        )
    }

    /// All entries in [from, to] inclusive, ordered by project then date —
    /// the shape both reports want.
    pub fn log_entries_in_range(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        dev_only: bool,
    ) -> Result<Vec<LogEntryRow>> {
        let mut sql = format!(
            "{SELECT} WHERE e.work_date >= ?1 AND e.work_date <= ?2"
        );
        if dev_only {
            sql.push_str(" AND e.is_dev = 1");
        }
        sql.push_str(" ORDER BY p.code, e.work_date, e.id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![from.to_string(), to.to_string()],
            row_to_entry,
        )?;
        rows.collect()
    }
}
