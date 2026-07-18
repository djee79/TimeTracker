use chrono::Datelike;

use crate::app::{today, WorklogApp};
use crate::report;
use crate::ui;

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            weekly_section(app, ui_);
            ui_.add_space(16.0);
            ui_.separator();
            ui_.add_space(8.0);
            monthly_section(app, ui_);
            ui_.add_space(16.0);
            ui_.separator();
            ui_.add_space(8.0);
            annual_section(app, ui_);
        });
}

fn monthly_section(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.heading("Monthly summary");
    ui_.add_space(4.0);
    ui_.horizontal(|ui_| {
        if ui_.button("⏴").on_hover_text("previous month").clicked() {
            app.month_start = app.month_start - chrono::Months::new(1);
        }
        ui_.label(
            egui::RichText::new(app.month_start.format("%B %Y").to_string()).strong(),
        );
        if ui_.button("⏵").on_hover_text("next month").clicked() {
            app.month_start = app.month_start + chrono::Months::new(1);
        }
        let this_month = report::month_start_of(today());
        if app.month_start != this_month && ui_.button("This month").clicked() {
            app.month_start = this_month;
        }
        let rep = report::monthly(&app.db, app.month_start);
        if let Ok(rep) = &rep {
            ui_.label(
                egui::RichText::new(format!("total {} h", report::fmt_hours(rep.total_hours)))
                    .weak(),
            );
        }
    });
    ui_.add_space(6.0);
    let Ok(rep) = report::monthly(&app.db, app.month_start) else {
        return;
    };
    ui_.horizontal(|ui_| {
        if ui_.button("Copy month").clicked() {
            ui_.ctx().copy_text(rep.to_text());
            app.set_status("Month copied to clipboard");
        }
        let stamp = app.month_start.format("%Y-%m").to_string();
        if ui_.button("Export CSV…").clicked() {
            app.export_csv(&format!("timesheet-{stamp}.csv"), &rep.to_csv());
        }
        if ui_.button("Export PDF…").clicked() {
            let doc = report::pdf::monthly_pdf(&app.db, app.month_start, app.pdf_include_notes);
            app.export_pdf(&format!("timesheet-{stamp}.pdf"), doc);
        }
        notes_option(app, ui_);
    });
    // per-project totals at a glance; details live in the preview/exports
    for g in &rep.groups {
        ui_.horizontal(|ui_| {
            ui_.label(egui::RichText::new(&g.code).monospace().strong());
            ui_.label(egui::RichText::new(format!("{} h", report::fmt_hours(g.total_hours))));
            ui_.label(egui::RichText::new(&g.name).weak());
        });
    }
    if rep.groups.is_empty() {
        ui_.label(egui::RichText::new("No entries this month.").weak());
    }
    ui_.add_space(4.0);
    egui::CollapsingHeader::new("Plain-text preview")
        .id_salt("reports/month_preview")
        .default_open(false)
        .show(ui_, |ui_| {
            egui::Frame::group(ui_.style())
                .fill(ui_.visuals().extreme_bg_color)
                .show(ui_, |ui_| {
                    ui_.set_width(ui_.available_width());
                    ui_.label(egui::RichText::new(rep.to_text()).monospace());
                });
        });
}

fn weekly_section(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.heading("Weekly summary");
    ui_.add_space(4.0);

    ui_.horizontal(|ui_| {
        if ui_.button("⏴").on_hover_text("previous week").clicked() {
            app.week_start -= chrono::Duration::days(7);
        }
        // Calendar jump: pick any date, we snap to its Monday.
        let mut pick = app.week_start;
        if ui::date_picker(ui_, "reports/week", &mut pick) {
            app.week_start = report::week_start_of(pick);
        }
        if ui_.button("⏵").on_hover_text("next week").clicked() {
            app.week_start += chrono::Duration::days(7);
        }
        let this_week = report::week_start_of(today());
        if app.week_start != this_week && ui_.button("This week").clicked() {
            app.week_start = this_week;
        }
        let week_end = app.week_start + chrono::Duration::days(6);
        ui_.label(
            egui::RichText::new(format!("Mon {} – Sun {}", app.week_start, week_end)).weak(),
        );
    });
    ui_.add_space(6.0);

    let text = app.weekly_report().to_text();
    let grid = report::WeekGrid::from_report(app.weekly_report());

    ui_.horizontal(|ui_| {
        if ui_.button("Copy full week").clicked() {
            ui_.ctx().copy_text(text.clone());
            app.set_status("Full week copied to clipboard");
        }
        if ui_.button("Export CSV…").clicked() {
            let csv = app.weekly_report().to_csv();
            app.export_csv(&format!("timesheet-{}.csv", app.week_start), &csv);
        }
        if ui_.button("Export PDF…").clicked() {
            let doc = report::pdf::weekly_pdf(&app.db, app.week_start, app.pdf_include_notes);
            app.export_pdf(&format!("timesheet-{}.pdf", app.week_start), doc);
        }
        notes_option(app, ui_);
        ui_.label(
            egui::RichText::new("click any hours cell to copy that day's text")
                .weak()
                .small(),
        );
    });
    ui_.add_space(6.0);

    week_grid(app, ui_, &grid);

    ui_.add_space(8.0);
    egui::CollapsingHeader::new("Plain-text preview")
        .default_open(false)
        .show(ui_, |ui_| {
            egui::Frame::group(ui_.style())
                .fill(ui_.visuals().extreme_bg_color)
                .show(ui_, |ui_| {
                    ui_.set_width(ui_.available_width());
                    ui_.label(egui::RichText::new(&text).monospace());
                });
        });
}

