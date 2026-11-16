use chrono::{Datelike, NaiveDate};
use egui::{Align, Align2, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::app::{WorklogApp, today};
use crate::report::{self, WeekGrid, fmt_hours};
use crate::ui::theme::{self as t, Weight};
use crate::ui::widgets::{self as w, icon};

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            weekly_card(app, ui_);
            ui_.add_space(20.0);
            let width = ui_.available_width();
            if width >= 900.0 {
                // monthly : SR&ED ≈ 3 : 2, side by side
                let left = ((width - 20.0) * 0.6).floor();
                ui_.horizontal_top(|ui_| {
                    ui_.spacing_mut().item_spacing.x = 20.0;
                    let down = Layout::top_down(Align::Min);
                    ui_.allocate_ui_with_layout(vec2(left, 0.0), down, |ui_| monthly_card(app, ui_));
                    let rest = ui_.available_width();
                    ui_.allocate_ui_with_layout(vec2(rest, 0.0), down, |ui_| annual_card(app, ui_));
                });
            } else {
                monthly_card(app, ui_);
                ui_.add_space(20.0);
                annual_card(app, ui_);
            }
            ui_.add_space(8.0);
        });
}

fn report_card() -> egui::Frame {
    w::card_frame().inner_margin(Margin::symmetric(22, 20))
}

fn card_title(ui_: &mut egui::Ui, text: &str) {
    ui_.label(RichText::new(text).font(t::font(17.0, Weight::Semibold)).color(t::TEXT));
}

/// ‹ middle › in an inset track. Returns -1 / +1 when an arrow is clicked.
fn stepper(
    ui_: &mut egui::Ui,
    prev_tip: &str,
    next_tip: &str,
    next_enabled: bool,
    middle: impl FnOnce(&mut egui::Ui),
) -> i32 {
    let mut step = 0;
    egui::Frame::new()
        .fill(t::BG)
        .stroke(Stroke::new(1.0, t::BORDER_STRONG))
        .corner_radius(10)
        .inner_margin(Margin::same(3))
        .show(ui_, |ui_| {
            ui_.spacing_mut().item_spacing.x = 4.0;
            if w::icon_button_ex(ui_, icon::CARET_LEFT, prev_tip, 30.0, t::MUTED, true).clicked() {
                step = -1;
            }
            middle(ui_);
            if w::icon_button_ex(ui_, icon::CARET_RIGHT, next_tip, 30.0, t::MUTED, next_enabled)
                .clicked()
            {
                step = 1;
            }
        });
    step
}

/// "Nov 9 – 15, 2026" (month and year repeated only when they change).
fn week_label(start: NaiveDate) -> String {
    let end = start + chrono::Duration::days(6);
    let tail = if end.month() == start.month() {
        end.format("%-d, %Y").to_string()
    } else {
        end.format("%b %-d, %Y").to_string()
    };
    if end.year() != start.year() {
        format!("{} – {tail}", start.format("%b %-d, %Y"))
    } else {
        format!("{} – {tail}", start.format("%b %-d"))
    }
}

// ---------------------------------------------------------------- weekly

fn weekly_card(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let grid = WeekGrid::from_report(app.weekly_report());
    let text = app.weekly_text();
    let missing = app.missing_in_week(app.week_start);
    let entries_per_project: Vec<(i64, usize)> = app
        .weekly_report()
        .groups
        .iter()
        .map(|g| (g.project_id, g.entries.len()))
        .collect();

    report_card().show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.spacing_mut().item_spacing.y = 20.0;
        weekly_header(app, ui_, &text, missing.len());
        kpi_cards(app, ui_, &grid, &entries_per_project);
        reorder_hint(app, ui_);
        hours_grid(app, ui_, &grid, &missing);
        preview_box(app, ui_, "reports/week_preview", &text, true);
    });
}

