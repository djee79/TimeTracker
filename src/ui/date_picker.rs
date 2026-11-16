//! The app's own date picker (replaces `egui_extras::DatePickerButton`,
//! whose weekend colour is hard-coded red and whose combo-box layout
//! doesn't fit). A trigger button opens a popover with a month grid, ISO
//! week numbers, quick picks and a marker dot per day; one click picks.

use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use egui::{
    Align2, Color32, CornerRadius, Frame, Id, Key, Margin, Modifiers, Order, Pos2, Rect, Response,
    RichText, Sense, Shadow, Stroke, StrokeKind, Ui, pos2, vec2,
};

use super::theme::{self as t, Weight};
use super::widgets::{self as w, icon};
use crate::db::DayOffKind;

/// Marker colour per day, for one visible month (6 weeks).
pub type DayMarkers = HashMap<NaiveDate, Color32>;

/// Supplies the markers for the month starting at the given date.
pub type MarkerSource<'a> = dyn FnMut(NaiveDate) -> Rc<DayMarkers> + 'a;

const POPOVER_W: f32 = 316.0;
const PAD: f32 = 12.0;
const INNER_W: f32 = POPOVER_W - 2.0 * PAD - 2.0;
const WEEK_COL: f32 = 26.0;
const GAP: f32 = 2.0;
const HEAD_H: f32 = 24.0;
const CELL_H: f32 = 36.0;
/// Rough popover height, to decide whether it fits below the trigger.
const EST_H: f32 = 400.0;

/// Which picker is open (only one at a time) and the pass it was last drawn.
const OPEN_KEY: &str = "date_picker/open";

#[derive(Clone, Copy)]
struct State {
    /// First day of the month on screen.
    view: NaiveDate,
    /// Keyboard focus (starts on the selected date).
    focus: NaiveDate,
    months: bool,
}

// ---------------------------------------------------------------- date maths

pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// Same day in a month `n` months away, clamped to that month's length.
pub fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + n;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    (1..=d.day())
        .rev()
        .find_map(|day| NaiveDate::from_ymd_opt(y, m, day))
        .unwrap_or(d)
}

/// The Monday that starts the 6-week grid for a month.
pub fn grid_start(month: NaiveDate) -> NaiveDate {
    let first = month_start(month);
    first - Duration::days(first.weekday().num_days_from_monday() as i64)
}

/// The most recent Friday strictly before `today` (a week back on Fridays).
pub fn last_friday(today: NaiveDate) -> NaiveDate {
    let back = (today.weekday().num_days_from_monday() as i64 - 4).rem_euclid(7);
    today - Duration::days(if back == 0 { 7 } else { back })
}

/// "Today, Nov 15", "Thu, Nov 12", or "Nov 12, 2025" outside this year.
pub fn trigger_label(d: NaiveDate, today: NaiveDate) -> String {
    if d == today {
        d.format("Today, %b %-d").to_string()
    } else if d.year() == today.year() {
        d.format("%a, %b %-d").to_string()
    } else {
        d.format("%b %-d, %Y").to_string()
    }
}

/// What a day's dot shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marker {
    /// The project with the most hours that day.
    Project(i64),
    DayOff(DayOffKind),
    /// A past weekday with no hours and no day off.
    Missing,
}

/// Logged work wins over a day off, which wins over the "no hours" warning.
/// `hours` is in report order, so ties go to the project listed first.
pub fn marker(hours: &[(i64, f64)], off: Option<DayOffKind>, missing: bool) -> Option<Marker> {
    let mut best: Option<(i64, f64)> = None;
    for &(pid, h) in hours {
        if h > 0.0 && best.is_none_or(|(_, b)| h > b) {
            best = Some((pid, h));
        }
    }
    if let Some((pid, _)) = best {
        Some(Marker::Project(pid))
    } else if let Some(kind) = off {
        Some(Marker::DayOff(kind))
    } else if missing {
        Some(Marker::Missing)
    } else {
        None
    }
}

// ---------------------------------------------------------------- open state

fn open_id(ctx: &egui::Context) -> Option<(Id, u64)> {
    ctx.data(|d| d.get_temp::<(Id, u64)>(Id::new(OPEN_KEY)))
}

fn set_open(ctx: &egui::Context, id: Option<Id>) {
    let pass = ctx.cumulative_pass_nr(); // outside the lock below
    ctx.data_mut(|d| match id {
        Some(id) => {
            d.insert_temp(Id::new(OPEN_KEY), (id, pass));
        }
        None => d.remove::<(Id, u64)>(Id::new(OPEN_KEY)),
    });
}

