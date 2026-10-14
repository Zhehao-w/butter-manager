use super::*;
use crate::db::Database;
use crate::domain::GameEdit;
use std::fs;

struct Fixture {
    db: Database,
    temp: tempfile::TempDir,
    old: PathBuf,
    new: PathBuf,
    ids: Vec<String>,
}
impl Fixture {
    fn new(count: usize) -> Self {
        // Close SQLite before the TempDir field is dropped on Windows.
        let temp = tempfile::Builder::new()
            .prefix("bulk-relocation-")
            .tempdir()
            .unwrap();
        let old = temp.path().join("old library");
        let new = temp.path().join("new library");
        fs::create_dir_all(&new).unwrap();
        let mut candidates = vec![];
        for index in 0..count {
            for root in [&old, &new] {
                let game = root.join(format!("RPG/彼女 {index}"));
                fs::create_dir_all(game.join("save")).unwrap();
                fs::write(game.join("Game.exe"), b"fixture game").unwrap();
                fs::write(game.join("save/progress"), b"unchanged progress").unwrap();
            }
            candidates.push(
                crate::scanner::analyze_directory(&old.join(format!("RPG/彼女 {index}"))).unwrap(),
            );
        }
        let mut db = Database::open(&temp.path().join("fixture.db")).unwrap();
        let ids = db.register(&candidates).unwrap();
        let mut settings = db.settings().unwrap();
        settings.game_root = path_text(&old).unwrap();
        db.save_settings(settings).unwrap();
        Self {
            temp,
            db,
            old,
            new,
            ids,
        }
    }
    fn plan(&self) -> Plan {
        self.db
            .preview_library_relocation(
                &path_text(&self.old).unwrap(),
                &path_text(&self.new).unwrap(),
                "fixture-token",
            )
            .unwrap()
    }
    fn edit(&mut self, index: usize, change: impl FnOnce(&mut GameEdit)) {
        let mut edit: GameEdit = serde_json::from_value(
            serde_json::to_value(self.db.game(&self.ids[index]).unwrap()).unwrap(),
        )
        .unwrap();
        change(&mut edit);
        self.db.edit_game(edit).unwrap();
    }
    fn tools(&mut self) -> PathBuf {
        for root in [&self.old, &self.new] {
            let tool = root.join("任意公共工具名");
            for file in [
                "MTool.exe",
                "loaders/inject.exe",
                "loaders/mzHook32.dll",
                "loaders/mzHook.dll",
            ] {
                fs::create_dir_all(tool.join(file).parent().unwrap()).unwrap();
                fs::write(tool.join(file), b"tool fixture").unwrap();
            }
        }
        let mut settings = self.db.settings().unwrap();
        settings.mtool_root = path_text(&self.old.join("任意公共工具名")).unwrap();
        self.db.save_settings(settings).unwrap();
        self.new.join("任意公共工具名")
    }
}

