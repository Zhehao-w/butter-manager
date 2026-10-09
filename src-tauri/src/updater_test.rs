use super::*;
use crate::{domain::GameEdit, jobs::TaskManager};

struct Fixture {
    _temp: tempfile::TempDir,
    store: ImportStore,
    db: Arc<Mutex<Database>>,
    tasks: TaskManager,
    root: PathBuf,
    source: PathBuf,
    external: PathBuf,
    id: String,
}
impl Fixture {
    fn new() -> Self {
        Self::with_library_root(None)
    }
    fn with_library_root(library: Option<&Path>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = library
            .map(Path::to_path_buf)
            .unwrap_or_else(|| temp.path().join("游戏库 日本語"));
        let old = root.join("魔法少女");
        let source = temp.path().join("下载/新版本 v2");
        let external = temp.path().join("外部存档");
        let data = if library.is_some() {
            root.parent().unwrap().join("data")
        } else {
            temp.path().join("data")
        };
        for path in [&old, &source] {
            fs::create_dir_all(path.join("www/save")).unwrap();
            fs::write(path.join("游戏.html"), b"old game").unwrap();
        }
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("global.sav"), b"external progress").unwrap();
        fs::write(old.join("www/save/進行.sav"), b"old progress").unwrap();
        fs::write(source.join("www/save/進行.sav"), b"bundled sample").unwrap();
        fs::write(source.join("游戏.html"), b"new game").unwrap();
        fs::create_dir_all(&data).unwrap();
        let mut db = Database::open(&data.join("fixture.db")).unwrap();
        db.save_settings(Settings {
            game_root: paths::path_text(&root).unwrap(),
            mtool_injector: "loaders/inject.exe".into(),
            mtool_runtime: "MTool.exe".into(),
            ..Settings::default()
        })
        .unwrap();
        let id = db
            .register(&[scanner::analyze_directory(&old).unwrap()])
            .unwrap()
            .remove(0);
        let game = db.game(&id).unwrap();
        db.edit_game(GameEdit {
            id: id.clone(),
            canonical_title: "魔法少女".into(),
            display_title: "我的游戏".into(),
            current_version: "v1".into(),
            engine: "RPG Maker MV".into(),
            play_status: crate::domain::PlayStatus::Completed,
            main_executable: Some("游戏.html".into()),
            working_directory: ".".into(),
            launch_type: "DIRECT".into(),
            external_player: None,
            mtool_target_exe: None,
            mtool_loader: None,
            aliases: vec!["my alias".into()],
            save_paths: vec![
                "<GAME>/www/save".into(),
                paths::path_text(&external).unwrap(),
            ],
        })
        .unwrap();
        assert_eq!(db.game(&id).unwrap().created_at, game.created_at);
        db.record_launch(&id).unwrap();
        let store = ImportStore::open(data.join("imports")).unwrap();
        Self {
            _temp: temp,
            store,
            db: Arc::new(Mutex::new(db)),
            tasks: TaskManager::default(),
            root,
            source,
            external,
            id,
        }
    }
    fn plan(&self, preserve: bool, confirmed: bool) -> String {
        self.plan_with(preserve, confirmed, |_| {})
    }
    fn plan_with(
        &self,
        preserve: bool,
        confirmed: bool,
        configure: impl FnOnce(&mut Selection),
    ) -> String {
        let candidate = scanner::analyze_directory(&self.source).unwrap();
        let mut selection = Selection {
            source: paths::path_text(&self.source).unwrap(),
            title: "incoming name".into(),
            target_name: "ignored incoming folder".into(),
            version: "v2".into(),
            engine: "RPG Maker MV".into(),
            executable: "游戏.html".into(),
            working_directory: None,
            mtool_loader: None,
            external_player: None,
            mtool: false,
            existing_id: Some(self.id.clone()),
            new_override: false,
            preserve_saves: preserve,
            saves_confirmed: confirmed,
        };
        configure(&mut selection);
        let settings = self.db.lock().unwrap().settings().unwrap();
        let games = self.db.lock().unwrap().games().unwrap();
        let job = self
            .tasks
            .begin("import_plan", settings.game_root.clone())
            .unwrap();
        self.store
            .prepare(&job, settings, vec![candidate], vec![selection], &games)
            .unwrap();
        job.finish(Ok(vec![]));
        job.id.clone()
    }
    fn apply(&self, id: &str) -> Result<Vec<String>> {
        let job = self
            .tasks
            .begin("import_apply", paths::path_text(&self.root).unwrap())
            .unwrap();
        let result = self.store.apply(id, &job, &self.db);
        job.finish(
            result
                .as_ref()
                .map(Clone::clone)
                .map_err(ToString::to_string),
        );
        result
    }
    fn rollback(&self, id: &str) -> Result<Vec<String>> {
        let job = self
            .tasks
            .begin("import_rollback", paths::path_text(&self.root).unwrap())
            .unwrap();
        let result = self.store.rollback(id, 0, &job, &self.db);
        job.finish(
            result
                .as_ref()
                .map(Clone::clone)
                .map_err(ToString::to_string),
        );
        result
    }
    fn old(&self) -> PathBuf {
        self.root.join("魔法少女")
    }
    fn use_legacy_journal(&mut self, id: &str) {
        let plan = self.store.get(id).unwrap();
        let job = Job::recovery(plan.root.clone());
        let mut record = serde_json::to_value(&plan).unwrap();
        let item = record["items"][0].as_object_mut().unwrap();
        item.remove("basic_transfer");
        let update = item["update"].as_object_mut().unwrap();
        for field in [
            "lightweight",
            "incoming_saves",
            "rollback_originals",
            "inherit_launch_config",
            "mtool_loader_launch_scoped",
        ] {
            update.remove(field);
        }
        update.insert(
            "old_digest".into(),
            serde_json::Value::String(
                super::updater::digest(&inventory(&self.old(), &job).unwrap()).unwrap(),
            ),
        );
        update.insert(
            "required_bytes".into(),
            serde_json::json!(2 * plan.items[0].bytes + 2 * b"old progress".len() as u64),
        );
        fs::write(
            self.store.directory.join(format!("{id}.json")),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        self.store = ImportStore::open(self.store.directory.clone()).unwrap();
        assert!(self.store.recovery_issues().is_empty());
    }
}

