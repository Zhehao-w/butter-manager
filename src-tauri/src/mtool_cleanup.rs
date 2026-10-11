//! Import-only, bounded MTool recognition and journaled recycling. Never runs a BAT.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_ENTRIES: usize = 4096;
const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MtoolBundle {
    pub folder: String,
    pub bats: Vec<String>,
    pub safe: bool,
    pub reason: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    directory: bool,
    bytes: u64,
    modified: u128,
    identity: String,
    // Only small launch/manifest files need content fingerprints; resource trees use identity + metadata.
    digest: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct CleanupTarget {
    path: String,
    state: String,
    error: Option<String>,
    snapshot: BTreeMap<String, Stamp>,
    bound: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CleanupJournal {
    bundles: Vec<MtoolBundle>,
    targets: Vec<CleanupTarget>,
}
#[derive(Clone, Serialize)]
pub struct CleanupView {
    pub bundles: Vec<MtoolBundle>,
    pub targets: Vec<CleanupTargetView>,
}
#[derive(Clone, Serialize)]
pub struct CleanupTargetView {
    path: String,
    state: String,
    error: Option<String>,
}
impl CleanupJournal {
    pub fn pending(&self) -> bool {
        self.targets.iter().any(|t| t.state != "recycled")
    }
    pub fn view(&self) -> CleanupView {
        CleanupView {
            bundles: self.bundles.clone(),
            targets: self
                .targets
                .iter()
                .map(|t| CleanupTargetView {
                    path: t.path.clone(),
                    state: t.state.clone(),
                    error: t.error.clone(),
                })
                .collect(),
        }
    }
}

fn children(path: &Path) -> Result<Vec<PathBuf>> {
    checked_dir(path)?;
    let mut found = vec![];
    for entry in fs::read_dir(path)? {
        if found.len() >= MAX_ENTRIES {
            return Err(invalid("工具目录超过检查上限，保留文件"));
        }
        found.push(entry?.path());
    }
    found.sort();
    Ok(found)
}
fn plain(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if scanner::is_link(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
        return Err(invalid("含链接或特殊文件，保留工具目录"));
    }
    Ok(metadata)
}
fn fingerprint(path: &Path, stop: &dyn Fn() -> bool) -> Result<String> {
    if plain(path)?.len() > MAX_BYTES {
        return Err(invalid("工具文件超过检查上限"));
    }
    let mut reader = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut bytes = 0;
    loop {
        if stop() {
            return Err(invalid("已停止工具检查"));
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > MAX_BYTES {
            return Err(invalid("工具文件超过检查上限"));
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn snapshot(
    path: &Path,
    record_identity: bool,
    stop: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Stamp>> {
    let mut queue = vec![path.to_path_buf()];
    let mut result = BTreeMap::new();
    let mut bytes: u64 = 0;
    while let Some(current) = queue.pop() {
        if stop() {
            return Err(invalid("已停止工具检查"));
        }
        let metadata = plain(&current)?;
        let relative = current
            .strip_prefix(path)
            .map_err(|_| invalid("工具路径越界"))?;
        let name = paths::path_text(relative)?.replace('\\', "/");
        let small = current
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bat"))
            || current
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("package.json"));
        let digest = if record_identity && metadata.is_file() && small {
            Some(fingerprint(&current, stop)?)
        } else {
            None
        };
        bytes = bytes.saturating_add(if metadata.is_file() {
            metadata.len()
        } else {
            0
        });
        if result.len() >= MAX_ENTRIES || bytes > MAX_BYTES {
            return Err(invalid("工具内容超过检查上限，保留文件"));
        }
        result.insert(
            name,
            Stamp {
                directory: metadata.is_dir(),
                bytes: if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                modified: if metadata.is_file() {
                    metadata
                        .modified()?
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| invalid("工具文件时间无效"))?
                        .as_nanos()
                } else {
                    0
                },
                identity: if record_identity {
                    identity(&current)?
                } else {
                    String::new()
                },
                digest,
            },
        );
        if metadata.is_dir() {
            queue.extend(children(&current)?);
        }
    }
    Ok(result)
}
fn same_content(a: &BTreeMap<String, Stamp>, b: &BTreeMap<String, Stamp>, ids: bool) -> bool {
    a.len() == b.len()
        && a.iter().all(|(path, stamp)| {
            b.get(path).is_some_and(|other| {
                stamp.directory == other.directory
                    && stamp.bytes == other.bytes
                    && stamp.modified == other.modified
                    && stamp.digest == other.digest
                    && (!ids || stamp.identity == other.identity)
            })
        })
}
fn named(path: &Path, name: &str) -> Option<PathBuf> {
    // Windows lookup is case-insensitive; avoid enumerating entire resource folders for each probe.
    let path = path.join(name);
    plain(&path).is_ok().then_some(path)
}
fn tool_signature(root: &Path) -> bool {
    let Some(package) = named(root, "package.json") else {
        return false;
    };
    if plain(&package).is_err() || fs::metadata(&package).map_or(true, |m| m.len() > 64 * 1024) {
        return false;
    }
    let value = fs::read(&package)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    value
        .as_ref()
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == "mtool" || name == "mtoolclient" || name.starts_with("mtoolclient_")
        })
        && named(root, "loaders")
            .and_then(|p| named(&p, "inject.exe"))
            .is_some_and(|p| plain(&p).is_ok_and(|m| m.is_file()))
}

// Narrow static paths only. Filenames alone are never authority to remove a BAT.
fn bat_root(root: &Path, bat: &Path) -> Option<PathBuf> {
    let relative = paths::path_text(bat.file_name().map(Path::new)?).ok()?;
    let analysis = scanner::analyze_bat(bat, relative);
    let observed = if let Some(recipe) = analysis.recipe {
        recipe.observed_root
    } else {
        let bytes = fs::read(bat).ok()?;
        if bytes.len() > 256 * 1024 {
            return None;
        }
        let content = String::from_utf8(bytes).ok()?;
        let mut observed = None;
        for line in content.lines() {
            let line = line.trim().trim_start_matches('@').trim();
            let lower = line.to_ascii_lowercase();
            if line.is_empty()
                || lower == "echo off"
                || lower == "chcp 65001"
                || lower == "cd /d \"%~dp0\""
                || lower.starts_with("rem ")
                || lower.starts_with("::")
            {
                continue;
            }
            if line.chars().any(|c| "&|<>^!".contains(c)) {
                return None;
            }
            let parts = crate::mtool::tokens(line).ok()?;
            if !(parts.len() == 3 || parts.len() == 4)
                || !parts[0].eq_ignore_ascii_case("start")
                || !parts[1].is_empty()
                || observed.is_some()
            {
                return None;
            }
            let exe = parts[2].replace('\\', "/");
            if !exe.to_ascii_lowercase().ends_with("/mtool.exe") {
                return None;
            }
            let parent = exe[..exe.len() - "MTool.exe".len()].trim_end_matches('/');
            if parts.len() == 4 && parts[3].replace('\\', "/").trim_end_matches('/') != parent {
                return None;
            }
            observed = Some(parent.to_string());
        }
        observed?
    };
    let path = if observed.to_ascii_lowercase().starts_with("%~dp0") {
        root.join(paths::relative_path(observed[5..].trim_start_matches(['\\', '/'])).ok()?)
    } else {
        if observed.contains('%') || observed.contains('!') {
            return None;
        }
        PathBuf::from(observed)
    };
    let path = checked_dir(&path).ok()?;
    let root = checked_dir(root).ok()?;
    path.strip_prefix(&root).ok()?;
    (path != root).then_some(path)
}
fn known_file(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "mtool.exe"
            | "nw.exe"
            | "package.json"
            | "nw.dll"
            | "nw_elf.dll"
            | "node.dll"
            | "ffmpeg.dll"
            | "libegl.dll"
            | "libglesv2.dll"
            | "icudtl.dat"
            | "resources.pak"
            | "nw_100_percent.pak"
            | "nw_200_percent.pak"
            | "d3dcompiler_47.dll"
            | "v8_context_snapshot.bin"
            | "snapshot_blob.bin"
            | "credits.html"
            | "mtool-runtime"
            | "loaders/inject.exe"
            | "loaders/inject.bin"
            | "loaders/inject.ver"
            | "loaders/mzhook.dll"
            | "loaders/mzhook32.dll"
            | "loaders/rgsshook.dll"
            | "loaders/rgsshook64.dll"
            | "loaders/pythonhook.dll"
            | "loaders/pythonhook64.dll"
    )
}
fn safe_tree(top: &Path, tool: &Path, shared: &str, stop: &dyn Fn() -> bool) -> Result<()> {
    if !tool_signature(tool) {
        return Err(invalid("未确认 MTool package.json 与注入器，保留目录"));
    }
    let reference = (!shared.is_empty())
        .then(|| checked_dir(Path::new(shared)).ok())
        .flatten()
        .filter(|p| tool_signature(p));
    let tree = snapshot(top, false, stop)?;
    let prefix = tool
        .strip_prefix(top)
        .map_err(|_| invalid("工具路径越界"))?;
    for (name, stamp) in &tree {
        if name.is_empty() {
            continue;
        }
        let path = paths::relative_path(name)?;
        // An outer wrapper must contain just the tool package, not siblings or extra files.
        if !path.starts_with(prefix) {
            if stamp.directory && prefix.starts_with(&path) {
                continue;
            }
            return Err(invalid(format!("一级目录混有其他内容：{name}")));
        }
        let member = path
            .strip_prefix(prefix)
            .map_err(|_| invalid("工具路径越界"))?;
        let relative = paths::path_text(member)?.replace('\\', "/");
        let lower = relative.to_ascii_lowercase();
        if stamp.directory
            && matches!(lower.as_str(), "gamelib" | "gamesavebackup")
            && children(&top.join(&path))?.is_empty()
        {
            continue;
        }
        if lower
            .split('/')
            .any(|s| matches!(s, "save" | "saves" | "gamesavebackup" | "gamelib"))
            || lower.ends_with(".rpa")
            || lower.ends_with(".xp3")
            || lower.ends_with(".qsp")
            || lower.ends_with(".rpgsave")
            || lower.ends_with(".rmmzsave")
            || lower.ends_with(".save")
            || lower.ends_with("/system.json")
            || lower.ends_with("/game.exe")
            || lower.contains("/map0")
        {
            return Err(invalid(format!("疑似游戏资源、存档或工具用户资料：{name}")));
        }
        if relative.is_empty()
            || (stamp.directory && lower == "loaders")
            || (!stamp.directory && known_file(&relative))
        {
            continue;
        }
        // Unknown package assets are removable only when an identical shared-tool member proves ownership.
        let Some(reference) = &reference else {
            return Err(invalid(format!("无法确认工具内容：{name}；未清理")));
        };
        let reference_file = reference.join(member);
        let meta = plain(&reference_file)
            .map_err(|_| invalid(format!("公共 MTool 中没有对应内容：{name}")))?;
        if meta.is_dir() != stamp.directory
            || (!stamp.directory
                && (meta.len() != stamp.bytes
                    || fingerprint(&reference_file, stop)? != fingerprint(&top.join(&path), stop)?))
        {
            return Err(invalid(format!(
                "无法确认工具内容：{name}；与公共 MTool 不同"
            )));
        }
    }
    Ok(())
}

pub(super) fn detect(root: &Path, shared: &str, stop: &dyn Fn() -> bool) -> Vec<MtoolBundle> {
    let Ok(root) = checked_dir(root) else {
        return vec![];
    };
    let Ok(entries) = children(&root) else {
        return vec![];
    };
    let mut bats: Vec<_> = entries
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bat")))
        .filter(|p| plain(p).is_ok_and(|m| m.is_file() && m.len() <= 256 * 1024))
        .cloned()
        .collect();
    bats.sort_by_key(|p| {
        !crate::mtool::generated_launcher_name(&p.file_name().unwrap().to_string_lossy())
    });
    if bats.len() > 128 {
        return vec![];
    }
    let references: Vec<_> = bats
        .iter()
        .filter_map(|bat| bat_root(&root, bat).map(|tool| (bat, tool)))
        .collect();
    let mut tools = BTreeMap::new();
    // BAT references first, then a two-level reverse lookup; never walk game resource trees to find an EXE.
    for (_, tool) in &references {
        tools.insert(tool.clone(), ());
    }
    let mut search_entries = 0;
    for top in entries
        .iter()
        .filter(|p| references.is_empty() && plain(p).is_ok_and(|m| m.is_dir()))
    {
        if stop() {
            break;
        }
        if named(top, "MTool.exe").is_some() {
            tools.insert(top.clone(), ());
            continue;
        }
        if let Ok(children) = children(top) {
            search_entries += children.len();
            if search_entries > MAX_ENTRIES {
                break;
            }
            for child in children
                .into_iter()
                .filter(|p| plain(p).is_ok_and(|m| m.is_dir()))
            {
                if stop() {
                    break;
                }
                if named(&child, "MTool.exe").is_some() {
                    tools.insert(child, ());
                }
            }
        }
    }
    let mut result = BTreeMap::new();
    for tool in tools.keys() {
        let Ok(relative) = tool.strip_prefix(&root) else {
            continue;
        };
        if relative.components().count() > 2 {
            continue;
        }
        let Some(first) = relative.components().next() else {
            continue;
        };
        let top = root.join(first.as_os_str());
        let folder = first.as_os_str().to_string_lossy().to_string();
        let associated = references
            .iter()
            .filter(|(_, path)| path == tool)
            .map(|(bat, _)| bat.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        let safety =
            if !named(tool, "MTool.exe").is_some_and(|p| plain(&p).is_ok_and(|m| m.is_file())) {
                Err(invalid("没有确认 MTool.exe，保留目录"))
            } else {
                safe_tree(&top, tool, shared, stop)
            };
        let bundle = MtoolBundle {
            folder: folder.clone(),
            bats: associated,
            safe: safety.is_ok(),
            reason: safety.err().map_or_else(
                || "已确认随包 MTool；导入后使用公共 MTool 启动".into(),
                |e| e.to_string(),
            ),
        };
        // Multiple packages in one first-level folder are ambiguous, not independent deletion targets.
        if let Some(previous) = result.insert(folder.clone(), bundle) {
            result.get_mut(&folder).unwrap().safe = false;
            result.get_mut(&folder).unwrap().reason = "一级目录包含多个工具包，保留目录".into();
            result.get_mut(&folder).unwrap().bats.extend(previous.bats);
        }
    }
    let mut bundles: Vec<_> = result.into_values().collect();
    // The two explicitly supported companion removal scripts are recycled, never interpreted.
    // Keep them if several packages exist and any package has uncertain ownership.
    if bundles.len() == 1 || (!bundles.is_empty() && bundles.iter().all(|b| b.safe)) {
        let companions = entries
            .iter()
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| crate::mtool::generated_removal_name(&n.to_string_lossy()))
                    && plain(p).is_ok_and(|m| m.is_file() && m.len() <= 256 * 1024)
            })
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string());
        bundles[0].bats.extend(companions);
        bundles[0].bats.sort();
        bundles[0].bats.dedup();
    }
    bundles
}