/// Call once per frame before any UI: Esc closes an open picker (and is
/// consumed, so forms behind it don't also close). A picker whose trigger
/// wasn't drawn last frame (tab switched, form closed) is forgotten.
/// Returns true while a picker is open, so bare-key shortcuts can stand down.
pub fn begin_frame(ctx: &egui::Context) -> bool {
    let Some((_, drawn)) = open_id(ctx) else {
        return false;
    };
    if drawn + 1 < ctx.cumulative_pass_nr() {
        set_open(ctx, None);
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        set_open(ctx, None);
        return false;
    }
    true
}

/// Open the picker with this id salt (debug screenshots).
#[cfg(debug_assertions)]
pub fn open(ctx: &egui::Context, id_salt: &str, date: NaiveDate, months: bool) {
    let id = Id::new(id_salt);
    set_open(ctx, Some(id));
    ctx.data_mut(|d| {
        d.insert_temp(id, State { view: month_start(date), focus: date, months })
    });
}

// ---------------------------------------------------------------- widget

/// The date button plus its popover. `label` overrides the trigger text
/// (Reports shows the week range); the caller styles the trigger's frame.
/// Returns true when the date changed.
pub fn show(
    ui: &mut Ui,
    id_salt: &str,
    date: &mut NaiveDate,
    label: Option<String>,
    markers: &mut MarkerSource<'_>,
) -> bool {
    let id = Id::new(id_salt);
    let ctx = ui.ctx().clone();
    let today = crate::app::today();
    let is_open = open_id(&ctx).is_some_and(|(o, _)| o == id);
    let mut state = ctx
        .data(|d| d.get_temp::<State>(id))
        .unwrap_or(State { view: month_start(*date), focus: *date, months: false });

    // Keys first, so the focused trigger doesn't also see Enter.
    let mut picked: Option<NaiveDate> = None;
    if is_open {
        handle_keys(ui, &mut state, &mut picked);
    }

    let text = label.unwrap_or_else(|| trigger_label(*date, today));
    let trigger = ui.add(egui::Button::new(format!("      {text}")));
    ui.painter().text(
        pos2(trigger.rect.left() + 13.0, trigger.rect.center().y),
        Align2::LEFT_CENTER,
        icon::CALENDAR_BLANK,
        t::icon_font(15.0),
        if is_open { t::ACCENT_TEXT } else { t::MUTED },
    );

    let mut close = false;
    if trigger.clicked() {
        if is_open {
            close = true;
        } else {
            state = State { view: month_start(*date), focus: *date, months: false };
            set_open(&ctx, Some(id));
        }
    }
    let is_open = open_id(&ctx).is_some_and(|(o, _)| o == id) && !close;

    if is_open && picked.is_none() {
        ring(ui, trigger.rect);
        set_open(&ctx, Some(id)); // refresh "drawn this pass"
        let below = trigger.rect.left_bottom() + vec2(0.0, 6.0);
        let screen = ctx.content_rect();
        let (pos, pivot) = if below.y + EST_H > screen.bottom() && trigger.rect.top() - EST_H > screen.top() {
            (trigger.rect.left_top() - vec2(0.0, 6.0), Align2::LEFT_BOTTOM)
        } else {
            (below, Align2::LEFT_TOP)
        };
        let marks = markers(state.view);
        let area = egui::Area::new(id.with("popover"))
            .order(Order::Foreground)
            .fixed_pos(pos)
            .pivot(pivot)
            .constrain(true)
            .show(&ctx, |ui| {
                popover_frame().show(ui, |ui| {
                    ui.set_width(INNER_W);
                    if state.months {
                        month_view(ui, &mut state, *date);
                    } else {
                        day_view(ui, &mut state, *date, today, &marks, &mut picked);
                    }
                });
            });
        // outside click closes (the trigger toggles itself above)
        let pressed_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p) && !trigger.rect.contains(p))
        });
        if pressed_outside {
            close = true;
        }
    }

    let mut changed = false;
    if let Some(d) = picked {
        changed = d != *date;
        *date = d;
        close = true;
    }
    if close {
        set_open(&ctx, None);
        ctx.data_mut(|d| d.remove::<State>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, state));
    }
    changed
}

/// Accent focus ring around the trigger while open.
fn ring(ui: &Ui, rect: Rect) {
    ui.painter().rect_stroke(rect, t::R_INPUT, Stroke::new(1.0, t::ACCENT), StrokeKind::Inside);
    ui.painter().rect_stroke(
        rect.expand(1.0),
        CornerRadius::same(t::R_INPUT + 2),
        Stroke::new(3.0, t::alpha(t::ACCENT, 0.22)),
        StrokeKind::Outside,
    );
}

