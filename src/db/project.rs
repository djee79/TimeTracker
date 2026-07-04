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
    /// "CODE — Name", the standard way a project is shown in pickers.
    pub fn label(&self) -> String {
        format!("{} — {}", self.code, self.name)
    }

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
            "INSERT INTO projects (code, name, client) VALUES (?1, ?2, ?3)",
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
}