fn write_pe_fixture(path: &Path, machine: u16) {
    let mut bytes = vec![0u8; 70];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&machine.to_le_bytes());
    fs::write(path, bytes).unwrap();
}

fn manual_launch_fixture(mtool: bool) -> Fixture {
    let f = Fixture::new();
    for (root, executable) in [(&f.old(), "original"), (&f.source, "replacement")] {
        fs::create_dir(root.join(executable)).unwrap();
        fs::create_dir(root.join("manual-cwd")).unwrap();
        fs::create_dir(root.join("override-cwd")).unwrap();
        write_pe_fixture(&root.join(executable).join("Game.exe"), 0x8664);
    }
    let shared = f._temp.path().join("shared-tool");
    fs::create_dir_all(shared.join("loaders")).unwrap();
    for loader in ["custom.dll", "replacement.dll"] {
        write_pe_fixture(&shared.join("loaders").join(loader), 0x8664);
    }
    let mut db = f.db.lock().unwrap();
    let mut settings = db.settings().unwrap();
    settings.mtool_root = paths::path_text(&shared).unwrap();
    db.save_settings(settings).unwrap();
    let game = db.game(&f.id).unwrap();
    db.edit_game(GameEdit {
        id: game.id,
        canonical_title: game.canonical_title,
        display_title: game.display_title,
        current_version: game.current_version,
        engine: game.engine,
        play_status: game.play_status,
        main_executable: Some("original/Game.exe".into()),
        working_directory: "manual-cwd".into(),
        launch_type: if mtool { "MTOOL" } else { "DIRECT" }.into(),
        external_player: None,
        mtool_target_exe: mtool.then(|| "original/Game.exe".into()),
        mtool_loader: mtool.then(|| "loaders/custom.dll".into()),
        aliases: game.aliases,
        save_paths: game.save_paths,
    })
    .unwrap();
    drop(db);
    f
}

#[test]
fn update_inherits_manual_launch_configuration_across_exe_changes_and_restart_then_rolls_back() {
    for (mtool, legacy) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut f = manual_launch_fixture(mtool);
        let before = f.db.lock().unwrap().version_config(&f.id).unwrap();
        let id = f.plan_with(true, true, |selection| {
            selection.executable = "replacement/Game.exe".into();
            selection.mtool = mtool;
        });
        assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
        if legacy {
            f.use_legacy_journal(&id);
        }
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        f.apply(&id).unwrap();
        let updated = f.db.lock().unwrap().game(&f.id).unwrap();
        assert_eq!(updated.working_directory, before.working_directory);
        assert_eq!(updated.mtool_loader, before.mtool_loader);
        assert_eq!(
            updated.main_executable.as_deref(),
            Some("replacement/Game.exe")
        );
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        f.rollback(&id).unwrap();
        assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), before);
    }
}

#[test]
fn explicit_update_launch_overrides_and_automatic_loader_choice_do_not_change_rollback_configuration(
) {
    for (mtool, loader, working_directory) in [
        (false, None, Some("override-cwd")),
        (true, Some("loaders/replacement.dll"), None),
        (true, None, Some("override-cwd")),
        (true, Some(""), None),
    ] {
        let f = manual_launch_fixture(mtool);
        let before = f.db.lock().unwrap().version_config(&f.id).unwrap();
        let id = f.plan_with(true, true, |selection| {
            selection.executable = "replacement/Game.exe".into();
            selection.mtool = mtool;
            selection.working_directory = working_directory.map(str::to_owned);
            selection.mtool_loader = loader.map(str::to_owned);
        });
        assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
        f.apply(&id).unwrap();
        let updated = f.db.lock().unwrap().game(&f.id).unwrap();
        assert_eq!(
            updated.working_directory,
            working_directory.unwrap_or("manual-cwd")
        );
        assert_eq!(
            updated.mtool_loader.as_deref(),
            if mtool && loader.is_none() {
                Some("loaders/custom.dll")
            } else {
                loader.filter(|value| !value.is_empty())
            }
        );
        f.rollback(&id).unwrap();
        assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), before);
    }
}

#[test]
fn mtool_to_direct_update_discards_loader_and_restores_old_configuration_on_rollback() {
    assert_launch_mode_transition(true, "DIRECT");
}

#[test]
fn mtool_to_qsp_update_discards_loader_and_restores_old_configuration_on_rollback() {
    assert_launch_mode_transition(true, "EXTERNAL_PLAYER");
}

#[test]
fn direct_to_mtool_update_accepts_loader_override_or_automatic_detection_and_rolls_back() {
    assert_launch_mode_transition(false, "MTOOL");
}

fn assert_launch_mode_transition(old_mtool: bool, launch_type: &str) {
    for loader in [None, Some("loaders/replacement.dll"), Some("")] {
        let mut f = manual_launch_fixture(old_mtool);
        let before = f.db.lock().unwrap().version_config(&f.id).unwrap();
        if launch_type == "EXTERNAL_PLAYER" {
            fs::write(f.source.join("冒険.qsp"), b"new qsp game").unwrap();
        }
        let id = f.plan_with(true, true, |selection| {
            selection.executable = "replacement/Game.exe".into();
            selection.mtool = launch_type == "MTOOL";
            selection.mtool_loader = loader.map(str::to_owned);
            if launch_type == "EXTERNAL_PLAYER" {
                selection.external_player = Some(crate::domain::ExternalPlayer {
                    player_type: "QSP".into(),
                    scope: "GAME_LOCAL".into(),
                    game_file: Some("冒険.qsp".into()),
                });
            }
        });
        assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
        if launch_type == "EXTERNAL_PLAYER" {
            let mut item = f.store.get(&id).unwrap().items.remove(0);
            item.selection.mtool = true;
            let config = super::updater::configuration(&item).unwrap();
            assert_eq!(config.launch_type, "EXTERNAL_PLAYER");
            assert!(config.mtool_loader.is_none());
        }
        f.apply(&id).unwrap();
        let updated = f.db.lock().unwrap().version_config(&f.id).unwrap();
        assert_eq!(updated.launch_type, launch_type);
        assert_eq!(updated.working_directory, before.working_directory);
        assert_eq!(
            updated.mtool_loader.as_deref(),
            if launch_type == "MTOOL" {
                loader.filter(|value| !value.is_empty())
            } else {
                None
            }
        );
        if let Some(player) = &updated.external_player {
            assert_eq!(player.game_file.as_deref(), Some("冒険.qsp"));
        }
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        f.rollback(&id).unwrap();
        assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), before);
    }
}