fn popover_frame() -> Frame {
    Frame::new()
        .fill(t::PANEL)
        .stroke(Stroke::new(1.0, t::BORDER_STRONG))
        .corner_radius(t::R_CARD)
        .inner_margin(Margin::same(PAD as i8))
        .shadow(Shadow {
            offset: [0, 18],
            blur: 48,
            spread: 0,
            color: Color32::from_black_alpha(140),
        })
}

fn handle_keys(ui: &mut Ui, state: &mut State, picked: &mut Option<NaiveDate>) {
    if state.months {
        return;
    }
    let mut f = state.focus;
    ui.input_mut(|i| {
        let mut key = |k| i.consume_key(Modifiers::NONE, k);
        if key(Key::ArrowLeft) {
            f -= Duration::days(1);
        }
        if key(Key::ArrowRight) {
            f += Duration::days(1);
        }
        if key(Key::ArrowUp) {
            f -= Duration::days(7);
        }
        if key(Key::ArrowDown) {
            f += Duration::days(7);
        }
        if key(Key::PageUp) {
            f = add_months(f, -1);
        }
        if key(Key::PageDown) {
            f = add_months(f, 1);
        }
        if key(Key::Home) {
            f -= Duration::days(f.weekday().num_days_from_monday() as i64);
        }
        if key(Key::End) {
            f += Duration::days(6 - f.weekday().num_days_from_monday() as i64);
        }
        if key(Key::Enter) {
            *picked = Some(f);
        }
    });
    if f != state.focus {
        state.focus = f;
        state.view = month_start(f);
    }
}

/// Ghost button around a prepared layout job (the month/year title).
fn title_button(ui: &mut Ui, job: egui::text::LayoutJob) -> Response {
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let size = vec2(galley.size().x + 16.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, t::R_ICON, t::CONTROL_HOVER);
    }
    let pos = pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, t::TEXT);
    resp
}

fn title_job(main: &str, year: Option<&str>, caret: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let fmt = |font, color| egui::TextFormat { font_id: font, color, valign: egui::Align::Center, ..Default::default() };
    job.append(main, 0.0, fmt(t::font(14.0, Weight::Semibold), t::TEXT));
    if let Some(y) = year {
        job.append(y, 5.0, fmt(t::font(14.0, Weight::Medium), t::MUTED));
    }
    job.append(caret, 6.0, fmt(t::icon_font(13.0), t::OTHER_OFF));
    job
}

/// Title on the left, ‹ › on the right. Returns (title clicked, step).
fn header(ui: &mut Ui, job: egui::text::LayoutJob, prev: &str, next: &str) -> (bool, i32) {
    let mut out = (false, 0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        out.0 = title_button(ui, job).clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button_ex(ui, icon::CARET_RIGHT, next, 32.0, t::WEEKEND_TEXT, true).clicked() {
                out.1 = 1;
            }
            if w::icon_button_ex(ui, icon::CARET_LEFT, prev, 32.0, t::WEEKEND_TEXT, true).clicked() {
                out.1 = -1;
            }
        });
    });
    out
}

fn day_view(
    ui: &mut Ui,
    state: &mut State,
    selected: NaiveDate,
    today: NaiveDate,
    marks: &DayMarkers,
    picked: &mut Option<NaiveDate>,
) {
    ui.spacing_mut().item_spacing.y = 10.0;
    let year = state.view.year().to_string();
    let month = state.view.format("%B").to_string();
    let (title, step) = header(ui, title_job(&month, Some(&year), icon::CARET_DOWN), "Previous month", "Next month");
    if title {
        state.months = true;
    }
    if step != 0 {
        state.view = add_months(state.view, step);
        state.focus = add_months(state.focus, step);
    }

    // quick picks
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.spacing_mut().button_padding = vec2(10.0, 0.0);
        for (label, day) in [
            ("Today", today),
            ("Yesterday", today - Duration::days(1)),
            ("Last Friday", last_friday(today)),
        ] {
            let b = egui::Button::new(RichText::new(label).size(12.0).color(t::TEXT_SOFT))
                .fill(t::CONTROL)
                .stroke(Stroke::new(1.0, t::BORDER_STRONG))
                .corner_radius(7)
                .min_size(vec2(0.0, 28.0));
            if ui.add(b).on_hover_text(day.format("%a, %b %-d").to_string()).clicked() {
                *picked = Some(day);
            }
        }
    });

    day_grid(ui, state, selected, today, marks, picked);

    // footer
    footer(ui, |ui| {
        for (color, text) in [(t::PALETTE[0].base, "logged"), (t::AMBER, "no hours")] {
            ui.spacing_mut().item_spacing.x = 5.0;
            w::dot(ui, color, 5.0);
            ui.label(RichText::new(text).size(11.5).color(t::MUTED));
            ui.add_space(7.0);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new("close").size(11.5).color(t::MUTED));
            w::kbd(ui, "Esc");
            ui.add_space(8.0);
            ui.label(RichText::new("pick").size(11.5).color(t::MUTED));
            w::kbd(ui, "↵");
        });
    });
}

