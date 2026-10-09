use crate::domain::{Error, Game, LibraryPathCheck, Result, ScanCandidate};
use crate::{paths, scanner};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

#[derive(Clone, Serialize)]
pub struct JobPage {
    pub id: String,
    pub kind: String,
    pub root: String,
    pub status: String,
    pub phase: String,
    pub total: usize,
    pub processed: usize,
    pub active: HashMap<String, String>,
    pub elapsed_ms: u64,
    pub idle_ms: u64,
    pub changes: Vec<ScanCandidate>,
    pub next_cursor: usize,
    pub change_count: usize,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub registered_ids: Vec<String>,
    pub path_checks: Vec<LibraryPathCheck>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub overall_done: u64,
    pub overall_total: u64,
    pub indeterminate: bool,
    pub current_game: Option<String>,
    pub transfer_rate: Option<f64>,
    pub remaining_seconds: Option<f64>,
}

struct Data {
    status: String,
    phase: String,
    total: usize,
    processed: usize,
    active: HashMap<String, String>,
    changes: Vec<ScanCandidate>,
    change_base: usize,
    latest: HashMap<String, ScanCandidate>,
    skipped: HashSet<String>,
    warnings: Vec<String>,
    error: Option<String>,
    registered_ids: Vec<String>,
    path_checks: Vec<LibraryPathCheck>,
    finished_ms: Option<u64>,
    last_progress: Instant,
    bytes_done: u64,
    bytes_total: u64,
    overall_done: u64,
    overall_total: u64,
    indeterminate: bool,
    item_base: u64,
    item_weight: u64,
    item_fraction: f64,
    current_game: Option<String>,
    transfer_started: Option<Instant>,
    transfer_bytes: u64,
    transfer_sample: u64,
    transfer_at: Option<Instant>,
    transfer_range: Option<(f64, f64)>,
}

pub struct Job {
    pub id: String,
    pub kind: String,
    pub root: String,
    started: Instant,
    pub cancelled: AtomicBool,
    data: Mutex<Data>,
}