#[test]
fn unscoped_loader_journals_resume_and_roll_back_without_reinterpreting_committed_configuration() {
    for launch_type in ["DIRECT", "EXTERNAL_PLAYER"] {
        for checkpoint in [None, Some("update_copy")] {
            let mut f = manual_launch_fixture(true);
            let before = f.db.lock().unwrap().version_config(&f.id).unwrap();
            if launch_type == "EXTERNAL_PLAYER" {
                fs::write(f.source.join("冒険.qsp"), b"new qsp game").unwrap();
            }
            let id = f.plan_with(true, true, |selection| {
                selection.executable = "replacement/Game.exe".into();
                if launch_type == "EXTERNAL_PLAYER" {
                    selection.external_player = Some(crate::domain::ExternalPlayer {
                        player_type: "QSP".into(),
                        scope: "GAME_LOCAL".into(),
                        game_file: Some("冒険.qsp".into()),
                    });
                }
            });
            if let Some(state) = checkpoint {
                let job = f
                    .tasks
                    .begin("import_apply", paths::path_text(&f.root).unwrap())
                    .unwrap();
                let mut plan = f.store.get(&id).unwrap();
                FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some(state.into()));
                assert!(f.store.update_one(&mut plan, 0, &job, &f.db).is_err());
                job.finish(Err("simulated crash".into()));
                assert_eq!(f.store.get(&id).unwrap().items[0].state, state);
            }
            use_unscoped_loader_journal(&mut f, &id);
            f.apply(&id).unwrap();
            let updated = f.db.lock().unwrap().version_config(&f.id).unwrap();
            assert_eq!(updated.launch_type, launch_type);
            assert_eq!(
                updated.mtool_loader,
                if checkpoint.is_some() {
                    before.mtool_loader.clone()
                } else {
                    None
                }
            );
            if checkpoint.is_some() {
                // Reopen a completed old journal too: rollback compares its historical config.
                use_unscoped_loader_journal(&mut f, &id);
            }
            f.rollback(&id).unwrap();
            assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), before);
        }
    }
}

fn use_unscoped_loader_journal(f: &mut Fixture, id: &str) {
    let mut record = serde_json::to_value(f.store.get(id).unwrap()).unwrap();
    record["items"][0]["update"]
        .as_object_mut()
        .unwrap()
        .remove("mtool_loader_launch_scoped");
    fs::write(
        f.store.directory.join(format!("{id}.json")),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    f.store = ImportStore::open(f.store.directory.clone()).unwrap();
    assert!(f.store.recovery_issues().is_empty());
}

#[test]
fn moved_data_directory_preserves_unfinished_update_recovery_and_rollback() {
    let mut f = manual_launch_fixture(true);
    let before = f.db.lock().unwrap().version_config(&f.id).unwrap();
    let id = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.mtool = true;
    });
    let job = f
        .tasks
        .begin("import_apply", paths::path_text(&f.root).unwrap())
        .unwrap();
    let mut plan = f.store.get(&id).unwrap();
    FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some("update_publish".into()));
    assert!(f.store.update_one(&mut plan, 0, &job, &f.db).is_err());
    job.finish(Err("simulated crash".into()));
    let old_data = f.store.directory.parent().unwrap().to_path_buf();
    // Close the fixture's SQLite connection before moving its directory, including WAL/SHM.
    drop(Arc::try_unwrap(f.db).ok().unwrap().into_inner().unwrap());
    let moved = f._temp.path().join("moved manager 日本語/data");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&old_data, &moved).unwrap();
    f.db = Arc::new(Mutex::new(
        Database::open(&moved.join("fixture.db")).unwrap(),
    ));
    f.store = ImportStore::open(moved.join("imports")).unwrap();
    assert!(f.store.recovery_issues().is_empty());
    assert!(f.store.has_pending_files());
    assert_eq!(f.store.get(&id).unwrap().items[0].state, "update_publish");
    f.apply(&id).unwrap();
    f.rollback(&id).unwrap();
    assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), before);
    assert!(f.old().join("original/Game.exe").is_file());
}

#[test]
fn incompatible_working_directory_blocks_preview_and_is_rechecked_before_moving_files() {
    let f = manual_launch_fixture(false);
    fs::remove_dir(f.source.join("manual-cwd")).unwrap();
    let blocked = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
    });
    let plan = f.store.get(&blocked).unwrap();
    assert!(plan.items[0]
        .blockers
        .iter()
        .any(|reason| reason.contains("重新配置")));
    assert!(f.apply(&blocked).is_err());
    let ready = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.working_directory = Some("override-cwd".into());
    });
    assert!(f.store.get(&ready).unwrap().items[0].blockers.is_empty());
    fs::remove_dir(f.source.join("override-cwd")).unwrap();
    assert!(f
        .apply(&ready)
        .unwrap_err()
        .to_string()
        .contains("重新配置"));
    assert!(f.old().join("original/Game.exe").is_file());
    assert!(f.source.join("replacement/Game.exe").is_file());
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().working_directory,
        "manual-cwd"
    );
}

