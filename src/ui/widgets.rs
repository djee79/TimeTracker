//! Shared building blocks for the redesigned UI: buttons, segmented
//! controls, switches, badges, cards, list rows and labelled fields.
//! Everything paints with tokens from `theme`.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};

use super::theme::{self as t, Weight};

pub use egui_phosphor::regular as icon;

// ---- text helpers ----

/// Text that may contain icon glyphs: icon characters get the Phosphor
/// family (a touch larger), everything else `font`.
pub fn rich(text: &str, font: egui::FontId, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let icon_font = t::icon_font(font.size + 2.0);
    let mut run = String::new();
    let mut run_is_icon = false;
    let flush = |job: &mut egui::text::LayoutJob, run: &mut String, is_icon: bool| {
        if run.is_empty() {
            return;
        }
        let f = if is_icon { icon_font.clone() } else { font.clone() };
        job.append(
            run,
            0.0,
            egui::TextFormat {
                font_id: f,
                color,
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
        run.clear();
    };
    for c in text.chars() {
        let is_icon = t::is_icon_char(c);
        if is_icon != run_is_icon {
            flush(&mut job, &mut run, run_is_icon);
            run_is_icon = is_icon;
        }
        run.push(c);
    }
    flush(&mut job, &mut run, run_is_icon);
    job
}

/// A label that may contain icons.
pub fn icon_label(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(rich(text, t::font(size, Weight::Regular), color))
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(t::MUTED)
}

pub fn faint(text: impl Into<String>) -> RichText {
    RichText::new(text).color(t::FAINT)
}

pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(t::mono(size))
}

pub fn mono_semi(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(t::mono_semi(size))
}

pub fn semibold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(t::font(size, Weight::Semibold)).color(t::TEXT)
}

pub fn medium(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(t::font(size, Weight::Medium))
}

/// Card / section title (16 px, 600).
pub fn title(ui: &mut Ui, text: &str) -> Response {
    ui.label(semibold(text, 16.0))
}

/// Uppercase 11 px label with letter-spacing ("NOTES", "NEW PROJECT").
pub fn eyebrow(ui: &mut Ui, text: &str) -> Response {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: t::font(11.0, Weight::Semibold),
            color: t::FAINT,
            extra_letter_spacing: 0.9,
            ..Default::default()
        },
    );
    ui.label(job)
}

/// A keyboard hint chip: `Enter`, `Ctrl K`.
pub fn kbd(ui: &mut Ui, text: &str) -> Response {
    Frame::new()
        .fill(t::CONTROL)
        .stroke(Stroke::new(1.0, t::BORDER_STRONG))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| ui.label(mono(text, 11.0).color(t::MUTED)))
        .response
}

// ---- buttons ----

/// Accent-filled call to action.
pub fn primary(text: impl AsRef<str>) -> egui::Button<'static> {
    egui::Button::new(rich(text.as_ref(), t::font(13.0, Weight::Semibold), t::ACCENT_INK))
    .fill(t::ACCENT)
    .stroke(Stroke::NONE)
    .corner_radius(t::R_INPUT)
    .min_size(vec2(0.0, t::H_SMALL))
}

/// The default framed button (`CONTROL` fill, `BORDER_STRONG` stroke).
pub fn secondary(text: impl AsRef<str>) -> egui::Button<'static> {
    egui::Button::new(rich(text.as_ref(), t::font(13.0, Weight::Medium), t::TEXT_2))
        .corner_radius(t::R_INPUT)
        .min_size(vec2(0.0, t::H_SMALL))
}

/// Transparent until hovered.
pub fn ghost(text: impl AsRef<str>) -> egui::Button<'static> {
    egui::Button::new(rich(text.as_ref(), t::font(13.0, Weight::Medium), t::TEXT_2))
        .frame_when_inactive(false)
        .corner_radius(t::R_INPUT)
        .min_size(vec2(0.0, t::H_SMALL))
}

/// Square ghost button holding one icon, with a tooltip.
pub fn icon_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    icon_button_ex(ui, glyph, tip, 28.0, t::MUTED, true)
}

