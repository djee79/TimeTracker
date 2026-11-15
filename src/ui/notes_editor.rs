//! The notes editor window: Edit / Split / Preview over one task's
//! markdown, a formatting toolbar, a source pane with line numbers and
//! syntax colouring, and a live preview whose checkboxes toggle the
//! source line.

use std::sync::Arc;

use egui::{Align, Layout, Margin, RichText, Sense, Stroke, vec2};

use crate::app::{NoteMode, TaskDetails, WorklogApp};
use crate::ui::theme::{self as t, Weight};
use crate::ui::widgets::{self as w, Segment, icon};

const EDITOR_ID: &str = "task_details_editor";
const LINE_H: f32 = 26.0;

/// A formatting action from the notes toolbar.
enum Md {
    /// Wrap the selection in a symmetric marker (`**`, `*`, `~~`, `` ` ``).
    Wrap(&'static str),
    /// Toggle a prefix on every selected line (`# `, `- `, `> `, `- [ ] `).
    LinePrefix(&'static str),
    /// Number the selected lines `1. 2. 3.` (or strip the numbers).
    Numbered,
    CodeBlock,
    Rule,
    Table,
    /// Flip (or add) the checkbox on the cursor's line — handled by the
    /// caller, since it works on the whole line rather than the selection.
    ToggleCheck,
}

fn byte_of(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

/// Byte length of a `N. ` list prefix, if the line has one.
fn numbered_prefix(line: &str) -> Option<usize> {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    (digits > 0 && line[digits..].starts_with(". ")).then_some(digits + 2)
}

/// Apply a toolbar action to `text` at the (sorted, char-indexed) selection.
/// Wrap toggles off when the markers are already there; line prefixes toggle
/// per block, and a heading replaces whatever heading level was in place.
/// Returns the new selection, again in chars.
fn apply_md(text: &mut String, sel: (usize, usize), md: &Md) -> (usize, usize) {
    let (a, b) = sel;
    let (ab, bb) = (byte_of(text, a), byte_of(text, b));
    // The whole lines the selection touches — what line-based actions edit.
    let line_start = text[..ab].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[bb..].find('\n').map(|i| bb + i).unwrap_or(text.len());
    let select_block = |text: &String, new_block: &str| {
        let s = text[..line_start].chars().count();
        (s, s + new_block.chars().count())
    };
    match md {
        Md::ToggleCheck => sel,
        Md::Wrap(m) => {
            let ml = m.chars().count();
            let selected = text[ab..bb].to_string();
            if selected.len() >= 2 * m.len() && selected.starts_with(m) && selected.ends_with(m) {
                let inner = selected[m.len()..selected.len() - m.len()].to_string();
                text.replace_range(ab..bb, &inner);
                (a, b - 2 * ml)
            } else if text[..ab].ends_with(m) && text[bb..].starts_with(m) {
                text.replace_range(bb..bb + m.len(), "");
                text.replace_range(ab - m.len()..ab, "");
                (a - ml, b - ml)
            } else {
                text.insert_str(bb, m);
                text.insert_str(ab, m);
                (a + ml, b + ml)
            }
        }
        Md::LinePrefix(p) => {
            let lines: Vec<String> =
                text[line_start..line_end].split('\n').map(String::from).collect();
            let heading = p.starts_with('#');
            let all_have = lines.iter().all(|l| l.starts_with(p));
            let new_block = lines
                .iter()
                .map(|l| {
                    if all_have {
                        l[p.len()..].to_string()
                    } else if heading {
                        let bare = l.trim_start_matches('#');
                        let bare = bare.strip_prefix(' ').unwrap_or(bare);
                        format!("{p}{bare}")
                    } else {
                        format!("{p}{l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            text.replace_range(line_start..line_end, &new_block);
            select_block(text, &new_block)
        }
        Md::Numbered => {
            let lines: Vec<String> =
                text[line_start..line_end].split('\n').map(String::from).collect();
            let all_have = lines.iter().all(|l| numbered_prefix(l).is_some());
            let new_block = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    if all_have {
                        l[numbered_prefix(l).unwrap()..].to_string()
                    } else {
                        format!("{}. {l}", i + 1)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            text.replace_range(line_start..line_end, &new_block);
            select_block(text, &new_block)
        }
        Md::CodeBlock => {
            let new_block = format!("```\n{}\n```", &text[line_start..line_end]);
            text.replace_range(line_start..line_end, &new_block);
            select_block(text, &new_block)
        }
        Md::Rule => {
            text.insert_str(line_end, "\n\n---\n");
            let p = text[..line_end].chars().count() + 6;
            (p, p)
        }
        Md::Table => {
            let t = "\n\n| Col 1 | Col 2 |\n| --- | --- |\n|  |  |\n";
            text.insert_str(line_end, t);
            let p = text[..line_end].chars().count() + t.chars().count();
            (p, p)
        }
    }
}


// ---- cursor context ----

/// What kind of block the cursor's line is in — lights up the matching
/// toolbar button.
#[derive(Debug, Default, PartialEq, Eq)]
struct LineKind {
    heading: bool,
    bullet: bool,
    numbered: bool,
    checklist: bool,
    quote: bool,
    code_block: bool,
}

fn line_kind(text: &str, line: usize) -> LineKind {
    let mut in_code = false;
    for (i, l) in text.lines().enumerate() {
        let fence = l.trim_start().starts_with("```");
        if i == line {
            let tl = l.trim_start();
            let checklist = !crate::ui::notes::checklist_items(l).is_empty();
            return LineKind {
                heading: tl.starts_with('#'),
                bullet: !checklist && (tl.starts_with("- ") || tl.starts_with("* ")),
                numbered: numbered_prefix(tl).is_some(),
                checklist,
                quote: tl.starts_with('>'),
                code_block: in_code || fence,
            };
        }
        if fence {
            in_code = !in_code;
        }
    }
    LineKind { code_block: in_code, ..Default::default() }
}

/// Line index of a char offset.
fn line_of(text: &str, char_idx: usize) -> usize {
    text.chars().take(char_idx).filter(|&c| c == '\n').count()
}

/// Ctrl+Enter: flip the checkbox on `line`, or turn the line into one.
fn toggle_line_checkbox(text: &str, line: usize) -> String {
    let current = text.lines().nth(line).unwrap_or("");
    if !crate::ui::notes::checklist_items(current).is_empty() {
        return crate::ui::notes::toggle_item(text, line);
    }
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    if out.is_empty() {
        out.push(String::new());
    }
    if let Some(l) = out.get_mut(line) {
        let indent = l.len() - l.trim_start().len();
        let body = l[indent..]
            .strip_prefix("- ")
            .or_else(|| l[indent..].strip_prefix("* "))
            .unwrap_or(&l[indent..])
            .to_string();
        *l = format!("{}- [ ] {body}", &l[..indent]);
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

// ---- syntax colouring ----

fn fmt(font: egui::FontId, color: egui::Color32) -> egui::TextFormat {
    egui::TextFormat {
        font_id: font,
        color,
        line_height: Some(LINE_H),
        valign: Align::Center,
        ..Default::default()
    }
}

/// The source pane's colouring: list markers faint, checkboxes accent,
/// headings and **bold** in 600, `code` in accent text.
pub fn highlight(text: &str) -> egui::text::LayoutJob {
    let size = 13.5;
    let normal = fmt(t::mono(size), t::TEXT);
    let mut job = egui::text::LayoutJob::default();
    let mut in_fence = false;
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            job.append("\n", 0.0, normal.clone());
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            job.append(line, 0.0, fmt(t::mono(size), t::FAINT));
            continue;
        }
        if in_fence {
            job.append(line, 0.0, fmt(t::mono(size), t::ACCENT_TEXT));
            continue;
        }
        if trimmed.starts_with('#') {
            job.append(line, 0.0, fmt(t::mono_semi(size), t::TEXT));
            continue;
        }
        // leading list marker (and checkbox)
        let indent = line.len() - trimmed.len();
        let mut rest = line;
        let marker_len = if ["- ", "* ", "+ ", "> "].iter().any(|m| trimmed.starts_with(m)) {
            2
        } else {
            numbered_prefix(trimmed).unwrap_or(0)
        };
        if marker_len > 0 {
            job.append(&line[..indent + marker_len], 0.0, fmt(t::mono(size), t::FAINT));
            rest = &line[indent + marker_len..];
            for b in ["[ ]", "[x]", "[X]"] {
                if let Some(after) = rest.strip_prefix(b) {
                    job.append(b, 0.0, fmt(t::mono(size), t::ACCENT));
                    rest = after;
                    break;
                }
            }
        }
        inline_spans(&mut job, rest, size);
    }
    job
}

/// `code` and **bold** inside a line.
fn inline_spans(job: &mut egui::text::LayoutJob, s: &str, size: f32) {
    let mut rest = s;
    while !rest.is_empty() {
        let tick = rest.find('`');
        let bold = rest.find("**");
        let next = match (tick, bold) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let Some(at) = next else {
            job.append(rest, 0.0, fmt(t::mono(size), t::TEXT));
            return;
        };
        job.append(&rest[..at], 0.0, fmt(t::mono(size), t::TEXT));
        rest = &rest[at..];
        let (marker, color, font) = if rest.starts_with("**") {
            ("**", t::TEXT, t::mono_semi(size))
        } else {
            ("`", t::ACCENT_TEXT, t::mono(size))
        };
        match rest[marker.len()..].find(marker) {
            Some(end) => {
                let span = &rest[..marker.len() * 2 + end];
                job.append(span, 0.0, fmt(font, color));
                rest = &rest[span.len()..];
            }
            None => {
                job.append(rest, 0.0, fmt(t::mono(size), t::TEXT));
                return;
            }
        }
    }
}

// ---- the window ----

pub fn details_window(app: &mut WorklogApp, ctx: &egui::Context) {
    let Some(mut d) = app.task_details.take() else {
        return;
    };
    let mut close = false;
    let mut save = false;
    let mut discard_and_close = false;
    let dirty = d.text != d.saved;

    // keys: Ctrl+S saves; Esc closes, asking first if there are changes
    let (ctrl_s, esc) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::CTRL, egui::Key::S),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if ctrl_s {
        save = true;
    }
    let editor_focused = ctx.memory(|m| m.focused() == Some(egui::Id::new(EDITOR_ID)));
    if esc && !editor_focused {
        if dirty && !d.confirm_close {
            d.confirm_close = true;
        } else if !dirty {
            close = true;
        }
    }

    let screen = ctx.content_rect();
    egui::Window::new("notes_editor")
        .id(egui::Id::new("task_details"))
        .title_bar(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .resizable(true)
        .default_size(vec2(860.0_f32.min(screen.width() - 40.0), 640.0_f32.min(screen.height() - 80.0)))
        .min_size(vec2(480.0, 200.0))
        .frame(
            egui::Frame::new()
                .fill(t::PANEL)
                .stroke(Stroke::new(1.0, t::BORDER_STRONG))
                .corner_radius(t::R_DIALOG)
                .shadow(ctx.global_style().visuals.window_shadow),
        )
        .show(ctx, |ui_| {
            ui_.spacing_mut().item_spacing.y = 0.0;
            header(ui_, &mut d, &mut close);
            if d.collapsed {
                return;
            }
            hr(ui_);
            let width = ui_.available_width();
            let mode = d.mode.unwrap_or(if width >= 800.0 {
                NoteMode::Split
            } else if d.saved.trim().is_empty() {
                NoteMode::Edit
            } else {
                NoteMode::Preview
            });
            let mut picked = mode;
            let cursor_line = egui::TextEdit::load_state(ctx, egui::Id::new(EDITOR_ID))
                .and_then(|s| s.cursor.char_range())
                .map(|r| line_of(&d.text, r.primary.index.0));
            let md = toolbar(ui_, &mut picked, mode != NoteMode::Preview, cursor_line.map(|l| line_kind(&d.text, l)));
            if picked != mode {
                d.mode = Some(picked);
            }
            hr(ui_);
            if d.confirm_close {
                confirm_bar(ui_, &mut d, &mut save, &mut close, &mut discard_and_close);
            }
            let footer_h = 61.0;
            let body_h = (ui_.available_height() - footer_h).max(120.0);
            body(app, ui_, &mut d, picked, md, body_h);
            hr(ui_);
            footer(ui_, &mut d, &mut save);
        });

    if discard_and_close {
        app.set_status(format!("Discarded changes — {}", d.task_title));
        return;
    }
    // closing (✕) saves, as it always has — notes are too long to lose to
    // a misclick; Discard and the Esc prompt are the explicit way out
    if (save || close) && d.text != d.saved {
        match app.db.set_task_details(d.task_id, d.text.trim()) {
            Ok(()) => {
                d.saved = d.text.clone();
                app.set_status(format!("Notes saved — {}", d.task_title));
            }
            Err(e) => app.set_status(format!("Save failed: {e}")),
        }
        app.touch();
    }
    if !close {
        app.task_details = Some(d);
    }
}

fn hr(ui_: &mut egui::Ui) {
    let (rect, _) = ui_.allocate_exact_size(vec2(ui_.available_width(), 1.0), Sense::hover());
    ui_.painter().rect_filled(rect, 0, t::DIVIDER);
}

fn header(ui_: &mut egui::Ui, d: &mut TaskDetails, close: &mut bool) {
    egui::Frame::new()
        .inner_margin(Margin { left: 24, right: 16, top: 16, bottom: 14 })
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.horizontal(|ui_| {
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    ui_.spacing_mut().item_spacing.x = 2.0;
                    if w::icon_button_ex(ui_, icon::X, "Close — saves your changes", 36.0, t::MUTED, true)
                        .clicked()
                    {
                        *close = true;
                    }
                    let (glyph, tip) = if d.collapsed {
                        (icon::CARET_DOWN, "Expand")
                    } else {
                        (icon::ARROWS_IN_SIMPLE, "Collapse")
                    };
                    if w::icon_button_ex(ui_, glyph, tip, 36.0, t::MUTED, true).clicked() {
                        d.collapsed = !d.collapsed;
                    }
                    ui_.with_layout(Layout::top_down(Align::Min), |ui_| {
                        ui_.spacing_mut().item_spacing.y = 4.0;
                        w::eyebrow(ui_, "Notes");
                        ui_.horizontal(|ui_| {
                            ui_.spacing_mut().item_spacing.x = 12.0;
                            ui_.add(
                                egui::Label::new(
                                    RichText::new(&d.task_title)
                                        .font(t::font(18.0, Weight::Semibold))
                                        .color(t::TEXT),
                                )
                                .truncate(),
                            );
                            ui_.horizontal(|ui_| {
                                ui_.spacing_mut().item_spacing.x = 6.0;
                                w::dot(ui_, d.swatch.base, 7.0);
                                ui_.label(w::mono(&d.project_code, 12.5).color(d.swatch.light));
                            });
                            if d.is_dev {
                                w::rd_badge(ui_);
                            }
                        });
                    });
                });
            });
        });
}

/// Mode switch + formatting buttons. Returns the formatting action clicked.
fn toolbar(
    ui_: &mut egui::Ui,
    mode: &mut NoteMode,
    can_format: bool,
    kind: Option<LineKind>,
) -> Option<Md> {
    let mut md = None;
    let kind = kind.unwrap_or_default();
    egui::Frame::new()
        .fill(t::PANEL_ALT)
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.horizontal_wrapped(|ui_| {
                ui_.spacing_mut().item_spacing.x = 2.0;
                w::segmented(
                    ui_,
                    mode,
                    &[
                        Segment::new(NoteMode::Edit, "Edit"),
                        Segment::new(NoteMode::Split, "Split"),
                        Segment::new(NoteMode::Preview, "Preview"),
                    ],
                    t::INSET,
                    28.0,
                );
                sep(ui_, 22.0, 10.0);
                let mut tb = |ui_: &mut egui::Ui, glyph: &str, tip: &str, active: bool, action: Md| {
                    if tool_button(ui_, glyph, tip, active, can_format) {
                        md = Some(action);
                    }
                };
                tb(ui_, icon::TEXT_B, "Bold  Ctrl+B", false, Md::Wrap("**"));
                tb(ui_, icon::TEXT_ITALIC, "Italic  Ctrl+I", false, Md::Wrap("*"));
                tb(ui_, icon::TEXT_STRIKETHROUGH, "Strikethrough", false, Md::Wrap("~~"));
                tb(ui_, icon::CODE, "Inline code", false, Md::Wrap("`"));
                sep(ui_, 18.0, 6.0);
                // heading: a small menu for the level
                let h = tool_button_resp(ui_, icon::TEXT_H, "Heading", kind.heading, can_format);
                egui::Popup::menu(&h).show(|ui_| {
                    for (label, p) in [("Heading 1", "# "), ("Heading 2", "## "), ("Heading 3", "### ")] {
                        if ui_.button(label).clicked() {
                            md = Some(Md::LinePrefix(p));
                        }
                    }
                });
                let mut tb = |ui_: &mut egui::Ui, glyph: &str, tip: &str, active: bool, action: Md| {
                    if tool_button(ui_, glyph, tip, active, can_format) {
                        md = Some(action);
                    }
                };
                tb(ui_, icon::LIST_BULLETS, "Bulleted list", kind.bullet, Md::LinePrefix("- "));
                tb(ui_, icon::LIST_NUMBERS, "Numbered list", kind.numbered, Md::Numbered);
                tb(ui_, icon::LIST_CHECKS, "Checklist", kind.checklist, Md::LinePrefix("- [ ] "));
                tb(ui_, icon::QUOTES, "Quote", kind.quote, Md::LinePrefix("> "));
                sep(ui_, 18.0, 6.0);
                tb(ui_, icon::CHECK_SQUARE, "Toggle checkbox  Ctrl+Enter", false, Md::ToggleCheck);
                tb(ui_, icon::CODE_BLOCK, "Code block", kind.code_block, Md::CodeBlock);
                tb(ui_, icon::TABLE, "Table", false, Md::Table);
                tb(ui_, icon::MINUS, "Divider", false, Md::Rule);
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    ui_.label(RichText::new("Markdown").size(12.0).color(t::FAINT));
                });
            });
        });
    md
}

