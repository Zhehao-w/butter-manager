//! Explicit disposal of a reviewed incoming copy. Library records are never removed here.
use crate::deletion::{self, DeletePlan};
use crate::domain::{Error, Game, Result, ScanCandidate, Settings};
use crate::{importer, paths};
use serde::Serialize;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn version_key(value: &str) -> String {
    let value = value.nfkc().collect::<String>().trim().to_lowercase();
    for prefix in ["version", "ver", "v"] {
        if let Some(number) = value.strip_prefix(prefix) {
            let number = number.trim_start_matches(['.', ' ', '_', '-']);
            if number.starts_with(|character: char| character.is_ascii_digit()) {
                return number.into();
            }
        }
    }
    value
}
pub fn same_version(incoming: &str, installed: &str) -> bool {
    let incoming = version_key(incoming);
    !["", "-", "unknown", "未知", "未识别"].contains(&incoming.as_str())
        && incoming == version_key(installed)
}

#[derive(Clone, Serialize)]
pub struct DuplicatePlan {
    #[serde(flatten)]
    files: DeletePlan,
    pub existing_id: String,
    pub existing_title: String,
    pub existing_path: String,
    pub version: String,
    #[serde(skip)]
    candidate: ScanCandidate,
    #[serde(skip)]
    incoming_version: String,
    #[serde(skip)]
    source_identity: String,
}

pub fn preview(
    candidate: &ScanCandidate,
    game: &Game,
    incoming_version: &str,
    games: &[Game],
    settings: &Settings,
    data: &Path,
    token: &str,
) -> Result<DuplicatePlan> {
    if !same_version(incoming_version, &game.current_version) {
        return Err(invalid("仅在关联游戏且版本号明确相同时，才能删除导入副本"));
    }
    let source = Path::new(&candidate.install_path);
    let source_identity = importer::identity(source)?;
    let source_key = std::path::PathBuf::from(paths::path_key(&dunce::canonicalize(source)?)?);
    for root in [&settings.game_root, &settings.mtool_root] {
        if !root.is_empty() {
            let root = std::path::PathBuf::from(paths::path_key(&dunce::canonicalize(root)?)?);
            if source_key.starts_with(&root) || root.starts_with(&source_key) {
                return Err(invalid(
                    "导入副本不能位于游戏库或公共工具目录内，也不能包含这些目录",
                ));
            }
        }
    }
    // Reuse deletion boundaries, including every registered game's directory and saves.
    // The incoming copy has no independently configured external saves to delete.
    let mut incoming = game.clone();
    incoming.id = format!("incoming:{token}");
    incoming.install_path = candidate.install_path.clone();
    incoming.display_title = candidate.suggested_title.clone();
    incoming.save_paths.clear();
    let files = deletion::preview(&incoming, games, settings, data, token);
    Ok(DuplicatePlan {
        files,
        existing_id: game.id.clone(),
        existing_title: game.display_title.clone(),
        existing_path: game.install_path.clone(),
        version: game.current_version.clone(),
        candidate: candidate.clone(),
        incoming_version: incoming_version.into(),
        source_identity,
    })
}