impl Job {
    pub(crate) fn recovery(root: String) -> Self {
        Self::new("recovery", root)
    }
    fn new(kind: &str, root: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.into(),
            root,
            started: Instant::now(),
            cancelled: AtomicBool::new(false),
            data: Mutex::new(Data {
                status: "running".into(),
                phase: "准备".into(),
                total: 0,
                processed: 0,
                active: HashMap::new(),
                changes: vec![],
                change_base: 0,
                latest: HashMap::new(),
                skipped: HashSet::new(),
                warnings: vec![],
                error: None,
                registered_ids: vec![],
                path_checks: vec![],
                finished_ms: None,
                last_progress: Instant::now(),
                bytes_done: 0,
                bytes_total: 0,
                overall_done: 0,
                overall_total: 0,
                indeterminate: true,
                item_base: 0,
                item_weight: 0,
                item_fraction: 0.0,
                current_game: None,
                transfer_started: None,
                transfer_bytes: 0,
                transfer_sample: 0,
                transfer_at: None,
                transfer_range: None,
            }),
        }
    }
    pub fn is_active(&self) -> bool {
        matches!(
            self.data.lock().unwrap().status.as_str(),
            "running" | "cancel_requested"
        )
    }
    pub fn stop(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn request_cancel(&self) {
        let mut data = self.data.lock().unwrap();
        if matches!(data.status.as_str(), "running" | "cancel_requested") {
            self.cancelled.store(true, Ordering::Release);
            data.status = "cancel_requested".into();
        }
    }
    pub fn skip(&self, path: &str) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        if self.kind != "scan" || !data.latest.contains_key(path) {
            return Err(Error::Validation("没有对应的扫描候选".into()));
        }
        data.skipped.insert(path.into());
        Ok(())
    }
    pub fn interrupted(&self, path: &str) -> bool {
        self.stop() || self.data.lock().unwrap().skipped.contains(path)
    }
    pub fn stage(&self, phase: &str, total: usize) {
        let mut data = self.data.lock().unwrap();
        data.phase = phase.into();
        data.total = total;
        data.processed = 0;
        data.indeterminate = true;
        data.last_progress = Instant::now();
    }
    pub fn transfer(&self, phase: &str, done: u64, total: u64) {
        let mut data = self.data.lock().unwrap();
        if data.phase != phase || done < data.bytes_done || total != data.bytes_total {
            data.transfer_started = Some(Instant::now());
            data.transfer_bytes = 0;
            data.transfer_sample = 0;
        }
        let delta = done.saturating_sub(data.transfer_sample);
        data.transfer_bytes = data.transfer_bytes.saturating_add(delta);
        data.transfer_sample = done;
        data.transfer_at = Some(Instant::now());
        data.phase = phase.into();
        data.bytes_done = done;
        data.bytes_total = total;
        data.indeterminate = total == 0;
        data.last_progress = Instant::now();
        if let Some((start, end)) = data.transfer_range.filter(|_| total > 0) {
            let fraction = start + (end - start) * (done.min(total) as f64 / total as f64);
            Self::advance_item(&mut data, fraction);
        }
    }
    pub fn batch_transfer(&self, weights: &[u64]) {
        let mut data = self.data.lock().unwrap();
        data.overall_total = weights.iter().fold(0_u64, |total, weight| {
            total.saturating_add((*weight).max(1))
        });
        data.overall_done = 0;
        data.item_base = 0;
        data.indeterminate = true;
    }
    pub fn transfer_item(&self, title: &str, weight: u64) {
        let mut data = self.data.lock().unwrap();
        data.item_base = data.overall_done;
        data.item_weight = weight.max(1);
        data.item_fraction = 0.0;
        data.current_game = Some(title.into());
        data.transfer_started = None;
        data.transfer_at = None;
        data.transfer_range = None;
        data.bytes_done = 0;
        data.bytes_total = 0;
        data.indeterminate = true;
    }
    fn advance_item(data: &mut Data, fraction: f64) {
        data.item_fraction = data.item_fraction.max(fraction.clamp(0.0, 1.0));
        data.overall_done = data
            .item_base
            .saturating_add((data.item_weight as f64 * data.item_fraction) as u64)
            .min(data.overall_total);
    }
    pub fn transfer_phase(&self, phase: &str, fraction: f64) {
        let mut data = self.data.lock().unwrap();
        data.phase = phase.into();
        data.transfer_at = None;
        data.transfer_range = None;
        data.indeterminate = false;
        data.last_progress = Instant::now();
        Self::advance_item(&mut data, fraction);
    }
    pub fn copy_phase(&self, phase: &str, start: f64, end: f64) {
        self.transfer_phase(phase, start);
        let mut data = self.data.lock().unwrap();
        data.transfer_range = Some((start, end));
        data.indeterminate = true;
    }
    pub fn activity_phase(&self, phase: &str) {
        let mut data = self.data.lock().unwrap();
        data.phase = phase.into();
        data.indeterminate = true;
        data.transfer_at = None;
        data.transfer_range = None;
        data.last_progress = Instant::now();
    }
    pub fn progress(&self, path: &str, current: &Path) {
        let mut data = self.data.lock().unwrap();
        data.last_progress = Instant::now();
        if data.overall_total > 0 {
            data.active.clear();
        }
        data.active
            .insert(path.into(), current.display().to_string());
    }
    pub fn completed_one(&self, path: &str) {
        let mut data = self.data.lock().unwrap();
        data.processed += 1;
        if data.item_weight > 0 {
            Self::advance_item(&mut data, 1.0);
            data.item_weight = 0;
        }
        data.last_progress = Instant::now();
        data.active.remove(path);
    }
    pub fn publish(&self, candidate: ScanCandidate) {
        let mut data = self.data.lock().unwrap();
        data.last_progress = Instant::now();
        data.latest
            .insert(candidate.install_path.clone(), candidate.clone());
        data.changes.push(candidate);
        // Bound the journal even after many explicit reanalyses. A lagging cursor receives
        // the current snapshot in normal 100-item pages, retaining every latest candidate.
        if data.changes.len() > 20_000 {
            data.change_base += data.changes.len();
            data.changes = data.latest.values().cloned().collect();
        }
    }
    pub fn warn(&self, warning: String) {
        let mut data = self.data.lock().unwrap();
        if data.warnings.len() < 20 {
            data.warnings.push(warning);
        }
    }
    pub fn publish_path_check(&self, result: LibraryPathCheck) {
        let mut data = self.data.lock().unwrap();
        if let Some(previous) = data
            .path_checks
            .iter_mut()
            .find(|check| check.id == result.id)
        {
            *previous = result;
        } else {
            data.path_checks.push(result);
        }
        data.last_progress = Instant::now();
    }
    pub fn candidate(&self, path: &str) -> Option<ScanCandidate> {
        self.data.lock().unwrap().latest.get(path).cloned()
    }

    pub fn candidates(&self) -> Vec<ScanCandidate> {
        self.data.lock().unwrap().latest.values().cloned().collect()
    }
    pub fn finish(&self, result: std::result::Result<Vec<String>, String>) {
        let mut data = self.data.lock().unwrap();
        if !matches!(data.status.as_str(), "running" | "cancel_requested") {
            return;
        }
        data.finished_ms = Some(self.started.elapsed().as_millis() as u64);
        data.indeterminate = false;
        data.active.clear();
        match result {
            // A successful registration commit wins over a cancellation arriving afterwards.
            Ok(ids) if matches!(self.kind.as_str(), "register" | "metadata") => {
                data.registered_ids = ids;
                data.status = "completed".into();
            }
            Ok(ids) => {
                data.registered_ids = ids;
                data.status = if self.stop() {
                    "cancelled"
                } else {
                    "completed"
                }
                .into();
            }
            Err(error) if self.stop() => {
                data.status = "cancelled".into();
                data.error = Some(error);
            }
            Err(error) => {
                data.status = "failed".into();
                data.error = Some(error);
            }
        }
    }
    pub fn page(&self, cursor: usize) -> JobPage {
        let data = self.data.lock().unwrap();
        let rate = data
            .transfer_started
            .filter(|start| start.elapsed().as_secs_f64() >= 0.5)
            .zip(data.transfer_at.filter(|last| last.elapsed().as_secs() < 2))
            .map(|(start, _)| data.transfer_bytes as f64 / start.elapsed().as_secs_f64())
            .filter(|rate| *rate > 0.0 && data.finished_ms.is_none());
        let cursor = cursor
            .saturating_sub(data.change_base)
            .min(data.changes.len());
        let end = (cursor + 100).min(data.changes.len());
        JobPage {
            id: self.id.clone(),
            kind: self.kind.clone(),
            bytes_done: data.bytes_done,
            bytes_total: data.bytes_total,
            overall_done: data.overall_done,
            overall_total: data.overall_total,
            indeterminate: data.indeterminate,
            current_game: data.current_game.clone(),
            transfer_rate: rate,
            remaining_seconds: rate.map(|rate| {
                // Overall units reserve 20% for saves, metadata and recycling.
                data.overall_total.saturating_sub(data.overall_done) as f64 * 0.75 / rate
            }),
            root: self.root.clone(),
            status: data.status.clone(),
            phase: data.phase.clone(),
            total: data.total,
            processed: data.processed,
            active: data.active.clone(),
            elapsed_ms: data
                .finished_ms
                .unwrap_or_else(|| self.started.elapsed().as_millis() as u64),
            idle_ms: if data.finished_ms.is_some() {
                0
            } else {
                data.last_progress.elapsed().as_millis() as u64
            },
            changes: data.changes[cursor..end].to_vec(),
            next_cursor: data.change_base + end,
            change_count: data.change_base + data.changes.len(),
            warnings: data.warnings.clone(),
            error: data.error.clone(),
            registered_ids: data.registered_ids.clone(),
            path_checks: if data.finished_ms.is_some() {
                data.path_checks.clone()
            } else {
                vec![]
            },
        }
    }
}

