use crate::domain::{Error, Game, LaunchConfiguration, MToolLaunchPreview, Result, Settings};
use crate::paths::contained_file;
use std::path::Path;
use std::process::Command;

/// A one-time launch keeps the library identity and directory; it never saves the draft.
pub fn with_configuration(mut game: Game, config: LaunchConfiguration) -> Game {
    game.main_executable = config.main_executable;
    game.working_directory = config.working_directory;
    game.launch_type = config.launch_type;
    game.external_player = config.external_player;
    game.mtool_target_exe = config.mtool_target_exe;
    game.mtool_loader = config.mtool_loader;
    game
}

pub fn direct_file(game: &Game) -> Result<std::path::PathBuf> {
    let executable = game
        .main_executable
        .as_deref()
        .ok_or_else(|| Error::Validation("请先配置启动文件".into()))?;
    crate::paths::launch_file(Path::new(&game.install_path), executable)
}

pub fn launch(game: &Game, settings: &Settings) -> Result<()> {
    launch_observed(game, settings, None)
}
fn launch_observed(
    game: &Game,
    settings: &Settings,
    running: Option<&crate::runtime::RunningGames>,
) -> Result<()> {
    if !cfg!(windows) {
        return Err(Error::Validation("游戏启动仅支持 Windows".into()));
    }
    let working =
        crate::paths::working_directory(Path::new(&game.install_path), &game.working_directory)?;
    match game.launch_type.as_str() {
        "DIRECT" => {
            let file = direct_file(game)?;
            if file
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
            {
                let child = Command::new(file).current_dir(&working).spawn()?;
                if let Some(running) = running {
                    running.observe(&game.id, crate::runtime::ProcessLease::from_child(child)?)?;
                }
            } else {
                if file
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("qsp"))
                {
                    return Err(Error::Validation(
                        "QSP 游戏需要配置本地播放器，请在详情中选择 QSP / 外部播放器".into(),
                    ));
                }
                if let Some(process) = crate::association::open_file_tracked(&file, &working)? {
                    if let Some(running) = running {
                        running.observe(&game.id, process)?;
                    }
                }
            }
            Ok(())
        }
        "EXTERNAL_PLAYER" => {
            let child = crate::external_player::command(game)?.spawn()?;
            if let Some(running) = running {
                running.observe(&game.id, crate::runtime::ProcessLease::from_child(child)?)?;
            }
            Ok(())
        }
        "MTOOL" => {
            let (mut injector, mut runtime) = mtool_commands(game, settings)?;
            // Pass each argument separately; never execute or interpolate an incoming BAT.
            injector.status().and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(format!(
                        "injector 退出状态：{status}"
                    )))
                }
            })?;
            runtime.spawn().map_err(|error| {
                Error::Validation(format!(
                    "injector 已启动，但 MTool runtime 启动失败：{error}"
                ))
            })?;
            Ok(())
        }
        _ => Err(Error::Validation(
            "CUSTOM_BAT 暂不支持；下载 BAT 不会被执行".into(),
        )),
    }
}

fn mtool_commands(game: &Game, settings: &Settings) -> Result<(Command, Command)> {
    let (injector, target, loader, runtime) = mtool_files(game, settings)?;
    let working =
        crate::paths::working_directory(Path::new(&game.install_path), &game.working_directory)?;
    let mut inject_command = Command::new(injector);
    inject_command.arg(target).arg(loader).current_dir(&working);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        inject_command.creation_flags(0x0800_0000);
    }
    let mut runtime_command = Command::new(runtime);
    runtime_command
        .arg(Path::new(&settings.mtool_root))
        .current_dir(&working);
    Ok((inject_command, runtime_command))
}

/// Launch first, then persist the successful request without holding a database lock during launch.
pub fn launch_tracked<T>(
    game: &Game,
    settings: &Settings,
    record: impl FnOnce() -> Result<T>,
) -> Result<T> {
    launch(game, settings)?;
    record().map_err(|e| Error::Validation(format!("启动请求已发送，但运行记录保存失败：{e}")))
}
pub fn launch_with_runtime<T>(
    game: &Game,
    settings: &Settings,
    running: &crate::runtime::RunningGames,
    record: impl FnOnce() -> Result<T>,
) -> Result<T> {
    launch_observed(game, settings, Some(running))?;
    record().map_err(|e| Error::Validation(format!("启动请求已发送，但运行记录保存失败：{e}")))
}

