//! Read-only, explicitly requested save-path discovery for an existing game.
use crate::domain::{Error, Game, Result};
use crate::{deletion, importer, paths, save_detection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub expected_install_path: String,
    pub engine: String,
    pub working_directory: String,
    pub executable: Option<String>,
}

pub fn preview(game: &Game, request: &Request) -> Result<Vec<String>> {
    if game.install_path != request.expected_install_path {
        return Err(Error::Validation(
            "游戏目录已变化，请重新打开详情后检测".into(),
        ));
    }
    let engine = request.engine.trim();
    if engine.is_empty() || engine.chars().count() > 80 || engine.chars().any(char::is_control) {
        return Err(Error::Validation("请先填写有效的游戏引擎".into()));
    }
    let root = Path::new(&game.install_path);
    deletion::plain_path(root)?;
    paths::working_directory(root, &request.working_directory)?;
    // Check the original spelling before canonicalization, including linked ancestors.
    let work_path = if request.working_directory == "." {
        root.to_path_buf()
    } else {
        root.join(paths::relative_path(&request.working_directory)?)
    };
    deletion::plain_path(&work_path)?;
    let mut identities = vec![
        (root.to_path_buf(), importer::metadata_identity(root)?),
        (work_path.clone(), importer::metadata_identity(&work_path)?),
    ];
    if let Some(executable) = request.executable.as_deref() {
        let file = root.join(paths::relative_path(executable)?);
        deletion::plain_path(&file)?;
        paths::launch_file(root, executable)?;
        identities.push((file.clone(), importer::metadata_identity(&file)?));
    }
    let candidates = save_detection::detect_for_engine(
        root,
        &request.working_directory,
        engine,
        request.executable.as_deref(),
    );
    let canonical_root = dunce::canonicalize(root)?;
    let candidates = candidates
        .into_iter()
        .filter(|value| {
            deletion::resolve_save(game, value).is_ok_and(|path| {
                (path.is_dir() || path.is_file()) && !canonical_root.starts_with(&path)
            })
        })
        .collect();
    for (path, expected) in identities {
        deletion::plain_path(&path)?;
        if importer::metadata_identity(&path)? != expected {
            return Err(Error::Validation(
                "检测期间游戏目录或启动文件已变化，请重新检测".into(),
            ));
        }
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(root: &Path) -> (Game, Request) {
        let game: Game = serde_json::from_value(serde_json::json!({
            "id": "fixture", "canonical_title": "fixture", "display_title": "fixture",
            "install_path": root.to_str().unwrap(), "working_directory": ".",
            "current_version": "1", "version_source": "manual", "main_executable": null,
            "engine": "Unknown", "launch_type": "DIRECT", "mtool_target_exe": null,
            "mtool_loader": null, "created_at": "0", "updated_at": "0",
            "last_launched_at": null, "play_status": "UNPLAYED", "aliases": [],
            "save_paths": ["<GAME>/手工配置"]
        }))
        .unwrap();
        let request = Request {
            expected_install_path: game.install_path.clone(),
            engine: game.engine.clone(),
            working_directory: ".".into(),
            executable: None,
        };
        (game, request)
    }

    #[test]
    fn explicit_discovery_preserves_manual_configuration_and_save_bytes() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("SaveData")).unwrap();
        let slot = temp.path().join("SaveData/合成存档.dat");
        fs::write(&slot, b"synthetic bytes must remain unchanged").unwrap();
        let (game, request) = fixture(temp.path());
        assert_eq!(preview(&game, &request).unwrap(), vec!["<GAME>/SaveData"]);
        assert_eq!(game.save_paths, vec!["<GAME>/手工配置"]);
        assert_eq!(
            fs::read(&slot).unwrap(),
            b"synthetic bytes must remain unchanged"
        );
        fs::remove_file(&slot).unwrap();
        fs::remove_dir(temp.path().join("SaveData")).unwrap();
        assert!(preview(&game, &request).unwrap().is_empty());
        assert!(!temp.path().join("手工配置").exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn draft_engine_launcher_and_working_directory_drive_the_existing_rules() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("Wrapper/Chosen_Data/SaveData")).unwrap();
        fs::create_dir_all(root.join("Wrapper/Other_Data/Saves")).unwrap();
        fs::write(root.join("Wrapper/Chosen.exe"), b"not executed").unwrap();
        fs::write(root.join("Wrapper/Save01.rvdata2"), b"synthetic slot").unwrap();
        fs::write(root.join("Wrapper/Actors.rvdata2"), b"synthetic resources").unwrap();
        let (game, mut request) = fixture(root);
        request.working_directory = "Wrapper".into();
        request.executable = Some("Wrapper/Chosen.exe".into());
        request.engine = "Unity".into();
        assert_eq!(
            preview(&game, &request).unwrap(),
            vec!["<GAME>/Wrapper/Chosen_Data/SaveData"]
        );
        request.engine = "RPG Maker (legacy)".into();
        assert_eq!(
            preview(&game, &request).unwrap(),
            vec!["<GAME>/Wrapper/Save01.rvdata2"]
        );
        assert_eq!(
            fs::read(root.join("Wrapper/Actors.rvdata2")).unwrap(),
            b"synthetic resources"
        );
    }

    #[test]
    fn stale_missing_and_escaping_configuration_fail_without_creating_paths() {
        let temp = tempfile::tempdir().unwrap();
        let (game, valid) = fixture(temp.path());
        assert!(preview(&game, &valid).unwrap().is_empty());
        let mut request = valid.clone();
        request.expected_install_path.push_str("-old");
        assert!(preview(&game, &request).is_err());
        for work in ["../outside", "missing", "/absolute"] {
            request = valid.clone();
            request.working_directory = work.into();
            assert!(preview(&game, &request).is_err(), "{work}");
        }
        for executable in ["../outside.exe", "missing.exe", "C:/outside.exe"] {
            request = valid.clone();
            request.executable = Some(executable.into());
            assert!(preview(&game, &request).is_err(), "{executable}");
        }
        fs::write(temp.path().join("launch.bat"), b"must not execute").unwrap();
        request = valid.clone();
        request.executable = Some("launch.bat".into());
        assert!(preview(&game, &request).is_err());
        request = valid;
        request.engine = "\n".into();
        assert!(preview(&game, &request).is_err());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        fs::remove_file(temp.path().join("launch.bat")).unwrap();
        fs::remove_dir(temp.path()).unwrap();
        request.expected_install_path = game.install_path.clone();
        request.engine = "Unknown".into();
        assert!(preview(&game, &request).is_err());
        assert!(!temp.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn linked_roots_working_directories_launchers_and_save_candidates_are_not_followed() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("game");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("Game.exe"), b"outside").unwrap();
        symlink(&outside, root.join("SaveData")).unwrap();
        symlink(outside.join("Game.exe"), root.join("Game.exe")).unwrap();
        let (game, mut request) = fixture(&root);
        assert!(preview(&game, &request).unwrap().is_empty());
        request.working_directory = "SaveData".into();
        assert!(preview(&game, &request).is_err());
        request.working_directory = ".".into();
        request.executable = Some("Game.exe".into());
        assert!(preview(&game, &request).is_err());
        symlink(&root, temp.path().join("linked-game")).unwrap();
        let (game, request) = fixture(&temp.path().join("linked-game"));
        assert!(preview(&game, &request).is_err());
        assert_eq!(fs::read(outside.join("Game.exe")).unwrap(), b"outside");
    }
}
