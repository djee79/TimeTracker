//! PDF exports: the weekly timesheet and the annual R&D report, each with
//! an optional rendered-markdown appendix (the task notes) under entries
//! that were logged from a task.
//!
//! Fonts are embedded (Liberation Sans, SIL OFL — see assets/fonts/LICENSE)
//! so the binary stays self-contained and the PDFs open anywhere.

use chrono::NaiveDate;
use genpdf::style::{Color, Style};
use genpdf::{elements, fonts, Document, Element as _, Margins};

use super::fmt_hours;
use crate::db::Db;

const FONT_REGULAR: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Bold.ttf");
const FONT_ITALIC: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Italic.ttf");
const FONT_BOLD_ITALIC: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-BoldItalic.ttf");

const GRAY: Color = Color::Rgb(110, 110, 110);
const CODE_GRAY: Color = Color::Rgb(70, 70, 90);

fn document(title: &str) -> Result<Document, String> {
    let load = |data: &[u8]| {
        fonts::FontData::new(data.to_vec(), None).map_err(|e| format!("loading font: {e}"))
    };
    let family = fonts::FontFamily {
        regular: load(FONT_REGULAR)?,
        bold: load(FONT_BOLD)?,
        italic: load(FONT_ITALIC)?,
        bold_italic: load(FONT_BOLD_ITALIC)?,
    };
    let mut doc = Document::new(family);
    doc.set_title(title);
    doc.set_paper_size(genpdf::PaperSize::Letter);
    doc.set_line_spacing(1.15);
    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(Margins::trbl(15, 15, 18, 15));
    // page number, top right, from page 2 on
    decorator.set_header(|page| {
        let mut p = elements::Paragraph::default();
        if page > 1 {
            p.push_styled(
                format!("page {page}"),
                Style::new().with_color(GRAY).with_font_size(8),
            );
            p.set_alignment(genpdf::Alignment::Right);
        }
        p.padded(Margins::trbl(0.0, 0.0, 3.0, 0.0))
    });
    doc.set_page_decorator(decorator);
    Ok(doc)
}

/// "Prepared by … — generated …" line under the subtitle; the name comes
/// from the `report_author` setting (Projects window → App settings).
fn byline(db: &Db) -> String {
    let today = chrono::Local::now().date_naive();
    match db.setting("report_author").filter(|a| !a.trim().is_empty()) {
        Some(author) => format!("Prepared by {} — generated {today}", author.trim()),
        None => format!("Generated {today}"),
    }
}

/// Title block shared by the reports.
fn push_header(doc: &mut Document, title: &str, subtitle: &str, byline: &str) {
    let mut p = elements::Paragraph::default();
    p.push_styled(title, Style::new().bold().with_font_size(16));
    doc.push(p);
    let mut p = elements::Paragraph::default();
    p.push_styled(subtitle, Style::new().with_color(GRAY).with_font_size(10));
    doc.push(p);
    let mut p = elements::Paragraph::default();
    p.push_styled(byline, Style::new().with_color(GRAY).with_font_size(9));
    doc.push(p);
    doc.push(elements::Break::new(1.0));
}

/// One project's section header: "CODE — Name  (total h)".
fn push_group_header(doc: &mut Document, code: &str, name: &str, hours: f64) {
    doc.push(elements::Break::new(0.6));
    let mut p = elements::Paragraph::default();
    p.push_styled(format!("{code} — {name}"), Style::new().bold().with_font_size(12));
    p.push_styled(
        format!("   {} h", fmt_hours(hours)),
        Style::new().with_color(GRAY).with_font_size(10),
    );
    doc.push(p);
    doc.push(elements::Break::new(0.2));
}

/// One entry line: bold date, gray hours, then the description.
fn push_entry(doc: &mut Document, date: &str, hours: f64, description: &str) {
    let mut p = elements::Paragraph::default();
    p.push_styled(format!("{date}   "), Style::new().bold().with_font_size(10));
    p.push_styled(
        format!("{} h — ", fmt_hours(hours)),
        Style::new().with_color(GRAY).with_font_size(10),
    );
    p.push_styled(description, Style::new().with_font_size(10));
    doc.push(p.padded(Margins::trbl(0.0, 0.0, 0.5, 2.0)));
}

/// The task notes under an entry: rendered markdown, indented and boxed off
/// with a left padding so it reads as an appendix to the line above.
fn push_notes(doc: &mut Document, notes: &str) {
    doc.push(markdown(notes).padded(Margins::trbl(0.5, 0.0, 1.5, 8.0)));
}