pub fn icon_button_ex(
    ui: &mut Ui,
    glyph: &str,
    tip: &str,
    size: f32,
    color: Color32,
    enabled: bool,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), sense);
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, t::R_ICON, t::CONTROL_HOVER);
        }
        let col = if !enabled {
            t::DISABLED
        } else if hovered {
            t::TEXT
        } else {
            color
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            glyph,
            t::icon_font((size * 0.57).round()),
            col,
        );
    }
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// A round icon button (timer pill): filled or outlined.
pub fn round_button(ui: &mut Ui, glyph: &str, tip: &str, filled: bool) -> Response {
    let size = 26.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let c = rect.center();
    let hovered = resp.hovered();
    if filled {
        let fill = if hovered { t::ACCENT_TEXT } else { t::ACCENT };
        ui.painter().circle_filled(c, size / 2.0, fill);
        ui.painter()
            .text(c, Align2::CENTER_CENTER, glyph, t::icon_font(14.0), t::ACCENT_INK);
    } else {
        if hovered {
            ui.painter().circle_filled(c, size / 2.0, t::alpha(t::ACCENT, 0.18));
        }
        ui.painter()
            .circle_stroke(c, size / 2.0 - 0.5, Stroke::new(1.0, t::alpha(t::ACCENT, 0.6)));
        ui.painter()
            .text(c, Align2::CENTER_CENTER, glyph, t::icon_font(14.0), t::ACCENT_TEXT);
    }
    resp.on_hover_text(tip)
}

// ---- segmented control ----

pub struct Segment<'a, T> {
    pub value: T,
    pub label: &'a str,
    /// Small mono hint after the label (keyboard shortcut).
    pub hint: Option<&'a str>,
}

impl<'a, T> Segment<'a, T> {
    pub fn new(value: T, label: &'a str) -> Self {
        Segment { value, label, hint: None }
    }
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }
}

/// Rounded track with the selected segment raised. Returns true on change.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    segments: &[Segment<'_, T>],
    track: Color32,
    height: f32,
) -> bool {
    let mut changed = false;
    Frame::new()
        .fill(track)
        .stroke(Stroke::new(1.0, t::BORDER_STRONG))
        .corner_radius(10)
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for seg in segments {
                    let selected = *value == seg.value;
                    let (font, color) = if selected {
                        (t::font(13.0, Weight::Semibold), Color32::WHITE)
                    } else {
                        (t::font(13.0, Weight::Medium), t::MUTED)
                    };
                    let galley = ui.painter().layout_no_wrap(seg.label.to_string(), font, color);
                    let hint = seg.hint.map(|h| {
                        ui.painter()
                            .layout_no_wrap(h.to_string(), t::mono(11.0), t::FAINT)
                    });
                    let pad = 14.0;
                    let hint_w = hint.as_ref().map_or(0.0, |h| h.size().x + 8.0);
                    let size = vec2(galley.size().x + hint_w + 2.0 * pad, height);
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    if selected {
                        ui.painter().rect_filled(rect, 7, t::ACTIVE_TAB);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 7, t::alpha(t::ACTIVE_TAB, 0.5));
                    }
                    let text_pos = pos2(rect.left() + pad, rect.center().y - galley.size().y / 2.0);
                    let label_w = galley.size().x;
                    let text_color = if resp.hovered() && !selected { t::TEXT } else { color };
                    ui.painter().galley(text_pos, galley, text_color);
                    if let Some(h) = hint {
                        let p = pos2(text_pos.x + label_w + 8.0, rect.center().y - h.size().y / 2.0);
                        ui.painter().galley(p, h, t::FAINT);
                    }
                    if resp.clicked() && !selected {
                        *value = seg.value;
                        changed = true;
                    }
                }
            });
        });
    changed
}

// ---- toggle switch ----