pub(super) fn prepare(root: &Path, bundles: &[MtoolBundle], job: &Job) -> Result<CleanupJournal> {
    let selected: Vec<_> = bundles.iter().filter(|b| b.safe).cloned().collect();
    if selected.is_empty() {
        return Err(invalid("没有可安全清理的随包 MTool，请取消清理"));
    }
    let mut paths = BTreeMap::new();
    for bundle in &selected {
        paths.insert(bundle.folder.clone(), ());
        for bat in &bundle.bats {
            paths.insert(bat.clone(), ());
        }
    }
    if paths.len() > 128 {
        return Err(invalid("工具清理目标超过检查上限，保留文件"));
    }
    let mut entries = 0;
    let mut targets = vec![];
    for path in paths.keys() {
        let snapshot = snapshot(&root.join(path), true, &|| job.stop())?;
        entries += snapshot.len();
        if entries > MAX_ENTRIES {
            return Err(invalid("工具清理记录超过检查上限，保留文件"));
        }
        targets.push(CleanupTarget {
            path: path.clone(),
            state: "pending".into(),
            error: None,
            snapshot,
            bound: false,
        });
    }
    Ok(CleanupJournal {
        bundles: selected,
        targets,
    })
}
fn touches(cleanup: &CleanupJournal, path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    cleanup.targets.iter().any(|t| {
        let target = t.path.to_ascii_lowercase();
        normalized == target || normalized.starts_with(&format!("{target}/"))
    })
}
pub(super) fn check_save_paths(cleanup: &CleanupJournal, saves: &[String]) -> Result<()> {
    for path in saves {
        let normalized = path.replace('\\', "/");
        let relative = normalized.strip_prefix("<GAME>/").unwrap_or(&normalized);
        if relative == "<GAME>" || relative == "." || touches(cleanup, relative) {
            return Err(invalid("存档配置依赖随包 MTool，取消清理或先调整存档位置"));
        }
    }
    Ok(())
}
pub(super) fn check_dependencies(
    cleanup: &CleanupJournal,
    config: &VersionConfig,
    settings: &Settings,
    saves: &[String],
) -> Result<()> {
    for path in [
        config.executable.as_deref(),
        config.mtool_target.as_deref(),
        Some(config.working_directory.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if touches(cleanup, path) {
            return Err(invalid(
                "启动配置依赖待清理文件，请更改游戏 EXE / 工作目录或取消清理",
            ));
        }
    }
    if config.launch_type != "MTOOL" {
        return Err(invalid("清理随包 MTool 后必须使用公共 MTool 启动"));
    }
    if settings.mtool_root.is_empty() {
        return Err(invalid("请先配置公共 MTool，或取消随包清理"));
    }
    check_save_paths(cleanup, saves)
}

pub(super) fn check_launch_paths(
    root: &Path,
    cleanup: &CleanupJournal,
    config: &VersionConfig,
) -> Result<()> {
    for launch in [
        config.executable.as_deref(),
        config.mtool_target.as_deref(),
        Some(config.working_directory.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if launch == "." {
            continue;
        }
        let path = dunce::canonicalize(root.join(paths::relative_path(launch)?))?;
        for target in &cleanup.targets {
            let target = root.join(&target.path);
            // Also catch Windows short names and directory aliases before approving a plan.
            let target = if exists(&target)? {
                dunce::canonicalize(target)?
            } else {
                target
            };
            if overlaps(&path, &target)? {
                return Err(invalid("启动路径依赖待清理文件，请调整配置或取消清理"));
            }
        }
    }
    Ok(())
}

pub(super) fn check_shared_launch(
    root: &Path,
    config: &VersionConfig,
    settings: &Settings,
) -> Result<()> {
    let game = Game {
        id: String::new(),
        canonical_title: String::new(),
        display_title: String::new(),
        install_path: paths::path_text(root)?,
        working_directory: config.working_directory.clone(),
        current_version: config.version.clone(),
        version_source: config.version_source.clone(),
        main_executable: config.executable.clone(),
        engine: config.engine.clone(),
        launch_type: config.launch_type.clone(),
        external_player: config.external_player.clone(),
        mtool_target_exe: config.mtool_target.clone(),
        mtool_loader: config.mtool_loader.clone(),
        created_at: String::new(),
        updated_at: String::new(),
        last_launched_at: None,
        play_status: crate::domain::PlayStatus::Unplayed,
        aliases: vec![],
        save_paths: vec![],
    };
    crate::launcher::preview_mtool(&game, settings)
        .map(|_| ())
        .map_err(|e| invalid(format!("清理后公共 MTool 启动配置不可用，请先调整：{e}")))
}

pub(super) fn validate_journal(item: &Item) -> Result<()> {
    let Some(cleanup) = &item.mtool_cleanup else {
        return Ok(());
    };
    if !item.selection.cleanup_mtool || cleanup.targets.is_empty() || cleanup.targets.len() > 128 {
        return Err(invalid("工具清理记录无效"));
    }
    let mut seen = HashSet::new();
    let mut entries = 0;
    for bundle in &cleanup.bundles {
        folder_name(&bundle.folder)?;
        if !bundle.safe {
            return Err(invalid("清理记录包含未确认工具包"));
        }
        for bat in &bundle.bats {
            folder_name(bat)?;
            if !bat.to_ascii_lowercase().ends_with(".bat") {
                return Err(invalid("清理记录包含非 BAT 文件"));
            }
        }
    }
    for target in &cleanup.targets {
        folder_name(&target.path)?;
        if !seen.insert(target.path.to_ascii_lowercase())
            || !matches!(
                target.state.as_str(),
                "pending" | "recycling" | "recycled" | "failed"
            )
            || target.snapshot.is_empty()
            || target.snapshot.len() > MAX_ENTRIES
            || !target.snapshot.contains_key("")
        {
            return Err(invalid("工具清理路径或状态无效"));
        }
        if !cleanup
            .bundles
            .iter()
            .any(|b| b.safe && (b.folder == target.path || b.bats.contains(&target.path)))
        {
            return Err(invalid("工具清理目标不属于已确认工具包"));
        }
        entries += target.snapshot.len();
        let root = &target.snapshot[""];
        let folder = cleanup.bundles.iter().any(|b| b.folder == target.path);
        if entries > MAX_ENTRIES
            || root.directory != folder
            || (!folder && target.snapshot.len() != 1)
            || target
                .snapshot
                .values()
                .any(|stamp| stamp.identity.is_empty())
            || (matches!(target.state.as_str(), "recycling" | "recycled") && !target.bound)
        {
            return Err(invalid("工具清理身份记录无效"));
        }
        for name in target.snapshot.keys().filter(|n| !n.is_empty()) {
            paths::relative_path(name)?;
        }
    }
    Ok(())
}

impl ImportStore {
    pub(super) fn cleanup_mtool(
        &self,
        plan: &mut Plan,
        index: usize,
        job: &Job,
        database: &Arc<Mutex<Database>>,
    ) -> Result<()> {
        if plan.items[index].state != "completed"
            || plan.items[index]
                .mtool_cleanup
                .as_ref()
                .is_none_or(|c| c.targets.iter().all(|t| t.state == "recycled"))
        {
            return Ok(());
        }
        validate_journal(&plan.items[index])?;
        job.activity_phase("清理随包 MTool");
        let root = PathBuf::from(&plan.items[index].target);
        let validation = (|| -> Result<()> {
            require_identity(Path::new(&plan.root), Some(&plan.root_id))?;
            require_identity(&root, plan.items[index].payload_id.as_deref())?;
            let db = database.lock().unwrap();
            let item = &plan.items[index];
            let game = db.game(&item.game_id)?;
            if paths::path_key(Path::new(&game.install_path))? != paths::path_key(&root)? {
                return Err(invalid("游戏目录已改变，保留工具"));
            }
            if item.update.is_some() {
                db.ensure_latest_version(&item.game_id, &format!("{}:{index}", plan.id))?;
            }
            let settings = db.settings()?;
            let cleanup = item.mtool_cleanup.as_ref().unwrap();
            let config = db.version_config(&item.game_id)?;
            check_dependencies(cleanup, &config, &settings, &game.save_paths)?;
            check_launch_paths(&root, cleanup, &config)?;
            crate::launcher::preview_mtool(&game, &settings)?;
            let games = db.games()?;
            for target in &cleanup.targets {
                let path = root.join(&target.path);
                if overlaps(&path, &dunce::canonicalize(&settings.mtool_root)?)?
                    || games.iter().any(|g| {
                        g.id != item.game_id
                            && overlaps(&path, Path::new(&g.install_path)).unwrap_or(true)
                    })
                {
                    return Err(invalid("清理范围与公共 MTool 或其他游戏重叠"));
                }
                for save in &game.save_paths {
                    let save = crate::deletion::resolve_save(&game, save)?;
                    let save = if save.exists() {
                        dunce::canonicalize(save)?
                    } else {
                        save
                    };
                    if overlaps(&path, &save)? {
                        return Err(invalid("清理范围包含关联存档，保留文件"));
                    }
                }
            }
            check_item_idle(std::slice::from_ref(&root), job)
        })();
        if let Err(error) = validation {
            for target in &mut plan.items[index].mtool_cleanup.as_mut().unwrap().targets {
                if target.state != "recycled" {
                    target.state = "failed".into();
                    target.error = Some(error.to_string());
                }
            }
            return self.save(plan);
        }
        let count = plan.items[index]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .len();
        for position in 0..count {
            stopped(job)?;
            let target =
                plan.items[index].mtool_cleanup.as_ref().unwrap().targets[position].clone();
            if target.state == "recycled" {
                continue;
            }
            let path = root.join(&target.path);
            let result = (|| -> Result<()> {
                require_identity(&root, plan.items[index].payload_id.as_deref())?;
                if !exists(&path)? {
                    if target.state == "recycling"
                        && target.bound
                        && self
                            .recycled_version(&target.snapshot[""].identity)?
                            .is_some()
                    {
                        return Ok(());
                    }
                    return Err(invalid("清理目标消失且无法确认回收身份；未处理其他文件"));
                }
                let actual = snapshot(&path, true, &|| job.stop())?;
                if !same_content(
                    &target.snapshot,
                    &actual,
                    target.bound || !plan.items[index].cross_volume,
                ) {
                    return Err(invalid("工具文件身份或内容已变化，保留文件"));
                }
                {
                    let entry =
                        &mut plan.items[index].mtool_cleanup.as_mut().unwrap().targets[position];
                    entry.snapshot = actual;
                    entry.bound = true;
                    entry.state = "recycling".into();
                    entry.error = None;
                }
                self.save(plan)?;
                #[cfg(test)]
                if FAIL_CHECKPOINT.with(|f| {
                    let mut f = f.borrow_mut();
                    if f.as_deref() == Some("mtool_before_recycle") {
                        f.take();
                        true
                    } else {
                        false
                    }
                }) {
                    return Err(invalid("测试夹具：回收前中断"));
                }
                #[cfg(test)]
                if FAIL_CHECKPOINT.with(|f| f.borrow().as_deref() == Some("mtool_recycle_fail")) {
                    return Err(invalid("测试夹具：回收失败"));
                }
                self.recycle_version(&path)?;
                if exists(&path)? {
                    return Err(invalid("回收操作未移走工具，保留并报告"));
                }
                Ok(())
            })();
            #[cfg(test)]
            if result.is_ok()
                && FAIL_CHECKPOINT.with(|f| {
                    let mut f = f.borrow_mut();
                    if f.as_deref() == Some("mtool_after_recycle") {
                        f.take();
                        true
                    } else {
                        false
                    }
                })
            {
                return Err(invalid("测试夹具：回收后中断"));
            }
            // Missing after an error may mean Shell succeeded before its callback/journal completed.
            let success = result.is_ok()
                || (!exists(&path)? && {
                    let current =
                        &plan.items[index].mtool_cleanup.as_ref().unwrap().targets[position];
                    current.bound
                        && self
                            .recycled_version(&current.snapshot[""].identity)?
                            .is_some()
                });
            let entry = &mut plan.items[index].mtool_cleanup.as_mut().unwrap().targets[position];
            entry.state = if success { "recycled" } else { "failed" }.into();
            entry.error = if success {
                None
            } else {
                result.err().map(|e| e.to_string())
            };
            self.save(plan)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::TaskManager;

    fn pe(path: &Path) {
        let mut bytes = vec![0u8; 70];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }
    fn package(root: &Path) {
        fs::create_dir_all(root.join("loaders")).unwrap();
        fs::write(
            root.join("package.json"),
            br#"{"name":"MToolClient_fixture"}"#,
        )
        .unwrap();
        for name in ["MTool.exe", "loaders/inject.exe", "loaders/mzHook.dll"] {
            pe(&root.join(name));
        }
    }
    fn launcher(root: &Path, folder: &str, name: &str) {
        fs::write(root.join(name), format!(
            "@echo off\n@cd /d \"%~dp0\"\n\"%~dp0{folder}/loaders/inject.exe\" \"%~dp0Game.exe\" \"%~dp0{folder}/loaders/mzHook.dll\"\nstart \"\" \"%~dp0{folder}/MTool.exe\" \"%~dp0{folder}\""
        )).unwrap();
    }
    struct Fixture {
        _temp: tempfile::TempDir,
        source: PathBuf,
        shared: PathBuf,
        root: PathBuf,
        store: ImportStore,
        db: Arc<Mutex<Database>>,
        tasks: TaskManager,
    }
    impl Fixture {
        fn new(nested: bool) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("incoming");
            let shared = temp.path().join("shared");
            let root = temp.path().join("library");
            let data = temp.path().join("data");
            for dir in [&source, &root, &data] {
                fs::create_dir_all(dir).unwrap();
            }
            let folder = if nested {
                "辅助工具/client"
            } else {
                "辅助工具"
            };
            package(&source.join(folder));
            package(&shared);
            pe(&source.join("Game.exe"));
            launcher(&source, folder, "与工具一同启动.bat");
            fs::write(source.join("普通启动.bat"), b"do not run me").unwrap();
            let db = Database::open(&data.join("fixture.sqlite3")).unwrap();
            db.save_settings(Settings {
                game_root: paths::path_text(&root).unwrap(),
                mtool_root: paths::path_text(&shared).unwrap(),
                mtool_runtime: "MTool.exe".into(),
                mtool_injector: "loaders/inject.exe".into(),
                ..Settings::default()
            })
            .unwrap();
            Self {
                _temp: temp,
                source,
                shared,
                root,
                store: ImportStore::open(data.join("imports")).unwrap(),
                db: Arc::new(Mutex::new(db)),
                tasks: TaskManager::default(),
            }
        }
        fn plan(
            &self,
            cleanup: bool,
            existing: Option<String>,
            executable: &str,
            cross: bool,
        ) -> String {
            let candidate = scanner::analyze_directory(&self.source).unwrap();
            let settings = self.db.lock().unwrap().settings().unwrap();
            let games = self.db.lock().unwrap().games().unwrap();
            let job = self
                .tasks
                .begin("import_plan", settings.game_root.clone())
                .unwrap();
            self.store
                .prepare(
                    &job,
                    settings,
                    vec![candidate],
                    vec![Selection {
                        source: paths::path_text(&self.source).unwrap(),
                        title: "Game v2".into(),
                        target_name: "Game v2".into(),
                        version: "v2".into(),
                        engine: "RPG Maker MZ".into(),
                        executable: executable.into(),
                        working_directory: None,
                        mtool_loader: None,
                        external_player: None,
                        mtool: false,
                        existing_id: existing,
                        new_override: true,
                        preserve_saves: true,
                        saves_confirmed: true,
                        cleanup_mtool: cleanup,
                    }],
                    &games,
                )
                .unwrap();
            if cross {
                let mut plan = self.store.get(&job.id).unwrap();
                plan.items[0].cross_volume = true;
                self.store.save(&plan).unwrap();
            }
            job.finish(Ok(vec![]));
            job.id.clone()
        }
        fn apply(&self, id: &str) -> Result<Vec<String>> {
            let job = self
                .tasks
                .begin("import_apply", paths::path_text(&self.root)?)
                .unwrap();
            let result = self.store.apply(id, &job, &self.db);
            job.finish(
                result
                    .as_ref()
                    .map(|v| v.clone())
                    .map_err(|e| e.to_string()),
            );
            result
        }
    }

    #[test]
    fn recognizes_chinese_bat_nested_and_reverse_lookup_without_name_guessing() {
        for nested in [false, true] {
            let f = Fixture::new(nested);
            let bundles = detect(&f.source, "", &|| false);
            assert_eq!(bundles.len(), 1);
            assert_eq!(bundles[0].folder, "辅助工具");
            assert!(bundles[0].safe);
            assert_eq!(bundles[0].bats, vec!["与工具一同启动.bat"]);
            fs::remove_file(f.source.join("与工具一同启动.bat")).unwrap();
            assert!(detect(&f.source, "", &|| false)[0].safe);
            assert!(detect(&f.source, "", &|| false)[0].bats.is_empty());
        }
    }
    #[test]
    fn recognizes_both_known_root_removal_scripts_without_executing_them() {
        let f = Fixture::new(false);
        for name in [
            "从游戏中移除工具文件.bat",
            "Remove Tool files from game.bat",
        ] {
            fs::write(f.source.join(name), b"this script must never execute").unwrap();
        }
        let id = f.plan(true, None, "Game.exe", false);
        let plan = f.store.get(&id).unwrap();
        assert_eq!(
            plan.items[0].mtool_cleanup.as_ref().unwrap().targets.len(),
            4
        );
        f.apply(&id).unwrap();
        for name in [
            "从游戏中移除工具文件.bat",
            "Remove Tool files from game.bat",
        ] {
            assert!(!f.root.join("Game v2").join(name).exists());
        }
        assert!(f.root.join("Game v2/Game.exe").exists());
    }
    #[test]
    fn conservatively_keeps_mixed_resources_unknown_files_user_data_and_multiple_packages() {
        for relative in [
            "Game.exe",
            "notes.txt",
            "gameSaveBackup/file.dat",
            "www/data/System.json",
        ] {
            let f = Fixture::new(false);
            let file = f.source.join("辅助工具").join(relative);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, b"keep").unwrap();
            assert!(
                !detect(&f.source, &paths::path_text(&f.shared).unwrap(), &|| false)[0].safe,
                "{relative}"
            );
        }
        let f = Fixture::new(true);
        package(&f.source.join("辅助工具/other"));
        assert!(!detect(&f.source, "", &|| false)[0].safe);
    }
    #[test]
    fn unknown_tool_assets_require_exact_shared_member_and_unrelated_bats_are_preserved() {
        let f = Fixture::new(false);
        for root in [&f.source.join("辅助工具"), &f.shared] {
            fs::create_dir(root.join("www")).unwrap();
            fs::write(root.join("www/client.js"), b"known tool asset").unwrap();
        }
        assert!(!detect(&f.source, "", &|| false)[0].safe);
        assert!(detect(&f.source, &paths::path_text(&f.shared).unwrap(), &|| false)[0].safe);
        fs::write(f.source.join("辅助工具/www/client.js"), b"unconfirmed code").unwrap();
        assert!(!detect(&f.source, &paths::path_text(&f.shared).unwrap(), &|| false)[0].safe);
        launcher(&f.source, "辅助工具", "StartWithTool.bat");
        fs::remove_file(f.source.join("辅助工具/www/client.js")).unwrap();
        fs::remove_dir(f.source.join("辅助工具/www")).unwrap();
        assert!(detect(&f.source, "", &|| false)[0]
            .bats
            .contains(&"StartWithTool.bat".into()));
        fs::write(
            f.source.join("StartWithTool.bat"),
            b"@echo off\ndel Game.exe",
        )
        .unwrap();
        assert!(!detect(&f.source, "", &|| false)[0]
            .bats
            .contains(&"StartWithTool.bat".into()));
    }
    #[test]
    fn cleanup_works_after_new_import_and_copy_switches_to_shared_mtool_and_is_idempotent() {
        for cross in [false, true] {
            let f = Fixture::new(true);
            let id = f.plan(true, None, "Game.exe", cross);
            assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
            let ids = f.apply(&id).unwrap();
            let target = f.root.join("Game v2");
            assert!(target.join("Game.exe").is_file());
            assert!(target.join("普通启动.bat").is_file());
            assert!(!target.join("辅助工具").exists());
            assert!(!target.join("与工具一同启动.bat").exists());
            let game = f.db.lock().unwrap().game(&ids[0]).unwrap();
            assert_eq!(game.launch_type, "MTOOL");
            assert_eq!(game.mtool_target_exe.as_deref(), Some("Game.exe"));
            assert!(f.shared.join("MTool.exe").is_file());
            assert!(f.store.get(&id).unwrap().items[0]
                .mtool_cleanup
                .as_ref()
                .unwrap()
                .targets
                .iter()
                .all(|t| t.state == "recycled"));
            assert_eq!(f.apply(&id).unwrap(), ids);
        }
    }
    #[test]
    fn opt_out_keeps_bundle_and_dependencies_or_missing_shared_tool_block_the_plan() {
        let f = Fixture::new(false);
        let id = f.plan(false, None, "Game.exe", false);
        let ids = f.apply(&id).unwrap();
        assert!(f.root.join("Game v2/辅助工具/MTool.exe").is_file());
        assert_eq!(
            f.db.lock().unwrap().game(&ids[0]).unwrap().launch_type,
            "DIRECT"
        );
        let f = Fixture::new(false);
        let id = f.plan(true, None, "辅助工具/MTool.exe", false);
        assert!(!f.store.get(&id).unwrap().items[0].blockers.is_empty());
        assert!(f.apply(&id).is_err());
        let f = Fixture::new(false);
        fs::remove_file(f.shared.join("loaders/mzHook.dll")).unwrap();
        let id = f.plan(true, None, "Game.exe", false);
        assert!(!f.store.get(&id).unwrap().items[0].blockers.is_empty());
    }
    #[test]
    fn changed_content_is_retained_and_recycling_failure_keeps_successful_game_retryable() {
        let f = Fixture::new(false);
        let id = f.plan(true, None, "Game.exe", false);
        fs::write(
            f.source.join("辅助工具/loaders/mzHook.dll"),
            b"changed loader",
        )
        .unwrap();
        let ids = f.apply(&id).unwrap();
        assert!(f.root.join("Game v2/辅助工具").exists());
        assert!(f.db.lock().unwrap().game(&ids[0]).is_ok());
        assert!(f.store.get(&id).unwrap().items[0]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .any(|t| t.state == "failed"));
        let f = Fixture::new(false);
        let id = f.plan(true, None, "Game.exe", false);
        FAIL_CHECKPOINT.with(|v| *v.borrow_mut() = Some("mtool_recycle_fail".into()));
        let ids = f.apply(&id).unwrap();
        FAIL_CHECKPOINT.with(|v| *v.borrow_mut() = None);
        assert_eq!(f.store.get(&id).unwrap().status, "completed");
        assert!(f.root.join("Game v2/辅助工具").exists());
        assert!(f.db.lock().unwrap().game(&ids[0]).is_ok());
        assert!(f.store.get(&id).unwrap().items[0]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .all(|t| t.error.is_some()));
        f.apply(&id).unwrap();
        assert!(!f.root.join("Game v2/辅助工具").exists());
    }
    #[test]
    fn journal_resume_after_shell_recycle_does_not_delete_replacement_or_recycle_twice() {
        let mut f = Fixture::new(false);
        let id = f.plan(true, None, "Game.exe", false);
        FAIL_CHECKPOINT.with(|v| *v.borrow_mut() = Some("mtool_after_recycle".into()));
        assert!(f.apply(&id).is_err());
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        assert!(f.store.recovery_issues().is_empty());
        let before = f.store.recycled_versions().unwrap().len();
        f.apply(&id).unwrap();
        assert_eq!(f.store.recycled_versions().unwrap().len(), before + 1);
        assert!(!f.root.join("Game v2/辅助工具").exists());
        // Reoccupying the old cleanup name after completion cannot cause another removal.
        fs::create_dir(f.root.join("Game v2/辅助工具")).unwrap();
        fs::write(f.root.join("Game v2/辅助工具/keep.txt"), b"new content").unwrap();
        f.apply(&id).unwrap();
        assert!(f.root.join("Game v2/辅助工具/keep.txt").exists());
    }
    #[test]
    fn reopened_cleanup_retries_failure_before_shell_and_retains_reoccupied_path_after_shell() {
        let mut f = Fixture::new(false);
        let id = f.plan(true, None, "Game.exe", false);
        FAIL_CHECKPOINT.with(|v| *v.borrow_mut() = Some("mtool_before_recycle".into()));
        f.apply(&id).unwrap();
        assert!(f.store.get(&id).unwrap().items[0]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .any(|t| t.state == "failed"));
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        assert!(f.store.recovery_issues().is_empty());
        f.apply(&id).unwrap();
        assert!(f.store.get(&id).unwrap().items[0]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .all(|t| t.state == "recycled"));

        let mut f = Fixture::new(false);
        let id = f.plan(true, None, "Game.exe", false);
        FAIL_CHECKPOINT.with(|v| *v.borrow_mut() = Some("mtool_after_recycle".into()));
        assert!(f.apply(&id).is_err());
        let plan = f.store.get(&id).unwrap();
        let pending = plan.items[0]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .find(|t| t.state == "recycling")
            .unwrap();
        let replacement = f.root.join("Game v2").join(&pending.path);
        if pending.snapshot[""].directory {
            package(&replacement);
        } else {
            fs::write(&replacement, b"replacement must stay").unwrap();
        }
        let replacement_id = identity(&replacement).unwrap();
        f.store = ImportStore::open(f.store.directory.clone()).unwrap();
        f.apply(&id).unwrap();
        assert_eq!(identity(&replacement).unwrap(), replacement_id);
        assert!(f.store.get(&id).unwrap().items[0]
            .mtool_cleanup
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .any(|t| t.state == "failed"));
    }
    #[test]
    fn cleanup_after_update_preserves_saves_history_and_rollback() {
        let f = Fixture::new(true);
        let old = f.root.join("Game v1");
        fs::create_dir_all(old.join("www/save")).unwrap();
        fs::write(old.join("www/save/file1.rmmzsave"), b"old progress").unwrap();
        pe(&old.join("Game.exe"));
        let id =
            f.db.lock()
                .unwrap()
                .register(&[scanner::analyze_directory(&old).unwrap()])
                .unwrap()
                .remove(0);
        let operation = f.plan(true, Some(id.clone()), "Game.exe", false);
        assert!(f.store.get(&operation).unwrap().items[0]
            .blockers
            .is_empty());
        f.apply(&operation).unwrap_or_else(|error| {
            let plan = f.store.get(&operation).unwrap();
            panic!("update failed at {}: {error}", plan.items[0].state);
        });
        let target = f.root.join("Game v2");
        assert_eq!(
            fs::read(target.join("www/save/file1.rmmzsave")).unwrap(),
            b"old progress"
        );
        assert!(!target.join("辅助工具").exists());
        assert_eq!(f.db.lock().unwrap().game(&id).unwrap().launch_type, "MTOOL");
        let job = f
            .tasks
            .begin("import_rollback", paths::path_text(&f.root).unwrap())
            .unwrap();
        let result = f.store.rollback(&operation, 0, &job, &f.db);
        job.finish(result.as_ref().map(|_| vec![]).map_err(|e| e.to_string()));
        result.unwrap();
        assert_eq!(
            fs::read(old.join("www/save/file1.rmmzsave")).unwrap(),
            b"old progress"
        );
    }
}
