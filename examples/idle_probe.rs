//! Verifies idle detection on this machine: which backend gets picked and
//! whether it reports. Run: cargo run --example idle_probe
#[path = "../src/idle.rs"]
mod idle;

fn main() {
    let m = idle::IdleMonitor::new();
    let backend = match &m {
        #[cfg(target_os = "linux")]
        idle::IdleMonitor::Wayland(_) => "wayland ext-idle-notify",
        idle::IdleMonitor::Poll => "user-idle poll",
    };
    println!("backend: {backend}");
    for _ in 0..3 {
        println!("idle_secs: {:?}", m.idle_secs());
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