fn mtool_files(
    game: &Game,
    settings: &Settings,
) -> Result<(
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
)> {
    if settings.mtool_root.is_empty() {
        return Err(Error::Validation("请先设置共享 MTool Root".into()));
    }
    let root = Path::new(&settings.mtool_root);
    let target = game
        .mtool_target_exe
        .as_deref()
        .or(game.main_executable.as_deref())
        .ok_or_else(|| Error::Validation("未配置 MTool target".into()))?;
    let target_file = contained_file(Path::new(&game.install_path), target, "exe")?;
    let loader = resolve_loader(&target_file, game.mtool_loader.as_deref())?;
    Ok((
        contained_file(root, &settings.mtool_injector, "exe")?,
        target_file,
        contained_file(root, &loader, "dll")?,
        contained_file(root, &settings.mtool_runtime, "exe")?,
    ))
}

fn resolve_loader(target: &Path, configured: Option<&str>) -> Result<String> {
    if let Some(loader) = configured {
        return Ok(loader.into());
    }
    match crate::scanner::architecture(target).as_str() {
        "x86" => Ok("loaders/mzHook32.dll".into()),
        "x64" => Ok("loaders/mzHook.dll".into()),
        _ => Err(Error::Validation(
            "无法自动判断游戏位数，请在详情中选择 32 位或 64 位 loader".into(),
        )),
    }
}

pub fn preview_mtool(game: &Game, settings: &Settings) -> Result<MToolLaunchPreview> {
    let target = game
        .mtool_target_exe
        .as_deref()
        .or(game.main_executable.as_deref())
        .ok_or_else(|| Error::Validation("请选择游戏启动 EXE".into()))?;
    let file = contained_file(Path::new(&game.install_path), target, "exe")?;
    let loader = resolve_loader(&file, game.mtool_loader.as_deref())?;
    let work =
        crate::paths::working_directory(Path::new(&game.install_path), &game.working_directory)?;
    mtool_files(game, settings)?;
    Ok(MToolLaunchPreview {
        shared_root: settings.mtool_root.clone(),
        target_exe: target.into(),
        architecture: crate::scanner::architecture(&file),
        loader,
        runtime: settings.mtool_runtime.clone(),
        working_directory: work.display().to_string(),
    })
}

pub fn debug_bat(game: &Game, settings: &Settings) -> Result<String> {
    let (injector, target, loader, runtime) = mtool_files(game, settings)?;
    fn quoted(path: &Path) -> Result<String> {
        let value = path
            .to_str()
            .ok_or_else(|| Error::Validation("路径编码不支持".into()))?
            .strip_prefix("\\\\?\\")
            .unwrap_or_else(|| path.to_str().unwrap());
        if value.chars().any(|c| {
            matches!(
                c,
                '"' | '%' | '!' | '^' | '&' | '|' | '<' | '>' | '\r' | '\n'
            )
        }) {
            return Err(Error::Validation(
                "路径含 BAT 特殊字符；请使用直接进程启动".into(),
            ));
        }
        Ok(format!("\"{value}\""))
    }
    Ok(format!(
        "@chcp 65001\r\n@cd /d {}\r\n{} {} {}\r\nstart \"\" {} {}\r\n",
        quoted(&crate::paths::working_directory(
            Path::new(&game.install_path),
            &game.working_directory
        )?)?,
        quoted(&injector)?,
        quoted(&target)?,
        quoted(&loader)?,
        quoted(&runtime)?,
        quoted(Path::new(&settings.mtool_root))?
    ))
}

pub fn open_folder(game: &Game) -> Result<()> {
    if !cfg!(windows) {
        return Err(Error::Validation("打开文件夹仅支持 Windows".into()));
    }
    let root = dunce::canonicalize(&game.install_path)?;
    if !root.is_dir() {
        return Err(Error::Validation("游戏目录不存在".into()));
    }
    Command::new("explorer.exe").arg(root).spawn()?;
    Ok(())
}

