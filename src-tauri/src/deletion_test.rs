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