#[test]
fn changed_exe_architecture_or_missing_loader_requires_explicit_reconfiguration() {
    let f = manual_launch_fixture(true);
    write_pe_fixture(&f.source.join("replacement/Game.exe"), 0x14c);
    let blocked = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.mtool = true;
    });
    assert!(f.store.get(&blocked).unwrap().items[0]
        .blockers
        .iter()
        .any(|reason| reason.contains("位数不同")));
    assert!(f.apply(&blocked).is_err());
    let missing = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.mtool = true;
        selection.mtool_loader = Some("loaders/missing.dll".into());
    });
    assert!(f.store.get(&missing).unwrap().items[0]
        .blockers
        .iter()
        .any(|reason| reason.contains("重新选择")));
    let escaped = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.mtool = true;
        selection.mtool_loader = Some("../outside.dll".into());
    });
    assert!(!f.store.get(&escaped).unwrap().items[0].blockers.is_empty());
    let ready = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.mtool = true;
        selection.mtool_loader = Some(String::new());
    });
    f.apply(&ready).unwrap();
    assert!(f
        .db
        .lock()
        .unwrap()
        .game(&f.id)
        .unwrap()
        .mtool_loader
        .is_none());
    f.rollback(&ready).unwrap();
    assert_eq!(
        f.db.lock()
            .unwrap()
            .game(&f.id)
            .unwrap()
            .mtool_loader
            .as_deref(),
        Some("loaders/custom.dll")
    );
}

#[test]
fn pre_fix_completed_journal_still_rolls_back_using_its_original_configuration() {
    let mut f = manual_launch_fixture(true);
    let before = f.db.lock().unwrap().version_config(&f.id).unwrap();
    let id = f.plan_with(true, true, |selection| {
        selection.executable = "replacement/Game.exe".into();
        selection.mtool = true;
        // Reproduce the configuration produced before the fix without changing launch logic.
        selection.working_directory = Some(".".into());
        selection.mtool_loader = Some(String::new());
    });
    f.apply(&id).unwrap();
    let mut record = serde_json::to_value(f.store.get(&id).unwrap()).unwrap();
    record["items"][0]["update"]
        .as_object_mut()
        .unwrap()
        .remove("inherit_launch_config");
    record["items"][0]["selection"]
        .as_object_mut()
        .unwrap()
        .remove("working_directory");
    record["items"][0]["selection"]
        .as_object_mut()
        .unwrap()
        .remove("mtool_loader");
    fs::write(
        f.store.directory.join(format!("{id}.json")),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    f.store = ImportStore::open(f.store.directory.clone()).unwrap();
    f.rollback(&id).unwrap();
    assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), before);
}

