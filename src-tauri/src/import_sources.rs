//! Read-only selection discovery. A game root and a collection share one picker.
use crate::domain::{Error, Result};
use crate::{paths, scanner};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Default, Serialize)]
pub struct Discovery {
    pub sources: Vec<String>,
    pub choices: Vec<ScopeChoice>,
    pub warnings: Vec<String>,
}
#[derive(Serialize)]
pub struct ScopeChoice {
    pub root: String,
    pub children: Vec<String>,
}
struct Probe {
    direct_game: bool,
    files: bool,
    children: Vec<PathBuf>,
}
struct Budget {
    started: Instant,
    entries: usize,
}
impl Budget {
    fn expired(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(5) || self.entries >= 20_000
    }
    fn check(&self) -> Result<()> {
        if self.expired() {
            Err(Error::Validation(
                "文件夹较多或读取较慢，请改选具体游戏文件夹".into(),
            ))
        } else {
            Ok(())
        }
    }
}
fn probe(root: &Path, budget: &mut Budget) -> Result<Probe> {
    let mut result = Probe {
        direct_game: false,
        files: false,
        children: vec![],
    };
    for entry in fs::read_dir(root)? {
        budget.check()?;
        budget.entries += 1;
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if scanner::excluded_name(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if scanner::is_link(&metadata) || scanner::hidden_or_system(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            result.children.push(entry.path());
        } else if metadata.is_file() {
            result.files = true;
            result.direct_game |= name.ends_with(".exe") && !scanner::helper(&name)
                || [".qsp", ".html", ".htm", ".gam", ".swf"]
                    .iter()
                    .any(|extension| name.ends_with(extension));
        }
    }
    result.children.sort();
    Ok(result)
}

pub fn discover(selected: &[String]) -> Result<Discovery> {
    if selected.is_empty() || selected.len() > 5000 {
        return Err(Error::Validation("请选择 1–5000 个文件夹".into()));
    }
    let mut result = Discovery::default();
    let mut budget = Budget {
        started: Instant::now(),
        entries: 0,
    };
    let mut seen = HashSet::new();
    for selected in selected {
        budget.check()?;
        let path = Path::new(selected);
        if scanner::excluded_name(&path.file_name().unwrap_or_default().to_string_lossy()) {
            result
                .warnings
                .push(format!("已跳过隐藏或系统文件夹：{selected}"));
            continue;
        }
        let metadata = fs::symlink_metadata(path)?;
        if scanner::hidden_or_system(&metadata) {
            result
                .warnings
                .push(format!("已跳过隐藏或系统文件夹：{selected}"));
            continue;
        }
        if !metadata.is_dir() || scanner::is_link(&metadata) {
            return Err(Error::Validation(
                "请选择实际文件夹，链接目录不参与自动识别".into(),
            ));
        }
        let root = dunce::canonicalize(path)?;
        if !seen.insert(paths::path_key(&root)?) {
            continue;
        }
        let outer = probe(&root, &mut budget)?;
        if outer.direct_game {
            result.sources.push(paths::path_text(&root)?);
            continue;
        }
        let mut games = vec![];
        let mut uncertain = false;
        for child in &outer.children {
            budget.check()?;
            if scanner::resource_directory(
                &child
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
            ) {
                continue;
            }
            let inner = probe(child, &mut budget)?;
            if inner.direct_game {
                games.push(paths::path_text(child)?);
            } else if !inner.children.is_empty() {
                let candidate =
                    scanner::analyze_quick_controlled(child, &|| budget.expired(), &|_| {})?;
                budget.entries += candidate.entries_scanned;
                budget.check()?;
                if candidate.qsp.is_some() || !candidate.executables.is_empty() {
                    games.push(paths::path_text(child)?);
                    uncertain |= candidate.status != "ready";
                }
            }
        }
        if games.is_empty() {
            if outer.files || !outer.children.is_empty() {
                result.sources.push(paths::path_text(&root)?);
            } else {
                result.warnings.push(format!(
                    "文件夹中没有可识别的内容（隐藏及系统项已忽略）：{}",
                    root.display()
                ));
            }
        } else if !outer.files && games.len() == outer.children.len() && !uncertain {
            result.sources.extend(games);
        } else {
            result.choices.push(ScopeChoice {
                root: paths::path_text(&root)?,
                children: games,
            });
        }
    }
    let mut seen = HashSet::new();
    result
        .sources
        .retain(|source| seen.insert(source.to_lowercase()));
    if result.sources.len() > 5000 {
        return Err(Error::Validation(
            "识别出的游戏超过 5000 个，请分批选择".into(),
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn select(path: &Path) -> Discovery {
        discover(&[paths::path_text(path).unwrap()]).unwrap()
    }
    #[test]
    fn collection_expands_visible_games_and_never_traverses_dot_folders() {
        let temp = tempfile::Builder::new()
            .prefix("import-sources-")
            .tempdir()
            .unwrap();
        for folder in ["真实游戏", ".accelerate", ".git", ".butter-import-private"] {
            fs::create_dir(temp.path().join(folder)).unwrap();
            fs::write(temp.path().join(folder).join("Game.exe"), b"fixture").unwrap();
        }
        fs::write(temp.path().join(".DS_Store"), b"fixture").unwrap();
        let result = select(temp.path());
        assert_eq!(
            result.sources,
            vec![paths::path_text(&temp.path().join("真实游戏")).unwrap()]
        );
        assert!(result.choices.is_empty());
        let scan = scanner::scan_root(temp.path()).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert!(temp.path().join(".accelerate/Game.exe").is_file());
    }
    #[test]
    fn local_qsp_root_stays_whole_and_overlapping_selections_are_deduplicated() {
        let temp = tempfile::Builder::new()
            .prefix("import-sources-")
            .tempdir()
            .unwrap();
        let game = temp.path().join("彼女の冒険");
        fs::create_dir_all(game.join("Qqsp")).unwrap();
        fs::write(game.join("游戏.qsp"), b"fixture").unwrap();
        fs::write(game.join("Qqsp/播放器.exe"), b"fixture").unwrap();
        let result = select(&game);
        assert_eq!(result.sources, vec![paths::path_text(&game).unwrap()]);
        assert!(result.choices.is_empty());
        let result = discover(&[
            paths::path_text(temp.path()).unwrap(),
            paths::path_text(&game).unwrap(),
        ])
        .unwrap();
        assert_eq!(result.sources.len(), 1);
    }
    #[test]
    fn wrapper_with_root_configuration_or_sibling_resources_requires_scope_choice() {
        let temp = tempfile::Builder::new()
            .prefix("import-sources-")
            .tempdir()
            .unwrap();
        fs::create_dir_all(temp.path().join("bin")).unwrap();
        fs::create_dir_all(temp.path().join("images")).unwrap();
        fs::write(temp.path().join("bin/Game.exe"), b"fixture").unwrap();
        fs::write(temp.path().join("config.ini"), b"fixture").unwrap();
        let result = select(temp.path());
        assert!(result.sources.is_empty());
        assert_eq!(result.choices.len(), 1);
        assert_eq!(
            result.choices[0].root,
            paths::path_text(temp.path()).unwrap()
        );
        assert_eq!(
            result.choices[0].children,
            vec![paths::path_text(&temp.path().join("bin")).unwrap()]
        );
    }
    #[cfg(windows)]
    #[test]
    fn windows_hidden_and_system_items_are_not_game_candidates() {
        let temp = tempfile::Builder::new()
            .prefix("import-sources-")
            .tempdir()
            .unwrap();
        for (folder, attribute) in [("Hidden", "+h"), ("System", "+s")] {
            let path = temp.path().join(folder);
            fs::create_dir(&path).unwrap();
            fs::write(path.join("game.qsp"), b"fixture").unwrap();
            assert!(std::process::Command::new("attrib.exe")
                .arg(attribute)
                .arg(&path)
                .status()
                .unwrap()
                .success());
            assert!(select(&path).sources.is_empty());
        }
        let hidden_file = temp.path().join("hidden.qsp");
        fs::write(&hidden_file, b"fixture").unwrap();
        assert!(std::process::Command::new("attrib.exe")
            .arg("+h")
            .arg(&hidden_file)
            .status()
            .unwrap()
            .success());
        assert!(select(temp.path()).sources.is_empty());
        assert!(scanner::scan_root(temp.path())
            .unwrap()
            .candidates
            .is_empty());
    }
}