fn paint_switch(ui: &Ui, rect: Rect, id: egui::Id, on: bool) {
    let how_on = ui.ctx().animate_bool_responsive(id, on);
    let radius = rect.height() / 2.0;
    let track = if on { t::ACCENT } else { t::SWITCH_OFF };
    ui.painter().rect_filled(rect, radius, track);
    let knob_r = 7.0;
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    let knob = if on { t::ACCENT_INK } else { t::TEXT_2 };
    ui.painter()
        .circle_filled(pos2(x, rect.center().y), knob_r, if on { Color32::WHITE } else { knob });
}

/// A 34×20 switch with a label on its right; the whole thing is clickable.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), t::font(13.0, Weight::Medium), t::TEXT_2);
    let gap = if label.is_empty() { 0.0 } else { 8.0 };
    let size = vec2(34.0 + gap + galley.size().x, 22.0_f32.max(galley.size().y));
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let sw = Rect::from_min_size(pos2(rect.left(), rect.center().y - 10.0), vec2(34.0, 20.0));
        paint_switch(ui, sw, resp.id, *on);
        let color = if resp.hovered() { t::TEXT } else { t::TEXT_2 };
        ui.painter().galley(
            pos2(sw.right() + gap, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }
    resp
}

/// A toggle inside a bordered box (the R&D switch in entry forms).
pub fn boxed_toggle(ui: &mut Ui, on: &mut bool, label: &str, tip: &str) -> Response {
    Frame::new()
        .stroke(Stroke::new(1.0, t::BORDER_STRONG))
        .corner_radius(t::R_INPUT)
        .inner_margin(Margin::symmetric(10, 5))
        .show(ui, |ui| toggle(ui, on, label))
        .inner
        .on_hover_text(tip)
}

// ---- checkbox ----

/// Rounded checkbox with an accent fill when checked (task completion and
/// checklist items — settings use `toggle`).
pub fn checkbox(ui: &mut Ui, checked: bool, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    let b = Rect::from_center_size(rect.center(), Vec2::splat(17.0));
    if checked {
        ui.painter().rect_filled(b, 5, t::ACCENT);
        ui.painter().text(
            b.center(),
            Align2::CENTER_CENTER,
            icon::CHECK,
            t::icon_font(13.0),
            t::ACCENT_INK,
        );
    } else {
        let stroke = if resp.hovered() { t::ACCENT } else { t::DISABLED };
        ui.painter().rect_filled(b, 5, t::INSET);
        ui.painter()
            .rect_stroke(b, 5, Stroke::new(1.5, stroke), StrokeKind::Inside);
    }
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

// ---- project identity ----

pub fn dot(ui: &mut Ui, color: Color32, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
    resp
}

/// 8 px dot then the code in mono 600, project light colour.
pub fn project_tag(ui: &mut Ui, code: &str, sw: t::Swatch) {
    ui.spacing_mut().item_spacing.x = 7.0;
    dot(ui, sw.base, 8.0);
    ui.label(mono_semi(code, 13.0).color(sw.light));
}

// ---- badges / pills ----

pub fn pill(
    ui: &mut Ui,
    text: RichText,
    fill: Color32,
    stroke: Color32,
    radius: u8,
) -> Response {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(radius)
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| ui.label(text))
        .response
}

pub fn rd_badge(ui: &mut Ui) -> Response {
    pill(
        ui,
        RichText::new("R&D").font(t::font(11.0, Weight::Semibold)).color(t::AMBER),
        t::alpha(t::AMBER, 0.12),
        t::alpha(t::AMBER, 0.32),
        6,
    )
    .on_hover_text("Flagged for the annual SR&ED / R&D export")
}

pub fn client_tag(ui: &mut Ui, client: &str) -> Response {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &client.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: t::font(10.5, Weight::Semibold),
            color: t::MUTED,
            extra_letter_spacing: 0.5,
            ..Default::default()
        },
    );
    Frame::new()
        .stroke(Stroke::new(1.0, t::SWITCH_OFF))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| ui.label(job))
        .response
}

pub fn count_pill(ui: &mut Ui, n: usize) -> Response {
    pill(
        ui,
        mono(n.to_string(), 11.0).color(t::MUTED),
        t::COUNT_PILL,
        Color32::TRANSPARENT,
        t::R_PILL,
    )
}

