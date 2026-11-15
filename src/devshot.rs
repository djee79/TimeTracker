//! Debug builds only: render one screen to a PNG and exit — a way to check
//! the UI without clicking through it. Point it at a throwaway data dir:
//!
//! ```sh
//! XDG_DATA_HOME=/tmp/wl WORKLOG_SHOT=/tmp/journal.png WORKLOG_SHOT_VIEW=journal cargo run
//! ```
//!
//! `WORKLOG_SHOT_VIEW`: journal | tasks | reports | calendar | projects | notes | help.

use std::cell::RefCell;

use crate::app::{Tab, WorklogApp};

struct Shot {
    path: String,
    frames: u32,
    requested: bool,
}

thread_local! {
    static SHOT: RefCell<Option<Shot>> = RefCell::new(
        std::env::var("WORKLOG_SHOT").ok().map(|path| Shot { path, frames: 0, requested: false })
    );
}

pub fn active() -> bool {
    SHOT.with(|s| s.borrow().is_some())
}

/// Called every frame once the app is running.
pub fn tick(app: &mut WorklogApp, ctx: &egui::Context) {
    SHOT.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(shot) = cell.as_mut() else {
            return;
        };
        shot.frames += 1;
        if shot.frames == 1 {
            let view = std::env::var("WORKLOG_SHOT_VIEW").unwrap_or_default();
            match view.as_str() {
                "tasks" => app.tab = Tab::Tasks,
                "reports" => app.tab = Tab::Reports,
                "projects" => app.show_projects = true,
                "help" => app.show_help = true,
                "bridge" => {
                    if let Some(t) = app.active_task().cloned() {
                        app.stop_and_log(&t);
                    }
                }
                "notes" => {
                    app.tab = Tab::Tasks;
                    if let Some(t) = app.open_tasks.first().cloned() {
                        app.open_task_details(&t);
                    }
                }
                _ => {}
            }
        }
        ctx.request_repaint();
        if shot.frames == 12 && !shot.requested {
            shot.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Err(e) =
                image::save_buffer(&shot.path, &bytes, w as u32, h as u32, image::ColorType::Rgba8)
            {
                eprintln!("screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}