fn sep(ui_: &mut egui::Ui, h: f32, pad: f32) {
    ui_.add_space(pad);
    let (rect, _) = ui_.allocate_exact_size(vec2(1.0, h), Sense::hover());
    ui_.painter().rect_filled(rect, 0, t::BORDER_STRONG);
    ui_.add_space(pad);
}

fn tool_button_resp(ui_: &mut egui::Ui, glyph: &str, tip: &str, active: bool, enabled: bool) -> egui::Response {
    let (rect, resp) = ui_.allocate_exact_size(vec2(32.0, 32.0), if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    if active {
        ui_.painter().rect_filled(rect, 7, t::alpha(t::ACCENT, 0.16));
    } else if hovered {
        ui_.painter().rect_filled(rect, 7, t::CONTROL_HOVER);
    }
    let color = if !enabled {
        t::DISABLED
    } else if active {
        t::ACCENT_TEXT
    } else if hovered {
        t::TEXT
    } else {
        t::TEXT_2
    };
    ui_.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, t::icon_font(17.0), color);
    resp.on_hover_text(tip)
}

fn tool_button(ui_: &mut egui::Ui, glyph: &str, tip: &str, active: bool, enabled: bool) -> bool {
    tool_button_resp(ui_, glyph, tip, active, enabled).clicked()
}

