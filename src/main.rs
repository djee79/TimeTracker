#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod db;
mod idle;
mod report;
mod ui;

use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;

/// The bundled window icon (title bar / taskbar where the OS supports it;
/// on Wayland the .desktop entry provides it instead — see README).
fn app_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/timetrackerIcons/worklog-256.png");
    let img = image::load_from_memory(bytes)
        .expect("bundled icon decodes")
        .into_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

use ui::{load_splash, Splash};

/// With the splash enabled it stays up at least this long — long enough to
/// actually see; the boot itself usually finishes much sooner. With the
/// splash disabled it only shows for however long the boot really takes.
const MIN_SPLASH_MS: u128 = 1400;

/// Boots the database on a background thread while the splash shows, then
/// becomes the real app. Failure to open the database is the only fatal path.
#[allow(clippy::large_enum_variant)] // exactly one Boot exists, briefly
enum Boot {
    Loading {
        rx: mpsc::Receiver<Result<(db::Db, app::BootReport), String>>,
        booted: Option<Result<(db::Db, app::BootReport), String>>,
        started: Instant,
        splash: Option<Splash>,
    },
    Running(Box<app::WorklogApp>),
    Failed(String),
}

impl eframe::App for Boot {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Boot::Loading { rx, booted, started, splash } = self {
            if booted.is_none()
                && let Ok(result) = rx.try_recv()
            {
                *booted = Some(result);
            }
            // "show splash at startup" off → hand over the moment the boot
            // finishes instead of holding for the minimum display time
            let skip_hold = matches!(booted, Some(Ok((_, report))) if !report.show_splash);
            if (started.elapsed().as_millis() >= MIN_SPLASH_MS || skip_hold)
                && let Some(result) = booted.take()
            {
                *self = match result {
                    Ok((db, report)) => {
                        Boot::Running(Box::new(app::WorklogApp::from_db(db, report)))
                    }
                    Err(e) => Boot::Failed(e),
                };
            } else {
                let ctx = root.ctx().clone();
                let splash = splash.get_or_insert_with(|| load_splash(&ctx));
                let ready = booted.is_some();
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(splash.background))
                    .show(root, |ui_| {
                        let rect = egui::Rect::from_center_size(
                            ui_.max_rect().center(),
                            egui::vec2(560.0, 360.0),
                        );
                        let painter = ui_.painter();
                        painter.image(
                            splash.texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                        // The live layer the art deliberately leaves out:
                        // real version, real status, real progress.
                        let dim = egui::Color32::from_rgb(138, 122, 109);
                        let accent = egui::Color32::from_rgb(217, 95, 14);
                        let track_bg = egui::Color32::from_rgb(66, 54, 45);
                        let track = egui::Rect::from_center_size(
                            egui::pos2(rect.center().x, rect.top() + 300.0),
                            egui::vec2(180.0, 3.0),
                        );
                        painter.rect_filled(track, 1.5, track_bg);
                        if ready {
                            painter.rect_filled(track, 1.5, accent);
                        } else {
                            // indeterminate: a segment gliding back and forth
                            let t = ui_.input(|i| i.time) as f32;
                            let width = 60.0;
                            let span = track.width() - width;
                            let x = track.left() + span * (0.5 - 0.5 * (t * 2.4).cos());
                            let segment = egui::Rect::from_min_size(
                                egui::pos2(x, track.top()),
                                egui::vec2(width, track.height()),
                            );
                            painter.rect_filled(segment, 1.5, accent);
                        }
                        painter.text(
                            egui::pos2(rect.left() + 24.0, rect.bottom() - 18.0),
                            egui::Align2::LEFT_BOTTOM,
                            if ready { "ready" } else { "opening worklog.db…" },
                            egui::FontId::monospace(11.0),
                            dim,
                        );
                        painter.text(
                            egui::pos2(rect.right() - 24.0, rect.bottom() - 18.0),
                            egui::Align2::RIGHT_BOTTOM,
                            concat!("v", env!("CARGO_PKG_VERSION")),
                            egui::FontId::monospace(11.0),
                            dim,
                        );
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                return;
            }
        }
        match self {
            Boot::Running(app) => app.ui(root, frame),
            Boot::Failed(e) => {
                egui::CentralPanel::default().show(root, |ui_| {
                    ui_.heading("Worklog could not start");
                    ui_.label(e.as_str());
                });
            }
            Boot::Loading { .. } => unreachable!("handled above"),
        }
    }
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([720.0, 480.0])
            .with_icon(app_icon())
            .with_app_id("worklog"), // stable Wayland app_id, for WM rules/shortcuts
        ..Default::default()
    };
    eframe::run_native(
        "Worklog",
        native_options,
        Box::new(|_cc| {
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("worklog-boot".into())
                .spawn(move || {
                    let _ = tx.send(app::WorklogApp::boot_db());
                })
                .expect("spawning the boot thread");
            Ok(Box::new(Boot::Loading {
                rx,
                booted: None,
                started: Instant::now(),
                splash: None,
            }))
        }),
    )
}
