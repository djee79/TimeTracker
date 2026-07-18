pub mod pdf;

use chrono::{Datelike, NaiveDate};

use crate::db::{Db, LogEntryRow};

/// One project's slice of a report: total hours plus every entry, in
/// chronological order.
pub struct ProjectGroup {
    pub code: String,
    pub name: String,
    pub total_hours: f64,
    pub entries: Vec<LogEntryRow>,
}

pub struct WeeklyReport {
    pub week_start: NaiveDate, // Monday
    pub groups: Vec<ProjectGroup>,
    pub total_hours: f64,
}

/// Monday of the week containing `date`.
pub fn week_start_of(date: NaiveDate) -> NaiveDate {
    date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// Format hours without trailing noise: 8, 1.5, 0.25.
pub fn fmt_hours(h: f64) -> String {
    let s = format!("{h:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Human-readable tracked duration: "45 min", "2 h 05".
pub fn fmt_duration(secs: i64) -> String {
    let mins = (secs + 30) / 60;
    match (mins / 60, mins % 60) {
        (0, 0) => "less than a minute".to_string(),
        (0, m) => format!("{m} min"),
        (h, m) => format!("{h} h {m:02}"),
    }
}

fn group_by_project(rows: Vec<LogEntryRow>) -> (Vec<ProjectGroup>, f64) {
    let mut groups: Vec<ProjectGroup> = Vec::new();
    let mut total = 0.0;
    for row in rows {
        total += row.entry.hours;
        match groups.last_mut() {
            Some(g) if g.code == row.project_code => {
                g.total_hours += row.entry.hours;
                g.entries.push(row);
            }
            _ => groups.push(ProjectGroup {
                code: row.project_code.clone(),
                name: row.project_name.clone(),
                total_hours: row.entry.hours,
                entries: vec![row],
            }),
        }
    }
    (groups, total)
}

pub fn weekly(db: &Db, week_start: NaiveDate) -> Result<WeeklyReport, rusqlite::Error> {
    let week_end = week_start + chrono::Duration::days(6);
    let rows = db.log_entries_in_range(week_start, week_end, false)?;
    let (groups, total_hours) = group_by_project(rows);
    Ok(WeeklyReport {
        week_start,
        groups,
        total_hours,
    })
}

impl WeeklyReport {
    /// Plain text meant to be pasted straight into a timesheet.
    pub fn to_text(&self) -> String {
        let week_end = self.week_start + chrono::Duration::days(6);
        let mut out = format!(
            "Week of {} to {}  —  total {} h\n",
            self.week_start, week_end,
            fmt_hours(self.total_hours)
        );
        if self.groups.is_empty() {
            out.push_str("\n(no entries this week)\n");
            return out;
        }
        for g in &self.groups {
            out.push_str(&format!(
                "\n{} — {}  ({} h)\n",
                g.code,
                g.name,
                fmt_hours(g.total_hours)
            ));
            for row in &g.entries {
                out.push_str(&format!(
                    "  - {} ({} h): {}\n",
                    row.entry.work_date.format("%a %m-%d"),
                    fmt_hours(row.entry.hours),
                    row.entry.description
                ));
            }
        }
        out
    }

    pub fn to_csv(&self) -> String {
        let mut out = String::from("date,project_code,project_name,hours,is_dev,description\n");
        for g in &self.groups {
            for row in &g.entries {
                out.push_str(&csv_row(&[
                    &row.entry.work_date.to_string(),
                    &g.code,
                    &g.name,
                    &fmt_hours(row.entry.hours),
                    if row.entry.is_dev { "yes" } else { "no" },
                    &row.entry.description,
                ]));
            }
        }
        out
    }
}

/// The weekly report re-shaped as a projects × days grid — the shape of an
/// actual timesheet. Every cell knows its descriptions so the UI can offer
/// click-to-copy at cell, row (project) and column (day) granularity.
pub struct WeekGrid {
    pub week_start: NaiveDate,
    pub rows: Vec<GridRow>,
    pub day_totals: [f64; 7],
    pub total: f64,
}

pub struct GridRow {
    pub code: String,
    pub name: String,
    pub days: [DayCell; 7],
    pub total: f64,
}

#[derive(Default, Clone)]
pub struct DayCell {
    pub hours: f64,
    pub descriptions: Vec<String>,
}

impl DayCell {
    /// What lands in the clipboard for one project-day: just the prose,
    /// ready for a timesheet's comment field.
    pub fn copy_text(&self) -> String {
        self.descriptions.join("; ")
    }
}

impl GridRow {
    /// One project's whole week, day by day.
    pub fn copy_text(&self, week_start: NaiveDate) -> String {
        let mut out = String::new();
        for (i, cell) in self.days.iter().enumerate() {
            if cell.hours > 0.0 {
                let date = week_start + chrono::Duration::days(i as i64);
                out.push_str(&format!(
                    "{} ({} h): {}\n",
                    date.format("%a %m-%d"),
                    fmt_hours(cell.hours),
                    cell.copy_text()
                ));
            }
        }
        out
    }
}

impl WeekGrid {
    pub fn from_report(rep: &WeeklyReport) -> WeekGrid {
        let mut rows = Vec::new();
        let mut day_totals = [0.0; 7];
        for g in &rep.groups {
            let mut days: [DayCell; 7] = Default::default();
            for row in &g.entries {
                let idx = (row.entry.work_date - rep.week_start).num_days();
                if (0..7).contains(&idx) {
                    let cell = &mut days[idx as usize];
                    cell.hours += row.entry.hours;
                    cell.descriptions.push(row.entry.description.clone());
                    day_totals[idx as usize] += row.entry.hours;
                }
            }
            rows.push(GridRow {
                code: g.code.clone(),
                name: g.name.clone(),
                days,
                total: g.total_hours,
            });
        }
        WeekGrid {
            week_start: rep.week_start,
            rows,
            day_totals,
            total: rep.total_hours,
        }
    }

    /// One day across all projects — for filling a timesheet day by day.
    pub fn day_copy_text(&self, day: usize) -> String {
        let mut out = String::new();
        for row in &self.rows {
            let cell = &row.days[day];
            if cell.hours > 0.0 {
                out.push_str(&format!(
                    "{} ({} h): {}\n",
                    row.code,
                    fmt_hours(cell.hours),
                    cell.copy_text()
                ));
            }
        }
        out
    }
}

/// Annual dev export: `is_dev` entries for `year`, grouped by project,
/// chronological within each project. Columns match the manual extraction
/// previously done by hand for the SR&ED claim.
pub fn annual_dev_csv(db: &Db, year: i32) -> Result<(String, usize), rusqlite::Error> {
    let from = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year start");
    let to = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year end");
    let rows = db.log_entries_in_range(from, to, true)?;
    let count = rows.len();
    let mut out = String::from("date,project_code,project_name,description,hours\n");
    for row in &rows {
        out.push_str(&csv_row(&[
            &row.entry.work_date.to_string(),
            &row.project_code,
            &row.project_name,
            &row.entry.description,
            &fmt_hours(row.entry.hours),
        ]));
    }
    Ok((out, count))
}

/// How many dev entries exist for `year` (shown next to the export button).
pub fn annual_dev_count(db: &Db, year: i32) -> usize {
    let from = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year start");
    let to = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year end");
    db.log_entries_in_range(from, to, true)
        .map(|r| r.len())
        .unwrap_or(0)
}

fn csv_row(fields: &[&str]) -> String {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if f.contains(',') || f.contains('"') || f.contains('\n') {
            out.push('"');
            out.push_str(&f.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(f);
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn week_start_snaps_to_monday() {
        assert_eq!(week_start_of(d("2026-07-03")), d("2026-06-29")); // Fri -> Mon
        assert_eq!(week_start_of(d("2026-06-29")), d("2026-06-29")); // Mon stays
        assert_eq!(week_start_of(d("2026-07-05")), d("2026-06-29")); // Sun stays in week
    }

    #[test]
    fn hours_format_trims() {
        assert_eq!(fmt_hours(8.0), "8");
        assert_eq!(fmt_hours(1.5), "1.5");
        assert_eq!(fmt_hours(0.25), "0.25");
        assert_eq!(fmt_hours(2.10), "2.1");
    }

    fn seeded_db() -> Db {
        let db = Db::open_in_memory().unwrap();
        let p1 = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let p2 = db.insert_project("BBB-002", "Beta", None).unwrap();
        db.insert_log_entry(d("2026-06-29"), p1, "wired the panel", 3.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-01"), p1, "PLC logic, v2", 2.5, true, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-02"), p2, "site \"visit\"", 4.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-07-10"), p1, "next week, excluded", 1.0, true, None)
            .unwrap();
        db
    }

    #[test]
    fn weekly_groups_and_totals() {
        let db = seeded_db();
        let rep = weekly(&db, d("2026-06-29")).unwrap();
        assert_eq!(rep.groups.len(), 2);
        assert_eq!(rep.total_hours, 9.5);
        assert_eq!(rep.groups[0].code, "AAA-001");
        assert_eq!(rep.groups[0].total_hours, 5.5);
        assert_eq!(rep.groups[0].entries[0].entry.description, "wired the panel");

        let text = rep.to_text();
        assert!(text.contains("total 9.5 h"));
        assert!(text.contains("AAA-001 — Alpha  (5.5 h)"));
        assert!(!text.contains("excluded"));
    }

    #[test]
    fn weekly_csv_escapes() {
        let db = seeded_db();
        let csv = weekly(&db, d("2026-06-29")).unwrap().to_csv();
        // comma in description gets quoted, quotes get doubled
        assert!(csv.contains("\"PLC logic, v2\""));
        assert!(csv.contains("\"site \"\"visit\"\"\""));
        assert!(csv.lines().next().unwrap().starts_with("date,project_code"));
    }

    #[test]
    fn annual_export_dev_only() {
        let db = seeded_db();
        let (csv, count) = annual_dev_csv(&db, 2026).unwrap();
        assert_eq!(count, 2);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
        assert_eq!(lines[0], "date,project_code,project_name,description,hours");
        assert!(lines[1].starts_with("2026-07-01,AAA-001"));
        assert!(lines[2].starts_with("2026-07-10,AAA-001"));
        assert_eq!(annual_dev_count(&db, 2025), 0);
    }

    #[test]
    fn week_grid_cells_and_totals() {
        let db = seeded_db();
        let rep = weekly(&db, d("2026-06-29")).unwrap();
        let grid = WeekGrid::from_report(&rep);

        assert_eq!(grid.rows.len(), 2);
        assert_eq!(grid.total, 9.5);
        // Mon: 3h Alpha; Wed: 2.5h Alpha; Thu: 4h Beta
        assert_eq!(grid.day_totals[0], 3.0);
        assert_eq!(grid.day_totals[2], 2.5);
        assert_eq!(grid.day_totals[3], 4.0);
        assert_eq!(grid.day_totals[5], 0.0);

        let alpha = &grid.rows[0];
        assert_eq!(alpha.code, "AAA-001");
        assert_eq!(alpha.days[0].copy_text(), "wired the panel");
        assert!(alpha.days[1].descriptions.is_empty());
        assert_eq!(alpha.total, 5.5);
        let week_text = alpha.copy_text(grid.week_start);
        assert!(week_text.contains("Mon 06-29 (3 h): wired the panel"));
        assert!(week_text.contains("Wed 07-01 (2.5 h): PLC logic, v2"));

        // whole-Wednesday copy pulls from all projects with hours that day
        let wed = grid.day_copy_text(2);
        assert_eq!(wed, "AAA-001 (2.5 h): PLC logic, v2\n");
    }

    #[test]
    fn week_grid_merges_same_day_entries() {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        db.insert_log_entry(d("2026-06-29"), p, "morning fix", 1.0, false, None)
            .unwrap();
        db.insert_log_entry(d("2026-06-29"), p, "afternoon docs", 2.0, false, None)
            .unwrap();
        let grid = WeekGrid::from_report(&weekly(&db, d("2026-06-29")).unwrap());
        assert_eq!(grid.rows[0].days[0].hours, 3.0);
        assert_eq!(grid.rows[0].days[0].copy_text(), "morning fix; afternoon docs");
    }

    #[test]
    fn empty_week_still_renders() {
        let db = Db::open_in_memory().unwrap();
        let rep = weekly(&db, d("2026-06-29")).unwrap();
        assert!(rep.to_text().contains("no entries"));
    }
}
