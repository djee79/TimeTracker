//! Design tokens (colours, type, shape) and the one-time style setup.
//! Every colour the UI paints comes from here — no hex literals elsewhere.

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Vec2};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// ---- surfaces ----
pub const BG: Color32 = hex(0x111113);
pub const BAR: Color32 = hex(0x16161a);
pub const PANEL: Color32 = hex(0x18181c);
pub const PANEL_ALT: Color32 = hex(0x151518);
pub const INSET: Color32 = hex(0x0f0f12);
pub const PREVIEW_BG: Color32 = hex(0x0d0d10);
pub const TABLE_HEAD: Color32 = hex(0x141417);
pub const CONTROL: Color32 = hex(0x1d1d22);
pub const HOVER: Color32 = hex(0x1f1f25);
pub const CONTROL_HOVER: Color32 = hex(0x2a2a32);
pub const ACTIVE_TAB: Color32 = hex(0x2c2c35);
pub const BORDER: Color32 = hex(0x26262d);
pub const BORDER_STRONG: Color32 = hex(0x2c2c34);
pub const DIVIDER: Color32 = hex(0x24242a);
pub const LIST_DIVIDER: Color32 = hex(0x222228);
pub const OUT_OF_MONTH: Color32 = hex(0x131316);
pub const CURRENT_LINE: Color32 = hex(0x17171c);
pub const SWITCH_OFF: Color32 = hex(0x33333b);
pub const COUNT_PILL: Color32 = hex(0x26262d);

// ---- text ----
pub const TEXT: Color32 = hex(0xececf1);
pub const TEXT_2: Color32 = hex(0xc9c9d2);
pub const MUTED: Color32 = hex(0x9a9aa6);
/// Weekend day numbers in the date picker (between TEXT_2 and MUTED).
pub const WEEKEND_TEXT: Color32 = hex(0xa9a9b5);
/// Quick-pick button text.
pub const TEXT_SOFT: Color32 = hex(0xd6d6de);
pub const FAINT: Color32 = hex(0x85858f);
pub const DISABLED: Color32 = hex(0x4a4a52);

// ---- accent + states ----
pub const ACCENT: Color32 = hex(0x8b9cff);
pub const ACCENT_INK: Color32 = hex(0x0f1330);
pub const ACCENT_TEXT: Color32 = hex(0xc8d0ff);
pub const BANNER_BG: Color32 = hex(0x1a1c2a);
pub const SELECTED_ROW: Color32 = hex(0x1e1e27);
pub const TODAY_COL: Color32 = hex(0x1c1d29);
pub const AMBER: Color32 = hex(0xf2b04c);
pub const AMBER_BG: Color32 = hex(0x1b1915);
pub const DANGER: Color32 = hex(0xf07070);
pub const LINK: Color32 = hex(0xaab6ff);

/// `ACCENT` at ~14 % — running pill, "Today" badge.
pub fn accent_tint() -> Color32 {
    alpha(ACCENT, 0.14)
}

/// `c` with its alpha replaced (0.0–1.0).
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

// ---- project palette ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    /// Stored in `projects.color`.
    pub key: &'static str,
    pub name: &'static str,
    /// Dots, fills, bars.
    pub base: Color32,
    /// Text in the project's colour.
    pub light: Color32,
}

pub const PALETTE: [Swatch; 6] = [
    Swatch { key: "blue", name: "Blue", base: hex(0x7aa2f7), light: hex(0xc9d7fb) },
    Swatch { key: "lime", name: "Lime", base: hex(0xb8d46a), light: hex(0xdbe9b0) },
    Swatch { key: "orange", name: "Orange", base: hex(0xf08a5d), light: hex(0xf5b396) },
    Swatch { key: "pink", name: "Pink", base: hex(0xe48ac6), light: hex(0xedb3da) },
    Swatch { key: "teal", name: "Teal", base: hex(0x5ccfc0), light: hex(0x8fe0d5) },
    Swatch { key: "violet", name: "Violet", base: hex(0xb294f5), light: hex(0xd9c9fb) },
];

/// Palette entry by stored key; unknown/missing keys fall back to a stable
/// pick from the project id so nothing ever renders colourless.
pub fn swatch(key: Option<&str>, project_id: i64) -> Swatch {
    key.and_then(|k| PALETTE.iter().find(|s| s.key == k))
        .copied()
        .unwrap_or(PALETTE[(project_id.rem_euclid(PALETTE.len() as i64)) as usize])
}

// ---- day-off kinds ----
pub const HOLIDAY: Color32 = PALETTE[4].base; // teal
pub const VACATION: Color32 = PALETTE[5].base; // violet
pub const SICK: Color32 = PALETTE[3].base; // pink
pub const OTHER_OFF: Color32 = hex(0x8e8e9a);

// ---- shape ----
pub const R_DIALOG: u8 = 16;
pub const R_CARD: u8 = 14;
pub const R_LIST: u8 = 12;
pub const R_INPUT: u8 = 9;
pub const R_ICON: u8 = 8;
pub const R_PILL: u8 = 255;

pub const H_SMALL: f32 = 30.0;
pub const PAGE_PAD: i8 = 24;
pub const CARD_PAD: i8 = 18;