/// A configured save may be a directory or an individual save file.
/// Files are browsed through their containing directory, never executed.
pub fn save_folder(game: &Game, configured: &str) -> Result<std::path::PathBuf> {
    let path = crate::deletion::resolve_save(game, configured)?;
    if path.is_dir() {
        return Ok(path);
    }
    if path.is_file() {
        if let Some(parent) = path.parent() {
            return Ok(parent.to_owned());
        }
    }
    Err(Error::Validation(format!(
        "存档位置不存在：{}",
        path.display()
    )))
}

pub fn open_save_folder(game: &Game, configured: &str) -> Result<()> {
    let folder = save_folder(game, configured)?;
    if !cfg!(windows) {
        return Err(Error::Validation("打开文件夹仅支持 Windows".into()));
    }
    Command::new("explorer.exe").arg(folder).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Database, scanner::analyze_directory};
    #[test]
    fn one_time_configuration_uses_draft_paths_but_records_only_the_saved_game() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let game_root = root.join("game");
        let tools = root.join("shared");
        std::fs::create_dir_all(game_root.join("新版")).unwrap();
        std::fs::create_dir_all(tools.join("loaders")).unwrap();
        for file in [
            game_root.join("old.exe"),
            game_root.join("新版/new.exe"),
            game_root.join("新版/player.exe"),
            game_root.join("新版/story.qsp"),
            tools.join("loaders/inject.exe"),
            tools.join("loaders/custom.dll"),
            tools.join("MTool.exe"),
        ] {
            std::fs::write(file, b"fixture-not-executed").unwrap();
        }
        let mut db = Database::open(&root.join("test.db")).unwrap();
        let id = db
            .register(&[analyze_directory(&game_root).unwrap()])
            .unwrap()
            .remove(0);
        let saved = db.game(&id).unwrap();
        let settings = Settings {
            mtool_root: tools.display().to_string(),
            mtool_injector: "loaders/inject.exe".into(),
            mtool_runtime: "MTool.exe".into(),
            ..Settings::default()
        };
        for mode in ["DIRECT", "MTOOL", "EXTERNAL_PLAYER"] {
            let draft = with_configuration(
                saved.clone(),
                LaunchConfiguration {
                    launch_type: mode.into(),
                    main_executable: Some(
                        if mode == "EXTERNAL_PLAYER" {
                            "新版/player.exe"
                        } else {
                            "新版/new.exe"
                        }
                        .into(),
                    ),
                    working_directory: "新版".into(),
                    mtool_target_exe: Some("新版/new.exe".into()),
                    mtool_loader: Some("loaders/custom.dll".into()),
                    external_player: Some(crate::domain::ExternalPlayer {
                        player_type: "QSP".into(),
                        scope: "GAME_LOCAL".into(),
                        game_file: Some("新版/story.qsp".into()),
                    }),
                },
            );
            assert_eq!(draft.id, saved.id);
            assert_eq!(draft.install_path, saved.install_path);
            assert_eq!(draft.display_title, saved.display_title);
            assert_eq!(draft.aliases, saved.aliases);
            assert_eq!(draft.save_paths, saved.save_paths);
            match mode {
                "DIRECT" => {
                    assert_eq!(direct_file(&draft).unwrap(), game_root.join("新版/new.exe"));
                    assert_eq!(
                        crate::paths::working_directory(&game_root, &draft.working_directory)
                            .unwrap(),
                        game_root.join("新版")
                    );
                }
                "MTOOL" => {
                    let (injector, _) = mtool_commands(&draft, &settings).unwrap();
                    assert_eq!(
                        injector.get_args().collect::<Vec<_>>(),
                        vec![
                            game_root.join("新版").join("new.exe").as_os_str(),
                            tools.join("loaders").join("custom.dll").as_os_str()
                        ]
                    );
                    assert_eq!(
                        injector.get_current_dir(),
                        Some(game_root.join("新版").as_path())
                    );
                    let mut invalid = draft.clone();
                    invalid.mtool_loader = Some("../escape.dll".into());
                    assert!(mtool_commands(&invalid, &settings).is_err());
                }
                _ => {
                    let command = crate::external_player::command(&draft).unwrap();
                    assert_eq!(
                        command.get_program(),
                        game_root.join("新版").join("player.exe").as_os_str()
                    );
                    assert_eq!(
                        command.get_args().collect::<Vec<_>>(),
                        vec![game_root.join("新版").join("story.qsp").as_os_str()]
                    );
                    assert_eq!(
                        command.get_current_dir(),
                        Some(game_root.join("新版").as_path())
                    );
                }
            }
            let mut invalid = draft.clone();
            invalid.main_executable = Some("../escape.exe".into());
            assert!(direct_file(&invalid).is_err());
            assert!(crate::paths::working_directory(&game_root, "../shared").is_err());
            // The same history callback used after a successful launch returns persisted config.
            let recorded = db.record_launch(&draft.id).unwrap();
            assert_eq!(recorded.main_executable, saved.main_executable);
            assert_eq!(recorded.working_directory, saved.working_directory);
            assert_eq!(recorded.launch_type, saved.launch_type);
            assert_eq!(recorded.external_player, saved.external_player);
            assert_eq!(recorded.mtool_target_exe, saved.mtool_target_exe);
            assert_eq!(recorded.mtool_loader, saved.mtool_loader);
            assert!(recorded.last_launched_at.is_some());
        }
        assert_eq!(db.launch_history(&id).unwrap().len(), 3);
    }
    #[test]
    fn save_browsing_resolves_game_relative_unicode_directories_and_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ゲーム 中文 with spaces");
        let save = root.join("www/save");
        std::fs::create_dir_all(&save).unwrap();
        std::fs::write(save.join("存档.rpgsave"), b"fixture").unwrap();
        let game = crate::maintenance::tests::fixture_game(&root);
        let expected = dunce::canonicalize(&save).unwrap();
        for path in [
            "<GAME>/www/save",
            "www\\save",
            "<GAME>/www/save/存档.rpgsave",
        ] {
            assert_eq!(save_folder(&game, path).unwrap(), expected);
        }
        assert_eq!(
            save_folder(&game, save.to_str().unwrap()).unwrap(),
            expected
        );
        assert!(save_folder(&game, "<GAME>/missing").is_err());
        assert!(save_folder(&game, "<GAME>/../escape").is_err());
        assert!(save_folder(&game, "").is_err());
    }
    fn write_pe(path: &Path, machine: u16) {
        let mut bytes = vec![0u8; 134];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&machine.to_le_bytes());
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn automatic_loader_and_commands_follow_game_architecture_and_only_use_shared_tools() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let tool = root.join("shared tools");
        let game_dir = root.join("游戏 & spaces");
        std::fs::create_dir_all(tool.join("loaders")).unwrap();
        std::fs::create_dir_all(game_dir.join("Tool/loaders")).unwrap();
        for path in [
            tool.join("loaders/inject.exe"),
            tool.join("loaders/mzHook32.dll"),
            tool.join("loaders/mzHook.dll"),
            tool.join("MTool.exe"),
            game_dir.join("Tool/loaders/mzHook32.dll"),
        ] {
            std::fs::write(path, b"fixture-not-executed").unwrap();
        }
        let settings = Settings {
            mtool_root: tool.display().to_string(),
            mtool_injector: "loaders/inject.exe".into(),
            mtool_runtime: "MTool.exe".into(),
            ..Settings::default()
        };
        let mut game = crate::maintenance::tests::fixture_game(&game_dir);
        game.launch_type = "MTOOL".into();
        game.main_executable = Some("任意游戏.exe".into());
        for (machine, expected, arch) in [
            (0x14c, "loaders/mzHook32.dll", "x86"),
            (0x8664, "loaders/mzHook.dll", "x64"),
        ] {
            write_pe(&game_dir.join("任意游戏.exe"), machine);
            let preview = preview_mtool(&game, &settings).unwrap();
            assert_eq!(preview.loader, expected);
            assert_eq!(preview.architecture, arch);
            let (inject, runtime) = mtool_commands(&game, &settings).unwrap();
            assert_eq!(
                inject.get_program(),
                tool.join("loaders").join("inject.exe").as_os_str()
            );
            assert_eq!(
                inject.get_args().collect::<Vec<_>>(),
                vec![
                    game_dir.join("任意游戏.exe").as_os_str(),
                    expected
                        .split('/')
                        .fold(tool.clone(), |path, part| path.join(part))
                        .as_os_str()
                ]
            );
            assert_eq!(inject.get_current_dir(), Some(game_dir.as_path()));
            assert_eq!(runtime.get_program(), tool.join("MTool.exe").as_os_str());
            assert_eq!(
                runtime.get_args().collect::<Vec<_>>(),
                vec![tool.as_os_str()]
            );
            assert_eq!(runtime.get_current_dir(), inject.get_current_dir());
        }
        write_pe(&game_dir.join("任意游戏.exe"), 0xaa64);
        assert!(preview_mtool(&game, &settings).is_err());
        game.mtool_loader = Some("loaders/mzHook32.dll".into());
        assert!(preview_mtool(&game, &settings).is_ok());
        std::fs::remove_file(tool.join("loaders/mzHook32.dll")).unwrap();
        assert!(preview_mtool(&game, &settings).is_err());
        // A bundled loader cannot substitute for a missing shared loader.
        assert!(game_dir.join("Tool/loaders/mzHook32.dll").exists());
    }
    #[test]
    fn failed_launch_does_not_record_a_run() {
        let root = tempfile::tempdir().unwrap();
        let mut db = crate::db::Database::open(&root.path().join("library.db")).unwrap();
        let id = db
            .register(&[crate::scanner::analyze_directory(root.path()).unwrap()])
            .unwrap()
            .remove(0);
        let game = with_configuration(
            db.game(&id).unwrap(),
            LaunchConfiguration {
                launch_type: "DIRECT".into(),
                main_executable: Some("missing.exe".into()),
                working_directory: ".".into(),
                external_player: None,
                mtool_target_exe: None,
                mtool_loader: None,
            },
        );
        assert!(launch_tracked(&game, &db.settings().unwrap(), || db.record_launch(&id)).is_err());
        assert!(db.game(&id).unwrap().last_launched_at.is_none());
        assert!(db.launch_history(&id).unwrap().is_empty());
    }
    #[test]
    fn validates_every_mtool_file_before_launch_and_produces_debug_text() {
        let root = tempfile::tempdir().unwrap();
        let tool = root.path().join("shared tool");
        let game_dir = root.path().join("游戏");
        std::fs::create_dir_all(tool.join("loaders")).unwrap();
        std::fs::create_dir(&game_dir).unwrap();
        for path in [
            tool.join("loaders/inject.exe"),
            tool.join("loaders/mzHook32.dll"),
            tool.join("nw.exe"),
            game_dir.join("任意名.exe"),
        ] {
            std::fs::write(path, b"fixture").unwrap();
        }
        let mut db = Database::open(&root.path().join("test.db")).unwrap();
        let id = db
            .register(&[analyze_directory(&game_dir).unwrap()])
            .unwrap()
            .remove(0);
        let mut game = db.game(&id).unwrap();
        let settings = Settings {
            game_root: String::new(),
            mtool_root: tool.to_str().unwrap().into(),
            mtool_injector: "loaders/inject.exe".into(),
            mtool_runtime: "nw.exe".into(),
            scan_workers: 2,
        };
        assert!(debug_bat(&game, &settings).is_err());
        game.mtool_target_exe = Some("任意名.exe".into());
        game.mtool_loader = Some("loaders/mzHook32.dll".into());
        let text = debug_bat(&game, &settings).unwrap();
        assert!(text.contains("任意名.exe"));
        assert!(text.contains("mzHook32.dll"));
        game.mtool_target_exe = Some("../else.exe".into());
        assert!(debug_bat(&game, &settings).is_err());
    }
}