fn footer(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let top = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        top,
        Stroke::new(1.0, t::DIVIDER),
    );
    ui.add_space(0.0);
    Frame::new().inner_margin(Margin { left: 4, right: 4, top: 0, bottom: 2 }).show(ui, |ui| {
        ui.horizontal(|ui| add(ui));
    });
}

fn day_grid(
    ui: &mut Ui,
    state: &mut State,
    selected: NaiveDate,
    today: NaiveDate,
    marks: &DayMarkers,
    picked: &mut Option<NaiveDate>,
) {
    let col_w = (INNER_W - WEEK_COL - 7.0 * GAP) / 7.0;
    let height = HEAD_H + 6.0 * (CELL_H + GAP);
    let (rect, _) = ui.allocate_exact_size(vec2(INNER_W, height), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let col_x = |c: usize| rect.left() + WEEK_COL + GAP + c as f32 * (col_w + GAP);

    // headings
    let head_y = rect.top() + HEAD_H / 2.0;
    painter.text(
        pos2(rect.left() + WEEK_COL / 2.0, head_y),
        Align2::CENTER_CENTER,
        "wk",
        t::mono(10.5),
        t::FAINT,
    );
    for (c, name) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
        painter.text(
            pos2(col_x(c) + col_w / 2.0, head_y),
            Align2::CENTER_CENTER,
            *name,
            t::font(11.5, Weight::Medium),
            if c >= 5 { t::FAINT } else { t::MUTED },
        );
    }

    let start = grid_start(state.view);
    for row in 0..6 {
        let top = rect.top() + HEAD_H + GAP + row as f32 * (CELL_H + GAP);
        let monday = start + Duration::days(row as i64 * 7);
        painter.text(
            pos2(rect.left() + WEEK_COL / 2.0, top + CELL_H / 2.0 - 3.0), // level with the day numbers
            Align2::CENTER_CENTER,
            monday.iso_week().week().to_string(),
            t::mono(10.5),
            t::FAINT,
        );
        for c in 0..7 {
            let day = monday + Duration::days(c as i64);
            let cell = Rect::from_min_size(pos2(col_x(c), top), vec2(col_w, CELL_H));
            let resp = ui.interact(cell, ui.id().with(("day", day)), Sense::click());
            if resp.clicked() {
                *picked = Some(day);
            }
            let in_month = day.month() == state.view.month();
            let weekend = matches!(day.weekday(), Weekday::Sat | Weekday::Sun);
            let is_sel = day == selected;
            let (fill, text, font) = if is_sel {
                (t::ACCENT, t::ACCENT_INK, t::font(13.0, Weight::Bold))
            } else {
                let fill = if resp.hovered() || day == state.focus && state.focus != selected {
                    t::CONTROL_HOVER
                } else {
                    Color32::TRANSPARENT
                };
                let text = if !in_month {
                    t::FAINT
                } else if weekend {
                    t::WEEKEND_TEXT
                } else {
                    t::TEXT
                };
                (fill, text, t::font(13.0, Weight::Regular))
            };
            painter.rect_filled(cell, t::R_ICON, fill);
            if day == today && !is_sel {
                painter.rect_stroke(cell, t::R_ICON, Stroke::new(1.5, t::ACCENT), StrokeKind::Inside);
            }
            let center = cell.center();
            painter.text(pos2(center.x, center.y - 3.0), Align2::CENTER_CENTER, day.day().to_string(), font, text);
            if in_month && let Some(&color) = marks.get(&day) {
                let color = if is_sel { t::ACCENT_INK } else { color };
                painter.circle_filled(pos2(center.x, center.y + 9.0), 2.0, color);
            }
        }
    }
}

