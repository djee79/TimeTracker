pub mod calendar;
pub mod date_picker;
pub mod help;
pub mod journal;
pub mod notes;
pub mod notes_editor;
pub mod projects;
pub mod reports;
pub mod search;
pub mod shell;
pub mod tasks;
pub mod theme;
pub mod widgets;

use chrono::NaiveDate;

use crate::app::WorklogApp;

// Focus markers: request_focus(marker) in an action, apply_focus(marker, resp)
// where the widget is rendered.
pub const FOCUS_CAPTURE_DESC: &str = "focus/capture_desc";
pub const FOCUS_TASK_TITLE: &str = "focus/task_title";
pub const FOCUS_TASK_EDIT: &str = "focus/task_edit";
pub const FOCUS_TASK_FILTER: &str = "focus/task_filter";
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

/// A project's palette entry.
pub fn swatch_of(p: &crate::db::Project) -> theme::Swatch {
    theme::swatch(p.color.as_deref(), p.id)
}

/// "● CODE" — dot in the base colour, code in mono 600, light colour.
pub fn project_job(code: &str, sw: theme::Swatch, size: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        "● ",
        0.0,
        egui::TextFormat {
            font_id: theme::font(size - 2.0, theme::Weight::Regular),
            color: sw.base,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        code,
        0.0,
        egui::TextFormat {
            font_id: theme::mono_semi(size),
            color: sw.light,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job
}

/// Scope tweak for form fields: dropdowns and the date button get the
/// input look (`INSET` fill, `BORDER_STRONG` stroke, 36 px tall).
pub fn field_style(ui: &mut egui::Ui) {
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.weak_bg_fill = theme::INSET;
    w.inactive.bg_stroke = egui::Stroke::new(1.0, theme::BORDER_STRONG);
    w.hovered.weak_bg_fill = theme::INSET;
    w.open.weak_bg_fill = theme::INSET;
    w.open.bg_stroke = egui::Stroke::new(1.0, theme::ACCENT);
    ui.spacing_mut().button_padding = egui::vec2(12.0, 9.0);
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
    let text: egui::WidgetText = match selected.and_then(|id| app_projects.iter().find(|p| p.id == id)) {
        Some(p) => project_job(&p.code, swatch_of(p), 13.0).into(),
        None => egui::RichText::new(all_option.unwrap_or("Project…"))
            .color(theme::TEXT_2)
            .into(),
    };
    let filter_id = egui::Id::new((id_salt, "picker_filter"));
    let open_id = egui::Id::new((id_salt, "picker_open"));
    let was_open: bool = ui.ctx().data(|d| d.get_temp(open_id)).unwrap_or(false);
    let inner = egui::ComboBox::from_id_salt(id_salt)
        .selected_text(text)
        .width(150.0)
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
                let mut label = project_job(&p.code, swatch_of(p), 13.0);
                label.append(
                    &format!("  {}", p.name),
                    0.0,
                    egui::TextFormat {
                        font_id: theme::font(13.0, theme::Weight::Regular),
                        color: theme::MUTED,
                        valign: egui::Align::Center,
                        ..Default::default()
                    },
                );
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
    project_picker(ui, id_salt, app_projects, selected, Some("All projects"), false)
}

/// The themed date button + popover (see `date_picker`), with marker dots
/// from the journal. Reads "Today, Nov 15" for today.
pub fn date_picker(
    app: &mut WorklogApp,
    ui: &mut egui::Ui,
    id_salt: &str,
    date: &mut NaiveDate,
) -> bool {
    date_picker::show(ui, id_salt, date, None, &mut |m| app.day_markers(m))
}

/// The dev flag, rendered the same everywhere: an "R&D" switch in a box.
pub fn dev_checkbox(ui: &mut egui::Ui, is_dev: &mut bool) {
    widgets::boxed_toggle(
        ui,
        is_dev,
        "R&D",
        "Flags this entry for the annual SR&ED / R&D export",
    );
}

/// Two-step inline delete: first click arms it, second confirms.
/// Returns true when deletion is confirmed.
pub fn confirm_delete_button(ui: &mut egui::Ui, armed: &mut Option<i64>, id: i64) -> bool {
    use widgets::icon;
    if *armed == Some(id) {
        let mut confirmed = false;
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("Delete").color(theme::DANGER).strong(),
                )
                .stroke(egui::Stroke::new(1.0, theme::alpha(theme::DANGER, 0.5)))
                .corner_radius(theme::R_INPUT),
            )
            .clicked()
        {
            confirmed = true;
            *armed = None;
        }
        if widgets::icon_button(ui, icon::X, "cancel").clicked() {
            *armed = None;
        }
        confirmed
    } else {
        if widgets::icon_button(ui, icon::TRASH, "delete").clicked() {
            *armed = Some(id);
        }
        false
    }
}