fn weekly_header(app: &mut WorklogApp, ui_: &mut egui::Ui, text: &str, missing: usize) {
    let left = |app: &mut WorklogApp, ui_: &mut egui::Ui| {
        ui_.spacing_mut().item_spacing.x = 12.0;
        card_title(ui_, "Weekly summary");
        let this_week = report::week_start_of(today());
        let step = stepper(ui_, "Previous week", "Next week", true, |ui_| {
            let mut pick = app.week_start;
            ui_.scope(|ui_| {
                let v = &mut ui_.visuals_mut().widgets;
                v.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                v.inactive.bg_stroke = Stroke::NONE;
                v.hovered.bg_stroke = Stroke::NONE;
                ui_.spacing_mut().button_padding = vec2(10.0, 7.0);
                let label = week_label(pick);
                crate::ui::date_picker::show(ui_, "reports/week", &mut pick, Some(label), &mut |m| {
                    app.day_markers(m)
                });
            });
            if pick != app.week_start {
                // calendar jump: any date snaps to its Monday
                app.week_start = report::week_start_of(pick);
            }
        });
        app.week_start += chrono::Duration::days(7 * step as i64);
        if app.week_start == this_week {
            w::pill(
                ui_,
                RichText::new("This week").font(t::font(11.0, Weight::Semibold)).color(t::ACCENT_TEXT),
                t::accent_tint(),
                egui::Color32::TRANSPARENT,
                t::R_PILL,
            );
        } else if ui_.add(w::ghost("This week")).clicked() {
            app.week_start = this_week;
        }
        if missing > 0 {
            let s = if missing == 1 { "" } else { "s" };
            w::warn_pill(ui_, &format!("{missing} weekday{s} with no hours"))
                .on_hover_text("Past weekdays with nothing logged and no day off");
        }
    };
    let right = |app: &mut WorklogApp, ui_: &mut egui::Ui| {
        ui_.spacing_mut().item_spacing.x = 6.0;
        if ui_.add(w::primary(format!("{}  PDF", icon::FILE_PDF)).min_size(vec2(0.0, 34.0))).clicked() {
            let doc = report::pdf::weekly_pdf(&app.db, app.week_start, app.pdf_include_notes);
            app.export_pdf(&format!("timesheet-{}.pdf", app.week_start), doc);
        }
        if ui_.add(w::secondary(format!("{}  CSV", icon::DOWNLOAD_SIMPLE)).min_size(vec2(0.0, 34.0))).clicked() {
            let csv = app.weekly_report().to_csv();
            app.export_csv(&format!("timesheet-{}.csv", app.week_start), &csv);
        }
        if ui_.add(w::secondary(format!("{}  Copy week", icon::COPY)).min_size(vec2(0.0, 34.0))).clicked() {
            ui_.ctx().copy_text(text.to_string());
            app.set_status("Full week copied to clipboard");
        }
        ui_.add_space(10.0);
        notes_option(app, ui_);
    };
    if ui_.available_width() >= 1180.0 {
        ui_.horizontal(|ui_| {
            left(app, ui_);
            ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| right(app, ui_));
        });
    } else {
        ui_.horizontal_wrapped(|ui_| left(app, ui_));
        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| right(app, ui_));
    }
}

/// Total card (with a stacked bar) plus one card per project.
fn kpi_cards(
    app: &mut WorklogApp,
    ui_: &mut egui::Ui,
    grid: &WeekGrid,
    entries: &[(i64, usize)],
) {
    let n = 1 + grid.rows.len();
    let gap = 12.0;
    let avail = ui_.available_width();
    let per_row = ((avail + gap) / (200.0 + gap)).floor().max(1.0) as usize;
    let cols = n.min(per_row);
    let width = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor() - 1.0;
    ui_.horizontal_wrapped(|ui_| {
        ui_.spacing_mut().item_spacing = vec2(gap, gap);
        kpi_card(ui_, width, |ui_| {
            ui_.label(RichText::new("Total this week").size(12.0).color(t::MUTED));
            big_hours(ui_, grid.total);
            // stacked bar split by project colour
            let (rect, _) = ui_.allocate_exact_size(vec2(ui_.available_width(), 6.0), Sense::hover());
            if grid.total > 0.0 {
                let mut x = rect.left();
                let gaps = (grid.rows.len().saturating_sub(1)) as f32 * 2.0;
                let span = rect.width() - gaps;
                for row in &grid.rows {
                    let w_ = span * (row.total / grid.total) as f32;
                    let seg = Rect::from_min_size(pos2(x, rect.top()), vec2(w_, 6.0));
                    ui_.painter().rect_filled(seg, 3, app.project_swatch(row.project_id).base);
                    x += w_ + 2.0;
                }
            } else {
                ui_.painter().rect_filled(rect, 3, t::COUNT_PILL);
            }
        });
        for row in &grid.rows {
            let sw = app.project_swatch(row.project_id);
            let count = entries
                .iter()
                .find(|(id, _)| *id == row.project_id)
                .map_or(0, |(_, c)| *c);
            kpi_card(ui_, width, |ui_| {
                ui_.horizontal(|ui_| {
                    ui_.spacing_mut().item_spacing.x = 7.0;
                    w::dot(ui_, sw.base, 7.0);
                    ui_.label(w::mono(&row.code, 12.0).color(sw.light));
                });
                big_hours(ui_, row.total);
                let pct = if grid.total > 0.0 { row.total / grid.total * 100.0 } else { 0.0 };
                let s = if count == 1 { "entry" } else { "entries" };
                ui_.label(
                    RichText::new(format!("{pct:.0}% of week · {count} {s}"))
                        .size(12.0)
                        .color(t::MUTED),
                );
            });
        }
    });
}

