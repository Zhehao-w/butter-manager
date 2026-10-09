use crate::db::Database;
use crate::domain::{
    Error, Game, GameEdit, LaunchConfiguration, MToolLaunchPreview, RegistrationSelection,
    RelocateGame, ResetReport, Settings, ToolCheck,
};
use crate::importer::{self, ImportStore, PlanView, Selection};
use crate::jobs::{JobPage, TaskManager};
use crate::{jobs, launcher, paths, scanner};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    #[cfg(windows)]
    _data_guard: crate::data_directory::InstanceGuard,
    appearance: Mutex<crate::appearance::Appearance>,
    db: Arc<Mutex<Database>>,
    tasks: Arc<TaskManager>,
    activity: Arc<Mutex<usize>>,
    imports: Arc<ImportStore>,
    data: std::path::PathBuf,
    deletion_plans: Arc<Mutex<std::collections::HashMap<String, crate::deletion::DeletePlan>>>,
    duplicate_plans:
        Arc<Mutex<std::collections::HashMap<String, crate::import_duplicate::DuplicatePlan>>>,
    running: Arc<crate::runtime::RunningGames>,
    external_saves: Arc<crate::save_editor::ExternalSaves>,
}
type CommandResult<T> = std::result::Result<T, String>;
#[tauri::command]
fn open_data_directory(state: State<'_, AppState>) -> CommandResult<()> {
    if !cfg!(windows) {
        return Err("打开文件夹仅支持 Windows".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(&state.data)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开数据目录：{error}"))
}
#[tauri::command]
fn get_appearance(state: State<'_, AppState>) -> CommandResult<crate::appearance::Appearance> {
    let _guard = state.appearance.lock().map_err(|_| "外观设置锁不可用")?;
    crate::appearance::load(&state.data).map_err(|e| format!("无法读取外观设置：{e}"))
}
#[tauri::command]
fn save_appearance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    appearance: crate::appearance::Appearance,
) -> CommandResult<crate::appearance::Appearance> {
    let mut current = state.appearance.lock().map_err(|_| "外观设置锁不可用")?;
    let window = app.get_webview_window("main").ok_or("应用窗口不可用")?;
    window
        .set_icon(crate::appearance::icon(appearance.icon))
        .map_err(|e| e.to_string())?;
    if let Err(error) = crate::appearance::save(&state.data, appearance) {
        let _ = window.set_icon(crate::appearance::icon(current.icon));
        return Err(format!("无法保存外观设置：{error}"));
    }
    *current = appearance;
    Ok(appearance)
}
fn with_db<T>(
    state: &AppState,
    action: impl FnOnce(&mut Database) -> crate::domain::Result<T>,
) -> CommandResult<T> {
    let mut db = state.db.lock().map_err(|_| "数据库锁不可用".to_string())?;
    action(&mut db).map_err(|e| e.to_string())
}
#[tauri::command]
fn list_games(state: State<'_, AppState>) -> CommandResult<Vec<Game>> {
    with_db(&state, |db| db.games())
}
#[tauri::command]
async fn choose_import_sources(app: tauri::AppHandle) -> CommandResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择文件夹：单个游戏或包含多个游戏的文件夹，可多选")
            .blocking_pick_folders()
            .unwrap_or_default()
            .into_iter()
            .map(|f| {
                f.into_path()
                    .map(|p| p.display().to_string())
                    .map_err(|e| e.to_string())
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn discover_import_sources(
    sources: Vec<String>,
) -> CommandResult<crate::import_sources::Discovery> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::import_sources::discover(&sources).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn start_import_analysis(
    state: State<'_, AppState>,
    sources: Vec<String>,
) -> CommandResult<String> {
    if state.imports.has_pending_files() {
        return Err("请先在导入页继续或撤回未完成的文件操作".into());
    }
    let gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if *gate > 0 {
        return Err("请先结束启动请求".into());
    }
    if sources.is_empty() || sources.len() > 5000 {
        return Err("请选择 1–5000 个游戏目录".into());
    }
    let workers = with_db(&state, |db| Ok(db.settings()?.scan_workers))?;
    let job = state
        .tasks
        .begin("import_analysis", String::new())
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            importer::analyze(&job, &sources, workers)
        }));
        job.finish(match result {
            Ok(r) => r.map(|_| vec![]).map_err(|e| e.to_string()),
            Err(_) => Err("导入分析异常退出".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
async fn import_matches(
    state: State<'_, AppState>,
    scan_id: String,
) -> CommandResult<std::collections::HashMap<String, Vec<importer::Match>>> {
    let scan = state.tasks.get(&scan_id).map_err(|e| e.to_string())?;
    let games = with_db(&state, |db| db.games())?;
    tauri::async_runtime::spawn_blocking(move || {
        scan.candidates()
            .into_iter()
            .map(|c| (c.install_path.clone(), importer::matches(&c, &games)))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn import_plans(state: State<'_, AppState>) -> Vec<PlanView> {
    state.imports.views()
}
#[tauri::command]
fn import_recovery_issues(state: State<'_, AppState>) -> Vec<crate::importer::RecoveryIssue> {
    state.imports.recovery_issues().to_vec()
}
#[tauri::command]
async fn preview_import_duplicate(
    state: State<'_, AppState>,
    scan_id: String,
    source: String,
    existing_id: String,
    version: String,
) -> CommandResult<crate::import_duplicate::DuplicatePlan> {
    let scan = state.tasks.get(&scan_id).map_err(|e| e.to_string())?;
    if scan.kind != "import_analysis" || scan.is_active() {
        return Err("请先结束导入分析".into());
    }
    let candidate = scan.candidate(&source).ok_or("来源不属于当前分析结果")?;
    let (game, games, settings) = with_db(&state, |db| {
        Ok((db.game(&existing_id)?, db.games()?, db.settings()?))
    })?;
    let data = state.data.clone();
    let plans = state.duplicate_plans.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let token = uuid::Uuid::new_v4().to_string();
        let plan = crate::import_duplicate::preview(
            &candidate, &game, &version, &games, &settings, &data, &token,
        )
        .map_err(|e| e.to_string())?;
        let mut stored = plans.lock().map_err(|_| "删除预览不可用")?;
        if stored.len() >= 32 {
            stored.clear();
        }
        stored.insert(token, plan.clone());
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn recycle_import_duplicate(state: State<'_, AppState>, token: String) -> CommandResult<()> {
    let plans = state.duplicate_plans.clone();
    let data = state.data.clone();
    let activity = state.activity.clone();
    let tasks = state.tasks.clone();
    let imports = state.imports.clone();
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = activity.lock().map_err(|_| "操作锁不可用")?;
        imports.ensure_recovery_clear().map_err(|e| e.to_string())?;
        if *count > 0 || tasks.active() || imports.has_pending_files() {
            return Err("请先结束当前任务、启动请求或未完成的导入文件操作".into());
        }
        let plan = plans
            .lock()
            .map_err(|_| "删除预览不可用")?
            .remove(&token)
            .ok_or("删除预览已失效，请重新打开确认框")?;
        let database = db.lock().map_err(|_| "数据库锁不可用")?;
        let game = database
            .game(&plan.existing_id)
            .map_err(|e| e.to_string())?;
        let games = database.games().map_err(|e| e.to_string())?;
        let settings = database.settings().map_err(|e| e.to_string())?;
        crate::import_duplicate::apply(
            &plan,
            &game,
            &games,
            &settings,
            &data,
            crate::deletion::recycle,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn open_import_records(state: State<'_, AppState>) -> CommandResult<()> {
    if !cfg!(windows) {
        return Err("打开文件夹仅支持 Windows".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(state.data.join("imports"))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn start_import_plan(
    state: State<'_, AppState>,
    scan_id: String,
    selections: Vec<Selection>,
) -> CommandResult<String> {
    if state.imports.has_pending_files() {
        return Err("请先在导入页继续或撤回未完成的文件操作".into());
    }
    let gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if *gate > 0 {
        return Err("请先结束启动请求".into());
    }
    if selections.is_empty() || selections.len() > 5000 {
        return Err("请选择 1–5000 项".into());
    }
    let scan = state.tasks.get(&scan_id).map_err(|e| e.to_string())?;
    if scan.kind != "import_analysis" || scan.is_active() {
        return Err("请先结束导入分析".into());
    }
    let candidates = selections
        .iter()
        .map(|s| {
            scan.candidate(&s.source)
                .ok_or_else(|| "来源不属于当前分析结果".into())
        })
        .collect::<CommandResult<Vec<_>>>()?;
    let (settings, games) = with_db(&state, |db| Ok((db.settings()?, db.games()?)))?;
    if settings.game_root.is_empty() {
        return Err("请先设置游戏库目录".into());
    }
    let job = state
        .tasks
        .begin("import_plan", settings.game_root.clone())
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    let imports = state.imports.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            imports.prepare(&job, settings, candidates, selections, &games)
        }));
        job.finish(match result {
            Ok(r) => r.map(|_| vec![]).map_err(|e| e.to_string()),
            Err(_) => Err("计划生成异常退出".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn start_import_apply(
    state: State<'_, AppState>,
    plan_id: String,
    withdraw: bool,
) -> CommandResult<String> {
    let gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if *gate > 0 {
        return Err("请先结束启动请求".into());
    }
    let plan = state.imports.get(&plan_id).map_err(|e| e.to_string())?;
    if matches!(plan.status.as_str(), "completed" | "withdrawn") {
        return Err("该计划已经结束".into());
    }
    let item_count = plan.items.len();
    let job = state
        .tasks
        .begin(
            if withdraw {
                "import_withdraw"
            } else {
                "import_apply"
            },
            plan.root,
        )
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    job.stage("检查游戏运行状态", item_count);
    let imports = state.imports.clone();
    let database = state.db.clone();
    let running = state.running.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for item in &imports.get(&plan_id)?.items {
                if let Some(game_id) = &item.selection.existing_id {
                    let game = database.lock().unwrap().game(game_id)?;
                    running.ensure_tracked_idle(&game)?;
                }
            }
            if withdraw {
                imports.withdraw(&plan_id, &job, &database).map(|_| vec![])
            } else {
                imports.apply(&plan_id, &job, &database)
            }
        }));
        job.finish(match result {
            Ok(r) => r.map_err(|e| e.to_string()),
            Err(_) => Err("导入意外中断；可在导入页面继续或撤回未完成项".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn version_history(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<Vec<importer::VersionHistory>> {
    with_db(&state, |db| db.version_history(&id))
}
#[tauri::command]
fn start_version_rollback(
    state: State<'_, AppState>,
    plan_id: String,
    index: usize,
) -> CommandResult<String> {
    let gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if *gate > 0 {
        return Err("请先结束启动请求".into());
    }
    let plan = state.imports.get(&plan_id).map_err(|e| e.to_string())?;
    let item = plan.items.get(index).ok_or("更新项不存在")?;
    let game_id = item
        .selection
        .existing_id
        .clone()
        .ok_or("这不是已有游戏的更新")?;
    let job = state
        .tasks
        .begin("import_rollback", plan.root)
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    job.stage("检查回退状态", 1);
    let imports = state.imports.clone();
    let database = state.db.clone();
    let running = state.running.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let game = database.lock().unwrap().game(&game_id)?;
            running.ensure_tracked_idle(&game)?;
            imports.rollback(&plan_id, index, &job, &database)
        }));
        job.finish(match result {
            Ok(result) => result.map_err(|e| e.to_string()),
            Err(_) => Err("版本回退中断，请在导入页继续未完成项".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn discard_import_plan(state: State<'_, AppState>, plan_id: String) -> CommandResult<()> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if state.tasks.active() {
        return Err("请先结束任务".into());
    }
    state
        .imports
        .discard_preview(&plan_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn games_by_ids(state: State<'_, AppState>, ids: Vec<String>) -> CommandResult<Vec<Game>> {
    with_db(&state, |db| db.games_by_ids(&ids))
}

#[tauri::command]
async fn sync_scan_mtool(state: State<'_, AppState>, scan_id: String) -> CommandResult<Vec<Game>> {
    let db = state.db.clone();
    let tasks = state.tasks.clone();
    let activity = state.activity.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = activity.lock().map_err(|_| "操作锁不可用")?;
        if *count > 0 || tasks.active() {
            return Err("请先结束当前任务或启动请求".into());
        }
        let job = tasks.get(&scan_id).map_err(|e| e.to_string())?;
        if job.kind != "scan" || job.page(0).status != "completed" {
            return Err("仅使用完整扫描同步默认启动方式".into());
        }
        let result = db
            .lock()
            .map_err(|_| "数据库锁不可用")?
            .sync_mtool_defaults(&job.candidates())
            .map_err(|e| e.to_string());
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn preview_mtool_launch(
    state: State<'_, AppState>,
    id: String,
    executable: Option<String>,
    loader: Option<String>,
    working_directory: String,
) -> CommandResult<MToolLaunchPreview> {
    let (mut game, settings) = with_db(&state, |db| Ok((db.game(&id)?, db.settings()?)))?;
    game.main_executable = executable.clone();
    game.mtool_target_exe = executable;
    game.mtool_loader = loader;
    game.working_directory = working_directory;
    tauri::async_runtime::spawn_blocking(move || {
        launcher::preview_mtool(&game, &settings).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn start_library_check(state: State<'_, AppState>) -> CommandResult<String> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    let games = with_db(&state, |db| db.games())?;
    let job = state
        .tasks
        .begin("paths", String::new())
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            jobs::run_path_check(&job, &games)
        }));
        job.finish(match result {
            Ok(result) => result.map(|_| vec![]).map_err(|e| e.to_string()),
            Err(_) => Err("目录检查异常退出".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn remove_game(state: State<'_, AppState>, id: String, confirmation: String) -> CommandResult<()> {
    if confirmation != id {
        return Err("请先确认移除这条游戏记录".into());
    }
    if state.imports.has_pending_files() {
        return Err("请先在导入页继续或撤回未完成的文件操作".into());
    }
    let count = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if *count > 0 || state.tasks.active() {
        return Err("请先结束当前任务或启动请求".into());
    }
    let removed = with_db(&state, |db| db.remove_game(&id))?;
    state.tasks.update_registration(&removed.install_path, None);
    Ok(())
}
#[tauri::command]
async fn preview_game_delete(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<crate::deletion::DeletePlan> {
    let (game, games, settings) = with_db(&state, |db| {
        Ok((db.game(&id)?, db.games()?, db.settings()?))
    })?;
    let data = state.data.clone();
    let plans = state.deletion_plans.clone();
    let running = state.running.clone();
    let imports = state.imports.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let token = uuid::Uuid::new_v4().to_string();
        let mut plan = crate::deletion::preview(&game, &games, &settings, &data, &token);
        if let Err(error) = imports.include_recovery_deletion(&id, &mut plan) {
            plan.blockers.push(error.to_string());
        }
        if let Err(error) = running.ensure_idle(&game) {
            plan.blockers.push(error.to_string());
        }
        let mut stored = plans.lock().map_err(|_| "删除计划不可用")?;
        if stored.len() >= 32 {
            stored.clear();
        }
        stored.insert(token, plan.clone());
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn delete_game_files(
    state: State<'_, AppState>,
    id: String,
    token: String,
) -> CommandResult<crate::deletion::DeleteReport> {
    let plans = state.deletion_plans.clone();
    let data = state.data.clone();
    let activity = state.activity.clone();
    let imports = state.imports.clone();
    let tasks = state.tasks.clone();
    let db = state.db.clone();
    let running = state.running.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Hold the same gate used by settings, imports, relocation and launches.
        let count = activity.lock().map_err(|_| "操作锁不可用")?;
        imports.ensure_recovery_clear().map_err(|e| e.to_string())?;
        if *count > 0 || tasks.active() || imports.has_pending_files() {
            return Err("请先结束当前任务、启动请求或未完成的导入文件操作".into());
        }
        let plan = plans
            .lock()
            .map_err(|_| "删除计划不可用")?
            .remove(&token)
            .ok_or("删除预览已失效，请重新打开确认框")?;
        if plan.id != id {
            return Err("删除计划与游戏不一致".into());
        }
        let mut database = db.lock().map_err(|_| "数据库锁不可用")?;
        let game = database.game(&id).map_err(|e| e.to_string())?;
        let games = database.games().map_err(|e| e.to_string())?;
        let settings = database.settings().map_err(|e| e.to_string())?;
        let mut current = crate::deletion::preview(&game, &games, &settings, &data, &token);
        imports
            .include_recovery_deletion(&id, &mut current)
            .map_err(|e| e.to_string())?;
        if current != plan {
            return Err("目录或存档配置已变化，请重新检查删除预览".into());
        }
        running.ensure_idle(&game).map_err(|e| e.to_string())?;
        let mut report = crate::deletion::apply_files(&plan, crate::deletion::recycle);
        if report.error.is_none() {
            match database.remove_game(&id) {
                Ok(_) => {
                    tasks.update_registration(&game.install_path, None);
                    report.removed = true;
                }
                Err(e) => report.error = Some(format!("文件已送入回收站，但库记录移除失败：{e}")),
            }
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn preview_relocation(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> CommandResult<RelocateGame> {
    let game = with_db(&state, |db| db.game(&id))?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::maintenance::relocation_plan(&game, Path::new(&path)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn relocate_game(state: State<'_, AppState>, change: RelocateGame) -> CommandResult<Game> {
    let activity = state.activity.clone();
    let tasks = state.tasks.clone();
    let db = state.db.clone();
    let imports = state.imports.clone();
    let running = state.running.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = activity.lock().map_err(|_| "操作锁不可用")?;
        imports.ensure_recovery_clear().map_err(|e| e.to_string())?;
        if *count > 0 || tasks.active() || imports.has_pending_files() {
            return Err("请先结束当前任务或启动请求".into());
        }
        let mut database = db.lock().map_err(|_| "数据库锁不可用")?;
        let game = database.game(&change.id).map_err(|e| e.to_string())?;
        running.ensure_idle(&game).map_err(|e| e.to_string())?;
        crate::runtime::ensure_paths_idle(&[std::path::PathBuf::from(&change.install_path)])
            .map_err(|e| e.to_string())?;
        let saved = database
            .relocate_game(change.clone())
            .map_err(|e| e.to_string())?;
        tasks.update_registration(&change.expected_install_path, None);
        tasks.update_registration(&saved.install_path, Some(saved.id.clone()));
        Ok(saved)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn check_mtool(state: State<'_, AppState>) -> CommandResult<Vec<ToolCheck>> {
    let settings = with_db(&state, |db| db.settings())?;
    tauri::async_runtime::spawn_blocking(move || crate::maintenance::check_mtool(&settings))
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn run_mtool(state: State<'_, AppState>) -> CommandResult<()> {
    let (settings, permit) = {
        let mut count = state.activity.lock().map_err(|_| "操作锁不可用")?;
        if state.tasks.active() {
            return Err("请先结束当前任务".into());
        }
        let settings = with_db(&state, |db| db.settings())?;
        *count += 1;
        (settings, LaunchPermit(state.activity.clone()))
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        crate::maintenance::launch_mtool(&settings).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> CommandResult<Settings> {
    with_db(&state, |db| db.settings())
}
#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> CommandResult<Settings> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if state.tasks.active() {
        return Err("扫描或登记过程中请先取消任务，再更改设置".into());
    }
    if state.imports.has_pending_files() {
        return Err("请先恢复导入任务，再更改相关目录设置".into());
    }
    with_db(&state, |db| db.save_settings(settings))
}
#[tauri::command]
fn start_scan(state: State<'_, AppState>) -> CommandResult<String> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    let (settings, registered) =
        with_db(&state, |db| Ok((db.settings()?, db.registered_paths()?)))?;
    let workers = settings.scan_workers;
    if settings.game_root.is_empty() {
        return Err("请先配置 Game Root".into());
    }
    let job = state
        .tasks
        .begin("scan", settings.game_root)
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            jobs::run_scan(&job, workers, &registered)
        }));
        job.finish(match result {
            Ok(result) => result.map(|_| vec![]).map_err(|e| e.to_string()),
            Err(_) => Err("扫描任务异常退出".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn job_page(state: State<'_, AppState>, id: String, cursor: usize) -> CommandResult<JobPage> {
    Ok(state
        .tasks
        .get(&id)
        .map_err(|e| e.to_string())?
        .page(cursor))
}
#[tauri::command]
fn cancel_job(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state
        .tasks
        .get(&id)
        .map_err(|e| e.to_string())?
        .request_cancel();
    Ok(())
}
#[tauri::command]
fn skip_game(state: State<'_, AppState>, id: String, path: String) -> CommandResult<()> {
    state
        .tasks
        .get(&id)
        .map_err(|e| e.to_string())?
        .skip(&path)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn start_registration(
    state: State<'_, AppState>,
    scan_id: String,
    selections: Vec<RegistrationSelection>,
) -> CommandResult<String> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if selections.is_empty() || selections.len() > 5000 {
        return Err("请选择 1–5000 个候选".into());
    }
    let scan = state.tasks.get(&scan_id).map_err(|e| e.to_string())?;
    if scan.kind != "scan" || scan.is_active() {
        return Err("请先完成或取消扫描，再加入库".into());
    }
    let root = with_db(&state, |db| Ok(db.settings()?.game_root))?;
    if root != scan.root {
        return Err("Game Root 已改变，请重新扫描当前目录".into());
    }
    let mut seen = std::collections::HashSet::new();
    let candidates = selections
        .iter()
        .map(|s| {
            if !seen.insert(s.install_path.clone()) {
                return Err("选择中有重复目录".to_string());
            }
            let c = scan.candidate(&s.install_path).ok_or("扫描候选已过期")?;
            if c.status != "ready" && s.executable.is_none() {
                return Err("未完整分析的游戏请先手工选择启动文件".into());
            }
            Ok(c)
        })
        .collect::<CommandResult<Vec<_>>>()?;
    let job = state
        .tasks
        .begin("register", root.clone())
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> crate::domain::Result<Vec<String>> {
                let root = dunce::canonicalize(&root)?;
                job.stage("验证所选游戏", selections.len());
                let mut entries = vec![];
                for (candidate, selection) in candidates.into_iter().zip(selections) {
                    if job.stop() {
                        return Err(Error::Validation("登记已取消".into()));
                    }
                    job.progress(&candidate.install_path, Path::new(&candidate.install_path));
                    let entry = crate::registration::prepare(&root, candidate, selection)?;
                    job.completed_one(&entry.candidate.install_path);
                    entries.push(entry);
                }
                job.stage("写入游戏库", entries.len());
                let ids = db
                    .lock()
                    .map_err(|_| Error::Validation("数据库锁不可用".into()))?
                    .register_entries(&entries, &|| job.stop(), &|_, path| {
                        job.completed_one(path)
                    })?;
                for (entry, id) in entries.into_iter().zip(&ids) {
                    let mut candidate = entry.candidate;
                    candidate.registered_id = Some(id.clone());
                    scan.publish(candidate);
                }
                Ok(ids)
            },
        ));
        job.finish(match result {
            Ok(result) => result.map_err(|e| e.to_string()),
            Err(_) => Err("登记任务异常退出".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn save_game(state: State<'_, AppState>, edit: GameEdit) -> CommandResult<Game> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    if state.tasks.active()
        || state
            .imports
            .blocks_game(&with_db(&state, |db| db.game(&edit.id))?)
    {
        return Err("请先完成或恢复当前文件操作，再修改游戏资料".into());
    }
    with_db(&state, |db| db.edit_game(edit))
}
#[tauri::command]
fn start_game_analysis(state: State<'_, AppState>, id: String) -> CommandResult<String> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    let game = with_db(&state, |db| db.game(&id))?;
    let job = state
        .tasks
        .begin("analysis", game.install_path.clone())
        .map_err(|e| e.to_string())?;
    let task_id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        job.stage("分析 EXE / BAT", 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scanner::analyze_selected_controlled(
                Path::new(&game.install_path),
                if game.launch_type == "MTOOL" {
                    game.mtool_target_exe
                        .as_deref()
                        .or(game.main_executable.as_deref())
                } else {
                    game.main_executable.as_deref()
                },
                &|| job.stop(),
                &|current| job.progress(&game.install_path, current),
            )
        }));
        job.finish(match result {
            Ok(Ok(mut candidate)) if !job.stop() => {
                candidate.registered_id = Some(game.id);
                job.publish(candidate);
                job.completed_one(&game.install_path);
                Ok(vec![])
            }
            Ok(Ok(_)) => Err("分析已取消，原有结果保留".into()),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err("分析任务异常退出".into()),
        });
    });
    Ok(task_id)
}
#[tauri::command]
fn start_deep_analysis(
    state: State<'_, AppState>,
    scan_id: String,
    path: String,
) -> CommandResult<String> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    let previous = state.tasks.get(&scan_id).map_err(|e| e.to_string())?;
    if previous.kind != "scan" || previous.is_active() {
        return Err("请先完成或取消扫描".into());
    }
    let candidate = previous.candidate(&path).ok_or("候选已过期")?;
    // Reuse the same snapshot and task ID so unrelated preview choices survive.
    let tasks = state.tasks.clone();
    let job = tasks
        .begin("analysis", previous.root.clone())
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        job.stage("单独深度分析", 1);
        let result =
            scanner::analyze_controlled(Path::new(&path), true, &|| job.stop(), &|current| {
                job.progress(&path, current)
            });
        match result {
            Ok(mut updated) => {
                job.completed_one(&path);
                if job.stop() {
                    job.finish(Err("分析已取消".into()));
                } else {
                    updated.registered_id = candidate.registered_id;
                    previous.publish(updated);
                    job.finish(Ok(vec![]));
                }
            }
            Err(e) => job.finish(Err(e.to_string())),
        }
    });
    Ok(id)
}
struct LaunchPermit(Arc<Mutex<usize>>);
impl Drop for LaunchPermit {
    fn drop(&mut self) {
        if let Ok(mut count) = self.0.lock() {
            *count = count.saturating_sub(1);
        }
    }
}
#[tauri::command]
async fn play_game(
    state: State<'_, AppState>,
    id: String,
    configuration: Option<LaunchConfiguration>,
) -> CommandResult<Game> {
    let (game, settings, permit) = {
        let mut count = state.activity.lock().map_err(|_| "操作锁不可用")?;
        let pair = with_db(&state, |db| Ok((db.game(&id)?, db.settings()?)))?;
        if state.imports.blocks_game(&pair.0) {
            return Err("该游戏涉及未恢复的导入文件操作，请先在导入页处理".into());
        }
        *count += 1;
        (pair.0, pair.1, LaunchPermit(state.activity.clone()))
    };
    let db = state.db.clone();
    let running = state.running.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let game = match configuration {
            Some(config) => launcher::with_configuration(game, config),
            None => game,
        };
        launcher::launch_with_runtime(&game, &settings, &running, || {
            let mut database = db
                .lock()
                .map_err(|_| Error::Validation("数据库锁不可用".into()))?;
            database.record_launch(&id)
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn launch_history(state: State<'_, AppState>, id: String) -> CommandResult<Vec<String>> {
    with_db(&state, |db| db.launch_history(&id))
}
#[tauri::command]
async fn clear_library(
    state: State<'_, AppState>,
    confirmation: String,
) -> CommandResult<ResetReport> {
    if confirmation != "清空数据库" {
        return Err("请输入清空数据库以确认".into());
    }
    if state.imports.has_pending_files() {
        return Err("仍有未完成导入，不能清空数据库；请先继续或撤回".into());
    }
    let activity = state.activity.clone();
    let tasks = state.tasks.clone();
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = activity.lock().map_err(|_| "操作锁不可用")?;
        if *count > 0 {
            return Err("启动请求尚未结束，请稍后重试".into());
        }
        let job = tasks
            .begin("reset", String::new())
            .map_err(|e| e.to_string())?;
        let result = db
            .lock()
            .map_err(|_| "数据库锁不可用".to_string())
            .and_then(|mut db| db.clear_library().map_err(|e| e.to_string()));
        job.finish(result.as_ref().map(|_| vec![]).map_err(Clone::clone));
        if result.is_ok() {
            tasks.clear_finished().map_err(|e| e.to_string())?;
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn start_metadata_refresh(state: State<'_, AppState>) -> CommandResult<String> {
    let _gate = state.activity.lock().map_err(|_| "操作锁不可用")?;
    let games = with_db(&state, |db| db.games())?;
    let job = state
        .tasks
        .begin("metadata", String::new())
        .map_err(|e| e.to_string())?;
    let id = job.id.clone();
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> crate::domain::Result<Vec<String>> {
                job.stage("补充引擎与未知版本", games.len());
                let mut values = vec![];
                for game in games {
                    if job.stop() {
                        return Err(Error::Validation("补充已取消".into()));
                    }
                    match scanner::analyze_quick_selected(
                        Path::new(&game.install_path),
                        game.main_executable.as_deref(),
                        &|| job.stop(),
                        &|p| job.progress(&game.install_path, p),
                    ) {
                        Ok(mut candidate) if candidate.status == "ready" => {
                            (candidate.suggested_version, candidate.version_source) =
                                crate::version::suggest(
                                    &candidate.suggested_title,
                                    game.main_executable.as_deref(),
                                );
                            values.push((game.id, candidate));
                        }
                        Ok(_) => job.warn(format!("目录未完整读取：{}", game.install_path)),
                        Err(e) => job.warn(format!("{}：{e}", game.install_path)),
                    }
                    job.completed_one(&game.install_path);
                }
                job.stage("保存补充结果", values.len());
                db.lock()
                    .map_err(|_| Error::Validation("数据库锁不可用".into()))?
                    .supplement_metadata(&values, &|| job.stop())
            },
        ));
        job.finish(match result {
            Ok(r) => r.map_err(|e| e.to_string()),
            Err(_) => Err("补充任务异常退出".into()),
        });
    });
    Ok(id)
}
#[tauri::command]
fn open_game_folder(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    with_db(&state, |db| launcher::open_folder(&db.game(&id)?))
}
#[tauri::command]
async fn list_editable_saves(
    state: State<'_, AppState>,
    game_id: String,
) -> CommandResult<crate::save_editor::Catalog> {
    let game = with_db(&state, |db| db.game(&game_id))?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::save_editor::list(&game).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn read_editable_save(
    state: State<'_, AppState>,
    game_id: String,
    save_id: String,
) -> CommandResult<crate::save_editor::Document> {
    let game = with_db(&state, |db| db.game(&game_id))?;
    let external = state.external_saves.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if save_id.starts_with("external-") {
            external.read(&game, &save_id)
        } else {
            crate::save_editor::read(&game, &save_id)
        }
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn apply_save_edits(
    state: State<'_, AppState>,
    game_id: String,
    save_id: String,
    revision: String,
    changes: Vec<crate::save_editor::Change>,
    source_trusted: Option<bool>,
) -> CommandResult<crate::save_editor::Document> {
    let (game, permit) = save_write_permit(&state, &game_id)?;
    let external = state.external_saves.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        if save_id.starts_with("external-") {
            external.write(
                &game,
                &save_id,
                &revision,
                Some(&changes),
                source_trusted.unwrap_or(false),
            )
        } else {
            crate::save_editor::apply(&game, &save_id, &revision, &changes)
        }
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
fn save_write_permit(
    state: &AppState,
    game_id: &str,
) -> CommandResult<(Game, crate::save_editor::WritePermit)> {
    crate::save_editor::WritePermit::acquire(state.activity.clone(), || {
        let game = state
            .db
            .lock()
            .map_err(|_| Error::Validation("数据库锁不可用".into()))?
            .game(game_id)?;
        if state.tasks.active() || state.imports.blocks_game(&game) {
            return Err(Error::Validation(
                "当前有管理器文件任务或该游戏的未恢复导入操作，请先完成或恢复后再保存存档".into(),
            ));
        }
        Ok(game)
    })
    .map_err(|e| e.to_string())
}
#[tauri::command]
async fn resign_renpy_save(
    state: State<'_, AppState>,
    game_id: String,
    save_id: String,
    revision: String,
    source_trusted: bool,
) -> CommandResult<crate::save_editor::Document> {
    let (game, permit) = save_write_permit(&state, &game_id)?;
    let external = state.external_saves.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        if save_id.starts_with("external-") {
            external.write(&game, &save_id, &revision, None, source_trusted)
        } else {
            crate::save_editor::resign(&game, &save_id, &revision)
        }
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_external_renpy_save(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> CommandResult<Option<crate::save_editor::Document>> {
    let game = with_db(&state, |db| db.game(&game_id))?;
    let external = state.external_saves.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .set_title("选择外部 Ren’Py 存档（.save 或 persistent）")
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        external
            .choose(&game, &path)
            .map(Some)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn release_external_saves(state: State<'_, AppState>, game_id: String) -> CommandResult<()> {
    state
        .external_saves
        .release(&game_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn open_save_folder(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> CommandResult<()> {
    let game = with_db(&state, |db| db.game(&id))?;
    tauri::async_runtime::spawn_blocking(move || {
        launcher::open_save_folder(&game, &path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_save_directory(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    path: Option<String>,
) -> CommandResult<Option<String>> {
    let game = with_db(&state, |db| db.game(&id))?;
    tauri::async_runtime::spawn_blocking(move || {
        let initial = path
            .as_deref()
            .and_then(|p| launcher::save_folder(&game, p).ok())
            .or_else(|| {
                dunce::canonicalize(&game.install_path)
                    .ok()
                    .filter(|p| p.is_dir())
            });
        let mut picker = app.dialog().file().set_title("添加存档目录");
        if let Some(initial) = initial {
            picker = picker.set_directory(initial);
        }
        picker
            .blocking_pick_folder()
            .map(|file| {
                file.into_path()
                    .map(|p| p.display().to_string())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn preview_mtool_bat(state: State<'_, AppState>, id: String) -> CommandResult<String> {
    with_db(&state, |db| {
        launcher::debug_bat(&db.game(&id)?, &db.settings()?)
    })
}
#[tauri::command]
fn suggest_version(folder: String, executable: Option<String>) -> (String, String) {
    crate::version::suggest(&folder, executable.as_deref())
}
#[tauri::command]
async fn choose_directory(app: tauri::AppHandle) -> CommandResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择目录")
            .blocking_pick_folder()
            .map(|file| {
                file.into_path()
                    .map(|p| p.display().to_string())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_launch_file(
    app: tauri::AppHandle,
    root: String,
    extension: Option<String>,
) -> CommandResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
        let picker = app.dialog().file().set_directory(&root);
        let picker = match extension.as_deref() {
            Some("exe") => picker
                .add_filter("本地 QSP 播放器", &["exe"])
                .set_title("选择游戏目录内的 QSP 播放器"),
            Some("qsp") => picker
                .add_filter("QSP 游戏文件", &["qsp"])
                .set_title("选择 QSP 主游戏文件"),
            None => picker
                .add_filter("常用启动文件", &["exe", "html", "htm", "qsp", "gam", "swf"])
                .add_filter("所有文件", &["*"])
                .set_title("选择游戏目录内的启动文件"),
            _ => return Err("不支持的文件类型".into()),
        };
        let selected = picker.blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let selected = dunce::canonicalize(selected.into_path().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let relative = selected
            .strip_prefix(&root)
            .map_err(|_| "请选择这个游戏目录内的启动文件")?;
        let value = paths::path_text(relative).map_err(|e| e.to_string())?;
        if let Some(extension) = extension {
            paths::contained_file(&root, &value, &extension).map_err(|e| e.to_string())?;
        } else {
            paths::launch_file(&root, &value).map_err(|e| e.to_string())?;
        }
        Ok(Some(value))
    })
    .await
    .map_err(|e| e.to_string())?
}
fn initialize(app: &tauri::AppHandle) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let data = app.path().app_local_data_dir()?;
    crate::data_directory::ensure_writable(&data)?;
    let data = dunce::canonicalize(data)?;
    #[cfg(windows)]
    let data_guard = crate::data_directory::InstanceGuard::acquire(&data)?;
    std::fs::create_dir_all(app.path().app_cache_dir()?)?;
    std::fs::create_dir_all(app.path().app_log_dir()?)?;
    let appearance = crate::appearance::load(&data).unwrap_or_else(|error| {
        eprintln!("Unable to load appearance settings: {error}");
        crate::appearance::Appearance::default()
    });
    app.manage(AppState {
        #[cfg(windows)]
        _data_guard: data_guard,
        appearance: Mutex::new(appearance),
        db: Arc::new(Mutex::new(Database::open(
            &data.join(crate::data_directory::DATABASE),
        )?)),
        tasks: Arc::new(TaskManager::default()),
        activity: Arc::new(Mutex::new(0)),
        imports: Arc::new(ImportStore::open(data.join("imports"))?),
        data,
        deletion_plans: Arc::new(Mutex::new(std::collections::HashMap::new())),
        duplicate_plans: Arc::new(Mutex::new(std::collections::HashMap::new())),
        running: Arc::new(crate::runtime::RunningGames::default()),
        external_saves: Arc::new(crate::save_editor::ExternalSaves::default()),
    });
    // Create the WebView after validating and opening this build's data directory.
    let window =
        tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?.build()?;
    #[cfg(windows)]
    style_native_title_bar(&window);
    window.set_icon(crate::appearance::icon(appearance.icon))?;
    if let Err(error) = crate::window_placement::position(&window) {
        eprintln!("Unable to position startup window: {error}");
        let _ = window.center();
    }
    window.show()?;
    let _ = window.set_focus();
    Ok(())
}

pub fn run() {
    let mut context = tauri::generate_context!();
    if let Some(root) = crate::data_directory::development_override(
        cfg!(windows),
        cfg!(debug_assertions) || tauri::is_dev(),
    ) {
        context.config_mut().app.app_directories_override = Some(
            tauri::utils::config::AppDirectoriesOverride::Root(root.into()),
        );
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(error) = initialize(&handle) {
                    handle
                        .dialog()
                        .message(format!("butter-manager 无法启动：{error}"))
                        .title("启动失败")
                        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                        .blocking_show();
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.state::<AppState>().tasks.cancel_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_data_directory,
            get_appearance,
            save_appearance,
            list_games,
            choose_import_sources,
            discover_import_sources,
            start_import_analysis,
            import_matches,
            import_plans,
            import_recovery_issues,
            preview_import_duplicate,
            recycle_import_duplicate,
            open_import_records,
            start_import_plan,
            start_import_apply,
            discard_import_plan,
            version_history,
            start_version_rollback,
            games_by_ids,
            sync_scan_mtool,
            preview_mtool_launch,
            start_library_check,
            remove_game,
            preview_game_delete,
            delete_game_files,
            preview_relocation,
            relocate_game,
            check_mtool,
            run_mtool,
            get_settings,
            save_settings,
            start_scan,
            job_page,
            cancel_job,
            skip_game,
            start_registration,
            save_game,
            start_game_analysis,
            start_deep_analysis,
            play_game,
            launch_history,
            clear_library,
            start_metadata_refresh,
            open_game_folder,
            open_save_folder,
            list_editable_saves,
            read_editable_save,
            apply_save_edits,
            resign_renpy_save,
            choose_external_renpy_save,
            release_external_saves,
            choose_save_directory,
            preview_mtool_bat,
            choose_directory,
            choose_launch_file,
            suggest_version
        ])
        .run(context)
        .expect("butter-manager 启动失败");
}

#[cfg(windows)]
fn style_native_title_bar(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    // COLORREF stores RGB in the low-to-high bytes; match the light-blue webview theme.
    for (attribute, color) in [
        (DWMWA_CAPTION_COLOR, 0x00ff_f5edu32),
        (DWMWA_TEXT_COLOR, 0x0050_2a18u32),
    ] {
        // SAFETY: the owned window handle is live; color remains valid during the synchronous call.
        // Older Windows versions may reject these optional colors; keep native controls usable.
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0 as _,
                attribute as u32,
                (&color as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}
