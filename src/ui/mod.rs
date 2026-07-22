pub mod help;
pub mod journal;
pub mod projects;
pub mod reports;
pub mod search;
pub mod tasks;

use chrono::{Datelike, NaiveDate};

use crate::app::WorklogApp;

// Focus markers: request_focus(marker) in an action, apply_focus(marker, resp)
// where the widget is rendered.
pub const FOCUS_CAPTURE_DESC: &str = "focus/capture_desc";
pub const FOCUS_TASK_TITLE: &str = "focus/task_title";
pub const FOCUS_TASK_EDIT: &str = "focus/task_edit";
pub const FOCUS_BRIDGE_HOURS: &str = "focus/bridge_hours";
pub const FOCUS_SEARCH: &str = "focus/search";

/// The splash artwork, loaded once per texture user (boot screen, help
/// footer). Both sizes draw at 560×360 logical points.
pub struct Splash {
    pub texture: egui::TextureHandle,
    /// Fill behind/around the image (sampled from the art — the corners
    /// themselves are transparent).
    pub background: egui::Color32,
}

pub fn load_splash(ctx: &egui::Context) -> Splash {
    let bytes: &[u8] = if ctx.pixels_per_point() > 1.25 {
        include_bytes!("../../assets/SplashScreen/splash-1120x720@2x.png")
    } else {
        include_bytes!("../../assets/SplashScreen/splash-560x360.png")
    };
    let img = image::load_from_memory(bytes)
        .expect("bundled splash decodes")
        .into_rgba8();
    let p = img.get_pixel(img.width() / 2, 2);
    let background = egui::Color32::from_rgb(p[0], p[1], p[2]);
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Splash {
        texture: ctx.load_texture("splash", color, egui::TextureOptions::LINEAR),
        background,
    }
}

/// True when this widget lost focus because Enter was pressed — the
/// "type, hit Enter, saved" interaction every form here uses.
pub fn enter_pressed(resp: &egui::Response, ui: &egui::Ui) -> bool {
    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// A stable per-project color: golden-angle hue steps from the id, so the
/// palette looks random but never changes between runs and neighboring ids
/// land far apart on the wheel. Saturation/brightness adapt to the theme.
pub fn project_color(ui: &egui::Ui, project_id: i64) -> egui::Color32 {
    let hue = (project_id as f32 * 0.618_034) % 1.0; // golden-ratio conjugate
    let (s, v) = if ui.visuals().dark_mode {
        (0.55, 0.90)
    } else {
        (0.75, 0.62)
    };
    egui::ecolor::Hsva::new(hue, s, v, 1.0).into()
}

/// Case-insensitive "every word matches somewhere" test against a project's
/// code, name and client — the same contract as the journal search.
fn project_matches(p: &crate::db::Project, filter: &str) -> bool {
    filter.split_whitespace().all(|w| {
        let w = w.to_lowercase();
        p.code.to_lowercase().contains(&w)
            || p.name.to_lowercase().contains(&w)
            || p.client.as_deref().is_some_and(|c| c.to_lowercase().contains(&w))
    })
}

/// The searchable project dropdown behind both pickers: a filter box (focused
/// when the popup opens; Enter picks the first hit) above the project list.
/// `all_option` adds a "no project" row with that label and doubles as the
/// unselected button text; `active_only` hides archived projects.
fn project_picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    app_projects: &[crate::db::Project],
    selected: &mut Option<i64>,
    all_option: Option<&str>,
    active_only: bool,
) -> bool {
    let mut changed = false;
    let text = selected
        .and_then(|id| app_projects.iter().find(|p| p.id == id))
        .map(|p| p.code.clone())
        .unwrap_or_else(|| all_option.unwrap_or("project…").into());
    let filter_id = egui::Id::new((id_salt, "picker_filter"));
    let open_id = egui::Id::new((id_salt, "picker_open"));
    let was_open: bool = ui.ctx().data(|d| d.get_temp(open_id)).unwrap_or(false);
    let inner = egui::ComboBox::from_id_salt(id_salt)
        .selected_text(text)
        .height(380.0)
        .show_ui(ui, |ui| {
            let mut filter: String =
                ui.ctx().data(|d| d.get_temp(filter_id)).unwrap_or_default();
            if !was_open {
                filter.clear(); // fresh popup, fresh search
            }
            let filter_resp = ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("type to filter…")
                    .desired_width(f32::INFINITY),
            );
            if !was_open {
                filter_resp.request_focus();
            }
            let submit = enter_pressed(&filter_resp, ui);
            ui.separator();
            if let Some(label) = all_option
                && ui.selectable_value(selected, None, label).clicked()
            {
                changed = true;
            }
            let mut first_match = None;
            for p in app_projects.iter().filter(|p| {
                (!active_only || p.status == crate::db::ProjectStatus::Active)
                    && project_matches(p, &filter)
            }) {
                first_match.get_or_insert(p.id);
                let label = egui::RichText::new(p.label()).color(project_color(ui, p.id));
                if ui.selectable_value(selected, Some(p.id), label).clicked() {
                    changed = true;
                }
            }
            match first_match {
                None => {
                    ui.label(egui::RichText::new("no project matches").weak());
                }
                Some(first) if submit => {
                    *selected = Some(first);
                    changed = true;
                    ui.close();
                }
                Some(_) => {}
            }
            ui.ctx().data_mut(|d| d.insert_temp(filter_id, filter));
        });
    ui.ctx()
        .data_mut(|d| d.insert_temp(open_id, inner.inner.is_some()));
    changed
}

