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
            let pattern = format!("%{term}%");
            params.push(Box::new(pattern.clone()));
            sql.push_str(&format!(" AND (e.description LIKE ?{}", params.len()));
            params.push(Box::new(pattern));
            sql.push_str(&format!(" OR p.code LIKE ?{})", params.len()));
        }
        sql.push_str(" ORDER BY e.work_date DESC, e.id DESC LIMIT 500");
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