fn kpi_card(ui_: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(t::BG)
        .stroke(Stroke::new(1.0, t::DIVIDER))
        .corner_radius(t::R_LIST)
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui_, |ui_| {
            ui_.set_width(width - 34.0);
            ui_.vertical(|ui_| {
                ui_.spacing_mut().item_spacing.y = 10.0;
                add(ui_);
            });
        });
}

fn big_hours(ui_: &mut egui::Ui, hours: f64) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 4.0;
        ui_.label(w::mono_semi(fmt_hours(hours), 28.0).color(t::TEXT));
        ui_.label(RichText::new("h").size(15.0).color(t::MUTED));
    });
}

fn reorder_hint(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 10.0;
        w::icon_label(
            ui_,
            &format!(
                "{}  Drag or use the arrows to match your office timesheet. Row order is saved \
                 and used by Copy, CSV and PDF.",
                icon::ARROWS_DOWN_UP
            ),
            12.5,
            t::MUTED,
        );
        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
            let armed_id = egui::Id::new("reports/reset_armed");
            let armed: bool = ui_.ctx().data(|d| d.get_temp(armed_id)).unwrap_or(false);
            if armed {
                if ui_.add(w::ghost("Cancel").min_size(vec2(0.0, 28.0))).clicked() {
                    ui_.ctx().data_mut(|d| d.insert_temp(armed_id, false));
                }
                if ui_
                    .add(w::primary("Reset to A–Z").min_size(vec2(0.0, 28.0)))
                    .clicked()
                {
                    match app.db.reset_project_order() {
                        Ok(()) => app.set_status("Report order reset to A–Z"),
                        Err(e) => app.set_status(format!("Reset failed: {e}")),
                    }
                    app.reload_projects();
                    app.touch();
                    ui_.ctx().data_mut(|d| d.insert_temp(armed_id, false));
                }
                ui_.label(RichText::new("Forget the custom order?").size(12.0).color(t::AMBER));
            } else if ui_
                .add(w::ghost("Reset to A–Z").min_size(vec2(0.0, 28.0)))
                .on_hover_text("Sort the rows alphabetically by code again")
                .clicked()
            {
                ui_.ctx().data_mut(|d| d.insert_temp(armed_id, true));
            }
        });
    });
}

enum GridAction {
    Copy(String, String),
    /// Move project .0 next to .1 (above when .2).
    Move(i64, i64, bool),
}