/// Project picker over active projects. `selected` stays None until picked.
pub fn project_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    app_projects: &[crate::db::Project],
    selected: &mut Option<i64>,
) -> bool {
    project_picker(ui, id_salt, app_projects, selected, None, true)
}

/// Like `project_combo` but with an "all projects" option, for filters.
pub fn project_filter_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    app_projects: &[crate::db::Project],
    selected: &mut Option<i64>,
) -> bool {
    project_picker(ui, id_salt, app_projects, selected, Some("all projects"), false)
}

/// Calendar-popup date button (egui_extras uses jiff dates; we live in chrono).
pub fn date_picker(ui: &mut egui::Ui, id_salt: &str, date: &mut NaiveDate) -> bool {
    let mut jd = jiff::civil::Date::new(date.year() as i16, date.month() as i8, date.day() as i8)
        .unwrap_or_default();
    ui.add(
        egui_extras::DatePickerButton::new(&mut jd)
            .id_salt(id_salt)
            .show_icon(true)
            .calendar_week(true),
    );
    let new = NaiveDate::from_ymd_opt(jd.year() as i32, jd.month() as u32, jd.day() as u32)
        .unwrap_or(*date);
    let changed = new != *date;
    *date = new;
    changed
}

/// The dev flag, rendered the same everywhere.
pub fn dev_checkbox(ui: &mut egui::Ui, is_dev: &mut bool) {
    ui.checkbox(is_dev, "dev (R&D)")
        .on_hover_text("Flags this entry for the annual SR&ED / R&D export");
}

/// Small orange "dev" badge on list rows.
pub fn dev_badge(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("dev")
            .small()
            .color(egui::Color32::from_rgb(230, 150, 40)),
    );
}

/// Two-step inline delete: first click arms it, second confirms.
/// Returns true when deletion is confirmed.
pub fn confirm_delete_button(ui: &mut egui::Ui, armed: &mut Option<i64>, id: i64) -> bool {
    if *armed == Some(id) {
        let mut confirmed = false;
        if ui
            .button(egui::RichText::new("confirm").color(ui.visuals().error_fg_color))
            .clicked()
        {
            confirmed = true;
            *armed = None;
        }
        if ui.button("×").on_hover_text("cancel").clicked() {
            *armed = None;
        }
        confirmed
    } else {
        if ui.button("🗑").on_hover_text("delete").clicked() {
            *armed = Some(id);
        }
        false
    }
}

/// Shared row of entry-form fields (project, date, hours, dev, description).
/// Returns true if Enter was hit in the description or hours field.
pub fn entry_form_fields(
    app: &mut WorklogApp,
    ui: &mut egui::Ui,
    form: &mut crate::app::EntryForm,
    id_salt: &str,
    desc_focus_marker: Option<&'static str>,
    hours_focus_marker: Option<&'static str>,
) -> bool {
    let mut submit = false;
    let projects = app.projects.clone();
    ui.horizontal(|ui| {
        project_combo(ui, &format!("{id_salt}/proj"), &projects, &mut form.project_id);
        date_picker(ui, &format!("{id_salt}/date"), &mut form.work_date);

        let hours_resp = ui.add(
            egui::TextEdit::singleline(&mut form.hours_text)
                .hint_text("hrs")
                .desired_width(44.0),
        );
        if let Some(marker) = hours_focus_marker {
            app.apply_focus(marker, &hours_resp);
        }
        if enter_pressed(&hours_resp, ui) {
            submit = true;
        }
        if !form.hours_text.trim().is_empty() && form.parse_hours().is_none() {
            ui.label(egui::RichText::new("?").color(ui.visuals().error_fg_color))
                .on_hover_text("Hours must be a number like 1.5 or 1:30, up to 24");
        }

        dev_checkbox(ui, &mut form.is_dev);
    });
    let desc_resp = ui.add(
        egui::TextEdit::singleline(&mut form.description)
            .hint_text("what did you do?")
            .desired_width(f32::INFINITY),
    );
    if let Some(marker) = desc_focus_marker {
        app.apply_focus(marker, &desc_resp);
    }
    if enter_pressed(&desc_resp, ui) {
        submit = true;
    }
    submit
}
