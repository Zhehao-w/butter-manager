//! Import-only, bounded MTool recognition and journaled recycling. Never runs a BAT.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_ENTRIES: usize = 4096;
const MAX_SMALL_FILE: u64 = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MtoolBundle {
    pub folder: String,
    pub bats: Vec<String>,
    // Legacy serialized fields; cleanup no longer uses eligibility flags.
    pub safe: bool,
    pub reason: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    directory: bool,
    bytes: u64,
    modified: u128,
    identity: String,
    // Only BATs/manifests need hashes; new journals record directory and key-file identities.
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
// Only BATs and package manifests are hashed; never read the tool resource tree.
fn stamp(path: &Path) -> Result<Stamp> {
    let metadata = plain(path)?;
    let small = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bat"))
        || path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("package.json"));
    let digest = if metadata.is_file() && small {
        if metadata.len() > MAX_SMALL_FILE {
            return Err(invalid("工具启动文件超过检查上限"));
        }
        Some(format!("{:x}", Sha256::digest(fs::read(path)?)))
    } else {
        None
    };
    Ok(Stamp {
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
        identity: identity(path)?,
        digest,
    })
}
fn capture(
    path: &Path,
    keys: impl IntoIterator<Item = String>,
    stop: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Stamp>> {
    let mut result = BTreeMap::new();
    for name in keys {
        if stop() {
            return Err(invalid("已停止工具检查"));
        }
        let member = if name.is_empty() {
            path.to_path_buf()
        } else {
            path.join(paths::relative_path(&name)?)
        };
        result.insert(name, stamp(&member)?);
    }
    Ok(result)
}
fn snapshot(path: &Path, stop: &dyn Fn() -> bool) -> Result<BTreeMap<String, Stamp>> {
    // The cleanup unit is the entire selected first-level directory. Record its
    // identity, not its contents or an additional tool-package signature. Old
    // journals keep using their recorded keys through capture().
    capture(path, [String::new()], stop)
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
// Narrow static paths only. Filenames alone are never authority to remove a BAT.
fn bat_root(root: &Path, bat: &Path) -> Option<PathBuf> {
    let relative = paths::path_text(bat.file_name().map(Path::new)?).ok()?;
    let analysis = scanner::analyze_bat(bat, relative);
    let recipe = analysis.recipe;
    let observed = if let Some(recipe) = &recipe {
        recipe.observed_root.clone()
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
    let root = checked_dir(root).ok()?;
    if let Ok(path) = checked_dir(&path) {
        path.strip_prefix(&root).ok()?;
        return (path != root).then_some(path);
    }
    // Generated launchers often retain absolute paths from the machine that packaged the game.
    // Relocate only a complete known recipe with a missing old root and a unique matching package.
    let recipe = recipe?;
    if !crate::mtool::generated_launcher_name(&bat.file_name()?.to_string_lossy())
        || !path.is_absolute()
        || !matches!(fs::symlink_metadata(&path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        || !root
            .join(paths::relative_path(&recipe.target_exe).ok()?)
            .is_file()
    {
        return None;
    }
    let parts: Vec<_> = recipe
        .observed_root
        .trim_end_matches(['\\', '/'])
        .split(['\\', '/'])
        .collect();
    let mut matches = vec![];
    for depth in 1..=2.min(parts.len()) {
        let suffix = parts[parts.len() - depth..].join("/");
        let candidate = root.join(paths::relative_path(&suffix).ok()?);
        if let Ok(candidate) = checked_dir(&candidate) {
            if named(&candidate, "MTool.exe").is_some_and(|p| plain(&p).is_ok_and(|m| m.is_file()))
                && [&recipe.runtime, &recipe.injector, &recipe.loader]
                    .into_iter()
                    .all(|p| {
                        paths::relative_path(p)
                            .is_ok_and(|p| plain(&candidate.join(p)).is_ok_and(|m| m.is_file()))
                    })
            {
                matches.push(candidate);
            }
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
}
pub(super) fn detect(root: &Path, stop: &dyn Fn() -> bool) -> Vec<MtoolBundle> {
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
        if !named(tool, "MTool.exe").is_some_and(|p| plain(&p).is_ok_and(|m| m.is_file())) {
            continue;
        }
        let Ok(relative) = tool.strip_prefix(&root) else {
            continue;
        };
        if relative.components().count() > 2 {
            continue;
        }
        let Some(first) = relative.components().next() else {
            continue;
        };
        let folder = first.as_os_str().to_string_lossy().to_string();
        let associated = references
            .iter()
            .filter(|(_, path)| path == tool)
            .map(|(bat, _)| bat.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        let bundle = MtoolBundle {
            folder: folder.clone(),
            bats: associated,
            // Retain these serialized fields for older clients and journals.
            safe: true,
            reason: "已发现随包 MTool；勾选后回收整个一级目录".into(),
        };
        if let Some(previous) = result.insert(folder.clone(), bundle) {
            result.get_mut(&folder).unwrap().bats.extend(previous.bats);
        }
    }
    let mut bundles: Vec<_> = result.into_values().collect();
    // The two explicitly supported companion removal scripts are recycled, never interpreted.
    if !bundles.is_empty() {
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
    if bundles.is_empty() {
        return Err(invalid("没有确认随包 MTool，请取消清理"));
    }
    let selected = bundles.to_vec();
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
        let snapshot = snapshot(&root.join(path), &|| job.stop())?;
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
            .any(|b| b.folder == target.path || b.bats.contains(&target.path))
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
            Ok(())
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
                // Recheck only recorded anchors. Old journals retain their recorded identities too.
                let actual = capture(&path, target.snapshot.keys().cloned(), &|| job.stop())?;
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
                        mtool_strategy: None,
                        source: paths::path_text(&self.source).unwrap(),
                        title: "Game v2".into(),
                        target_name: "Game v2".into(),
                        version: "v2".into(),
                        engine: "RPG Maker MZ".into(),
                        executable: executable.into(),
                        working_directory: None,
                        mtool_loader: None,
                        launch_arguments: None,
                        external_player: None,
                        mtool: false,
                        existing_id: existing,
                        new_override: true,
                        remember_association: false,
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
            let bundles = detect(&f.source, &|| false);
            assert_eq!(bundles.len(), 1);
            assert_eq!(bundles[0].folder, "辅助工具");
            assert_eq!(bundles[0].bats, vec!["与工具一同启动.bat"]);
            launcher(
                &f.source,
                if nested {
                    "辅助工具/client"
                } else {
                    "辅助工具"
                },
                "StartWithTool.bat",
            );
            assert!(detect(&f.source, &|| false)[0]
                .bats
                .contains(&"StartWithTool.bat".into()));
            fs::write(
                f.source.join("StartWithTool.bat"),
                b"@echo off\ndel Game.exe",
            )
            .unwrap();
            assert!(!detect(&f.source, &|| false)[0]
                .bats
                .contains(&"StartWithTool.bat".into()));
            fs::remove_file(f.source.join("与工具一同启动.bat")).unwrap();
            assert!(detect(&f.source, &|| false)[0].bats.is_empty());
        }
    }
    #[test]
    fn cleans_entire_first_level_directory_regardless_of_contents_or_package_signature() {
        for nested in [false, true] {
            let f = Fixture::new(nested);
            let tool = f.source.join(if nested {
                "辅助工具/client"
            } else {
                "辅助工具"
            });
            fs::remove_file(tool.join("package.json")).unwrap();
            fs::remove_dir_all(tool.join("loaders")).unwrap();
            fs::remove_file(f.source.join("与工具一同启动.bat")).unwrap();
            for relative in [
                "Game.exe",
                "www/data/System.json",
                "save/file.rmmzsave",
                "未知组件/readme.txt",
            ] {
                let file = f.source.join("辅助工具").join(relative);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, b"entire cleanup unit").unwrap();
            }
            if nested {
                package(&f.source.join("辅助工具/other"));
            }
            let bundles = detect(&f.source, &|| false);
            assert_eq!(bundles.len(), 1);
            assert_eq!(bundles[0].folder, "辅助工具");
            let id = f.plan(true, None, "Game.exe", false);
            assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
            f.apply(&id).unwrap();
            assert!(!f.root.join("Game v2/辅助工具").exists());
            assert!(f.root.join("Game v2/Game.exe").is_file());
        }
    }
    #[cfg(windows)]
    #[test]
    fn stale_generated_absolute_bat_is_associated_only_with_matching_local_package() {
        let f = Fixture::new(false);
        let bat = f.source.join("与工具一同启动.bat");
        let stale = f._temp.path().join("old/辅助工具");
        let content = fs::read_to_string(&bat)
            .unwrap()
            .replace("%~dp0辅助工具", &stale.to_string_lossy());
        fs::write(&bat, &content).unwrap();
        assert_eq!(
            detect(&f.source, &|| false)[0].bats,
            vec!["与工具一同启动.bat"]
        );
        // An existing external package is not assumed to be bundled.
        package(&stale);
        assert!(detect(&f.source, &|| false)[0].bats.is_empty());
        fs::write(&bat, format!("{content}\ndel Game.exe")).unwrap();
        assert!(detect(&f.source, &|| false)[0].bats.is_empty());
    }
    #[test]
    fn cleanup_works_after_new_import_and_copy_preserves_launch_choice_and_is_idempotent() {
        for (nested, cross) in [(false, false), (true, false), (true, true)] {
            let f = Fixture::new(nested);
            let folder = if nested {
                "辅助工具/client"
            } else {
                "辅助工具"
            };
            for relative in [
                "gameLib/old.gljson",
                "gameSaveBackup/other-game/save/file.rmmzsave",
                "loaders/Wolf/Game.exe",
                "www/client.js",
            ] {
                let path = f.source.join(folder).join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, b"tool-owned data").unwrap();
            }
            for name in [
                "从游戏中移除工具文件.bat",
                "Remove Tool files from game.bat",
            ] {
                fs::write(f.source.join(name), b"never execute this script").unwrap();
            }
            let id = f.plan(true, None, "Game.exe", cross);
            assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
            let cleanup = f.store.get(&id).unwrap().items[0]
                .mtool_cleanup
                .clone()
                .unwrap();
            assert_eq!(cleanup.targets.len(), 4);
            assert!(cleanup.targets.iter().any(|t| t.path == "辅助工具"));
            // Directory contents may change after planning; the entire unit is still selected.
            fs::write(
                f.source.join(folder).join("package.json"),
                b"changed contents",
            )
            .unwrap();
            let ids = f.apply(&id).unwrap();
            let target = f.root.join("Game v2");
            assert!(target.join("Game.exe").is_file());
            assert!(target.join("普通启动.bat").is_file());
            assert!(!target.join("辅助工具").exists());
            assert!(!target.join("与工具一同启动.bat").exists());
            let game = f.db.lock().unwrap().game(&ids[0]).unwrap();
            assert_eq!(game.launch_type, "DIRECT");
            assert_eq!(game.main_executable.as_deref(), Some("Game.exe"));
            assert!(f.shared.join("MTool.exe").is_file());
            assert!(!f.shared.join("gameLib").exists());
            assert!(!f.shared.join("gameSaveBackup").exists());
            for name in [
                "从游戏中移除工具文件.bat",
                "Remove Tool files from game.bat",
            ] {
                assert!(!target.join(name).exists());
            }
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
    fn opt_out_keeps_bundle_and_cleanup_is_independent_of_launch_and_shared_tool() {
        let f = Fixture::new(false);
        let id = f.plan(false, None, "Game.exe", false);
        let ids = f.apply(&id).unwrap();
        assert!(f.root.join("Game v2/辅助工具/MTool.exe").is_file());
        assert_eq!(
            f.db.lock().unwrap().game(&ids[0]).unwrap().launch_type,
            "DIRECT"
        );
        for executable in ["Game.exe", "辅助工具/MTool.exe"] {
            let f = Fixture::new(false);
            let mut settings = f.db.lock().unwrap().settings().unwrap();
            settings.mtool_root.clear();
            f.db.lock().unwrap().save_settings(settings).unwrap();
            fs::remove_dir_all(&f.shared).unwrap();
            let id = f.plan(true, None, executable, false);
            assert!(f.store.get(&id).unwrap().items[0].blockers.is_empty());
            let ids = f.apply(&id).unwrap();
            assert!(!f.root.join("Game v2/辅助工具").exists());
            let game = f.db.lock().unwrap().game(&ids[0]).unwrap();
            assert_eq!(game.launch_type, "DIRECT");
            assert_eq!(game.main_executable.as_deref(), Some(executable));
        }
    }
    #[test]
    fn recycling_failure_keeps_the_imported_game_and_can_be_retried() {
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
        // Older journals recorded every member. Preserve their recovery without a second tree scan.
        let mut legacy = f.store.get(&id).unwrap();
        let folder = legacy.items[0]
            .mtool_cleanup
            .as_mut()
            .unwrap()
            .targets
            .iter_mut()
            .find(|t| t.path == "辅助工具")
            .unwrap();
        for key in [
            "MTool.exe",
            "package.json",
            "loaders/inject.exe",
            "loaders/mzHook.dll",
        ] {
            folder.snapshot.insert(
                key.into(),
                stamp(&f.source.join("辅助工具").join(key)).unwrap(),
            );
        }
        f.store.save(&legacy).unwrap();
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
    fn root_runtime_is_not_a_cleanup_directory_and_inner_links_do_not_change_recognition() {
        let f = Fixture::new(false);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&f.shared, f.source.join("辅助工具/linked-content"))
                .unwrap();
            assert_eq!(detect(&f.source, &|| false)[0].folder, "辅助工具");
        }
        fs::remove_dir_all(f.source.join("辅助工具")).unwrap();
        pe(&f.source.join("MTool.exe"));
        assert!(detect(&f.source, &|| false).is_empty());
    }

    #[test]
    fn legacy_tasks_without_cleanup_fields_do_not_enable_cleanup_retroactively() {
        let f = Fixture::new(false);
        let id = f.plan(false, None, "Game.exe", false);
        let mut legacy = serde_json::to_value(f.store.get(&id).unwrap()).unwrap();
        legacy["items"][0]["selection"]
            .as_object_mut()
            .unwrap()
            .remove("cleanup_mtool");
        legacy["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("mtool_cleanup");
        let plan: Plan = serde_json::from_value(legacy).unwrap();
        assert!(!plan.items[0].selection.cleanup_mtool);
        assert!(plan.items[0].mtool_cleanup.is_none());
        f.store.save(&plan).unwrap();
        f.apply(&id).unwrap();
        assert!(f.root.join("Game v2/辅助工具/MTool.exe").is_file());
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
        assert_eq!(
            f.db.lock().unwrap().game(&id).unwrap().launch_type,
            "DIRECT"
        );
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