/// The timesheet grid: projects down, Mon–Sun across. Cells, project labels
/// and day totals are all click-to-copy at their granularity.
fn week_grid(app: &mut WorklogApp, ui_: &mut egui::Ui, grid: &report::WeekGrid) {
    if grid.rows.is_empty() {
        ui_.label(egui::RichText::new("No entries this week.").weak());
        return;
    }
    let mut copied: Option<(String, String)> = None; // (status label, clipboard text)

    // frameless button that quietly copies on click
    fn copy_cell(
        ui_: &mut egui::Ui,
        label: egui::RichText,
        hover: &str,
    ) -> bool {
        ui_.add(egui::Button::new(label).frame(false))
            .on_hover_text(hover)
            .clicked()
    }

    egui::ScrollArea::horizontal()
        .id_salt("reports/week_grid")
        .show(ui_, |ui_| {
            egui::Grid::new("reports/grid")
                .striped(true)
                .min_col_width(52.0)
                .show(ui_, |ui_| {
                    // header
                    ui_.label("");
                    for i in 0..7 {
                        let date = grid.week_start + chrono::Duration::days(i);
                        ui_.label(
                            egui::RichText::new(date.format("%a %d").to_string())
                                .weak()
                                .small(),
                        );
                    }
                    ui_.label(egui::RichText::new("total").weak().small());
                    ui_.end_row();

                    // one row per project
                    for row in &grid.rows {
                        let row_text = row.copy_text(grid.week_start);
                        if copy_cell(
                            ui_,
                            egui::RichText::new(&row.code).monospace().strong(),
                            &format!("{} — copy the whole week:\n{row_text}", row.name),
                        ) {
                            copied = Some((format!("{} week", row.code), row_text.clone()));
                        }
                        for (i, cell) in row.days.iter().enumerate() {
                            if cell.hours > 0.0 {
                                let cell_text = cell.copy_text();
                                if copy_cell(
                                    ui_,
                                    egui::RichText::new(report::fmt_hours(cell.hours)).strong(),
                                    &format!("copy:\n{cell_text}"),
                                ) {
                                    let date = grid.week_start + chrono::Duration::days(i as i64);
                                    copied = Some((
                                        format!("{} {}", row.code, date.format("%a")),
                                        cell_text,
                                    ));
                                }
                            } else {
                                ui_.label(egui::RichText::new("·").weak());
                            }
                        }
                        ui_.label(egui::RichText::new(report::fmt_hours(row.total)).weak());
                        ui_.end_row();
                    }

                    // day totals; click = that day across all projects
                    ui_.label(egui::RichText::new("total").weak().small());
                    for (i, t) in grid.day_totals.iter().enumerate() {
                        if *t > 0.0 {
                            let day_text = grid.day_copy_text(i);
                            if copy_cell(
                                ui_,
                                egui::RichText::new(report::fmt_hours(*t)),
                                &format!("copy the whole day:\n{day_text}"),
                            ) {
                                let date = grid.week_start + chrono::Duration::days(i as i64);
                                copied =
                                    Some((date.format("%a %m-%d").to_string(), day_text));
                            }
                        } else {
                            ui_.label(egui::RichText::new("·").weak());
                        }
                    }
                    ui_.label(
                        egui::RichText::new(report::fmt_hours(grid.total)).strong(),
                    );
                    ui_.end_row();
                });
        });

    if let Some((what, text)) = copied {
        ui_.ctx().copy_text(text);
        app.set_status(format!("Copied {what} to clipboard"));
    }
}

fn annual_section(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.heading("Annual dev export (SR&ED)");
    ui_.add_space(4.0);
    ui_.label(
        egui::RichText::new(
            "All entries flagged “dev (R&D)”, grouped by project, chronological — \
             CSV columns: date, project code, project name, description, hours.",
        )
        .weak(),
    );
    ui_.add_space(4.0);

    ui_.horizontal(|ui_| {
        if ui_.button("⏴").clicked() {
            app.report_year -= 1;
        }
        ui_.label(egui::RichText::new(app.report_year.to_string()).strong());
        if ui_.add_enabled(app.report_year < today().year(), egui::Button::new("⏵")).clicked() {
            app.report_year += 1;
        }

        let count = report::annual_dev_count(&app.db, app.report_year);
        ui_.label(egui::RichText::new(format!("{count} dev entries")).weak());

        if ui_
            .add_enabled(count > 0, egui::Button::new("Export CSV…"))
            .clicked()
        {
            match report::annual_dev_csv(&app.db, app.report_year) {
                Ok((csv, _)) => {
                    app.export_csv(&format!("dev-export-{}.csv", app.report_year), &csv)
                }
                Err(e) => app.set_status(format!("Export failed: {e}")),
            }
        }
        if ui_
            .add_enabled(count > 0, egui::Button::new("Export PDF…"))
            .clicked()
        {
            let doc = report::pdf::annual_pdf(&app.db, app.report_year, app.pdf_include_notes);
            app.export_pdf(&format!("dev-export-{}.pdf", app.report_year), doc);
        }
        notes_option(app, ui_);
    });
}

/// The shared PDF option: include each entry's task notes under it.
fn notes_option(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let mut with_notes = app.pdf_include_notes;
    ui_.checkbox(&mut with_notes, "PDF with task notes")
        .on_hover_text("include each entry's task notes (rendered markdown) under it in the PDF");
    if with_notes != app.pdf_include_notes {
        app.pdf_include_notes = with_notes;
        let _ = app
            .db
            .set_setting("pdf_notes", if with_notes { "1" } else { "0" });
    }
}