#[test]
fn preview_requires_explicit_save_review_and_does_not_touch_either_game() {
    let f = Fixture::new();
    let id = f.plan(true, false);
    assert!(!f.store.get(&id).unwrap().items[0].blockers.is_empty());
    assert!(f.apply(&id).is_err());
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");
    assert_eq!(fs::read(f.source.join("游戏.html")).unwrap(), b"new game");
    assert!(f
        .db
        .lock()
        .unwrap()
        .version_history(&f.id)
        .unwrap()
        .is_empty());
}
#[test]
fn database_commit_failure_restores_files_and_configuration_then_can_resume() {
    let f = Fixture::new();
    let aliases = f.db.lock().unwrap().game(&f.id).unwrap().aliases;
    let id = f.plan(true, true);
    f.db.lock().unwrap().fail_version_commit(true).unwrap();
    assert!(f.apply(&id).is_err());
    assert_eq!(f.db.lock().unwrap().game(&f.id).unwrap().aliases, aliases);
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().current_version,
        "v1"
    );
    assert!(f
        .db
        .lock()
        .unwrap()
        .version_history(&f.id)
        .unwrap()
        .is_empty());
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");
    assert!(stage_path(&f.store.get(&id).unwrap(), 0)
        .join("payload")
        .exists());
    f.db.lock().unwrap().fail_version_commit(false).unwrap();
    f.apply(&id).unwrap();
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().current_version,
        "v2"
    );
}
#[test]
fn confirmed_import_alias_is_atomic_idempotent_and_survives_reopen_and_rollback() {
    let mut f = Fixture::new();
    let original = f.db.lock().unwrap().game(&f.id).unwrap();
    let config = f.db.lock().unwrap().version_config(&f.id).unwrap();
    let title = "新译名 シスターズコンパス v1.2";
    let renamed = f.source.parent().unwrap().join(title);
    fs::rename(&f.source, &renamed).unwrap();
    f.source = renamed;
    let cancelled = f.plan(true, true);
    f.store.discard_preview(&cancelled).unwrap();
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().aliases,
        original.aliases
    );
    let id = f.plan(true, true);
    f.apply(&id).unwrap();
    f.apply(&id).unwrap();
    let mut game = f.db.lock().unwrap().game(&f.id).unwrap();
    assert_eq!(
        game.aliases.iter().filter(|alias| *alias == title).count(),
        1
    );
    assert!(original
        .aliases
        .iter()
        .all(|alias| game.aliases.contains(alias)));
    let reopened = Database::open(&f.store.directory.parent().unwrap().join("fixture.db")).unwrap();
    assert_eq!(reopened.game(&f.id).unwrap().aliases, game.aliases);
    let future = scanner::pending_candidate(
        &f.source
            .parent()
            .unwrap()
            .join("新译名 シスターズコンパス v1.3"),
    )
    .unwrap();
    assert!(matches(&future, std::slice::from_ref(&game))[0].auto_associate);
    f.rollback(&id).unwrap();
    game = f.db.lock().unwrap().game(&f.id).unwrap();
    assert_eq!(game.display_title, original.display_title);
    assert_eq!(f.db.lock().unwrap().version_config(&f.id).unwrap(), config);
    assert_eq!(
        game.aliases.iter().filter(|alias| *alias == title).count(),
        1
    );
}
#[test]
fn sequential_updates_can_only_be_rolled_back_in_reverse_order() {
    let f = Fixture::new();
    let first = f.plan(true, true);
    f.apply(&first).unwrap();
    fs::create_dir_all(f.source.join("www/save")).unwrap();
    fs::write(f.source.join("游戏.html"), b"third game").unwrap();
    fs::write(f.source.join("www/save/進行.sav"), b"sample").unwrap();
    let second = f.plan(true, true);
    f.apply(&second).unwrap();
    assert!(f.rollback(&first).is_err());
    f.rollback(&second).unwrap();
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"new game");
    f.rollback(&first).unwrap();
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().current_version,
        "v1"
    );
    assert!(f
        .db
        .lock()
        .unwrap()
        .version_history(&f.id)
        .unwrap()
        .iter()
        .all(|history| history.status == "rolled_back"));
}
#[test]
fn update_preserves_uuid_metadata_local_and_external_saves_and_records_history_once() {
    let f = Fixture::new();
    let before = f.db.lock().unwrap().game(&f.id).unwrap();
    let id = f.plan(true, true);
    let preview = f.store.get(&id).unwrap();
    assert!(preview.items[0].blockers.is_empty());
    assert_eq!(preview.items[0].target, before.install_path);
    assert_eq!(
        preview.items[0].update.as_ref().unwrap().required_bytes,
        2 * b"old progress".len() as u64
    );
    assert!(!stage_path(&preview, 0).exists());
    // Merely confirming a plan cannot change aliases. Remember the source only at commit.
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().aliases,
        before.aliases
    );
    assert_eq!(f.apply(&id).unwrap(), vec![f.id.clone()]);
    let after = f.db.lock().unwrap().game(&f.id).unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.display_title, before.display_title);
    assert!(before
        .aliases
        .iter()
        .all(|alias| after.aliases.contains(alias)));
    assert!(after
        .aliases
        .contains(&preview.items[0].candidate.suggested_title));
    assert_eq!(after.aliases.len(), before.aliases.len() + 1);
    assert_eq!(after.save_paths, before.save_paths);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.last_launched_at, before.last_launched_at);
    assert_eq!(after.play_status, before.play_status);
    assert_eq!(after.current_version, "v2");
    assert_eq!(
        fs::read(f.old().join("www/save/進行.sav")).unwrap(),
        b"old progress"
    );
    assert_eq!(
        fs::read(f.external.join("global.sav")).unwrap(),
        b"external progress"
    );
    let recycled = f
        .store
        .recycled_version(&preview.items[0].update.as_ref().unwrap().old_id)
        .unwrap()
        .unwrap();
    assert_eq!(fs::read(recycled.join("游戏.html")).unwrap(), b"old game");
    assert!(!stage_path(&preview, 0).exists());
    assert_eq!(preview.items[0].update.as_ref().unwrap().saves.len(), 1);
    assert!(!f.source.exists());
    f.apply(&id).unwrap();
    assert_eq!(
        f.db.lock().unwrap().version_history(&f.id).unwrap().len(),
        1
    );
    let reopened = ImportStore::open(f.store.directory.clone()).unwrap();
    assert!(
        reopened.recovery_issues().is_empty(),
        "{:?}",
        reopened
            .recovery_issues()
            .iter()
            .map(|e| &e.message)
            .collect::<Vec<_>>()
    );
    assert!(
        reopened.get(&id).unwrap().items[0]
            .update
            .as_ref()
            .unwrap()
            .rollback_available
    );
    let game = f.db.lock().unwrap().game(&f.id).unwrap();
    let settings = f.db.lock().unwrap().settings().unwrap();
    let games = f.db.lock().unwrap().games().unwrap();
    let mut deletion = crate::deletion::preview(
        &game,
        &games,
        &settings,
        f.store.directory.parent().unwrap(),
        "test",
    );
    f.store
        .include_recovery_deletion(&f.id, &mut deletion)
        .unwrap();
    assert!(deletion.blockers.is_empty());
    assert!(!deletion
        .saves
        .iter()
        .any(|save| Path::new(&save.path) == stage_path(&f.store.get(&id).unwrap(), 0)));
    assert!(!stage_path(&f.store.get(&id).unwrap(), 0).exists());
}
#[test]
fn explicit_no_migration_keeps_new_bundled_saves_and_recycles_old_saves() {
    let f = Fixture::new();
    let id = f.plan(false, true);
    f.apply(&id).unwrap();
    assert_eq!(
        fs::read(f.old().join("www/save/進行.sav")).unwrap(),
        b"bundled sample"
    );
    let plan = f.store.get(&id).unwrap();
    let recycled = f
        .store
        .recycled_version(&plan.items[0].update.as_ref().unwrap().old_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        fs::read(recycled.join("www/save/進行.sav")).unwrap(),
        b"old progress"
    );
    assert!(!stage_path(&plan, 0).exists());
    assert_eq!(
        fs::read(f.external.join("global.sav")).unwrap(),
        b"external progress"
    );
}
#[test]
fn rollback_carries_latest_progress_back_without_discarding_current_version_or_history() {
    let f = Fixture::new();
    let id = f.plan(true, true);
    f.apply(&id).unwrap();
    fs::write(f.old().join("www/save/進行.sav"), b"newest progress").unwrap();
    f.rollback(&id).unwrap();
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");
    assert_eq!(
        fs::read(f.old().join("www/save/進行.sav")).unwrap(),
        b"newest progress"
    );
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().current_version,
        "v1"
    );
    assert_eq!(
        f.db.lock().unwrap().version_history(&f.id).unwrap()[0].status,
        "rolled_back"
    );
    let plan = f.store.get(&id).unwrap();
    let recycled = f
        .store
        .recycled_version(plan.items[0].payload_id.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(fs::read(recycled.join("游戏.html")).unwrap(), b"new game");
    assert!(!stage_path(&plan, 0).exists());
    assert!(f
        .store
        .recycled_version(&plan.items[0].update.as_ref().unwrap().old_id)
        .unwrap()
        .is_none());
    assert_eq!(plan.items[0].state, "rolled_back");
    assert!(!f.store.has_pending_files());
    assert!(ImportStore::open(f.store.directory.clone())
        .unwrap()
        .recovery_issues()
        .is_empty());
}
#[test]
fn every_update_checkpoint_survives_restart_without_losing_sources_or_duplicate_history() {
    for legacy in [false, true] {
        for state in [
            "update_snapshot",
            "update_copy",
            "update_ready",
            "update_isolate",
            "update_publish",
            "update_restore",
            "update_commit",
            "update_cleanup",
            "update_recycle",
            "update_finish",
        ] {
            let mut f = Fixture::new();
            let id = f.plan(true, true);
            if legacy {
                f.use_legacy_journal(&id);
            }
            let job = f
                .tasks
                .begin("import_apply", paths::path_text(&f.root).unwrap())
                .unwrap();
            let mut plan = f.store.get(&id).unwrap();
            FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some(state.into()));
            assert!(
                f.store.update_one(&mut plan, 0, &job, &f.db).is_err(),
                "{state}"
            );
            job.finish(Err("simulated crash".into()));
            if state == "update_recycle" {
                assert!(!f.source.exists());
                fs::remove_dir(f.source.parent().unwrap()).unwrap(); // empty fixture download folder
            }
            f.store = ImportStore::open(f.store.directory.clone()).unwrap();
            assert!(f.store.recovery_issues().is_empty(), "{state}");
            f.apply(&id)
                .unwrap_or_else(|error| panic!("{state}: {error}"));
            assert_eq!(
                fs::read(f.old().join("www/save/進行.sav")).unwrap(),
                b"old progress",
                "{state}"
            );
            assert!(
                !stage_path(&f.store.get(&id).unwrap(), 0).exists(),
                "{state}"
            );
            assert_eq!(
                f.db.lock().unwrap().version_history(&f.id).unwrap().len(),
                1,
                "{state}"
            );
        }
    }
}
#[test]
fn every_rollback_checkpoint_resumes_and_preserves_latest_progress() {
    for legacy in [false, true] {
        for state in [
            "rollback_retrieve",
            "rollback_snapshot",
            "rollback_copy",
            "rollback_restore",
            "rollback_isolate",
            "rollback_publish",
            "rollback_commit",
            "rollback_cleanup",
            "rolled_back",
        ] {
            let mut f = Fixture::new();
            let id = f.plan(true, true);
            if legacy {
                f.use_legacy_journal(&id);
            }
            f.apply(&id).unwrap();
            fs::write(f.old().join("www/save/進行.sav"), b"latest progress").unwrap();
            FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some(state.into()));
            assert!(f.rollback(&id).is_err(), "{state}");
            f.store = ImportStore::open(f.store.directory.clone()).unwrap();
            assert!(f.store.recovery_issues().is_empty(), "{state}");
            f.apply(&id)
                .unwrap_or_else(|error| panic!("{state}: {error}"));
            assert_eq!(
                f.db.lock().unwrap().game(&f.id).unwrap().current_version,
                "v1",
                "{state}"
            );
            assert!(
                !stage_path(&f.store.get(&id).unwrap(), 0).exists(),
                "{state}"
            );
            assert_eq!(
                fs::read(f.old().join("www/save/進行.sav")).unwrap(),
                b"latest progress",
                "{state}"
            );
        }
    }
}
#[test]
fn qsp_update_keeps_bundled_player_and_uses_new_relative_game_file_without_execution() {
    let f = Fixture::new();
    fs::write(f.source.join("プレイヤー.exe"), b"bundled player fixture").unwrap();
    fs::write(f.source.join("彼女の冒険.qsp"), b"new qsp game").unwrap();
    fs::create_dir_all(f.source.join("standalone_content")).unwrap();
    fs::write(
        f.source.join("standalone_content/game.css"),
        b"customized assets",
    )
    .unwrap();
    let id = f.plan(true, true);
    let mut plan = f.store.get(&id).unwrap();
    plan.items[0].selection.engine = "QSP".into();
    plan.items[0].selection.executable = "プレイヤー.exe".into();
    plan.items[0].selection.external_player = Some(crate::domain::ExternalPlayer {
        player_type: "QSP".into(),
        scope: "GAME_LOCAL".into(),
        game_file: Some("彼女の冒険.qsp".into()),
    });
    f.store.save(&plan).unwrap();
    f.apply(&id).unwrap();
    let game = f.db.lock().unwrap().game(&f.id).unwrap();
    assert_eq!(game.launch_type, "EXTERNAL_PLAYER");
    assert_eq!(game.main_executable.as_deref(), Some("プレイヤー.exe"));
    assert_eq!(
        game.external_player.unwrap().game_file.as_deref(),
        Some("彼女の冒険.qsp")
    );
    assert_eq!(
        fs::read(f.old().join("standalone_content/game.css")).unwrap(),
        b"customized assets"
    );
    f.rollback(&id).unwrap();
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().launch_type,
        "DIRECT"
    );
}
#[test]
fn mtool_update_keeps_shared_settings_root_cwd_and_bundled_files() {
    let f = Fixture::new();
    let shared = f._temp.path().join("共享 MTool");
    fs::create_dir(&shared).unwrap();
    fs::write(shared.join("MTool.exe"), b"global tool fixture").unwrap();
    let mut settings = f.db.lock().unwrap().settings().unwrap();
    settings.mtool_root = paths::path_text(&dunce::canonicalize(&shared).unwrap()).unwrap();
    f.db.lock()
        .unwrap()
        .save_settings(settings.clone())
        .unwrap();
    fs::create_dir_all(f.source.join("wrapper")).unwrap();
    fs::write(f.source.join("wrapper/Game.exe"), b"game fixture").unwrap();
    fs::create_dir(f.source.join("Tool")).unwrap();
    fs::write(f.source.join("Tool/MTool.exe"), b"keep bundled file").unwrap();
    let id = f.plan(true, true);
    let mut plan = f.store.get(&id).unwrap();
    plan.items[0].selection.mtool = true;
    plan.items[0].selection.executable = "wrapper/Game.exe".into();
    f.store.save(&plan).unwrap();
    f.apply(&id).unwrap();
    let game = f.db.lock().unwrap().game(&f.id).unwrap();
    assert_eq!(game.launch_type, "MTOOL");
    assert_eq!(game.mtool_target_exe.as_deref(), Some("wrapper/Game.exe"));
    assert_eq!(game.working_directory, ".");
    assert_eq!(
        f.db.lock().unwrap().settings().unwrap().mtool_root,
        settings.mtool_root
    );
    assert_eq!(
        fs::read(f.old().join("Tool/MTool.exe")).unwrap(),
        b"keep bundled file"
    );
}
#[test]
fn source_containing_an_external_save_is_blocked_but_ordinary_payload_changes_are_accepted() {
    let f = Fixture::new();
    let mut game = f.db.lock().unwrap().game(&f.id).unwrap();
    game.save_paths = vec![paths::path_text(&f.source).unwrap()];
    let settings = f.db.lock().unwrap().settings().unwrap();
    let job = f.tasks.begin("plan", settings.game_root.clone()).unwrap();
    let selection = Selection {
        source: paths::path_text(&f.source).unwrap(),
        title: "game".into(),
        target_name: "game".into(),
        version: "v2".into(),
        engine: "HTML".into(),
        executable: "游戏.html".into(),
        working_directory: None,
        mtool_loader: None,
        external_player: None,
        mtool: false,
        existing_id: Some(f.id.clone()),
        new_override: false,
        preserve_saves: true,
        saves_confirmed: true,
    };
    assert!(f
        .store
        .prepare_update(
            &game,
            &selection,
            &settings,
            std::slice::from_ref(&game),
            &job,
            (1, &f.root.join("quarantine"))
        )
        .is_err());
    job.finish(Ok(vec![]));
    let id = f.plan(true, true);
    let job = f.tasks.begin("apply", settings.game_root.clone()).unwrap();
    let mut plan = f.store.get(&id).unwrap();
    FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some("update_ready".into()));
    assert!(f.store.update_one(&mut plan, 0, &job, &f.db).is_err());
    job.finish(Err("fixture".into()));
    fs::write(stage_path(&plan, 0).join("payload/游戏.html"), b"tampered").unwrap();
    f.apply(&id).unwrap();
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"tampered");
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().current_version,
        "v2"
    );
}
#[test]
fn withdrawing_uncommitted_update_restores_original_and_keeps_source() {
    for (state, incoming_save) in [
        ("update_publish", true),
        ("update_commit", true),
        ("update_commit", false),
    ] {
        let g = Fixture::new();
        if !incoming_save {
            fs::remove_dir_all(g.source.join("www/save")).unwrap();
        }
        let source_identity = identity(&g.source).unwrap();
        let id = g.plan(true, true);
        FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some(state.into()));
        assert!(g.apply(&id).is_err());
        let job = g
            .tasks
            .begin("import_withdraw", paths::path_text(&g.root).unwrap())
            .unwrap();
        g.store.withdraw(&id, &job, &g.db).unwrap();
        assert_eq!(identity(&g.source).unwrap(), source_identity);
        assert!(!g.store.has_pending_files());
        assert_eq!(
            fs::read(g.old().join("www/save/進行.sav")).unwrap(),
            b"old progress"
        );
        if incoming_save {
            assert_eq!(
                fs::read(g.source.join("www/save/進行.sav")).unwrap(),
                b"bundled sample"
            );
        } else {
            assert!(!g.source.join("www/save").exists());
        }
        assert!(!stage_path(&g.store.get(&id).unwrap(), 0).exists());
    }
}

