//! How long since the user last touched the machine.
//!
//! Three paths:
//! - Wayland sessions: a native `ext-idle-notify-v1` listener (the protocol
//!   Hyprland, KDE and other wlroots compositors implement) on a background
//!   thread. Neither of `user-idle`'s backends works there — the X screensaver
//!   extension is absent under XWayland and GNOME/wlroots don't provide
//!   `org.freedesktop.ScreenSaver`.
//! - X11 sessions and Windows: the `user-idle` crate, polled on demand.
//! - Anywhere it fails: `idle_secs` returns None and the caller turns the
//!   feature off for the session — never a per-poll warning.

pub enum IdleMonitor {
    #[cfg(target_os = "linux")]
    Wayland(wayland::WaylandIdle),
    /// Read `user-idle` on every call.
    Poll,
}

impl IdleMonitor {
    pub fn new() -> IdleMonitor {
        #[cfg(target_os = "linux")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
            && let Some(w) = wayland::WaylandIdle::start()
        {
            return IdleMonitor::Wayland(w);
        }
        IdleMonitor::Poll
    }

    /// Seconds of user inactivity. None = this platform can't say (caller
    /// should stop asking).
    pub fn idle_secs(&self) -> Option<u64> {
        match self {
            #[cfg(target_os = "linux")]
            IdleMonitor::Wayland(w) => w.idle_secs(),
            IdleMonitor::Poll => user_idle::UserIdle::get_time()
                .ok()
                .map(|t| t.as_seconds()),
        }
    }
}

#[cfg(target_os = "linux")]
mod wayland {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use wayland_client::globals::{registry_queue_init, GlobalListContents};
    use wayland_client::protocol::{wl_registry, wl_seat};
    use wayland_client::{Connection, Dispatch, QueueHandle};
    use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{
        self, ExtIdleNotificationV1,
    };
    use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

    /// The compositor only tells us "idle for ≥ this long", so reported idle
    /// time is granular; 30 s is plenty for a 10-minute pause threshold.
    const GRANULARITY_SECS: u64 = 30;

    pub struct WaylandIdle {
        /// Set to the instant the compositor declared us idle; None = active.
        idle_since: Arc<Mutex<Option<Instant>>>,
        /// Cleared by the listener thread if it dies.
        alive: Arc<AtomicBool>,
    }

    impl WaylandIdle {
        /// Connect and bind; None if the compositor lacks the protocol.
        /// The event loop then runs on its own thread for the app's lifetime.
        pub fn start() -> Option<WaylandIdle> {
            let conn = Connection::connect_to_env().ok()?;
            let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
            let qh = queue.handle();
            let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=9, ()).ok()?;
            let notifier: ExtIdleNotifierV1 = globals.bind(&qh, 1..=2, ()).ok()?;
            let _notification =
                notifier.get_idle_notification((GRANULARITY_SECS * 1000) as u32, &seat, &qh, ());

            let idle_since = Arc::new(Mutex::new(None));
            let alive = Arc::new(AtomicBool::new(true));
            let mut state = State {
                idle_since: idle_since.clone(),
            };
            let thread_alive = alive.clone();
            std::thread::Builder::new()
                .name("wayland-idle".into())
                .spawn(move || {
                    while queue.blocking_dispatch(&mut state).is_ok() {}
                    thread_alive.store(false, Ordering::Relaxed);
                })
                .ok()?;
            Some(WaylandIdle { idle_since, alive })
        }

        pub fn idle_secs(&self) -> Option<u64> {
            if !self.alive.load(Ordering::Relaxed) {
                return None;
            }
            let since = self.idle_since.lock().ok()?;
            Some(match *since {
                Some(at) => GRANULARITY_SECS + at.elapsed().as_secs(),
                None => 0,
            })
        }
    }

    struct State {
        idle_since: Arc<Mutex<Option<Instant>>>,
    }

    impl Dispatch<ExtIdleNotificationV1, ()> for State {
        fn event(
            state: &mut State,
            _proxy: &ExtIdleNotificationV1,
            event: ext_idle_notification_v1::Event,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<State>,
        ) {
            let value = match event {
                ext_idle_notification_v1::Event::Idled => Some(Instant::now()),
                ext_idle_notification_v1::Event::Resumed => None,
                _ => return,
            };
            if let Ok(mut since) = state.idle_since.lock() {
                *since = value;
            }
        }
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut State,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<State>,
        ) {
        }
    }

    wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
    wayland_client::delegate_noop!(State: ignore ExtIdleNotifierV1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_construction_never_panics() {
        // headless CI: Wayland absent → Poll; idle_secs is Some or None but
        // must not panic either way
        let m = IdleMonitor::new();
        let _ = m.idle_secs();
    }
}
