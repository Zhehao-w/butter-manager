//! Read-only measurements. Without --root, generate isolated synthetic games in a temp directory.
use butter_manager::{
    db::Database,
    domain::{RegistrationSelection, Result},
    jobs, paths, registration,
};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let fixture = tempfile::tempdir()?;
    let real = args
        .iter()
        .position(|v| v == "--root")
        .and_then(|i| args.get(i + 1));
    let root = if let Some(root) = real {
        dunce::canonicalize(root)?
    } else {
        for i in 0..500 {
            let path = fixture.path().join(format!("游戏 {i} v1.02.3"));
            std::fs::create_dir_all(path.join("包装/www/img"))?;
            std::fs::write(path.join("包装/启动.exe"), b"fixture")?;
            for j in 0..50 {
                std::fs::write(path.join(format!("包装/www/img/{j}.png")), b"asset")?;
            }
        }
        dunce::canonicalize(fixture.path())?
    };
    let worker_options = if real.is_some() {
        vec![2]
    } else {
        vec![1, 2, 4]
    };
    for workers in worker_options {
        let manager = jobs::TaskManager::default();
        let job = manager.begin("scan", paths::path_text(&root)?)?;
        let task = Arc::clone(&job);
        let handle = std::thread::spawn(move || {
            let result = jobs::run_scan(&task, workers, &[]);
            task.finish(result.map(|_| vec![]).map_err(|e| e.to_string()));
        });
        let started = Instant::now();
        let mut first_ready_ms = None;
        let mut latest = HashMap::new();
        let mut cursor = 0;
        loop {
            let page = job.page(cursor);
            cursor = page.next_cursor;
            for candidate in page.changes {
                if candidate.status == "ready" && first_ready_ms.is_none() {
                    first_ready_ms = Some(started.elapsed().as_millis());
                }
                latest.insert(candidate.install_path.clone(), candidate);
            }
            if !job.is_active() && cursor >= page.change_count {
                break;
            }
            if started.elapsed() >= Duration::from_secs(60) {
                job.request_cancel();
            }
            if cursor >= page.change_count {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        handle.join().expect("benchmark worker panicked");
        let page = job.page(cursor);
        let entries: usize = latest.values().map(|v| v.entries_scanned).sum();
        let recommended = latest
            .values()
            .filter(|v| !v.executables.is_empty())
            .count();
        println!(
            "{}",
            serde_json::json!({"fixture": real.is_none(), "workers": workers, "status": page.status,
            "directories": page.total, "processed": page.processed, "entries_scanned": entries,
            "recommended_exe": recommended, "elapsed_ms": page.elapsed_ms, "first_ready_ms": first_ready_ms,
            "warnings": page.warnings, "incomplete": latest.values().filter(|v| v.status == "incomplete").count()})
        );
        if real.is_none() {
            let started = Instant::now();
            let mut entries = vec![];
            for candidate in latest.into_values() {
                let selection = RegistrationSelection {
                    install_path: candidate.install_path.clone(),
                    executable: candidate
                        .executables
                        .first()
                        .map(|v| v.relative_path.clone()),
                    external_player: butter_manager::external_player::qsp_config(&candidate),
                    exe_override: false,
                    version: candidate.suggested_version.clone(),
                    version_override: false,
                };
                entries.push(registration::prepare(
                    Path::new(&root),
                    candidate,
                    selection,
                )?);
            }
            let validation_ms = started.elapsed().as_millis();
            let mut db = Database::open(&fixture.path().join(format!("benchmark-{workers}.db")))?;
            let started = Instant::now();
            let ids = db.register_entries(&entries, &|| false, &|_, _| {})?;
            let write_ms = started.elapsed().as_millis();
            let started = Instant::now();
            let loaded = db.games_by_ids(&ids)?;
            println!(
                "{}",
                serde_json::json!({"registered": loaded.len(), "validation_ms": validation_ms,
                "transaction_ms": write_ms, "batch_query_ms": started.elapsed().as_millis()})
            );
            let checked = manager.begin("scan", paths::path_text(&root)?)?;
            let result = jobs::run_scan(&checked, workers, &loaded);
            checked.finish(result.map(|_| vec![]).map_err(|e| e.to_string()));
            let page = checked.page(0);
            assert_eq!(page.path_checks.len(), loaded.len());
            println!(
                "{}",
                serde_json::json!({"fixture": true, "workers": workers,
                "combined_scan_ms": page.elapsed_ms, "directories": page.total,
                "checked_games": page.path_checks.len(), "status": page.status})
            );
        }
    }
    Ok(())
}
