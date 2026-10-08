//! Explicit, server-generated deletion plans. No permanent-delete fallback.
use crate::domain::{Error, Game, Result, Settings};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveAction {
    pub path: String,
    pub action: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletePlan {
    pub token: String,
    pub id: String,
    pub title: String,
    pub game_path: String,
    pub saves: Vec<SaveAction>,
    pub blockers: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct DeleteReport {
    pub removed: bool,
    pub recycled: Vec<String>,
    pub error: Option<String>,
}
fn text(path: &Path) -> Result<String> {
    crate::paths::path_text(path)
}
fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
fn fail(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

// Resolve only explicit configured paths; never expand wildcards or guess save locations.
pub fn resolve_save(game: &Game, value: &str) -> Result<PathBuf> {
    let mut value = value.trim().replace("<GAME>", &game.install_path);
    let mut expansions = 0;
    while let Some(start) = value.find('%') {
        expansions += 1;
        if expansions > 32 {
            return Err(fail("存档环境变量展开存在循环"));
        }
        let end = value[start + 1..]
            .find('%')
            .map(|i| i + start + 1)
            .ok_or_else(|| fail("存档路径中的环境变量不完整"))?;
        let key = &value[start + 1..end];
        let expanded = std::env::var(key).map_err(|_| fail(format!("无法解析存档变量 %{key}%")))?;
        value.replace_range(start..=end, &expanded);
    }
    if value.is_empty() || value.contains(['<', '>', '*', '?', '\0']) {
        return Err(fail("请先把存档位置配置为明确的文件或文件夹路径"));
    }
    let path = PathBuf::from(value.replace('\\', "/"));
    let path = if path.is_absolute() {
        path
    } else {
        Path::new(&game.install_path).join(crate::paths::relative_path(&value)?)
    };
    // Resolve existing ancestors even for a missing target, to catch traversal/links.
    plain_path(&path)?;
    absolute_path(&path)
}
fn absolute_path(path: &Path) -> Result<PathBuf> {
    match dunce::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| fail("路径没有父目录"))?;
            let name = path
                .file_name()
                .ok_or_else(|| fail("路径不是明确的文件或文件夹"))?;
            if name == ".." || name == "." {
                return Err(fail("路径不能包含目录跳转"));
            }
            Ok(absolute_path(parent)?.join(name))
        }
        Err(e) => Err(e.into()),
    }
}
fn link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn plain_path(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(ancestor) {
            if link(&meta) {
                return Err(fail(format!(
                    "路径含链接或联接点，不能自动删除：{}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}
fn check_tree(path: &Path, budget: &mut usize) -> Result<()> {
    if *budget == 0 {
        return Err(fail("目录过大，请手动检查后处理"));
    }
    *budget -= 1;
    let meta = fs::symlink_metadata(path)?;
    if link(&meta) {
        return Err(fail(format!("目录含链接或联接点：{}", path.display())));
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            check_tree(&entry?.path(), budget)?;
        }
    }
    Ok(())
}
fn protected(
    path: &Path,
    settings: &Settings,
    data: &Path,
    games: &[Game],
    id: &str,
) -> Result<()> {
    if path.parent().is_none() {
        return Err(fail("不能删除磁盘根目录"));
    }
    for root in [
        Some(data.to_owned()),
        (!settings.game_root.is_empty()).then(|| PathBuf::from(&settings.game_root)),
        (!settings.mtool_root.is_empty()).then(|| PathBuf::from(&settings.mtool_root)),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_owned)),
    ]
    .into_iter()
    .flatten()
    {
        let root = absolute_path(&root)?;
        if path == root || root.starts_with(path) || (root == data && path.starts_with(&root)) {
            return Err(fail(format!(
                "不能删除管理器、游戏库或公共工具目录：{}",
                path.display()
            )));
        }
    }
    for key in [
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "WINDIR",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        if let Ok(root) = std::env::var(key) {
            if absolute_path(Path::new(&root)).is_ok_and(|root| root.starts_with(path)) {
                return Err(fail("不能删除用户资料或系统目录"));
            }
        }
    }
    for other in games.iter().filter(|g| g.id != id) {
        let root = absolute_path(Path::new(&other.install_path))?;
        if overlaps(path, &root) {
            return Err(fail(format!(
                "删除范围与另一游戏重叠：{}",
                other.display_title
            )));
        }
        for save in &other.save_paths {
            if resolve_save(other, save).is_ok_and(|save| overlaps(path, &save)) {
                return Err(fail(format!("此路径包含共用存档：{}", other.display_title)));
            }
        }
    }
    Ok(())
}

pub fn preview(
    game: &Game,
    games: &[Game],
    settings: &Settings,
    data: &Path,
    token: &str,
) -> DeletePlan {
    let mut plan = DeletePlan {
        token: token.into(),
        id: game.id.clone(),
        title: game.display_title.clone(),
        game_path: game.install_path.clone(),
        saves: vec![],
        blockers: vec![],
    };
    let result = (|| -> Result<()> {
        let raw = Path::new(&game.install_path);
        plain_path(raw)?;
        let root = dunce::canonicalize(raw)?;
        if !root.is_dir() {
            return Err(fail("游戏目录不存在，可选择仅从库中移除"));
        }
        plan.game_path = text(&root)?;
        protected(&root, settings, data, games, &game.id)?;
        check_tree(&root, &mut 1_000_000)?;
        let mut saves = Vec::new();
        for configured in &game.save_paths {
            let save = resolve_save(game, configured)?;
            plain_path(Path::new(&configured.replace("<GAME>", &game.install_path)))?;
            plain_path(&save)?;
            if save == root || root.starts_with(&save) {
                return Err(fail(
                    "存档位置不能是游戏目录本身或其父目录，请指定实际存档子目录或文件",
                ));
            }
            if !save.try_exists()? {
                plan.saves.push(SaveAction {
                    path: text(&save)?,
                    action: "missing".into(),
                });
                continue;
            }
            protected(&save, settings, data, games, &game.id)?;
            check_tree(&save, &mut 1_000_000)?;
            saves.push(save);
        }
        saves.sort();
        saves.dedup();
        let selected = saves
            .iter()
            .filter(|s| !saves.iter().any(|p| p != *s && s.starts_with(p)))
            .collect::<Vec<_>>();
        for save in selected {
            plan.saves.push(SaveAction {
                path: text(save)?,
                action: "recycle".into(),
            });
        }
        Ok(())
    })();
    if let Err(e) = result {
        plan.blockers.push(e.to_string());
    }
    plan
}

pub fn apply_files(
    plan: &DeletePlan,
    mut recycle: impl FnMut(&Path) -> Result<()>,
) -> DeleteReport {
    let mut report = DeleteReport {
        removed: false,
        recycled: vec![],
        error: None,
    };
    let result =
        (|| -> Result<()> {
            if !plan.blockers.is_empty() {
                return Err(fail(plan.blockers.join("；")));
            }
            let mut paths = vec![PathBuf::from(&plan.game_path)];
            paths.extend(
                plan.saves
                    .iter()
                    .filter(|s| s.action == "recycle")
                    .map(|s| PathBuf::from(&s.path)),
            );
            crate::runtime::ensure_paths_idle(&paths)?;
            // External saves first; internal saves are recycled with the game directory.
            // All failures leave the database record intact and report completed steps.
            for save in plan.saves.iter().filter(|s| {
                s.action == "recycle" && !Path::new(&s.path).starts_with(&plan.game_path)
            }) {
                recycle(Path::new(&save.path))?;
                if Path::new(&save.path).try_exists()? {
                    return Err(fail("存档仍在原位，回收站操作未完成"));
                }
                report.recycled.push(save.path.clone());
            }
            recycle(Path::new(&plan.game_path))?;
            if Path::new(&plan.game_path).try_exists()? {
                return Err(fail("游戏目录仍在原位，回收站操作未完成"));
            }
            report.recycled.push(plan.game_path.clone());
            Ok(())
        })();
    if let Err(error) = result {
        report.error = Some(format!("文件操作未全部完成，库记录保留：{error}"));
    }
    report
}

pub fn recycle(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let path = path.to_owned();
        std::thread::spawn(move || crate::recycle_windows::recycle(&path))
            .join()
            .map_err(|_| fail("回收站操作异常退出"))?
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(fail("文件删除仅支持 Windows 回收站"))
    }
}

#[cfg(test)]
#[path = "deletion_test.rs"]
mod tests;
