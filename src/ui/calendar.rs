//! The Calendar tab: a month of logged hours, days off and missing days,
//! with a side panel to mark days off and import Québec holidays.

use chrono::{Datelike, Duration, NaiveDate};
use egui::{
    Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind,
    pos2, vec2,
};

use crate::app::{CalData, DayOffForm, QcImport, WorklogApp, today};
use crate::db::{DayOff, DayOffKind};
use crate::holidays::{self, EasterDay};
use crate::report::{self, fmt_hours};
use crate::ui;
use crate::ui::theme::{self as t, Weight};
use crate::ui::widgets::{self as w, Segment, icon};

const SIDE_WIDTH: f32 = 320.0;
const WEEK_COL: f32 = 44.0;
const HEAD_H: f32 = 34.0;
const MIN_CELL_H: f32 = 104.0;

/// Monday of the first visible week through Sunday of the sixth.
pub fn visible_range(month_start: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = report::week_start_of(month_start);
    (first, first + Duration::days(41))
}

/// Base colour and light text colour for a kind of day off.
pub fn kind_colors(kind: DayOffKind) -> (Color32, Color32) {
    match kind {
        DayOffKind::Holiday => (t::HOLIDAY, t::PALETTE[4].light),
        DayOffKind::Vacation => (t::VACATION, t::PALETTE[5].light),
        DayOffKind::Sick => (t::SICK, t::PALETTE[3].light),
        DayOffKind::Other => (t::OTHER_OFF, t::TEXT_2),
    }
}

/// Select a day and load the Mark day form from it.
pub fn select_day(app: &mut WorklogApp, day: NaiveDate) {
    app.cal_selected = Some(day);
    let mut form = DayOffForm::new(day);
    let existing = app
        .db
        .days_off_between(day, day)
        .ok()
        .and_then(|v| v.into_iter().next());
    if let Some(off) = existing {
        form.kind = off.kind;
        form.label = off.label.unwrap_or_default();
        form.half = off.half;
        form.existing = true;
    }
    app.cal_form = form;
    if report::month_start_of(day) != app.cal_month {
        app.cal_month = report::month_start_of(day);
    }
}

pub fn open_import(app: &mut WorklogApp) {
    let year = if today().month() >= 11 { today().year() + 1 } else { today().year() };
    app.qc_import = Some(import_rows(app, year, EasterDay::GoodFriday));
}

fn import_rows(app: &WorklogApp, year: i32, easter: EasterDay) -> QcImport {
    let taken: Vec<NaiveDate> = app
        .db
        .days_off_between(
            NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or_default(),
            NaiveDate::from_ymd_opt(year, 12, 31).unwrap_or_default(),
        )
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.date)
        .collect();
    QcImport {
        year,
        easter,
        rows: holidays::quebec_holidays(year, easter)
            .into_iter()
            .map(|(d, name)| (d, name, !taken.contains(&d)))
            .collect(),
    }
}

pub fn show(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    header(app, ui_);
    ui_.add_space(18.0);
    let data = app.calendar_data();
    ui::notes::with_side_panel(
        app,
        ui_,
        "calendar/side",
        true,
        SIDE_WIDTH,
        |app, ui_| side_panel(app, ui_, &data),
        |app, ui_| {
            egui::ScrollArea::both()
                .id_salt("calendar/grid")
                .auto_shrink([false, false])
                .show(ui_, |ui_| month_grid(app, ui_, &data));
        },
    );
}