/// Projects down, Mon–Sun across, totals right and bottom. Cells copy
/// their text on click; rows reorder by grip drag or the ↑/↓ arrows.
fn hours_grid(app: &mut WorklogApp, ui_: &mut egui::Ui, grid: &WeekGrid, missing: &[NaiveDate]) {
    let min_width = 820.0;
    let avail = ui_.available_width();
    let mut action: Option<GridAction> = None;
    egui::ScrollArea::horizontal()
        .id_salt("reports/week_grid")
        .show(ui_, |ui_| {
            let width = avail.max(min_width);
            egui::Frame::new()
                .stroke(Stroke::new(1.0, t::DIVIDER))
                .corner_radius(t::R_LIST)
                .show(ui_, |ui_| {
                    ui_.set_width(width - 2.0);
                    ui_.spacing_mut().item_spacing.y = 0.0;
                    let cols = columns(width - 2.0);
                    let today = today();
                    let day = |i: usize| grid.week_start + chrono::Duration::days(i as i64);
                    let is_missing = |i: usize| missing.contains(&day(i));

                    // header
                    let (rect, _) = ui_.allocate_exact_size(vec2(width - 2.0, 38.0), Sense::hover());
                    let p = ui_.painter();
                    p.rect_filled(rect, egui::CornerRadius { nw: 11, ne: 11, sw: 0, se: 0 }, t::TABLE_HEAD);
                    p.text(
                        pos2(rect.left() + 16.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        "Project",
                        t::font(12.0, Weight::Medium),
                        t::MUTED,
                    );
                    for i in 0..7 {
                        let c = cols.day(i, rect);
                        let d = day(i);
                        let label = d.format("%a %d").to_string();
                        let (bg, color, font, text) = if is_missing(i) {
                            (Some(t::AMBER_BG), t::AMBER, t::font(12.0, Weight::Medium), format!("{label} !"))
                        } else if d == today {
                            (Some(t::TODAY_COL), t::ACCENT_TEXT, t::font(12.0, Weight::Semibold), label)
                        } else if i >= 5 {
                            (None, t::FAINT, t::font(12.0, Weight::Regular), label)
                        } else {
                            (None, t::TEXT_2, t::font(12.0, Weight::Regular), label)
                        };
                        if let Some(bg) = bg {
                            p.rect_filled(c, 0, bg);
                        }
                        p.text(c.center(), Align2::CENTER_CENTER, text, font, color);
                    }
                    let tc = cols.total(rect);
                    p.vline(tc.left(), tc.y_range(), Stroke::new(1.0, t::DIVIDER));
                    p.text(
                        pos2(tc.right() - 16.0, tc.center().y),
                        Align2::RIGHT_CENTER,
                        "Total",
                        t::font(12.0, Weight::Medium),
                        t::MUTED,
                    );
                    p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, t::DIVIDER));

                    // project rows
                    let n = grid.rows.len();
                    for (r, row) in grid.rows.iter().enumerate() {
                        let prev = (r > 0).then(|| grid.rows[r - 1].project_id);
                        let next = grid.rows.get(r + 1).map(|x| x.project_id);
                        grid_row(app, ui_, grid, row, &cols, prev, next, &is_missing, &mut action);
                        if r + 1 < n {
                            let y = ui_.cursor().top();
                            ui_.painter().hline(
                                (ui_.max_rect().left())..=(ui_.max_rect().left() + width - 2.0),
                                y,
                                Stroke::new(1.0, t::LIST_DIVIDER),
                            );
                        }
                    }
                    if n == 0 {
                        let (rect, _) = ui_.allocate_exact_size(vec2(width - 2.0, 48.0), Sense::hover());
                        ui_.painter().text(
                            pos2(rect.left() + 16.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            "No entries this week.",
                            t::font(13.0, Weight::Regular),
                            t::MUTED,
                        );
                    }

                    // daily totals
                    let (rect, _) = ui_.allocate_exact_size(vec2(width - 2.0, 46.0), Sense::hover());
                    ui_.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0, t::DIVIDER));
                    ui_.painter().rect_filled(
                        rect.shrink2(vec2(0.0, 0.5)).translate(vec2(0.0, 0.5)),
                        egui::CornerRadius { nw: 0, ne: 0, sw: 11, se: 11 },
                        t::TABLE_HEAD,
                    );
                    ui_.painter().text(
                        pos2(rect.left() + 16.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        "Daily total",
                        t::font(12.0, Weight::Semibold),
                        t::MUTED,
                    );
                    for i in 0..7 {
                        let c = cols.day(i, rect);
                        let total = grid.day_totals[i];
                        if is_missing(i) {
                            ui_.painter().rect_filled(c, 0, t::AMBER_BG);
                            ui_.painter().text(c.center(), Align2::CENTER_CENTER, "0", t::mono_semi(13.0), t::AMBER);
                        } else if total > 0.0 {
                            if day(i) == today {
                                ui_.painter().rect_filled(c, 0, t::TODAY_COL);
                            }
                            let resp = ui_
                                .interact(c, ui_.id().with(("day_total", i)), Sense::click())
                                .on_hover_text(format!("Copy the whole day:\n{}", grid.day_copy_text(i)));
                            let color = if resp.hovered() { t::ACCENT_TEXT } else { t::TEXT };
                            ui_.painter().text(c.center(), Align2::CENTER_CENTER, fmt_hours(total), t::mono_semi(13.0), color);
                            if resp.clicked() {
                                action = Some(GridAction::Copy(
                                    day(i).format("%a %m-%d").to_string(),
                                    grid.day_copy_text(i),
                                ));
                            }
                        } else {
                            if day(i) == today {
                                ui_.painter().rect_filled(c, 0, t::TODAY_COL);
                            }
                            ui_.painter().text(c.center(), Align2::CENTER_CENTER, "·", t::mono(13.0), t::DISABLED);
                        }
                    }
                    let tc = cols.total(rect);
                    ui_.painter().vline(tc.left(), tc.y_range(), Stroke::new(1.0, t::DIVIDER));
                    ui_.painter().text(
                        pos2(tc.right() - 16.0, tc.center().y),
                        Align2::RIGHT_CENTER,
                        format!("{} h", fmt_hours(grid.total)),
                        t::mono_semi(14.0),
                        t::ACCENT_TEXT,
                    );
                });
        });

    match action {
        Some(GridAction::Copy(what, text)) => {
            ui_.ctx().copy_text(text);
            app.set_status(format!("Copied {what} to clipboard"));
        }
        Some(GridAction::Move(src, target, before)) => app.move_report_row(src, target, before),
        None => {}
    }
}

