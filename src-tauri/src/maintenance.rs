use crate::domain::{Error, Game, LibraryPathCheck, RelocateGame, Result, Settings, ToolCheck};
use crate::paths::{contained_file, executable_directory, launch_file, path_text};
use std::path::Path;

pub fn check_game(game: &Game) -> LibraryPathCheck {
    let root = Path::new(&game.install_path);
    if game.launch_type == "EXTERNAL_PLAYER" && root.is_dir() {
        let (state, message) = match crate::external_player::command(game) {
            Ok(_) => ("available", "本地播放器和 QSP 游戏文件可访问".into()),
            Err(Error::Validation(message)) => ("unconfigured", message),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                ("missing_launch", "QSP 播放器或游戏文件不存在".into())
            }
            Err(error) => ("unreadable", error.to_string()),
        };
        return LibraryPathCheck {
            id: game.id.clone(),
            install_path: game.install_path.clone(),
            state: state.into(),
            message,
        };
    }
    let (state, message) = match std::fs::metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ("missing_directory", "游戏目录不存在".into())
        }
        Err(error) => ("unreadable", format!("无法检查游戏目录：{error}")),
        Ok(metadata) if !metadata.is_dir() => ("missing_directory", "原路径已不是游戏目录".into()),
        Ok(_) => match if game.launch_type == "MTOOL" {
            game.mtool_target_exe
                .as_deref()
                .or(game.main_executable.as_deref())
        } else {
            game.main_executable.as_deref()
        } {
            None => ("unconfigured", "尚未配置启动文件".into()),
            Some(file) => match if game.launch_type == "MTOOL" {
                contained_file(root, file, "exe")
            } else {
                launch_file(root, file)
            } {
                Ok(_) => ("available", "目录和启动文件可访问".into()),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    ("missing_launch", "启动文件不存在".into())
                }
                Err(error) => ("unreadable", format!("无法检查启动文件：{error}")),
            },
        },
    };
    LibraryPathCheck {
        id: game.id.clone(),
        install_path: game.install_path.clone(),
        state: state.into(),
        message,
    }
}

pub fn relocation_plan(game: &Game, path: &Path) -> Result<RelocateGame> {
    let root = dunce::canonicalize(path)?;
    if !root.is_dir() {
        return Err(Error::Validation("请选择已有游戏目录".into()));
    }
    let main = game
        .main_executable
        .clone()
        .filter(|file| launch_file(&root, file).is_ok());
    let target = game
        .mtool_target_exe
        .clone()
        .filter(|file| contained_file(&root, file, "exe").is_ok());
    let working = if crate::paths::working_directory(&root, &game.working_directory).is_ok() {
        game.working_directory.clone()
    } else {
        executable_directory(main.as_deref())?
    };
    Ok(RelocateGame {
        id: game.id.clone(),
        expected_install_path: game.install_path.clone(),
        install_path: path_text(&root)?,
        main_executable: main,
        working_directory: working,
        launch_type: game.launch_type.clone(),
        external_player: game.external_player.clone().map(|mut config| {
            config.game_file = config
                .game_file
                .filter(|file| contained_file(&root, file, "qsp").is_ok());
            config
        }),
        mtool_target_exe: target,
    })
}

/// Only rebase absolute paths beneath the old game root. Tokens and external saves stay intact.
pub fn rebase_save(path: &str, old: &Path, new: &Path) -> String {
    let path_ref = Path::new(path);
    if !path_ref.is_absolute()
        || path_ref
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return path.into();
    }
    let old_components: Vec<_> = old.components().collect();
    let components: Vec<_> = path_ref.components().collect();
    if components.len() < old_components.len() {
        return path.into();
    }
    let matches = components.iter().zip(&old_components).all(|(a, b)| {
        if cfg!(windows) {
            a.as_os_str().to_string_lossy().to_lowercase()
                == b.as_os_str().to_string_lossy().to_lowercase()
        } else {
            a == b
        }
    });
    if !matches {
        return path.into();
    }
    let suffix: std::path::PathBuf = components
        .iter()
        .skip(old_components.len())
        .map(|c| c.as_os_str())
        .collect();
    new.join(suffix).display().to_string()
}

pub fn check_mtool(settings: &Settings) -> Vec<ToolCheck> {
    [
        ("MTool 主程序", "MTool.exe", "exe"),
        ("游戏注入器", settings.mtool_injector.as_str(), "exe"),
        ("游戏运行程序", settings.mtool_runtime.as_str(), "exe"),
        ("32 位 Loader", "loaders/mzHook32.dll", "dll"),
        ("64 位 Loader", "loaders/mzHook.dll", "dll"),
    ]
    .into_iter()
    .map(|(label, relative, extension)| {
        let checked = if settings.mtool_root.trim().is_empty() {
            Err(Error::Validation("请先设置共享 MTool 目录".into()))
        } else {
            contained_file(Path::new(&settings.mtool_root), relative, extension)
        };
        ToolCheck {
            label: label.into(),
            path: Path::new(&settings.mtool_root)
                .join(relative)
                .display()
                .to_string(),
            available: checked.is_ok(),
            message: checked
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "文件可访问".into()),
        }
    })
    .collect()
}