fn header(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 14.0;
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 8.0;
            ui_.label(
                RichText::new(app.cal_month.format("%B").to_string())
                    .font(t::font(24.0, Weight::Semibold))
                    .color(t::TEXT),
            );
            ui_.label(
                RichText::new(app.cal_month.year().to_string())
                    .font(t::font(24.0, Weight::Medium))
                    .color(t::MUTED),
            );
        });
        egui::Frame::new()
            .fill(t::PANEL)
            .stroke(Stroke::new(1.0, t::BORDER_STRONG))
            .corner_radius(10)
            .inner_margin(Margin::same(3))
            .show(ui_, |ui_| {
                ui_.spacing_mut().item_spacing.x = 4.0;
                if w::icon_button_ex(ui_, icon::CARET_LEFT, "Previous month", 30.0, t::MUTED, true).clicked() {
                    app.cal_month = app.cal_month - chrono::Months::new(1);
                }
                if ui_.add(w::ghost("Today").min_size(vec2(0.0, 30.0))).clicked() {
                    app.cal_month = report::month_start_of(today());
                }
                if w::icon_button_ex(ui_, icon::CARET_RIGHT, "Next month", 30.0, t::MUTED, true).clicked() {
                    app.cal_month = app.cal_month + chrono::Months::new(1);
                }
            });
        ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
            ui_.spacing_mut().item_spacing.x = 14.0;
            if ui_
                .add(w::primary(format!("{}  Mark day off", icon::PLUS)).min_size(vec2(0.0, 36.0)))
                .clicked()
            {
                select_day(app, today());
            }
            // legend (right-to-left, so reversed)
            legend_item(ui_, None, "No hours");
            for kind in [DayOffKind::Sick, DayOffKind::Vacation, DayOffKind::Holiday] {
                legend_item(ui_, Some(kind_colors(kind).0), kind.label());
            }
        });
    });
}

/// Square + label. Called inside a right-to-left row, so the label goes
/// first to end up on the right.
fn legend_item(ui_: &mut egui::Ui, color: Option<Color32>, label: &str) {
    ui_.horizontal(|ui_| {
        ui_.spacing_mut().item_spacing.x = 6.0;
        ui_.label(RichText::new(label).size(12.5).color(t::MUTED));
        let (rect, _) = ui_.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        match color {
            Some(c) => {
                ui_.painter().rect_filled(rect, 3, c);
            }
            None => w::dashed_rect(ui_, rect, t::AMBER),
        }
    });
}

// ---------------------------------------------------------------- grid