/// Column geometry: project ≥ 240 (1.8 fr) | 7 × day (1 fr) | total (1 fr).
struct Columns {
    project: f32,
    day: f32,
}

fn columns(width: f32) -> Columns {
    let fr = width / 9.8;
    let project = (fr * 1.8).max(240.0);
    let day = (width - project) / 8.0;
    Columns { project, day }
}

impl Columns {
    fn day(&self, i: usize, row: Rect) -> Rect {
        let x = row.left() + self.project + self.day * i as f32;
        Rect::from_min_max(pos2(x, row.top()), pos2(x + self.day, row.bottom()))
    }
    fn total(&self, row: Rect) -> Rect {
        let x = row.left() + self.project + self.day * 7.0;
        Rect::from_min_max(pos2(x, row.top()), row.right_bottom())
    }
    fn project(&self, row: Rect) -> Rect {
        Rect::from_min_size(row.min, vec2(self.project, row.height()))
    }
}

#[allow(clippy::too_many_arguments)]
fn grid_row(
    app: &mut WorklogApp,
    ui_: &mut egui::Ui,
    grid: &WeekGrid,
    row: &report::GridRow,
    cols: &Columns,
    prev: Option<i64>,
    next: Option<i64>,
    is_missing: &dyn Fn(usize) -> bool,
    action: &mut Option<GridAction>,
) {
    let width = ui_.available_width();
    let (rect, _) = ui_.allocate_exact_size(vec2(width, 48.0), Sense::hover());
    let sw = app.project_swatch(row.project_id);
    let today = today();

    // day cells
    for (i, cell) in row.days.iter().enumerate() {
        let c = cols.day(i, rect);
        let date = grid.week_start + chrono::Duration::days(i as i64);
        if is_missing(i) {
            ui_.painter().rect_filled(c, 0, t::alpha(t::AMBER, 0.05));
        } else if date == today {
            ui_.painter().rect_filled(c, 0, t::TODAY_COL);
        }
        if cell.hours > 0.0 {
            let inner = c.shrink(6.0);
            let strength = 0.22 + 0.16 * (cell.hours / 8.0).min(1.0) as f32;
            let resp = ui_
                .interact(inner, ui_.id().with(("cell", row.project_id, i)), Sense::click())
                .on_hover_text(format!("Click to copy:\n{}", cell.copy_text()));
            ui_.painter().rect_filled(inner, 7, t::alpha(sw.base, strength));
            if resp.hovered() {
                ui_.painter()
                    .rect_stroke(inner, 7, Stroke::new(1.5, t::ACCENT), StrokeKind::Inside);
            }
            ui_.painter().text(
                inner.center(),
                Align2::CENTER_CENTER,
                fmt_hours(cell.hours),
                t::mono_semi(13.0),
                egui::Color32::WHITE,
            );
            if resp.clicked() {
                *action = Some(GridAction::Copy(
                    format!("{} {}", row.code, date.format("%a")),
                    cell.copy_text(),
                ));
            }
        } else {
            ui_.painter()
                .text(c.center(), Align2::CENTER_CENTER, "·", t::mono(13.0), t::DISABLED);
        }
    }
    let tc = cols.total(rect);
    ui_.painter().vline(tc.left(), tc.y_range(), Stroke::new(1.0, t::DIVIDER));
    ui_.painter().text(
        pos2(tc.right() - 16.0, tc.center().y),
        Align2::RIGHT_CENTER,
        fmt_hours(row.total),
        t::mono_semi(13.0),
        t::TEXT,
    );

    // project cell: grip, dot, code + name, ↑ ↓
    let pc = cols.project(rect).shrink2(vec2(6.0, 0.0));
    ui_.scope_builder(
        egui::UiBuilder::new()
            .max_rect(pc)
            .layout(Layout::left_to_right(Align::Center)),
        |ui_| {
            ui_.spacing_mut().item_spacing.x = 9.0;
            ui_.dnd_drag_source(egui::Id::new(("report_row_drag", row.project_id)), row.project_id, |ui_| {
                ui_.label(
                    RichText::new(icon::DOTS_SIX_VERTICAL)
                        .font(t::icon_font(16.0))
                        .color(t::DISABLED),
                )
                .on_hover_text("Drag to reorder");
            });
            w::dot(ui_, sw.base, 8.0);
            let week_text = row.copy_text(grid.week_start);
            if ui_
                .add(
                    egui::Label::new(w::mono_semi(&row.code, 13.0).color(sw.light))
                        .sense(Sense::click()),
                )
                .on_hover_text(format!("{} — click to copy the whole week:\n{week_text}", row.name))
                .clicked()
            {
                *action = Some(GridAction::Copy(format!("{} week", row.code), week_text));
            }
            ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                ui_.spacing_mut().item_spacing.x = 2.0;
                if w::icon_button_ex(ui_, icon::ARROW_DOWN, "Move down", 26.0, t::MUTED, next.is_some())
                    .clicked()
                    && let Some(n) = next
                {
                    *action = Some(GridAction::Move(row.project_id, n, false));
                }
                if w::icon_button_ex(ui_, icon::ARROW_UP, "Move up", 26.0, t::MUTED, prev.is_some())
                    .clicked()
                    && let Some(p) = prev
                {
                    *action = Some(GridAction::Move(row.project_id, p, true));
                }
                ui_.with_layout(Layout::left_to_right(Align::Center), |ui_| {
                    ui_.add(
                        egui::Label::new(RichText::new(&row.name).size(12.0).color(t::MUTED))
                            .truncate(),
                    );
                });
            });
        },
    );

    // drop target with an insertion line
    if egui::DragAndDrop::has_any_payload(ui_.ctx())
        && let Some(pointer) = ui_.ctx().pointer_interact_pos()
        && rect.contains(pointer)
    {
        let before = pointer.y < rect.center().y;
        let y = if before { rect.top() } else { rect.bottom() };
        ui_.painter().hline(rect.x_range(), y, Stroke::new(2.0, t::ACCENT));
        if ui_.input(|i| i.pointer.any_released())
            && let Some(src) = egui::DragAndDrop::take_payload::<i64>(ui_.ctx())
        {
            *action = Some(GridAction::Move(*src, row.project_id, before));
        }
    }
}

