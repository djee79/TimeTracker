pub mod help;
pub mod journal;
pub mod projects;
pub mod reports;
pub mod tasks;

use chrono::{Datelike, NaiveDate};

use crate::app::WorklogApp;

// Focus markers: request_focus(marker) in an action, apply_focus(marker, resp)
// where the widget is rendered.
pub const FOCUS_CAPTURE_DESC: &str = "focus/capture_desc";
pub const FOCUS_TASK_TITLE: &str = "focus/task_title";
pub const FOCUS_BRIDGE_HOURS: &str = "focus/bridge_hours";

/// True when this widget lost focus because Enter was pressed — the
/// "type, hit Enter, saved" interaction every form here uses.
pub fn enter_pressed(resp: &egui::Response, ui: &egui::Ui) -> bool {
    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// Project picker over active projects. `selected` stays None until picked.
pub fn project_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    app_projects: &[crate::db::Project],
    selected: &mut Option<i64>,
) -> bool {
    let mut changed = false;
    let text = selected
        .and_then(|id| app_projects.iter().find(|p| p.id == id))
        .map(|p| p.code.clone())
        .unwrap_or_else(|| "project…".into());
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(text)
        .show_ui(ui, |ui| {
            for p in app_projects
                .iter()
                .filter(|p| p.status == crate::db::ProjectStatus::Active)
            {
                if ui
                    .selectable_value(selected, Some(p.id), p.label())
                    .clicked()
                {
                    changed = true;
                }
            }
        });
    changed
}

/// Like `project_combo` but with an "all projects" option, for filters.
pub fn project_filter_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    app_projects: &[crate::db::Project],
    selected: &mut Option<i64>,
) -> bool {
    let mut changed = false;
    let text = selected
        .and_then(|id| app_projects.iter().find(|p| p.id == id))
        .map(|p| p.code.clone())
        .unwrap_or_else(|| "all projects".into());
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(text)
        .show_ui(ui, |ui| {
            if ui.selectable_value(selected, None, "all projects").clicked() {
                changed = true;
            }
            for p in app_projects {
                if ui
                    .selectable_value(selected, Some(p.id), p.label())
                    .clicked()
                {
                    changed = true;
                }
            }
        });
    changed
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
