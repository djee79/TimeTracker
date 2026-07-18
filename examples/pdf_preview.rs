//! Renders sample weekly + annual PDFs with realistic data, for eyeballing
//! the layout: `cargo run --example pdf_preview -- /tmp/out-dir`
#![allow(dead_code, unused_imports)] // pulls in whole modules, uses a sliver
#[path = "../src/db/mod.rs"]
mod db;
#[path = "../src/report/mod.rs"]
mod report;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let db_path = format!("{dir}/preview.db");
    let _ = std::fs::remove_file(&db_path);
    let db = db::Db::open(std::path::PathBuf::from(db_path)).unwrap();
    seed(&db);
    report::pdf::weekly_pdf(&db, "2026-07-13".parse().unwrap(), true)
        .unwrap()
        .render_to_file(format!("{dir}/weekly.pdf"))
        .unwrap();
    report::pdf::annual_pdf(&db, 2026, true)
        .unwrap()
        .render_to_file(format!("{dir}/annual.pdf"))
        .unwrap();
    println!("wrote {dir}/weekly.pdf and {dir}/annual.pdf");
}

fn seed(db: &db::Db) {
    use db::Priority;
    let d = |s: &str| s.parse::<chrono::NaiveDate>().unwrap();
    let p1 = db.insert_project("HICA-003", "Hydro interface", None).unwrap();
    let p2 = db.insert_project("DEV-001", "Dev Work", None).unwrap();
    let t = db.insert_task(p2, "Ansible server, update the playbooks", Priority::High).unwrap();
    db.set_task_details(
        t,
        "## Plan\nRework the deploy pipeline so staging matches prod.\n\n**Key risks**: the *inventory* file is shared with the old cluster.\n\n### Steps\n1. Snapshot current configs\n2. Migrate `group_vars` to the new layout\n3. Validate with `ansible-lint`\n\n- [x] backups verified\n- [ ] dry-run on staging\n\n> Note from ops: keep the maintenance window under 2 h.\n\n```\nansible-playbook site.yml --check --diff\n```\n\n| Host | Role |\n| --- | --- |\n| web01 | frontend |\n| db02 | postgres |\n\n---\nLast reviewed 2026-07-12.",
    )
    .unwrap();
    let t2 = db.insert_task(p1, "Configure the Navisworks colors", Priority::Normal).unwrap();
    db.set_task_details(t2, "Palette agreed with the client:\n\n- piping in **red**\n- steel in *gray*\n- `#FFAA00` for tanks").unwrap();

    db.insert_log_entry(d("2026-07-13"), p2, "Ansible server, update the playbooks", 5.0, true, Some(t)).unwrap();
    db.insert_log_entry(d("2026-07-14"), p1, "Configure the Navisworks colors", 2.0, false, Some(t2)).unwrap();
    db.insert_log_entry(d("2026-07-14"), p2, "Need to create a new interface", 1.4, true, None).unwrap();
    db.insert_log_entry(d("2026-07-15"), p1, "site visit, punch list", 4.0, false, None).unwrap();
    db.insert_log_entry(d("2026-07-16"), p2, "Ansible server, update the playbooks", 2.5, true, Some(t)).unwrap();
}