pub fn today_badge(ui: &mut Ui) -> Response {
    pill(
        ui,
        RichText::new("Today")
            .font(t::font(11.0, Weight::Semibold))
            .color(t::ACCENT_TEXT),
        t::accent_tint(),
        Color32::TRANSPARENT,
        t::R_PILL,
    )
}

pub fn warn_pill(ui: &mut Ui, text: &str) -> Response {
    Frame::new()
        .fill(t::alpha(t::AMBER, 0.10))
        .stroke(Stroke::new(1.0, t::alpha(t::AMBER, 0.32)))
        .corner_radius(t::R_PILL)
        .inner_margin(Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.label(rich(
                &format!("{} {text}", icon::WARNING),
                t::font(12.0, Weight::Medium),
                t::AMBER,
            ))
        })
        .response
}

// ---- containers ----

/// `PANEL` card, radius 14.
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(t::PANEL)
        .stroke(Stroke::new(1.0, t::BORDER))
        .corner_radius(t::R_CARD)
        .inner_margin(Margin::same(t::CARD_PAD))
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame().show(ui, add).inner
}

/// Card whose rows touch its edges (list groups, tables).
pub fn list_frame() -> Frame {
    Frame::new()
        .fill(t::PANEL)
        .stroke(Stroke::new(1.0, t::BORDER))
        .corner_radius(t::R_LIST)
        .inner_margin(Margin::same(1))
}

/// One row of a list group: hover tint, or `SELECTED_ROW` with a 3 px accent
/// bar. The row's content is laid out left-to-right, vertically centred.
/// Returns the content's result and a hover/click response for the row.
pub fn list_row<R>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    height: f32,
    selected: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let bg = ui.painter().add(Shape::Noop);
    let inner = ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(14.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        add,
    );
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    if selected {
        ui.painter()
            .set(bg, Shape::rect_filled(rect, 0, t::SELECTED_ROW));
        let bar = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
        ui.painter().rect_filled(bar, 0, t::ACCENT);
    } else if hovered {
        ui.painter().set(bg, Shape::rect_filled(rect, 0, t::HOVER));
    }
    (inner.inner, resp)
}

/// 1 px separator between list rows.
pub fn list_divider(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, t::LIST_DIVIDER);
}

/// A hairline that fills the remaining width at mid-height (day headers).
pub fn hairline(ui: &mut Ui, reserve_right: f32) {
    let w = (ui.available_width() - reserve_right).max(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, t::DIVIDER));
}

/// A 12 px muted label above whatever `add` draws.
pub fn labeled<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        ui.label(RichText::new(label).font(t::font(12.0, Weight::Medium)).color(t::MUTED));
        add(ui)
    })
    .inner
}

/// A styled single/multi-line input: roomier margins and the accent focus
/// glow on top of the theme's focus stroke.
pub fn input(ui: &mut Ui, edit: egui::TextEdit<'_>) -> Response {
    let resp = ui.add(edit.margin(Margin::symmetric(10, 7)));
    focus_glow(ui, &resp);
    resp
}

pub fn focus_glow(ui: &Ui, resp: &Response) {
    if resp.has_focus() {
        ui.painter().rect_stroke(
            resp.rect.expand(1.0),
            CornerRadius::same(t::R_INPUT + 2),
            Stroke::new(3.0, t::alpha(t::ACCENT, 0.22)),
            StrokeKind::Outside,
        );
    }
}

/// Dashed outline (empty states, "+ Add an item").
pub fn dashed_rect(ui: &Ui, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let pts = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    ui.painter()
        .extend(Shape::dashed_line(&pts, stroke, 4.0, 3.0));
}

/// Thin rounded progress bar.
pub fn progress(ui: &mut Ui, frac: f32, width: f32, height: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let r = (height / 2.0).round() as u8;
    ui.painter().rect_filled(rect, r, t::COUNT_PILL);
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    if fill.width() > 0.0 {
        ui.painter().rect_filled(fill, r, color);
    }
    resp
}