/// Collapsible plain-text preview in a bordered box.
fn preview_box(app: &mut WorklogApp, ui_: &mut egui::Ui, id: &str, text: &str, default_open: bool) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, t::DIVIDER))
        .corner_radius(t::R_LIST)
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.spacing_mut().item_spacing.y = 0.0;
            let mut open = false;
            egui::Frame::new()
                .fill(t::TABLE_HEAD)
                .corner_radius(egui::CornerRadius { nw: 11, ne: 11, sw: 0, se: 0 })
                .inner_margin(Margin { left: 16, right: 12, top: 8, bottom: 8 })
                .show(ui_, |ui_| {
                    ui_.set_width(ui_.available_width());
                    ui_.horizontal(|ui_| {
                        open = w::collapsible_header(ui_, egui::Id::new(id), default_open, false, |ui_| {
                            ui_.label(
                                RichText::new("Plain-text preview")
                                    .font(t::font(13.0, Weight::Medium))
                                    .color(t::TEXT_2),
                            );
                        });
                        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                            if ui_
                                .add(w::secondary(format!("{} Copy", icon::COPY)).min_size(vec2(0.0, 28.0)))
                                .clicked()
                            {
                                ui_.ctx().copy_text(text.to_string());
                                app.set_status("Copied to clipboard");
                            }
                            ui_.label(
                                RichText::new("Click any hours cell to copy that day")
                                    .size(12.0)
                                    .color(t::FAINT),
                            );
                        });
                    });
                });
            if open {
                egui::Frame::new()
                    .fill(t::PREVIEW_BG)
                    .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: 11, se: 11 })
                    .inner_margin(Margin::symmetric(18, 14))
                    .show(ui_, |ui_| {
                        ui_.set_width(ui_.available_width());
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            text.trim_end(),
                            0.0,
                            egui::TextFormat {
                                font_id: t::mono(13.0),
                                color: t::TEXT_2,
                                line_height: Some(22.0),
                                ..Default::default()
                            },
                        );
                        ui_.add(egui::Label::new(job).selectable(true));
                    });
            }
        });
}

