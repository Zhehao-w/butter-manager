//! Exact root remapping of existing records; this module never changes game files.
use crate::domain::{Error, Game, Result, Settings};
use crate::paths::{path_key, path_text, relative_path};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub old_path: String,
    pub new_path: Option<String>,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Tool {
    pub old_path: String,
    pub new_path: Option<String>,
    pub state: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub token: String,
    pub old_root: String,
    pub new_root: String,
    pub items: Vec<Item>,
    pub mtool: Tool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub processed: usize,
    pub total: usize,
    pub current: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    path: PathBuf,
    id: String,
    directory: bool,
    size: u64,
    modified: Option<std::time::SystemTime>,
}
#[derive(Clone)]
pub struct Plan {
    pub view: Preview,
    pub revision: (u64, i64),
    pub games: Vec<Game>,
    pub settings: Settings,
    identities: Vec<Identity>,
}
#[derive(Serialize)]
pub struct Report {
    pub migrated: usize,
    pub skipped: usize,
    pub mtool_migrated: bool,
    pub settings: Settings,
    pub games: Vec<Game>,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

/// Lexical absolute paths, including missing old roots. Never use string prefix replacement.
fn absolute(value: &str) -> Result<PathBuf> {
    let value = value.trim().replace('\\', "/");
    let path = PathBuf::from(if cfg!(windows) {
        value.replace('/', "\\")
    } else {
        value
    });
    let path = dunce::simplified(&path);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid("请填写明确的绝对目录，不能包含 .."));
    }
    Ok(path.components().collect())
}
pub(crate) fn remap(value: &str, old: &Path, new: &Path) -> Result<Option<PathBuf>> {
    let path = absolute(value)?;
    let parts = path.components().collect::<Vec<_>>();
    let prefix = old.components().collect::<Vec<_>>();
    if parts.len() < prefix.len()
        || !parts.iter().zip(&prefix).all(|(a, b)| {
            if cfg!(windows) {
                a.as_os_str().to_string_lossy().to_lowercase()
                    == b.as_os_str().to_string_lossy().to_lowercase()
            } else {
                a == b
            }
        })
    {
        return Ok(None);
    }
    let suffix: PathBuf = parts[prefix.len()..]
        .iter()
        .map(|part| part.as_os_str())
        .collect();
    Ok(Some(new.join(suffix)))
}
#[derive(Default)]
struct Checks {
    plain: std::collections::HashSet<PathBuf>,
    identities: std::collections::HashMap<PathBuf, Identity>,
}
impl Checks {
    // Cache only inside one preview / revalidation; never reuse across confirmation.
    fn plain(&mut self, path: &Path) -> Result<()> {
        let mut checked = Vec::new();
        for ancestor in path.ancestors() {
            if self.plain.contains(ancestor) {
                break;
            }
            let metadata = std::fs::symlink_metadata(ancestor)?;
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = metadata.file_type().is_symlink();
            if linked {
                return Err(invalid("目标路径含链接或联接点，不能自动迁移"));
            }
            checked.push(ancestor.to_owned());
        }
        self.plain.extend(checked);
        Ok(())
    }
    fn identity(&mut self, path: &Path) -> Result<Identity> {
        if let Some(identity) = self.identities.get(path) {
            return Ok(identity.clone());
        }
        self.plain(path)?;
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(invalid("目标不是普通文件或目录"));
        }
        let identity = Identity {
            path: path.to_owned(),
            id: crate::importer::metadata_identity(path)?,
            directory: metadata.is_dir(),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified: if metadata.is_dir() {
                None
            } else {
                Some(metadata.modified()?)
            },
        };
        self.identities.insert(path.to_owned(), identity.clone());
        Ok(identity)
    }
}
fn validate_game(game: &Game, target: &Path, checks: &mut Checks) -> Result<Vec<Identity>> {
    checks.plain(target)?;
    let relocated = crate::maintenance::relocation_plan(game, target)?;
    if relocated.main_executable != game.main_executable {
        return Err(invalid("已配置的主启动文件缺失或失效"));
    }
    if relocated.mtool_target_exe != game.mtool_target_exe {
        return Err(invalid("已配置的 MTool target 缺失或失效"));
    }
    // Unlike single-game relocation, never substitute a working directory or clear configuration.
    if game.working_directory != "." {
        checks.plain(&target.join(relative_path(&game.working_directory)?))?;
    }
    let working = crate::paths::working_directory(target, &game.working_directory)?;
    if !["DIRECT", "MTOOL", "EXTERNAL_PLAYER"].contains(&game.launch_type.as_str()) {
        return Err(invalid("现有启动方式不支持，请先在游戏详情中调整"));
    }
    if let Some(config) = &game.external_player {
        crate::external_player::validate(target, game.main_executable.as_deref(), config)?;
    }
    let mut paths = vec![target.to_owned(), working];
    for file in [
        game.main_executable.as_deref(),
        game.mtool_target_exe.as_deref(),
        game.external_player
            .as_ref()
            .and_then(|config| config.game_file.as_deref()),
    ]
    .into_iter()
    .flatten()
    {
        let raw = target.join(relative_path(file)?);
        checks.plain(&raw)?;
        paths.push(dunce::canonicalize(raw)?);
    }
    paths.sort();
    paths.dedup();
    paths.iter().map(|path| checks.identity(path)).collect()
}

