use super::*;
use crate::maintenance::tests::fixture_game;

fn fixture() -> (tempfile::TempDir, Game, Settings, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let root = library.join("彼女の冒険 中文");
    let data = temp.path().join("manager-data");
    fs::create_dir_all(root.join("save/slot")).unwrap();
    fs::create_dir_all(&data).unwrap();
    fs::write(root.join("save/slot/存档.dat"), "魔法少女").unwrap();
    let mut game = fixture_game(&root);
    game.save_paths = vec!["<GAME>\\save".into()];
    let settings = Settings {
        game_root: library.display().to_string(),
        ..Settings::default()
    };
    (temp, game, settings, data)
}
fn fake_recycle(trash: &Path) -> impl FnMut(&Path) -> Result<()> + '_ {
    let mut index = 0;
    move |path| {
        fs::create_dir_all(trash)?;
        fs::rename(path, trash.join(index.to_string()))?;
        index += 1;
        Ok(())
    }
}
#[cfg(windows)]
#[test]
fn offline_volume_keeps_an_absolute_protected_namespace() {
    let drives = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    assert_ne!(drives, 0);
    let Some(letter) = (b'D'..=b'Z')
        .rev()
        .find(|letter| drives & (1 << (letter - b'A')) == 0)
    else {
        return;
    };
    let missing = PathBuf::from(format!("{}:\\public-tool\\nested", char::from(letter)));
    assert_eq!(absolute_path(&missing).unwrap(), missing);
}
#[test]
fn replaced_game_save_or_parent_invalidates_deletion_before_any_recycling() {
    for scope in ["game", "save", "parent"] {
        let (temp, mut game, settings, data) = fixture();
        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("slot.dat"), b"external progress").unwrap();
        game.save_paths.push(external.display().to_string());
        let games = std::slice::from_ref(&game);
        let plan = preview(&game, games, &settings, &data, "identity-test");
        assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
        let root = Path::new(&game.install_path);
        let replaced = match scope {
            "save" => external.clone(),
            "parent" => root.parent().unwrap().to_path_buf(),
            _ => root.to_path_buf(),
        };
        let original = temp.path().join("original");
        fs::rename(&replaced, &original).unwrap();
        fs::create_dir_all(root.join("save/slot")).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(replaced.join("unrelated.txt"), b"do not recycle").unwrap();
        let current = preview(&game, games, &settings, &data, "identity-test");
        assert!(current.blockers.is_empty(), "{:?}", current.blockers);
        assert_ne!(plan, current, "{scope}");
        let report = apply_files(&plan, |_| panic!("no target may be recycled"));
        assert!(report.error.is_some(), "{scope}");
        assert!(report.recycled.is_empty());
        assert!(original.is_dir());
        assert_eq!(
            fs::read(replaced.join("unrelated.txt")).unwrap(),
            b"do not recycle"
        );
    }
}

