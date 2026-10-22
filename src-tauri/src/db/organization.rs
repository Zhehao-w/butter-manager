use super::Database;
use crate::domain::{Error, Game, LibraryTag, PlayStatus, Result};
use crate::paths::normalize_alias;
use rusqlite::{params, OptionalExtension, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn tag_name(value: &str) -> Result<(&str, String)> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(Error::Validation(
            "标签须为 1–40 个字符，不能包含换行或控制字符".into(),
        ));
    }
    Ok((name, normalize_alias(name)))
}

impl Database {
    pub fn library_tags(&self) -> Result<Vec<LibraryTag>> {
        Ok(self
            .connection
            .prepare("SELECT id,name FROM library_tags ORDER BY name_key,id")?
            .query_map([], |row| {
                Ok(LibraryTag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub(super) fn load_game_tags(&self, games: &mut [Game]) -> Result<()> {
        let index: HashMap<_, _> = games
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id.clone(), i))
            .collect();
        // Bounded queries also keep games_by_ids from loading the entire library's relations.
        let ids: Vec<_> = index.keys().collect();
        for chunk in ids.chunks(900) {
            let sql = format!("SELECT g.game_id,t.id,t.name FROM game_tags g JOIN library_tags t ON t.id=g.tag_id WHERE g.game_id IN ({}) ORDER BY t.name_key,t.id", vec!["?"; chunk.len()].join(","));
            let mut statement = self.connection.prepare(&sql)?;
            let rows = statement.query_map(rusqlite::params_from_iter(chunk), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    LibraryTag {
                        id: row.get(1)?,
                        name: row.get(2)?,
                    },
                ))
            })?;
            for row in rows {
                let (id, tag) = row?;
                games[index[&id]].tags.push(tag);
            }
        }
        Ok(())
    }

    pub fn set_favorite(&mut self, id: &str, favorite: bool) -> Result<Game> {
        if self.connection.execute(
            "UPDATE games SET favorite=?2 WHERE id=?1",
            params![id, favorite],
        )? != 1
        {
            return Err(Error::Validation("游戏不存在".into()));
        }
        self.game(id)
    }

    pub fn set_play_statuses(&mut self, ids: &[String], status: PlayStatus) -> Result<Vec<Game>> {
        self.set_library_field(
            ids,
            "UPDATE games SET play_status=?2 WHERE id=?1",
            status.as_str(),
        )
    }

    pub fn set_favorites(&mut self, ids: &[String], favorite: bool) -> Result<Vec<Game>> {
        self.set_library_field(ids, "UPDATE games SET favorite=?2 WHERE id=?1", favorite)
    }

