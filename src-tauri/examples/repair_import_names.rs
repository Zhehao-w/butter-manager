//! Explicit, repeatable repair using the importer's normal identity journal.
//! No database backup/export and no reads or writes of game/save contents.
use butter_manager::{db::Database, importer::ImportStore};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Pass the existing app data directory")?,
    );
    let data = dunce::canonicalize(data)?;
    if !data.join("library.sqlite3").is_file() || !data.join("imports").is_dir() {
        return Err("Existing database and import records are required".into());
    }
    #[cfg(windows)]
    let _guard = butter_manager::data_directory::InstanceGuard::acquire(&data)?;
    let database = Arc::new(Mutex::new(Database::open(&data.join("library.sqlite3"))?));
    let before = database.lock().unwrap().games()?.len();
    let store = ImportStore::open(data.join("imports"))?;
    if store.has_pending_files()
        && !store.views().iter().all(|p| {
            p.items.iter().all(|i| {
                matches!(
                    i.state.as_str(),
                    "pending" | "completed" | "withdrawn" | "rolled_back" | "update_identity"
                )
            })
        })
    {
        return Err("Recover unfinished imports before repairing names".into());
    }
    let repaired = store.repair_completed_update_names(&database)?;
    let after = database.lock().unwrap().games()?.len();
    if before != after {
        return Err("Unexpected game count change".into());
    }
    println!(
        "Repaired {} completed updates; game count remains {}",
        repaired.len(),
        after
    );
    for title in repaired {
        println!("{title}");
    }
    Ok(())
}
