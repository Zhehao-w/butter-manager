//! Batch imports share the shallow scanner. Only a reviewed, persisted plan moves files.
//! Journals describe this operation, never export or back up the library database.
use crate::db::Database;
use crate::domain::{Error, Game, RegistrationEntry, Result, ScanCandidate, Settings};
use crate::{jobs::Job, paths, scanner};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

#[path = "import_matching.rs"]
mod import_matching;
#[path = "updater.rs"]
mod updater;
#[cfg(test)]
#[path = "updater_test.rs"]
mod updater_test;
pub use updater::{UpdateData, VersionConfig, VersionHistory};
fn default_preserve() -> bool {
    true
}
#[cfg(test)]
thread_local! { static FAIL_CHECKPOINT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn stopped(job: &Job) -> Result<()> {
    if job.stop() {
        Err(invalid("已停止；成功项保留，未完成项可继续或撤回"))
    } else {
        Ok(())
    }
}
fn check_item_idle(paths: &[PathBuf], job: &Job) -> Result<()> {
    job.activity_phase("检查游戏运行状态");
    crate::runtime::ensure_processes_idle_controlled(paths, &|| job.stop())
}
fn check_stage_idle(paths: &[PathBuf], lightweight: bool) -> Result<()> {
    if lightweight {
        crate::runtime::ensure_processes_idle(paths)
    } else {
        crate::runtime::ensure_paths_idle(paths)
    }
}
fn overlaps(a: &Path, b: &Path) -> Result<bool> {
    let a = PathBuf::from(paths::path_key(a)?);
    let b = PathBuf::from(paths::path_key(b)?);
    Ok(a.starts_with(&b) || b.starts_with(&a))
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn checked_dir(path: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || scanner::is_link(&metadata) {
        return Err(invalid("目录不存在或是链接目录"));
    }
    Ok(dunce::canonicalize(path)?)
}
pub fn folder_name(name: &str) -> Result<()> {
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || name != name.trim()
        || name.ends_with('.')
        || name == "."
        || name == ".."
        || name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
        || name.to_ascii_lowercase().starts_with(".butter-import-")
        || [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&base.as_str())
    {
        return Err(invalid(
            "目标名称必须是一个有效文件夹名称，不能含路径或保留名称",
        ));
    }
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct Match {
    pub id: String,
    pub title: String,
    pub version: String,
    pub path: String,
    pub reason: String,
    pub auto_associate: bool,
}
pub fn matches(candidate: &ScanCandidate, games: &[Game]) -> Vec<Match> {
    let incoming = import_matching::Name::new(&candidate.suggested_title);
    let mut result = vec![];
    for game in games {
        let names = [&game.canonical_title, &game.display_title]
            .into_iter()
            .chain(game.aliases.iter());
        let folder = Path::new(&game.install_path)
            .file_name()
            .map(|value| value.to_string_lossy().into_owned());
        let names = names.map(String::as_str).chain(folder.as_deref());
        let best = names
            .filter_map(|name| incoming.compare(&import_matching::Name::new(name)))
            .max_by_key(|(score, _)| *score);
        if let Some((score, reason)) = best {
            result.push((
                score,
                Match {
                    id: game.id.clone(),
                    title: game.display_title.clone(),
                    version: game.current_version.clone(),
                    path: game.install_path.clone(),
                    reason: if candidate.engine == "QSP" && game.engine == "QSP" {
                        format!("{reason} · 同为 QSP")
                    } else {
                        reason.into()
                    },
                    auto_associate: score >= 95,
                },
            ));
        }
    }
    result.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    result.into_iter().map(|(_, matched)| matched).collect()
}

pub fn analyze(job: &Arc<Job>, sources: &[String], workers: usize) -> Result<()> {
    job.stage("分析待导入目录", sources.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, 4) {
            scope.spawn(|| loop {
                if job.stop() {
                    break;
                }
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(source) = sources.get(i) else { break };
                match scanner::analyze_quick_controlled(Path::new(source), &|| job.stop(), &|p| {
                    job.progress(source, p)
                }) {
                    Ok(candidate) => job.publish(candidate),
                    Err(e) => {
                        if let Ok(mut c) = scanner::pending_candidate(Path::new(source)) {
                            c.status = "error".into();
                            c.warnings.push(e.to_string());
                            job.publish(c);
                        }
                    }
                }
                job.completed_one(source);
            });
        }
    });
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Selection {
    pub source: String,
    pub title: String,
    pub target_name: String,
    pub version: String,
    pub engine: String,
    pub executable: String,
    /// Update-only overrides; absent values inherit the previous configuration.
    #[serde(default)]
    pub working_directory: Option<String>,
    /// An explicit empty string selects automatic loader detection.
    #[serde(default)]
    pub mtool_loader: Option<String>,
    #[serde(default)]
    pub external_player: Option<crate::domain::ExternalPlayer>,
    pub mtool: bool,
    pub existing_id: Option<String>,
    #[serde(default)]
    pub new_override: bool,
    #[serde(default = "default_preserve")]
    pub preserve_saves: bool,
    #[serde(default)]
    pub saves_confirmed: bool,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
struct Entry {
    path: String,
    directory: bool,
    bytes: u64,
    modified: u128,
}
fn inventory(root: &Path, job: &Job) -> Result<Vec<Entry>> {
    checked_dir(root)?;
    let mut entries = vec![];
    let mut queue = vec![root.to_path_buf()];
    while let Some(dir) = queue.pop() {
        stopped(job)?;
        job.progress(&root.display().to_string(), &dir);
        for entry in fs::read_dir(&dir)? {
            stopped(job)?;
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if scanner::is_link(&metadata) || (!metadata.is_file() && !metadata.is_dir()) {
                return Err(invalid(format!(
                    "特殊文件或链接需手工处理：{}",
                    path.display()
                )));
            }
            let relative =
                paths::path_text(path.strip_prefix(root).map_err(|_| invalid("目录越界"))?)?;
            paths::relative_path(&relative)?;
            entries.push(Entry {
                path: relative,
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
                        .map_err(|_| invalid("文件时间不可读"))?
                        .as_nanos()
                } else {
                    0
                },
            });
            if entries.len() > 1_000_000 {
                return Err(invalid("单个游戏超过文件数量上限"));
            }
            if metadata.is_dir() {
                queue.push(path);
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}
fn unchanged(root: &Path, expected: &[Entry], job: &Job) -> Result<()> {
    if inventory(root, job)? != expected {
        return Err(invalid("目录内容在生成计划后已变化；保留文件，请重新检查"));
    }
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Item {
    pub selection: Selection,
    pub target: String,
    pub bytes: u64,
    pub files: usize,
    pub state: String,
    #[serde(default)]
    completed_at_ms: Option<u64>,
    pub error: Option<String>,
    pub registered_id: Option<String>,
    pub cross_volume: bool,
    pub blockers: Vec<String>,
    #[serde(default)]
    pub update: Option<UpdateData>,
    #[serde(default)]
    basic_transfer: bool,
    candidate: ScanCandidate,
    #[serde(skip)]
    manifest: Arc<Vec<Entry>>,
    game_id: String,
    source_id: String,
    payload_id: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Plan {
    pub id: String,
    pub root: String,
    pub status: String,
    pub items: Vec<Item>,
    settings: Settings,
    root_id: String,
}
#[derive(Clone, Serialize)]
pub struct PlanView {
    pub id: String,
    pub root: String,
    pub status: String,
    pub items: Vec<ItemView>,
    pub recorded_at_ms: Option<u64>,
}
#[derive(Clone, Serialize)]
pub struct ItemView {
    pub selection: Selection,
    pub target: String,
    pub bytes: u64,
    pub files: usize,
    pub state: String,
    pub completed_at_ms: Option<u64>,
    pub error: Option<String>,
    pub registered_id: Option<String>,
    pub cross_volume: bool,
    pub blockers: Vec<String>,
    pub update: Option<UpdateData>,
}
impl Plan {
    fn view(&self) -> PlanView {
        PlanView {
            id: self.id.clone(),
            root: self.root.clone(),
            status: self.status.clone(),
            recorded_at_ms: None,
            items: self
                .items
                .iter()
                .map(|i| ItemView {
                    selection: i.selection.clone(),
                    target: i.target.clone(),
                    bytes: i.bytes,
                    files: i.files,
                    state: i.state.clone(),
                    completed_at_ms: i.completed_at_ms,
                    error: i.error.clone(),
                    registered_id: i.registered_id.clone(),
                    cross_volume: i.cross_volume,
                    blockers: i.blockers.clone(),
                    update: i.update.clone(),
                })
                .collect(),
        }
    }
}
pub struct ImportStore {
    directory: PathBuf,
    plans: Mutex<HashMap<String, Plan>>,
    recovery_issues: Vec<RecoveryIssue>,
}
#[derive(Clone, Serialize)]
pub struct RecoveryIssue {
    pub record: String,
    pub message: String,
}
impl ImportStore {
    pub fn open(directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&directory)?;
        let mut plans = HashMap::new();
        let mut recovery_issues = vec![];
        for file in fs::read_dir(&directory)? {
            let path = match file {
                Ok(file) => file.path(),
                Err(error) => {
                    recovery_issues.push(RecoveryIssue {
                        record: "导入记录文件夹".into(),
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            if path.extension().is_some_and(|e| e == "json") {
                let loaded = (|| -> Result<Plan> {
                    let metadata = fs::symlink_metadata(&path)?;
                    if scanner::is_link(&metadata) || !metadata.is_file() {
                        return Err(invalid("导入记录不能是链接或目录"));
                    }
                    if metadata.len() > 16 * 1024 * 1024 {
                        return Err(invalid("导入记录超过读取上限"));
                    }
                    let mut plan: Plan = serde_json::from_reader(BufReader::with_capacity(
                        64 * 1024,
                        File::open(&path)?,
                    ))
                    .map_err(|e| invalid(format!("导入记录无法读取：{}：{e}", path.display())))?;
                    uuid::Uuid::parse_str(&plan.id).map_err(|_| invalid("无效的导入记录"))?;
                    if path.file_stem().and_then(|s| s.to_str()) != Some(plan.id.as_str())
                        || !matches!(
                            plan.status.as_str(),
                            "preview"
                                | "running"
                                | "interrupted"
                                | "cancelled"
                                | "failed"
                                | "completed"
                                | "withdrawing"
                                | "withdrawn"
                        )
                    {
                        return Err(invalid("导入记录身份或状态无效"));
                    }
                    if plan.status == "running" {
                        plan.status = "interrupted".into();
                    }
                    for (index, item) in plan.items.iter_mut().enumerate() {
                        if !matches!(
                            item.state.as_str(),
                            "pending"
                                | "moving"
                                | "copying"
                                | "staged"
                                | "publishing"
                                | "registering"
                                | "cleanup"
                                | "removing_source"
                                | "finishing"
                                | "completed"
                                | "withdrawn"
                                | "update_snapshot"
                                | "update_copy"
                                | "update_ready"
                                | "update_isolate"
                                | "update_publish"
                                | "update_restore"
                                | "update_commit"
                                | "update_cleanup"
                                | "update_recycle"
                                | "update_finish"
                                | "update_returned"
                                | "update_returning"
                                | "rollback_snapshot"
                                | "rollback_retrieve"
                                | "rollback_copy"
                                | "rollback_restore"
                                | "rollback_isolate"
                                | "rollback_publish"
                                | "rollback_commit"
                                | "rollback_cleanup"
                                | "rolled_back"
                        ) {
                            return Err(invalid("导入项目状态无法识别"));
                        }
                        if item.blockers.is_empty()
                            && (item.update.is_some()
                                || !matches!(item.state.as_str(), "completed" | "withdrawn"))
                        {
                            let manifest_path =
                                directory.join(format!("{}-{index}.manifest", plan.id));
                            let metadata = fs::symlink_metadata(&manifest_path)?;
                            if scanner::is_link(&metadata) || !metadata.is_file() {
                                return Err(invalid("导入文件清单不能是链接或目录"));
                            }
                            if metadata.len() > 256 * 1024 * 1024 {
                                return Err(invalid("导入文件清单超过读取上限"));
                            }
                            item.manifest = Arc::new(
                                serde_json::from_reader(BufReader::with_capacity(
                                    64 * 1024,
                                    File::open(manifest_path)?,
                                ))
                                .map_err(|e| invalid(e.to_string()))?,
                            );
                            let mut seen = HashSet::new();
                            for entry in item.manifest.iter() {
                                let relative = paths::relative_path(&entry.path)?;
                                if !seen.insert(paths::path_key(&relative)?) {
                                    return Err(invalid("导入文件清单包含重复路径"));
                                }
                            }
                        }
                        updater::validate_record(&plan.id, &plan.root, index, item)?;
                    }
                    Ok(plan)
                })();
                match loaded {
                    Ok(plan) => {
                        plans.insert(plan.id.clone(), plan);
                    }
                    Err(error) => recovery_issues.push(RecoveryIssue {
                        record: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        message: error.to_string(),
                    }),
                }
            }
        }
        Ok(Self {
            directory,
            plans: Mutex::new(plans),
            recovery_issues,
        })
    }
    pub fn recovery_issues(&self) -> &[RecoveryIssue] {
        &self.recovery_issues
    }
    pub fn ensure_recovery_clear(&self) -> Result<()> {
        if !self.recovery_issues.is_empty() {
            return Err(invalid(
                "部分导入记录无法读取，文件已保留；请先在导入页查看恢复提示",
            ));
        }
        Ok(())
    }
    pub fn views(&self) -> Vec<PlanView> {
        let mut views = self
            .plans
            .lock()
            .unwrap()
            .values()
            .map(|plan| {
                let mut view = plan.view();
                view.recorded_at_ms =
                    fs::metadata(self.directory.join(format!("{}.json", plan.id)))
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map(|duration| duration.as_millis() as u64);
                view
            })
            .collect::<Vec<_>>();
        views.sort_by(|a, b| a.id.cmp(&b.id));
        let recycled = views
            .iter()
            .any(|plan| {
                plan.items
                    .iter()
                    .any(|item| item.update.is_some() && item.state == "completed")
            })
            .then(|| self.recycled_versions().ok())
            .flatten();
        for view in &mut views {
            for item in &mut view.items {
                if let Some(update) = &mut item.update {
                    if item.state != "completed" {
                        update.rollback_available = false;
                    } else if let Some(recycled) = &recycled {
                        update.rollback_available = recycled.contains_key(&update.old_id);
                    }
                }
            }
        }
        views
    }
    pub fn has_pending_files(&self) -> bool {
        !self.recovery_issues.is_empty()
            || self.plans.lock().unwrap().values().any(|p| {
                p.items.iter().any(|i| {
                    !matches!(
                        i.state.as_str(),
                        "pending" | "completed" | "withdrawn" | "rolled_back"
                    )
                })
            })
    }
    pub fn blocks_game(&self, game: &Game) -> bool {
        !self.recovery_issues.is_empty()
            || self.plans.lock().unwrap().values().any(|plan| {
                plan.items.iter().any(|item| {
                    !matches!(
                        item.state.as_str(),
                        "pending" | "completed" | "withdrawn" | "rolled_back"
                    ) && [item.target.as_str(), item.selection.source.as_str()]
                        .iter()
                        .any(|path| {
                            overlaps(Path::new(&game.install_path), Path::new(path)).unwrap_or(true)
                        })
                })
            })
    }
    pub fn get(&self, id: &str) -> Result<Plan> {
        self.plans
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("导入计划已失效"))
    }
    fn save(&self, plan: &Plan) -> Result<()> {
        uuid::Uuid::parse_str(&plan.id).map_err(|_| invalid("无效的导入记录"))?;
        let temp = self.directory.join(format!("{}.tmp", plan.id));
        let target = self.directory.join(format!("{}.json", plan.id));
        // Immutable inventories are written once. State checkpoints stay small even for large batches.
        for (index, item) in plan.items.iter().enumerate() {
            let path = self.directory.join(format!("{}-{index}.manifest", plan.id));
            if !item.manifest.is_empty()
                && !exists(&path)?
                && (item.update.is_some()
                    || !matches!(item.state.as_str(), "completed" | "withdrawn"))
            {
                let temp = path.with_extension("manifest.tmp");
                let mut file = File::create(&temp)?;
                serde_json::to_writer(&mut file, &item.manifest)
                    .map_err(|e| invalid(e.to_string()))?;
                file.sync_all()?;
                drop(file);
                fs::rename(temp, path)?;
            }
        }
        let mut file = File::create(&temp)?;
        serde_json::to_writer(&mut file, plan).map_err(|e| invalid(e.to_string()))?;
        file.sync_all()?;
        drop(file);
        fs::rename(temp, target)?;
        let mut saved = plan.clone();
        for (index, item) in saved.items.iter_mut().enumerate() {
            if item.update.is_none() && matches!(item.state.as_str(), "completed" | "withdrawn") {
                item.manifest = Arc::new(vec![]);
                let path = self.directory.join(format!("{}-{index}.manifest", plan.id));
                if exists(&path)? {
                    fs::remove_file(path)?;
                }
            }
        }
        self.plans.lock().unwrap().insert(plan.id.clone(), saved);
        Ok(())
    }
    pub fn discard_preview(&self, id: &str) -> Result<()> {
        self.ensure_recovery_clear()?;
        let plan = self.get(id)?;
        if plan.items.iter().any(|i| i.state != "pending") {
            return Err(invalid("已有文件操作，请使用撤回未完成项"));
        }
        fs::remove_file(self.directory.join(format!("{id}.json")))?;
        for index in 0..plan.items.len() {
            let path = self.directory.join(format!("{id}-{index}.manifest"));
            if exists(&path)? {
                fs::remove_file(path)?;
            }
        }
        self.plans.lock().unwrap().remove(id);
        Ok(())
    }
    pub fn prepare(
        &self,
        job: &Arc<Job>,
        settings: Settings,
        candidates: Vec<ScanCandidate>,
        selections: Vec<Selection>,
        games: &[Game],
    ) -> Result<()> {
        self.ensure_recovery_clear()?;
        let root = checked_dir(Path::new(&settings.game_root))?;
        let mut plan = Plan {
            id: job.id.clone(),
            root: paths::path_text(&root)?,
            status: "preview".into(),
            items: vec![],
            settings: settings.clone(),
            root_id: identity(&root)?,
        };
        let mut sources: Vec<PathBuf> = vec![];
        let mut targets = HashSet::new();
        job.stage("检查文件与生成导入更新计划", selections.len());
        for (mut selection, candidate) in selections.into_iter().zip(candidates) {
            stopped(job)?;
            let source = checked_dir(Path::new(&selection.source))?;
            let mut blockers = vec![];
            let existing = selection
                .existing_id
                .as_ref()
                .and_then(|id| games.iter().find(|g| &g.id == id));
            if let Some(game) = existing {
                selection.target_name = Path::new(&game.install_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_owned();
            }
            if selection.existing_id.is_some() && existing.is_none() {
                blockers.push("关联的游戏已不存在，请重新选择".into());
            }
            let name_ok = folder_name(&selection.target_name).is_ok();
            if !name_ok {
                blockers.push("目标文件夹名称无效".into());
            }
            let target = if name_ok {
                root.join(&selection.target_name)
            } else {
                root.join("无效目标")
            };
            if overlaps(&source, &root)? {
                blockers.push("来源与游戏库目录重叠；库内游戏请使用扫描入库".into());
            }
            for protected in [
                Some(self.directory.parent().unwrap().to_path_buf()),
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(Path::to_path_buf)),
                if settings.mtool_root.is_empty() {
                    None
                } else {
                    Some(dunce::canonicalize(&settings.mtool_root)?)
                },
            ]
            .into_iter()
            .flatten()
            {
                if overlaps(&source, &protected)? {
                    blockers.push("来源与应用资料、程序或公共 MTool 目录重叠".into());
                }
            }
            for other in &sources {
                if overlaps(&source, other)? {
                    blockers.push("批量来源目录重复或互相包含".into());
                }
            }
            sources.push(source.clone());
            if !targets.insert(paths::path_key(&target)?) {
                blockers.push("批量目标文件夹重名".into());
            }
            if existing.is_none()
                && (exists(&target)?
                    || games.iter().any(|g| {
                        paths::path_key(Path::new(&g.install_path)).ok()
                            == paths::path_key(&target).ok()
                    }))
            {
                blockers.push("目标目录或游戏库记录已存在；不会覆盖".into());
            }
            if games
                .iter()
                .any(|g| overlaps(&source, Path::new(&g.install_path)).unwrap_or(true))
            {
                blockers.push(
                    "来源已在游戏库中登记，或包含已登记目录；请使用扫描入库或目录关联".into(),
                );
            }
            if selection.existing_id.is_none()
                && !selection.new_override
                && !matches(&candidate, games).is_empty()
            {
                blockers.push("名称匹配到已有游戏；请先关联或明确选择改为新游戏".into());
            }
            if selection.title.trim().is_empty() {
                blockers.push("游戏名称不能为空".into());
            }
            if selection.mtool
                && !Path::new(&selection.executable)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                blockers.push("MTool 需要游戏 EXE；文档请使用直接启动".into());
            }
            if selection.engine == "QSP" && selection.external_player.is_none() {
                selection.external_player = crate::external_player::qsp_config(&candidate);
            }
            let launch_check = if let Some(config) = &selection.external_player {
                if selection.mtool {
                    blockers.push("QSP 外部播放器不能同时使用 MTool".into());
                }
                if config.game_file.is_none() {
                    blockers.push("请选择 QSP 主游戏文件".into());
                }
                selection.engine = "QSP".into();
                crate::external_player::validate(
                    &source,
                    if selection.executable.is_empty() {
                        None
                    } else {
                        Some(&selection.executable)
                    },
                    config,
                )
            } else {
                paths::launch_file(&source, &selection.executable).map(|_| ())
            };
            if let Err(e) = launch_check {
                blockers.push(e.to_string());
            }
            selection.version = if selection.version.trim().is_empty() || selection.version == "-" {
                "Unknown".into()
            } else {
                selection.version.trim().into()
            };
            selection.source = paths::path_text(&source)?;
            let manifest = if blockers.is_empty() {
                inventory(&source, job)?
            } else {
                vec![]
            };
            let bytes = manifest.iter().map(|e| e.bytes).sum();
            let files = manifest.iter().filter(|e| !e.directory).count();
            let cross_volume = volume(&source)? != volume(&root)?;
            let update = if let Some(game) = existing {
                let quarantine =
                    root.join(format!(".butter-import-{}-{}", plan.id, plan.items.len()));
                match self.prepare_update(
                    game,
                    &selection,
                    &settings,
                    games,
                    job,
                    (bytes, &quarantine),
                ) {
                    Ok(update) => Some(update),
                    Err(error) => {
                        blockers.push(error.to_string());
                        None
                    }
                }
            } else {
                None
            };
            plan.items.push(Item {
                basic_transfer: true,
                selection,
                target: paths::path_text(&target)?,
                bytes,
                files,
                state: "pending".into(),
                completed_at_ms: None,
                error: None,
                registered_id: None,
                cross_volume,
                blockers,
                update,
                candidate,
                manifest: Arc::new(manifest),
                game_id: existing
                    .map(|g| g.id.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                source_id: identity(&source)?,
                payload_id: None,
            });
            job.completed_one(&paths::path_text(&source)?);
        }
        let needed: u64 = plan
            .items
            .iter()
            .filter(|i| i.blockers.is_empty())
            .map(|i| {
                i.update
                    .as_ref()
                    .map_or(if i.cross_volume { i.bytes } else { 0 }, |u| {
                        u.required_bytes
                    })
            })
            .sum();
        if let Some(available) = free_space(&root)? {
            if available < needed {
                for item in &mut plan.items {
                    if item.cross_volume || item.update.is_some() {
                        item.blockers.push("目标磁盘可用空间不足".into());
                    }
                }
            }
        }
        stopped(job)?;
        self.save(&plan)
    }
    pub fn apply(
        &self,
        id: &str,
        job: &Arc<Job>,
        database: &Arc<Mutex<Database>>,
    ) -> Result<Vec<String>> {
        self.ensure_recovery_clear()?;
        let mut plan = self.get(id)?;
        let settings = database.lock().unwrap().settings()?;
        if settings.game_root != plan.settings.game_root
            || settings.mtool_root != plan.settings.mtool_root
            || settings.mtool_injector != plan.settings.mtool_injector
            || settings.mtool_runtime != plan.settings.mtool_runtime
        {
            return Err(invalid(
                "相关设置已改变，计划不能继续，请撤回未完成项并重新生成",
            ));
        }
        if plan.items.iter().any(|i| !i.blockers.is_empty()) {
            return Err(invalid("计划存在冲突，请返回调整，重新生成"));
        }
        job.stage("读取目录信息", plan.items.len());
        for index in 0..plan.items.len() {
            if plan.items[index].state == "pending"
                && plan.items[index].basic_transfer
                && plan.items[index].cross_volume
            {
                validate_paths(&plan, &plan.items[index], &self.directory)?;
                let entries = inventory(Path::new(&plan.items[index].selection.source), job)?;
                let path = self.directory.join(format!("{}-{index}.manifest", plan.id));
                let temporary = path.with_extension("manifest.tmp");
                let mut file = File::create(&temporary)?;
                serde_json::to_writer(&mut file, &entries)
                    .map_err(|error| invalid(error.to_string()))?;
                file.sync_all()?;
                drop(file);
                fs::rename(temporary, path)?;
                plan.items[index].bytes = entries.iter().map(|entry| entry.bytes).sum();
                plan.items[index].files = entries.iter().filter(|entry| !entry.directory).count();
                plan.items[index].manifest = Arc::new(entries);
                let source_bytes = plan.items[index].bytes;
                let preserve = plan.items[index].selection.preserve_saves;
                if let Some(update) = &mut plan.items[index].update {
                    if update.lightweight {
                        let saves = update
                            .saves
                            .iter()
                            .filter(|save| preserve && save.present)
                            .map(|save| save.bytes)
                            .sum::<u64>();
                        update.required_bytes =
                            source_bytes.saturating_add(saves.saturating_mul(2));
                    }
                }
            }
        }
        plan.status = "running".into();
        self.save(&plan)?;
        job.stage("导入与更新游戏", plan.items.len());
        job.batch_transfer(&plan.items.iter().map(|item| item.bytes).collect::<Vec<_>>());
        for index in 0..plan.items.len() {
            job.transfer_item(&plan.items[index].selection.title, plan.items[index].bytes);
            if matches!(
                plan.items[index].state.as_str(),
                "completed" | "withdrawn" | "rolled_back"
            ) {
                job.completed_one(&plan.items[index].target);
                continue;
            }
            let outcome = if plan.items[index].update.is_some() {
                self.update_one(&mut plan, index, job, database)
            } else {
                self.move_one(&mut plan, index, job, database)
            };
            if let Err(e) = outcome {
                if plan.items[index].update.is_some() {
                    if let Err(recovery) = self.restore_uncommitted(&mut plan, index, database) {
                        plan.items[index].error = Some(format!(
                            "{e}；自动恢复未完成：{recovery}。文件保留，请继续或撤回。"
                        ));
                    }
                }
                plan.status = if job.stop() { "cancelled" } else { "failed" }.into();
                if plan.items[index].error.is_none() {
                    plan.items[index].error = Some(e.to_string());
                }
                self.save(&plan)?;
                return Err(e);
            }
            job.completed_one(&plan.items[index].target);
        }
        plan.status = "completed".into();
        self.save(&plan)?;
        Ok(plan
            .items
            .iter()
            .filter_map(|i| i.registered_id.clone())
            .collect())
    }
    fn checkpoint(&self, plan: &mut Plan, index: usize, state: &str) -> Result<()> {
        plan.items[index].state = state.into();
        if state == "completed" && plan.items[index].completed_at_ms.is_none() {
            plan.items[index].completed_at_ms = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as u64);
        }
        plan.items[index].error = None;
        self.save(plan)?;
        #[cfg(test)]
        if FAIL_CHECKPOINT.with(|failure| {
            let mut failure = failure.borrow_mut();
            if failure.as_deref() == Some(state) {
                failure.take();
                true
            } else {
                false
            }
        }) {
            return Err(invalid("测试夹具：检查点后中断"));
        }
        Ok(())
    }
    fn move_one(
        &self,
        plan: &mut Plan,
        index: usize,
        job: &Job,
        database: &Arc<Mutex<Database>>,
    ) -> Result<()> {
        stopped(job)?;
        let item = plan.items[index].clone();
        let source = PathBuf::from(&item.selection.source);
        let target = PathBuf::from(&item.target);
        let container = stage_path(plan, index);
        let payload = container.join("payload");
        validate_paths(plan, &item, &self.directory)?;
        check_item_idle(&[source.clone(), target.clone(), payload.clone()], job)?;
        job.progress(&item.selection.source, &source);
        if item.state == "pending" {
            if !item.basic_transfer {
                unchanged(&source, &item.manifest, job)?;
            }
            if exists(&target)? || database.lock().unwrap().registered_id(&target)?.is_some() {
                return Err(invalid("目标已存在，拒绝覆盖"));
            }
            if !exists(&container)? {
                fs::create_dir(&container)?;
            }
            if !exists(&container.join("owner"))? {
                if fs::read_dir(&container)?.next().is_some() {
                    return Err(invalid("暂存目录包含未知文件"));
                }
                let mut marker = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(container.join("owner"))?;
                marker.write_all(plan.id.as_bytes())?;
                marker.sync_all()?;
            } else {
                verify_owner(&container, &plan.id)?;
            }
            self.checkpoint(plan, index, "moving")?;
        }
        if item.state == "finishing" {
            require_identity(&target, item.payload_id.as_deref())?;
            if exists(&container)? {
                finish_container(&container, &plan.id)?;
            }
            self.checkpoint(plan, index, "completed")?;
            return Ok(());
        }
        verify_owner(&container, &plan.id)?;
        let state = plan.items[index].state.clone();
        if state == "moving" {
            job.transfer_phase("移动游戏目录", 0.05);
            if exists(&source)? && !exists(&payload)? {
                if !item.basic_transfer {
                    unchanged(&source, &item.manifest, job)?;
                }
                if item.cross_volume {
                    self.checkpoint(plan, index, "copying")?;
                } else {
                    check_stage_idle(std::slice::from_ref(&source), item.basic_transfer)?;
                    rename_new(&source, &payload)?;
                    job.transfer_phase("游戏目录已就位", 0.8);
                    self.checkpoint(plan, index, "staged")?;
                }
            } else if !exists(&source)? && exists(&payload)? {
                require_identity(&payload, Some(&item.source_id))?;
                if !item.basic_transfer {
                    unchanged(&payload, &item.manifest, job)?;
                }
                self.checkpoint(plan, index, "staged")?;
            } else {
                return Err(invalid("来源与暂存目录状态不明确，保留文件，需人工检查"));
            }
        }
        if plan.items[index].state == "copying" {
            job.copy_phase("跨盘复制", 0.05, 0.8);
            if !item.basic_transfer {
                unchanged(&source, &item.manifest, job)?;
            }
            if let Some(available) = free_space(Path::new(&plan.root))? {
                if available < item.bytes {
                    return Err(invalid("复制所需空间不足"));
                }
            }
            if item.basic_transfer {
                copy_tree_basic(&source, &payload, &item.manifest, job)?;
                verify_sizes(&payload, &item.manifest, job)?;
            } else {
                copy_tree(&source, &payload, &item.manifest, job)?;
            }
            if !item.basic_transfer {
                unchanged(&source, &item.manifest, job)?;
            }
            if !item.basic_transfer {
                verify_copy(&source, &payload, &item.manifest, job)?;
            }
            self.checkpoint(plan, index, "staged")?;
        }
        if plan.items[index].state == "staged" {
            job.transfer_phase("检查复制完成", 0.82);
            if item.cross_volume {
                if !item.basic_transfer {
                    unchanged(&source, &item.manifest, job)?;
                }
                if item.state == "staged" && !item.basic_transfer {
                    verify_copy(&source, &payload, &item.manifest, job)?;
                }
            } else {
                if !item.basic_transfer {
                    unchanged(&payload, &item.manifest, job)?;
                }
            }
            stopped(job)?;
            plan.items[index].payload_id = Some(identity(&payload)?);
            self.checkpoint(plan, index, "publishing")?;
        }
        if plan.items[index].state == "publishing" {
            if exists(&payload)? && !exists(&target)? {
                check_stage_idle(std::slice::from_ref(&payload), item.basic_transfer)?;
                rename_new(&payload, &target)?;
            } else if exists(&payload)? || !exists(&target)? {
                return Err(invalid("目标冲突，保留暂存内容"));
            }
            require_identity(&target, plan.items[index].payload_id.as_deref())?;
            // After a crash, a published directory is only accepted with its exact recorded content.
            if item.cross_volume {
                if !item.basic_transfer {
                    unchanged(&source, &item.manifest, job)?;
                }
                if item.state == "publishing" && !item.basic_transfer {
                    verify_copy(&source, &target, &item.manifest, job)?;
                }
            } else {
                if !item.basic_transfer {
                    unchanged(&target, &item.manifest, job)?;
                }
            }
            self.checkpoint(plan, index, "registering")?;
        }
        if plan.items[index].state == "registering" {
            job.transfer_phase("保存游戏资料", 0.93);
            require_identity(&target, plan.items[index].payload_id.as_deref())?;
            if !item.cross_volume {
                if !item.basic_transfer {
                    unchanged(&target, &item.manifest, job)?;
                }
            } else {
                if !item.basic_transfer {
                    unchanged(&source, &item.manifest, job)?;
                }
                if item.state == "registering" && !item.basic_transfer {
                    verify_copy(&source, &target, &item.manifest, job)?;
                }
            }
            // The commit is deliberately non-cancellable; a committed game is never rolled back as cancelled.
            let mut db = database.lock().unwrap();
            let mut candidate = item.candidate.clone();
            candidate.install_path = item.target.clone();
            candidate.suggested_title = item.selection.title.clone();
            candidate.engine = item.selection.engine.clone();
            candidate.mtool_detected = item.selection.mtool;
            let entry = RegistrationEntry {
                candidate,
                executable: if item.selection.executable.is_empty() {
                    None
                } else {
                    Some(item.selection.executable.clone())
                },
                external_player: item.selection.external_player.clone(),
                version: item.selection.version.clone(),
                version_source: "manual".into(),
                working_directory: if item.selection.external_player.is_some() {
                    ".".into()
                } else {
                    paths::executable_directory(Some(&item.selection.executable))?
                },
            };
            if let Some(config) = &item.selection.external_player {
                crate::external_player::validate(&target, entry.executable.as_deref(), config)?;
            } else {
                paths::launch_file(&target, &item.selection.executable)?;
            }
            plan.items[index].registered_id = Some(db.register_import(entry, &item.game_id)?);
            drop(db);
            self.checkpoint(plan, index, "cleanup")?;
        }
        if plan.items[index].state == "cleanup" {
            require_identity(&target, plan.items[index].payload_id.as_deref())?;
            if item.cross_volume && exists(&source)? {
                // Never delete a source changed since preview or a target that no longer matches.
                if !item.basic_transfer {
                    unchanged(&source, &item.manifest, job)?;
                }
                if item.state == "cleanup" && !item.basic_transfer {
                    verify_copy(&source, &target, &item.manifest, job)?;
                }
                self.checkpoint(plan, index, "removing_source")?;
            } else {
                self.checkpoint(plan, index, "finishing")?;
            }
        }
        if plan.items[index].state == "removing_source" {
            job.transfer_phase("清理跨盘来源", 0.95);
            check_stage_idle(&[source.clone(), target.clone()], item.basic_transfer)?;
            require_identity(&target, plan.items[index].payload_id.as_deref())?;
            // Delete only recorded files that still equal the committed target, one at a time.
            if item.basic_transfer {
                updater::remove_source_basic(&source, &target, &container, &item, job)?;
            } else {
                remove_source(&source, &target, &item.manifest, job)?;
            }
            self.checkpoint(plan, index, "finishing")?;
        }
        if plan.items[index].state == "finishing" {
            job.transfer_phase("完成导入", 0.99);
            require_identity(&target, plan.items[index].payload_id.as_deref())?;
            finish_container(&container, &plan.id)?;
            self.checkpoint(plan, index, "completed")?;
        }
        Ok(())
    }
    pub fn withdraw(&self, id: &str, job: &Job, database: &Arc<Mutex<Database>>) -> Result<()> {
        self.ensure_recovery_clear()?;
        let mut plan = self.get(id)?;
        plan.status = "withdrawing".into();
        self.save(&plan)?;
        job.stage("撤回未登记项", plan.items.len());
        for index in 0..plan.items.len() {
            stopped(job)?;
            let item = plan.items[index].clone();
            if matches!(
                item.state.as_str(),
                "completed" | "withdrawn" | "rolled_back"
            ) {
                job.completed_one(&item.target);
                continue;
            }
            if item.update.is_some() {
                self.withdraw_update(&mut plan, index, database)?;
                job.completed_one(&item.target);
                continue;
            }
            let source = Path::new(&item.selection.source);
            let target = Path::new(&item.target);
            let container = stage_path(&plan, index);
            let payload = container.join("payload");
            if let Some(id) = database.lock().unwrap().registered_id(target)? {
                if id != item.game_id {
                    return Err(invalid("目标被其他记录占用，不能撤回"));
                }
                require_identity(Path::new(&plan.root), Some(&plan.root_id))?;
                require_identity(target, item.payload_id.as_deref())?;
                if exists(&container)? {
                    finish_container(&container, &plan.id)?;
                }
                plan.items[index].registered_id = Some(id);
                plan.items[index].state = "completed".into();
                plan.items[index].error =
                    Some("已保留登记成功的游戏，以及剩余来源文件；不再清理来源".into());
                self.save(&plan)?;
                job.completed_one(&item.target);
                continue;
            }
            validate_paths(&plan, &item, &self.directory)?;
            if exists(&container)? {
                if !exists(&container.join("owner"))? && fs::read_dir(&container)?.next().is_none()
                {
                    unchanged(source, &item.manifest, job)?;
                    fs::remove_dir(&container)?;
                    plan.items[index].state = "withdrawn".into();
                    self.save(&plan)?;
                    job.completed_one(&item.target);
                    continue;
                }
                verify_owner(&container, &plan.id)?;
                let location = if exists(&payload)? {
                    payload.clone()
                } else if matches!(item.state.as_str(), "publishing" | "registering")
                    && exists(target)?
                {
                    require_identity(target, item.payload_id.as_deref())?;
                    target.to_path_buf()
                } else {
                    payload.clone()
                };
                if exists(&location)? {
                    check_stage_idle(
                        &[source.to_path_buf(), location.clone()],
                        item.basic_transfer,
                    )?;
                    if item.cross_volume {
                        unchanged(source, &item.manifest, job)?;
                        remove_partial_copy(source, &location, &item.manifest, job)?;
                    } else {
                        unchanged(&location, &item.manifest, job)?;
                        if exists(source)? {
                            return Err(invalid("来源已被占用，不能撤回移动"));
                        }
                        rename_new(&location, source)?;
                    }
                }
                fs::remove_file(container.join("owner"))?;
                fs::remove_dir(&container)?;
            }
            plan.items[index].state = "withdrawn".into();
            self.save(&plan)?;
            job.completed_one(&item.target);
        }
        plan.status = if plan
            .items
            .iter()
            .all(|i| matches!(i.state.as_str(), "completed" | "withdrawn" | "rolled_back"))
        {
            "withdrawn"
        } else {
            "cancelled"
        }
        .into();
        self.save(&plan)
    }
}

fn stage_path(plan: &Plan, index: usize) -> PathBuf {
    Path::new(&plan.root).join(format!(".butter-import-{}-{index}", plan.id))
}
fn verify_owner(container: &Path, id: &str) -> Result<()> {
    checked_dir(container)?;
    let marker = container.join("owner");
    if scanner::is_link(&fs::symlink_metadata(&marker)?) || fs::read_to_string(marker)? != id {
        return Err(invalid("暂存目录不属于本次导入，拒绝处理"));
    }
    Ok(())
}
fn finish_container(container: &Path, id: &str) -> Result<()> {
    checked_dir(container)?;
    if exists(&container.join("owner"))? {
        verify_owner(container, id)?;
        // Unknown contents cause failure before even removing the ownership marker.
        if fs::read_dir(container)?.any(|e| e.map_or(true, |e| e.file_name() != "owner")) {
            return Err(invalid("暂存目录仍有文件，保留并停止清理"));
        }
        fs::remove_file(container.join("owner"))?;
    }
    fs::remove_dir(container)?;
    Ok(())
}
fn validate_paths(plan: &Plan, item: &Item, journal: &Path) -> Result<()> {
    let root = checked_dir(Path::new(&plan.root))?;
    require_identity(&root, Some(&plan.root_id))?;
    folder_name(&item.selection.target_name)?;
    if paths::path_key(&root.join(&item.selection.target_name))?
        != paths::path_key(Path::new(&item.target))?
    {
        return Err(invalid("计划目标路径不一致"));
    }
    let source = Path::new(&item.selection.source);
    if exists(source)? {
        require_identity(source, Some(&item.source_id))?;
    }
    let parent = source
        .parent()
        .ok_or_else(|| invalid("来源不能是磁盘根目录"))?;
    if paths::path_key(
        &dunce::canonicalize(parent)?.join(source.file_name().ok_or_else(|| invalid("无效来源"))?),
    )? != paths::path_key(source)?
        || overlaps(source, &root)?
        || overlaps(source, journal.parent().unwrap())?
    {
        return Err(invalid("来源路径已变化或越界"));
    }
    for entry in item.manifest.iter() {
        paths::relative_path(&entry.path)?;
    }
    Ok(())
}
#[cfg(windows)]
pub(crate) fn identity(path: &Path) -> Result<String> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
    };
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the file owns a live handle and the output has the exact Windows ABI layout.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(format!(
        "{}:{}:{}",
        information.dwVolumeSerialNumber, information.nFileIndexHigh, information.nFileIndexLow
    ))
}
#[cfg(not(windows))]
pub(crate) fn identity(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path)?;
    Ok(format!("{}:{}", m.dev(), m.ino()))
}
fn require_identity(path: &Path, expected: Option<&str>) -> Result<()> {
    checked_dir(path)?;
    if Some(identity(path)?.as_str()) != expected {
        return Err(invalid("目标目录已被更换，保留文件并停止操作"));
    }
    Ok(())
}
#[cfg(windows)]
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
#[cfg(windows)]
fn volume(path: &Path) -> Result<String> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumePathNameW;
    let mut buffer = vec![0u16; 32768];
    // SAFETY: both buffers are live and sized; the input is NUL terminated.
    if unsafe {
        GetVolumePathNameW(
            wide(path).as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(String::from_utf16_lossy(
        &buffer[..buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len())],
    )
    .to_lowercase())
}
#[cfg(not(windows))]
fn volume(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::metadata(path)?.dev().to_string())
}
#[cfg(windows)]
fn free_space(path: &Path) -> Result<Option<u64>> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut available = 0;
    // SAFETY: output points to a live u64 and input is NUL terminated; unused outputs are nullable.
    if unsafe {
        GetDiskFreeSpaceExW(
            wide(path).as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(Some(available))
}
#[cfg(not(windows))]
fn free_space(_: &Path) -> Result<Option<u64>> {
    Ok(None)
}
fn rename_new(source: &Path, target: &Path) -> Result<()> {
    if exists(target)? {
        return Err(invalid("目标已存在，拒绝覆盖"));
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        // SAFETY: live NUL-terminated paths. No REPLACE_EXISTING or COPY_ALLOWED: fail on collisions/cross-volume.
        if unsafe {
            MoveFileExW(
                wide(source).as_ptr(),
                wide(target).as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    fs::rename(source, target)?;
    Ok(())
}
fn copy_tree(source: &Path, target: &Path, manifest: &[Entry], job: &Job) -> Result<()> {
    copy_tree_mode(source, target, manifest, job, true)
}
fn copy_tree_basic(source: &Path, target: &Path, manifest: &[Entry], job: &Job) -> Result<()> {
    copy_tree_mode(source, target, manifest, job, false)
}
fn copy_tree_mode(
    source: &Path,
    target: &Path,
    manifest: &[Entry],
    job: &Job,
    verify_prefix: bool,
) -> Result<()> {
    if !exists(target)? {
        fs::create_dir(target)?;
    }
    checked_dir(target)?;
    let total = manifest.iter().map(|e| e.bytes).sum();
    let mut done = 0;
    for entry in manifest.iter().filter(|e| e.directory) {
        stopped(job)?;
        fs::create_dir_all(target.join(&entry.path))?;
        checked_dir(&target.join(&entry.path))?;
    }
    for entry in manifest.iter().filter(|e| !e.directory) {
        stopped(job)?;
        let from = source.join(&entry.path);
        let to = target.join(&entry.path);
        job.progress(&source.display().to_string(), &from);
        if exists(&to)? {
            if scanner::is_link(&fs::symlink_metadata(&to)?) {
                return Err(invalid("暂存文件是链接"));
            }
            // Legacy journals require a matching prefix; lightweight copies only check size.
            if verify_prefix {
                equal_prefix(&from, &to, job)?;
            } else if fs::metadata(&to)?.len() > entry.bytes {
                return Err(invalid("暂存文件超过计划大小，保留并停止"));
            }
        }
        if scanner::is_link(&fs::symlink_metadata(&from)?) {
            return Err(invalid("来源文件不能是链接"));
        }
        let mut input = locked_read(&from)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&to)?;
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            stopped(job)?;
            let size = input.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            output.write_all(&buffer[..size])?;
            done += size as u64;
            job.transfer("复制文件", done, total);
        }
        output.sync_all()?;
        output.set_times(fs::FileTimes::new().set_modified(input.metadata()?.modified()?))?;
    }
    Ok(())
}
fn verify_sizes(target: &Path, manifest: &[Entry], job: &Job) -> Result<()> {
    let actual = inventory(target, job)?;
    let expected: HashMap<_, _> = manifest
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    if actual.len() != expected.len()
        || actual.iter().any(|entry| {
            expected.get(entry.path.as_str()).is_none_or(|original| {
                original.directory != entry.directory || original.bytes != entry.bytes
            })
        })
    {
        return Err(invalid("复制未完成，文件数量或大小不一致；来源保留"));
    }
    Ok(())
}
fn equal_prefix(source: &Path, target: &Path, job: &Job) -> Result<()> {
    let mut source = locked_read(source)?;
    let mut target = locked_read(target)?;
    compare_streams(&mut source, &mut target, job)
}
fn locked_read(path: &Path) -> Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};
        Ok(OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .open(path)?)
    }
    #[cfg(not(windows))]
    {
        Ok(File::open(path)?)
    }
}
fn compare_streams(source: &mut File, target: &mut File, job: &Job) -> Result<()> {
    let total = target.metadata()?.len();
    let mut done = 0;
    let mut a = vec![0; 1024 * 1024];
    let mut b = vec![0; 1024 * 1024];
    loop {
        stopped(job)?;
        let size = target.read(&mut b)?;
        if size == 0 {
            break;
        }
        source.read_exact(&mut a[..size])?;
        if a[..size] != b[..size] {
            return Err(invalid("暂存或目标文件内容发生变化，保留文件"));
        }
        done += size as u64;
        job.transfer("校验当前文件内容", done, total);
    }
    Ok(())
}
fn delete_verified(source: &Path, target: &Path, job: &Job) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::GENERIC_READ;
        use windows_sys::Win32::Storage::FileSystem::{
            FileDispositionInfo, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_INFO,
            FILE_SHARE_READ,
        };
        // Lock the exact source object against writes/renames through validation and deletion.
        let mut input = OpenOptions::new()
            .access_mode(GENERIC_READ | DELETE)
            .share_mode(FILE_SHARE_READ)
            .open(source)?;
        let mut output = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(target)?;
        let a = input.metadata()?;
        let b = output.metadata()?;
        if scanner::is_link(&a)
            || scanner::is_link(&b)
            || !a.is_file()
            || !b.is_file()
            || a.len() != b.len()
        {
            return Err(invalid("来源或目标已变化，不移除来源"));
        }
        compare_streams(&mut input, &mut output, job)?;
        let information = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: live owned handle and exact ABI struct. Delete the verified open file, never a replaced path.
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
        equal_file(source, target, job)?;
        fs::remove_file(source)?;
    }
    Ok(())
}
fn equal_file(source: &Path, target: &Path, job: &Job) -> Result<()> {
    let a = fs::symlink_metadata(source)?;
    let b = fs::symlink_metadata(target)?;
    if scanner::is_link(&a)
        || scanner::is_link(&b)
        || !a.is_file()
        || !b.is_file()
        || a.len() != b.len()
    {
        return Err(invalid("文件结构或大小变化，停止移动"));
    }
    equal_prefix(source, target, job)
}
fn verify_copy(source: &Path, target: &Path, manifest: &[Entry], job: &Job) -> Result<()> {
    let actual = inventory(target, job)?;
    if actual.len() != manifest.len()
        || actual
            .iter()
            .zip(manifest)
            .any(|(a, b)| a.path != b.path || a.bytes != b.bytes || a.directory != b.directory)
    {
        return Err(invalid("复制后的文件清单不一致"));
    }
    let total = manifest.iter().map(|e| e.bytes).sum();
    let mut done = 0;
    for entry in manifest.iter().filter(|e| !e.directory) {
        stopped(job)?;
        equal_file(&source.join(&entry.path), &target.join(&entry.path), job)?;
        done += entry.bytes;
        job.transfer("校验内容", done, total);
    }
    Ok(())
}
fn remove_source(source: &Path, target: &Path, manifest: &[Entry], job: &Job) -> Result<()> {
    if !exists(source)? {
        return Ok(());
    }
    let actual = inventory(source, job)?;
    if actual.iter().any(|a| !manifest.contains(a)) {
        return Err(invalid("来源出现新增或修改的文件，不继续清理"));
    }
    for entry in manifest.iter().filter(|e| !e.directory) {
        stopped(job)?;
        let path = source.join(&entry.path);
        if exists(&path)? {
            delete_verified(&path, &target.join(&entry.path), job)?;
        }
    }
    let mut dirs = manifest.iter().filter(|e| e.directory).collect::<Vec<_>>();
    dirs.sort_by_key(|e| std::cmp::Reverse(e.path.len()));
    for entry in dirs {
        let path = source.join(&entry.path);
        if exists(&path)? {
            fs::remove_dir(path)?;
        }
    }
    fs::remove_dir(source)?;
    Ok(())
}
fn remove_partial_copy(source: &Path, target: &Path, manifest: &[Entry], job: &Job) -> Result<()> {
    let actual = inventory(target, job)?;
    for entry in &actual {
        if !manifest.iter().any(|e| {
            e.path == entry.path
                && e.directory == entry.directory
                && (e.directory || e.bytes >= entry.bytes)
        }) {
            return Err(invalid("暂存出现未知内容，不撤回"));
        }
    }
    for entry in actual.iter().filter(|e| !e.directory) {
        equal_prefix(&source.join(&entry.path), &target.join(&entry.path), job)?;
    }
    for entry in actual.iter().filter(|e| !e.directory) {
        stopped(job)?;
        fs::remove_file(target.join(&entry.path))?;
    }
    let mut dirs = actual.iter().filter(|e| e.directory).collect::<Vec<_>>();
    dirs.sort_by_key(|e| std::cmp::Reverse(e.path.len()));
    for entry in dirs {
        fs::remove_dir(target.join(&entry.path))?;
    }
    fs::remove_dir(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_qsp_engine_only_strengthens_an_existing_name_match() {
        let temp = tempfile::tempdir().unwrap();
        let mut candidate = scanner::pending_candidate(temp.path()).unwrap();
        candidate.suggested_title = "Game A".into();
        candidate.engine = "QSP".into();
        let mut game = crate::maintenance::tests::fixture_game(temp.path());
        game.engine = "QSP".into();
        assert!(matches(&candidate, &[game.clone()]).is_empty());
        game.aliases.push("Game A".into());
        let matched = matches(&candidate, &[game]);
        assert_eq!(matched.len(), 1);
        assert!(matched[0].reason.contains("同为 QSP"));
    }
    #[test]
    fn qsp_import_blocks_ambiguous_files_until_explicit_choice_without_moving() {
        let f = Fixture::new();
        let source = f.source.join("QSP game");
        fs::create_dir(&source).unwrap();
        for file in ["game.qsp", "mod.qsp", "qspgui.exe"] {
            fs::write(source.join(file), b"fixture").unwrap();
        }
        let settings = f.db.lock().unwrap().settings().unwrap();
        let candidate = scanner::analyze_directory(&source).unwrap();
        let mut selection = Selection {
            source: candidate.install_path.clone(),
            title: candidate.suggested_title.clone(),
            target_name: "QSP game".into(),
            version: "Unknown".into(),
            engine: "QSP".into(),
            executable: "qspgui.exe".into(),
            external_player: crate::external_player::qsp_config(&candidate),
            working_directory: None,
            mtool_loader: None,
            mtool: false,
            existing_id: None,
            new_override: false,
            preserve_saves: true,
            saves_confirmed: false,
        };
        let job = f
            .tasks
            .begin("import_plan", settings.game_root.clone())
            .unwrap();
        f.store
            .prepare(
                &job,
                settings.clone(),
                vec![candidate.clone()],
                vec![selection.clone()],
                &[],
            )
            .unwrap();
        job.finish(Ok(vec![]));
        assert!(f.store.get(&job.id).unwrap().items[0]
            .blockers
            .iter()
            .any(|message| message.contains("请选择 QSP")));
        assert!(f.apply(&job.id).is_err());
        assert!(source.join("mod.qsp").is_file());
        assert!(!f.root.join("QSP game").exists());
        assert!(f.db.lock().unwrap().games().unwrap().is_empty());
        selection.external_player.as_mut().unwrap().game_file = Some("mod.qsp".into());
        let job = f
            .tasks
            .begin("import_plan", settings.game_root.clone())
            .unwrap();
        f.store
            .prepare(&job, settings, vec![candidate], vec![selection], &[])
            .unwrap();
        job.finish(Ok(vec![]));
        let id = f.apply(&job.id).unwrap().remove(0);
        assert_eq!(
            f.db.lock()
                .unwrap()
                .game(&id)
                .unwrap()
                .external_player
                .unwrap()
                .game_file
                .as_deref(),
            Some("mod.qsp")
        );
    }
    #[test]
    fn qsp_import_preserves_local_runtime_custom_resources_and_allows_missing_player() {
        let f = Fixture::new();
        let sources = [f.source.join("魔法少女"), f.source.join("No player")];
        for source in &sources {
            fs::create_dir_all(source.join("standalone_content")).unwrap();
            fs::write(source.join("彼女の冒険.qsp"), b"game file").unwrap();
        }
        for file in [
            "QuestNavigator.exe",
            "config.xml",
            "game.css",
            "game.js",
            "standalone_content/resource.dat",
            "save.dat",
        ] {
            fs::write(sources[0].join(file), b"preserved bytes").unwrap();
        }
        let settings = f.db.lock().unwrap().settings().unwrap();
        let candidates = sources
            .iter()
            .map(|source| scanner::analyze_directory(source).unwrap())
            .collect::<Vec<_>>();
        let selections = candidates
            .iter()
            .map(|c| Selection {
                source: c.install_path.clone(),
                title: c.suggested_title.clone(),
                target_name: leaf_name(&c.install_path),
                version: "Unknown".into(),
                engine: c.engine.clone(),
                executable: c
                    .qsp
                    .as_ref()
                    .unwrap()
                    .recommended_player
                    .clone()
                    .unwrap_or_default(),
                external_player: crate::external_player::qsp_config(c),
                working_directory: None,
                mtool_loader: None,
                mtool: false,
                existing_id: None,
                new_override: false,
                preserve_saves: true,
                saves_confirmed: false,
            })
            .collect();
        let job = f
            .tasks
            .begin("import_plan", settings.game_root.clone())
            .unwrap();
        f.store
            .prepare(&job, settings, candidates, selections, &[])
            .unwrap();
        job.finish(Ok(vec![]));
        assert!(sources.iter().all(|source| source.is_dir()));
        let ids = f.apply(&job.id).unwrap();
        let db = f.db.lock().unwrap();
        let with_player = db.game(&ids[0]).unwrap();
        assert_eq!(with_player.launch_type, "EXTERNAL_PLAYER");
        assert_eq!(
            with_player.main_executable.as_deref(),
            Some("QuestNavigator.exe")
        );
        assert_eq!(with_player.working_directory, ".");
        assert_eq!(
            with_player.external_player.unwrap().game_file.as_deref(),
            Some("彼女の冒険.qsp")
        );
        for file in [
            "QuestNavigator.exe",
            "config.xml",
            "game.css",
            "game.js",
            "standalone_content/resource.dat",
            "save.dat",
        ] {
            assert_eq!(
                fs::read(f.root.join("魔法少女").join(file)).unwrap(),
                b"preserved bytes"
            );
        }
        let without_player = db.game(&ids[1]).unwrap();
        assert_eq!(without_player.launch_type, "EXTERNAL_PLAYER");
        assert!(without_player.main_executable.is_none());
        assert_eq!(
            crate::maintenance::check_game(&without_player).state,
            "unconfigured"
        );
        assert!(sources.iter().all(|source| !source.exists()));
    }
    use super::*;
    use crate::jobs::TaskManager;

    struct Fixture {
        _temp: tempfile::TempDir,
        root: PathBuf,
        source: PathBuf,
        store: ImportStore,
        db: Arc<Mutex<Database>>,
        tasks: TaskManager,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let base = dunce::canonicalize(temp.path()).unwrap();
            let root = base.join("游戏库");
            let source = base.join("下载");
            let data = base.join("data");
            for directory in [&root, &source, &data] {
                fs::create_dir(directory).unwrap();
            }
            let db = Database::open(&data.join("fixture.db")).unwrap();
            let mut settings = db.settings().unwrap();
            settings.game_root = paths::path_text(&root).unwrap();
            db.save_settings(settings).unwrap();
            assert!(db.games().unwrap().is_empty());
            Self {
                _temp: temp,
                root,
                source,
                store: ImportStore::open(data.join("imports")).unwrap(),
                db: Arc::new(Mutex::new(db)),
                tasks: TaskManager::default(),
            }
        }
        fn game(&self, name: &str) -> PathBuf {
            let game = self.source.join(name);
            fs::create_dir_all(game.join("包装/Save")).unwrap();
            fs::write(game.join("包装/游戏.html"), b"game document content").unwrap();
            fs::write(game.join("包装/Save/存档.dat"), b"important save").unwrap();
            fs::create_dir(game.join("空目录")).unwrap();
            game
        }
        fn plan(&self, sources: &[PathBuf]) -> String {
            let settings = self.db.lock().unwrap().settings().unwrap();
            let candidates = sources
                .iter()
                .map(|p| scanner::analyze_directory(p).unwrap())
                .collect::<Vec<_>>();
            let selections = candidates
                .iter()
                .map(|c| Selection {
                    source: c.install_path.clone(),
                    title: c.suggested_title.clone(),
                    target_name: leaf_name(&c.install_path),
                    version: "v1.2.3".into(),
                    engine: "HTML".into(),
                    executable: "包装/游戏.html".into(),
                    working_directory: None,
                    mtool_loader: None,
                    external_player: None,
                    mtool: false,
                    existing_id: None,
                    new_override: false,
                    preserve_saves: true,
                    saves_confirmed: false,
                })
                .collect();
            let job = self
                .tasks
                .begin("import_plan", settings.game_root.clone())
                .unwrap();
            self.store
                .prepare(
                    &job,
                    settings,
                    candidates,
                    selections,
                    &self.db.lock().unwrap().games().unwrap(),
                )
                .unwrap();
            job.finish(Ok(vec![]));
            job.id.clone()
        }
        fn apply(&self, id: &str) -> Result<Vec<String>> {
            let job = self
                .tasks
                .begin("import_apply", self.root.display().to_string())
                .unwrap();
            let result = self.store.apply(id, &job, &self.db);
            job.finish(
                result
                    .as_ref()
                    .map(Clone::clone)
                    .map_err(ToString::to_string),
            );
            result
        }
        fn stage(&self, id: &str, index: usize, state: &str, copied: bool) -> Plan {
            let mut plan = self.store.get(id).unwrap();
            let item = &plan.items[index];
            let container = stage_path(&plan, index);
            fs::create_dir(&container).unwrap();
            fs::write(container.join("owner"), id).unwrap();
            if copied {
                fs::create_dir(container.join("payload")).unwrap();
            } else {
                rename_new(
                    Path::new(&item.selection.source),
                    &container.join("payload"),
                )
                .unwrap();
            }
            plan.status = "running".into();
            plan.items[index].state = state.into();
            plan.items[index].cross_volume = copied;
            if state == "publishing" {
                plan.items[index].payload_id = Some(identity(&container.join("payload")).unwrap());
                rename_new(
                    &container.join("payload"),
                    Path::new(&plan.items[index].target),
                )
                .unwrap();
            }
            self.store.save(&plan).unwrap();
            plan
        }
    }
    fn leaf_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn corrupt_journals_and_missing_manifests_are_isolated_without_losing_valid_plans_or_files() {
        let f = Fixture::new();
        let good_source = f.game("good");
        let good_id = f.plan(std::slice::from_ref(&good_source));
        let bad_source = f.game("broken manifest");
        let bad_id = f.plan(std::slice::from_ref(&bad_source));
        let manifest = f.store.directory.join(format!("{bad_id}-0.manifest"));
        let original_manifest = fs::read(&manifest).unwrap();
        fs::remove_file(&manifest).unwrap();
        let broken_record = f
            .store
            .directory
            .join(format!("{}.json", uuid::Uuid::new_v4()));
        fs::write(&broken_record, b"{truncated").unwrap();
        let recovered = ImportStore::open(f.store.directory.clone()).unwrap();
        assert_eq!(recovered.views().len(), 1);
        assert_eq!(recovered.views()[0].id, good_id);
        assert_eq!(recovered.recovery_issues().len(), 2);
        assert!(recovered.has_pending_files());
        assert!(recovered.get(&bad_id).is_err());
        assert!(recovered.ensure_recovery_clear().is_err());
        let job = f.tasks.begin("import_apply", String::new()).unwrap();
        assert!(recovered.apply(&good_id, &job, &f.db).is_err());
        assert_eq!(fs::read(&broken_record).unwrap(), b"{truncated");
        assert_eq!(
            fs::read(good_source.join("包装/Save/存档.dat")).unwrap(),
            b"important save"
        );
        assert!(bad_source.exists());
        assert!(f.db.lock().unwrap().games().unwrap().is_empty());
        fs::write(&manifest, original_manifest).unwrap();
        fs::remove_file(&broken_record).unwrap();
        let repaired = ImportStore::open(f.store.directory.clone()).unwrap();
        assert!(repaired.recovery_issues().is_empty());
        assert_eq!(repaired.views().len(), 2);
    }

    #[test]
    fn unsafe_manifest_and_unknown_journal_state_are_not_replayable() {
        let f = Fixture::new();
        let source = f.game("invalid");
        let id = f.plan(&[source]);
        let path = f.store.directory.join(format!("{id}-0.manifest"));
        // Cross many read-buffer boundaries; late invalid entries must still block recovery.
        let mut entries = (0..5000)
            .map(|index| Entry {
                path: format!("资源/scene-{index}.png"),
                directory: false,
                bytes: 1,
                modified: 0,
            })
            .collect::<Vec<_>>();
        let valid = serde_json::to_vec(&entries).unwrap();
        assert!(valid.len() > 64 * 1024);
        fs::write(&path, &valid).unwrap();
        let recovered = ImportStore::open(f.store.directory.clone()).unwrap();
        assert!(recovered.recovery_issues().is_empty());
        assert_eq!(
            recovered.get(&id).unwrap().items[0].manifest.len(),
            entries.len()
        );
        for invalid_path in ["../outside", "资源/scene-0.png"] {
            entries.push(Entry {
                path: invalid_path.into(),
                directory: false,
                bytes: 1,
                modified: 0,
            });
            fs::write(&path, serde_json::to_vec(&entries).unwrap()).unwrap();
            let recovered = ImportStore::open(f.store.directory.clone()).unwrap();
            assert_eq!(recovered.recovery_issues().len(), 1);
            assert!(recovered.views().is_empty());
            entries.pop();
        }
        fs::write(&path, &valid[..valid.len() - 1]).unwrap();
        let recovered = ImportStore::open(f.store.directory.clone()).unwrap();
        assert_eq!(recovered.recovery_issues().len(), 1);
        assert!(recovered.views().is_empty());
        let mut plan = f.store.get(&id).unwrap();
        plan.items[0].state = "future-unknown-operation".into();
        f.store.save(&plan).unwrap();
        let recovered = ImportStore::open(f.store.directory.clone()).unwrap();
        assert_eq!(recovered.recovery_issues().len(), 1);
        assert!(recovered.views().is_empty());
    }

    #[test]
    fn batch_preview_is_read_only_and_move_registers_unicode_documents_and_all_saves() {
        let fixture = Fixture::new();
        let a = fixture.game("新游戏 v1.2 中文");
        let b = fixture.game("ゲーム [A] & B");
        let id = fixture.plan(&[a.clone(), b.clone()]);
        assert!(fixture.store.views()[0].recorded_at_ms.is_some());
        assert!(fixture.store.views()[0]
            .items
            .iter()
            .all(|item| item.completed_at_ms.is_none()));
        assert!(a.is_dir() && b.is_dir());
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
        assert!(fixture.db.lock().unwrap().games().unwrap().is_empty());
        // Update-only overrides must not change the defaults for a new import.
        let mut preview = fixture.store.get(&id).unwrap();
        for item in &mut preview.items {
            item.selection.working_directory = Some("missing-cwd".into());
            item.selection.mtool_loader = Some("loaders/missing.dll".into());
        }
        fixture.store.save(&preview).unwrap();
        let ids = fixture.apply(&id).unwrap();
        assert_eq!(ids.len(), 2);
        let completed = fixture.store.views()[0]
            .items
            .iter()
            .map(|item| item.completed_at_ms)
            .collect::<Vec<_>>();
        assert!(completed.iter().all(Option::is_some));
        let reopened = ImportStore::open(fixture.store.directory.clone()).unwrap();
        assert_eq!(
            reopened.views()[0]
                .items
                .iter()
                .map(|item| item.completed_at_ms)
                .collect::<Vec<_>>(),
            completed
        );
        let record = fixture.store.directory.join(format!("{id}.json"));
        let mut legacy: serde_json::Value =
            serde_json::from_reader(File::open(&record).unwrap()).unwrap();
        for item in legacy["items"].as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove("completed_at_ms");
        }
        fs::write(&record, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let legacy_store = ImportStore::open(fixture.store.directory.clone()).unwrap();
        assert!(legacy_store.views()[0].recorded_at_ms.is_some());
        assert!(legacy_store.views()[0]
            .items
            .iter()
            .all(|item| item.completed_at_ms.is_none()));
        assert!(!a.exists() && !b.exists());
        for game in fixture.db.lock().unwrap().games().unwrap() {
            assert_eq!(game.main_executable.as_deref(), Some("包装/游戏.html"));
            assert_eq!(game.working_directory, "包装");
            assert!(game.mtool_loader.is_none());
            assert_eq!(game.engine, "HTML");
            assert_eq!(
                fs::read(Path::new(&game.install_path).join("包装/Save/存档.dat")).unwrap(),
                b"important save"
            );
            assert!(Path::new(&game.install_path).join("空目录").is_dir());
        }
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 2);
    }
    #[test]
    fn ordinary_source_additions_are_moved_but_target_collisions_are_never_overwritten() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        fs::write(source.join("新增文件"), b"new").unwrap();
        f.apply(&id).unwrap();
        assert_eq!(fs::read(f.root.join("game/新增文件")).unwrap(), b"new");
        assert_eq!(f.db.lock().unwrap().games().unwrap().len(), 1);
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        fs::create_dir(f.root.join("game")).unwrap();
        fs::write(f.root.join("game/keep"), b"unrelated").unwrap();
        assert!(f.apply(&id).is_err());
        assert_eq!(fs::read(f.root.join("game/keep")).unwrap(), b"unrelated");
        assert!(source.exists());
    }
    #[test]
    fn duplicate_sources_and_library_overlapping_paths_are_blocked() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(&[source.clone(), source.clone()]);
        assert!(!f.store.get(&id).unwrap().items[1].blockers.is_empty());
        assert!(f.apply(&id).is_err());
        assert!(source.exists());
        let inside = f.root.join("already here");
        fs::create_dir_all(inside.join("包装")).unwrap();
        fs::write(inside.join("包装/游戏.html"), b"page").unwrap();
        let id = f.plan(std::slice::from_ref(&inside));
        assert!(f.store.get(&id).unwrap().items[0]
            .blockers
            .iter()
            .any(|s| s.contains("重叠")));
        assert!(inside.exists());
    }
    #[test]
    fn cancelled_before_move_retains_every_source_and_can_resume() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        let job = f.tasks.begin("import_apply", String::new()).unwrap();
        job.request_cancel();
        assert!(f.store.apply(&id, &job, &f.db).is_err());
        job.finish(Err("cancel".into()));
        assert!(source.exists());
        assert!(f.db.lock().unwrap().games().unwrap().is_empty());
        assert_eq!(f.apply(&id).unwrap().len(), 1);
    }
    #[test]
    fn crash_after_rename_or_publish_resumes_with_original_identity() {
        for state in ["moving", "publishing"] {
            let f = Fixture::new();
            let source = f.game("game");
            let id = f.plan(std::slice::from_ref(&source));
            f.stage(&id, 0, state, false);
            let recovered = ImportStore::open(f.store.directory.clone()).unwrap();
            assert_eq!(recovered.get(&id).unwrap().status, "interrupted");
            let job = f.tasks.begin("import_apply", String::new()).unwrap();
            assert_eq!(recovered.apply(&id, &job, &f.db).unwrap().len(), 1);
            assert!(!source.exists());
            assert_eq!(f.db.lock().unwrap().games().unwrap().len(), 1);
        }
    }
    #[test]
    fn moved_but_unregistered_can_be_withdrawn_without_deleting_source_or_foreign_targets() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        f.stage(&id, 0, "moving", false);
        fs::create_dir(f.root.join("game")).unwrap();
        fs::write(f.root.join("game/keep"), b"foreign").unwrap();
        let job = f.tasks.begin("import_withdraw", String::new()).unwrap();
        f.store.withdraw(&id, &job, &f.db).unwrap();
        assert!(source.exists());
        assert_eq!(fs::read(f.root.join("game/keep")).unwrap(), b"foreign");
        assert!(f.db.lock().unwrap().games().unwrap().is_empty());
    }
    #[test]
    fn changed_staging_or_source_reoccupation_blocks_rollback() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        let p = f.stage(&id, 0, "moving", false);
        fs::write(stage_path(&p, 0).join("payload/包装/游戏.html"), b"changed").unwrap();
        let job = f.tasks.begin("import_withdraw", String::new()).unwrap();
        assert!(f.store.withdraw(&id, &job, &f.db).is_err());
        assert!(stage_path(&p, 0).join("payload").exists());
    }
    #[test]
    fn cross_volume_copy_is_verified_then_source_is_removed_and_partial_copy_recovers() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        let mut p = f.stage(&id, 0, "copying", true);
        fs::create_dir(stage_path(&p, 0).join("payload/包装")).unwrap();
        fs::write(stage_path(&p, 0).join("payload/包装/游戏.html"), b"game").unwrap();
        p.items[0].cross_volume = true;
        f.store.save(&p).unwrap();
        assert_eq!(f.apply(&id).unwrap().len(), 1);
        assert!(!source.exists());
        assert_eq!(
            fs::read(f.root.join("game/包装/Save/存档.dat")).unwrap(),
            b"important save"
        );
    }
    #[test]
    fn oversized_partial_files_are_preserved_and_not_overwritten() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        let p = f.stage(&id, 0, "copying", true);
        let file = stage_path(&p, 0).join("payload/包装/游戏.html");
        fs::create_dir(file.parent().unwrap()).unwrap();
        let oversized = vec![b'X'; 1024];
        fs::write(&file, &oversized).unwrap();
        assert!(f.apply(&id).is_err());
        assert!(source.exists());
        assert_eq!(fs::read(file).unwrap(), oversized);
        assert!(f.db.lock().unwrap().games().unwrap().is_empty());
    }
    #[test]
    fn committed_identity_survives_crash_before_journal_checkpoint() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        let p = f.stage(&id, 0, "publishing", false);
        let item = &p.items[0];
        let mut candidate = item.candidate.clone();
        candidate.install_path = item.target.clone();
        let entry = RegistrationEntry {
            candidate,
            executable: Some(item.selection.executable.clone()),
            external_player: None,
            version: "v1.2.3".into(),
            version_source: "manual".into(),
            working_directory: "包装".into(),
        };
        f.db.lock()
            .unwrap()
            .register_import(entry, &item.game_id)
            .unwrap();
        assert_eq!(f.apply(&id).unwrap(), vec![item.game_id.clone()]);
        assert_eq!(f.db.lock().unwrap().games().unwrap().len(), 1);
    }
    #[test]
    fn batch_resume_keeps_previously_completed_games_and_skips_them() {
        let f = Fixture::new();
        let a = f.game("A");
        let b = f.game("B");
        let id = f.plan(&[a.clone(), b.clone()]);
        let mut p = f.store.get(&id).unwrap();
        let job = f.tasks.begin("import_apply", String::new()).unwrap();
        f.store.move_one(&mut p, 0, &job, &f.db).unwrap();
        p.status = "cancelled".into();
        f.store.save(&p).unwrap();
        job.finish(Err("cancel".into()));
        assert!(!a.exists());
        assert!(b.exists());
        let first = p.items[0].registered_id.clone();
        assert_eq!(f.apply(&id).unwrap().len(), 2);
        assert_eq!(f.store.get(&id).unwrap().items[0].registered_id, first);
        assert_eq!(f.db.lock().unwrap().games().unwrap().len(), 2);
    }
    #[test]
    fn matching_handles_package_labels_and_translations_without_merging_sequels_or_short_names() {
        let f = Fixture::new();
        let source = f.game("Game 2 v1.2");
        let id = f.plan(std::slice::from_ref(&source));
        f.apply(&id).unwrap();
        let mut games = f.db.lock().unwrap().games().unwrap();
        let mut candidate = scanner::pending_candidate(Path::new("Game 2 v1.3")).unwrap();
        assert_eq!(matches(&candidate, &games).len(), 1);
        games[0].display_title = "用户改过的名称".into();
        games[0].canonical_title = "另一个名称".into();
        games[0].aliases.clear();
        assert_eq!(matches(&candidate, &games).len(), 1); // the installation folder still matches
        candidate.suggested_title = "Game 3 v1.3".into();
        assert!(matches(&candidate, &games).is_empty());
        for (incoming, old, expected) in [
            (
                "University of Problems 问题学校 学校情缘 1.9.5",
                "University of Problems v1.8.0 Extended 问题大学 学院情缘",
                Some(false),
            ),
            (
                "亚洲之子东方之乡 SOA V89 (26.07.02) electron",
                "SOA V88.GG 亚洲之子：东方之乡",
                Some(false),
            ),
            (
                "Ｆａｌｌｅｎ＿Ｓｔａｒ v1.2",
                "Fallen Star v1.1",
                Some(false),
            ),
            (
                "Night Bloom 夜幕之花 v0.723",
                "夜幕之花 Night Bloom v0.698",
                Some(false),
            ),
            (
                "Night Bloom 2 夜幕之花 v0.723",
                "夜幕之花 Night Bloom 3 v0.698",
                None,
            ),
            (
                "University of Problems 2 学校情缘",
                "University of Problems 3 学院情缘",
                None,
            ),
            ("SOA V89", "SOA V88.GG", None),
            (
                "希尔丝大冒险 v1.1.4",
                "希尔丝大冒险 The Adventures of HILLS v1.1.4",
                Some(false),
            ),
            (
                "希尔丝大冒险 The Adventures of HILLS v1.1.5",
                "希尔丝大冒险 v1.1.4",
                Some(false),
            ),
            ("希尔丝大冒险 2 v1.1.4", "希尔丝大冒险 3 v1.1.4", None),
            ("希尔丝大冒险 v1.1.4", "希尔丝大冒险外传 v1.1.4", None),
            ("希尔丝大冒险 Alpha Story", "希尔丝大冒险 Other Story", None),
            (
                "Unrelated School 问题学校",
                "University of Problems 问题大学",
                None,
            ),
            ("Night Bloom v0.723", "Night Bloom v0.698", Some(true)),
        ] {
            games[0].display_title = old.into();
            games[0].canonical_title = old.into();
            games[0].install_path = format!("E:/Library/{old}");
            candidate.suggested_title = incoming.into();
            let result = matches(&candidate, &games);
            assert_eq!(
                result.first().map(|matched| matched.auto_associate),
                expected,
                "{incoming} / {old}"
            );
            assert!(result.len() <= 1);
        }
        // A literal alias wins even if another name only produces a weak recommendation.
        games[0].aliases.push(candidate.suggested_title.clone());
        assert!(matches(&candidate, &games)[0].auto_associate);
        for name in [
            "..",
            "CON",
            "a/b",
            "a\\b",
            "C:foo",
            "trail.",
            " leading",
            ".butter-import-foreign",
        ] {
            assert!(folder_name(name).is_err());
        }
    }
    #[cfg(windows)]
    #[test]
    fn native_cross_volume_import_verifies_bytes_and_removes_only_fixture_source() {
        let mut f = Fixture::new();
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let fixture_root = project.join(".tools");
        fs::create_dir_all(&fixture_root).unwrap();
        let destination = tempfile::Builder::new()
            .prefix("import-cross-drive-")
            .tempdir_in(&fixture_root)
            .unwrap();
        f.root = dunce::canonicalize(destination.path()).unwrap();
        if volume(&f.root).unwrap() == volume(&f.source).unwrap() {
            eprintln!("Cross-volume fixture needs the project and temporary directory on different volumes");
            return;
        }
        let mut settings = f.db.lock().unwrap().settings().unwrap();
        settings.game_root = paths::path_text(&f.root).unwrap();
        f.db.lock().unwrap().save_settings(settings).unwrap();
        let source = f.game("跨盘游戏 v1.2");
        let id = f.plan(std::slice::from_ref(&source));
        assert!(f.store.get(&id).unwrap().items[0].cross_volume);
        assert_eq!(f.apply(&id).unwrap().len(), 1);
        assert!(!source.exists());
        assert_eq!(
            fs::read(f.root.join("跨盘游戏 v1.2/包装/Save/存档.dat")).unwrap(),
            b"important save"
        );
    }
    #[test]
    fn replacing_the_library_directory_invalidates_the_plan_before_any_move() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        fs::rename(&f.root, f.root.with_file_name("old-root")).unwrap();
        fs::create_dir(&f.root).unwrap();
        assert!(f.apply(&id).is_err());
        assert!(source.exists());
        assert!(f.db.lock().unwrap().games().unwrap().is_empty());
    }
    #[test]
    fn registered_sources_outside_the_current_root_cannot_be_moved_as_new_games() {
        let f = Fixture::new();
        let source = f.game("old registered game");
        let candidate = scanner::analyze_directory(&source).unwrap();
        f.db.lock().unwrap().register(&[candidate]).unwrap();
        let id = f.plan(std::slice::from_ref(&source));
        assert!(f.store.get(&id).unwrap().items[0]
            .blockers
            .iter()
            .any(|b| b.contains("来源已在")));
        assert!(f.apply(&id).is_err());
        assert!(source.exists());
        assert_eq!(f.db.lock().unwrap().games().unwrap().len(), 1);
    }
    #[test]
    fn withdrawing_after_registration_keeps_changed_saves_and_remaining_sources() {
        let f = Fixture::new();
        let source = f.game("game");
        let id = f.plan(std::slice::from_ref(&source));
        let mut p = f.store.get(&id).unwrap();
        p.items[0].cross_volume = true;
        let container = stage_path(&p, 0);
        fs::create_dir(&container).unwrap();
        fs::write(container.join("owner"), &id).unwrap();
        let job = f.tasks.begin("import_apply", String::new()).unwrap();
        copy_tree(
            &source,
            &container.join("payload"),
            &p.items[0].manifest,
            &job,
        )
        .unwrap();
        p.items[0].payload_id = Some(identity(&container.join("payload")).unwrap());
        rename_new(&container.join("payload"), Path::new(&p.items[0].target)).unwrap();
        let mut candidate = p.items[0].candidate.clone();
        candidate.install_path = p.items[0].target.clone();
        let entry = RegistrationEntry {
            candidate,
            executable: Some("包装/游戏.html".into()),
            external_player: None,
            version: "v1.2.3".into(),
            version_source: "manual".into(),
            working_directory: "包装".into(),
        };
        let game_id =
            f.db.lock()
                .unwrap()
                .register_import(entry, &p.items[0].game_id)
                .unwrap();
        p.items[0].registered_id = Some(game_id);
        p.items[0].state = "cleanup".into();
        f.store.save(&p).unwrap();
        fs::write(
            Path::new(&p.items[0].target).join("包装/Save/存档.dat"),
            b"new save after playing",
        )
        .unwrap();
        f.store.withdraw(&id, &job, &f.db).unwrap();
        assert!(source.exists());
        assert_eq!(
            fs::read(Path::new(&p.items[0].target).join("包装/Save/存档.dat")).unwrap(),
            b"new save after playing"
        );
        assert!(!f.store.has_pending_files());
        assert_eq!(f.db.lock().unwrap().games().unwrap().len(), 1);
    }
    #[test]
    fn exact_name_match_needs_explicit_new_game_override_before_moving() {
        let f = Fixture::new();
        let first = f.game("Game 2 v1.2");
        let id = f.plan(&[first]);
        f.apply(&id).unwrap();
        let second = f.game("Game 2 v1.3");
        let id = f.plan(std::slice::from_ref(&second));
        assert!(f.store.get(&id).unwrap().items[0]
            .blockers
            .iter()
            .any(|b| b.contains("名称匹配")));
        assert!(f.apply(&id).is_err());
        assert!(second.exists());
    }
    #[test]
    fn confirmed_import_launch_and_engine_choices_are_not_overwritten_by_later_scans() {
        let f = Fixture::new();
        let source = f.game("game");
        fs::write(source.join("main.exe"), b"fixture").unwrap();
        fs::write(source.join("与工具一同启动.bat"), b"").unwrap();
        let id = f.plan(&[source]);
        let mut p = f.store.get(&id).unwrap();
        p.items[0].selection.executable = "main.exe".into();
        p.items[0].selection.mtool = false;
        p.items[0].selection.engine = "QSP".into();
        f.store.save(&p).unwrap();
        let game_id = f.apply(&id).unwrap().remove(0);
        let mut candidate = scanner::analyze_directory(Path::new(&p.items[0].target)).unwrap();
        candidate.registered_id = Some(game_id.clone());
        candidate.mtool_detected = true;
        candidate.engine = "Unity".into();
        let mut db = f.db.lock().unwrap();
        db.sync_mtool_defaults(std::slice::from_ref(&candidate))
            .unwrap();
        db.supplement_metadata(&[(game_id.clone(), candidate)], &|| false)
            .unwrap();
        let game = db.game(&game_id).unwrap();
        assert_eq!(game.launch_type, "DIRECT");
        assert_eq!(game.engine, "QSP");
    }
}
