//! Version replacement uses temporary staging, then recycles the previous version.
//! Completed operations retain only journal/configuration metadata, never game copies.
use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct VersionConfig {
    pub version: String,
    pub version_source: String,
    pub engine: String,
    pub engine_source: String,
    pub launch_source: String,
    pub executable: Option<String>,
    pub working_directory: String,
    pub launch_type: String,
    pub external_player: Option<crate::domain::ExternalPlayer>,
    pub mtool_target: Option<String>,
    pub mtool_loader: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct VersionHistory {
    pub operation: String,
    pub old_version: String,
    pub new_version: String,
    pub status: String,
    pub created_at: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct SaveSnapshot {
    pub configured: String,
    pub source: String,
    pub relative: Option<String>,
    pub directory: bool,
    pub present: bool,
    pub bytes: u64,
    digest: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct UpdateData {
    pub old_version: String,
    pub saves: Vec<SaveSnapshot>,
    pub required_bytes: u64,
    pub quarantine: String,
    pub rollback_available: bool,
    old: VersionConfig,
    pub(super) old_id: String,
    old_digest: String,
    old_updated_at: String,
    old_save_paths: Vec<String>,
    #[serde(default)]
    ready_digest: Option<String>,
    #[serde(default)]
    rollback_id: Option<String>,
    #[serde(default)]
    rollback_saves: Vec<SaveSnapshot>,
    #[serde(default)]
    rollback_started: bool,
    #[serde(default)]
    pub(super) cleanup_ready: bool,
    /// Older journals retain their original recovery checks. New plans move whole folders.
    #[serde(default)]
    pub(super) lightweight: bool,
    /// Keep the original configuration calculation for journals created before this fix.
    #[serde(default)]
    inherit_launch_config: bool,
    /// Older in-flight/completed journals may contain a loader on a non-MTool version.
    #[serde(default)]
    mtool_loader_launch_scoped: bool,
    #[serde(default)]
    incoming_saves: Vec<SaveSnapshot>,
    #[serde(default)]
    rollback_originals: Vec<SaveSnapshot>,
}

fn check_old_version(path: &Path, update: &UpdateData, job: &Job) -> Result<()> {
    require_identity(path, Some(&update.old_id))?;
    if !update.lightweight {
        require_digest(path, &update.old_digest, job)?;
    }
    Ok(())
}

fn version_digest(path: &Path, update: &UpdateData, job: &Job) -> Result<Option<String>> {
    if update.lightweight {
        Ok(None)
    } else {
        Ok(Some(digest(&inventory(path, job)?)?))
    }
}

fn local_originals(root: &Path, saves: &[SaveSnapshot], job: &Job) -> Result<Vec<SaveSnapshot>> {
    let mut game = dummy_game();
    game.install_path = paths::path_text(root)?;
    game.save_paths = saves
        .iter()
        .filter_map(|save| save.relative.clone())
        .collect();
    save_snapshots(&game, job)
}

/// Classify without reading external folders, including unavailable AppData variables.
fn internal_save_path(game: &Game, configured: &str) -> Result<Option<PathBuf>> {
    let value = configured
        .trim()
        .replace("<GAME>", &game.install_path)
        .replace('\\', "/");
    if value.starts_with('%') {
        return Ok(None);
    }
    let path = PathBuf::from(&value);
    if path.is_absolute() {
        let root_key = paths::path_key(Path::new(&game.install_path))?.replace('\\', "/");
        let key = paths::path_key(&path)?.replace('\\', "/");
        if !Path::new(&key).starts_with(Path::new(&root_key)) {
            return Ok(None);
        }
    }
    let path = crate::deletion::resolve_save(game, configured)?;
    let root = checked_dir(Path::new(&game.install_path))?;
    if !path.starts_with(&root) || path == root {
        return Err(invalid("存档必须是游戏目录内的文件或子目录"));
    }
    Ok(Some(path))
}

pub(super) fn digest(entries: &[Entry]) -> Result<String> {
    // Stable metadata fingerprint across app/toolchain updates. Byte equality is checked separately.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in serde_json::to_vec(entries).map_err(|e| invalid(e.to_string()))? {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}
fn path_inventory(path: &Path, job: &Job) -> Result<Vec<Entry>> {
    let metadata = fs::symlink_metadata(path)?;
    if scanner::is_link(&metadata) {
        return Err(invalid("存档或暂存内容含链接"));
    }
    if metadata.is_dir() {
        return inventory(path, job);
    }
    if !metadata.is_file() {
        return Err(invalid("存档不是普通文件或目录"));
    }
    Ok(vec![Entry {
        path: "file".into(),
        directory: false,
        bytes: metadata.len(),
        modified: metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("存档时间不可读"))?
            .as_nanos(),
    }])
}
fn require_digest(path: &Path, expected: &str, job: &Job) -> Result<()> {
    if digest(&path_inventory(path, job)?)? != expected {
        return Err(invalid(format!(
            "内容在确认后发生变化，请重新生成计划：{}",
            path.display()
        )));
    }
    Ok(())
}
fn copy_path(source: &Path, target: &Path, job: &Job) -> Result<()> {
    for ancestor in target.ancestors() {
        if let Ok(metadata) = fs::symlink_metadata(ancestor) {
            if scanner::is_link(&metadata) {
                return Err(invalid("存档目标包含链接"));
            }
        }
    }
    let entries = path_inventory(source, job)?;
    if fs::metadata(source)?.is_dir() {
        copy_tree(source, target, &entries, job)?;
        unchanged(source, &entries, job)?;
        verify_copy(source, target, &entries, job)?;
    } else {
        if exists(target)? {
            equal_prefix(source, target, job)?;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
            checked_dir(parent)?;
        }
        let mut input = locked_read(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(target)?;
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            stopped(job)?;
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n])?;
        }
        output.sync_all()?;
        output.set_times(fs::FileTimes::new().set_modified(input.metadata()?.modified()?))?;
        drop(output);
        drop(input);
        equal_file(source, target, job)?;
        if path_inventory(source, job)? != entries {
            return Err(invalid("复制时存档发生变化"));
        }
    }
    Ok(())
}
pub(super) fn save_snapshots(game: &Game, job: &Job) -> Result<Vec<SaveSnapshot>> {
    let root = checked_dir(Path::new(&game.install_path))?;
    let mut saves: Vec<SaveSnapshot> = vec![];
    for configured in &game.save_paths {
        let Some(path) = internal_save_path(game, configured)? else {
            continue;
        };
        if overlaps(&root, &path)? && !path.starts_with(&root) || path == root {
            return Err(invalid("存档范围不能包含整个游戏或其父目录"));
        }
        if saves.iter().any(|s| path.starts_with(Path::new(&s.source))) {
            continue;
        }
        saves.retain(|s| !Path::new(&s.source).starts_with(&path));
        let present = exists(&path)?;
        let entries = if present {
            path_inventory(&path, job)?
        } else {
            vec![]
        };
        saves.push(SaveSnapshot {
            configured: configured.clone(),
            source: paths::path_text(&path)?,
            relative: path
                .strip_prefix(&root)
                .ok()
                .map(paths::path_text)
                .transpose()?,
            directory: present && fs::metadata(&path)?.is_dir(),
            present,
            bytes: entries.iter().map(|e| e.bytes).sum(),
            digest: digest(&entries)?,
        });
    }
    Ok(saves)
}
fn snapshot_saves(saves: &[SaveSnapshot], directory: &Path, job: &Job) -> Result<()> {
    fs::create_dir_all(directory)?;
    checked_dir(directory)?;
    for (i, save) in saves.iter().enumerate() {
        stopped(job)?;
        if save.relative.is_none() {
            continue;
        }
        if !save.present {
            if exists(Path::new(&save.source))? {
                return Err(invalid("原先不存在的存档已出现，请重新确认"));
            }
            continue;
        }
        let source = Path::new(&save.source);
        require_digest(source, &save.digest, job)?;
        crate::runtime::ensure_paths_idle(&[source.to_path_buf()])?;
        copy_path(source, &directory.join(i.to_string()), job)?;
        require_digest(source, &save.digest, job)?;
    }
    Ok(())
}
fn restore_saves(
    saves: &[SaveSnapshot],
    snapshots: &Path,
    target: &Path,
    bundled: &Path,
    originals: Option<&[SaveSnapshot]>,
    job: &Job,
) -> Result<()> {
    fs::create_dir_all(bundled)?;
    checked_dir(bundled)?;
    for (i, save) in saves.iter().enumerate() {
        stopped(job)?;
        let Some(relative) = save.relative.as_ref().filter(|_| save.present) else {
            continue;
        };
        let destination = target.join(paths::relative_path(relative)?);
        // Resolving a fabricated local save checks every ancestor, including missing ones.
        let mut check = Game {
            install_path: paths::path_text(target)?,
            ..dummy_game()
        };
        check.save_paths = vec![];
        crate::deletion::resolve_save(&check, relative)?;
        let archive = bundled.join(i.to_string());
        let original = originals.and_then(|originals| originals.get(i));
        if exists(&destination)? && !exists(&archive)? && original.is_none_or(|save| save.present) {
            if let Some(original) = original {
                require_digest(&destination, &original.digest, job)?;
            }
            crate::runtime::ensure_paths_idle(std::slice::from_ref(&destination))?;
            rename_new(&destination, &archive)?;
        }
        let snapshot = snapshots.join(i.to_string());
        require_digest(&snapshot, &save.digest, job)?;
        copy_path(&snapshot, &destination, job)?;
        require_digest(&destination, &save.digest, job)?;
    }
    Ok(())
}

fn check_saved_paths(saves: &[SaveSnapshot], root: &Path, preserve: bool, job: &Job) -> Result<()> {
    if !preserve {
        return Ok(());
    }
    for save in saves {
        let Some(relative) = &save.relative else {
            continue;
        };
        let path = root.join(paths::relative_path(relative)?);
        if save.present {
            require_digest(&path, &save.digest, job)?;
        } else if exists(&path)? {
            return Err(invalid("确认后出现新的存档，请重新生成计划"));
        }
    }
    Ok(())
}
fn check_reviewed_saves(update: &UpdateData, preserve: bool, job: &Job) -> Result<()> {
    if !preserve {
        return Ok(());
    }
    for save in &update.saves {
        if save.present {
            require_digest(Path::new(&save.source), &save.digest, job)?;
        } else if exists(Path::new(&save.source))? {
            return Err(invalid("确认后出现新的存档，请重新生成计划"));
        }
    }
    Ok(())
}

/// Undo only the save overlay; moved incoming game files are never deleted on withdrawal.
fn undo_save_overlay(
    saves: &[SaveSnapshot],
    originals: &[SaveSnapshot],
    snapshots: &Path,
    target: &Path,
    bundled: &Path,
    job: &Job,
) -> Result<()> {
    for (i, save) in saves.iter().enumerate().filter(|(_, save)| save.present) {
        let original = originals
            .get(i)
            .ok_or_else(|| invalid("缺少原始存档记录"))?;
        let destination = target.join(paths::relative_path(
            save.relative
                .as_deref()
                .ok_or_else(|| invalid("存档路径无效"))?,
        )?);
        let archive = bundled.join(i.to_string());
        if original.present && !exists(&archive)? {
            require_digest(&destination, &original.digest, job)?;
            continue;
        }
        if exists(&destination)? {
            let snapshot = snapshots.join(i.to_string());
            let entries = path_inventory(&snapshot, job)?;
            if save.directory {
                remove_partial_copy(&snapshot, &destination, &entries, job)?;
            } else {
                equal_prefix(&snapshot, &destination, job)?;
                fs::remove_file(&destination)?;
            }
        }
        if original.present {
            require_digest(&archive, &original.digest, job)?;
            rename_new(&archive, &destination)?;
        }
    }
    Ok(())
}

/// Copy completion is checked by path, type and size, never by ordinary game-file bytes.
pub(super) fn remove_source_basic(
    source: &Path,
    target: &Path,
    container: &Path,
    item: &Item,
    job: &Job,
) -> Result<()> {
    if !exists(source)? {
        return Ok(());
    }
    require_identity(source, Some(&item.source_id))?;
    let update = item.update.as_ref();
    let expected: HashMap<_, _> = item
        .manifest
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let current = inventory(source, job)?;
    if current
        .iter()
        .any(|entry| expected.get(entry.path.as_str()).copied() != Some(entry))
    {
        return Err(invalid("来源文件在复制后新增或变化，保留来源"));
    }
    // Check all remaining copies before removing the first source file.
    let mut copies = Vec::new();
    for entry in current.iter().filter(|entry| !entry.directory) {
        stopped(job)?;
        let relative = paths::relative_path(&entry.path)?;
        let mut copy = target.join(&relative);
        if let Some(update) = update.filter(|_| item.selection.preserve_saves) {
            for (i, save) in update
                .saves
                .iter()
                .enumerate()
                .filter(|(_, save)| save.present)
            {
                let saved = paths::relative_path(save.relative.as_deref().unwrap())?;
                if let Ok(suffix) = relative.strip_prefix(&saved) {
                    copy = container
                        .join("bundled-saves")
                        .join(i.to_string())
                        .join(suffix);
                    break;
                }
            }
        }
        let metadata = fs::symlink_metadata(&copy)?;
        if scanner::is_link(&metadata) || !metadata.is_file() || metadata.len() != entry.bytes {
            return Err(invalid("跨盘复制未完成，保留来源"));
        }
        copies.push((entry, copy));
    }
    for (entry, copy) in copies {
        stopped(job)?;
        delete_sized_source(&source.join(&entry.path), &copy, entry)?;
    }
    let mut dirs = current
        .iter()
        .filter(|entry| entry.directory)
        .collect::<Vec<_>>();
    dirs.sort_by_key(|entry| std::cmp::Reverse(Path::new(&entry.path).components().count()));
    for entry in dirs {
        fs::remove_dir(source.join(&entry.path))?;
    }
    fs::remove_dir(source)?;
    Ok(())
}

fn delete_sized_source(source: &Path, copy: &Path, entry: &Entry) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::{
            Foundation::GENERIC_READ,
            Storage::FileSystem::{
                FileDispositionInfo, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_INFO,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            },
        };
        let input = OpenOptions::new()
            .access_mode(GENERIC_READ | DELETE)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(source)?;
        let output = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(copy)?;
        let metadata = input.metadata()?;
        let copied = output.metadata()?;
        if scanner::is_link(&metadata)
            || scanner::is_link(&copied)
            || !metadata.is_file()
            || !copied.is_file()
            || metadata.len() != entry.bytes
            || copied.len() != entry.bytes
            || metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map_err(|_| invalid("文件时间无效"))?
                .as_nanos()
                != entry.modified
        {
            return Err(invalid("来源在清理时变化，保留并停止"));
        }
        let information = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: the open handle locks this exact file; the structure matches the Windows ABI.
        if unsafe {
            SetFileInformationByHandle(
                input.as_raw_handle() as _,
                FileDispositionInfo,
                (&information as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    {
        if fs::metadata(source)?.len() != entry.bytes || fs::metadata(copy)?.len() != entry.bytes {
            return Err(invalid("复制未完成"));
        }
        fs::remove_file(source)?;
    }
    Ok(())
}
fn dummy_game() -> Game {
    Game {
        id: String::new(),
        canonical_title: String::new(),
        display_title: String::new(),
        install_path: String::new(),
        working_directory: ".".into(),
        current_version: "Unknown".into(),
        version_source: "manual".into(),
        main_executable: None,
        engine: "Unknown".into(),
        launch_type: "DIRECT".into(),
        external_player: None,
        mtool_target_exe: None,
        mtool_loader: None,
        created_at: String::new(),
        updated_at: String::new(),
        last_launched_at: None,
        play_status: Default::default(),
        aliases: vec![],
        save_paths: vec![],
    }
}
pub(super) fn validate_record(id: &str, root: &str, index: usize, item: &Item) -> Result<()> {
    let Some(update) = &item.update else {
        if item.state.starts_with("update_")
            || item.state.starts_with("rollback_")
            || item.state == "rolled_back"
        {
            return Err(invalid("更新状态缺少对应记录"));
        }
        return Ok(());
    };
    if item.selection.existing_id.as_deref() != Some(&item.game_id)
        || paths::path_key(Path::new(&update.quarantine))?
            != paths::path_key(&Path::new(root).join(format!(".butter-import-{id}-{index}")))?
        || paths::path_key(Path::new(&item.target))?
            != paths::path_key(&Path::new(root).join(&item.selection.target_name))?
    {
        return Err(invalid("更新记录身份或隔离目录不一致"));
    }
    uuid::Uuid::parse_str(&item.game_id).map_err(|_| invalid("更新游戏编号无效"))?;
    if update.old_id.is_empty() || !update.lightweight && update.old_digest.len() != 16 {
        return Err(invalid("更新缺少旧版本身份校验"));
    }
    if matches!(
        item.state.as_str(),
        "update_ready"
            | "update_isolate"
            | "update_publish"
            | "update_restore"
            | "update_commit"
            | "update_cleanup"
            | "update_recycle"
            | "update_finish"
            | "completed"
    ) && (item.payload_id.is_none() || !update.lightweight && update.ready_digest.is_none())
    {
        return Err(invalid("更新缺少新版本身份校验"));
    }
    if matches!(
        item.state.as_str(),
        "rollback_retrieve"
            | "rollback_snapshot"
            | "rollback_copy"
            | "rollback_restore"
            | "rollback_isolate"
            | "rollback_publish"
            | "rollback_commit"
            | "rollback_cleanup"
            | "rolled_back"
    ) && !update.rollback_started
    {
        return Err(invalid("回退状态与操作记录不一致"));
    }
    if matches!(
        item.state.as_str(),
        "rollback_isolate"
            | "rollback_publish"
            | "rollback_commit"
            | "rollback_cleanup"
            | "rolled_back"
    ) && (update.rollback_id.is_none() || !update.lightweight && update.ready_digest.is_none())
    {
        return Err(invalid("回退缺少版本身份校验"));
    }
    for save in update.saves.iter().chain(update.rollback_saves.iter()) {
        if let Some(relative) = &save.relative {
            let relative = paths::relative_path(relative)?;
            if paths::path_key(&Path::new(&item.target).join(relative))?.replace('\\', "/")
                != paths::path_key(Path::new(&save.source))?.replace('\\', "/")
            {
                return Err(invalid("更新存档路径越界"));
            }
        }
    }
    Ok(())
}
fn verify_updated_payload(payload: &Path, package: &Path, item: &Item, job: &Job) -> Result<()> {
    unchanged(package, &item.manifest, job)?;
    let saved: Vec<PathBuf> = item
        .update
        .as_ref()
        .unwrap()
        .saves
        .iter()
        .filter(|s| item.selection.preserve_saves && s.present)
        .filter_map(|s| s.relative.as_deref())
        .map(paths::relative_path)
        .collect::<Result<_>>()?;
    let is_saved = |value: &str| {
        let value = PathBuf::from(value.replace('\\', "/"));
        saved.iter().any(|save| value.starts_with(save))
    };
    let actual = inventory(payload, job)?;
    let planned: HashMap<_, _> = item
        .manifest
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let found: HashMap<_, _> = actual
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    for entry in &actual {
        if is_saved(&entry.path) {
            continue;
        }
        let ancestor = entry.directory
            && saved
                .iter()
                .any(|save| save.starts_with(PathBuf::from(entry.path.replace('\\', "/"))));
        if !ancestor && planned.get(entry.path.as_str()).copied() != Some(entry) {
            return Err(invalid("新版本暂存出现计划外内容，保留文件并停止"));
        }
    }
    for entry in item.manifest.iter().filter(|entry| !is_saved(&entry.path)) {
        if found.get(entry.path.as_str()).copied() != Some(entry) {
            return Err(invalid("新版本文件缺失或改变，未提交更新"));
        }
        if !entry.directory {
            equal_file(&package.join(&entry.path), &payload.join(&entry.path), job)?;
        }
    }
    Ok(())
}
fn selection_configuration(
    selection: &Selection,
    previous: Option<&VersionConfig>,
) -> Result<VersionConfig> {
    Ok(VersionConfig {
        version: selection.version.clone(),
        version_source: "manual".into(),
        engine: selection.engine.clone(),
        engine_source: "manual".into(),
        launch_source: "manual".into(),
        executable: (!selection.executable.is_empty()).then(|| selection.executable.clone()),
        working_directory: match previous {
            Some(old) => selection
                .working_directory
                .clone()
                .unwrap_or_else(|| old.working_directory.clone()),
            None if selection.external_player.is_some() || selection.mtool => ".".into(),
            None => paths::executable_directory(Some(&selection.executable))?,
        },
        launch_type: if selection.external_player.is_some() {
            "EXTERNAL_PLAYER"
        } else if selection.mtool {
            "MTOOL"
        } else {
            "DIRECT"
        }
        .into(),
        external_player: selection.external_player.clone(),
        mtool_target: selection.mtool.then(|| selection.executable.clone()),
        mtool_loader: if selection.mtool && selection.external_player.is_none() {
            previous.and_then(|old| match &selection.mtool_loader {
                Some(value) => (!value.is_empty()).then(|| value.clone()),
                None => old.mtool_loader.clone(),
            })
        } else {
            None
        },
    })
}
pub(super) fn configuration(item: &Item) -> Result<VersionConfig> {
    let previous = item
        .update
        .as_ref()
        .filter(|update| update.inherit_launch_config);
    let mut config = selection_configuration(&item.selection, previous.map(|update| &update.old))?;
    if let Some(update) = previous.filter(|update| !update.mtool_loader_launch_scoped) {
        // Preserve the configuration already published (or staged) by an older journal.
        config.mtool_loader = match &item.selection.mtool_loader {
            Some(value) => (!value.is_empty()).then(|| value.clone()),
            None => update.old.mtool_loader.clone(),
        };
    }
    Ok(config)
}

fn validate_update_launch(
    target: &Path,
    config: &VersionConfig,
    settings: &Settings,
) -> Result<()> {
    validate_launch(target, config).map_err(|error| {
        invalid(format!(
            "新版启动文件或工作目录不适用，请在更新启动配置中重新配置：{error}"
        ))
    })?;
    if config.launch_type == "MTOOL" {
        if let Some(loader) = &config.mtool_loader {
            let loader_file = paths::contained_file(Path::new(&settings.mtool_root), loader, "dll")
                .map_err(|error| {
                    invalid(format!(
                        "MTool loader 不适用，请在更新启动配置中重新选择：{error}"
                    ))
                })?;
            let executable =
                paths::contained_file(target, config.executable.as_deref().unwrap_or(""), "exe")?;
            let executable_arch = scanner::architecture(&executable);
            let loader_arch = scanner::architecture(&loader_file);
            if executable_arch != "Unknown"
                && loader_arch != "Unknown"
                && executable_arch != loader_arch
            {
                return Err(invalid(
                    "新版 EXE 与 MTool loader 位数不同，请重新选择 loader",
                ));
            }
        }
    }
    Ok(())
}
fn validate_launch(target: &Path, config: &VersionConfig) -> Result<()> {
    if let Some(player) = &config.external_player {
        crate::external_player::validate(target, config.executable.as_deref(), player)?;
    } else {
        paths::launch_file(
            target,
            config
                .executable
                .as_deref()
                .ok_or_else(|| invalid("未配置启动文件"))?,
        )?;
    }
    paths::working_directory(target, &config.working_directory)?;
    Ok(())
}

impl ImportStore {
    pub(super) fn recycled_versions(&self) -> Result<HashMap<String, PathBuf>> {
        #[cfg(test)]
        let paths = {
            let bin = self.directory.join("test-recycle-bin");
            if !exists(&bin)? {
                return Ok(HashMap::new());
            }
            fs::read_dir(bin)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?
        };
        #[cfg(all(windows, not(test)))]
        let paths = std::thread::spawn(crate::recycle_windows::recycled_paths)
            .join()
            .map_err(|_| invalid("读取回收站异常退出"))??;
        #[cfg(all(not(windows), not(test)))]
        let paths: Vec<PathBuf> = vec![];
        Ok(paths
            .into_iter()
            .filter_map(|path| identity(&path).ok().map(|id| (id, path)))
            .collect())
    }

    pub(super) fn recycled_version(&self, expected: &str) -> Result<Option<PathBuf>> {
        Ok(self.recycled_versions()?.remove(expected))
    }

    fn recycle_version(&self, path: &Path) -> Result<()> {
        #[cfg(test)]
        {
            let bin = self.directory.join("test-recycle-bin");
            fs::create_dir_all(&bin)?;
            rename_new(path, &bin.join(uuid::Uuid::new_v4().to_string()))
        }
        #[cfg(not(test))]
        {
            crate::deletion::recycle(path)
        }
    }

    fn retrieve_version(&self, expected: &str, target: &Path) -> Result<()> {
        let source = self
            .recycled_version(expected)?
            .ok_or_else(|| invalid("旧版本已不在回收站，无法回退（可能已清空或手动恢复）"))?;
        if exists(target)? {
            return Err(invalid("旧版本恢复位置已被占用"));
        }
        #[cfg(test)]
        {
            rename_new(&source, target)?;
        }
        #[cfg(all(windows, not(test)))]
        {
            let target = target.to_path_buf();
            std::thread::spawn(move || crate::recycle_windows::restore(&source, &target))
                .join()
                .map_err(|_| invalid("回收站恢复异常退出"))??;
        }
        #[cfg(all(not(windows), not(test)))]
        {
            return Err(invalid("回收站回退仅支持 Windows"));
        }
        require_identity(target, Some(expected))
    }

    /// Build deletion authority from known copies, never from arbitrary current children.
    fn owned_cleanup_entries(&self, plan: &Plan, index: usize, job: &Job) -> Result<Vec<Entry>> {
        let item = &plan.items[index];
        let update = item.update.as_ref().unwrap();
        let container = stage_path(plan, index);
        let current = inventory(&container, job)?;
        let mut allowed: HashMap<PathBuf, (bool, u64)> = HashMap::new();
        fn add(
            allowed: &mut HashMap<PathBuf, (bool, u64)>,
            root: &str,
            entries: &[Entry],
        ) -> Result<()> {
            allowed.insert(PathBuf::from(root), (true, 0));
            for entry in entries {
                let path = Path::new(root).join(paths::relative_path(&entry.path)?);
                for parent in path
                    .ancestors()
                    .skip(1)
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    allowed.entry(parent.to_path_buf()).or_insert((true, 0));
                }
                allowed.insert(path, (entry.directory, entry.bytes));
            }
            Ok(())
        }
        if !update.lightweight {
            add(&mut allowed, "package", &item.manifest)?;
            add(&mut allowed, "payload", &item.manifest)?;
        } else if item.cross_volume {
            add(&mut allowed, "payload", &item.manifest)?;
        }
        // A snapshot directory is authorized only after matching the recorded save fingerprint.
        for (root, saves) in [
            ("saves", &update.saves),
            ("bundled-saves", &update.incoming_saves),
            ("rollback-saves", &update.rollback_saves),
            ("rollback-bundled", &update.rollback_originals),
        ] {
            add(&mut allowed, root, &[])?;
            for (i, save) in saves.iter().enumerate().filter(|(_, save)| save.present) {
                let relative = format!("{root}/{i}");
                let tree = container.join(&relative);
                if !exists(&tree)? {
                    continue;
                }
                let entries = path_inventory(&tree, job)?;
                if digest(&entries)? != save.digest {
                    // Interrupted save copies can be withdrawn while their exact source survives.
                    if !matches!(root, "saves" | "rollback-saves") {
                        return Err(invalid("存档临时副本变化，保留并停止清理"));
                    }
                    let source = Path::new(&save.source);
                    require_digest(source, &save.digest, job)?;
                    if save.directory {
                        let expected = path_inventory(source, job)?;
                        let lookup: HashMap<_, _> = expected
                            .iter()
                            .map(|entry| (entry.path.as_str(), entry))
                            .collect();
                        for entry in &entries {
                            stopped(job)?;
                            if lookup.get(entry.path.as_str()).is_none_or(|original| {
                                original.directory != entry.directory
                                    || original.bytes < entry.bytes
                            }) {
                                return Err(invalid("存档暂存含未知文件，保留并停止清理"));
                            }
                            if !entry.directory {
                                equal_prefix(
                                    &source.join(&entry.path),
                                    &tree.join(&entry.path),
                                    job,
                                )?;
                            }
                        }
                    } else {
                        equal_prefix(source, &tree, job)?;
                    }
                }
                if save.directory {
                    add(&mut allowed, &relative, &entries)?;
                } else {
                    allowed.insert(
                        PathBuf::from(&relative),
                        (false, fs::metadata(&tree)?.len()),
                    );
                }
            }
        }
        if !update.lightweight {
            // Legacy archives correspond to the original incoming package's save subtrees.
            for (i, save) in update
                .saves
                .iter()
                .enumerate()
                .filter(|(_, save)| save.present)
            {
                let prefix = paths::relative_path(save.relative.as_deref().unwrap())?;
                let root = format!("bundled-saves/{i}");
                for entry in item.manifest.iter() {
                    let path = paths::relative_path(&entry.path)?;
                    if let Ok(suffix) = path.strip_prefix(&prefix) {
                        allowed.insert(
                            Path::new(&root).join(suffix),
                            (entry.directory, entry.bytes),
                        );
                    }
                }
            }
            let previous = if exists(&container.join("previous"))? {
                Some(container.join("previous"))
            } else {
                self.recycled_version(&update.old_id)?
            };
            if let Some(previous) = previous {
                check_old_version(&previous, update, job)?;
                let entries = inventory(&previous, job)?;
                if exists(&container.join("previous"))? {
                    add(&mut allowed, "previous", &entries)?;
                }
                if exists(&container.join("rollback-payload"))? {
                    add(&mut allowed, "rollback-payload", &entries)?;
                }
                add(&mut allowed, "rollback-bundled", &[])?;
                for (i, save) in update
                    .rollback_saves
                    .iter()
                    .enumerate()
                    .filter(|(_, save)| save.present)
                {
                    let prefix = paths::relative_path(save.relative.as_deref().unwrap())?;
                    let root = format!("rollback-bundled/{i}");
                    for entry in &entries {
                        let path = paths::relative_path(&entry.path)?;
                        if let Ok(suffix) = path.strip_prefix(&prefix) {
                            allowed.insert(
                                Path::new(&root).join(suffix),
                                (entry.directory, entry.bytes),
                            );
                        }
                    }
                }
            }
        }
        // A restored save can be larger than the new package's bundled save.
        // Replace that subtree's authority with the recorded save snapshot, not current contents.
        for (root, archives, snapshots, saves) in [
            ("payload", "bundled-saves", "saves", &update.saves),
            (
                "rollback-payload",
                "rollback-bundled",
                "rollback-saves",
                &update.rollback_saves,
            ),
        ] {
            if !exists(&container.join(root))?
                || root == "payload" && !item.selection.preserve_saves
            {
                continue;
            }
            for (i, save) in saves.iter().enumerate().filter(|(_, save)| save.present) {
                let prefix =
                    Path::new(root).join(paths::relative_path(save.relative.as_deref().unwrap())?);
                let overlay = container.join(&prefix);
                let archived = exists(&container.join(archives).join(i.to_string()))?;
                if !exists(&overlay)?
                    || !archived && allowed.keys().any(|path| path.starts_with(&prefix))
                {
                    continue;
                }
                let snapshot = container.join(snapshots).join(i.to_string());
                require_digest(&snapshot, &save.digest, job)?;
                let entries = path_inventory(&snapshot, job)?;
                let actual = path_inventory(&overlay, job)?;
                let expected: HashMap<_, _> = entries
                    .iter()
                    .map(|entry| (entry.path.as_str(), entry))
                    .collect();
                for entry in &actual {
                    if expected.get(entry.path.as_str()).is_none_or(|original| {
                        original.directory != entry.directory || original.bytes < entry.bytes
                    }) {
                        return Err(invalid("恢复的存档含未知文件，保留并停止清理"));
                    }
                    if !entry.directory {
                        if save.directory {
                            equal_prefix(
                                &snapshot.join(&entry.path),
                                &overlay.join(&entry.path),
                                job,
                            )?;
                        } else {
                            equal_prefix(&snapshot, &overlay, job)?;
                        }
                    }
                }
                allowed.retain(|path, _| !path.starts_with(&prefix));
                if save.directory {
                    add(&mut allowed, &paths::path_text(&prefix)?, &entries)?;
                } else {
                    allowed.insert(prefix, (false, save.bytes));
                }
            }
        }
        allowed.insert(PathBuf::from("owner"), (false, plan.id.len() as u64));
        for entry in &current {
            stopped(job)?;
            let path = paths::relative_path(&entry.path)?;
            if allowed.get(&path).is_none_or(|(directory, bytes)| {
                *directory != entry.directory || (!entry.directory && entry.bytes > *bytes)
            }) {
                return Err(invalid("暂存目录出现未记录的文件，保留并停止清理"));
            }
        }
        Ok(current)
    }

    /// Remove only owned temporary copies, with a resumable manifest outside the game.
    fn cleanup_stage(&self, plan: &mut Plan, index: usize, job: &Job) -> Result<()> {
        let container = stage_path(plan, index);
        require_identity(Path::new(&plan.root), Some(&plan.root_id))?;
        let manifest = self.directory.join(format!("{}-{index}.cleanup", plan.id));
        if exists(&container)? {
            if !exists(&container.join("owner"))? {
                if !plan.items[index].update.as_ref().unwrap().cleanup_ready
                    || fs::read_dir(&container)?.next().is_some()
                {
                    return Err(invalid("暂存目录归属不明确，停止清理"));
                }
                fs::remove_dir(&container)?;
            } else {
                verify_owner(&container, &plan.id)?;
                let expected: Vec<Entry> =
                    if plan.items[index].update.as_ref().unwrap().cleanup_ready {
                        let metadata = fs::symlink_metadata(&manifest)?;
                        if scanner::is_link(&metadata)
                            || !metadata.is_file()
                            || metadata.len() > 256 * 1024 * 1024
                        {
                            return Err(invalid("清理清单不能是链接"));
                        }
                        serde_json::from_reader(BufReader::with_capacity(
                            64 * 1024,
                            File::open(&manifest)?,
                        ))
                        .map_err(|e| invalid(e.to_string()))?
                    } else {
                        let entries = self.owned_cleanup_entries(plan, index, job)?;
                        let mut file = File::create(&manifest)?;
                        serde_json::to_writer(&mut file, &entries)
                            .map_err(|e| invalid(e.to_string()))?;
                        file.sync_all()?;
                        plan.items[index].update.as_mut().unwrap().cleanup_ready = true;
                        self.save(plan)?;
                        entries
                    };
                let mut lookup = HashMap::new();
                for entry in &expected {
                    paths::relative_path(&entry.path)?;
                    if lookup.insert(entry.path.as_str(), entry).is_some() {
                        return Err(invalid("清理清单存在重复路径"));
                    }
                }
                let current = inventory(&container, job)?;
                if current
                    .iter()
                    .any(|entry| lookup.get(entry.path.as_str()).copied() != Some(entry))
                {
                    return Err(invalid("暂存内容在清理时发生变化，保留并停止"));
                }
                crate::runtime::ensure_paths_idle(std::slice::from_ref(&container))?;
                for entry in current
                    .iter()
                    .filter(|entry| !entry.directory && entry.path != "owner")
                {
                    stopped(job)?;
                    let path = container.join(paths::relative_path(&entry.path)?);
                    let actual = path_inventory(&path, job)?;
                    if actual[0].bytes != entry.bytes || actual[0].modified != entry.modified {
                        return Err(invalid("暂存文件在清理时变化"));
                    }
                    fs::remove_file(path)?;
                }
                let mut dirs = current
                    .iter()
                    .filter(|entry| entry.directory)
                    .collect::<Vec<_>>();
                dirs.sort_by_key(|entry| {
                    std::cmp::Reverse(Path::new(&entry.path).components().count())
                });
                for entry in dirs {
                    stopped(job)?;
                    fs::remove_dir(container.join(paths::relative_path(&entry.path)?))?;
                }
                finish_container(&container, &plan.id)?;
            }
        }
        if exists(&manifest)? {
            fs::remove_file(manifest)?;
        }
        Ok(())
    }

    fn active_version_identity(&self, item: &Item, database: &Database) -> Result<String> {
        let actual = identity(Path::new(&item.target))?;
        let expected = item
            .payload_id
            .as_ref()
            .ok_or_else(|| invalid("缺少当前版本身份"))?;
        let mut reachable = HashSet::from([expected.clone()]);
        let plans = self.plans.lock().unwrap();
        loop {
            if reachable.contains(&actual) {
                return Ok(actual);
            }
            let count = reachable.len();
            for plan in plans.values() {
                for (index, candidate) in plan.items.iter().enumerate().filter(|(_, candidate)| {
                    candidate.game_id == item.game_id && candidate.state == "rolled_back"
                }) {
                    let Some(update) = &candidate.update else {
                        continue;
                    };
                    if reachable.contains(&update.old_id)
                        && database
                            .version_operation(&format!("{}:{index}", plan.id))?
                            .as_deref()
                            == Some("rolled_back")
                    {
                        if let Some(id) = &update.rollback_id {
                            reachable.insert(id.clone());
                        }
                    }
                }
            }
            if count == reachable.len() {
                return Err(invalid("当前游戏目录已被替换，不能按此历史回退"));
            }
        }
    }
    pub fn include_recovery_deletion(
        &self,
        id: &str,
        deletion: &mut crate::deletion::DeletePlan,
    ) -> Result<()> {
        for plan in self.plans.lock().unwrap().values() {
            for (index, item) in plan
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.game_id == id && item.update.is_some())
            {
                let container = stage_path(plan, index);
                if !exists(&container)? {
                    continue;
                }
                require_identity(Path::new(&plan.root), Some(&plan.root_id))?;
                verify_owner(&container, &plan.id)?;
                if !matches!(
                    item.state.as_str(),
                    "completed" | "withdrawn" | "rolled_back"
                ) {
                    return Err(invalid("请先恢复未完成的更新"));
                }
                let text = paths::path_text(&container)?;
                if !deletion.saves.iter().any(|save| save.path == text) {
                    deletion.saves.push(crate::deletion::SaveAction {
                        path: text,
                        action: "recycle".into(),
                    });
                }
            }
        }
        deletion.saves.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(())
    }
    pub(super) fn prepare_update(
        &self,
        game: &Game,
        selection: &Selection,
        settings: &Settings,
        games: &[Game],
        job: &Job,
        storage: (u64, &Path),
    ) -> Result<UpdateData> {
        if !selection.saves_confirmed {
            return Err(invalid("请确认已有游戏的存档范围和迁移方式"));
        }
        let old = crate::db::config_from_game(game);
        validate_update_launch(
            Path::new(&selection.source),
            &selection_configuration(selection, Some(&old))?,
            settings,
        )?;
        let (bytes, quarantine) = storage;
        let root = checked_dir(Path::new(&settings.game_root))?;
        let target = checked_dir(Path::new(&game.install_path))?;
        if target.parent() != Some(root.as_path()) {
            return Err(invalid("更新仅支持当前游戏库目录下的直属游戏"));
        }
        let mut local_game = game.clone();
        local_game.save_paths = game
            .save_paths
            .iter()
            .filter_map(|path| match internal_save_path(game, path) {
                Ok(Some(_)) => Some(Ok(path.clone())),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<_>>()?;
        let deletion_check = crate::deletion::preview(
            &local_game,
            games,
            settings,
            self.directory.parent().unwrap(),
            "update-check",
        );
        if !deletion_check.blockers.is_empty() {
            return Err(invalid(deletion_check.blockers.join("；")));
        }
        let saves = save_snapshots(game, job)?;
        for configured in &game.save_paths {
            let value = configured
                .trim()
                .replace("<GAME>", &game.install_path)
                .replace('\\', "/");
            let path = Path::new(&value);
            if path.is_absolute() && overlaps(Path::new(&selection.source), path)? {
                return Err(invalid(
                    "导入来源与已配置存档重叠，不能清理来源；请另选新版本目录",
                ));
            }
        }
        for save in &saves {
            if overlaps(Path::new(&selection.source), Path::new(&save.source))? {
                return Err(invalid("来源目录与已有存档范围重叠，不能更新或清理来源"));
            }
        }
        let save_bytes: u64 = saves
            .iter()
            .filter(|s| selection.preserve_saves && s.present)
            .map(|s| s.bytes)
            .sum();
        let incoming_saves = local_originals(Path::new(&selection.source), &saves, job)?;
        Ok(UpdateData {
            old_version: game.current_version.clone(),
            saves,
            required_bytes: (if volume(Path::new(&selection.source))? != volume(&root)? {
                bytes
            } else {
                0
            })
            .saturating_add(save_bytes.saturating_mul(2)),
            quarantine: paths::path_text(quarantine)?,
            rollback_available: false,
            old,
            old_id: identity(&target)?,
            old_digest: String::new(),
            old_updated_at: game.updated_at.clone(),
            old_save_paths: game.save_paths.clone(),
            ready_digest: None,
            rollback_id: None,
            rollback_saves: vec![],
            rollback_started: false,
            cleanup_ready: false,
            lightweight: true,
            inherit_launch_config: true,
            mtool_loader_launch_scoped: true,
            incoming_saves,
            rollback_originals: vec![],
        })
    }
    fn check_update_record(
        &self,
        plan: &Plan,
        index: usize,
        db: &Database,
        initial: bool,
    ) -> Result<()> {
        let item = &plan.items[index];
        let update = item
            .update
            .as_ref()
            .ok_or_else(|| invalid("缺少更新记录"))?;
        let game = db.game(&item.game_id)?;
        if paths::path_key(Path::new(&game.install_path))?
            != paths::path_key(Path::new(&item.target))?
            || game.save_paths != update.old_save_paths
            || initial && game.updated_at != update.old_updated_at
        {
            return Err(invalid("游戏资料在计划确认后已改变，请重新生成计划"));
        }
        Ok(())
    }
    pub(super) fn update_one(
        &self,
        plan: &mut Plan,
        index: usize,
        job: &Job,
        database: &Arc<Mutex<Database>>,
    ) -> Result<()> {
        if plan.items[index].state == "pending" {
            let actual = database
                .lock()
                .unwrap()
                .version_config(&plan.items[index].game_id)?;
            let expected = &mut plan.items[index].update.as_mut().unwrap().old;
            let mut compared = actual.clone();
            compared.engine_source = expected.engine_source.clone();
            compared.launch_source = expected.launch_source.clone();
            if compared != *expected {
                return Err(invalid("已有游戏配置已改变，请重新生成计划"));
            }
            *expected = actual;
            // A pending journal has not changed files or committed its new configuration yet.
            // It can safely adopt inheritance; in-flight/completed legacy journals cannot.
            plan.items[index]
                .update
                .as_mut()
                .unwrap()
                .inherit_launch_config = true;
            plan.items[index]
                .update
                .as_mut()
                .unwrap()
                .mtool_loader_launch_scoped = true;
            if plan.items[index].update.as_ref().unwrap().lightweight {
                let originals = local_originals(
                    Path::new(&plan.items[index].selection.source),
                    &plan.items[index].update.as_ref().unwrap().saves,
                    job,
                )?;
                plan.items[index].update.as_mut().unwrap().incoming_saves = originals;
            }
            self.save(plan)?;
        }
        if matches!(
            plan.items[index].state.as_str(),
            "update_returning" | "update_returned"
        ) {
            self.restore_uncommitted(plan, index, database)?;
            let payload = stage_path(plan, index).join("payload");
            plan.items[index].payload_id = Some(identity(&payload)?);
            plan.items[index].update.as_mut().unwrap().ready_digest =
                version_digest(&payload, plan.items[index].update.as_ref().unwrap(), job)?;
            self.checkpoint(plan, index, "update_ready")?;
        }
        let item = plan.items[index].clone();
        let update = item
            .update
            .as_ref()
            .ok_or_else(|| invalid("缺少更新计划"))?;
        let source = Path::new(&item.selection.source);
        let target = Path::new(&item.target);
        let container = stage_path(plan, index);
        let package = container.join("package");
        let payload = container.join("payload");
        let previous = container.join("previous");
        if update.rollback_started {
            return self.rollback_one(plan, index, job, database);
        }
        if matches!(item.state.as_str(), "update_recycle" | "update_finish") {
            // Source cleanup already succeeded; its parent may legitimately be gone.
            require_identity(Path::new(&plan.root), Some(&plan.root_id))?;
            require_identity(target, item.payload_id.as_deref())?;
            validate_record(&plan.id, &plan.root, index, &item)?;
        } else {
            validate_paths(plan, &item, &self.directory)?;
        }
        let operation = format!("{}:{index}", plan.id);
        if database
            .lock()
            .unwrap()
            .version_operation(&operation)?
            .as_deref()
            == Some("committed")
            && item.state == "update_commit"
        {
            require_identity(target, item.payload_id.as_deref())?;
            plan.items[index].registered_id = Some(item.game_id.clone());
            self.checkpoint(plan, index, "update_cleanup")?;
        }
        if !matches!(
            plan.items[index].state.as_str(),
            "update_cleanup" | "update_recycle" | "update_finish"
        ) {
            self.check_update_record(
                plan,
                index,
                &database.lock().unwrap(),
                item.state == "pending",
            )?;
            if database.lock().unwrap().version_config(&item.game_id)? != update.old {
                return Err(invalid("已有版本的启动配置已变化"));
            }
        }
        check_item_idle(
            &[
                source.to_path_buf(),
                target.to_path_buf(),
                container.clone(),
            ],
            job,
        )?;
        if item.state == "pending" {
            validate_update_launch(source, &configuration(&item)?, &plan.settings)?;
            if !update.lightweight {
                unchanged(source, &item.manifest, job)?;
            }
            require_identity(target, Some(&update.old_id))?;
            check_old_version(target, update, job)?;
            if let Some(available) = free_space(Path::new(&plan.root))? {
                if available < update.required_bytes {
                    return Err(invalid("更新暂存与存档快照所需空间不足"));
                }
            }
            if !exists(&container)? {
                fs::create_dir(&container)?;
            }
            if !exists(&container.join("owner"))? {
                if fs::read_dir(&container)?.next().is_some() {
                    return Err(invalid("更新暂存目录包含未知文件"));
                }
                let mut marker = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(container.join("owner"))?;
                marker.write_all(plan.id.as_bytes())?;
                marker.sync_all()?;
            }
            verify_owner(&container, &plan.id)?;
            self.checkpoint(plan, index, "update_snapshot")?;
        }
        if plan.items[index].state != "update_finish" {
            verify_owner(&container, &plan.id)?;
        }
        if plan.items[index].state == "update_snapshot" {
            job.copy_phase("保留内部存档", 0.0, 0.05);
            job.progress("更新 · 存档快照", target);
            require_identity(target, Some(&update.old_id))?;
            check_old_version(target, update, job)?;
            if item.selection.preserve_saves {
                snapshot_saves(&update.saves, &container.join("saves"), job)?;
            }
            self.checkpoint(plan, index, "update_copy")?;
        }
        if plan.items[index].state == "update_copy" {
            job.copy_phase("准备新版本", 0.05, 0.8);
            job.progress("更新 · 准备新版本", source);
            if update.lightweight {
                if item.cross_volume {
                    copy_tree_basic(source, &payload, &item.manifest, job)?;
                    verify_sizes(&payload, &item.manifest, job)?;
                } else {
                    job.transfer_phase("移动新版本", 0.8);
                    if exists(source)? && !exists(&payload)? {
                        require_identity(source, Some(&item.source_id))?;
                        rename_new(source, &payload)?;
                    }
                    require_identity(&payload, Some(&item.source_id))?;
                    if exists(source)? {
                        return Err(invalid("来源目录重新出现，保留并停止"));
                    }
                }
            } else {
                if !update.lightweight {
                    unchanged(source, &item.manifest, job)?;
                }
                copy_tree(source, &package, &item.manifest, job)?;
                if !update.lightweight {
                    unchanged(source, &item.manifest, job)?;
                }
                if !update.lightweight {
                    verify_copy(source, &package, &item.manifest, job)?;
                }
                copy_tree(&package, &payload, &item.manifest, job)?;
                verify_copy(&package, &payload, &item.manifest, job)?;
            }
            validate_update_launch(&payload, &configuration(&item)?, &plan.settings)?;
            plan.items[index].payload_id = Some(identity(&payload)?);
            plan.items[index].update.as_mut().unwrap().ready_digest =
                version_digest(&payload, plan.items[index].update.as_ref().unwrap(), job)?;
            self.checkpoint(plan, index, "update_ready")?;
        }
        if plan.items[index].state == "update_ready" {
            stopped(job)?;
            if !update.lightweight {
                unchanged(source, &item.manifest, job)?;
            }
            require_identity(target, Some(&update.old_id))?;
            check_old_version(target, update, job)?;
            if !update.lightweight {
                verify_updated_payload(&payload, &package, &item, job)?;
            }
            check_reviewed_saves(update, item.selection.preserve_saves, job)?;
            self.checkpoint(plan, index, "update_isolate")?;
        }
        if plan.items[index].state == "update_isolate" {
            job.transfer_phase("切换游戏目录", 0.82);
            check_stage_idle(
                &[target.to_path_buf(), previous.clone()],
                update.lightweight,
            )?;
            if exists(target)? && !exists(&previous)? {
                require_identity(target, Some(&update.old_id))?;
                check_old_version(target, update, job)?;
                rename_new(target, &previous)?;
            }
            require_identity(&previous, Some(&update.old_id))?;
            if exists(target)? {
                return Err(invalid("隔离旧版本后目标被占用"));
            }
            self.checkpoint(plan, index, "update_publish")?;
        }
        if plan.items[index].state == "update_publish" {
            stopped(job)?;
            check_old_version(&previous, update, job)?;
            check_stage_idle(&[payload.clone(), target.to_path_buf()], update.lightweight)?;
            if exists(&payload)? && !exists(target)? {
                rename_new(&payload, target)?;
            }
            if exists(&payload)? {
                return Err(invalid("新版本发布发生目标冲突"));
            }
            require_identity(target, plan.items[index].payload_id.as_deref())?;
            if !update.lightweight {
                require_digest(
                    target,
                    plan.items[index]
                        .update
                        .as_ref()
                        .unwrap()
                        .ready_digest
                        .as_deref()
                        .ok_or_else(|| invalid("缺少新版本校验"))?,
                    job,
                )?;
            }
            self.checkpoint(plan, index, "update_restore")?;
        }
        if plan.items[index].state == "update_restore" {
            job.copy_phase("恢复内部存档", 0.85, 0.92);
            job.progress("更新 · 恢复存档", target);
            require_identity(target, plan.items[index].payload_id.as_deref())?;
            if item.selection.preserve_saves {
                restore_saves(
                    &update.saves,
                    &container.join("saves"),
                    target,
                    &container.join("bundled-saves"),
                    update
                        .lightweight
                        .then_some(update.incoming_saves.as_slice()),
                    job,
                )?;
            }
            validate_update_launch(target, &configuration(&item)?, &plan.settings)?;
            plan.items[index].update.as_mut().unwrap().ready_digest =
                version_digest(target, update, job)?;
            self.checkpoint(plan, index, "update_commit")?;
        }
        if plan.items[index].state == "update_commit" {
            job.transfer_phase("保存游戏资料", 0.93);
            stopped(job)?;
            check_old_version(&previous, update, job)?;
            check_saved_paths(&update.saves, &previous, item.selection.preserve_saves, job)?;
            if !update.lightweight {
                verify_updated_payload(target, &package, &item, job)?;
            }
            if !update.lightweight {
                unchanged(source, &item.manifest, job)?;
            }
            if !update.lightweight {
                verify_copy(source, &package, &item.manifest, job)?;
            }
            require_identity(target, plan.items[index].payload_id.as_deref())?;
            if !update.lightweight {
                require_digest(
                    target,
                    plan.items[index]
                        .update
                        .as_ref()
                        .unwrap()
                        .ready_digest
                        .as_deref()
                        .unwrap(),
                    job,
                )?;
            }
            check_stage_idle(
                &[target.to_path_buf(), previous.clone()],
                update.lightweight,
            )?;
            validate_update_launch(target, &configuration(&item)?, &plan.settings)?;
            database.lock().unwrap().commit_version(
                &item.game_id,
                &operation,
                &update.old,
                &configuration(&item)?,
            )?;
            plan.items[index].registered_id = Some(item.game_id.clone());
            self.checkpoint(plan, index, "update_cleanup")?;
        }
        if plan.items[index].state == "update_cleanup" {
            job.transfer_phase("清理跨盘来源", 0.95);
            require_identity(target, plan.items[index].payload_id.as_deref())?;
            require_identity(&previous, Some(&update.old_id))?;
            if update.lightweight {
                if item.cross_volume {
                    remove_source_basic(source, target, &container, &item, job)?;
                } else if exists(source)? {
                    return Err(invalid("来源路径被重新占用，不能清理"));
                }
            } else {
                crate::runtime::ensure_paths_idle(&[source.to_path_buf(), package.clone()])?;
                remove_source(source, &package, &item.manifest, job)?;
            }
            self.checkpoint(plan, index, "update_recycle")?;
        }
        if plan.items[index].state == "update_recycle" {
            job.transfer_phase("旧版本移入回收站", 0.98);
            job.progress("更新 · 旧版本移入回收站", &previous);
            require_identity(target, plan.items[index].payload_id.as_deref())?;
            if exists(&previous)? {
                require_identity(&previous, Some(&update.old_id))?;
                check_old_version(&previous, update, job)?;
                check_stage_idle(std::slice::from_ref(&previous), update.lightweight)?;
                self.recycle_version(&previous)?;
            }
            self.checkpoint(plan, index, "update_finish")?;
        }
        if plan.items[index].state == "update_finish" {
            job.transfer_phase("完成更新", 0.99);
            job.progress("更新 · 清理临时文件", &container);
            if exists(&previous)? {
                return Err(invalid("旧版本尚未移入回收站"));
            }
            self.cleanup_stage(plan, index, job)?;
            plan.items[index]
                .update
                .as_mut()
                .unwrap()
                .rollback_available = self.recycled_version(&update.old_id)?.is_some();
            self.checkpoint(plan, index, "completed")?;
        }
        Ok(())
    }
    /// Recovery after a failed uncommitted switch keeps both versions, restoring the original path.
    pub(super) fn restore_uncommitted(
        &self,
        plan: &mut Plan,
        index: usize,
        database: &Arc<Mutex<Database>>,
    ) -> Result<()> {
        let item = plan.items[index].clone();
        let update = item
            .update
            .as_ref()
            .ok_or_else(|| invalid("缺少更新记录"))?;
        if update.rollback_started
            || database
                .lock()
                .unwrap()
                .version_operation(&format!("{}:{index}", plan.id))?
                .is_some()
        {
            return Ok(());
        }
        let container = stage_path(plan, index);
        let previous = container.join("previous");
        let target = Path::new(&item.target);
        let payload = container.join("payload");
        if !exists(&previous)? {
            return Ok(());
        }
        verify_owner(&container, &plan.id)?;
        require_identity(&previous, Some(&update.old_id))?;
        check_stage_idle(
            &[previous.clone(), target.to_path_buf(), payload.clone()],
            update.lightweight,
        )?;
        self.checkpoint(plan, index, "update_returning")?;
        if exists(target)? {
            require_identity(target, item.payload_id.as_deref())?;
            rename_new(target, &payload)?;
        }
        rename_new(&previous, target)?;
        // Keep a checkpoint before re-validation: if the process dies here, resume detects the old identity.
        self.checkpoint(plan, index, "update_returned")
    }
    pub(super) fn withdraw_update(
        &self,
        plan: &mut Plan,
        index: usize,
        database: &Arc<Mutex<Database>>,
    ) -> Result<()> {
        if plan.items[index].update.as_ref().unwrap().rollback_started {
            let item = plan.items[index].clone();
            let container = stage_path(plan, index);
            let target = Path::new(&item.target);
            if !matches!(
                item.state.as_str(),
                "rollback_retrieve"
                    | "rollback_snapshot"
                    | "rollback_copy"
                    | "rollback_restore"
                    | "rollback_isolate"
            ) || exists(&container.join("replaced"))?
            {
                return Err(invalid("回退已切换版本，请选择继续未完成项"));
            }
            require_identity(target, item.payload_id.as_deref())?;
            let update = item.update.as_ref().unwrap();
            let previous = container.join("previous");
            let recovery = Job::recovery(plan.root.clone());
            if exists(&previous)? {
                require_identity(&previous, Some(&update.old_id))?;
                check_old_version(&previous, update, &recovery)?;
                if update.lightweight {
                    undo_save_overlay(
                        &update.rollback_saves,
                        &update.rollback_originals,
                        &container.join("rollback-saves"),
                        &previous,
                        &container.join("rollback-bundled"),
                        &recovery,
                    )?;
                }
                self.recycle_version(&previous)?;
            }
            self.cleanup_stage(plan, index, &recovery)?;
            let update = plan.items[index].update.as_mut().unwrap();
            update.rollback_started = false;
            update.rollback_available = self.recycled_version(&update.old_id)?.is_some();
            return self.checkpoint(plan, index, "completed");
        }
        if database
            .lock()
            .unwrap()
            .version_operation(&format!("{}:{index}", plan.id))?
            .is_some()
        {
            // A committed replacement still must recycle the old version and clean staging.
            let recovery = Job::recovery(plan.root.clone());
            return self.update_one(plan, index, &recovery, database);
        }
        self.restore_uncommitted(plan, index, database)?;
        // Source and original game are intact. Remove only verified manager-owned copies.
        let item = &plan.items[index];
        require_identity(
            Path::new(&item.target),
            Some(&item.update.as_ref().unwrap().old_id),
        )?;
        let recovery = Job::recovery(plan.root.clone());
        if item.update.as_ref().unwrap().lightweight && !item.cross_volume {
            let payload = stage_path(plan, index).join("payload");
            if exists(&payload)? {
                require_identity(&payload, Some(&item.source_id))?;
                let update = item.update.as_ref().unwrap();
                if item.selection.preserve_saves {
                    undo_save_overlay(
                        &update.saves,
                        &update.incoming_saves,
                        &stage_path(plan, index).join("saves"),
                        &payload,
                        &stage_path(plan, index).join("bundled-saves"),
                        &recovery,
                    )?;
                }
                rename_new(&payload, Path::new(&item.selection.source))?;
            }
        }
        self.cleanup_stage(plan, index, &recovery)?;
        self.checkpoint(plan, index, "withdrawn")
    }
    pub fn rollback(
        &self,
        id: &str,
        index: usize,
        job: &Job,
        database: &Arc<Mutex<Database>>,
    ) -> Result<Vec<String>> {
        self.ensure_recovery_clear()?;
        let mut plan = self.get(id)?;
        let item = plan
            .items
            .get(index)
            .ok_or_else(|| invalid("更新记录不存在"))?;
        if item.update.is_none() || item.state != "completed" {
            return Err(invalid("只能回退已完成的更新"));
        }
        if self.has_pending_files() {
            return Err(invalid("请先恢复未完成的操作"));
        }
        let game = database.lock().unwrap().game(&item.game_id)?;
        database
            .lock()
            .unwrap()
            .ensure_latest_version(&item.game_id, &format!("{}:{index}", plan.id))?;
        if database.lock().unwrap().version_config(&item.game_id)? != configuration(item)? {
            return Err(invalid("当前启动配置变化，请先恢复更新后的配置再回退"));
        }
        if paths::path_key(Path::new(&game.install_path))?
            != paths::path_key(Path::new(&item.target))?
        {
            return Err(invalid("游戏目录已重新关联，不能使用原路径回退"));
        }
        let active_id = self.active_version_identity(item, &database.lock().unwrap())?;
        let update = item.update.as_ref().unwrap();
        // Completed operations never use a retained directory as the rollback source.
        let previous = self
            .recycled_version(&update.old_id)?
            .ok_or_else(|| invalid("旧版本已不在回收站，无法回退（可能已清空或手动恢复）"))?;
        check_old_version(&previous, update, job)?;
        let saves = save_snapshots(&game, job)?;
        let previous_bytes: u64 = if update.lightweight {
            0
        } else {
            inventory(&previous, job)?.iter().map(|e| e.bytes).sum()
        };
        let originals = if update.lightweight {
            local_originals(&previous, &saves, job)?
        } else {
            vec![]
        };
        plan.items[index]
            .update
            .as_mut()
            .unwrap()
            .rollback_originals = originals;
        let needed = previous_bytes
            .saturating_add(saves.iter().map(|s| s.bytes.saturating_mul(2)).sum::<u64>());
        if free_space(Path::new(&plan.root))?.is_some_and(|available| available < needed) {
            return Err(invalid("回退暂存空间不足"));
        }
        plan.items[index].update.as_mut().unwrap().rollback_saves = saves;
        plan.items[index].payload_id = Some(active_id);
        plan.items[index].update.as_mut().unwrap().rollback_started = true;
        plan.items[index].update.as_mut().unwrap().cleanup_ready = false;
        plan.status = "running".into();
        self.checkpoint(&mut plan, index, "rollback_retrieve")?;
        match self.rollback_one(&mut plan, index, job, database) {
            Ok(()) => {
                plan.status = "completed".into();
                self.save(&plan)?;
                Ok(vec![game.id])
            }
            Err(error) => {
                plan.status = "failed".into();
                plan.items[index].error = Some(error.to_string());
                self.save(&plan)?;
                Err(error)
            }
        }
    }
    fn rollback_one(
        &self,
        plan: &mut Plan,
        index: usize,
        job: &Job,
        database: &Arc<Mutex<Database>>,
    ) -> Result<()> {
        let item = plan.items[index].clone();
        let update = item.update.as_ref().unwrap();
        let operation = format!("{}:{index}", plan.id);
        let target = Path::new(&item.target);
        let container = stage_path(plan, index);
        let previous = container.join("previous");
        let payload = if update.lightweight {
            previous.clone()
        } else {
            container.join("rollback-payload")
        };
        let replaced = container.join("replaced");
        require_identity(Path::new(&plan.root), Some(&plan.root_id))?;
        let committed = database
            .lock()
            .unwrap()
            .version_operation(&operation)?
            .as_deref()
            == Some("rolled_back");
        if committed {
            require_identity(target, update.rollback_id.as_deref())?;
            if plan.items[index].state != "rollback_cleanup" {
                self.checkpoint(plan, index, "rollback_cleanup")?;
            }
        } else {
            database
                .lock()
                .unwrap()
                .ensure_latest_version(&item.game_id, &operation)?;
        }
        crate::runtime::ensure_paths_idle(&[target.to_path_buf(), container.clone()])?;
        if plan.items[index].state == "rollback_retrieve" {
            require_identity(target, item.payload_id.as_deref())?;
            if !exists(&container)? {
                fs::create_dir(&container)?;
            }
            if !exists(&container.join("owner"))? {
                if fs::read_dir(&container)?.next().is_some() {
                    return Err(invalid("回退暂存目录含未知内容"));
                }
                let mut marker = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(container.join("owner"))?;
                marker.write_all(plan.id.as_bytes())?;
                marker.sync_all()?;
            }
            verify_owner(&container, &plan.id)?;
            if !exists(&previous)? {
                self.retrieve_version(&update.old_id, &previous)?;
            }
            require_identity(&previous, Some(&update.old_id))?;
            check_old_version(&previous, update, job)?;
            self.checkpoint(plan, index, "rollback_snapshot")?;
        }
        if plan.items[index].state != "rollback_cleanup" {
            verify_owner(&container, &plan.id)?;
            if !update.lightweight
                || !matches!(
                    plan.items[index].state.as_str(),
                    "rollback_commit" | "rollback_publish"
                )
                || exists(&previous)?
            {
                require_identity(&previous, Some(&update.old_id))?;
            }
        }
        if plan.items[index].state == "rollback_snapshot" {
            require_identity(target, item.payload_id.as_deref())?;
            snapshot_saves(
                &update.rollback_saves,
                &container.join("rollback-saves"),
                job,
            )?;
            self.checkpoint(plan, index, "rollback_copy")?;
        }
        if plan.items[index].state == "rollback_copy" {
            check_old_version(&previous, update, job)?;
            if !update.lightweight {
                let manifest = inventory(&previous, job)?;
                copy_tree(&previous, &payload, &manifest, job)?;
                verify_copy(&previous, &payload, &manifest, job)?;
            }
            self.checkpoint(plan, index, "rollback_restore")?;
        }
        if plan.items[index].state == "rollback_restore" {
            restore_saves(
                &update.rollback_saves,
                &container.join("rollback-saves"),
                &payload,
                &container.join("rollback-bundled"),
                update
                    .lightweight
                    .then_some(update.rollback_originals.as_slice()),
                job,
            )?;
            validate_launch(&payload, &update.old)?;
            plan.items[index].update.as_mut().unwrap().rollback_id = Some(identity(&payload)?);
            plan.items[index].update.as_mut().unwrap().ready_digest =
                version_digest(&payload, plan.items[index].update.as_ref().unwrap(), job)?;
            self.checkpoint(plan, index, "rollback_isolate")?;
        }
        if plan.items[index].state == "rollback_isolate" {
            stopped(job)?;
            crate::runtime::ensure_paths_idle(&[
                target.to_path_buf(),
                payload.clone(),
                replaced.clone(),
            ])?;
            if exists(target)? && !exists(&replaced)? {
                require_identity(target, item.payload_id.as_deref())?;
                // Current saves must still match the snapshot reviewed for this rollback.
                for save in &update.rollback_saves {
                    if save.present {
                        require_digest(Path::new(&save.source), &save.digest, job)?;
                    }
                }
                rename_new(target, &replaced)?;
            }
            require_identity(&replaced, item.payload_id.as_deref())?;
            if exists(target)? {
                return Err(invalid("回退目标被其他目录占用"));
            }
            self.checkpoint(plan, index, "rollback_publish")?;
        }
        if plan.items[index].state == "rollback_publish" {
            // Finish the short switch even if cancellation arrived after isolation.
            crate::runtime::ensure_paths_idle(&[target.to_path_buf(), payload.clone()])?;
            if exists(&payload)? && !exists(target)? {
                rename_new(&payload, target)?;
            }
            if exists(&payload)? {
                return Err(invalid("回退目标冲突"));
            }
            require_identity(
                target,
                plan.items[index]
                    .update
                    .as_ref()
                    .unwrap()
                    .rollback_id
                    .as_deref(),
            )?;
            let recovery = Job::recovery(plan.root.clone());
            if !update.lightweight {
                require_digest(
                    target,
                    plan.items[index]
                        .update
                        .as_ref()
                        .unwrap()
                        .ready_digest
                        .as_deref()
                        .ok_or_else(|| invalid("缺少回退文件校验"))?,
                    &recovery,
                )?;
            }
            self.checkpoint(plan, index, "rollback_commit")?;
        }
        if plan.items[index].state == "rollback_commit" {
            require_identity(
                target,
                plan.items[index]
                    .update
                    .as_ref()
                    .unwrap()
                    .rollback_id
                    .as_deref(),
            )?;
            let recovery = Job::recovery(plan.root.clone());
            if !update.lightweight {
                require_digest(
                    target,
                    plan.items[index]
                        .update
                        .as_ref()
                        .unwrap()
                        .ready_digest
                        .as_deref()
                        .ok_or_else(|| invalid("缺少回退文件校验"))?,
                    &recovery,
                )?;
            }
            crate::runtime::ensure_paths_idle(&[target.to_path_buf(), replaced.clone()])?;
            // Commit is not cancellable; the old config and history move together.
            validate_launch(target, &update.old)?;
            database.lock().unwrap().rollback_version(
                &item.game_id,
                &operation,
                &configuration(&item)?,
                &update.old,
            )?;
            plan.items[index]
                .update
                .as_mut()
                .unwrap()
                .rollback_available = false;
            self.checkpoint(plan, index, "rollback_cleanup")?;
        }
        if plan.items[index].state == "rollback_cleanup" {
            require_identity(
                target,
                plan.items[index]
                    .update
                    .as_ref()
                    .unwrap()
                    .rollback_id
                    .as_deref(),
            )?;
            if exists(&replaced)? {
                require_identity(&replaced, item.payload_id.as_deref())?;
                job.progress("回退 · 当前版本移入回收站", &replaced);
                self.recycle_version(&replaced)?;
            }
            self.cleanup_stage(plan, index, job)?;
            plan.items[index]
                .update
                .as_mut()
                .unwrap()
                .rollback_available = false;
            self.checkpoint(plan, index, "rolled_back")?;
        }
        Ok(())
    }
}
