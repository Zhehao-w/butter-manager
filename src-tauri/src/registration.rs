use crate::domain::{Error, RegistrationEntry, RegistrationSelection, Result, ScanCandidate};
use crate::{paths, version};
use std::path::Path;

// Reuse the backend snapshot. Only validate the selected directory/EXE, never rescan assets.
pub fn prepare(
    root: &Path,
    mut candidate: ScanCandidate,
    selection: RegistrationSelection,
) -> Result<RegistrationEntry> {
    let path = dunce::canonicalize(&candidate.install_path)?;
    if selection.install_path != candidate.install_path || path.parent() != Some(root) {
        return Err(Error::Validation(
            "候选目录已变化或不属于当前 Game Root".into(),
        ));
    }
    if candidate.qsp.is_some() || selection.external_player.is_some() {
        let config = selection
            .external_player
            .clone()
            .or_else(|| crate::external_player::qsp_config(&candidate))
            .unwrap();
        if config.game_file.is_none() {
            return Err(Error::Validation(
                "请选择 QSP 主游戏文件；多个候选不会自动选择".into(),
            ));
        }
        crate::external_player::validate(&path, selection.executable.as_deref(), &config)?;
        if selection.version.trim().is_empty() {
            return Err(Error::Validation("版本不能为空，可使用 Unknown".into()));
        }
        candidate.engine = "QSP".into();
        let (suggested_version, suggested_source) =
            version::suggest(&candidate.suggested_title, config.game_file.as_deref());
        let version_source =
            if !selection.version_override && selection.version.trim() == suggested_version {
                suggested_source
            } else {
                "manual".into()
            };
        return Ok(RegistrationEntry {
            candidate,
            executable: selection.executable,
            external_player: Some(config),
            version: selection.version.trim().into(),
            version_source,
            working_directory: ".".into(),
        });
    }
    let suggested_exe = candidate
        .executables
        .first()
        .map(|v| v.relative_path.as_str());
    // An incomplete resource scan does not invalidate an already captured launcher.
    // Keep normal path and snapshot validation; never treat a failed/pending scan as ready.
    let usable_snapshot = candidate.status == "ready"
        || (candidate.status == "incomplete" && selection.executable.is_some());
    if !selection.exe_override
        && (!usable_snapshot || selection.executable.as_deref() != suggested_exe)
    {
        return Err(Error::Validation("请明确手工选择启动文件".into()));
    }
    if candidate.status != "ready" && selection.executable.is_none() {
        return Err(Error::Validation(
            "未完整分析的游戏请先手工选择启动文件".into(),
        ));
    }
    if selection.version.trim().is_empty() {
        return Err(Error::Validation("版本不能为空，可使用 Unknown".into()));
    }
    if let Some(exe) = &selection.executable {
        let file = paths::launch_file(&path, exe)?;
        if !selection.exe_override {
            let original = candidate
                .executables
                .iter()
                .find(|v| v.relative_path == *exe)
                .ok_or_else(|| Error::Validation("启动文件不在扫描快照中，请手工选择".into()))?;
            let metadata = std::fs::metadata(file)?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            if original.size_bytes != metadata.len() || original.modified_ms != modified {
                return Err(Error::Validation(
                    "启动文件在预览后已变化，请重新分析或重新手选".into(),
                ));
            }
        }
    }
    // A manual executable choice may belong to another engine; do not transfer its sibling's signature.
    if selection.executable.as_deref() != suggested_exe {
        candidate.engine = "Unknown".into();
    }
    let working_directory = paths::executable_directory(selection.executable.as_deref())?;
    paths::working_directory(&path, &working_directory)?;
    let (suggested_version, suggested_source) =
        version::suggest(&candidate.suggested_title, selection.executable.as_deref());
    let version_source =
        if !selection.version_override && selection.version.trim() == suggested_version {
            suggested_source
        } else {
            "manual".into()
        };
    Ok(RegistrationEntry {
        candidate,
        executable: selection.executable,
        external_player: None,
        version: selection.version.trim().into(),
        version_source,
        working_directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    #[test]
    fn incomplete_scan_registers_captured_launcher_without_bypassing_validation() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let path = root.join("游戏");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("Game.exe"), b"fixture").unwrap();
        let mut candidate = scanner::analyze_quick_controlled(&path, &|| false, &|_| {}).unwrap();
        candidate.status = "incomplete".into();
        let mut selection = RegistrationSelection {
            install_path: candidate.install_path.clone(),
            executable: Some("Game.exe".into()),
            external_player: None,
            exe_override: false,
            version: "Unknown".into(),
            version_override: false,
        };
        let entry = prepare(&root, candidate.clone(), selection.clone()).unwrap();
        let db_path = root.join("library.sqlite3");
        let mut db = crate::db::Database::open(&db_path).unwrap();
        let id = db
            .register_entries(&[entry], &|| false, &|_, _| {})
            .unwrap()
            .remove(0);
        drop(db);
        assert_eq!(
            crate::db::Database::open(&db_path)
                .unwrap()
                .game(&id)
                .unwrap()
                .main_executable
                .as_deref(),
            Some("Game.exe")
        );
        selection.executable = None;
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_err());
        selection.executable = Some("../outside.exe".into());
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_err());
        selection.executable = Some("Game.exe".into());
        for status in ["pending", "error", "skipped"] {
            let mut failed = candidate.clone();
            failed.status = status.into();
            assert!(prepare(&root, failed, selection.clone()).is_err());
        }
        std::fs::write(path.join("Game.exe"), b"changed fixture").unwrap();
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_err());
        std::fs::remove_file(path.join("Game.exe")).unwrap();
        assert!(prepare(&root, candidate, selection).is_err());
    }
    #[test]
    fn qsp_without_player_can_register_but_ambiguous_game_file_requires_selection() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let game = root.join("魔法少女");
        std::fs::create_dir(&game).unwrap();
        std::fs::write(game.join("彼女の冒険.qsp"), b"fixture").unwrap();
        let candidate = scanner::analyze_directory(&game).unwrap();
        let selection = RegistrationSelection {
            install_path: candidate.install_path.clone(),
            executable: None,
            external_player: None,
            exe_override: false,
            version: "Unknown".into(),
            version_override: false,
        };
        let entry = prepare(&root, candidate, selection.clone()).unwrap();
        let mut db = crate::db::Database::open(&root.join("test.db")).unwrap();
        let id = db
            .register_entries(&[entry], &|| false, &|_, _| {})
            .unwrap()
            .remove(0);
        let saved = db.game(&id).unwrap();
        assert_eq!(saved.launch_type, "EXTERNAL_PLAYER");
        assert_eq!(
            saved.external_player.unwrap().game_file.as_deref(),
            Some("彼女の冒険.qsp")
        );
        assert!(saved.main_executable.is_none());
        std::fs::write(game.join("mod.qsp"), b"fixture").unwrap();
        let candidate = scanner::analyze_directory(&game).unwrap();
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_err());
        let mut choice = selection;
        choice.external_player = Some(crate::domain::ExternalPlayer {
            player_type: "QSP".into(),
            scope: "GAME_LOCAL".into(),
            game_file: Some("mod.qsp".into()),
        });
        assert!(prepare(&root, candidate.clone(), choice.clone()).is_ok());
        choice.external_player.as_mut().unwrap().game_file = Some("../escape.qsp".into());
        assert!(prepare(&root, candidate, choice).is_err());
    }
    #[test]
    fn manual_document_registration_does_not_require_an_exe_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let game = root.join("HTML game");
        std::fs::create_dir_all(game.join("包装")).unwrap();
        std::fs::write(game.join("包装/游戏.html"), b"fixture").unwrap();
        let candidate = scanner::analyze_directory(&game).unwrap();
        assert!(candidate.executables.is_empty());
        let mut selection = RegistrationSelection {
            install_path: candidate.install_path.clone(),
            executable: Some("包装/游戏.html".into()),
            external_player: None,
            exe_override: false,
            version: "Unknown".into(),
            version_override: false,
        };
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_err());
        selection.exe_override = true;
        let prepared = prepare(&root, candidate, selection).unwrap();
        assert_eq!(prepared.working_directory, "包装");
        assert_eq!(prepared.candidate.engine, "Unknown");
        let mut db = crate::db::Database::open(&root.join("test.db")).unwrap();
        let id = db
            .register_entries(&[prepared], &|| false, &|_, _| {})
            .unwrap()
            .remove(0);
        assert_eq!(
            db.game(&id).unwrap().main_executable.as_deref(),
            Some("包装/游戏.html")
        );
    }
    #[test]
    fn stale_exe_requires_explicit_override_and_never_allows_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let path = root.join("游戏");
        std::fs::create_dir_all(path.join("包装")).unwrap();
        std::fs::write(path.join("包装/main-v1.2.3.exe"), b"fixture").unwrap();
        let candidate = scanner::analyze_directory(&path).unwrap();
        let mut selection = RegistrationSelection {
            install_path: candidate.install_path.clone(),
            executable: Some(candidate.executables[0].relative_path.clone()),
            external_player: None,
            exe_override: false,
            version: "v1.2.3".into(),
            version_override: false,
        };
        let prepared = prepare(&root, candidate.clone(), selection.clone()).unwrap();
        assert_eq!(prepared.working_directory, "包装");
        assert_eq!(prepared.version_source, "file_name");
        selection.version_override = true;
        assert_eq!(
            prepare(&root, candidate.clone(), selection.clone())
                .unwrap()
                .version_source,
            "manual"
        );
        std::fs::write(path.join("包装/main-v1.2.3.exe"), b"changed fixture").unwrap();
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_err());
        selection.exe_override = true;
        assert!(prepare(&root, candidate.clone(), selection.clone()).is_ok());
        selection.executable = Some("../outside.exe".into());
        assert!(prepare(&root, candidate, selection).is_err());
    }
}
