//! Every migration file must actually be applied at startup.
//!
//! The list in `main.rs` is written by hand, and a migration added without
//! touching that list is invisible: the app starts, and then every query
//! against the new column fails at runtime. That happened once, with
//! `0004_draft_kind.sql`, so it is now checked mechanically.

use std::fs;

#[test]
fn every_migration_is_wired_into_startup() {
    let main = fs::read_to_string("src/main.rs").expect("read src/main.rs");

    let mut files: Vec<String> = fs::read_dir("migrations")
        .expect("read migrations/")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sql"))
        .collect();
    files.sort();

    assert!(!files.is_empty(), "no migrations found");

    let missing: Vec<&String> = files
        .iter()
        .filter(|n| !main.contains(&format!("migrations/{n}")))
        .collect();

    assert!(
        missing.is_empty(),
        "migrations exist but are never applied: {missing:?}\n\
         add include_str!(\"../migrations/<file>.sql\") to the list in main.rs"
    );
}