/// Shared entry-form fields (project, date, hours, description, R&D),
/// laid out inline — call inside a `horizontal_wrapped`. With `labels`
/// every field gets a 12 px label above it. `reserve` is the width left
/// for whatever the caller puts after the description (buttons).
/// Returns true if Enter was hit in the description or hours field.
#[allow(clippy::too_many_arguments)]
pub fn entry_form_fields(
    app: &mut WorklogApp,
    ui: &mut egui::Ui,
    form: &mut crate::app::EntryForm,
    id_salt: &str,
    desc_focus_marker: Option<&'static str>,
    hours_focus_marker: Option<&'static str>,
    labels: bool,
    reserve: f32,
) -> bool {
    let mut submit = false;
    let projects = app.projects.clone();
    let field = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        if labels {
            widgets::labeled(ui, label, |ui| add(ui));
        } else {
            add(ui);
        }
    };
    ui.scope(|ui| {
        field_style(ui);
        field(ui, "Project", &mut |ui| {
            project_combo(ui, &format!("{id_salt}/proj"), &projects, &mut form.project_id);
        });
        field(ui, "Date", &mut |ui| {
            date_picker(app, ui, &format!("{id_salt}/date"), &mut form.work_date);
        });
    });
    field(ui, "Hours", &mut |ui| {
        let hours_resp = widgets::input(
            ui,
            egui::TextEdit::singleline(&mut form.hours_text)
                .hint_text("0.0")
                .font(theme::mono(14.0))
                .horizontal_align(egui::Align::RIGHT)
                .desired_width(60.0),
        );
        if let Some(marker) = hours_focus_marker {
            app.apply_focus(marker, &hours_resp);
        }
        if enter_pressed(&hours_resp, ui) {
            submit = true;
        }
        if !form.hours_text.trim().is_empty() && form.parse_hours().is_none() {
            ui.painter().rect_stroke(
                hours_resp.rect,
                theme::R_INPUT,
                egui::Stroke::new(1.0, theme::DANGER),
                egui::StrokeKind::Inside,
            );
            hours_resp.on_hover_text("Hours must be a number like 1.5 or 1:30, up to 24");
        }
    });
    // in a wrapping row available_width() is the whole row; measure what's
    // left on this line from the cursor instead
    let left_on_line = ui.max_rect().right() - ui.cursor().min.x;
    let desc_w = (left_on_line - reserve).max(240.0);
    field(ui, "What did you do?", &mut |ui| {
        let desc_resp = widgets::input(
            ui,
            egui::TextEdit::singleline(&mut form.description)
                .hint_text("What did you do?")
                .desired_width(desc_w),
        );
        if let Some(marker) = desc_focus_marker {
            app.apply_focus(marker, &desc_resp);
        }
        if enter_pressed(&desc_resp, ui) {
            submit = true;
        }
    });
    field(ui, " ", &mut |ui| dev_checkbox(ui, &mut form.is_dev));
    submit
}
