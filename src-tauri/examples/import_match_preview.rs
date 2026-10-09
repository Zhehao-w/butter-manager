//! Read-only recommendation diagnostics: DATABASE SOURCE_ROOT TITLE_PREFIX...
//! No Database::open (migrations), plan generation, game/save writes or record export.
use butter_manager::{
    domain::{Game, PlayStatus, Result},
    importer::MatchIndex,
    scanner,
};
use rusqlite::{Connection, OpenFlags};
use std::{fs, path::Path, time::Instant};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 4 {
        return Err(butter_manager::domain::Error::Validation(
            "usage: import_match_preview DATABASE SOURCE_ROOT TITLE_PREFIX...".into(),
        ));
    }
    let db = Connection::open_with_flags(&args[1], OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.execute_batch("PRAGMA query_only=ON")?;
    let mut games = db.prepare("SELECT id,canonical_title,display_title,install_path,working_directory,current_version,main_executable,engine,mtool_target_exe FROM games")?
        .query_map([], |row| Ok(Game {
            id: row.get(0)?, canonical_title: row.get(1)?, display_title: row.get(2)?,
            install_path: row.get(3)?, working_directory: row.get(4)?, current_version: row.get(5)?,
            main_executable: row.get(6)?, engine: row.get(7)?, mtool_target_exe: row.get(8)?,
            version_source: String::new(), launch_type: String::new(), external_player: None,
            mtool_loader: None, created_at: String::new(), updated_at: String::new(),
            last_launched_at: None, play_status: PlayStatus::Unplayed, aliases: vec![], save_paths: vec![],
        }))?.collect::<std::result::Result<Vec<_>,_>>()?;
    for game in &mut games {
        for (table, column, values) in [
            ("aliases", "alias", &mut game.aliases),
            ("save_paths", "path", &mut game.save_paths),
        ] {
            *values = db
                .prepare(&format!("SELECT {column} FROM {table} WHERE game_id=?1"))?
                .query_map([&game.id], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
        }
    }
    let started = Instant::now();
    let mut index = MatchIndex::new(&games);
    println!("library={}, prepare={:?}", games.len(), started.elapsed());
    let mut sources =
        fs::read_dir(Path::new(&args[2]))?.collect::<std::result::Result<Vec<_>, _>>()?;
    sources.sort_by_key(|entry| entry.file_name());
    for entry in sources {
        if !entry.file_type()?.is_dir()
            || !args[3..]
                .iter()
                .any(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
        {
            continue;
        }
        let candidate = scanner::analyze_quick_controlled(&entry.path(), &|| false, &|_| {})?;
        let started = Instant::now();
        let matched = index.matches(&candidate, &|| false);
        println!(
            "source={}, results={}, matching={:?}",
            candidate.suggested_title,
            matched.len(),
            started.elapsed()
        );
        for (position, matched) in matched.iter().enumerate() {
            println!(
                "  {}. {} | auto={} | {}",
                position + 1,
                matched.title,
                matched.auto_associate,
                matched.reason
            );
        }
    }
    Ok(())
}