fn month_grid(app: &mut WorklogApp, ui_: &mut egui::Ui, data: &CalData) {
    let width = ui_.available_width().max(760.0);
    let cell_w = ((width - WEEK_COL - 8.0) / 7.0).floor();
    let grid_w = WEEK_COL + cell_w * 7.0 + 8.0;
    let cell_h = ((ui_.available_height() - HEAD_H - 8.0) / 6.0).floor().max(MIN_CELL_H);
    let grid_h = HEAD_H + cell_h * 6.0 + 8.0;
    let (outer, _) = ui_.allocate_exact_size(vec2(grid_w, grid_h), Sense::hover());
    // 1 px gaps: the divider colour shows between the cells
    ui_.painter().rect_filled(outer, 12, t::DIVIDER);
    let inner = outer.shrink(1.0);
    let col_x = |c: usize| -> f32 {
        if c == 0 {
            inner.left()
        } else {
            inner.left() + WEEK_COL + 1.0 + (c - 1) as f32 * (cell_w + 1.0)
        }
    };
    let col_w = |c: usize| if c == 0 { WEEK_COL } else { cell_w };
    let row_y = |r: usize| -> f32 {
        if r == 0 {
            inner.top()
        } else {
            inner.top() + HEAD_H + 1.0 + (r - 1) as f32 * (cell_h + 1.0)
        }
    };
    let row_h = |r: usize| if r == 0 { HEAD_H } else { cell_h - 1.0 };
    let corner = |r: usize, c: usize| -> CornerRadius {
        let mut cr = CornerRadius::ZERO;
        if r == 0 && c == 0 {
            cr.nw = 11;
        }
        if r == 0 && c == 7 {
            cr.ne = 11;
        }
        if r == 6 && c == 0 {
            cr.sw = 11;
        }
        if r == 6 && c == 7 {
            cr.se = 11;
        }
        cr
    };
    let cell = |r: usize, c: usize| Rect::from_min_size(pos2(col_x(c), row_y(r)), vec2(col_w(c), row_h(r)));

    // header
    ui_.painter().rect_filled(cell(0, 0), corner(0, 0), t::TABLE_HEAD);
    for (c, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
        let rect = cell(0, c + 1);
        ui_.painter().rect_filled(rect, corner(0, c + 1), t::TABLE_HEAD);
        ui_.painter().text(
            pos2(rect.left() + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            *name,
            t::font(12.0, Weight::Medium),
            if c >= 5 { t::FAINT } else { t::MUTED },
        );
    }

    let (first, _) = visible_range(app.cal_month);
    let today = today();
    let mut clicked: Option<NaiveDate> = None;
    let mut double: Option<NaiveDate> = None;
    let mut week_click: Option<NaiveDate> = None;
    for r in 0..6 {
        let monday = first + Duration::days(7 * r as i64);
        // ISO week number
        let wk = cell(r + 1, 0);
        ui_.painter().rect_filled(wk, corner(r + 1, 0), t::TABLE_HEAD);
        let wresp = ui_
            .interact(wk, ui_.id().with(("cal_week", r)), Sense::click())
            .on_hover_text("Open this week in Reports");
        ui_.painter().text(
            pos2(wk.center().x, wk.top() + 16.0),
            Align2::CENTER_CENTER,
            format!("W{}", monday.iso_week().week()),
            t::mono(11.0),
            if wresp.hovered() { t::ACCENT_TEXT } else { t::FAINT },
        );
        if wresp.clicked() {
            week_click = Some(monday);
        }
        for c in 0..7 {
            let day = monday + Duration::days(c as i64);
            let rect = cell(r + 1, c + 1);
            let resp = ui_.interact(rect, ui_.id().with(("cal_day", day)), Sense::click());
            day_cell(app, ui_, rect, corner(r + 1, c + 1), day, today, data, resp.hovered());
            let resp = resp.on_hover_text("Click to mark a day off · double-click to open the journal");
            if resp.double_clicked() {
                double = Some(day);
            } else if resp.clicked() {
                clicked = Some(day);
            }
        }
    }
    if let Some(d) = double {
        app.open_journal_day(d);
    } else if let Some(d) = clicked {
        select_day(app, d);
    }
    if let Some(m) = week_click {
        app.open_report_week(m);
    }
}

#[allow(clippy::too_many_arguments)]
fn day_cell(
    app: &WorklogApp,
    ui_: &egui::Ui,
    rect: Rect,
    corner: CornerRadius,
    day: NaiveDate,
    today: NaiveDate,
    data: &CalData,
    hovered: bool,
) {
    let p = ui_.painter();
    let in_month = day.month() == app.cal_month.month();
    let weekend = day.weekday().num_days_from_monday() >= 5;
    let missing = in_month && data.missing.contains(&day);
    let off = data.days_off.get(&day);
    let hours = data.hours.get(&day);
    let selected = app.cal_selected == Some(day);

    let bg = if !in_month {
        t::OUT_OF_MONTH
    } else if day == today {
        t::TODAY_COL
    } else if missing {
        t::AMBER_BG
    } else if weekend {
        t::PANEL_ALT
    } else {
        t::PANEL
    };
    let bg = if hovered && in_month && day != today && !missing { t::HOVER } else { bg };
    p.rect_filled(rect, corner, bg);

    let pad = vec2(10.0, 8.0);
    let top = rect.top() + pad.y;
    // day number
    let num = day.day().to_string();
    if day == today {
        let c = pos2(rect.left() + pad.x + 12.0, top + 12.0);
        p.circle_filled(c, 12.0, t::ACCENT);
        p.text(c, Align2::CENTER_CENTER, num, t::font(12.0, Weight::Bold), t::ACCENT_INK);
    } else {
        let (font, color) = if !in_month {
            (t::font(13.0, Weight::Regular), t::FAINT)
        } else if weekend {
            (t::font(13.0, Weight::Semibold), t::MUTED)
        } else {
            (t::font(13.0, Weight::Semibold), t::TEXT_2)
        };
        p.text(pos2(rect.left() + pad.x, top + 2.0), Align2::LEFT_TOP, num, font, color);
    }
    let total: f64 = hours.map_or(0.0, |v| v.iter().map(|(_, h)| h).sum());
    if total > 0.0 {
        p.text(
            pos2(rect.right() - pad.x, top + 12.0),
            Align2::RIGHT_CENTER,
            format!("{} h", fmt_hours(total)),
            t::mono_semi(12.0),
            if in_month { t::TEXT } else { t::FAINT },
        );
    }

    // chips: day off first, then one per project
    let chip_h = 22.0;
    let mut y = top + 32.0;
    let bottom = rect.bottom() - 6.0;
    let chip_rect = |y: f32| {
        Rect::from_min_size(pos2(rect.left() + 6.0, y), vec2(rect.width() - 12.0, chip_h))
    };
    if let Some(off) = off {
        let (base, light) = kind_colors(off.kind);
        let r = chip_rect(y);
        p.rect_filled(r, 6, t::alpha(base, 0.12));
        w::dashed_rect(ui_, r.shrink(0.5), t::alpha(base, 0.6));
        let sq = Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(6.0, 6.0));
        p.rect_filled(sq, 2, base);
        let mut text = off.title();
        if off.half {
            text.push_str(" ½");
        }
        let galley = one_line(ui_, text, t::font(11.5, Weight::Medium), light, r.width() - 24.0);
        p.galley(pos2(r.left() + 18.0, r.center().y - galley.size().y / 2.0), galley, light);
        y += chip_h + 4.0;
    }
    if let Some(list) = hours {
        let room = ((bottom - y + 4.0) / (chip_h + 4.0)).floor().max(0.0) as usize;
        let shown = if list.len() > room { room.saturating_sub(1) } else { list.len() };
        for (pid, h) in list.iter().take(shown) {
            let sw = app.project_swatch(*pid);
            let code = app.project(*pid).map(|p| p.code.clone()).unwrap_or_default();
            let r = chip_rect(y);
            p.rect_filled(r, 6, t::alpha(sw.base, if in_month { 0.16 } else { 0.08 }));
            p.circle_filled(pos2(r.left() + 10.0, r.center().y), 3.0, sw.base);
            let hours_txt = fmt_hours(*h);
            let hw = p
                .layout_no_wrap(hours_txt.clone(), t::mono(11.5), t::TEXT)
                .size()
                .x;
            p.text(pos2(r.right() - 7.0, r.center().y), Align2::RIGHT_CENTER, hours_txt, t::mono(11.5), t::TEXT_2);
            let galley = one_line(ui_, code, t::mono(11.5), sw.light, r.width() - 30.0 - hw);
            p.galley(pos2(r.left() + 18.0, r.center().y - galley.size().y / 2.0), galley, sw.light);
            y += chip_h + 4.0;
        }
        if shown < list.len() {
            p.text(
                pos2(rect.left() + 10.0, y + chip_h / 2.0),
                Align2::LEFT_CENTER,
                format!("+{} more", list.len() - shown),
                t::font(11.0, Weight::Medium),
                t::MUTED,
            );
        }
    }

    if missing {
        p.rect_stroke(rect, corner, Stroke::new(1.0, t::alpha(t::AMBER, 0.28)), StrokeKind::Inside);
        let job = w::rich(&format!("{} No hours", icon::WARNING), t::font(11.5, Weight::Medium), t::AMBER);
        let galley = ui_.fonts_mut(|f| f.layout_job(job));
        p.galley(pos2(rect.left() + pad.x, rect.bottom() - 10.0 - galley.size().y), galley, t::AMBER);
    }
    if day == today {
        p.rect_stroke(rect, corner, Stroke::new(1.5, t::ACCENT), StrokeKind::Inside);
    }
    if selected {
        let (base, _) = kind_colors(app.cal_form.kind);
        p.rect_stroke(rect, corner, Stroke::new(1.5, t::alpha(base, 0.8)), StrokeKind::Inside);
    }
}