#[derive(Default)]
struct Slots {
    scan: Option<Arc<Job>>,
    registration: Option<Arc<Job>>,
    recent: std::collections::VecDeque<Arc<Job>>,
}
#[derive(Default)]
pub struct TaskManager {
    slots: Mutex<Slots>,
}
impl TaskManager {
    pub fn begin(&self, kind: &str, root: String) -> Result<Arc<Job>> {
        let mut slots = self.slots.lock().unwrap();
        if slots.scan.as_ref().is_some_and(|v| v.is_active())
            || slots.registration.as_ref().is_some_and(|v| v.is_active())
        {
            return Err(Error::Validation(
                "已有扫描或登记任务，请先等待完成或取消".into(),
            ));
        }
        let job = Arc::new(Job::new(kind, root));
        let old = if kind == "scan" {
            slots.scan.replace(job.clone())
        } else {
            slots.registration.replace(job.clone())
        };
        if let Some(old) = old {
            slots.recent.push_back(old);
            while slots.recent.len() > 16 {
                slots.recent.pop_front();
            }
        }
        Ok(job)
    }
    pub fn active(&self) -> bool {
        let slots = self.slots.lock().unwrap();
        slots.scan.as_ref().is_some_and(|v| v.is_active())
            || slots.registration.as_ref().is_some_and(|v| v.is_active())
    }
    pub fn get(&self, id: &str) -> Result<Arc<Job>> {
        let slots = self.slots.lock().unwrap();
        let result = [slots.scan.as_ref(), slots.registration.as_ref()]
            .into_iter()
            .flatten()
            .chain(slots.recent.iter())
            .find(|v| v.id == id)
            .cloned()
            .ok_or_else(|| Error::Validation("任务已过期，请重新扫描".into()));
        result
    }
    pub fn clear_finished(&self) -> Result<()> {
        let mut slots = self.slots.lock().unwrap();
        if slots.scan.as_ref().is_some_and(|j| j.is_active())
            || slots.registration.as_ref().is_some_and(|j| j.is_active())
        {
            return Err(Error::Validation("请先结束当前任务".into()));
        }
        *slots = Slots::default();
        Ok(())
    }
    pub fn cancel_all(&self) {
        let slots = self.slots.lock().unwrap();
        for job in [slots.scan.as_ref(), slots.registration.as_ref()]
            .into_iter()
            .flatten()
        {
            job.request_cancel();
        }
    }
    pub fn update_registration(&self, path: &str, id: Option<String>) {
        let slots = self.slots.lock().unwrap();
        if let Some(scan) = &slots.scan {
            if let Some(mut candidate) = scan.candidate(path) {
                candidate.registered_id = id;
                scan.publish(candidate);
            }
        }
    }
}

