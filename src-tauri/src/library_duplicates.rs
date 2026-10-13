//! Library-only, metadata-only duplicate recommendations. Never scans or changes game files.
use crate::{
    domain::{Game, Result},
    importer::MatchIndex,
    jobs::Job,
};
use serde::{Deserialize, Serialize};
use std::{fs, time::UNIX_EPOCH};

#[derive(Clone, Debug, Serialize)]
pub struct GameReference {
    pub id: String,
    pub directory_modified_ms: Option<u128>,
}

// Reference information only: one metadata lookup per requested root, no traversal.
pub fn game_references(games: &[Game]) -> Vec<GameReference> {
    games
        .iter()
        .map(|game| GameReference {
            id: game.id.clone(),
            directory_modified_ms: fs::symlink_metadata(&game.install_path)
                .ok()
                .filter(|metadata| metadata.is_dir())
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis()),
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicatePair {
    pub left_id: String,
    pub right_id: String,
    pub score: u8,
    pub confidence: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicateReview {
    pub left_id: String,
    pub right_id: String,
    pub created_at: String,
    pub status: String,
}

pub fn ordered_pair<'a>(left: &'a str, right: &'a str) -> Result<(&'a str, &'a str)> {
    if left == right || left.is_empty() || right.is_empty() {
        return Err(crate::domain::Error::Validation(
            "请选择两个不同的已入库游戏".into(),
        ));
    }
    Ok(if left < right {
        (left, right)
    } else {
        (right, left)
    })
}

pub fn run(job: &Job, games: &[Game]) -> Result<()> {
    job.stage("匹配已入库游戏资料", games.len());
    let pairs =
        MatchIndex::new(games).library_pairs(&|| job.stop(), &|id| job.completed_one(id))?;
    if job.stop() {
        return Err(crate::domain::Error::Validation("重复检查已取消".into()));
    }
    job.publish_duplicates(pairs);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_references_read_only_roots_and_tolerate_missing_or_non_directory_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("game");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("resource.dat"), b"untouched").unwrap();
        let mut present = crate::maintenance::tests::fixture_game(&root);
        present.id = "present".into();
        let mut missing = present.clone();
        missing.id = "missing".into();
        missing.install_path = root.join("not-present").to_string_lossy().into_owned();
        let mut file = present.clone();
        file.id = "file".into();
        file.install_path = root.join("resource.dat").to_string_lossy().into_owned();
        let expected = fs::symlink_metadata(&root)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let references = game_references(&[present, missing, file]);
        assert_eq!(references[0].directory_modified_ms, Some(expected));
        assert_eq!(references[1].directory_modified_ms, None);
        assert_eq!(references[2].directory_modified_ms, None);
        assert_eq!(fs::read(root.join("resource.dat")).unwrap(), b"untouched");
    }
    #[test]
    fn duplicate_tasks_keep_scan_snapshots_and_cancelled_checks_cannot_publish_results() {
        let tasks = crate::jobs::TaskManager::default();
        let scan = tasks.begin("scan", "fixture".into()).unwrap();
        scan.finish(Ok(vec![]));
        let check = tasks.begin("duplicates", String::new()).unwrap();
        check.request_cancel();
        assert!(run(&check, &[]).is_err());
        check.finish(Ok(vec![]));
        assert_eq!(check.page(0).status, "cancelled");
        assert!(check.duplicate_pairs().is_err());
        assert!(tasks.get(&scan.id).is_ok());
        let completed = tasks.begin("duplicates", String::new()).unwrap();
        run(&completed, &[]).unwrap();
        assert!(completed.duplicate_pairs().is_err());
        completed.finish(Ok(vec![]));
        assert!(completed.duplicate_pairs().unwrap().is_empty());
        assert!(scan.duplicate_pairs().is_err());
    }
}