fn confirm_bar(
    ui_: &mut egui::Ui,
    d: &mut TaskDetails,
    save: &mut bool,
    close: &mut bool,
    discard: &mut bool,
) {
    egui::Frame::new()
        .fill(t::AMBER_BG)
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 8.0;
                w::icon_label(ui_, &format!("{}  Close without saving your changes?", icon::WARNING), 13.0, t::AMBER);
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    if ui_.add(w::primary("Save & close").min_size(vec2(0.0, 30.0))).clicked() {
                        *save = true;
                        *close = true;
                        d.confirm_close = false;
                    }
                    if ui_.add(w::secondary("Discard & close").min_size(vec2(0.0, 30.0))).clicked() {
                        *discard = true;
                    }
                    if ui_.add(w::ghost("Keep editing").min_size(vec2(0.0, 30.0))).clicked() {
                        d.confirm_close = false;
                    }
                });
            });
        });
}

fn body(app: &mut WorklogApp, ui_: &mut egui::Ui, d: &mut TaskDetails, mode: NoteMode, md: Option<Md>, height: f32) {
    let ctx = ui_.ctx().clone();
    let editor_id = egui::Id::new(EDITOR_ID);
    let mut md = md;
    if mode != NoteMode::Preview {
        if ui_.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::B)) {
            md = Some(Md::Wrap("**"));
        }
        if ui_.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::I)) {
            md = Some(Md::Wrap("*"));
        }
        if ui_.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter)) {
            md = Some(Md::ToggleCheck);
        }
    }
    if let Some(md) = md {
        // Last frame's cursor tells us what to format; fall back to the end
        // of the text if the editor was never focused.
        let sel = egui::TextEdit::load_state(&ctx, editor_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let (x, y) = (r.primary.index.0, r.secondary.index.0);
                (x.min(y), x.max(y))
            })
            .unwrap_or_else(|| {
                let n = d.text.chars().count();
                (n, n)
            });
        let (na, nb) = if let Md::ToggleCheck = md {
            d.text = toggle_line_checkbox(&d.text, line_of(&d.text, sel.0));
            let n = d.text.chars().count();
            (sel.0.min(n), sel.1.min(n))
        } else {
            apply_md(&mut d.text, sel, &md)
        };
        let mut state = egui::TextEdit::load_state(&ctx, editor_id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(na),
            egui::text::CCursor::new(nb),
        )));
        state.store(&ctx, editor_id);
        ctx.memory_mut(|m| m.request_focus(editor_id));
    }

    let width = ui_.available_width();
    let (rect, _) = ui_.allocate_exact_size(vec2(width, height), Sense::hover());
    match mode {
        NoteMode::Edit => {
            source_pane(ui_, rect, d);
        }
        NoteMode::Preview => {
            ui_.painter().rect_filled(rect, 0, t::PANEL);
            preview_pane(app, ui_, rect, d);
        }
        NoteMode::Split => {
            let half = (width / 2.0).floor();
            let left = egui::Rect::from_min_size(rect.min, vec2(half, height));
            let right = egui::Rect::from_min_max(egui::pos2(left.right(), rect.top()), rect.max);
            source_pane(ui_, left, d);
            ui_.painter().vline(left.right(), left.y_range(), Stroke::new(1.0, t::DIVIDER));
            preview_pane(app, ui_, right.shrink2(vec2(0.5, 0.0)), d);
        }
    }
}