#[cfg(windows)]
#[test]
fn native_cross_volume_update_copies_once_and_preserves_saves_before_source_cleanup() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture_root = project.join(".tools");
    fs::create_dir_all(&fixture_root).unwrap();
    let destination = tempfile::Builder::new()
        .prefix("update-cross-drive-")
        .tempdir_in(&fixture_root)
        .unwrap();
    let f = Fixture::with_library_root(Some(&destination.path().join("library")));
    if volume(&f.root).unwrap() == volume(&f.source).unwrap() {
        eprintln!(
            "Cross-volume fixture needs the project and temporary directory on different volumes"
        );
        return;
    }
    let id = f.plan(true, true);
    let plan = f.store.get(&id).unwrap();
    assert!(plan.items[0].cross_volume);
    assert_eq!(
        plan.items[0].update.as_ref().unwrap().required_bytes,
        plan.items[0].bytes + 2 * b"old progress".len() as u64
    );
    FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some("update_cleanup".into()));
    assert!(f.apply(&id).is_err());
    assert!(f.source.exists());
    let stage = stage_path(&plan, 0);
    assert!(!stage.join("package").exists());
    assert!(!stage.join("payload").exists());
    assert_eq!(
        fs::read(f.old().join("www/save/進行.sav")).unwrap(),
        b"old progress"
    );
    assert_eq!(
        fs::read(f.source.join("www/save/進行.sav")).unwrap(),
        b"bundled sample"
    );
    f.apply(&id).unwrap();
    assert!(!f.source.exists());
    assert!(!stage.exists());
    f.rollback(&id).unwrap();
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");

    // Cross-volume withdrawal removes copied payloads while returning neither version's saves.
    let g = Fixture::with_library_root(Some(&destination.path().join("second-library")));
    fs::write(g.old().join("www/save/進行.sav"), vec![b'S'; 2048]).unwrap();
    let id = g.plan(true, true);
    FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some("update_commit".into()));
    assert!(g.apply(&id).is_err());
    let job = g
        .tasks
        .begin("import_withdraw", paths::path_text(&g.root).unwrap())
        .unwrap();
    g.store.withdraw(&id, &job, &g.db).unwrap();
    assert_eq!(
        fs::read(g.source.join("www/save/進行.sav")).unwrap(),
        b"bundled sample"
    );
    assert_eq!(
        fs::read(g.old().join("www/save/進行.sav")).unwrap().len(),
        2048
    );
    assert!(!stage_path(&g.store.get(&id).unwrap(), 0).exists());
}
#[test]
fn changed_old_saves_block_updates_but_ordinary_game_changes_do_not() {
    for change_old in [true, false] {
        let f = Fixture::new();
        let id = f.plan(true, true);
        let changed = if change_old {
            f.old().join("www/save/進行.sav")
        } else {
            f.source.join("游戏.html")
        };
        fs::write(&changed, b"changed after preview").unwrap();
        if change_old {
            assert!(f.apply(&id).is_err());
            assert!(f.source.exists());
            assert_eq!(fs::read(changed).unwrap(), b"changed after preview");
            assert_eq!(
                f.db.lock().unwrap().game(&f.id).unwrap().current_version,
                "v1"
            );
        } else {
            fs::write(f.old().join("ordinary-mod.txt"), b"old mod").unwrap();
            f.apply(&id).unwrap();
            assert_eq!(
                fs::read(f.old().join("游戏.html")).unwrap(),
                b"changed after preview"
            );
            assert_eq!(
                f.db.lock().unwrap().game(&f.id).unwrap().current_version,
                "v2"
            );
        }
    }
}
#[test]
fn emptied_recycle_bin_disables_rollback_without_touching_current_game() {
    let f = Fixture::new();
    let id = f.plan(true, true);
    f.apply(&id).unwrap();
    let old_id = f.store.get(&id).unwrap().items[0]
        .update
        .as_ref()
        .unwrap()
        .old_id
        .clone();
    let recycled = f.store.recycled_version(&old_id).unwrap().unwrap();
    fs::remove_dir_all(recycled).unwrap(); // fixture bin only, never Windows' real bin
    assert!(
        !f.store.views()[0].items[0]
            .update
            .as_ref()
            .unwrap()
            .rollback_available
    );
    assert!(f.rollback(&id).unwrap_err().to_string().contains("回收站"));
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"new game");
    assert_eq!(
        f.db.lock().unwrap().game(&f.id).unwrap().current_version,
        "v2"
    );
    assert!(!f.store.has_pending_files());
    assert!(!stage_path(&f.store.get(&id).unwrap(), 0).exists());
}