/// One line of text cut with "…" to fit `width` (chips never wrap).
fn one_line(
    ui_: &egui::Ui,
    text: String,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(10.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui_.fonts_mut(|f| f.layout_job(job))
}

// ---------------------------------------------------------------- side

fn side_panel(app: &mut WorklogApp, ui_: &mut egui::Ui, data: &CalData) {
    egui::ScrollArea::vertical()
        .id_salt("calendar/side_scroll")
        .auto_shrink([false, false])
        .show(ui_, |ui_| {
            ui_.spacing_mut().item_spacing.y = 16.0;
            mark_day_card(app, ui_);
            upcoming_card(app, ui_, data);
            glance_card(app, ui_, data);
        });
}

fn side_card(stroke: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(t::PANEL)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(t::R_CARD)
        .inner_margin(Margin::same(18))
}

fn mark_day_card(app: &mut WorklogApp, ui_: &mut egui::Ui) {
    let Some(day) = app.cal_selected else {
        side_card(t::BORDER).show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.spacing_mut().item_spacing.y = 6.0;
            w::eyebrow(ui_, "Mark day");
            ui_.add(
                egui::Label::new(
                    RichText::new("Click a day in the calendar to mark it off — holiday, vacation, sick or other.")
                        .size(13.0)
                        .color(t::MUTED),
                )
                .wrap(),
            );
        });
        return;
    };
    let (base, _) = kind_colors(app.cal_form.kind);
    let mut close = false;
    side_card(t::alpha(base, 0.45)).show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.spacing_mut().item_spacing.y = 14.0;
        // owned for the frame, so the date picker below can borrow `app`
        let mut owned = std::mem::replace(&mut app.cal_form, DayOffForm::new(day));
        let form = &mut owned;
        // title row
        ui_.horizontal(|ui_| {
            ui_.with_layout(Layout::right_to_left(Align::Min), |ui_| {
                if w::icon_button_ex(ui_, icon::X, "Close", 32.0, t::MUTED, true).clicked() {
                    close = true;
                }
                ui_.with_layout(Layout::top_down(Align::Min), |ui_| {
                    ui_.spacing_mut().item_spacing.y = 3.0;
                    w::eyebrow(ui_, if form.existing { "Edit day off" } else { "Mark day" });
                    let title = if form.range && form.range_end != day {
                        let (a, b) = if day <= form.range_end { (day, form.range_end) } else { (form.range_end, day) };
                        format!("{} – {}", a.format("%b %-d"), b.format("%b %-d"))
                    } else {
                        day.format("%A, %b %-d").to_string()
                    };
                    ui_.label(RichText::new(title).font(t::font(17.0, Weight::Semibold)).color(t::TEXT));
                });
            });
        });
        // kind toggles, 4 across
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 6.0;
            let bw = ((ui_.available_width() - 18.0) / 4.0).floor();
            for kind in DayOffKind::ALL {
                if kind_toggle(ui_, kind, form.kind == kind, bw) {
                    form.kind = kind;
                }
            }
        });
        w::labeled(ui_, "Label (optional)", |ui_| {
            w::input(
                ui_,
                egui::TextEdit::singleline(&mut form.label)
                    .hint_text(RichText::new("e.g. long weekend").color(t::FAINT))
                    .desired_width(ui_.available_width()),
            );
        });
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 10.0;
            ui_.label(RichText::new("Length").font(t::font(12.0, Weight::Medium)).color(t::MUTED));
            w::segmented(
                ui_,
                &mut form.half,
                &[Segment::new(false, "Full day"), Segment::new(true, "Half day")],
                t::INSET,
                28.0,
            );
            ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                let label = if form.range { "Single day" } else { "Range…" };
                if ui_
                    .add(w::ghost(label).min_size(vec2(0.0, 32.0)))
                    .on_hover_text("Mark every weekday between two dates")
                    .clicked()
                {
                    form.range = !form.range;
                    form.range_end = day;
                }
            });
        });
        if form.range {
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 10.0;
                ui_.label(RichText::new("Until").font(t::font(12.0, Weight::Medium)).color(t::MUTED));
                ui_.scope(|ui_| {
                    ui::field_style(ui_);
                    ui::date_picker(app, ui_, "calendar/range_end", &mut form.range_end);
                });
                let n = holidays::weekdays_in(day, form.range_end).len();
                ui_.label(RichText::new(format!("{n} weekday{}", if n == 1 { "" } else { "s" })).size(12.0).color(t::MUTED));
            });
        }
        ui_.add(
            egui::Label::new(
                RichText::new(
                    "Days off clear the “No hours” warning and show up in the weekly summary and timesheet copy.",
                )
                .size(12.0)
                .color(t::MUTED),
            )
            .wrap(),
        );
        let mut save = false;
        let mut delete = false;
        ui_.horizontal(|ui_| {
            ui_.spacing_mut().item_spacing.x = 8.0;
            let n = if form.existing { 3.0 } else { 2.0 };
            let bw = ((ui_.available_width() - 8.0 * (n - 1.0)) / n).floor();
            if ui_.add(w::secondary("Cancel").min_size(vec2(bw, 36.0))).clicked() {
                close = true;
            }
            if form.existing
                && ui_
                    .add(
                        egui::Button::new(RichText::new("Delete").color(t::DANGER).strong())
                            .stroke(Stroke::new(1.0, t::alpha(t::DANGER, 0.5)))
                            .corner_radius(t::R_INPUT)
                            .min_size(vec2(bw, 36.0)),
                    )
                    .clicked()
            {
                delete = true;
            }
            if ui_.add(w::primary("Save day off").min_size(vec2(bw, 36.0))).clicked() {
                save = true;
            }
        });
        let form = owned.clone();
        app.cal_form = owned;
        if save {
            let dates = if form.range {
                holidays::weekdays_in(day, form.range_end)
            } else {
                vec![day]
            };
            let label = form.label.trim();
            let label = (!label.is_empty()).then_some(label);
            match app.db.save_days_off(&dates, form.kind, label, form.half, "manual", true) {
                Ok(n) => app.set_status(format!(
                    "Marked {n} day{} off — {}",
                    if n == 1 { "" } else { "s" },
                    form.kind.label()
                )),
                Err(e) => app.set_status(format!("Save failed: {e}")),
            }
            app.touch();
            close = true;
        }
        if delete {
            match app.db.delete_day_off(day) {
                Ok(()) => app.set_status("Day off removed"),
                Err(e) => app.set_status(format!("Delete failed: {e}")),
            }
            app.touch();
            close = true;
        }
    });
    if close {
        app.cal_selected = None;
    }
}