// ---------------------------------------------------------------- monthly

fn monthly_card(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let Ok(rep) = report::monthly(&app.db, app.month_start) else {
        return;
    };
    report_card().show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.spacing_mut().item_spacing.y = 18.0;
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 12.0;
            card_title(ui_, "Monthly summary");
            let label = app.month_start.format("%B %Y").to_string();
            let step = stepper(ui_, "Previous month", "Next month", true, |ui_| {
                ui_.add_space(4.0);
                ui_.label(RichText::new(label).font(t::font(13.0, Weight::Medium)).color(t::TEXT));
                ui_.add_space(4.0);
            });
            if step < 0 {
                app.month_start = app.month_start - chrono::Months::new(1);
            } else if step > 0 {
                app.month_start = app.month_start + chrono::Months::new(1);
            }
            let this_month = report::month_start_of(today());
            if app.month_start != this_month && ui_.add(w::ghost("This month")).clicked() {
                app.month_start = this_month;
            }
            ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                ui_.spacing_mut().item_spacing.x = 6.0;
                let stamp = app.month_start.format("%Y-%m").to_string();
                if ui_.add(w::secondary("PDF").min_size(vec2(0.0, 34.0))).clicked() {
                    let doc = report::pdf::monthly_pdf(&app.db, app.month_start, app.pdf_include_notes);
                    app.export_pdf(&format!("timesheet-{stamp}.pdf"), doc);
                }
                if ui_.add(w::secondary("CSV").min_size(vec2(0.0, 34.0))).clicked() {
                    app.export_csv(&format!("timesheet-{stamp}.csv"), &rep.to_csv());
                }
                if ui_
                    .add(w::secondary(format!("{} Copy", icon::COPY)).min_size(vec2(0.0, 34.0)))
                    .clicked()
                {
                    ui_.ctx().copy_text(rep.to_text());
                    app.set_status("Month copied to clipboard");
                }
            });
        });
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 8.0;
            big_hours(ui_, rep.total_hours);
            let n = rep.groups.len();
            let s = if n == 1 { "" } else { "s" };
            ui_.label(
                RichText::new(format!(
                    "logged in {} across {n} project{s}",
                    app.month_start.format("%B")
                ))
                .size(13.0)
                .color(t::MUTED),
            );
        });
        if rep.groups.is_empty() {
            ui_.label(RichText::new("No entries this month.").color(t::MUTED));
        } else {
            ui_.vertical(|ui_| {
                ui_.spacing_mut().item_spacing.y = 14.0;
                for g in &rep.groups {
                    let sw = app.project_swatch(g.project_id);
                    ui_.horizontal(|ui_| {
                        ui_.spacing_mut().item_spacing.x = 14.0;
                        ui_.allocate_ui_with_layout(vec2(200.0, 20.0), Layout::left_to_right(Align::Center), |ui_| {
                            ui_.set_width(200.0);
                            ui_.spacing_mut().item_spacing.x = 9.0;
                            w::dot(ui_, sw.base, 8.0);
                            ui_.label(w::mono_semi(&g.code, 13.0).color(sw.light));
                            ui_.add(egui::Label::new(RichText::new(&g.name).size(12.5).color(t::MUTED)).truncate());
                        });
                        let bar_w = (ui_.available_width() - 70.0).max(40.0);
                        let share = if rep.total_hours > 0.0 { g.total_hours / rep.total_hours } else { 0.0 };
                        w::progress(ui_, share as f32, bar_w, 10.0, sw.base);
                        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                            ui_.label(w::mono_semi(format!("{} h", fmt_hours(g.total_hours)), 13.0).color(t::TEXT));
                        });
                    });
                }
            });
        }
        let id = egui::Id::new("reports/month_preview");
        let open = w::collapsible_header(ui_, id, false, false, |ui_| {
            ui_.label(RichText::new("Plain-text preview").size(13.0).color(t::TEXT_2));
        });
        if open {
            egui::Frame::new()
                .fill(t::PREVIEW_BG)
                .stroke(Stroke::new(1.0, t::DIVIDER))
                .corner_radius(t::R_LIST)
                .inner_margin(Margin::symmetric(18, 14))
                .show(ui_, |ui_| {
                    ui_.set_width(ui_.available_width());
                    ui_.add(egui::Label::new(w::mono(rep.to_text().trim_end(), 13.0).color(t::TEXT_2)).selectable(true));
                });
        }
    });
}