fn month_view(ui: &mut Ui, state: &mut State, selected: NaiveDate) {
    ui.spacing_mut().item_spacing.y = 10.0;
    let year = state.view.year().to_string();
    let (title, step) = header(ui, title_job(&year, None, icon::CARET_UP), "Previous year", "Next year");
    if title {
        state.months = false;
    }
    if step != 0 {
        state.view = add_months(state.view, 12 * step);
        state.focus = add_months(state.focus, 12 * step);
    }
    let gap = 6.0;
    let cw = (INNER_W - 2.0 * gap) / 3.0;
    let (rect, _) = ui.allocate_exact_size(vec2(INNER_W, 4.0 * 44.0 + 3.0 * gap), Sense::hover());
    let painter = ui.painter_at(rect);
    for m in 0..12u32 {
        let (r, c) = ((m / 3) as f32, (m % 3) as f32);
        let cell = Rect::from_min_size(
            Pos2::new(rect.left() + c * (cw + gap), rect.top() + r * (44.0 + gap)),
            vec2(cw, 44.0),
        );
        let resp = ui.interact(cell, ui.id().with(("month", m)), Sense::click());
        let first = NaiveDate::from_ymd_opt(state.view.year(), m + 1, 1).unwrap_or(state.view);
        let is_sel = selected.year() == first.year() && selected.month() == first.month();
        let (fill, color, font) = if is_sel {
            (t::ACCENT, t::ACCENT_INK, t::font(13.0, Weight::Bold))
        } else if resp.hovered() {
            (t::CONTROL_HOVER, t::TEXT, t::font(13.0, Weight::Regular))
        } else {
            (Color32::TRANSPARENT, t::TEXT, t::font(13.0, Weight::Regular))
        };
        painter.rect_filled(cell, 9, fill);
        painter.text(cell.center(), Align2::CENTER_CENTER, first.format("%b").to_string(), font, color);
        if resp.clicked() {
            state.view = first;
            state.focus = add_months(state.focus, (first.year() - state.focus.year()) * 12 + m as i32 - state.focus.month0() as i32);
            state.months = false;
        }
    }
    footer(ui, |ui| {
        ui.label(RichText::new("Pick a month to go back to days").size(11.5).color(t::MUTED));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn grid_starts_on_monday_before_the_first() {
        assert_eq!(grid_start(d(2026, 11, 15)), d(2026, 10, 26));
        // a month starting on Monday starts the grid on its first
        assert_eq!(grid_start(d(2026, 6, 20)), d(2026, 6, 1));
    }

    #[test]
    fn iso_weeks_match_mockup() {
        assert_eq!(d(2026, 10, 26).iso_week().week(), 44);
        assert_eq!(d(2026, 11, 30).iso_week().week(), 49);
        // ISO week 1 can start in December
        assert_eq!(d(2025, 12, 29).iso_week().week(), 1);
    }

    #[test]
    fn last_friday_is_strictly_before_today() {
        assert_eq!(last_friday(d(2026, 11, 15)), d(2026, 11, 13)); // Sunday
        assert_eq!(last_friday(d(2026, 11, 16)), d(2026, 11, 13)); // Monday
        assert_eq!(last_friday(d(2026, 11, 14)), d(2026, 11, 13)); // Saturday
        assert_eq!(last_friday(d(2026, 11, 13)), d(2026, 11, 6)); // Friday
    }

    #[test]
    fn add_months_clamps_and_wraps() {
        assert_eq!(add_months(d(2026, 1, 31), 1), d(2026, 2, 28));
        assert_eq!(add_months(d(2026, 12, 15), 1), d(2027, 1, 15));
        assert_eq!(add_months(d(2026, 1, 15), -1), d(2025, 12, 15));
        assert_eq!(add_months(d(2024, 2, 29), 12), d(2025, 2, 28));
    }

    #[test]
    fn trigger_labels() {
        let today = d(2026, 11, 15);
        assert_eq!(trigger_label(today, today), "Today, Nov 15");
        assert_eq!(trigger_label(d(2026, 11, 12), today), "Thu, Nov 12");
        assert_eq!(trigger_label(d(2025, 11, 12), today), "Nov 12, 2025");
    }

    #[test]
    fn marker_priority() {
        // most hours wins; ties go to the first (report order)
        assert_eq!(marker(&[(1, 2.0), (2, 5.0)], None, false), Some(Marker::Project(2)));
        assert_eq!(marker(&[(1, 3.0), (2, 3.0)], None, false), Some(Marker::Project(1)));
        // logged work beats a day off, a day off beats "no hours"
        assert_eq!(
            marker(&[(1, 1.0)], Some(DayOffKind::Holiday), false),
            Some(Marker::Project(1))
        );
        assert_eq!(
            marker(&[], Some(DayOffKind::Vacation), true),
            Some(Marker::DayOff(DayOffKind::Vacation))
        );
        assert_eq!(marker(&[(1, 0.0)], None, true), Some(Marker::Missing));
        assert_eq!(marker(&[], None, false), None);
    }
}