/// Mono source with a line-number gutter, current-line band and colouring.
fn source_pane(ui_: &mut egui::Ui, rect: egui::Rect, d: &mut TaskDetails) {
    ui_.painter().rect_filled(rect, 0, t::INSET);
    let gutter_w = 44.0;
    let mut child = ui_.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
    egui::ScrollArea::vertical()
        .id_salt("notes_editor/source")
        .auto_shrink([false, false])
        .show(&mut child, |ui_| {
            ui_.horizontal_top(|ui_| {
                ui_.spacing_mut().item_spacing.x = 0.0;
                let (gutter, _) = ui_.allocate_exact_size(vec2(gutter_w, 0.0), Sense::hover());
                let band = ui_.painter().add(egui::Shape::Noop);
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| -> Arc<egui::Galley> {
                    let mut job = highlight(buf.as_str());
                    job.wrap.max_width = wrap;
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let out = egui::TextEdit::multiline(&mut d.text)
                    .id(egui::Id::new(EDITOR_ID))
                    .hint_text(RichText::new("Anything worth remembering about this task…").color(t::FAINT))
                    .frame(egui::Frame::new().inner_margin(Margin { left: 16, right: 16, top: 14, bottom: 14 }))
                    .background_color(egui::Color32::TRANSPARENT)
                    .desired_width(f32::INFINITY)
                    .min_size(vec2(0.0, rect.height()))
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui_);
                // gutter: separator line, numbers on each logical line's
                // first row, the cursor's line brighter and banded
                let galley = &out.galley;
                let origin = out.galley_pos;
                let cursor_line = out.cursor_range.map(|r| line_of(&d.text, r.primary.index.0));
                let full = out.response.response.rect;
                ui_.painter().vline(
                    gutter.right(),
                    full.y_range(),
                    Stroke::new(1.0, t::CONTROL),
                );
                let mut line = 0;
                let mut first_row = true;
                for row in &galley.rows {
                    let r = row.rect().translate(origin.to_vec2());
                    if first_row {
                        let current = cursor_line == Some(line);
                        ui_.painter().text(
                            egui::pos2(gutter.right() - 10.0, r.center().y),
                            egui::Align2::RIGHT_CENTER,
                            (line + 1).to_string(),
                            t::mono(13.5),
                            if current { t::TEXT_2 } else { t::FAINT },
                        );
                    }
                    if cursor_line == Some(line) && out.response.response.has_focus() {
                        let band_rect = egui::Rect::from_min_max(
                            egui::pos2(gutter.right() + 1.0, r.top()),
                            egui::pos2(full.right(), r.bottom()),
                        );
                        ui_.painter().set(band, egui::Shape::rect_filled(band_rect, 0, t::CURRENT_LINE));
                    }
                    first_row = row.ends_with_newline;
                    if row.ends_with_newline {
                        line += 1;
                    }
                }
            });
        });
}