/// One of the four kind buttons. Returns true when clicked.
fn kind_toggle(ui_: &mut egui::Ui, kind: DayOffKind, selected: bool, width: f32) -> bool {
    let (base, light) = kind_colors(kind);
    let (rect, resp) = ui_.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let p = ui_.painter();
    if selected {
        p.rect_filled(rect, 8, t::alpha(base, 0.16));
        p.rect_stroke(rect, 8, Stroke::new(1.0, t::alpha(base, 0.7)), StrokeKind::Inside);
    } else {
        if resp.hovered() {
            p.rect_filled(rect, 8, t::CONTROL_HOVER);
        }
        p.rect_stroke(rect, 8, Stroke::new(1.0, t::BORDER_STRONG), StrokeKind::Inside);
    }
    let font = if selected { t::font(12.5, Weight::Semibold) } else { t::font(12.5, Weight::Regular) };
    let color = if selected { light } else { t::TEXT_2 };
    let galley = p.layout_no_wrap(kind.label().to_string(), font, color);
    let total = 7.0 + 6.0 + galley.size().x;
    let x0 = rect.center().x - total / 2.0;
    p.rect_filled(Rect::from_center_size(pos2(x0 + 3.5, rect.center().y), vec2(7.0, 7.0)), 2, base);
    p.galley(pos2(x0 + 13.0, rect.center().y - galley.size().y / 2.0), galley, color);
    resp.clicked()
}