pub fn launch_mtool(settings: &Settings) -> Result<()> {
    standalone_mtool_command(settings)?.spawn()?;
    Ok(())
}

fn standalone_mtool_command(settings: &Settings) -> Result<std::process::Command> {
    if settings.mtool_root.trim().is_empty() {
        return Err(Error::Validation("请先设置共享 MTool 目录".into()));
    }
    let file = contained_file(Path::new(&settings.mtool_root), "MTool.exe", "exe")?;
    let root = file
        .parent()
        .ok_or_else(|| Error::Validation("MTool 目录无效".into()))?;
    let mut command = std::process::Command::new(&file);
    command.current_dir(root);
    Ok(command)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn fixture_game(root: &Path) -> Game {
        Game {
            id: "fixture".into(),
            canonical_title: "游戏".into(),
            display_title: "游戏".into(),
            install_path: root.display().to_string(),
            working_directory: ".".into(),
            current_version: "Final".into(),
            version_source: "manual".into(),
            main_executable: Some("game.html".into()),
            engine: "HTML".into(),
            launch_type: "DIRECT".into(),
            external_player: None,
            mtool_target_exe: None,
            mtool_loader: None,
            created_at: "unchanged".into(),
            updated_at: "unchanged".into(),
            last_launched_at: None,
            play_status: crate::domain::PlayStatus::Unplayed,
            aliases: vec![],
            save_paths: vec![],
        }
    }

    #[test]
    fn path_checks_distinguish_missing_directory_launch_and_unconfigured_without_changing_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("游戏");
        let mut game = fixture_game(&root);
        assert_eq!(check_game(&game).state, "missing_directory");
        std::fs::create_dir(&root).unwrap();
        assert_eq!(check_game(&game).state, "missing_launch");
        std::fs::write(root.join("game.html"), b"unchanged").unwrap();
        assert_eq!(check_game(&game).state, "available");
        game.main_executable = None;
        assert_eq!(check_game(&game).state, "unconfigured");
        game.main_executable = Some("../outside.exe".into());
        assert_eq!(check_game(&game).state, "unreadable");
        game.launch_type = "MTOOL".into();
        game.main_executable = None;
        assert_eq!(check_game(&game).state, "unconfigured");
        game.mtool_target_exe = Some("target.exe".into());
        std::fs::write(root.join("target.exe"), b"fixture-not-executed").unwrap();
        assert_eq!(check_game(&game).state, "available");
        assert_eq!(std::fs::read(root.join("game.html")).unwrap(), b"unchanged");
    }

    #[test]
    fn relocation_preview_drops_missing_files_and_falls_back_from_old_working_directory() {
        let temp = tempfile::tempdir().unwrap();
        let mut game = fixture_game(&temp.path().join("old"));
        game.working_directory = "old-wrapper".into();
        game.main_executable = Some("wrapper/game.html".into());
        game.mtool_target_exe = Some("old.exe".into());
        let root = temp.path().join("new");
        std::fs::create_dir_all(root.join("wrapper")).unwrap();
        std::fs::write(root.join("wrapper/game.html"), b"fixture").unwrap();
        let plan = relocation_plan(&game, &root).unwrap();
        assert_eq!(plan.main_executable, game.main_executable);
        assert_eq!(plan.working_directory, "wrapper");
        assert!(plan.mtool_target_exe.is_none());
        assert_eq!(plan.expected_install_path, game.install_path);
        game.main_executable = Some("missing.exe".into());
        let plan = relocation_plan(&game, &root).unwrap();
        assert!(plan.main_executable.is_none());
        assert_eq!(plan.working_directory, ".");
    }

    #[test]
    fn save_rebase_respects_components_external_paths_tokens_and_parent_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        let sibling = temp.path().join("old-other/save").display().to_string();
        for untouched in ["<GAME>/save", "%APPDATA%/save", "relative/save", &sibling] {
            assert_eq!(rebase_save(untouched, &old, &new), untouched);
        }
        let escaped = old.join("../external/save").display().to_string();
        assert_eq!(rebase_save(&escaped, &old, &new), escaped);
        assert_eq!(
            rebase_save(&old.join("save/a.dat").display().to_string(), &old, &new),
            new.join("save").join("a.dat").display().to_string()
        );
    }

    #[test]
    fn standalone_tool_uses_only_root_mtool_exe_and_no_game_arguments_without_spawning() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        std::fs::write(root.join("MTool.exe"), b"fixture-not-executed").unwrap();
        let settings = Settings {
            mtool_root: root.display().to_string(),
            mtool_injector: "missing.exe".into(),
            mtool_runtime: "MTool.exe".into(),
            ..Settings::default()
        };
        let checks = check_mtool(&settings);
        assert!(checks[0].available);
        assert!(!checks[1].available);
        assert!(checks[2].available);
        let command = standalone_mtool_command(&settings).unwrap();
        assert_eq!(command.get_program(), root.join("MTool.exe").as_os_str());
        assert_eq!(command.get_current_dir(), Some(root.as_path()));
        assert_eq!(command.get_args().count(), 0);
        let escaping = Settings {
            mtool_injector: "../outside.exe".into(),
            ..settings
        };
        assert!(!check_mtool(&escaping)[1].available);
        assert!(standalone_mtool_command(&Settings::default()).is_err());
    }
}