// ---------------------------------------------------------------- annual

fn annual_card(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    report_card().show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.spacing_mut().item_spacing.y = 16.0;
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 10.0;
            card_title(ui_, "Annual dev export");
            w::pill(
                ui_,
                RichText::new("SR&ED").font(t::font(11.0, Weight::Semibold)).color(t::AMBER),
                t::alpha(t::AMBER, 0.12),
                t::alpha(t::AMBER, 0.32),
                6,
            );
        });
        ui_.add(
            egui::Label::new(
                RichText::new(
                    "Every entry flagged R&D, grouped by project in date order. CSV columns: \
                     date, project code, project name, description, hours.",
                )
                .size(13.0)
                .color(t::MUTED),
            )
            .wrap(),
        );
        let count = report::annual_dev_count(&app.db, app.report_year);
        egui::Frame::new()
            .fill(t::BG)
            .stroke(Stroke::new(1.0, t::DIVIDER))
            .corner_radius(t::R_LIST)
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui_, |ui_| {
                ui_.set_width(ui_.available_width());
                ui_.horizontal(|ui_| {
                    ui_.spacing_mut().item_spacing.x = 4.0;
                    if w::icon_button_ex(ui_, icon::CARET_LEFT, "Previous year", 30.0, t::MUTED, true).clicked() {
                        app.report_year -= 1;
                    }
                    ui_.label(w::mono_semi(app.report_year.to_string(), 15.0).color(t::TEXT));
                    let can_next = app.report_year < today().year();
                    if w::icon_button_ex(ui_, icon::CARET_RIGHT, "Next year", 30.0, t::MUTED, can_next).clicked() {
                        app.report_year += 1;
                    }
                    ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                        ui_.label(RichText::new("R&D entries").size(13.0).color(t::MUTED));
                        ui_.label(w::mono_semi(count.to_string(), 13.0).color(t::TEXT));
                    });
                });
            });
        notes_option(app, ui_);
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 8.0;
            let half = (ui_.available_width() - 8.0) / 2.0;
            if ui_
                .add_enabled(
                    count > 0,
                    w::secondary(format!("{}  Export CSV", icon::DOWNLOAD_SIMPLE)).min_size(vec2(half, 38.0)),
                )
                .clicked()
            {
                match report::annual_dev_csv(&app.db, app.report_year) {
                    Ok((csv, _)) => app.export_csv(&format!("dev-export-{}.csv", app.report_year), &csv),
                    Err(e) => app.set_status(format!("Export failed: {e}")),
                }
            }
            if ui_
                .add_enabled(
                    count > 0,
                    w::primary(format!("{}  Export PDF", icon::FILE_PDF)).min_size(vec2(half, 38.0)),
                )
                .clicked()
            {
                let doc = report::pdf::annual_pdf(&app.db, app.report_year, app.pdf_include_notes);
                app.export_pdf(&format!("dev-export-{}.pdf", app.report_year), doc);
            }
        });
    });
}

/// The shared PDF option: include each entry's task notes under it.
fn notes_option(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let resp = ui_
        .horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 8.0;
            let a = w::checkbox(ui_, app.pdf_include_notes, "");
            let b = ui_.add(
                egui::Label::new(RichText::new("Include task notes in PDF").size(13.0).color(t::TEXT_2))
                    .sense(Sense::click()),
            );
            a.clicked() || b.clicked()
        })
        .inner;
    if resp {
        app.pdf_include_notes = !app.pdf_include_notes;
        let _ = app
            .db
            .set_setting("pdf_notes", if app.pdf_include_notes { "1" } else { "0" });
    }
}