fn upcoming_card(app: &mut WorklogApp, ui_: &mut egui::Ui, data: &CalData) {
    side_card(t::BORDER).show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.spacing_mut().item_spacing.y = 12.0;
        ui_.label(RichText::new("Upcoming days off").font(t::font(14.0, Weight::Semibold)).color(t::TEXT));
        if data.upcoming.is_empty() {
            ui_.label(RichText::new("Nothing planned.").size(13.0).color(t::MUTED));
        } else {
            ui_.vertical(|ui_| {
                ui_.spacing_mut().item_spacing.y = 2.0;
                let mut go: Option<NaiveDate> = None;
                for off in &data.upcoming {
                    if upcoming_row(ui_, off) {
                        go = Some(off.date);
                    }
                }
                if let Some(d) = go {
                    select_day(app, d);
                }
            });
        }
        ui_.horizontal_wrapped(|ui_| {
            ui_.spacing_mut().item_spacing = vec2(8.0, 8.0);
            if ui_
                .add(w::secondary(format!("{}  Import Québec holidays", icon::DOWNLOAD_SIMPLE)).min_size(vec2(0.0, 34.0)))
                .clicked()
            {
                open_import(app);
            }
            if ui_
                .add(w::ghost("Office closure…").min_size(vec2(0.0, 34.0)))
                .on_hover_text("A range of weekdays off, type Other, with a label")
                .clicked()
            {
                let start = app.cal_selected.unwrap_or_else(today);
                select_day(app, start);
                app.cal_form = DayOffForm {
                    kind: DayOffKind::Other,
                    label: "Office closure".into(),
                    half: false,
                    range: true,
                    range_end: start + Duration::days(4),
                    existing: false,
                };
            }
        });
    });
}

