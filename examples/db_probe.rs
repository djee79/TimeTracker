// Headless diagnostic: open the real database exactly like the app does,
// print where it landed and its schema version. Run: cargo run --example db_probe
#![allow(dead_code, unused_imports)] // pulls in the whole db module, uses a sliver
#[path = "../src/db/mod.rs"]
mod db;

fn main() {
    match db::Db::open_default() {
        Ok(d) => {
            let cols = d.setting("__nonexistent__"); // force a query
            let _ = cols;
            println!("opened: {}", d.path().display());
        }
        Err(e) => println!("OPEN FAILED: {e}"),
    }
}
