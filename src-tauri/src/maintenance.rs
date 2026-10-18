use crate::domain::{Error, Game, LibraryPathCheck, RelocateGame, Result, Settings, ToolCheck};
use crate::paths::{contained_file, executable_directory, launch_file, path_text};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenpyAppdataPlan {
    pub game_id: String,
    pub path: String,
    install_path: String,
    working_directory: String,
    identities: Vec<(std::path::PathBuf, String)>,
}

fn invalid_cleanup(message: &str) -> Error {
    Error::Validation(message.into())
}

pub(crate) fn ensure_appdata_cleanup_idle(
    activity: usize,
    active: bool,
    pending: bool,
) -> Result<()> {
    if activity > 0 || active || pending {
        return Err(invalid_cleanup(
            "请先结束当前任务、启动或存档写入请求，并恢复未完成的文件操作",
        ));
    }
    Ok(())
}

pub fn preview_renpy_appdata(
    game: &Game,
    games: &[Game],
    settings: &Settings,
    data: &Path,
    appdata: &Path,
) -> Result<RenpyAppdataPlan> {
    if game.engine != "Ren'Py" {
        return Err(invalid_cleanup("仅 Ren'Py 游戏支持清理 AppData 存档"));
    }
    let raw_root = Path::new(&game.install_path);
    crate::deletion::plain_path(raw_root)?;
    crate::deletion::plain_path(appdata)?;
    let root = dunce::canonicalize(raw_root)?;
    let anchor = dunce::canonicalize(appdata)?;
    let name = crate::save_detection::renpy_save_directory(&root, &game.working_directory)
        .ok_or_else(|| invalid_cleanup("未识别到明确的 config.save_directory，不能清理"))?;
    // tokens is Ren'Py's shared signing data, never a game's disposable save identity.
    if name
        .components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case("tokens"))
    {
        return Err(invalid_cleanup("不能清理公共 tokens 目录"));
    }
    if game.working_directory != "." {
        if let Some(other) = crate::save_detection::renpy_save_directory(&root, ".") {
            if other != name {
                return Err(invalid_cleanup(
                    "游戏目录内的 Ren'Py 存档身份冲突，不能清理",
                ));
            }
        }
    }
    let renpy = anchor.join("RenPy");
    let raw = renpy.join(&name);
    crate::deletion::plain_path(&raw)?;
    let renpy = dunce::canonicalize(&renpy)?;
    let path = dunce::canonicalize(&raw).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            invalid_cleanup("对应 AppData 存档目录不存在")
        } else {
            error.into()
        }
    })?;
    if !path.is_dir() || path == renpy || !path.starts_with(&renpy) {
        return Err(invalid_cleanup("AppData 存档目录无效或越界"));
    }
    for other in games.iter().filter(|other| other.id != game.id) {
        // Check declared identities even when the other game maintains only game/saves.
        for work in [other.working_directory.as_str(), "."] {
            if let Some(other_name) =
                crate::save_detection::renpy_save_directory(Path::new(&other.install_path), work)
            {
                let other_raw = renpy.join(other_name);
                let other_path = std::path::PathBuf::from(crate::paths::path_key(
                    &dunce::canonicalize(&other_raw).unwrap_or(other_raw),
                )?);
                let key = std::path::PathBuf::from(crate::paths::path_key(&path)?);
                if other_path.starts_with(&key) || key.starts_with(&other_path) {
                    return Err(invalid_cleanup(
                        "此 AppData 存档目录被多个库内游戏使用，不能清理",
                    ));
                }
            }
        }
    }
    crate::deletion::validate_save_cleanup(&path, game, games, settings, data)?;
    let mut directories = vec![root, anchor.clone(), renpy.clone()];
    let mut current = renpy;
    for component in name.components() {
        current.push(component);
        directories.push(current.clone());
    }
    let identities = directories
        .into_iter()
        .map(|directory| {
            crate::importer::identity(&directory).map(|identity| (directory, identity))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(RenpyAppdataPlan {
        game_id: game.id.clone(),
        path: path_text(&path)?,
        install_path: game.install_path.clone(),
        working_directory: game.working_directory.clone(),
        identities,
    })
}

pub fn recycle_renpy_appdata(
    plan: &RenpyAppdataPlan,
    current: &RenpyAppdataPlan,
    recycle: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    if plan != current {
        return Err(invalid_cleanup(
            "目录身份或游戏配置已变化，请重新打开确认框",
        ));
    }
    for (path, identity) in &plan.identities {
        crate::deletion::plain_path(path)?;
        crate::importer::require_identity(path, Some(identity))?;
    }
    let path = Path::new(&plan.path);
    recycle(path)?;
    if path.try_exists()? {
        return Err(invalid_cleanup(
            "AppData 存档仍在原位置，未完成回收，请重试",
        ));
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RenameRecord {
    id: String,
    old: crate::importer::GameIdentity,
    new: crate::importer::GameIdentity,
    directory_id: String,
    parent_id: String,
    version: String,
    version_source: String,
}
const RENAME_RECORD: &str = "game-rename.json";

fn rename_record(game: &Game, name: &str, data: &Path) -> Result<RenameRecord> {
    crate::importer::folder_name(name)?;
    let root = dunce::canonicalize(&game.install_path)?;
    if crate::scanner::is_link(&std::fs::symlink_metadata(&game.install_path)?) || !root.is_dir() {
        return Err(Error::Validation(
            "只能重命名实际游戏目录，不能修改链接".into(),
        ));
    }
    let parent = root
        .parent()
        .ok_or_else(|| Error::Validation("不能重命名盘符根目录".into()))?;
    let target = parent.join(name);
    if crate::paths::path_key(&root)? == crate::paths::path_key(&target)? {
        return Err(Error::Validation("新名称与当前目录相同".into()));
    }
    if target.try_exists()? {
        return Err(Error::Validation("目标目录已存在，不会覆盖".into()));
    }
    if dunce::canonicalize(data)?.starts_with(&root) || std::env::current_exe()?.starts_with(&root)
    {
        return Err(Error::Validation(
            "不能重命名包含管理器或资料数据的目录".into(),
        ));
    }
    let (suggested, source) = crate::version::suggest(name, None);
    let (version, version_source) = if suggested == "Unknown" {
        (game.current_version.clone(), game.version_source.clone())
    } else {
        (suggested, source)
    };
    Ok(RenameRecord {
        id: game.id.clone(),
        old: crate::importer::GameIdentity::from_game(game),
        new: crate::importer::GameIdentity {
            install_path: path_text(&target)?,
            display_title: name.into(),
            canonical_title: name.into(),
        },
        directory_id: crate::importer::identity(&root)?,
        parent_id: crate::importer::identity(parent)?,
        version,
        version_source,
    })
}

fn write_rename_record(data: &Path, record: &RenameRecord) -> Result<()> {
    use std::io::Write;
    let temp = data.join(format!(".game-rename-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec(record).map_err(|e| Error::Validation(e.to_string()))?)?;
        file.sync_all()?;
        drop(file);
        crate::importer::rename_new(&temp, &data.join(RENAME_RECORD))
    })();
    if temp.exists() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn finish_rename(record: &RenameRecord, db: &mut crate::db::Database, data: &Path) -> Result<Game> {
    let old = Path::new(&record.old.install_path);
    let new = Path::new(&record.new.install_path);
    let parent = old
        .parent()
        .ok_or_else(|| Error::Validation("改名记录缺少父目录".into()))?;
    let name = new
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Validation("改名记录名称无效".into()))?;
    crate::importer::folder_name(name)?;
    if new.parent() != Some(parent)
        || record.new.display_title != name
        || record.new.canonical_title != name
        || crate::paths::path_key(old)? == crate::paths::path_key(new)?
        || dunce::canonicalize(data)?.starts_with(old)
        || dunce::canonicalize(data)?.starts_with(new)
    {
        return Err(Error::Validation("改名记录路径无效，文件已保留".into()));
    }
    crate::importer::require_identity(parent, Some(&record.parent_id))?;
    let actual = crate::importer::GameIdentity::from_game(&db.game(&record.id)?);
    if actual != record.old && actual != record.new {
        return Err(Error::Validation("游戏资料已改变，不能继续改名".into()));
    }
    if db
        .registered_id(new)?
        .is_some_and(|owner| owner != record.id)
    {
        return Err(Error::Validation("新目录已经关联其他游戏".into()));
    }
    crate::runtime::ensure_processes_idle(&[old.to_path_buf(), new.to_path_buf()])?;
    if old.try_exists()? {
        crate::importer::require_identity(old, Some(&record.directory_id))?;
        crate::importer::rename_new(old, new)?;
    }
    crate::importer::require_identity(new, Some(&record.directory_id))?;
    if let Err(error) = db.write_renamed_identity(
        &record.id,
        &record.old,
        &record.new,
        &record.version,
        &record.version_source,
    ) {
        if actual == record.old && crate::importer::rename_new(new, old).is_ok() {
            std::fs::remove_file(data.join(RENAME_RECORD))?;
        }
        return Err(error);
    }
    std::fs::remove_file(data.join(RENAME_RECORD))?;
    db.game(&record.id)
}

/// Metadata-only journal plus a same-parent rename; no copy, scan or new library identity.
pub fn rename_game(
    db: &mut crate::db::Database,
    data: &Path,
    id: &str,
    expected: &str,
    name: &str,
) -> Result<Game> {
    recover_rename(db, data)?;
    let game = db.game(id)?;
    if game.install_path != expected {
        return Err(Error::Validation("游戏目录已改变，请重新打开详情".into()));
    }
    let record = rename_record(&game, name, data)?;
    if db
        .registered_id(Path::new(&record.new.install_path))?
        .is_some()
    {
        return Err(Error::Validation("目标目录已有库记录".into()));
    }
    let root_key = std::path::PathBuf::from(crate::paths::path_key(Path::new(&game.install_path))?);
    for other in db.games()?.into_iter().filter(|other| other.id != id) {
        let other_key =
            std::path::PathBuf::from(crate::paths::path_key(Path::new(&other.install_path))?);
        if other_key.starts_with(&root_key) || root_key.starts_with(&other_key) {
            return Err(Error::Validation(
                "目录与其他游戏记录重叠，不能直接重命名".into(),
            ));
        }
        for save in &other.save_paths {
            if let Ok(path) = crate::deletion::resolve_save(&other, save) {
                if std::path::PathBuf::from(crate::paths::path_key(&path)?).starts_with(&root_key) {
                    return Err(Error::Validation(
                        "此目录被其他游戏作为存档位置使用，不能直接重命名".into(),
                    ));
                }
            }
        }
    }
    write_rename_record(data, &record)?;
    let result = finish_rename(&record, db, data);
    // A collision/locked folder before switching needs no recovery; leave the original intact.
    if result.is_err()
        && Path::new(&record.old.install_path).is_dir()
        && db
            .game(id)
            .is_ok_and(|game| crate::importer::GameIdentity::from_game(&game) == record.old)
        && crate::importer::identity(Path::new(&record.old.install_path))
            .ok()
            .as_deref()
            == Some(record.directory_id.as_str())
    {
        let _ = std::fs::remove_file(data.join(RENAME_RECORD));
    }
    result
}

pub fn recover_rename(db: &mut crate::db::Database, data: &Path) -> Result<()> {
    let path = data.join(RENAME_RECORD);
    if !path.try_exists()? {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(&path)?;
    if !metadata.is_file() || crate::scanner::is_link(&metadata) || metadata.len() > 64 * 1024 {
        return Err(Error::Validation("目录改名恢复记录无效，文件已保留".into()));
    }
    let record: RenameRecord =
        serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))
            .map_err(|e| Error::Validation(format!("目录改名恢复记录无法读取：{e}")))?;
    finish_rename(&record, db, data)?;
    Ok(())
}

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

    fn appdata_fixture(
        temp: &Path,
    ) -> (
        crate::db::Database,
        std::path::PathBuf,
        Game,
        std::path::PathBuf,
    ) {
        let (db, data, id) = rename_fixture(temp);
        let mut game = db.game(&id).unwrap();
        game.engine = "Ren'Py".into();
        let root = Path::new(&game.install_path);
        std::fs::create_dir_all(root.join("game/saves")).unwrap();
        std::fs::write(
            root.join("game/options.rpy"),
            "define config.save_directory = '彼女-1234'\n",
        )
        .unwrap();
        std::fs::write(root.join("game/saves/local.save"), b"local unchanged").unwrap();
        let appdata = temp.join("AppData");
        std::fs::create_dir_all(appdata.join("RenPy/彼女-1234")).unwrap();
        std::fs::write(
            appdata.join("RenPy/彼女-1234/persistent"),
            b"external fixture",
        )
        .unwrap();
        (db, data, game, appdata)
    }

    #[test]
    fn appdata_cleanup_preserves_local_saves_configuration_and_records_on_success_or_failure() {
        let temp = tempfile::tempdir().unwrap();
        let (db, data, mut game, appdata) = appdata_fixture(temp.path());
        let record = serde_json::to_string(&db.game(&game.id).unwrap()).unwrap();
        game.save_paths
            .push(path_text(&appdata.join("RenPy/彼女-1234")).unwrap());
        let metadata = serde_json::to_string(&game).unwrap();
        let games = vec![game.clone()];
        let settings = Settings::default();
        let plan = preview_renpy_appdata(&game, &games, &settings, &data, &appdata).unwrap();
        assert_eq!(
            crate::save_detection::detect_for_engine(
                Path::new(&game.install_path),
                ".",
                "Ren'Py",
                None
            ),
            vec!["<GAME>/game/saves"]
        );
        assert!(recycle_renpy_appdata(&plan, &plan, |_| Err(invalid_cleanup(
            "fixture recycle failure"
        )))
        .unwrap_err()
        .to_string()
        .contains("fixture recycle failure"));
        assert!(Path::new(&plan.path).is_dir());
        assert!(recycle_renpy_appdata(&plan, &plan, |_| Ok(())).is_err());
        recycle_renpy_appdata(&plan, &plan, |path| {
            assert_eq!(
                path,
                dunce::canonicalize(appdata.join("RenPy/彼女-1234")).unwrap()
            );
            std::fs::rename(path, temp.path().join("fixture-trash"))?;
            Ok(())
        })
        .unwrap();
        assert!(preview_renpy_appdata(&game, &games, &settings, &data, &appdata).is_err());
        assert_eq!(
            std::fs::read(temp.path().join("fixture-trash/persistent")).unwrap(),
            b"external fixture"
        );
        assert_eq!(
            std::fs::read(Path::new(&game.install_path).join("game/saves/local.save")).unwrap(),
            b"local unchanged"
        );
        assert_eq!(serde_json::to_string(&game).unwrap(), metadata);
        assert_eq!(
            serde_json::to_string(&db.game(&game.id).unwrap()).unwrap(),
            record
        );
    }

    #[cfg(windows)]
    #[test]
    fn appdata_manual_cleanup_does_not_consult_running_game_state() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        let temp = tempfile::tempdir().unwrap();
        let (_db, data, game, appdata) = appdata_fixture(temp.path());
        let mut child = Command::new(std::env::var_os("ComSpec").unwrap())
            .args(["/d", "/q", "/c", "set /p fixture="])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let running = crate::runtime::RunningGames::default();
        running
            .observe(
                &game.id,
                crate::runtime::ProcessLease::from_child(child).unwrap(),
            )
            .unwrap();
        assert!(running.ensure_tracked_idle(&game).is_err());
        ensure_appdata_cleanup_idle(0, false, false).unwrap();
        let plan = preview_renpy_appdata(
            &game,
            std::slice::from_ref(&game),
            &Settings::default(),
            &data,
            &appdata,
        )
        .unwrap();
        recycle_renpy_appdata(&plan, &plan, |path| {
            std::fs::rename(path, temp.path().join("fixture-trash"))?;
            Ok(())
        })
        .unwrap();
        assert!(running.ensure_tracked_idle(&game).is_err());
        assert_eq!(
            std::fs::read(Path::new(&game.install_path).join("game/saves/local.save")).unwrap(),
            b"local unchanged"
        );
        drop(input);
    }

    #[test]
    fn appdata_cleanup_rejects_ambiguous_public_shared_and_install_paths() {
        let temp = tempfile::tempdir().unwrap();
        let (_db, data, game, appdata) = appdata_fixture(temp.path());
        let settings = Settings::default();
        for expression in [
            "''",
            "'..'",
            "'../outside'",
            "'RenPy'",
            "'tokens'",
            "'Tokens/nested'",
            "'/absolute'",
            "title + '-1234'",
            "'missing'",
            "'彼女-1234'\ndefine config.save_directory = 'second'",
            "'彼女-1234'; config.save_directory = 'second'",
            "'彼女-1234'\nconfig.savedir = custom",
        ] {
            std::fs::write(
                Path::new(&game.install_path).join("game/options.rpy"),
                format!("define config.save_directory = {expression}\n"),
            )
            .unwrap();
            assert!(
                preview_renpy_appdata(
                    &game,
                    std::slice::from_ref(&game),
                    &settings,
                    &data,
                    &appdata
                )
                .is_err(),
                "{expression}"
            );
        }
        std::fs::write(
            Path::new(&game.install_path).join("game/options.rpy"),
            "define config.save_directory = '彼女-1234'\n",
        )
        .unwrap();
        let mut other = game.clone();
        other.id = "another-game".into();
        other.install_path = temp.path().join("other").display().to_string();
        std::fs::create_dir_all(Path::new(&other.install_path).join("game")).unwrap();
        std::fs::copy(
            Path::new(&game.install_path).join("game/options.rpy"),
            Path::new(&other.install_path).join("game/options.rpy"),
        )
        .unwrap();
        assert!(preview_renpy_appdata(
            &game,
            &[game.clone(), other.clone()],
            &settings,
            &data,
            &appdata
        )
        .unwrap_err()
        .to_string()
        .contains("多个"));
        std::fs::remove_file(Path::new(&other.install_path).join("game/options.rpy")).unwrap();
        other.save_paths = vec![path_text(&appdata.join("RenPy/彼女-1234")).unwrap()];
        assert!(
            preview_renpy_appdata(&game, &[game.clone(), other], &settings, &data, &appdata)
                .is_err()
        );
        let mut installed = game.clone();
        installed.install_path = path_text(&appdata.join("RenPy/彼女-1234")).unwrap();
        std::fs::create_dir_all(Path::new(&installed.install_path).join("game")).unwrap();
        std::fs::copy(
            Path::new(&game.install_path).join("game/options.rpy"),
            Path::new(&installed.install_path).join("game/options.rpy"),
        )
        .unwrap();
        assert!(preview_renpy_appdata(
            &installed,
            std::slice::from_ref(&installed),
            &settings,
            &data,
            &appdata
        )
        .is_err());
        let mut unknown = game;
        unknown.engine = "Unknown".into();
        assert!(preview_renpy_appdata(&unknown, &[], &settings, &data, &appdata).is_err());
    }

    #[test]
    fn appdata_cleanup_revalidates_identity_config_and_task_state_before_recycling() {
        let temp = tempfile::tempdir().unwrap();
        let (_db, data, game, appdata) = appdata_fixture(temp.path());
        let settings = Settings::default();
        let games = vec![game.clone()];
        let plan = preview_renpy_appdata(&game, &games, &settings, &data, &appdata).unwrap();
        for (activity, active, pending) in [(1, false, false), (0, true, false), (0, false, true)] {
            assert!(ensure_appdata_cleanup_idle(activity, active, pending).is_err());
        }
        ensure_appdata_cleanup_idle(0, false, false).unwrap();
        std::fs::rename(&plan.path, appdata.join("RenPy/original-preserved")).unwrap();
        std::fs::create_dir(&plan.path).unwrap();
        assert!(recycle_renpy_appdata(&plan, &plan, |_| panic!(
            "replaced directory must not recycle"
        ))
        .is_err());
        let current = preview_renpy_appdata(&game, &games, &settings, &data, &appdata).unwrap();
        assert!(recycle_renpy_appdata(&plan, &current, |_| panic!(
            "stale confirmation must not recycle"
        ))
        .is_err());
        std::fs::write(
            Path::new(&game.install_path).join("game/options.rpy"),
            "define config.save_directory = 'original-preserved'\n",
        )
        .unwrap();
        let changed = preview_renpy_appdata(&game, &games, &settings, &data, &appdata).unwrap();
        assert!(recycle_renpy_appdata(&plan, &changed, |_| panic!(
            "changed config must not recycle"
        ))
        .is_err());
        assert!(appdata
            .join("RenPy/original-preserved/persistent")
            .is_file());
    }

    #[cfg(windows)]
    #[test]
    fn appdata_cleanup_refuses_junctions_in_target_contents_and_ancestors() {
        use std::os::windows::process::CommandExt;
        let temp = tempfile::tempdir().unwrap();
        let (_db, data, game, appdata) = appdata_fixture(temp.path());
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        for link in [
            appdata.join("RenPy/彼女-1234/link"),
            appdata.join("RenPy/link"),
            temp.path().join("linked-appdata"),
        ] {
            let target = if link.file_name().unwrap() == "linked-appdata" {
                &appdata
            } else {
                &outside
            };
            let result = std::process::Command::new("cmd.exe")
                .args(["/C", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(target.to_string_lossy().replace('/', "\\"))
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(result.status.success(), "{:?}", result);
            let selected = if link.file_name().unwrap() == "linked-appdata" {
                &link
            } else {
                &appdata
            };
            if link == appdata.join("RenPy/link") {
                std::fs::write(
                    Path::new(&game.install_path).join("game/options.rpy"),
                    "define config.save_directory = 'link'\n",
                )
                .unwrap();
            }
            assert!(preview_renpy_appdata(
                &game,
                std::slice::from_ref(&game),
                &Settings::default(),
                &data,
                selected
            )
            .is_err());
            std::fs::remove_dir(&link).unwrap();
            std::fs::write(
                Path::new(&game.install_path).join("game/options.rpy"),
                "define config.save_directory = '彼女-1234'\n",
            )
            .unwrap();
        }
        assert!(outside.is_dir());
    }

    fn rename_fixture(temp: &Path) -> (crate::db::Database, std::path::PathBuf, String) {
        let data = temp.join("manager-data");
        let root = temp.join("游戏 v1.0");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(root.join("save")).unwrap();
        std::fs::write(root.join("game.html"), b"unchanged game").unwrap();
        std::fs::write(root.join("save/progress"), b"unchanged progress").unwrap();
        let mut db = crate::db::Database::open(&data.join("fixture.db")).unwrap();
        let id = db
            .register(&[crate::scanner::analyze_directory(&root).unwrap()])
            .unwrap()
            .remove(0);
        let connection = rusqlite::Connection::open(data.join("fixture.db")).unwrap();
        connection.execute("INSERT INTO aliases(id,game_id,alias,normalized_alias,source) VALUES('manual',?1,'手动译名','手动译名','manual')", [&id]).unwrap();
        for (key, path) in [
            ("absolute", path_text(&root.join("save")).unwrap()),
            ("token", "<GAME>/save".into()),
            ("external", path_text(&temp.join("external-save")).unwrap()),
        ] {
            connection
                .execute(
                    "INSERT OR IGNORE INTO save_paths(id,game_id,path) VALUES(?1,?2,?3)",
                    rusqlite::params![key, id, path],
                )
                .unwrap();
        }
        db.record_launch(&id).unwrap();
        (db, data, id)
    }

    #[test]
    fn directory_rename_preserves_records_and_rebases_only_internal_paths() {
        let temp = tempfile::tempdir().unwrap();
        let (mut db, data, id) = rename_fixture(temp.path());
        let old = db.game(&id).unwrap();
        let saved = rename_game(&mut db, &data, &id, &old.install_path, "新名称 v2.0").unwrap();
        assert_eq!(saved.id, old.id);
        assert_eq!(saved.display_title, "新名称 v2.0");
        assert_eq!(saved.canonical_title, saved.display_title);
        assert_eq!(saved.current_version, "v2.0");
        assert_eq!(saved.created_at, old.created_at);
        assert_eq!(saved.last_launched_at, old.last_launched_at);
        assert_eq!(saved.play_status, old.play_status);
        assert_eq!(saved.aliases, old.aliases);
        assert_eq!(saved.main_executable, old.main_executable);
        assert_eq!(saved.save_paths.len(), old.save_paths.len());
        for path in &old.save_paths {
            assert!(saved.save_paths.contains(&rebase_save(
                path,
                Path::new(&old.install_path),
                Path::new(&saved.install_path)
            )));
        }
        assert!(!Path::new(&old.install_path).exists());
        assert_eq!(
            std::fs::read(Path::new(&saved.install_path).join("save/progress")).unwrap(),
            b"unchanged progress"
        );
        assert!(db.version_history(&id).unwrap().is_empty());
        let renamed =
            rename_game(&mut db, &data, &id, &saved.install_path, "新名称 无版本号").unwrap();
        assert_eq!(renamed.current_version, saved.current_version);
        drop(db);
        assert_eq!(
            crate::db::Database::open(&data.join("fixture.db"))
                .unwrap()
                .game(&id)
                .unwrap()
                .install_path,
            renamed.install_path
        );
        assert!(!data.join(RENAME_RECORD).exists());
    }

    #[test]
    fn directory_rename_rejects_collision_invalid_names_stale_paths_and_restores_after_db_failure()
    {
        let temp = tempfile::tempdir().unwrap();
        let (mut db, data, id) = rename_fixture(temp.path());
        let original = db.game(&id).unwrap();
        let before = serde_json::to_string(&original).unwrap();
        for name in ["../outside", "CON", "bad?name", " trailing "] {
            assert!(rename_game(&mut db, &data, &id, &original.install_path, name).is_err());
        }
        std::fs::create_dir(temp.path().join("occupied")).unwrap();
        assert!(rename_game(&mut db, &data, &id, &original.install_path, "occupied").is_err());
        assert!(rename_game(&mut db, &data, &id, "stale", "new-name").is_err());
        let connection = rusqlite::Connection::open(data.join("fixture.db")).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_rename BEFORE UPDATE ON games BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        assert!(rename_game(&mut db, &data, &id, &original.install_path, "new-name").is_err());
        assert!(Path::new(&original.install_path).is_dir());
        assert!(!temp.path().join("new-name").exists());
        assert!(!data.join(RENAME_RECORD).exists());
        assert_eq!(
            serde_json::to_string(&db.game(&id).unwrap()).unwrap(),
            before
        );
        connection
            .execute_batch("DROP TRIGGER fail_rename;")
            .unwrap();
        assert!(rename_game(&mut db, &data, &id, &original.install_path, "new-name").is_ok());
    }

    #[test]
    fn directory_rename_recovers_each_transition_idempotently_and_refuses_replaced_directory() {
        for stage in ["prepared", "renamed", "committed", "replaced"] {
            let temp = tempfile::tempdir().unwrap();
            let (mut db, data, id) = rename_fixture(temp.path());
            let old = db.game(&id).unwrap();
            let record = rename_record(&old, "新目录 v2.0", &data).unwrap();
            write_rename_record(&data, &record).unwrap();
            if stage == "replaced" {
                std::fs::rename(
                    &record.old.install_path,
                    temp.path().join("original-preserved"),
                )
                .unwrap();
                std::fs::create_dir(&record.old.install_path).unwrap();
            } else if stage != "prepared" {
                crate::importer::rename_new(
                    Path::new(&record.old.install_path),
                    Path::new(&record.new.install_path),
                )
                .unwrap();
                if stage == "committed" {
                    db.write_renamed_identity(
                        &id,
                        &record.old,
                        &record.new,
                        &record.version,
                        &record.version_source,
                    )
                    .unwrap();
                }
            }
            drop(db);
            let mut db = crate::db::Database::open(&data.join("fixture.db")).unwrap();
            if stage == "replaced" {
                assert!(recover_rename(&mut db, &data).is_err());
                assert_eq!(db.game(&id).unwrap().install_path, old.install_path);
                assert!(temp
                    .path()
                    .join("original-preserved/save/progress")
                    .is_file());
                assert!(!Path::new(&record.new.install_path).exists());
            } else {
                recover_rename(&mut db, &data).unwrap();
                recover_rename(&mut db, &data).unwrap();
                let saved = db.game(&id).unwrap();
                assert_eq!(saved.install_path, record.new.install_path);
                assert_eq!(saved.aliases, old.aliases);
                assert_eq!(saved.created_at, old.created_at);
                assert!(Path::new(&saved.install_path)
                    .join("save/progress")
                    .is_file());
            }
        }
    }

    #[test]
    fn directory_rename_blocks_overlapping_records_and_shared_internal_saves() {
        let temp = tempfile::tempdir().unwrap();
        let (mut db, data, id) = rename_fixture(temp.path());
        let original = db.game(&id).unwrap();
        let nested = Path::new(&original.install_path).join("nested-game");
        std::fs::create_dir(&nested).unwrap();
        let other_id = db
            .register(&[crate::scanner::analyze_directory(&nested).unwrap()])
            .unwrap()
            .remove(0);
        assert!(rename_game(&mut db, &data, &id, &original.install_path, "new-name").is_err());
        db.remove_game(&other_id).unwrap();
        let sibling = temp.path().join("other-game");
        std::fs::create_dir(&sibling).unwrap();
        let other_id = db
            .register(&[crate::scanner::analyze_directory(&sibling).unwrap()])
            .unwrap()
            .remove(0);
        let connection = rusqlite::Connection::open(data.join("fixture.db")).unwrap();
        connection
            .execute(
                "INSERT INTO save_paths(id,game_id,path) VALUES('shared',?1,?2)",
                rusqlite::params![
                    other_id,
                    path_text(&Path::new(&original.install_path).join("save")).unwrap()
                ],
            )
            .unwrap();
        assert!(rename_game(&mut db, &data, &id, &original.install_path, "new-name").is_err());
        assert!(Path::new(&original.install_path)
            .join("save/progress")
            .is_file());
        assert!(!data.join(RENAME_RECORD).exists());
    }

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
            favorite: false,
            tags: vec![],
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