fn upcoming_row(ui_: &mut egui::Ui, off: &DayOff) -> bool {
    let (base, light) = kind_colors(off.kind);
    let (rect, resp) = ui_.allocate_exact_size(vec2(ui_.available_width(), 52.0), Sense::click());
    let p = ui_.painter();
    if resp.hovered() {
        p.rect_filled(rect, 9, t::HOVER);
    }
    let tile = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 6.0), vec2(40.0, 40.0));
    p.rect_filled(tile, 8, t::alpha(base, 0.12));
    p.text(
        pos2(tile.center().x, tile.top() + 11.0),
        Align2::CENTER_CENTER,
        off.date.format("%b").to_string().to_uppercase(),
        t::font(10.0, Weight::Semibold),
        light,
    );
    p.text(
        pos2(tile.center().x, tile.bottom() - 13.0),
        Align2::CENTER_CENTER,
        off.date.day().to_string(),
        t::mono_semi(15.0),
        t::TEXT,
    );
    let x = tile.right() + 12.0;
    let mut title = off.title();
    if off.half {
        title.push_str(" ½");
    }
    p.text(pos2(x, rect.center().y - 9.0), Align2::LEFT_CENTER, title, t::font(13.5, Weight::Medium), t::TEXT);
    p.text(
        pos2(x, rect.center().y + 10.0),
        Align2::LEFT_CENTER,
        format!("{} · {}", off.date.format("%A"), off.kind.label()),
        t::font(12.0, Weight::Regular),
        t::MUTED,
    );
    resp.clicked()
}

fn glance_card(app: &WorklogApp, ui_: &mut egui::Ui, data: &CalData) {
    let month = app.cal_month;
    let in_month = |d: &NaiveDate| d.month() == month.month() && d.year() == month.year();
    let logged: f64 = data
        .hours
        .iter()
        .filter(|(d, _)| in_month(d))
        .flat_map(|(_, v)| v.iter().map(|(_, h)| *h))
        .sum();
    let missing = data.missing.iter().filter(|d| in_month(d)).count();
    let off: f64 = data
        .days_off
        .values()
        .filter(|o| in_month(&o.date))
        .map(|o| if o.half { 0.5 } else { 1.0 })
        .sum();
    side_card(t::BORDER).show(ui_, |ui_| {
        ui_.set_width(ui_.available_width());
        ui_.columns(3, |cols| {
            stat(&mut cols[0], "Logged", format!("{} h", fmt_hours(logged)), t::TEXT);
            stat(
                &mut cols[1],
                "No hours",
                format!("{missing} d"),
                if missing > 0 { t::AMBER } else { t::TEXT },
            );
            stat(&mut cols[2], "Days off", fmt_hours(off), t::TEXT);
        });
    });
}

fn stat(ui_: &mut egui::Ui, label: &str, value: String, color: Color32) {
    ui_.spacing_mut().item_spacing.y = 4.0;
    ui_.label(RichText::new(label).size(12.0).color(t::MUTED));
    ui_.label(w::mono_semi(value, 20.0).color(color));
}

// ---------------------------------------------------------------- import

