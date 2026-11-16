use chrono::NaiveDate;
use rusqlite::Row;

use super::{Db, Result};

/// What kind of day off — each has its calendar colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DayOffKind {
    Holiday,
    Vacation,
    Sick,
    Other,
}

impl DayOffKind {
    pub const ALL: [DayOffKind; 4] = [
        DayOffKind::Holiday,
        DayOffKind::Vacation,
        DayOffKind::Sick,
        DayOffKind::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DayOffKind::Holiday => "holiday",
            DayOffKind::Vacation => "vacation",
            DayOffKind::Sick => "sick",
            DayOffKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> DayOffKind {
        match s {
            "holiday" => DayOffKind::Holiday,
            "vacation" => DayOffKind::Vacation,
            "sick" => DayOffKind::Sick,
            _ => DayOffKind::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DayOffKind::Holiday => "Holiday",
            DayOffKind::Vacation => "Vacation",
            DayOffKind::Sick => "Sick",
            DayOffKind::Other => "Other",
        }
    }
}

/// One day (or half day) off. One row per date.
#[derive(Debug, Clone, PartialEq)]
pub struct DayOff {
    pub id: i64,
    pub date: NaiveDate,
    pub kind: DayOffKind,
    pub label: Option<String>,
    pub half: bool,
    /// 'manual' or 'import:qc'.
    pub source: String,
}

impl DayOff {
    /// "Vacation (long weekend)", "Holiday ½ (Christmas Day)".
    pub fn describe(&self) -> String {
        let mut s = self.kind.label().to_string();
        if self.half {
            s.push_str(" ½");
        }
        if let Some(l) = self.label.as_deref().filter(|l| !l.trim().is_empty()) {
            s.push_str(&format!(" ({})", l.trim()));
        }
        s
    }

    /// The label if there is one, else the kind ("Christmas Day", "Vacation").
    pub fn title(&self) -> String {
        self.label
            .as_deref()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|| self.kind.label().to_string())
    }

    fn from_row(row: &Row) -> rusqlite::Result<DayOff> {
        let date: String = row.get(1)?;
        Ok(DayOff {
            id: row.get(0)?,
            date: NaiveDate::parse_from_str(&date, "%Y-%m-%d").unwrap_or_default(),
            kind: DayOffKind::parse(&row.get::<_, String>(2)?),
            label: row.get(3)?,
            half: row.get::<_, i64>(4)? != 0,
            source: row.get(5)?,
        })
    }
}

const SELECT: &str = "SELECT id, date, kind, label, half, source FROM days_off";

impl Db {
    /// Days off in [from, to] inclusive, by date.
    pub fn days_off_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<DayOff>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SELECT} WHERE date >= ?1 AND date <= ?2 ORDER BY date"))?;
        let rows = stmt.query_map(
            rusqlite::params![from.to_string(), to.to_string()],
            DayOff::from_row,
        )?;
        rows.collect()
    }

    /// The next `limit` days off from `from` on.
    pub fn upcoming_days_off(&self, from: NaiveDate, limit: usize) -> Result<Vec<DayOff>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SELECT} WHERE date >= ?1 ORDER BY date LIMIT ?2"))?;
        let rows = stmt.query_map(
            rusqlite::params![from.to_string(), limit as i64],
            DayOff::from_row,
        )?;
        rows.collect()
    }

    /// Save one row per date in one transaction. `overwrite` replaces an
    /// existing day off on that date; otherwise those dates are skipped.
    /// Returns how many rows were written.
    pub fn save_days_off(
        &self,
        dates: &[NaiveDate],
        kind: DayOffKind,
        label: Option<&str>,
        half: bool,
        source: &str,
        overwrite: bool,
    ) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let conflict = if overwrite {
            "ON CONFLICT(date) DO UPDATE SET kind = excluded.kind, label = excluded.label,
                 half = excluded.half, source = excluded.source"
        } else {
            "ON CONFLICT(date) DO NOTHING"
        };
        let sql = format!(
            "INSERT INTO days_off (date, kind, label, half, source) VALUES (?1, ?2, ?3, ?4, ?5) {conflict}"
        );
        let mut written = 0;
        for d in dates {
            written += tx.execute(
                &sql,
                rusqlite::params![d.to_string(), kind.as_str(), label, half as i64, source],
            )?;
        }
        tx.commit()?;
        Ok(written)
    }

    pub fn delete_day_off(&self, date: NaiveDate) -> Result<()> {
        self.conn
            .execute("DELETE FROM days_off WHERE date = ?1", [date.to_string()])?;
        Ok(())
    }

    /// Hours per (day, project) in [from, to] — one query for a calendar month.
    pub fn hours_by_day_project(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, i64, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.work_date, e.project_id, SUM(e.hours)
             FROM log_entries e JOIN projects p ON p.id = e.project_id
             WHERE e.work_date >= ?1 AND e.work_date <= ?2
             GROUP BY e.work_date, e.project_id
             ORDER BY e.work_date, p.report_order IS NULL, p.report_order, p.code",
        )?;
        let rows = stmt.query_map(rusqlite::params![from.to_string(), to.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, f64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (d, p, h) = row?;
            if let Ok(d) = NaiveDate::parse_from_str(&d, "%Y-%m-%d") {
                out.push((d, p, h));
            }
        }
        Ok(out)
    }
}