/// Rendered markdown. Checklist lines become real checkboxes (clicking one
/// rewrites its source line); everything between them goes through
/// CommonMark.
fn preview_pane(app: &mut WorklogApp, ui_: &mut egui::Ui, rect: egui::Rect, d: &mut TaskDetails) {
    let mut child = ui_.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
    let mut toggle: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("notes_editor/preview")
        .auto_shrink([false, false])
        .show(&mut child, |ui_| {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(24, 18))
                .show(ui_, |ui_| {
                    ui_.set_width(ui_.available_width());
                    if d.text.trim().is_empty() {
                        ui_.label(
                            RichText::new("Nothing here yet — write on the left (or switch to Edit).")
                                .color(t::FAINT),
                        );
                        return;
                    }
                    for (n, block) in preview_blocks(&d.text).into_iter().enumerate() {
                        match block {
                            Block::Markdown(text) => {
                                ui_.push_id(("md_block", n), |ui_| {
                                    egui_commonmark::CommonMarkViewer::new().show(ui_, &mut app.md_cache, &text);
                                });
                            }
                            Block::Check { line, checked, indent, text } => {
                                ui_.horizontal(|ui_| {
                                    ui_.add_space(indent as f32 * 10.0);
                                    ui_.spacing_mut().item_spacing.x = 12.0;
                                    let label = if checked {
                                        RichText::new(&text).size(14.0).color(t::MUTED).strikethrough()
                                    } else {
                                        RichText::new(&text).size(14.0).color(t::TEXT)
                                    };
                                    let a = w::checkbox(ui_, checked, "");
                                    let b = ui_.add(egui::Label::new(label).sense(Sense::click()));
                                    if a.clicked() || b.clicked() {
                                        toggle = Some(line);
                                    }
                                });
                                ui_.add_space(6.0);
                            }
                        }
                    }
                });
        });
    if let Some(line) = toggle {
        d.text = crate::ui::notes::toggle_item(&d.text, line);
    }
}