#[cfg(windows)]
#[test]
fn locked_external_save_does_not_block_update_or_rollback_and_is_not_copied() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    let lock = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(f.external.join("global.sav"))
        .unwrap();
    let id = f.plan(true, true);
    let plan = f.store.get(&id).unwrap();
    assert_eq!(plan.items[0].update.as_ref().unwrap().saves.len(), 1);
    f.apply(&id).unwrap();
    f.rollback(&id).unwrap();
    drop(lock);
    assert_eq!(
        fs::read(f.external.join("global.sav")).unwrap(),
        b"external progress"
    );
    assert!(!stage_path(&plan, 0).exists());
}

#[test]
fn external_paths_with_missing_variables_are_ignored_without_access() {
    let f = Fixture::new();
    let mut game = f.db.lock().unwrap().game(&f.id).unwrap();
    game.save_paths = vec![
        "<GAME>/www/save".into(),
        "%BUTTER_UNSET_EXTERNAL_VARIABLE%/blocked".into(),
    ];
    let job = Job::recovery(paths::path_text(&f.root).unwrap());
    let saves = super::updater::save_snapshots(&game, &job).unwrap();
    assert_eq!(saves.len(), 1);
    assert_eq!(
        saves[0].relative.as_deref().unwrap().replace('\\', "/"),
        "www/save"
    );
}