pub fn preview(
    games: Vec<Game>,
    settings: Settings,
    revision: (u64, i64),
    old: &str,
    new: &str,
    token: &str,
) -> Result<Plan> {
    preview_with_progress(games, settings, revision, old, new, token, |_| {})
}
pub fn preview_with_progress(
    games: Vec<Game>,
    settings: Settings,
    revision: (u64, i64),
    old: &str,
    new: &str,
    token: &str,
    mut progress: impl FnMut(Progress),
) -> Result<Plan> {
    let total = games.len() + 1;
    progress(Progress {
        processed: 0,
        total,
        current: "验证新根目录".into(),
    });
    let old = absolute(old)?;
    let new_raw = absolute(new)?;
    let mut checks = Checks::default();
    checks.plain(&new_raw)?;
    let new = dunce::canonicalize(new_raw)?;
    if !new.is_dir() || path_key(&old)? == path_key(&new)? {
        return Err(invalid("新根目录必须存在，且与旧根目录不同"));
    }
    let mut identities = vec![checks.identity(&new)?];
    let mut items = Vec::with_capacity(games.len());
    let mut claimed = std::collections::HashSet::new();
    let mut registered = std::collections::HashMap::<String, Vec<&str>>::new();
    for game in &games {
        let path =
            absolute(&game.install_path).unwrap_or_else(|_| PathBuf::from(&game.install_path));
        // Registered install paths are normalized on write. Missing old roots need no I/O.
        registered
            .entry(path_key(&path)?)
            .or_default()
            .push(&game.id);
    }
    for (index, game) in games.iter().enumerate() {
        let mut item = Item {
            id: game.id.clone(),
            title: game.display_title.clone(),
            old_path: game.install_path.clone(),
            new_path: None,
            reason: None,
        };
        progress(Progress {
            processed: index,
            total,
            current: game.display_title.clone(),
        });
        let result = (|| -> Result<()> {
            let target = remap(&game.install_path, &old, &new)?
                .ok_or_else(|| invalid("原路径不在指定旧根目录内"))?;
            item.new_path = Some(path_text(&target)?);
            let guards = validate_game(game, &target, &mut checks)?;
            let target = dunce::canonicalize(target)?;
            let key = path_key(&target)?;
            if registered
                .get(&key)
                .is_some_and(|owners| owners.iter().any(|owner| *owner != game.id))
                || !claimed.insert(key)
            {
                return Err(invalid("目标路径已被其他游戏 UUID 占用或映射冲突"));
            }
            item.new_path = Some(path_text(&target)?);
            identities.extend(guards);
            Ok(())
        })();
        if let Err(error) = result {
            item.reason = Some(error.to_string());
        }
        items.push(item);
        progress(Progress {
            processed: index + 1,
            total,
            current: game.display_title.clone(),
        });
    }
    progress(Progress {
        processed: games.len(),
        total,
        current: "公共 MTool".into(),
    });
    let mut mtool = Tool {
        old_path: settings.mtool_root.clone(),
        new_path: None,
        state: "unchanged".into(),
        reason: "公共 MTool 不在旧根目录内，设置保持原样".into(),
    };
    if !settings.mtool_root.is_empty() {
        match remap(&settings.mtool_root, &old, &new) {
            Ok(Some(target)) => {
                mtool.new_path = Some(path_text(&target)?);
                let result = (|| -> Result<Vec<Identity>> {
                    checks.plain(&target)?;
                    let mut changed = settings.clone();
                    changed.mtool_root = path_text(&dunce::canonicalize(&target)?)?;
                    let mut guards = vec![checks.identity(&target)?];
                    for check in crate::maintenance::check_mtool(&changed) {
                        if !check.available {
                            return Err(invalid(format!("{}：{}", check.label, check.message)));
                        }
                        guards.push(checks.identity(Path::new(&check.path))?);
                    }
                    for loader in games
                        .iter()
                        .filter(|game| game.launch_type == "MTOOL")
                        .filter_map(|game| game.mtool_loader.as_deref())
                        .collect::<std::collections::BTreeSet<_>>()
                    {
                        let raw = target.join(relative_path(loader)?);
                        crate::paths::contained_file(&target, loader, "dll")?;
                        guards.push(checks.identity(&raw)?);
                    }
                    mtool.new_path = Some(changed.mtool_root);
                    Ok(guards)
                })();
                match result {
                    Ok(guards) => {
                        identities.extend(guards);
                        mtool.state = "ready".into();
                        mtool.reason = "已验证，保留注入器及运行程序的相对配置".into();
                    }
                    Err(error) => {
                        mtool.state = "blocked".into();
                        mtool.reason = format!("{}；MTool 设置不变，有效游戏仍可迁移", error);
                    }
                }
            }
            Err(error) => {
                mtool.state = "blocked".into();
                mtool.reason = error.to_string();
            }
            Ok(None) => {}
        }
    }
    let view = Preview {
        token: token.into(),
        old_root: path_text(&old)?,
        new_root: path_text(&new)?,
        items,
        mtool,
    };
    identities.sort_by(|a, b| a.path.cmp(&b.path));
    identities.dedup_by(|a, b| a.path == b.path);
    progress(Progress {
        processed: total,
        total,
        current: "检查完成".into(),
    });
    Ok(Plan {
        view,
        revision,
        games,
        settings,
        identities,
    })
}

pub fn revalidate(plan: &Plan) -> Result<()> {
    ensure_migratable(plan)?;
    let current = preview(
        plan.games.clone(),
        plan.settings.clone(),
        plan.revision,
        &plan.view.old_root,
        &plan.view.new_root,
        &plan.view.token,
    )?;
    if current.view != plan.view || current.identities != plan.identities {
        return Err(invalid("目标目录或配置已变化，请重新预览迁移"));
    }
    Ok(())
}

pub(crate) fn ensure_migratable(plan: &Plan) -> Result<()> {
    if !plan.view.items.iter().any(|item| item.reason.is_none()) {
        return Err(invalid("没有可迁移的游戏，请检查新旧根目录"));
    }
    Ok(())
}

pub fn ensure_idle(activity: usize, active: bool, pending: bool) -> Result<()> {
    if activity != 0 || active || pending {
        return Err(invalid(
            "请先结束当前任务、启动或存档写入请求，并解决未完成的 Import Recovery",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "library_relocation_test.rs"]
mod tests;