#[derive(Debug, PartialEq)]
enum Block {
    Markdown(String),
    Check { line: usize, checked: bool, indent: usize, text: String },
}

/// Split notes into checklist lines and the markdown between them (lines
/// inside ``` fences always stay markdown).
fn preview_blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut buf: Vec<&str> = Vec::new();
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        let item = if in_fence { None } else { crate::ui::notes::checklist_items(line).pop() };
        match item {
            Some(item) => {
                if !buf.is_empty() {
                    out.push(Block::Markdown(buf.join("\n")));
                    buf.clear();
                }
                out.push(Block::Check {
                    line: i,
                    checked: item.checked,
                    indent: line.len() - line.trim_start().len(),
                    text: item.text,
                });
            }
            None => buf.push(line),
        }
    }
    if buf.iter().any(|l| !l.trim().is_empty()) {
        out.push(Block::Markdown(buf.join("\n")));
    }
    out.retain(|b| !matches!(b, Block::Markdown(t) if t.trim().is_empty()));
    out
}

fn footer(ui_: &mut egui::Ui, d: &mut TaskDetails, save: &mut bool) {
    egui::Frame::new()
        .fill(t::PANEL_ALT)
        .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: t::R_DIALOG, se: t::R_DIALOG })
        .inner_margin(Margin { left: 24, right: 16, top: 12, bottom: 12 })
        .show(ui_, |ui_| {
            ui_.set_width(ui_.available_width());
            ui_.horizontal(|ui_| {
                ui_.spacing_mut().item_spacing.x = 6.0;
                if ui_.available_width() > 720.0 {
                    for chip in ["**bold**", "# heading", "- list", "- [ ] todo", "`code`"] {
                        w::pill_job(
                            ui_,
                            w::mono(chip, 11.5).color(t::MUTED),
                            egui::Color32::TRANSPARENT,
                            t::BORDER_STRONG,
                            5,
                            vec2(7.0, 2.0),
                        );
                    }
                }
                ui_.with_layout(Layout::right_to_left(Align::Center), |ui_| {
                    ui_.spacing_mut().item_spacing.x = 14.0;
                    let mut job = egui::text::LayoutJob::default();
                    job.append("Save", 0.0, egui::TextFormat::simple(t::font(13.0, Weight::Semibold), t::ACCENT_INK));
                    job.append("   Ctrl S", 0.0, egui::TextFormat::simple(t::mono(11.0), t::alpha(t::ACCENT_INK, 0.6)));
                    let dirty = d.text != d.saved;
                    if ui_
                        .add(
                            egui::Button::new(job)
                                .fill(t::ACCENT)
                                .corner_radius(t::R_INPUT)
                                .min_size(vec2(0.0, 36.0)),
                        )
                        .clicked()
                    {
                        *save = true;
                    }
                    if ui_
                        .add_enabled(dirty, w::secondary("Discard").min_size(vec2(0.0, 36.0)))
                        .on_hover_text("Throw away the changes since the last save")
                        .clicked()
                    {
                        d.text = d.saved.clone();
                    }
                    let items = crate::ui::notes::checklist_items(&d.text);
                    if !items.is_empty() {
                        let done = items.iter().filter(|i| i.checked).count();
                        ui_.label(w::mono(format!("{done} / {}", items.len()), 12.5).color(t::ACCENT_TEXT))
                            .on_hover_text("Checklist items done");
                    }
                    if dirty {
                        ui_.label(RichText::new("● Unsaved changes").size(12.5).color(t::AMBER));
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_adds_and_toggles() {
        let mut t = String::from("hello world");
        assert_eq!(apply_md(&mut t, (6, 11), &Md::Wrap("**")), (8, 13));
        assert_eq!(t, "hello **world**");
        // same selection (the inner word) toggles it back off
        assert_eq!(apply_md(&mut t, (8, 13), &Md::Wrap("**")), (6, 11));
        assert_eq!(t, "hello world");
        // selecting the markers too also toggles off
        apply_md(&mut t, (6, 11), &Md::Wrap("~~"));
        assert_eq!(apply_md(&mut t, (6, 15), &Md::Wrap("~~")), (6, 11));
        assert_eq!(t, "hello world");
    }

    #[test]
    fn empty_selection_wrap_places_cursor_between_markers() {
        let mut t = String::from("ab");
        assert_eq!(apply_md(&mut t, (2, 2), &Md::Wrap("**")), (4, 4));
        assert_eq!(t, "ab****");
    }

    #[test]
    fn heading_swaps_level_and_toggles() {
        let mut t = String::from("title");
        apply_md(&mut t, (0, 0), &Md::LinePrefix("# "));
        assert_eq!(t, "# title");
        apply_md(&mut t, (3, 3), &Md::LinePrefix("## "));
        assert_eq!(t, "## title");
        apply_md(&mut t, (0, 0), &Md::LinePrefix("## "));
        assert_eq!(t, "title");
    }

    #[test]
    fn lists_cover_every_selected_line() {
        let mut t = String::from("a\nb\nc");
        apply_md(&mut t, (0, 5), &Md::LinePrefix("- "));
        assert_eq!(t, "- a\n- b\n- c");
        let mut t = String::from("a\nb");
        apply_md(&mut t, (0, 3), &Md::Numbered);
        assert_eq!(t, "1. a\n2. b");
        let len = t.chars().count();
        apply_md(&mut t, (0, len), &Md::Numbered);
        assert_eq!(t, "a\nb");
    }

    #[test]
    fn code_block_wraps_selected_lines() {
        let mut t = String::from("let x = 1;");
        apply_md(&mut t, (2, 5), &Md::CodeBlock);
        assert_eq!(t, "```\nlet x = 1;\n```");
    }

    #[test]
    fn wrap_survives_multibyte_text() {
        let mut t = String::from("café été");
        assert_eq!(apply_md(&mut t, (5, 8), &Md::Wrap("**")), (7, 10));
        assert_eq!(t, "café **été**");
    }

    #[test]
    fn highlight_keeps_text_and_colours_markers() {
        let src = "# Title\n- [x] done `code`\n1. **b** rest";
        let job = highlight(src);
        assert_eq!(job.text, src);
        let color_of = |needle: &str| {
            let at = job.text.find(needle).unwrap();
            job.sections
                .iter()
                .find(|s| s.byte_range.start.0 <= at && at < s.byte_range.end.0)
                .unwrap()
                .format
                .color
        };
        assert_eq!(color_of("[x]"), t::ACCENT);
        assert_eq!(color_of("`code`"), t::ACCENT_TEXT);
        assert_eq!(color_of("- ["), t::FAINT);
    }

    #[test]
    fn line_kinds_and_checkbox_toggle() {
        let text = "intro\n- [ ] a\n- b\n```\ncode\n```\n# H";
        assert!(line_kind(text, 1).checklist);
        assert!(line_kind(text, 2).bullet && !line_kind(text, 2).checklist);
        assert!(line_kind(text, 4).code_block);
        assert!(line_kind(text, 6).heading);
        assert_eq!(line_of(text, 7), 1);
        assert_eq!(toggle_line_checkbox("- [ ] a", 0), "- [x] a");
        assert_eq!(toggle_line_checkbox("  - b", 0), "  - [ ] b");
        assert_eq!(toggle_line_checkbox("plain", 0), "- [ ] plain");
    }

    #[test]
    fn preview_splits_checklist_from_markdown() {
        let blocks = preview_blocks("# T\n- [x] a\n  - [ ] b\ntext\n```\n- [ ] not\n```");
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0], Block::Markdown("# T".into()));
        assert!(matches!(blocks[1], Block::Check { line: 1, checked: true, .. }));
        assert!(matches!(blocks[2], Block::Check { line: 2, checked: false, indent: 2, .. }));
        assert!(matches!(&blocks[3], Block::Markdown(t) if t.contains("- [ ] not")));
    }
}