#[test]
fn partial_temporary_cleanup_resumes_and_refuses_unexpected_files() {
    for mode in ["partial", "foreign-after", "foreign-before"] {
        let mut f = Fixture::new();
        let id = f.plan(true, true);
        FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some("update_finish".into()));
        assert!(f.apply(&id).is_err());
        let mut plan = f.store.get(&id).unwrap();
        let container = stage_path(&plan, 0);
        let job = Job::recovery(plan.root.clone());
        if mode != "foreign-before" {
            let entries = inventory(&container, &job).unwrap();
            fs::write(
                f.store.directory.join(format!("{id}-0.cleanup")),
                serde_json::to_vec(&entries).unwrap(),
            )
            .unwrap();
            plan.items[0].update.as_mut().unwrap().cleanup_ready = true;
            f.store.save(&plan).unwrap();
            fs::remove_file(container.join("saves/0/進行.sav")).unwrap();
        }
        if mode != "partial" {
            fs::write(container.join("saves/0/用户文件.txt"), b"keep").unwrap();
        }
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        if mode != "partial" {
            assert!(f.apply(&id).is_err());
            assert_eq!(
                fs::read(container.join("saves/0/用户文件.txt")).unwrap(),
                b"keep"
            );
            fs::remove_file(container.join("saves/0/用户文件.txt")).unwrap();
        }
        f.apply(&id).unwrap();
        assert!(!container.exists());
        assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"new game");
    }
}

#[test]
fn cleared_bin_during_rollback_preparation_can_be_withdrawn_without_switching() {
    let f = Fixture::new();
    let id = f.plan(true, true);
    f.apply(&id).unwrap();
    FAIL_CHECKPOINT.with(|failure| *failure.borrow_mut() = Some("rollback_retrieve".into()));
    assert!(f.rollback(&id).is_err());
    let plan = f.store.get(&id).unwrap();
    let recycled = f
        .store
        .recycled_version(&plan.items[0].update.as_ref().unwrap().old_id)
        .unwrap()
        .unwrap();
    fs::remove_dir_all(recycled).unwrap(); // fixture only
    assert!(f.apply(&id).is_err());
    let job = f.tasks.begin("import_withdraw", plan.root).unwrap();
    f.store.withdraw(&id, &job, &f.db).unwrap();
    job.finish(Ok(vec![]));
    assert!(!f.store.has_pending_files());
    assert!(!stage_path(&f.store.get(&id).unwrap(), 0).exists());
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"new game");
}

#[cfg(windows)]
#[test]
fn locked_save_and_space_exhaustion_keep_both_versions_untouched() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    let id = f.plan(true, true);
    let lock = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(f.old().join("www/save/進行.sav"))
        .unwrap();
    assert!(f.apply(&id).is_err());
    assert!(f.source.exists());
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");
    drop(lock);
    // Save-copy failure now occurs after preparation; space rejection needs a fresh pending item.
    let f = Fixture::new();
    let id = f.plan(true, true);
    let mut plan = f.store.get(&id).unwrap();
    plan.items[0].update.as_mut().unwrap().required_bytes = u64::MAX;
    f.store.save(&plan).unwrap();
    assert!(f.apply(&id).is_err());
    assert!(f.source.exists());
    assert_eq!(fs::read(f.old().join("游戏.html")).unwrap(), b"old game");
}