pub fn run_path_check(job: &Arc<Job>, games: &[Game]) -> Result<()> {
    job.stage("检查目录与启动文件", games.len());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                while !job.stop() {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(game) = games.get(index) else {
                        break;
                    };
                    job.progress(&game.id, Path::new(&game.install_path));
                    job.publish_path_check(crate::maintenance::check_game(game));
                    job.completed_one(&game.id);
                }
            });
        }
    });
    if job.stop() {
        Err(Error::Validation("目录检查已取消，已检查的结果保留".into()))
    } else {
        Ok(())
    }
}

// Compare registered and discovered directories through the same filesystem identity.
// Keep inaccessible/missing entries in the task; check_game reports their actual state.
fn scan_path_key(path: &Path) -> Result<String> {
    let absolute = dunce::canonicalize(path).or_else(|_| std::path::absolute(path))?;
    paths::path_key(dunce::simplified(&absolute))
}

pub fn run_scan(job: &Arc<Job>, workers: usize, games: &[Game]) -> Result<()> {
    if ![1, 2, 4].contains(&workers) {
        return Err(Error::Validation(
            "扫描并发可选 1、2、4；8 线程留待真实测量".into(),
        ));
    }
    let registered = games
        .iter()
        .map(|game| Ok((scan_path_key(Path::new(&game.install_path))?, game)))
        .collect::<Result<HashMap<_, _>>>()?;
    job.stage("发现目录", 0);
    let (directories, warnings) =
        scanner::discover_root(Path::new(&job.root), &|| job.stop(), &mut |path| {
            if let Ok(mut candidate) = scanner::pending_candidate(path) {
                candidate.registered_id = scan_path_key(path)
                    .ok()
                    .and_then(|key| registered.get(&key).map(|game| game.id.clone()));
                job.publish(candidate);
            }
        })?;
    for warning in warnings {
        job.warn(warning);
    }
    let discovered = directories
        .iter()
        .map(|path| scan_path_key(path))
        .collect::<Result<HashSet<_>>>()?;
    // Missing games and games outside Game Root must still be checked, without rescanning them.
    let remaining = registered
        .iter()
        .filter(|(key, _)| !discovered.contains(*key))
        .map(|(_, game)| *game)
        .collect::<Vec<_>>();
    job.stage(
        "扫描目录与检查启动文件",
        directories.len() + remaining.len(),
    );
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while !job.stop() {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = directories.get(index) else {
                        let Some(game) = remaining.get(index - directories.len()) else {
                            break;
                        };
                        job.progress(&game.id, Path::new(&game.install_path));
                        job.publish_path_check(crate::maintenance::check_game(game));
                        job.completed_one(&game.id);
                        continue;
                    };
                    let key = path.display().to_string();
                    let candidate = scanner::analyze_quick_controlled(
                        path,
                        &|| job.interrupted(&key),
                        &|current| job.progress(&key, current),
                    );
                    let mut candidate = match candidate {
                        Ok(v) => v,
                        Err(error) => {
                            let mut value = scanner::pending_candidate(path).unwrap();
                            value.status = "error".into();
                            value.warnings.push(error.to_string());
                            value
                        }
                    };
                    if let Some(game) = scan_path_key(path)
                        .ok()
                        .and_then(|key| registered.get(&key))
                    {
                        candidate.registered_id = Some(game.id.clone());
                        if !job.interrupted(&key) {
                            job.publish_path_check(crate::maintenance::check_game(game));
                        }
                    }
                    job.publish(candidate);
                    job.completed_one(&key);
                }
            });
        }
    });
    if job.stop() {
        Err(Error::Validation("扫描已取消，已完成的结果保留".into()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_progress_never_rewinds_across_files_phases_or_games_and_expires_transfer_estimates() {
        let job = Job::recovery("fixture".into());
        job.stage("导入与更新", 2);
        job.batch_transfer(&[100, 200]);
        job.transfer_item("第一个游戏", 100);
        job.activity_phase("检查游戏运行状态");
        assert!(job.page(0).indeterminate);
        assert_eq!(job.page(0).overall_done, 0);
        job.copy_phase("跨盘复制", 0.05, 0.8);
        assert!(job.page(0).indeterminate);
        job.transfer("复制文件", 50, 100);
        let first = job.page(0);
        assert!(!first.indeterminate);
        assert_eq!(first.overall_total, 300);
        assert_eq!(first.overall_done, 42);
        {
            let mut data = job.data.lock().unwrap();
            data.transfer_started = Some(Instant::now() - std::time::Duration::from_secs(1));
        }
        let moving = job.page(0);
        assert!(moving.transfer_rate.unwrap() > 0.0);
        assert!(moving.remaining_seconds.unwrap() > 0.0);
        job.transfer("校验内部存档", 1, 10);
        assert!(job.page(0).overall_done >= first.overall_done);
        job.transfer_phase("保存资料", 0.93);
        assert_eq!(job.page(0).transfer_rate, None);
        assert_eq!(job.page(0).remaining_seconds, None);
        job.completed_one("first");
        assert_eq!(job.page(0).overall_done, 100);
        job.transfer_item("第二个游戏", 200);
        assert!(job.page(0).indeterminate);
        job.copy_phase("跨盘复制", 0.05, 0.8);
        job.transfer("复制文件", 100, 200);
        assert_eq!(job.page(0).overall_done, 185);
        assert_eq!(job.page(0).current_game.as_deref(), Some("第二个游戏"));
        job.completed_one("second");
        job.finish(Ok(vec![]));
        let finished = job.page(0);
        assert_eq!(finished.overall_done, finished.overall_total);
        assert_eq!(finished.processed, 2);
        assert_eq!(finished.transfer_rate, None);
        // A separately resumed task uses a fresh estimate and progress denominator.
        let resumed = Job::recovery("fixture".into());
        resumed.batch_transfer(&[100, 200]);
        resumed.transfer_item("已完成", 100);
        resumed.completed_one("first");
        resumed.transfer_item("继续处理", 200);
        assert_eq!(resumed.page(0).overall_done, 100);
        assert_eq!(resumed.page(0).transfer_rate, None);
    }
    #[test]
    fn import_planning_retains_library_scan_and_analysis_snapshots() {
        let tasks = TaskManager::default();
        let scan = tasks.begin("scan", "root".into()).unwrap();
        scan.finish(Ok(vec![]));
        let analysis = tasks.begin("import_analysis", String::new()).unwrap();
        analysis.finish(Ok(vec![]));
        let plan = tasks.begin("import_plan", String::new()).unwrap();
        plan.finish(Ok(vec![]));
        assert_eq!(tasks.get(&scan.id).unwrap().kind, "scan");
        assert_eq!(tasks.get(&analysis.id).unwrap().kind, "import_analysis");
    }
    #[test]
    fn path_check_results_publish_only_on_completion_and_cancel_preserves_completed_checks() {
        let temp = tempfile::tempdir().unwrap();
        let game = crate::maintenance::tests::fixture_game(temp.path());
        let manager = TaskManager::default();
        let job = manager.begin("paths", String::new()).unwrap();
        run_path_check(&job, std::slice::from_ref(&game)).unwrap();
        assert!(job.page(0).path_checks.is_empty());
        job.finish(Ok(vec![]));
        assert_eq!(job.page(0).processed, 1);
        assert_eq!(job.page(0).path_checks[0].state, "missing_launch");
        let cancelled = manager.begin("paths", String::new()).unwrap();
        cancelled.publish_path_check(crate::maintenance::check_game(&game));
        cancelled.request_cancel();
        assert!(run_path_check(&cancelled, &[game]).is_err());
        cancelled.finish(Err("cancelled".into()));
        assert_eq!(cancelled.page(0).status, "cancelled");
        assert_eq!(cancelled.page(0).path_checks.len(), 1);
    }

    #[test]
    fn scan_checks_registered_missing_and_external_directories_in_one_task() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("library");
        std::fs::create_dir(&root).unwrap();
        let mut games = Vec::new();
        for name in ["available", "manual", "unconfigured", "new"] {
            let path = root.join(name);
            std::fs::create_dir(&path).unwrap();
            std::fs::write(
                path.join("game.html"),
                b"<!doctype html><html><body>fixture</body></html>",
            )
            .unwrap();
            if name != "new" {
                let mut game = crate::maintenance::tests::fixture_game(&path);
                game.id = name.into();
                if name == "manual" {
                    game.main_executable = Some("missing.exe".into());
                }
                if name == "unconfigured" {
                    game.main_executable = None;
                }
                games.push(game);
            }
        }
        for name in ["missing", "outside"] {
            let path = temp.path().join(name);
            if name == "outside" {
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("game.html"), b"fixture").unwrap();
            }
            let mut game = crate::maintenance::tests::fixture_game(&path);
            game.id = name.into();
            games.push(game);
        }
        let manager = TaskManager::default();
        let job = manager.begin("scan", root.display().to_string()).unwrap();
        run_scan(&job, 2, &games).unwrap();
        job.finish(Ok(vec![]));
        let page = job.page(0);
        assert_eq!((page.total, page.processed), (6, 6));
        assert_eq!(job.candidates().len(), 4);
        assert_eq!(page.path_checks.len(), 5);
        for (id, expected) in [
            ("available", "available"),
            ("manual", "missing_launch"),
            ("unconfigured", "unconfigured"),
            ("missing", "missing_directory"),
            ("outside", "available"),
        ] {
            assert_eq!(
                page.path_checks
                    .iter()
                    .find(|check| check.id == id)
                    .unwrap()
                    .state,
                expected
            );
        }
        // Candidate lookup uses the path emitted by discovery, not the TEMP alias
        // used to create this fixture (e.g. an 8.3 ancestor or verbatim prefix).
        let discovered_root = dunce::canonicalize(&root).unwrap();
        assert!(job
            .candidate(&discovered_root.join("new").display().to_string())
            .unwrap()
            .registered_id
            .is_none());
        assert_eq!(
            job.candidate(&discovered_root.join("manual").display().to_string())
                .unwrap()
                .registered_id
                .as_deref(),
            Some("manual")
        );
        // A post-scan default update replaces only that game's check, rather than duplicating it.
        let mut updated = games
            .iter()
            .find(|game| game.id == "manual")
            .unwrap()
            .clone();
        updated.main_executable = Some("game.html".into());
        job.publish_path_check(crate::maintenance::check_game(&updated));
        assert_eq!(job.page(0).path_checks.len(), 5);
        assert_eq!(
            job.page(0)
                .path_checks
                .iter()
                .find(|check| check.id == "manual")
                .unwrap()
                .state,
            "available"
        );
        assert_eq!(
            games
                .iter()
                .find(|game| game.id == "manual")
                .unwrap()
                .main_executable
                .as_deref(),
            Some("missing.exe")
        );

        let cancelled = manager.begin("scan", root.display().to_string()).unwrap();
        cancelled.publish_path_check(crate::maintenance::check_game(&games[0]));
        cancelled.request_cancel();
        assert!(run_scan(&cancelled, 2, &games).is_err());
        cancelled.finish(Err("cancelled".into()));
        assert_eq!(cancelled.page(0).status, "cancelled");
        assert_eq!(cancelled.page(0).path_checks.len(), 1);
    }
    #[cfg(windows)]
    #[test]
    fn scan_windows_path_forms_do_not_duplicate_registered_work() {
        use std::os::windows::ffi::OsStrExt;
        // Keep relative-path coverage independent of the runner's TEMP spelling, without
        // changing process-wide current_dir or TEMP while other tests run.
        let cwd = std::env::current_dir().unwrap();
        let temp = tempfile::Builder::new()
            .prefix("scan identity 中文 ")
            .tempdir_in(&cwd)
            .unwrap();
        let base = dunce::canonicalize(temp.path()).unwrap();
        let root = base.join("library");
        std::fs::create_dir(&root).unwrap();
        let mut games = Vec::new();
        for name in [
            "available",
            "manual",
            "unconfigured",
            "new",
            "outside",
            "missing",
        ] {
            let path = if matches!(name, "outside" | "missing") {
                base.join(name)
            } else {
                root.join(name)
            };
            if name != "missing" {
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("game.html"), b"fixture").unwrap();
            }
            if name != "new" {
                let mut game = crate::maintenance::tests::fixture_game(&path);
                game.id = name.into();
                if name == "manual" {
                    game.main_executable = Some("missing.exe".into());
                } else if name == "unconfigured" {
                    game.main_executable = None;
                }
                games.push(game);
            }
        }
        // GitHub runners can use 8.3 TEMP ancestors; this also works when 8.3
        // generation is disabled and Windows returns the original long spelling.
        let input = base
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut buffer = vec![0u16; 32768];
        let length = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(
                input.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        } as usize;
        assert!(length > 0 && length < buffer.len());
        let short_base = std::path::PathBuf::from(String::from_utf16(&buffer[..length]).unwrap());
        for form in [
            "verbatim",
            "slashes",
            "relative",
            "dots",
            "uppercase",
            "short",
            "plain",
        ] {
            let represent = |path: &Path| match form {
                "verbatim" => format!(r"\\?\{}", path.display()),
                "slashes" => path.display().to_string().replace('\\', "/"),
                "relative" => path.strip_prefix(&cwd).unwrap().display().to_string(),
                "dots" => path
                    .parent()
                    .unwrap()
                    .join(".")
                    .join(path.file_name().unwrap())
                    .join("..")
                    .join(path.file_name().unwrap())
                    .display()
                    .to_string(),
                "uppercase" => path.display().to_string().to_uppercase(),
                "short" => short_base
                    .join(path.strip_prefix(&base).unwrap())
                    .display()
                    .to_string(),
                _ => path.display().to_string(),
            };
            let mut represented = games.clone();
            for game in &mut represented {
                game.install_path = represent(Path::new(&game.install_path));
            }
            let before = represented.clone();
            for workers in [1, 2, 4] {
                let job = TaskManager::default()
                    .begin("scan", represent(&root))
                    .unwrap();
                run_scan(&job, workers, &represented).unwrap();
                job.finish(Ok(vec![]));
                let page = job.page(0);
                assert_eq!((page.total, page.processed), (6, 6), "{form}/{workers}");
                assert_eq!(job.candidates().len(), 4, "{form}/{workers}");
                assert_eq!(page.path_checks.len(), 5, "{form}/{workers}");
                for (id, expected) in [
                    ("available", "available"),
                    ("manual", "missing_launch"),
                    ("unconfigured", "unconfigured"),
                    ("missing", "missing_directory"),
                    ("outside", "available"),
                ] {
                    let checks = page
                        .path_checks
                        .iter()
                        .filter(|check| check.id == id)
                        .collect::<Vec<_>>();
                    assert_eq!(checks.len(), 1, "{form}/{workers}/{id}");
                    assert_eq!(checks[0].state, expected, "{form}/{workers}/{id}");
                }
                let candidates = job.candidates();
                for name in ["available", "manual", "unconfigured", "new"] {
                    let candidate = candidates
                        .iter()
                        .find(|candidate| candidate.suggested_title == name)
                        .unwrap();
                    assert_eq!(
                        candidate.registered_id.as_deref(),
                        (name != "new").then_some(name),
                        "{form}/{workers}/{name}"
                    );
                }
            }
            assert_eq!(
                serde_json::to_value(&represented).unwrap(),
                serde_json::to_value(&before).unwrap()
            );
        }
    }
    #[test]
    fn record_maintenance_updates_retained_scan_registration_without_losing_overrides_or_results() {
        let manager = TaskManager::default();
        let job = manager.begin("scan", "root".into()).unwrap();
        let mut candidate = scanner::pending_candidate(Path::new("root/game")).unwrap();
        candidate.suggested_version = "v1.2".into();
        let path = candidate.install_path.clone();
        job.publish(candidate);
        job.finish(Ok(vec![]));
        manager.update_registration(&path, Some("identity".into()));
        let saved = job.candidate(&path).unwrap();
        assert_eq!(saved.registered_id.as_deref(), Some("identity"));
        assert_eq!(saved.suggested_version, "v1.2");
        manager.update_registration(&path, None);
        assert!(job.candidate(&path).unwrap().registered_id.is_none());
        assert_eq!(job.page(0).status, "completed");
    }
    #[test]
    fn reset_invalidates_snapshots_only_after_tasks_finish() {
        let tasks = TaskManager::default();
        let scan = tasks.begin("scan", "fixture".into()).unwrap();
        assert!(tasks.clear_finished().is_err());
        scan.finish(Ok(vec![]));
        let metadata = tasks.begin("metadata", String::new()).unwrap();
        metadata.request_cancel();
        metadata.finish(Ok(vec!["committed".into()]));
        assert_eq!(metadata.page(0).status, "completed");
        assert_eq!(metadata.page(0).registered_ids, vec!["committed"]);
        tasks.clear_finished().unwrap();
        assert!(tasks.get(&scan.id).is_err());
        assert!(tasks.get(&metadata.id).is_err());
    }
    #[test]
    fn paged_results_freeze_elapsed_time_and_terminal_status() {
        let manager = TaskManager::default();
        let job = manager.begin("scan", "root".into()).unwrap();
        for i in 0..250 {
            job.publish(scanner::pending_candidate(Path::new(&format!("root/game{i}"))).unwrap());
        }
        assert_eq!(job.page(0).changes.len(), 100);
        assert_eq!(job.page(100).next_cursor, 200);
        assert_eq!(job.page(200).changes.len(), 50);
        job.finish(Ok(vec![]));
        let ended = job.page(250);
        std::thread::sleep(std::time::Duration::from_millis(10));
        job.finish(Err("late failure".into()));
        assert_eq!(job.page(250).elapsed_ms, ended.elapsed_ms);
        assert_eq!(job.page(250).status, "completed");
    }
    #[test]
    fn compacted_journal_recovers_latest_snapshot_from_lagging_cursor() {
        let manager = TaskManager::default();
        let job = manager.begin("scan", "root".into()).unwrap();
        let candidate = scanner::pending_candidate(Path::new("root/game")).unwrap();
        for _ in 0..20_001 {
            job.publish(candidate.clone());
        }
        let page = job.page(0);
        assert_eq!(page.changes.len(), 1);
        assert_eq!(page.next_cursor, 20_002);
        assert!(job.page(page.next_cursor).changes.is_empty());
    }
    #[test]
    fn skipped_candidate_does_not_prevent_other_games_finishing() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["skip", "keep"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
            std::fs::write(temp.path().join(name).join("main.exe"), b"fixture").unwrap();
        }
        let root = dunce::canonicalize(temp.path()).unwrap();
        let manager = TaskManager::default();
        let job = manager.begin("scan", root.display().to_string()).unwrap();
        let skipped = scanner::pending_candidate(&root.join("skip")).unwrap();
        job.publish(skipped.clone());
        job.skip(&skipped.install_path).unwrap();
        run_scan(&job, 2, &[]).unwrap();
        job.finish(Ok(vec![]));
        assert_eq!(
            job.candidate(&skipped.install_path).unwrap().status,
            "skipped"
        );
        assert_eq!(
            job.candidate(&root.join("keep").display().to_string())
                .unwrap()
                .status,
            "ready"
        );
        assert_eq!(job.page(0).processed, 2);
    }
    #[test]
    fn one_task_cancel_keeps_results_and_late_cancel_preserves_commit() {
        let manager = TaskManager::default();
        let job = manager.begin("scan", "test".into()).unwrap();
        job.publish(scanner::pending_candidate(Path::new("test/game")).unwrap());
        assert!(manager.begin("scan", "other".into()).is_err());
        job.request_cancel();
        assert!(job.stop());
        assert!(job.is_active());
        job.finish(Ok(vec![]));
        assert_eq!(job.page(0).status, "cancelled");
        assert_eq!(job.page(0).changes.len(), 1);
        let registration = manager.begin("register", "test".into()).unwrap();
        registration.request_cancel();
        registration.finish(Ok(vec!["committed".into()]));
        assert_eq!(registration.page(0).status, "completed");
        registration.request_cancel();
        assert_eq!(registration.page(0).status, "completed");
    }
    #[test]
    fn bounded_workers_finish_all_unicode_directories() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..25 {
            let game = root.path().join(format!("游戏 {index}"));
            std::fs::create_dir(&game).unwrap();
            std::fs::write(game.join("任意.exe"), b"fixture").unwrap();
        }
        for workers in [1, 2, 4] {
            let manager = TaskManager::default();
            let job = manager
                .begin("scan", root.path().display().to_string())
                .unwrap();
            run_scan(&job, workers, &[]).unwrap();
            job.finish(Ok(vec![]));
            let page = job.page(0);
            assert_eq!(page.processed, 25);
            assert_eq!(
                page.changes.iter().filter(|v| v.status == "ready").count(),
                25
            );
        }
    }
}