// ---- typography ----

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");
const MONO_REGULAR: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const MONO_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-SemiBold.ttf");

/// egui has no weight axis, so each weight is its own family.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
    Bold,
}

fn family(w: Weight) -> FontFamily {
    match w {
        Weight::Regular => FontFamily::Proportional,
        Weight::Medium => FontFamily::Name("medium".into()),
        Weight::Semibold => FontFamily::Name("semibold".into()),
        Weight::Bold => FontFamily::Name("bold".into()),
    }
}

/// Inter at a size and weight.
pub fn font(size: f32, w: Weight) -> FontId {
    FontId::new(size, family(w))
}

/// JetBrains Mono regular.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// JetBrains Mono 600 — codes and hours.
pub fn mono_semi(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono-semibold".into()))
}

fn fonts() -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    // keep egui's bundled fonts as glyph fallbacks (emoji, symbols)
    let fallbacks: Vec<String> = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (name, bytes) in [
        ("inter", INTER_REGULAR),
        ("inter-medium", INTER_MEDIUM),
        ("inter-semibold", INTER_SEMIBOLD),
        ("inter-bold", INTER_BOLD),
        ("mono", MONO_REGULAR),
        ("mono-semibold", MONO_SEMIBOLD),
    ] {
        defs.font_data
            .insert(name.into(), egui::FontData::from_static(bytes).into());
    }
    defs.font_data.insert(
        "phosphor".into(),
        egui_phosphor::Variant::Regular.font_data().into(),
    );
    let stack = |primary: &str| -> Vec<String> {
        // phosphor is only a fallback here: it carries a few ASCII glyphs of
        // its own, and Inter has some private-use ones — icons are drawn with
        // the dedicated "icons" family instead (see `widgets::rich`)
        let mut v = vec![primary.to_string(), "phosphor".to_string(), "inter".to_string()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    defs.families.insert(FontFamily::Proportional, stack("inter"));
    defs.families.insert(FontFamily::Monospace, stack("mono"));
    for (fam, primary) in [
        ("medium", "inter-medium"),
        ("semibold", "inter-semibold"),
        ("bold", "inter-bold"),
        ("mono-semibold", "mono-semibold"),
    ] {
        defs.families
            .insert(FontFamily::Name(fam.into()), stack(primary));
    }
    defs.families.insert(
        FontFamily::Name("icons".into()),
        vec!["phosphor".to_string(), "inter".to_string()],
    );
    defs
}

/// Phosphor icons at a size.
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}

/// True for Unicode private-use characters — where the icon glyphs live.
pub fn is_icon_char(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
}

/// Fonts, colours and spacing — called once when the app starts.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, style);
}

fn style(s: &mut egui::Style) {
    s.text_styles = [
        (TextStyle::Small, font(12.0, Weight::Regular)),
        (TextStyle::Body, font(14.0, Weight::Regular)),
        (TextStyle::Button, font(13.0, Weight::Medium)),
        (TextStyle::Heading, font(18.0, Weight::Semibold)),
        (TextStyle::Monospace, mono(13.0)),
    ]
    .into();

    let sp = &mut s.spacing;
    sp.item_spacing = Vec2::new(8.0, 8.0);
    sp.button_padding = Vec2::new(12.0, 6.0);
    sp.interact_size = Vec2::new(30.0, H_SMALL);
    sp.window_margin = egui::Margin::same(20);
    sp.menu_margin = egui::Margin::same(6);
    sp.combo_height = 380.0;
    sp.icon_width = 16.0;
    sp.icon_width_inner = 9.0;
    sp.icon_spacing = 8.0;

    let v = &mut s.visuals;
    *v = egui::Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(R_DIALOG);
    v.window_shadow = Shadow {
        offset: [0, 16],
        blur: 48,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };
    v.popup_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = INSET;
    v.text_edit_bg_color = Some(INSET);
    v.faint_bg_color = TABLE_HEAD;
    v.code_bg_color = INSET;
    v.hyperlink_color = LINK;
    v.warn_fg_color = AMBER;
    v.error_fg_color = DANGER;
    v.weak_text_color = Some(MUTED);
    v.selection.bg_fill = alpha(ACCENT, 0.30);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.text_cursor.stroke = Stroke::new(2.0, ACCENT);
    v.striped = false;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;

    let r = CornerRadius::same(R_INPUT);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, DIVIDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_2);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = CONTROL;
    w.inactive.weak_bg_fill = CONTROL;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_2);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = CONTROL_HOVER;
    w.hovered.weak_bg_fill = CONTROL_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, hex(0x3a3a44));
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = ACTIVE_TAB;
    w.active.weak_bg_fill = ACTIVE_TAB;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = ACTIVE_TAB;
    w.open.weak_bg_fill = ACTIVE_TAB;
    w.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = r;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatch_lookup_and_fallback() {
        assert_eq!(swatch(Some("teal"), 1).key, "teal");
        assert_eq!(swatch(None, 0).key, PALETTE[0].key);
        assert_eq!(swatch(Some("nope"), 7).key, PALETTE[1].key);
        assert_eq!(swatch(None, -1).key, PALETTE[5].key);
    }
}
