fn main() {
    // Windows exe icon (Explorer, taskbar). The in-window icon is set at
    // runtime in main.rs; Linux gets its icon from a .desktop entry instead
    // (see README).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/timetrackerIcons/worklog.ico")
            .compile()
            .expect("embedding the Windows icon");
    }
    println!("cargo:rerun-if-changed=assets/timetrackerIcons/worklog.ico");
}