/// The Québec holidays preview: pick the year and the Easter holiday,
/// untick what doesn't apply, import (dates that already have a day off
/// are skipped).
pub fn import_dialog(app: &mut WorklogApp, ctx: &egui::Context) {
    let Some(mut imp) = app.qc_import.take() else {
        return;
    };
    let mut keep = true;
    let mut import = false;
    let mut reload: Option<(i32, EasterDay)> = None;
    let modal = egui::Modal::new(egui::Id::new("qc_import"))
        .backdrop_color(Color32::from_black_alpha(150))
        .frame(
            egui::Frame::new()
                .fill(t::PANEL)
                .stroke(Stroke::new(1.0, t::BORDER_STRONG))
                .corner_radius(t::R_DIALOG)
                .inner_margin(Margin::same(24))
                .shadow(ctx.global_style().visuals.window_shadow),
        )
        .show(ctx, |ui_| {
            ui_.set_width(460.0);
            ui_.spacing_mut().item_spacing.y = 14.0;
            ui_.horizontal(|ui_| {
                ui_.label(RichText::new("Import Québec holidays").font(t::font(18.0, Weight::Semibold)).color(t::TEXT));
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    if w::icon_button_ex(ui_, icon::X, "Cancel", 32.0, t::MUTED, true).clicked() {
                        keep = false;
                    }
                });
            });
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 12.0;
                egui::Frame::new()
                    .fill(t::INSET)
                    .stroke(Stroke::new(1.0, t::BORDER_STRONG))
                    .corner_radius(10)
                    .inner_margin(Margin::same(3))
                    .show(ui_, |ui_| {
                        ui_.spacing_mut().item_spacing.x = 4.0;
                        if w::icon_button_ex(ui_, icon::CARET_LEFT, "Previous year", 28.0, t::MUTED, true).clicked() {
                            reload = Some((imp.year - 1, imp.easter));
                        }
                        ui_.label(w::mono_semi(imp.year.to_string(), 14.0).color(t::TEXT));
                        if w::icon_button_ex(ui_, icon::CARET_RIGHT, "Next year", 28.0, t::MUTED, true).clicked() {
                            reload = Some((imp.year + 1, imp.easter));
                        }
                    });
                let mut easter = imp.easter;
                w::segmented(
                    ui_,
                    &mut easter,
                    &[
                        Segment::new(EasterDay::GoodFriday, "Good Friday"),
                        Segment::new(EasterDay::EasterMonday, "Easter Monday"),
                    ],
                    t::INSET,
                    28.0,
                );
                if easter != imp.easter {
                    reload = Some((imp.year, easter));
                }
            });
            ui_.add(
                egui::Label::new(
                    RichText::new(
                        "Your employer gives either Good Friday or Easter Monday. Untick anything that \
                         doesn't apply; dates that already have a day off are left alone.",
                    )
                    .size(12.5)
                    .color(t::MUTED),
                )
                .wrap(),
            );
            w::list_frame().show(ui_, |ui_| {
                ui_.spacing_mut().item_spacing.y = 0.0;
                let n = imp.rows.len();
                for (i, (date, name, checked)) in imp.rows.iter_mut().enumerate() {
                    let (_, resp) = w::list_row(ui_, ("qc_row", i), 40.0, false, |ui_| {
                        ui_.spacing_mut().item_spacing.x = 12.0;
                        if w::checkbox(ui_, *checked, "").clicked() {
                            *checked = !*checked;
                        }
                        ui_.allocate_ui_with_layout(vec2(110.0, 20.0), Layout::left_to_right(Align::Center), |ui_| {
                            ui_.set_width(110.0);
                            ui_.label(w::mono(date.format("%a %b %-d").to_string(), 12.5).color(t::TEXT_2));
                        });
                        ui_.label(RichText::new(*name).size(13.5).color(if *checked { t::TEXT } else { t::MUTED }));
                    });
                    if resp.clicked() {
                        *checked = !*checked;
                    }
                    if i + 1 < n {
                        w::list_divider(ui_);
                    }
                }
            });
            let count = imp.rows.iter().filter(|r| r.2).count();
            ui_.horizontal(|ui_| {
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    ui_.spacing_mut().item_spacing.x = 8.0;
                    let label = format!("Import {count} holiday{}", if count == 1 { "" } else { "s" });
                    if ui_.add_enabled(count > 0, w::primary(label).min_size(vec2(0.0, 36.0))).clicked() {
                        import = true;
                    }
                    if ui_.add(w::secondary("Cancel").min_size(vec2(0.0, 36.0))).clicked() {
                        keep = false;
                    }
                });
            });
        });
    if modal.should_close() {
        keep = false;
    }
    if let Some((year, easter)) = reload {
        imp = import_rows(app, year, easter);
    }
    if import {
        let mut written = 0;
        let mut error = None;
        for (date, name, checked) in &imp.rows {
            if !checked {
                continue;
            }
            match app
                .db
                .save_days_off(&[*date], DayOffKind::Holiday, Some(name), false, "import:qc", false)
            {
                Ok(n) => written += n,
                Err(e) => error = Some(e),
            }
        }
        match error {
            Some(e) => app.set_status(format!("Import failed: {e}")),
            None => app.set_status(format!("Imported {written} Québec holidays for {}", imp.year)),
        }
        app.touch();
        keep = false;
    }
    if keep {
        app.qc_import = Some(imp);
    }
}