    fn set_library_field(
        &mut self,
        ids: &[String],
        sql: &str,
        value: impl rusqlite::ToSql,
    ) -> Result<Vec<Game>> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Validation("请选择 1–10000 个游戏".into()));
        }
        let mut seen = HashSet::new();
        let unique: Vec<_> = ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect();
        let tx = self.connection.transaction()?;
        for id in &unique {
            if tx.execute(sql, params![id, value])? != 1 {
                return Err(Error::Validation("所选游戏已变化，请刷新列表后重试".into()));
            }
        }
        tx.commit()?;
        self.games_by_ids(&unique)
    }

    pub fn rename_library_tag(&mut self, id: &str, name: &str) -> Result<()> {
        let (name, key) = tag_name(name)?;
        let conflict = self
            .connection
            .query_row(
                "SELECT id FROM library_tags WHERE name_key=?1 AND id<>?2",
                params![key, id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        if conflict.is_some() {
            return Err(Error::Validation("已有同名标签，请使用其他名称".into()));
        }
        if self.connection.execute(
            "UPDATE library_tags SET name=?2,name_key=?3 WHERE id=?1",
            params![id, name, key],
        )? != 1
        {
            return Err(Error::Validation("标签已不存在，请刷新后重试".into()));
        }
        Ok(())
    }

    pub fn delete_library_tag(&mut self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM library_tags WHERE id=?1", [id])?;
        Ok(())
    }

    pub(super) fn replace_game_tags(
        tx: &Transaction<'_>,
        game_id: &str,
        tags: &[LibraryTag],
    ) -> Result<()> {
        if tags.len() > 32 {
            return Err(Error::Validation("每个游戏最多关联 32 个标签".into()));
        }
        tx.execute("DELETE FROM game_tags WHERE game_id=?1", [game_id])?;
        for tag in tags {
            let id = if tag.id.is_empty() {
                let (name, key) = tag_name(&tag.name)?;
                let id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT OR IGNORE INTO library_tags(id,name,name_key) VALUES(?1,?2,?3)",
                    params![id, name, key],
                )?;
                tx.query_row(
                    "SELECT id FROM library_tags WHERE name_key=?1",
                    [key],
                    |r| r.get::<_, String>(0),
                )?
            } else {
                // Stable IDs preserve renames and prevent stale drafts from recreating deleted tags.
                tx.query_row("SELECT id FROM library_tags WHERE id=?1", [&tag.id], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?
                .ok_or_else(|| Error::Validation("标签已被删除，请重新打开资料再编辑".into()))?
            };
            tx.execute(
                "INSERT OR IGNORE INTO game_tags(game_id,tag_id) VALUES(?1,?2)",
                params![game_id, id],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::GameEdit;
    use std::path::Path;

    fn insert(db: &Database, id: &str) {
        db.connection.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key) VALUES(?1,?1,?1,?1,?1)", [id]).unwrap();
    }
    fn edit(db: &Database, id: &str) -> GameEdit {
        serde_json::from_value(serde_json::to_value(db.game(id).unwrap()).unwrap()).unwrap()
    }
    fn assign(db: &mut Database, id: &str, names: &[&str]) -> Game {
        let mut draft = edit(db, id);
        draft.tags = Some(
            names
                .iter()
                .map(|name| LibraryTag {
                    id: String::new(),
                    name: (*name).into(),
                })
                .collect(),
        );
        db.edit_game(draft).unwrap()
    }
    fn downgrade_to_12(db: &Database) {
        db.connection.execute_batch("ALTER TABLE games DROP COLUMN mtool_strategy;ALTER TABLE games DROP COLUMN launch_arguments;DROP TABLE game_tags;DROP TABLE library_tags;ALTER TABLE games DROP COLUMN favorite;PRAGMA user_version=12;").unwrap();
    }
    fn schema(path: &Path) -> i64 {
        rusqlite::Connection::open(path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn upgrade_12_preserves_records_and_failed_migration_rolls_back() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("isolated.db");
        let mut db = Database::open(&path).unwrap();
        insert(&db, "a");
        db.connection
            .execute(
                "INSERT INTO aliases VALUES('alias','a','人工别名','人工别名','manual')",
                [],
            )
            .unwrap();
        db.connection
            .execute(
                "INSERT INTO save_paths VALUES('save','a','<GAME>/save')",
                [],
            )
            .unwrap();
        db.record_launch("a").unwrap();
        let before = serde_json::to_value(db.games().unwrap()).unwrap();
        let history = db.launch_history("a").unwrap();
        let settings = serde_json::to_value(db.settings().unwrap()).unwrap();
        downgrade_to_12(&db);
        // An incompatible preexisting table forces failure after ALTER TABLE.
        db.connection
            .execute_batch("CREATE TABLE library_tags(unexpected TEXT);")
            .unwrap();
        drop(db);
        assert!(Database::open(&path).is_err());
        assert_eq!(schema(&path), 12);
        let connection = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM pragma_table_info('games') WHERE name='favorite'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        connection
            .execute_batch("DROP TABLE library_tags;")
            .unwrap();
        drop(connection);
        let db = Database::open(&path).unwrap();
        assert_eq!(schema(&path), 15);
        assert_eq!(serde_json::to_value(db.games().unwrap()).unwrap(), before);
        assert_eq!(db.launch_history("a").unwrap(), history);
        assert_eq!(
            serde_json::to_value(db.settings().unwrap()).unwrap(),
            settings
        );
        assert!(db.library_tags().unwrap().is_empty());
        assert!(!db
            .connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
    }

    #[test]
    fn tags_deduplicate_rename_and_delete_without_stale_draft_resurrection() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("isolated.db");
        let mut db = Database::open(&path).unwrap();
        insert(&db, "a");
        insert(&db, "b");
        let a = assign(&mut db, "a", &[" 汉化 ", "ＪＳＯＮ", "json", "等更新"]);
        let b = assign(&mut db, "b", &["JSON", "汉化"]);
        assert_eq!(a.tags.len(), 3);
        assert_eq!(db.library_tags().unwrap().len(), 3);
        let json = b
            .tags
            .iter()
            .find(|t| normalize_alias(&t.name) == "json")
            .unwrap();
        let mut stale = edit(&db, "a");
        db.rename_library_tag(&json.id, "剧情向").unwrap();
        assert!(db.rename_library_tag(&json.id, "汉化").is_err());
        assert!(db.rename_library_tag(&json.id, "\n").is_err());
        db.edit_game(stale.clone()).unwrap();
        assert!(db
            .game("a")
            .unwrap()
            .tags
            .iter()
            .any(|t| t.id == json.id && t.name == "剧情向"));
        db.delete_library_tag(&json.id).unwrap();
        stale.display_title = "should rollback".into();
        assert!(db.edit_game(stale).is_err());
        assert_eq!(db.game("a").unwrap().display_title, "a");
        assert!(db
            .games()
            .unwrap()
            .iter()
            .all(|g| g.tags.iter().all(|t| t.id != json.id)));
        let before = serde_json::to_value(db.game("a").unwrap()).unwrap();
        for name in [String::new(), "x".repeat(41), "bad\nname".into()] {
            let mut invalid = edit(&db, "a");
            invalid.tags = Some(vec![LibraryTag {
                id: String::new(),
                name,
            }]);
            assert!(db.edit_game(invalid).is_err());
            assert_eq!(serde_json::to_value(db.game("a").unwrap()).unwrap(), before);
        }
        drop(db);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.game("a").unwrap().tags.len(), 2);
        db.remove_game("a").unwrap();
        assert_eq!(db.game("b").unwrap().tags.len(), 1);
        db.clear_library().unwrap();
        assert!(db.library_tags().unwrap().is_empty());
        assert!(db.games().unwrap().is_empty());
        assert_eq!(schema(&path), 15);
        assert!(!db
            .connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
    }

    #[test]
    fn favorites_tags_and_manual_status_survive_edits_version_changes_and_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("isolated.db");
        let mut db = Database::open(&path).unwrap();
        insert(&db, "a");
        let mut stale = edit(&db, "a");
        stale.tags = None; // Older callers must leave assignments alone.
        db.set_favorite("a", true).unwrap();
        let tags = assign(&mut db, "a", &["汉化", "等更新"]).tags;
        db.edit_game(stale).unwrap();
        db.set_play_statuses(&["a".into()], PlayStatus::Completed)
            .unwrap();
        let old = db.version_config("a").unwrap();
        let mut new = old.clone();
        new.version = "v2".into();
        db.commit_version("a", "operation", &old, &new).unwrap();
        db.commit_version("a", "operation", &old, &new).unwrap();
        db.rollback_version("a", "operation", &new, &old).unwrap();
        let launched = db.record_launch("a").unwrap();
        assert!(launched.favorite);
        assert_eq!(launched.tags, tags);
        assert_eq!(launched.play_status, PlayStatus::Completed);
        drop(db);
        let db = Database::open(&path).unwrap();
        assert!(db.game("a").unwrap().favorite);
        assert_eq!(db.game("a").unwrap().tags, tags);
    }

    #[test]
    fn bulk_status_is_atomic_deduplicated_and_does_not_change_history_or_timestamps() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = Database::open(&temp.path().join("isolated.db")).unwrap();
        insert(&db, "a");
        insert(&db, "b");
        db.record_launch("a").unwrap();
        db.set_favorite("a", true).unwrap();
        assign(&mut db, "a", &["汉化"]);
        let before = serde_json::to_value(db.games().unwrap()).unwrap();
        let history = db.launch_history("a").unwrap();
        assert!(db
            .set_play_statuses(&["a".into(), "missing".into()], PlayStatus::Completed)
            .is_err());
        assert!(db.set_play_statuses(&[], PlayStatus::Completed).is_err());
        db.connection.execute_batch("CREATE TEMP TRIGGER fail_batch BEFORE UPDATE OF play_status ON games WHEN OLD.id='b' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        assert!(db
            .set_play_statuses(&["a".into(), "b".into()], PlayStatus::Completed)
            .is_err());
        assert_eq!(serde_json::to_value(db.games().unwrap()).unwrap(), before);
        db.connection
            .execute_batch("DROP TRIGGER fail_batch;")
            .unwrap();
        let result = db
            .set_play_statuses(&["a".into(), "a".into(), "b".into()], PlayStatus::Completed)
            .unwrap();
        assert_eq!(result.len(), 2);
        let after = serde_json::to_value(db.games().unwrap()).unwrap();
        for (before, after) in before
            .as_array()
            .unwrap()
            .iter()
            .zip(after.as_array().unwrap())
        {
            let mut expected = before.clone();
            expected["play_status"] = "COMPLETED".into();
            assert_eq!(*after, expected);
        }
        assert_eq!(db.launch_history("a").unwrap(), history);
    }

    #[test]
    fn bulk_favorites_are_atomic_preserve_other_fields_and_support_both_values() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = Database::open(&temp.path().join("isolated.db")).unwrap();
        insert(&db, "a");
        insert(&db, "b");
        db.record_launch("a").unwrap();
        db.set_favorite("a", true).unwrap();
        assign(&mut db, "a", &["汉化"]);
        let before = serde_json::to_value(db.games().unwrap()).unwrap();
        let history = db.launch_history("a").unwrap();
        assert!(db.set_favorites(&[], true).is_err());
        assert!(db.set_favorites(&vec!["a".into(); 10001], true).is_err());
        assert!(db
            .set_favorites(&["a".into(), "missing".into()], false)
            .is_err());
        assert_eq!(serde_json::to_value(db.games().unwrap()).unwrap(), before);
        db.connection.execute_batch("CREATE TEMP TRIGGER fail_favorite BEFORE UPDATE OF favorite ON games WHEN OLD.id='b' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        assert!(db.set_favorites(&["a".into(), "b".into()], false).is_err());
        assert_eq!(serde_json::to_value(db.games().unwrap()).unwrap(), before);
        db.connection
            .execute_batch("DROP TRIGGER fail_favorite;")
            .unwrap();
        for favorite in [true, true, false] {
            let result = db
                .set_favorites(&["a".into(), "a".into(), "b".into()], favorite)
                .unwrap();
            assert_eq!(result.len(), 2);
            let after = serde_json::to_value(db.games().unwrap()).unwrap();
            for (before, after) in before
                .as_array()
                .unwrap()
                .iter()
                .zip(after.as_array().unwrap())
            {
                let mut expected = before.clone();
                expected["favorite"] = favorite.into();
                assert_eq!(*after, expected);
            }
            assert_eq!(db.launch_history("a").unwrap(), history);
        }
    }

    #[test]
    fn large_bulk_selection_loads_all_tags_and_deduplicates_across_query_chunks() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = Database::open(&temp.path().join("isolated.db")).unwrap();
        let mut ids: Vec<String> = (0..1001).map(|i| format!("game-{i}")).collect();
        {
            let setup = db.connection.transaction().unwrap();
            for id in &ids {
                setup.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key) VALUES(?1,?1,?1,?1,?1)", [id]).unwrap();
            }
            setup.commit().unwrap();
        }
        let tag = assign(&mut db, &ids[0], &["汉化"]).tags.remove(0);
        db.connection
            .execute(
                "INSERT OR IGNORE INTO game_tags(game_id,tag_id) SELECT id,?1 FROM games",
                [&tag.id],
            )
            .unwrap();
        ids.push(ids[0].clone());
        let result = db.set_play_statuses(&ids, PlayStatus::Completed).unwrap();
        assert_eq!(result.len(), 1001);
        assert!(result
            .iter()
            .all(|game| game.tags == [tag.clone()] && game.play_status == PlayStatus::Completed));
        let favorites = db.set_favorites(&ids, true).unwrap();
        assert_eq!(favorites.len(), 1001);
        assert!(favorites.iter().all(|game| game.favorite
            && game.tags == [tag.clone()]
            && game.play_status == PlayStatus::Completed));
    }
}
