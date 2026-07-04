#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod db;
mod report;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([720.0, 480.0])
            .with_app_id("worklog"), // stable Wayland app_id, for WM rules/shortcuts
        ..Default::default()
    };
    eframe::run_native(
        "Worklog",
        native_options,
        Box::new(|cc| {
            let app = app::WorklogApp::new(cc).map_err(std::io::Error::other)?;
            Ok(Box::new(app))
        }),
    )
}