/// Push the linked task's notes under an entry, when wanted and present.
/// A task logged several times gets its notes printed once — repeats get a
/// small pointer instead of the same block again.
fn maybe_push_notes(
    doc: &mut Document,
    db: &Db,
    include_notes: bool,
    task_id: Option<i64>,
    seen: &mut std::collections::HashSet<i64>,
) {
    if !include_notes {
        return;
    }
    let Some(id) = task_id else { return };
    let Some(task) = db.task(id).ok().flatten() else { return };
    let details = task.details.trim();
    if details.is_empty() {
        return;
    }
    if !seen.insert(id) {
        let mut p = elements::Paragraph::default();
        p.push_styled(
            "(task notes shown with the first entry above)",
            Style::new().with_color(GRAY).italic().with_font_size(9),
        );
        doc.push(p.padded(Margins::trbl(0.0, 0.0, 1.0, 8.0)));
        return;
    }
    push_notes(doc, details);
}

/// Timesheet for one week: entries grouped by project, chronological.
pub fn weekly_pdf(db: &Db, week_start: NaiveDate, include_notes: bool) -> Result<Document, String> {
    let rep = super::weekly(db, week_start).map_err(|e| e.to_string())?;
    let week_end = week_start + chrono::Duration::days(6);
    let mut doc = document(&format!("Timesheet — week of {week_start}"))?;
    push_header(
        &mut doc,
        "Weekly timesheet",
        &format!(
            "Mon {week_start} – Sun {week_end}   —   total {} h",
            fmt_hours(rep.total_hours)
        ),
        &byline(db),
    );
    if rep.groups.is_empty() {
        let mut p = elements::Paragraph::default();
        p.push_styled("No entries this week.", Style::new().with_color(GRAY));
        doc.push(p);
        return Ok(doc);
    }
    let mut seen = std::collections::HashSet::new();
    for g in &rep.groups {
        push_group_header(&mut doc, &g.code, &g.name, g.total_hours);
        for row in &g.entries {
            push_entry(
                &mut doc,
                &row.entry.work_date.format("%a %m-%d").to_string(),
                row.entry.hours,
                &row.entry.description,
            );
            maybe_push_notes(&mut doc, db, include_notes, row.entry.task_id, &mut seen);
        }
    }
    Ok(doc)
}

/// One month's entries grouped by project, chronological.
pub fn monthly_pdf(db: &Db, month_start: NaiveDate, include_notes: bool) -> Result<Document, String> {
    let rep = super::monthly(db, month_start).map_err(|e| e.to_string())?;
    let label = month_start.format("%B %Y").to_string();
    let mut doc = document(&format!("Timesheet — {label}"))?;
    push_header(
        &mut doc,
        "Monthly summary",
        &format!("{label}   —   total {} h", fmt_hours(rep.total_hours)),
        &byline(db),
    );
    if rep.groups.is_empty() {
        let mut p = elements::Paragraph::default();
        p.push_styled("No entries this month.", Style::new().with_color(GRAY));
        doc.push(p);
        return Ok(doc);
    }
    let mut seen = std::collections::HashSet::new();
    for g in &rep.groups {
        push_group_header(&mut doc, &g.code, &g.name, g.total_hours);
        for row in &g.entries {
            push_entry(
                &mut doc,
                &row.entry.work_date.format("%a %m-%d").to_string(),
                row.entry.hours,
                &row.entry.description,
            );
            maybe_push_notes(&mut doc, db, include_notes, row.entry.task_id, &mut seen);
        }
    }
    Ok(doc)
}

/// Annual R&D report: every `dev` entry of the year, grouped by project,
/// chronological — the SR&ED claim's supporting document.
pub fn annual_pdf(db: &Db, year: i32, include_notes: bool) -> Result<Document, String> {
    let from = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year start");
    let to = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year end");
    let rows = db.log_entries_in_range(from, to, true).map_err(|e| e.to_string())?;
    let count = rows.len();
    let (groups, total) = super::group_by_project(rows);
    let mut doc = document(&format!("R&D report {year}"))?;
    push_header(
        &mut doc,
        &format!("Annual R&D report — {year}"),
        &format!(
            "{count} entries flagged dev (R&D), grouped by project, chronological   —   total {} h",
            fmt_hours(total)
        ),
        &byline(db),
    );
    let mut seen = std::collections::HashSet::new();
    for g in &groups {
        push_group_header(&mut doc, &g.code, &g.name, g.total_hours);
        for row in &g.entries {
            push_entry(
                &mut doc,
                &row.entry.work_date.to_string(),
                row.entry.hours,
                &row.entry.description,
            );
            maybe_push_notes(&mut doc, db, include_notes, row.entry.task_id, &mut seen);
        }
    }
    Ok(doc)
}

