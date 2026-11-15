use rusqlite::Row;

use super::{Db, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStatus {
    Active,
    Archived,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::Archived => "archived",
        }
    }

    fn parse(s: &str) -> ProjectStatus {
        match s {
            "archived" => ProjectStatus::Archived,
            _ => ProjectStatus::Active,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub client: Option<String>,
    pub status: ProjectStatus,
}

impl Project {
    fn from_row(row: &Row) -> rusqlite::Result<Project> {
        Ok(Project {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
            client: row.get(3)?,
            status: ProjectStatus::parse(&row.get::<_, String>(4)?),
        })
    }
}

impl Db {
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, code, name, client, status FROM projects
             ORDER BY status = 'archived', code",
        )?;
        let rows = stmt.query_map([], Project::from_row)?;
        rows.collect()
    }

    pub fn insert_project(&self, code: &str, name: &str, client: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO projects (code, name, client, report_order)
             VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(report_order), -1) + 1 FROM projects))",
            rusqlite::params![code, name, client],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_project(
        &self,
        id: i64,
        code: &str,
        name: &str,
        client: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE projects SET code = ?2, name = ?3, client = ?4 WHERE id = ?1",
            rusqlite::params![id, code, name, client],
        )?;
        Ok(())
    }

    pub fn set_project_status(&self, id: i64, status: ProjectStatus) -> Result<()> {
        self.conn.execute(
            "UPDATE projects SET status = ?2 WHERE id = ?1",
            rusqlite::params![id, status.as_str()],
        )?;
        Ok(())
    }

    /// Every project id in report order (then code) — archived ones too,
    /// so they keep their slot.
    pub fn project_order(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM projects ORDER BY report_order IS NULL, report_order, code")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// Rewrite the report order from `ids` (first = top), in one transaction.
    pub fn save_project_order(&self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE projects SET report_order = ?2 WHERE id = ?1",
                rusqlite::params![id, i as i64],
            )?;
        }
        tx.commit()
    }

    /// Back to alphabetical by code.
    pub fn reset_project_order(&self) -> Result<()> {
        self.conn.execute_batch(
            "UPDATE projects SET report_order =
                (SELECT COUNT(*) FROM projects p2 WHERE p2.code < projects.code);",
        )
    }
}