#[test]
fn bulk_missing_old_root_maps_many_nested_games_and_preserves_user_records() {
    let mut f = Fixture::new(600);
    f.edit(0, |edit| {
        edit.aliases.push("用户维护的别名".into());
    });
    let old_install = f.db.game(&f.ids[0]).unwrap().install_path;
    let external = path_text(&f.temp.path().join("external-save")).unwrap();
    f.edit(0, |edit| {
        edit.save_paths = vec![
            "<GAME>/save".into(),
            "save/progress".into(),
            format!("{old_install}/save/progress"),
            external.clone(),
            "%APPDATA%/RenPy/彼女".into(),
        ];
    });
    f.db.record_launch(&f.ids[0]).unwrap();
    f.db.set_duplicate_review(&f.ids[0], &f.ids[1], "confirmed")
        .unwrap();
    let connection = rusqlite::Connection::open(f.temp.path().join("fixture.db")).unwrap();
    connection.execute("INSERT INTO version_history(operation,game_id,old_version,new_version,status) VALUES('history',?1,'v1','v2','committed')", [&f.ids[0]]).unwrap();
    let before = f.db.games().unwrap();
    let history = f.db.launch_history(&f.ids[0]).unwrap();
    let versions = serde_json::to_string(&f.db.version_history(&f.ids[0]).unwrap()).unwrap();
    let reviews = serde_json::to_string(&f.db.duplicate_reviews().unwrap()).unwrap();
    fs::remove_dir_all(&f.old).unwrap(); // Only this isolated fixture, simulating a copied library.
    let started = std::time::Instant::now();
    let plan = f.plan();
    let elapsed = started.elapsed();
    eprintln!("600-game relocation preview: {elapsed:?}");
    assert!(elapsed < std::time::Duration::from_secs(10));
    assert!(plan.view.items.iter().all(|item| item.reason.is_none()));
    let report = f.db.apply_library_relocation(&plan).unwrap();
    assert_eq!((report.migrated, report.skipped), (600, 0));
    for original in before {
        let saved = f.db.game(&original.id).unwrap();
        let mut expected = original.clone();
        expected.install_path = path_text(
            &remap(&original.install_path, &f.old, &f.new)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        expected.save_paths = original
            .save_paths
            .iter()
            .map(|path| {
                crate::maintenance::rebase_save(
                    path,
                    Path::new(&original.install_path),
                    Path::new(&expected.install_path),
                )
            })
            .collect();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    assert_eq!(f.db.launch_history(&f.ids[0]).unwrap(), history);
    assert_eq!(
        serde_json::to_string(&f.db.version_history(&f.ids[0]).unwrap()).unwrap(),
        versions
    );
    assert_eq!(
        serde_json::to_string(&f.db.duplicate_reviews().unwrap()).unwrap(),
        reviews
    );
    assert_eq!(
        fs::read(f.new.join("RPG/彼女 0/save/progress")).unwrap(),
        b"unchanged progress"
    );
    assert!(!f.old.exists());
    let repeated = f.plan();
    let revision = f.db.relocation_revision().unwrap();
    assert_eq!(
        f.db.apply_library_relocation(&repeated).unwrap().migrated,
        0
    );
    assert_eq!(f.db.relocation_revision().unwrap(), revision);
}

#[test]
fn bulk_skips_missing_launch_invalid_work_and_uuid_conflicts_without_clearing_configs() {
    let mut f = Fixture::new(4);
    f.edit(0, |edit| edit.main_executable = None);
    fs::remove_dir_all(f.new.join("RPG/彼女 1")).unwrap();
    fs::remove_file(f.new.join("RPG/彼女 2/Game.exe")).unwrap();
    let target = f.new.join("RPG/彼女 3");
    f.db.register(&[crate::scanner::analyze_directory(&target).unwrap()])
        .unwrap();
    let before = f.db.games().unwrap();
    let plan = f.plan();
    assert_eq!(
        plan.view
            .items
            .iter()
            .filter(|item| item.reason.is_none())
            .count(),
        1
    );
    assert!(plan.view.items.iter().any(|item| item
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("UUID"))));
    let result = f.db.apply_library_relocation(&plan).unwrap();
    assert_eq!((result.migrated, result.skipped), (1, 4));
    for original in before.into_iter().filter(|game| game.id != f.ids[0]) {
        assert_eq!(
            serde_json::to_value(f.db.game(&original.id).unwrap()).unwrap(),
            serde_json::to_value(original).unwrap()
        );
    }
    assert!(f.db.game(&f.ids[0]).unwrap().main_executable.is_none());
}

#[test]
fn bulk_shared_loader_identity_is_unique_and_unrelated_directory_writes_do_not_expire_preview() {
    let mut f = Fixture::new(12);
    f.tools();
    for index in 0..12 {
        f.edit(index, |edit| {
            edit.launch_type = "MTOOL".into();
            edit.mtool_target_exe = Some("Game.exe".into());
            edit.mtool_loader = Some("loaders/mzHook32.dll".into());
        });
    }
    let plan = f.plan();
    assert_eq!(plan.view.mtool.state, "ready");
    let paths = plan
        .identities
        .iter()
        .map(|identity| &identity.path)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(paths.len(), plan.identities.len());
    assert_eq!(
        plan.identities
            .iter()
            .filter(|identity| identity.path.ends_with("mzHook32.dll"))
            .count(),
        1
    );
    fs::write(f.new.join("unrelated.txt"), b"not a launch configuration").unwrap();
    fs::write(
        f.new.join("RPG/彼女 0/unrelated.txt"),
        b"not a launch configuration",
    )
    .unwrap();
    assert_eq!(f.db.apply_library_relocation(&plan).unwrap().migrated, 12);
}

#[test]
fn bulk_progress_counts_failed_items_and_metadata_only_identity_matches_existing_identity() {
    let f = Fixture::new(4);
    fs::remove_dir_all(f.new.join("RPG/彼女 0")).unwrap();
    let mut updates = Vec::new();
    let plan = preview_with_progress(
        f.db.games().unwrap(),
        f.db.settings().unwrap(),
        f.db.relocation_revision().unwrap(),
        &path_text(&f.old).unwrap(),
        &path_text(&f.new).unwrap(),
        "progress-token",
        |progress| updates.push(progress),
    )
    .unwrap();
    assert_eq!(updates.first().unwrap().processed, 0);
    assert_eq!(
        (
            updates.last().unwrap().processed,
            updates.last().unwrap().total
        ),
        (5, 5)
    );
    assert!(updates
        .windows(2)
        .all(|pair| pair[0].processed <= pair[1].processed));
    assert!(updates.iter().any(|update| update.current == "公共 MTool"));
    assert_eq!(
        plan.view
            .items
            .iter()
            .filter(|item| item.reason.is_some())
            .count(),
        1
    );
    for guard in plan.identities {
        assert_eq!(guard.id, crate::importer::identity(&guard.path).unwrap());
    }
}

#[test]
fn bulk_mtool_and_qsp_preserve_modes_and_only_migrate_valid_shared_tools() {
    for mode in ["valid", "missing", "outside"] {
        let mut f = Fixture::new(2);
        let tool = f.tools();
        f.edit(0, |edit| {
            edit.launch_type = "MTOOL".into();
            edit.mtool_target_exe = Some("Game.exe".into());
            edit.mtool_loader = Some("loaders/mzHook32.dll".into());
        });
        for root in [&f.old, &f.new] {
            fs::write(root.join("RPG/彼女 1/story.qsp"), b"fixture qsp").unwrap();
        }
        f.edit(1, |edit| {
            edit.launch_type = "EXTERNAL_PLAYER".into();
            edit.engine = "QSP".into();
            edit.external_player = Some(crate::domain::ExternalPlayer {
                player_type: "QSP".into(),
                scope: "GAME_LOCAL".into(),
                game_file: Some("story.qsp".into()),
            });
        });
        if mode == "missing" {
            fs::remove_file(tool.join("loaders/inject.exe")).unwrap();
        }
        if mode == "outside" {
            let outside = f.temp.path().join("external-tools");
            fs::create_dir(&outside).unwrap();
            let mut settings = f.db.settings().unwrap();
            settings.mtool_root = path_text(&outside).unwrap();
            f.db.save_settings(settings).unwrap();
        }
        let before = f.db.settings().unwrap();
        let plan = f.plan();
        assert_eq!(
            plan.view.mtool.state,
            match mode {
                "valid" => "ready",
                "missing" => "blocked",
                _ => "unchanged",
            }
        );
        let result = f.db.apply_library_relocation(&plan).unwrap();
        assert_eq!(result.migrated, 2);
        assert_eq!(
            result.settings.mtool_root,
            if mode == "valid" {
                path_text(&tool).unwrap()
            } else {
                before.mtool_root
            }
        );
        assert_eq!(result.settings.mtool_injector, before.mtool_injector);
        assert_eq!(result.settings.mtool_runtime, before.mtool_runtime);
        assert_eq!(f.db.game(&f.ids[0]).unwrap().launch_type, "MTOOL");
        assert_eq!(
            f.db.game(&f.ids[1])
                .unwrap()
                .external_player
                .unwrap()
                .game_file
                .as_deref(),
            Some("story.qsp")
        );
    }
    for mode in ["target", "qsp", "work"] {
        let mut f = Fixture::new(1);
        if mode == "target" {
            fs::write(f.old.join("RPG/彼女 0/target.exe"), b"target").unwrap();
            f.edit(0, |edit| {
                edit.launch_type = "MTOOL".into();
                edit.mtool_target_exe = Some("target.exe".into());
            });
        } else if mode == "qsp" {
            fs::write(f.old.join("RPG/彼女 0/story.qsp"), b"qsp").unwrap();
            f.edit(0, |edit| {
                edit.launch_type = "EXTERNAL_PLAYER".into();
                edit.external_player = Some(crate::domain::ExternalPlayer {
                    player_type: "QSP".into(),
                    scope: "GAME_LOCAL".into(),
                    game_file: Some("story.qsp".into()),
                });
            });
        } else {
            fs::create_dir(f.old.join("RPG/彼女 0/working")).unwrap();
            f.edit(0, |edit| edit.working_directory = "working".into());
        }
        assert!(f.plan().view.items[0].reason.is_some(), "{mode}");
    }
}

#[test]
fn bulk_rejects_stale_database_or_replaced_targets_and_rolls_back_all_rows_on_failure() {
    for failure in [
        "database",
        "external",
        "replacement",
        "launch",
        "transaction",
    ] {
        let mut f = Fixture::new(2);
        let connection = rusqlite::Connection::open(f.temp.path().join("fixture.db")).unwrap();
        if failure == "transaction" {
            connection.execute_batch(&format!("CREATE TRIGGER fail_move BEFORE UPDATE OF install_path ON games WHEN NEW.id='{}' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;", f.ids[1])).unwrap();
        }
        let plan = f.plan();
        if failure == "database" {
            f.db.record_launch(&f.ids[0]).unwrap();
        }
        if failure == "external" {
            connection
                .execute(
                    "UPDATE games SET display_title='external edit' WHERE id=?1",
                    [&f.ids[0]],
                )
                .unwrap();
        }
        if failure == "replacement" {
            fs::rename(f.new.join("RPG/彼女 0"), f.new.join("preserved")).unwrap();
            fs::create_dir(f.new.join("RPG/彼女 0")).unwrap();
            fs::write(f.new.join("RPG/彼女 0/Game.exe"), b"fixture game").unwrap();
        }
        if failure == "launch" {
            fs::remove_file(f.new.join("RPG/彼女 0/Game.exe")).unwrap();
        }
        let games = serde_json::to_string(&f.db.games().unwrap()).unwrap();
        let settings = serde_json::to_string(&f.db.settings().unwrap()).unwrap();
        assert!(f.db.apply_library_relocation(&plan).is_err(), "{failure}");
        assert_eq!(
            serde_json::to_string(&f.db.games().unwrap()).unwrap(),
            games
        );
        assert_eq!(
            serde_json::to_string(&f.db.settings().unwrap()).unwrap(),
            settings
        );
    }
    for (activity, active, recovery) in [(1, false, false), (0, true, false), (0, false, true)] {
        assert!(ensure_idle(activity, active, recovery).is_err());
    }
    ensure_idle(0, false, false).unwrap();
}

#[cfg(windows)]
#[test]
fn bulk_windows_mapping_handles_drives_case_separators_and_component_boundaries() {
    let old = absolute("E:\\Butter\\").unwrap();
    let new = absolute("D:/Games/Butter").unwrap();
    assert_eq!(
        remap("e:/bUtTeR/RPG/Game B", &old, &new).unwrap().unwrap(),
        PathBuf::from("D:\\Games\\Butter\\RPG\\Game B")
    );
    assert!(remap("E:/Butter2/Game", &old, &new).unwrap().is_none());
    assert!(remap("E:/Butter/../outside", &old, &new).is_err());
    assert_eq!(absolute("\\\\?\\E:\\Butter").unwrap(), old);
}

#[cfg(windows)]
#[test]
fn bulk_refuses_junctions_in_game_targets_and_configured_paths() {
    use std::os::windows::process::CommandExt;
    for mode in ["root", "work"] {
        let f = Fixture::new(1);
        let target = f.new.join("RPG/彼女 0");
        let outside = f.temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let link = if mode == "root" {
            fs::remove_dir_all(&target).unwrap();
            target
        } else {
            target.join("save")
        };
        if mode == "work" {
            fs::remove_dir_all(&link).unwrap();
        }
        let result = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(&outside)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(result.status.success());
        if mode == "work" {
            let connection = rusqlite::Connection::open(f.temp.path().join("fixture.db")).unwrap();
            connection
                .execute(
                    "UPDATE games SET working_directory='save' WHERE id=?1",
                    [&f.ids[0]],
                )
                .unwrap();
        }
        assert!(f.plan().view.items[0].reason.is_some());
        fs::remove_dir(link).unwrap();
        assert!(outside.is_dir());
    }
}