// ---- markdown → genpdf elements ----

/// Renders a markdown string into a vertical layout: headings, bold/italic,
/// bullet/numbered/task lists, quotes, code blocks, tables, rules. Kept in
/// sync with what the in-app preview (pulldown-cmark) supports.
fn markdown(text: &str) -> elements::LinearLayout {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let mut md = MdPdf::new();
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                md.flush(0.2);
                md.size = match level as usize {
                    1 => 14,
                    2 => 12,
                    _ => 11,
                };
                md.heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                md.flush(0.4);
                md.size = MdPdf::BASE_SIZE;
                md.heading = false;
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => md.flush(0.5),
            Event::Start(Tag::Strong) => md.bold += 1,
            Event::End(TagEnd::Strong) => md.bold -= 1,
            Event::Start(Tag::Emphasis) => md.italic += 1,
            Event::End(TagEnd::Emphasis) => md.italic -= 1,
            Event::Start(Tag::BlockQuote(..)) => {
                md.flush(0.2);
                md.quote += 1;
            }
            Event::End(TagEnd::BlockQuote(..)) => {
                md.flush(0.4);
                md.quote -= 1;
            }
            Event::Start(Tag::List(start)) => {
                md.flush(0.1);
                md.lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                md.lists.pop();
                if md.lists.is_empty() {
                    md.spacer(0.4);
                }
            }
            Event::Start(Tag::Item) => {
                md.prefix = Some(match md.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => "•  ".to_string(),
                });
            }
            Event::End(TagEnd::Item) => md.flush(0.1),
            Event::TaskListMarker(checked) => {
                md.prefix = Some(if checked { "[x] ".into() } else { "[  ] ".into() });
            }
            Event::Start(Tag::CodeBlock(_)) => {
                md.flush(0.2);
                md.code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                md.code_block = false;
                md.spacer(0.4);
            }
            Event::Start(Tag::Table(alignments)) => {
                md.flush(0.2);
                md.table = Some(elements::TableLayout::new(vec![1; alignments.len().max(1)]));
            }
            Event::End(TagEnd::Table) => md.finish_table(),
            Event::Start(Tag::TableHead) => md.in_table_head = true,
            Event::End(TagEnd::TableHead) => {
                md.in_table_head = false;
                md.finish_row();
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => md.finish_row(),
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => md.finish_cell(),
            Event::Text(t) => md.text(&t),
            Event::Code(t) => {
                let style = md.style().with_color(CODE_GRAY);
                md.take_prefix();
                md.para.push_styled(t.to_string(), style);
            }
            Event::SoftBreak => md.text(" "),
            Event::HardBreak => md.flush(0.0),
            Event::Rule => {
                md.flush(0.2);
                let mut p = elements::Paragraph::default();
                p.push_styled("―".repeat(30), Style::new().with_color(GRAY));
                md.out.push(p);
                md.spacer(0.2);
            }
            _ => {}
        }
    }
    md.flush(0.0);
    md.out
}

struct MdPdf {
    out: elements::LinearLayout,
    para: elements::Paragraph,
    para_used: bool,
    bold: usize,
    italic: usize,
    quote: usize,
    heading: bool,
    size: u8,
    lists: Vec<Option<u64>>,
    prefix: Option<String>,
    code_block: bool,
    table: Option<elements::TableLayout>,
    in_table_head: bool,
    cells: Vec<elements::Paragraph>,
}

impl MdPdf {
    const BASE_SIZE: u8 = 10;

    fn new() -> MdPdf {
        MdPdf {
            out: elements::LinearLayout::vertical(),
            para: elements::Paragraph::default(),
            para_used: false,
            bold: 0,
            italic: 0,
            quote: 0,
            heading: false,
            size: Self::BASE_SIZE,
            lists: Vec::new(),
            prefix: None,
            code_block: false,
            table: None,
            in_table_head: false,
            cells: Vec::new(),
        }
    }

    fn style(&self) -> Style {
        let mut s = Style::new().with_font_size(self.size);
        if self.bold > 0 || self.heading || self.in_table_head {
            s = s.bold();
        }
        if self.italic > 0 || self.quote > 0 {
            s = s.italic();
        }
        if self.quote > 0 {
            s = s.with_color(GRAY);
        }
        s
    }

    fn take_prefix(&mut self) {
        if let Some(p) = self.prefix.take() {
            self.para
                .push_styled(p, Style::new().with_font_size(self.size));
            self.para_used = true;
        }
    }