pub fn apply(
    plan: &DuplicatePlan,
    game: &Game,
    games: &[Game],
    settings: &Settings,
    data: &Path,
    recycle: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    if game.id != plan.existing_id {
        return Err(invalid("关联游戏已变化，请重新确认"));
    }
    let current = preview(
        &plan.candidate,
        game,
        &plan.incoming_version,
        games,
        settings,
        data,
        &plan.files.token,
    )?;
    if current.files != plan.files
        || current.source_identity != plan.source_identity
        || current.existing_path != plan.existing_path
        || current.version != plan.version
    {
        return Err(invalid("来源目录或关联游戏已变化，请重新打开删除确认框"));
    }
    let report = deletion::apply_files(&plan.files, recycle);
    if let Some(error) = report.error {
        return Err(invalid(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn recycles_only_reviewed_copy_and_preserves_installed_game_and_external_saves() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        let installed = library.join("希尔丝大冒险 The Adventures of HILLS v1.1.4");
        let source = temp.path().join("希尔丝大冒险 v1.1.4");
        let data = temp.path().join("manager");
        fs::create_dir_all(&installed).unwrap();
        fs::create_dir_all(source.join("www/save")).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(source.join("www/save/slot.sav"), b"incoming save").unwrap();
        let external = temp.path().join("external.sav");
        fs::write(&external, b"existing save").unwrap();
        let mut game = crate::maintenance::tests::fixture_game(&installed);
        game.current_version = "v1.1.4".into();
        game.save_paths = vec![external.display().to_string()];
        let settings = Settings {
            game_root: library.display().to_string(),
            ..Settings::default()
        };
        let candidate = crate::scanner::pending_candidate(&source).unwrap();
        let plan = preview(
            &candidate,
            &game,
            "1.1.4",
            std::slice::from_ref(&game),
            &settings,
            &data,
            "token",
        )
        .unwrap();
        let trash = temp.path().join("trash");
        apply(
            &plan,
            &game,
            std::slice::from_ref(&game),
            &settings,
            &data,
            |path| {
                assert_eq!(path, dunce::canonicalize(&source).unwrap());
                fs::rename(path, &trash)?;
                Ok(())
            },
        )
        .unwrap();
        assert!(installed.is_dir());
        assert!(!source.exists());
        assert_eq!(
            fs::read(trash.join("www/save/slot.sav")).unwrap(),
            b"incoming save"
        );
        assert_eq!(fs::read(external).unwrap(), b"existing save");
    }

    #[test]
    fn rejects_unknown_versions_library_paths_shared_saves_and_replaced_source() {
        for (a, b, equal) in [
            ("v1.1.4", "1.1.4", true),
            ("Ver.1.1.4", "v1.1.4", true),
            ("Unknown", "Unknown", false),
            ("-", "-", false),
            ("v1.1.4", "v1.1.5", false),
            ("v1.0", "v1.0.0", false),
        ] {
            assert_eq!(same_version(a, b), equal);
        }
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        let installed = library.join("game");
        let source = temp.path().join("incoming");
        let data = temp.path().join("manager");
        for path in [&installed, &source, &data] {
            fs::create_dir_all(path).unwrap();
        }
        let mut game = crate::maintenance::tests::fixture_game(&installed);
        game.current_version = "v1.1.4".into();
        let settings = Settings {
            game_root: library.display().to_string(),
            ..Settings::default()
        };
        let candidate = crate::scanner::pending_candidate(&source).unwrap();
        assert!(preview(
            &candidate,
            &game,
            "Unknown",
            std::slice::from_ref(&game),
            &settings,
            &data,
            "token"
        )
        .is_err());
        let own = crate::scanner::pending_candidate(&installed).unwrap();
        assert!(preview(
            &own,
            &game,
            "v1.1.4",
            std::slice::from_ref(&game),
            &settings,
            &data,
            "token"
        )
        .is_err());
        game.save_paths.push(source.display().to_string());
        let blocked = preview(
            &candidate,
            &game,
            "v1.1.4",
            std::slice::from_ref(&game),
            &settings,
            &data,
            "token",
        )
        .unwrap();
        assert!(!blocked.files.blockers.is_empty());
        assert!(apply(
            &blocked,
            &game,
            std::slice::from_ref(&game),
            &settings,
            &data,
            |_| panic!("must not recycle")
        )
        .is_err());
        game.save_paths.clear();
        let plan = preview(
            &candidate,
            &game,
            "v1.1.4",
            std::slice::from_ref(&game),
            &settings,
            &data,
            "token",
        )
        .unwrap();
        let mut changed_game = game.clone();
        changed_game.current_version = "v1.1.5".into();
        assert!(apply(
            &plan,
            &changed_game,
            std::slice::from_ref(&changed_game),
            &settings,
            &data,
            |_| panic!("must not recycle")
        )
        .is_err());
        fs::rename(&source, temp.path().join("original")).unwrap();
        fs::create_dir(&source).unwrap();
        assert!(apply(
            &plan,
            &game,
            std::slice::from_ref(&game),
            &settings,
            &data,
            |_| panic!("must not recycle")
        )
        .is_err());
        assert!(source.is_dir());
    }
}