#[test]
fn rechecks_game_identity_after_external_save_recycling_and_rejects_unbound_plans() {
    let (temp, mut game, settings, data) = fixture();
    let external = temp.path().join("external.dat");
    fs::write(&external, b"external progress").unwrap();
    game.save_paths.push(external.display().to_string());
    let plan = preview(
        &game,
        std::slice::from_ref(&game),
        &settings,
        &data,
        "race-test",
    );
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    // A serialized UI plan is not server-side deletion authority.
    let unbound: DeletePlan = serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert!(apply_files(&unbound, |_| panic!("unbound plan"))
        .error
        .is_some());
    let original = temp.path().join("original");
    let report = apply_files(&plan, |path| {
        assert_eq!(path, dunce::canonicalize(&external).unwrap());
        fs::rename(path, temp.path().join("fake-trash.dat"))?;
        fs::rename(&game.install_path, &original)?;
        fs::create_dir(&game.install_path)?;
        fs::write(Path::new(&game.install_path).join("unrelated.txt"), b"keep")?;
        Ok(())
    });
    assert!(report.error.is_some());
    assert_eq!(report.recycled.len(), 1);
    assert!(original.join("save/slot/存档.dat").is_file());
    assert!(Path::new(&game.install_path)
        .join("unrelated.txt")
        .is_file());
}
#[test]
fn deleting_one_confirmed_duplicate_preserves_the_kept_files_and_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let data = temp.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let mut db = crate::db::Database::open(&data.join("test.db")).unwrap();
    let mut ids = Vec::new();
    for name in ["同一游戏 v1.0", "同一游戏 v2.0"] {
        let root = library.join(name);
        fs::create_dir_all(root.join("save")).unwrap();
        fs::write(root.join("Game.exe"), name).unwrap();
        fs::write(root.join("save/slot.dat"), name).unwrap();
        ids.push(
            db.register(&[crate::scanner::analyze_directory(&root).unwrap()])
                .unwrap()
                .remove(0),
        );
    }
    db.set_duplicate_review(&ids[0], &ids[1], "confirmed")
        .unwrap();
    let kept = db.game(&ids[0]).unwrap();
    let deleted = db.game(&ids[1]).unwrap();
    let kept_json = serde_json::to_string(&kept).unwrap();
    let settings = Settings {
        game_root: library.display().to_string(),
        ..Settings::default()
    };
    let plan = preview(
        &deleted,
        &db.games().unwrap(),
        &settings,
        &data,
        "duplicate-test",
    );
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    let failed = apply_files(&plan, |_| Err(fail("回收失败")));
    assert!(failed.error.is_some());
    assert!(db.game(&ids[1]).is_ok());
    assert_eq!(db.duplicate_reviews().unwrap()[0].status, "confirmed");
    let report = apply_files(&plan, fake_recycle(&temp.path().join("trash")));
    assert!(report.error.is_none(), "{:?}", report.error);
    db.remove_game(&ids[1]).unwrap();
    assert_eq!(
        serde_json::to_string(&db.game(&ids[0]).unwrap()).unwrap(),
        kept_json
    );
    assert_eq!(
        fs::read_to_string(Path::new(&kept.install_path).join("Game.exe")).unwrap(),
        "同一游戏 v1.0"
    );
    assert_eq!(
        fs::read_to_string(Path::new(&kept.install_path).join("save/slot.dat")).unwrap(),
        "同一游戏 v1.0"
    );
    assert_eq!(db.games().unwrap().len(), 1);
    assert!(db.duplicate_reviews().unwrap().is_empty());
    assert!(!Path::new(&deleted.install_path).exists());
}
#[cfg(windows)]
#[test]
fn locked_save_prevents_any_recycling_and_preserves_game_directory() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_temp, game, settings, data) = fixture();
    let plan = preview(
        &game,
        std::slice::from_ref(&game),
        &settings,
        &data,
        "fixture",
    );
    let save = Path::new(&game.install_path).join("save/slot/存档.dat");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&save)
        .unwrap();
    let report = apply_files(&plan, |_| panic!("recycling must not start"));
    assert!(report.error.is_some());
    assert!(report.recycled.is_empty());
    assert!(Path::new(&game.install_path).exists());
    drop(lock);
    assert_eq!(fs::read_to_string(save).unwrap(), "魔法少女");
}
#[test]
fn recycles_internal_and_external_unicode_saves_without_preservation_or_duplicate_paths() {
    let (temp, mut game, settings, data) = fixture();
    let external = temp.path().join("outside-save.dat");
    fs::write(&external, "outside").unwrap();
    game.save_paths.extend([
        external.display().to_string(),
        "not-present".into(),
        "save/slot".into(),
    ]);
    let plan = preview(
        &game,
        std::slice::from_ref(&game),
        &settings,
        &data,
        "test-token",
    );
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert_eq!(
        plan.saves.iter().filter(|s| s.action == "recycle").count(),
        2
    );
    assert!(plan.saves.iter().any(|s| s.action == "missing"));
    let report = apply_files(&plan, fake_recycle(&temp.path().join("trash")));
    assert!(report.error.is_none(), "{:?}", report.error);
    assert!(!Path::new(&game.install_path).exists());
    assert!(!external.exists());
    assert_eq!(report.recycled.len(), 2);
    assert_eq!(
        fs::read_to_string(temp.path().join("trash/0")).unwrap(),
        "outside"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("trash/1/save/slot/存档.dat")).unwrap(),
        "魔法少女"
    );
    assert!(!data.join("preserved-saves").exists());
}
#[cfg(windows)]
#[test]
fn ordinary_game_resources_are_left_to_the_recycle_operation() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_temp, game, settings, data) = fixture();
    let resource = Path::new(&game.install_path).join("large-resource.dat");
    fs::write(&resource, "resource").unwrap();
    let plan = preview(
        &game,
        std::slice::from_ref(&game),
        &settings,
        &data,
        "fixture",
    );
    let _lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&resource)
        .unwrap();
    let mut attempted = false;
    let report = apply_files(&plan, |_| {
        attempted = true;
        Err(fail("shell sharing violation"))
    });
    assert!(
        attempted,
        "resources must not be individually opened during preflight"
    );
    assert!(report.error.unwrap().contains("shell sharing violation"));
    assert!(!report.removed);
    assert!(report.recycled.is_empty());
    assert!(Path::new(&game.install_path).exists());
}
#[test]
fn recycles_external_saves_then_game_and_stops_on_failure() {
    let (temp, mut game, settings, data) = fixture();
    let external = temp.path().join("save.dat");
    fs::write(&external, "outside").unwrap();
    game.save_paths.push(external.display().to_string());
    let plan = preview(
        &game,
        std::slice::from_ref(&game),
        &settings,
        &data,
        "test-token",
    );
    let mut count = 0;
    let trash = temp.path().join("trash");
    let mut fake = fake_recycle(&trash);
    let report = apply_files(&plan, |p| {
        count += 1;
        if count == 2 {
            Err(fail("locked game"))
        } else {
            fake(p)
        }
    });
    assert!(report.error.is_some());
    assert!(!report.removed);
    assert_eq!(report.recycled.len(), 1);
    assert!(Path::new(&game.install_path).exists());
    assert!(!external.exists());
}
#[test]
fn a_shared_external_save_blocks_all_recycling() {
    let (temp, mut game, settings, data) = fixture();
    let external = temp.path().join("shared-save.dat");
    fs::write(&external, "shared").unwrap();
    game.save_paths.push(external.display().to_string());
    let mut other = fixture_game(&temp.path().join("other-game"));
    other.id = "other".into();
    other.save_paths = vec![external.display().to_string()];
    let plan = preview(&game, &[game.clone(), other], &settings, &data, "t");
    assert!(!plan.blockers.is_empty());
    let report = apply_files(&plan, |_| panic!("blocked plan must not recycle"));
    assert!(report.error.is_some());
    assert!(Path::new(&game.install_path).exists());
    assert!(external.exists());
}
#[test]
fn rejects_roots_overlapping_games_shared_saves_and_unresolved_placeholders() {
    let (_temp, mut game, settings, data) = fixture();
    let mut other = game.clone();
    other.id = "other".into();
    assert!(
        !preview(&game, &[game.clone(), other.clone()], &settings, &data, "t")
            .blockers
            .is_empty()
    );
    other.install_path = data
        .parent()
        .unwrap()
        .join("other-game")
        .display()
        .to_string();
    other.save_paths = vec![Path::new(&game.install_path)
        .join("save")
        .display()
        .to_string()];
    assert!(
        !preview(&game, &[game.clone(), other], &settings, &data, "t")
            .blockers
            .is_empty()
    );
    game.save_paths = vec!["<UNKNOWN>/save".into()];
    assert!(
        !preview(&game, std::slice::from_ref(&game), &settings, &data, "t")
            .blockers
            .is_empty()
    );
    game.save_paths.clear();
    game.install_path = settings.game_root.clone();
    assert!(
        !preview(&game, std::slice::from_ref(&game), &settings, &data, "t")
            .blockers
            .is_empty()
    );
}
#[test]
fn explicit_delete_recycles_both_scopes_and_preview_detects_configuration_changes() {
    let (temp, mut game, settings, data) = fixture();
    let external = temp.path().join("save.dat");
    fs::write(&external, "outside").unwrap();
    game.save_paths.push(external.display().to_string());
    let plan = preview(&game, std::slice::from_ref(&game), &settings, &data, "t");
    let mut changed = game.clone();
    changed.save_paths.clear();
    assert_ne!(
        plan,
        preview(
            &changed,
            std::slice::from_ref(&changed),
            &settings,
            &data,
            "t"
        )
    );
    let report = apply_files(&plan, fake_recycle(&temp.path().join("trash")));
    assert!(report.error.is_none());
    assert_eq!(report.recycled.len(), 2);
    assert!(!data.join("preserved-saves").exists());
}