    fn text(&mut self, t: &str) {
        if self.code_block {
            // one gray paragraph per line, monospace-ish appendix look
            for line in t.trim_end_matches('\n').split('\n') {
                let mut p = elements::Paragraph::default();
                p.push_styled(
                    line.to_string(),
                    Style::new().with_color(CODE_GRAY).with_font_size(9),
                );
                self.out.push(p.padded(Margins::trbl(0.0, 0.0, 0.0, 4.0)));
            }
            return;
        }
        let style = self.style();
        self.take_prefix();
        self.para.push_styled(t.to_string(), style);
        self.para_used = true;
    }

    fn spacer(&mut self, lines: f64) {
        if lines > 0.0 {
            self.out.push(elements::Break::new(lines));
        }
    }

    /// Close the current paragraph, indented per list/quote depth.
    fn flush(&mut self, spacing_after: f64) {
        if !self.para_used {
            self.para = elements::Paragraph::default();
            return;
        }
        let para = std::mem::take(&mut self.para);
        self.para_used = false;
        let indent = (self.lists.len() as f64) * 5.0 + (self.quote as f64) * 5.0;
        if self.table.is_some() {
            self.cells.push(para);
            return;
        }
        self.out.push(para.padded(Margins::trbl(0.0, 0.0, 0.0, indent)));
        self.spacer(spacing_after);
    }

    fn finish_cell(&mut self) {
        if !self.para_used {
            // keep the table grid rectangular even for empty cells
            self.para.push_styled(" ", Style::new().with_font_size(self.size));
            self.para_used = true;
        }
        self.flush(0.0);
    }

    fn finish_row(&mut self) {
        let Some(table) = self.table.as_mut() else {
            self.cells.clear();
            return;
        };
        if self.cells.is_empty() {
            return;
        }
        let mut row = table.row();
        for cell in self.cells.drain(..) {
            row = row.element(cell.padded(Margins::trbl(0.5, 1.0, 0.5, 1.0)));
        }
        let _ = row.push();
    }

    fn finish_table(&mut self) {
        if let Some(mut table) = self.table.take() {
            table.set_cell_decorator(elements::FrameCellDecorator::new(true, true, false));
            self.out.push(table);
            self.spacer(0.5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Priority;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn render(doc: Document) -> Vec<u8> {
        let mut buf = Vec::new();
        doc.render(&mut buf).unwrap();
        buf
    }

    fn db_with_noted_task() -> Db {
        let db = Db::open_in_memory().unwrap();
        let p = db.insert_project("AAA-001", "Alpha", None).unwrap();
        let t = db.insert_task(p, "wired the panel", Priority::Normal).unwrap();
        db.set_task_details(
            t,
            "## Plan\nSome **bold** and *italic* and `code`.\n\n- step one\n- step two\n\n1. first\n2. second\n\n> a quote\n\n```\nlet x = 1;\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n---\ndone",
        )
        .unwrap();
        db.insert_log_entry(d("2026-06-29"), p, "wired the panel", 3.0, true, Some(t))
            .unwrap();
        db.insert_log_entry(d("2026-06-30"), p, "hand-typed, no task", 1.5, true, None)
            .unwrap();
        db
    }

    #[test]
    fn weekly_pdf_renders_with_notes() {
        let db = db_with_noted_task();
        let buf = render(weekly_pdf(&db, d("2026-06-29"), true).unwrap());
        assert!(buf.starts_with(b"%PDF"));
    }

    #[test]
    fn weekly_pdf_renders_without_notes() {
        let db = db_with_noted_task();
        let buf = render(weekly_pdf(&db, d("2026-06-29"), false).unwrap());
        assert!(buf.starts_with(b"%PDF"));
    }

    #[test]
    fn monthly_pdf_renders() {
        let db = db_with_noted_task();
        db.set_setting("report_author", "Test Author").unwrap();
        let buf = render(monthly_pdf(&db, d("2026-06-01"), true).unwrap());
        assert!(buf.starts_with(b"%PDF"));
    }

    #[test]
    fn annual_pdf_renders() {
        let db = db_with_noted_task();
        let buf = render(annual_pdf(&db, 2026, true).unwrap());
        assert!(buf.starts_with(b"%PDF"));
    }

    #[test]
    fn empty_week_pdf_renders() {
        let db = Db::open_in_memory().unwrap();
        let buf = render(weekly_pdf(&db, d("2026-06-29"), true).unwrap());
        assert!(buf.starts_with(b"%PDF"));
    }
}
