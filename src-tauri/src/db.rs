use crate::domain::{
    Error, Game, GameEdit, RegistrationEntry, RelocateGame, ResetReport, Result, ScanCandidate,
    Settings,
};
use crate::paths::{contained_file, normalize_alias, path_key, path_text, relative_path};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub struct Database {
    connection: Connection,
}

pub(crate) fn config_from_game(game: &Game) -> crate::importer::VersionConfig {
    crate::importer::VersionConfig {
        version: game.current_version.clone(),
        version_source: game.version_source.clone(),
        engine: game.engine.clone(),
        engine_source: "detected".into(),
        launch_source: "detected".into(),
        executable: game.main_executable.clone(),
        working_directory: game.working_directory.clone(),
        launch_type: game.launch_type.clone(),
        external_player: game.external_player.clone(),
        mtool_target: game.mtool_target_exe.clone(),
        mtool_loader: game.mtool_loader.clone(),
    }
}

impl Database {
    #[cfg(test)]
    pub(crate) fn fail_version_commit(&self, fail: bool) -> Result<()> {
        if fail {
            self.connection.execute_batch("CREATE TEMP TRIGGER fail_version_commit BEFORE INSERT ON version_history BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")?;
        } else {
            self.connection
                .execute_batch("DROP TRIGGER fail_version_commit;")?;
        }
        Ok(())
    }
    pub fn version_config(&self, id: &str) -> Result<crate::importer::VersionConfig> {
        let mut config = config_from_game(&self.game(id)?);
        let (engine, launch): (String, String) = self.connection.query_row(
            "SELECT engine_source,launch_source FROM games WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        config.engine_source = engine;
        config.launch_source = launch;
        Ok(config)
    }
    pub fn version_operation(&self, operation: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT status FROM version_history WHERE operation=?1",
                [operation],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn version_history(&self, id: &str) -> Result<Vec<crate::importer::VersionHistory>> {
        self.game(id)?;
        Ok(self.connection.prepare("SELECT operation,old_version,new_version,status,created_at FROM version_history WHERE game_id=?1 ORDER BY sequence DESC")?
            .query_map([id], |r| Ok(crate::importer::VersionHistory { operation:r.get(0)?,old_version:r.get(1)?,new_version:r.get(2)?,status:r.get(3)?,created_at:r.get(4)? }))?
            .collect::<std::result::Result<Vec<_>,_>>()?)
    }
    pub fn ensure_latest_version(&self, id: &str, operation: &str) -> Result<()> {
        let latest: Option<String> = self.connection.query_row(
            "SELECT operation FROM version_history WHERE game_id=?1 AND status='committed' ORDER BY sequence DESC LIMIT 1", [id], |r| r.get(0)).optional()?;
        if latest.as_deref() != Some(operation) {
            return Err(Error::Validation("只能回退最近一次尚未回退的更新".into()));
        }
        Ok(())
    }
    fn write_version(
        tx: &rusqlite::Transaction<'_>,
        id: &str,
        config: &crate::importer::VersionConfig,
    ) -> Result<()> {
        tx.execute("UPDATE games SET current_version=?2,version_source=?3,engine=?4,engine_source=?5,launch_source=?6,main_executable=?7,working_directory=?8,launch_type=?9,external_player=?10,mtool_target_exe=?11,mtool_loader=?12,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![id,config.version,config.version_source,config.engine,config.engine_source,config.launch_source,config.executable,config.working_directory,config.launch_type,
                config.external_player.as_ref().map(serde_json::to_string).transpose().map_err(|e| Error::Validation(e.to_string()))?,config.mtool_target,config.mtool_loader])?;
        Ok(())
    }
    pub fn commit_version(
        &mut self,
        id: &str,
        operation: &str,
        old: &crate::importer::VersionConfig,
        new: &crate::importer::VersionConfig,
    ) -> Result<()> {
        if self.version_operation(operation)?.as_deref() == Some("committed") {
            if self.version_config(id)? != *new {
                return Err(Error::Validation("提交记录与当前版本不一致".into()));
            }
            return Ok(());
        }
        if self.version_config(id)? != *old {
            return Err(Error::Validation("已有版本配置变化，未提交更新".into()));
        }
        let tx = self.connection.transaction()?;
        Self::write_version(&tx, id, new)?;
        tx.execute("INSERT INTO version_history(operation,game_id,old_version,new_version,status) VALUES(?1,?2,?3,?4,'committed')", params![operation,id,old.version,new.version])?;
        tx.commit()?;
        Ok(())
    }
    pub fn rollback_version(
        &mut self,
        id: &str,
        operation: &str,
        new: &crate::importer::VersionConfig,
        old: &crate::importer::VersionConfig,
    ) -> Result<()> {
        if self.version_operation(operation)?.as_deref() == Some("rolled_back") {
            return Ok(());
        }
        self.ensure_latest_version(id, operation)?;
        if self.version_config(id)? != *new {
            return Err(Error::Validation(
                "当前启动配置变化，不能按旧计划回退".into(),
            ));
        }
        let tx = self.connection.transaction()?;
        Self::write_version(&tx, id, old)?;
        tx.execute(
            "UPDATE version_history SET status='rolled_back' WHERE operation=?1 AND game_id=?2",
            params![operation, id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 9 {
            return Err(Error::Validation("数据库版本较新，当前程序无法打开".into()));
        }
        if version == 0 {
            connection.execute_batch("BEGIN IMMEDIATE;")?;
            if let Err(error) =
                connection.execute_batch(include_str!("../migrations/001_initial.sql"))
            {
                connection.execute_batch("ROLLBACK;")?;
                return Err(error.into());
            }
            connection.execute_batch("COMMIT;")?;
        }
        if version < 2 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/002_working_directory.sql"))?;
            transaction.commit()?;
        }
        if version < 3 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/003_launch_history.sql"))?;
            transaction.commit()?;
        }
        if version < 4 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/004_scan_workers.sql"))?;
            transaction.commit()?;
        }
        if version < 5 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/005_engine_source.sql"))?;
            transaction.commit()?;
        }
        if version < 6 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/006_launch_source.sql"))?;
            transaction.commit()?;
        }
        if version < 7 {
            connection.execute_batch("PRAGMA foreign_keys=OFF;")?;
            let migration = (|| -> Result<()> {
                let transaction = connection.unchecked_transaction()?;
                transaction.execute_batch(include_str!("../migrations/007_external_player.sql"))?;
                let violated = transaction
                    .prepare("PRAGMA foreign_key_check")?
                    .exists([])?;
                if violated {
                    return Err(Error::Validation("迁移发现关联记录损坏，已回滚".into()));
                }
                transaction.commit()?;
                Ok(())
            })();
            connection.execute_batch("PRAGMA foreign_keys=ON;")?;
            migration?;
        }
        if version < 8 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/008_play_status.sql"))?;
            transaction.commit()?;
        }
        if version < 9 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(include_str!("../migrations/009_version_history.sql"))?;
            transaction.commit()?;
        }
        Ok(Self { connection })
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self.connection.query_row(
            "SELECT game_root,mtool_root,mtool_injector,mtool_runtime,scan_workers FROM settings WHERE id=1",
            [],
            |row| {
                Ok(Settings {
                    game_root: row.get(0)?,
                    mtool_root: row.get(1)?,
                    mtool_injector: row.get(2)?,
                    mtool_runtime: row.get(3)?,
                    scan_workers: row.get(4)?,
                })
            },
        )?)
    }

    pub fn save_settings(&self, mut settings: Settings) -> Result<Settings> {
        if ![1, 2, 4].contains(&settings.scan_workers) {
            return Err(Error::Validation("请选择 1、2 或 4 个扫描线程".into()));
        }
        for root in [&mut settings.game_root, &mut settings.mtool_root] {
            if !root.trim().is_empty() {
                let path = dunce::canonicalize(&*root)?;
                if !path.is_dir() {
                    return Err(Error::Validation("Root 必须是已存在目录".into()));
                }
                *root = path_text(&path)?;
            } else {
                root.clear();
            }
        }
        for path in [&settings.mtool_injector, &settings.mtool_runtime] {
            let path = relative_path(path)?;
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                return Err(Error::Validation(
                    "MTool injector / runtime 必须为相对 EXE 路径".into(),
                ));
            }
        }
        self.connection.execute("UPDATE settings SET game_root=?1,mtool_root=?2,mtool_injector=?3,mtool_runtime=?4,scan_workers=?5 WHERE id=1",
            params![settings.game_root, settings.mtool_root, settings.mtool_injector, settings.mtool_runtime,settings.scan_workers])?;
        Ok(settings)
    }

    pub fn registered_id(&self, path: &Path) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id FROM games WHERE install_path_key=?1",
                [path_key(path)?],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn registered_paths(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut statement = self
            .connection
            .prepare("SELECT install_path_key,id FROM games")?;
        let pairs = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(pairs)
    }

    pub fn register(&mut self, candidates: &[ScanCandidate]) -> Result<Vec<String>> {
        let entries = candidates
            .iter()
            .map(|c| RegistrationEntry {
                candidate: c.clone(),
                executable: if let Some(qsp) = &c.qsp {
                    qsp.recommended_player.clone()
                } else {
                    c.executables.first().map(|v| v.relative_path.clone())
                },
                external_player: crate::external_player::qsp_config(c),
                version: c.suggested_version.clone(),
                version_source: c.version_source.clone(),
                working_directory: c.working_directory.clone(),
            })
            .collect::<Vec<_>>();
        self.register_entries(&entries, &|| false, &|_, _| {})
    }

    pub fn register_entries(
        &mut self,
        entries: &[RegistrationEntry],
        cancelled: &dyn Fn() -> bool,
        progress: &dyn Fn(usize, &str),
    ) -> Result<Vec<String>> {
        self.register_entries_with_ids(entries, cancelled, progress, None)
    }

    pub fn register_import(&mut self, entry: RegistrationEntry, id: &str) -> Result<String> {
        let ids = [id.to_owned()];
        Ok(self
            .register_entries_with_ids(&[entry], &|| false, &|_, _| {}, Some(&ids))?
            .remove(0))
    }

    fn register_entries_with_ids(
        &mut self,
        entries: &[RegistrationEntry],
        cancelled: &dyn Fn() -> bool,
        progress: &dyn Fn(usize, &str),
        planned: Option<&[String]>,
    ) -> Result<Vec<String>> {
        let transaction = self.connection.transaction()?;
        let mut ids = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            if cancelled() {
                return Err(Error::Validation("登记已取消，尚未提交的记录已回滚".into()));
            }
            let candidate = &entry.candidate;
            let key = path_key(Path::new(&candidate.install_path))?;
            let existing = transaction
                .prepare_cached("SELECT id FROM games WHERE install_path_key=?1")?
                .query_row([&key], |r| r.get::<_, String>(0))
                .optional()?;
            if let Some(id) = existing {
                if planned.is_some_and(|ids| ids[index] != id) {
                    return Err(Error::Validation(
                        "目标被其他游戏记录占用，导入已停止".into(),
                    ));
                }
                ids.push(id);
                progress(index + 1, &candidate.install_path);
                continue;
            }
            let id = planned.map_or_else(|| Uuid::new_v4().to_string(), |ids| ids[index].clone());
            if let Some(config) = &entry.external_player {
                crate::external_player::validate(
                    Path::new(&candidate.install_path),
                    entry.executable.as_deref(),
                    config,
                )?;
            }
            let mtool = entry.external_player.is_none()
                && candidate.mtool_detected
                && entry.executable.as_ref().is_some_and(|file| {
                    Path::new(file)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                });
            let mode = if entry.external_player.is_some() {
                "EXTERNAL_PLAYER"
            } else if mtool {
                "MTOOL"
            } else {
                "DIRECT"
            };
            let work = if mtool { "." } else { &entry.working_directory };
            let source = if planned.is_some() {
                "manual"
            } else {
                "detected"
            };
            transaction.prepare_cached("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,main_executable,engine,current_version,version_source,working_directory,launch_type,mtool_target_exe,launch_source,engine_source) VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12)")?.execute(params![id,candidate.suggested_title,candidate.install_path,key,entry.executable,candidate.engine,entry.version,entry.version_source,work,mode,if mtool { entry.executable.as_deref() } else { None },source])?;
            transaction.prepare_cached("INSERT INTO aliases(id,game_id,alias,normalized_alias,source) VALUES(?1,?2,?3,?4,'folder_name')")?.execute(params![Uuid::new_v4().to_string(),id,candidate.suggested_title,normalize_alias(&candidate.suggested_title)])?;
            transaction.execute(
                "UPDATE games SET external_player=?2 WHERE id=?1",
                params![
                    id,
                    entry
                        .external_player
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()
                        .map_err(|e| Error::Validation(e.to_string()))?
                ],
            )?;
            for path in crate::save_detection::detect_for_engine(
                Path::new(&candidate.install_path),
                &entry.working_directory,
                &candidate.engine,
                entry.executable.as_deref(),
            ) {
                transaction.execute(
                    "INSERT OR IGNORE INTO save_paths(id,game_id,path) VALUES(?1,?2,?3)",
                    params![Uuid::new_v4().to_string(), id, path],
                )?;
            }
            ids.push(id);
            progress(index + 1, &candidate.install_path);
        }
        if cancelled() {
            return Err(Error::Validation("登记已取消，尚未提交的记录已回滚".into()));
        }
        transaction.commit()?;
        Ok(ids)
    }

    pub fn games(&self) -> Result<Vec<Game>> {
        self.load_games(None)
    }

    pub fn sync_mtool_defaults(&mut self, candidates: &[ScanCandidate]) -> Result<Vec<Game>> {
        let mut eligible = vec![];
        let mut qsp_ids = vec![];
        let mut detected_saves = vec![];
        for candidate in candidates
            .iter()
            .filter(|c| c.status == "ready" && !c.save_paths.is_empty())
        {
            let Some(id) = &candidate.registered_id else {
                continue;
            };
            let game = self.game(id)?;
            if game.install_path != candidate.install_path || !game.save_paths.is_empty() {
                continue;
            }
            let paths = crate::save_detection::detect_for_engine(
                Path::new(&game.install_path),
                &candidate.working_directory,
                &candidate.engine,
                game.main_executable.as_deref(),
            );
            if !paths.is_empty() {
                detected_saves.push((id.clone(), paths));
            }
        }
        for candidate in candidates
            .iter()
            .filter(|c| c.status == "ready" && c.qsp.is_some())
        {
            let Some(id) = &candidate.registered_id else {
                continue;
            };
            let game = self.game(id)?;
            if game.install_path == candidate.install_path
                && candidate
                    .qsp
                    .as_ref()
                    .unwrap()
                    .game_files
                    .iter()
                    .any(|file| contained_file(Path::new(&game.install_path), file, "qsp").is_ok())
            {
                qsp_ids.push(id.clone());
            }
        }
        for candidate in candidates
            .iter()
            .filter(|c| c.mtool_detected && c.qsp.is_none() && c.status == "ready")
        {
            let Some(id) = &candidate.registered_id else {
                continue;
            };
            let game = self.game(id)?;
            let source: String = self.connection.query_row(
                "SELECT launch_source FROM games WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            if source == "manual"
                || game.launch_type != "DIRECT"
                || game.install_path != candidate.install_path
            {
                continue;
            }
            let Some(exe) = game.main_executable.as_deref() else {
                continue;
            };
            if contained_file(Path::new(&game.install_path), exe, "exe").is_err() {
                continue;
            }
            eligible.push(game);
        }
        let transaction = self.connection.transaction()?;
        let mut changed_ids = vec![];
        for id in qsp_ids {
            // A complete scan can refresh detected engines, but never a manual engine or launch.
            if transaction.execute("UPDATE games SET engine='QSP',engine_source='detected',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND engine!='QSP' AND engine_source!='manual'", [&id])? > 0 {
                changed_ids.push(id);
            }
        }
        for game in &eligible {
            transaction.execute("UPDATE games SET launch_type='MTOOL',mtool_target_exe=main_executable,mtool_loader=NULL,working_directory='.',launch_source='detected',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1", [&game.id])?;
            changed_ids.push(game.id.clone());
        }
        for (id, paths) in detected_saves {
            for path in paths {
                transaction.execute(
                    "INSERT OR IGNORE INTO save_paths(id,game_id,path) VALUES(?1,?2,?3)",
                    params![Uuid::new_v4().to_string(), id, path],
                )?;
            }
            transaction.execute(
                "UPDATE games SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                [&id],
            )?;
            if !changed_ids.contains(&id) {
                changed_ids.push(id);
            }
        }
        transaction.commit()?;
        self.games_by_ids(&changed_ids)
    }

    pub fn remove_game(&mut self, id: &str) -> Result<Game> {
        let game = self.game(id)?;
        let transaction = self.connection.transaction()?;
        for table in ["aliases", "save_paths", "launch_history"] {
            transaction.execute(&format!("DELETE FROM {table} WHERE game_id=?1"), [id])?;
        }
        transaction.execute("DELETE FROM games WHERE id=?1", [id])?;
        transaction.commit()?;
        Ok(game)
    }

    pub fn relocate_game(&mut self, change: RelocateGame) -> Result<Game> {
        let game = self.game(&change.id)?;
        if game.install_path != change.expected_install_path {
            return Err(Error::Validation(
                "游戏目录已改变，请重新打开关联预览".into(),
            ));
        }
        let root = dunce::canonicalize(&change.install_path)?;
        if !root.is_dir() {
            return Err(Error::Validation("请选择已有游戏目录".into()));
        }
        if self.registered_id(&root)?.is_some_and(|id| id != game.id) {
            return Err(Error::Validation(
                "新目录已经属于另一个库记录，请先检查重复记录".into(),
            ));
        }
        if !["DIRECT", "MTOOL", "EXTERNAL_PLAYER"].contains(&change.launch_type.as_str()) {
            return Err(Error::Validation("请选择有效启动方式".into()));
        }
        if change.launch_type == "EXTERNAL_PLAYER" {
            let config = change
                .external_player
                .as_ref()
                .ok_or_else(|| Error::Validation("请配置外部播放器类型".into()))?;
            crate::external_player::validate(&root, change.main_executable.as_deref(), config)?;
        }
        if let Some(file) = &change.main_executable {
            crate::paths::launch_file(&root, file)?;
        }
        if change.launch_type == "MTOOL" {
            let target = change
                .mtool_target_exe
                .as_deref()
                .ok_or_else(|| Error::Validation("请为新目录选择 MTool target EXE".into()))?;
            contained_file(&root, target, "exe")?;
        }
        crate::paths::working_directory(&root, &change.working_directory)?;
        let transaction = self.connection.transaction()?;
        transaction.execute("UPDATE games SET install_path=?2,install_path_key=?3,main_executable=?4,working_directory=?5,launch_type=?6,mtool_target_exe=?7,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![game.id, path_text(&root)?, path_key(&root)?, change.main_executable, change.working_directory, change.launch_type, change.mtool_target_exe])?;
        transaction.execute(
            "UPDATE games SET external_player=?2 WHERE id=?1",
            params![
                game.id,
                if change.launch_type == "EXTERNAL_PLAYER" {
                    change
                        .external_player
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()
                        .map_err(|e| Error::Validation(e.to_string()))?
                } else {
                    None
                }
            ],
        )?;
        for path in &game.save_paths {
            let rebased =
                crate::maintenance::rebase_save(path, Path::new(&game.install_path), &root);
            if rebased != *path {
                transaction.execute(
                    "UPDATE save_paths SET path=?3 WHERE game_id=?1 AND path=?2",
                    params![game.id, path, rebased],
                )?;
            }
        }
        transaction.commit()?;
        self.game(&game.id)
    }
    pub fn games_by_ids(&self, ids: &[String]) -> Result<Vec<Game>> {
        let mut result = Vec::new();
        for chunk in ids.chunks(900) {
            result.extend(self.load_games(Some(chunk))?);
        }
        Ok(result)
    }
    fn load_games(&self, ids: Option<&[String]>) -> Result<Vec<Game>> {
        let suffix = ids
            .map(|ids| format!(" WHERE id IN ({})", vec!["?"; ids.len()].join(",")))
            .unwrap_or_default();
        let query = format!("SELECT id,canonical_title,display_title,install_path,current_version,version_source,main_executable,engine,launch_type,mtool_target_exe,mtool_loader,created_at,updated_at,working_directory,last_launched_at,external_player,play_status FROM games{suffix} ORDER BY display_title COLLATE NOCASE,id");
        let mut statement = self.connection.prepare(&query)?;
        let mut games = statement
            .query_map(rusqlite::params_from_iter(ids.unwrap_or_default()), |row| {
                Ok(Game {
                    id: row.get(0)?,
                    canonical_title: row.get(1)?,
                    display_title: row.get(2)?,
                    install_path: row.get(3)?,
                    current_version: row.get(4)?,
                    version_source: row.get(5)?,
                    main_executable: row.get(6)?,
                    engine: row.get(7)?,
                    launch_type: row.get(8)?,
                    mtool_target_exe: row.get(9)?,
                    mtool_loader: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                    working_directory: row.get(13)?,
                    last_launched_at: row.get(14)?,
                    external_player: row
                        .get::<_, Option<String>>(15)?
                        .map(|value| {
                            serde_json::from_str(&value).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    15,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })
                        })
                        .transpose()?,
                    play_status: crate::domain::PlayStatus::parse(&row.get::<_, String>(16)?)
                        .map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                16,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                    aliases: vec![],
                    save_paths: vec![],
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let index = games
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id.clone(), i))
            .collect::<std::collections::HashMap<_, _>>();
        let relation_suffix = suffix.replace("WHERE id", "WHERE game_id");
        for (table, column) in [("aliases", "alias"), ("save_paths", "path")] {
            let mut statement = self.connection.prepare(&format!(
                "SELECT game_id,{column} FROM {table}{relation_suffix} ORDER BY rowid"
            ))?;
            let pairs = statement
                .query_map(rusqlite::params_from_iter(ids.unwrap_or_default()), |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
            for pair in pairs {
                let (id, value) = pair?;
                if let Some(&i) = index.get(&id) {
                    if table == "aliases" {
                        games[i].aliases.push(value);
                    } else {
                        games[i].save_paths.push(value);
                    }
                }
            }
        }
        Ok(games)
    }
    pub fn game(&self, id: &str) -> Result<Game> {
        self.games_by_ids(&[id.to_owned()])?
            .pop()
            .ok_or_else(|| Error::Validation("游戏不存在".into()))
    }

    pub fn record_launch(&mut self, id: &str) -> Result<Game> {
        let transaction = self.connection.transaction()?;
        if transaction.execute(
            "UPDATE games SET last_launched_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),play_status=CASE WHEN play_status='COMPLETED' THEN 'COMPLETED' ELSE 'PLAYING' END WHERE id=?1",
            [id],
        )? != 1
        {
            return Err(Error::Validation("游戏不存在".into()));
        }
        transaction.execute("INSERT INTO launch_history(id,game_id,launched_at) SELECT ?1,id,last_launched_at FROM games WHERE id=?2", params![Uuid::new_v4().to_string(), id])?;
        transaction.commit()?;
        self.game(id)
    }
    pub fn launch_history(&self, id: &str) -> Result<Vec<String>> {
        self.game(id)?;
        let mut statement = self.connection.prepare("SELECT launched_at FROM launch_history WHERE game_id=?1 ORDER BY launched_at DESC,rowid DESC LIMIT 100")?;
        let values = statement
            .query_map([id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(values)
    }
    /// Rebuild an empty schema, including settings. No filesystem paths from game records are used.
    pub fn clear_library(&mut self) -> Result<ResetReport> {
        self.connection.execute_batch("PRAGMA secure_delete=ON;")?;
        let transaction = self.connection.transaction()?;
        transaction.execute_batch("DROP TABLE version_history; DROP TABLE launch_history; DROP TABLE aliases; DROP TABLE save_paths; DROP TABLE games; DROP TABLE settings;")?;
        transaction.execute_batch(include_str!("../migrations/001_initial.sql"))?;
        transaction.execute_batch(include_str!("../migrations/002_working_directory.sql"))?;
        transaction.execute_batch(include_str!("../migrations/003_launch_history.sql"))?;
        transaction.execute_batch(include_str!("../migrations/004_scan_workers.sql"))?;
        transaction.execute_batch(include_str!("../migrations/005_engine_source.sql"))?;
        transaction.execute_batch(include_str!("../migrations/006_launch_source.sql"))?;
        transaction.execute_batch(include_str!("../migrations/007_external_player.sql"))?;
        transaction.execute_batch(include_str!("../migrations/008_play_status.sql"))?;
        transaction.execute_batch(include_str!("../migrations/009_version_history.sql"))?;
        transaction.commit()?;
        // Logical reset has committed. A compaction failure must not leave stale records in the UI.
        let warning = self.compact_reset_file().err().map(|error| {
            format!("记录已清空，但数据库压缩或日志清理未完成，请关闭其他实例后重试清空：{error}")
        });
        Ok(ResetReport {
            settings: self.settings()?,
            warning,
        })
    }
    fn compact_reset_file(&self) -> Result<()> {
        self.checkpoint_reset()?;
        self.connection.execute_batch("VACUUM;")?;
        self.checkpoint_reset()
    }
    fn checkpoint_reset(&self) -> Result<()> {
        let busy: i64 = self
            .connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
        if busy != 0 {
            return Err(Error::Validation("数据库仍被其他连接占用".into()));
        }
        Ok(())
    }

    pub fn supplement_metadata(
        &mut self,
        values: &[(String, ScanCandidate)],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<String>> {
        let transaction = self.connection.transaction()?;
        let mut ids = vec![];
        for (id, c) in values {
            if cancelled() {
                return Err(Error::Validation("补充已取消，改动已回滚".into()));
            }
            let changed = transaction.prepare_cached("UPDATE games SET engine=CASE WHEN engine_source='manual' THEN engine ELSE ?2 END,current_version=CASE WHEN current_version='Unknown' AND version_source!='manual' THEN ?3 ELSE current_version END,version_source=CASE WHEN current_version='Unknown' AND version_source!='manual' THEN ?4 ELSE version_source END WHERE id=?1 AND (engine_source!='manual' AND engine!=?2 OR current_version='Unknown' AND version_source!='manual' AND ?3!='Unknown')")?.execute(params![id,c.engine,c.suggested_version,c.version_source])?;
            if changed > 0 {
                ids.push(id.clone());
            }
        }
        if cancelled() {
            return Err(Error::Validation("补充已取消，改动已回滚".into()));
        }
        transaction.commit()?;
        Ok(ids)
    }

    pub fn edit_game(&mut self, mut edit: GameEdit) -> Result<Game> {
        let existing = self.game(&edit.id)?;
        let engine = edit.engine.trim();
        if engine.is_empty() || engine.chars().count() > 80 || engine.chars().any(char::is_control)
        {
            return Err(Error::Validation(
                "请填写有效引擎名称，最长 80 个字符".into(),
            ));
        }
        if edit.canonical_title.trim().is_empty()
            || edit.display_title.trim().is_empty()
            || edit.current_version.trim().is_empty()
        {
            return Err(Error::Validation(
                "标题和版本不能为空（未知版本请填写 Unknown）".into(),
            ));
        }
        if !["DIRECT", "MTOOL", "EXTERNAL_PLAYER"].contains(&edit.launch_type.as_str()) {
            return Err(Error::Validation("请选择有效启动方式".into()));
        }
        let launch_changed = edit.launch_type != existing.launch_type
            || edit.main_executable != existing.main_executable
            || edit.external_player != existing.external_player
            || edit.mtool_target_exe != existing.mtool_target_exe
            || edit.mtool_loader != existing.mtool_loader
            || edit.working_directory != existing.working_directory;
        // Metadata/status edits remain available even when launch files are missing.
        if launch_changed {
            if edit.launch_type == "EXTERNAL_PLAYER" {
                let config = edit
                    .external_player
                    .as_ref()
                    .ok_or_else(|| Error::Validation("请配置外部播放器类型".into()))?;
                crate::external_player::validate(
                    Path::new(&existing.install_path),
                    edit.main_executable.as_deref(),
                    config,
                )?;
            } else {
                edit.external_player = None;
            }
            if let Some(exe) = &edit.main_executable {
                crate::paths::launch_file(Path::new(&existing.install_path), exe)?;
            }
            if edit.launch_type == "MTOOL" {
                if edit.mtool_target_exe.is_none() {
                    edit.mtool_target_exe = edit.main_executable.clone();
                }
                let target = edit
                    .mtool_target_exe
                    .as_deref()
                    .ok_or_else(|| Error::Validation("请配置 MTool target EXE".into()))?;
                contained_file(Path::new(&existing.install_path), target, "exe")?;
                if let Some(loader) = edit.mtool_loader.as_deref() {
                    let path = relative_path(loader)?;
                    if !path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                    {
                        return Err(Error::Validation("loader 必须为相对 DLL 路径".into()));
                    }
                }
            }
            crate::paths::working_directory(
                Path::new(&existing.install_path),
                &edit.working_directory,
            )?;
        }
        let transaction = self.connection.transaction()?;
        let engine_source: String = if engine != existing.engine {
            "manual".into()
        } else {
            transaction.query_row(
                "SELECT engine_source FROM games WHERE id=?1",
                [&edit.id],
                |r| r.get(0),
            )?
        };
        let version_source = if edit.current_version == existing.current_version {
            &existing.version_source
        } else {
            "manual"
        };
        transaction.execute("UPDATE games SET canonical_title=?2,display_title=?3,current_version=?4,version_source=?5,main_executable=?6,launch_type=?7,mtool_target_exe=?8,mtool_loader=?9,working_directory=?10,engine=?11,engine_source=?12,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![edit.id, edit.canonical_title.trim(), edit.display_title.trim(), edit.current_version.trim(), version_source, edit.main_executable, edit.launch_type, edit.mtool_target_exe, edit.mtool_loader, edit.working_directory, engine, engine_source])?;
        transaction.execute(
            "UPDATE games SET external_player=?2 WHERE id=?1",
            params![
                edit.id,
                edit.external_player
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|e| Error::Validation(e.to_string()))?
            ],
        )?;
        if edit.external_player != existing.external_player
            || edit.launch_type != existing.launch_type
            || edit.main_executable != existing.main_executable
            || edit.mtool_target_exe != existing.mtool_target_exe
            || edit.mtool_loader != existing.mtool_loader
            || edit.working_directory != existing.working_directory
        {
            transaction.execute(
                "UPDATE games SET launch_source='manual' WHERE id=?1",
                [&edit.id],
            )?;
        }
        transaction.execute(
            "UPDATE games SET play_status=?2 WHERE id=?1",
            params![edit.id, edit.play_status.as_str()],
        )?;
        // Replace metadata only; never delete or mutate anything in a game directory.
        transaction.execute("DELETE FROM aliases WHERE game_id=?1", [&edit.id])?;
        for alias in edit
            .aliases
            .iter()
            .chain(std::iter::once(&edit.canonical_title))
        {
            let alias = alias.trim();
            if alias.is_empty() {
                continue;
            }
            transaction.execute("INSERT OR IGNORE INTO aliases(id,game_id,alias,normalized_alias,source) VALUES(?1,?2,?3,?4,'manual')",
                params![Uuid::new_v4().to_string(), edit.id, alias, normalize_alias(alias)])?;
        }
        transaction.execute("DELETE FROM save_paths WHERE game_id=?1", [&edit.id])?;
        for path in &edit.save_paths {
            let path = path.trim();
            if path.is_empty() {
                continue;
            }
            if path.contains('\0') {
                return Err(Error::Validation("存档路径包含无效字符".into()));
            }
            transaction.execute(
                "INSERT OR IGNORE INTO save_paths(id,game_id,path) VALUES(?1,?2,?3)",
                params![Uuid::new_v4().to_string(), edit.id, path],
            )?;
        }
        transaction.commit()?;
        self.game(&edit.id)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn schema_nine_adds_empty_history_without_changing_old_library_values() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fixture.db");
        let root = temp.path().join("game");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("Game.exe"), b"fixture").unwrap();
        let mut db = Database::open(&path).unwrap();
        let id = db
            .register(&[crate::scanner::analyze_directory(&root).unwrap()])
            .unwrap()
            .remove(0);
        db.record_launch(&id).unwrap();
        let before = serde_json::to_string(&db.game(&id).unwrap()).unwrap();
        let history = db.launch_history(&id).unwrap();
        db.connection
            .execute_batch("DROP TABLE version_history;PRAGMA user_version=8;")
            .unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            serde_json::to_string(&db.game(&id).unwrap()).unwrap(),
            before
        );
        assert_eq!(db.launch_history(&id).unwrap(), history);
        assert!(db.version_history(&id).unwrap().is_empty());
        assert_eq!(
            db.connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            9
        );
        assert!(!db
            .connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
    }
    #[test]
    fn scan_fills_only_unconfigured_saves_and_new_import_uses_target_relative_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("魔法少女");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Game.exe"), b"fixture").unwrap();
        let mut db = Database::open(&temp.path().join("test.db")).unwrap();
        let id = db
            .register(&[analyze_directory(&root).unwrap()])
            .unwrap()
            .remove(0);
        assert!(db.game(&id).unwrap().save_paths.is_empty());
        std::fs::create_dir_all(root.join("www/save")).unwrap();
        let mut candidate = analyze_directory(&root).unwrap();
        assert_eq!(candidate.save_paths, vec!["<GAME>/www/save"]);
        candidate.registered_id = Some(id.clone());
        let changed = db
            .sync_mtool_defaults(std::slice::from_ref(&candidate))
            .unwrap();
        assert_eq!(changed[0].save_paths, vec!["<GAME>/www/save"]);
        assert!(db
            .sync_mtool_defaults(std::slice::from_ref(&candidate))
            .unwrap()
            .is_empty());
        let mut edit = draft(&changed[0]);
        edit.save_paths = vec!["<GAME>/自定义存档".into()];
        db.edit_game(edit).unwrap();
        assert!(db.sync_mtool_defaults(&[candidate]).unwrap().is_empty());
        assert_eq!(db.game(&id).unwrap().save_paths, vec!["<GAME>/自定义存档"]);

        let target = temp.path().join("新游戏 日本語");
        std::fs::create_dir_all(target.join("SaveData")).unwrap();
        std::fs::write(target.join("Game.exe"), b"fixture").unwrap();
        let candidate = analyze_directory(&target).unwrap();
        let entry = RegistrationEntry {
            candidate,
            executable: Some("Game.exe".into()),
            external_player: None,
            version: "Unknown".into(),
            version_source: "manual".into(),
            working_directory: ".".into(),
        };
        let id = db.register_import(entry, "new-import").unwrap();
        assert_eq!(db.game(&id).unwrap().save_paths, vec!["<GAME>/SaveData"]);
    }

    #[test]
    fn play_status_migration_and_manual_completed_survive_launches_and_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("status.db");
        let old = Connection::open(&path).unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_working_directory.sql"),
            include_str!("../migrations/003_launch_history.sql"),
            include_str!("../migrations/004_scan_workers.sql"),
            include_str!("../migrations/005_engine_source.sql"),
            include_str!("../migrations/006_launch_source.sql"),
            include_str!("../migrations/007_external_player.sql"),
        ] {
            old.execute_batch(migration).unwrap();
        }
        old.execute_batch("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,last_launched_at) VALUES('played','played','played','missing-a','missing-a','2020-01-01'),('new','new','new','missing-b','missing-b',NULL);").unwrap();
        drop(old);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(
            db.game("played").unwrap().play_status,
            crate::domain::PlayStatus::Playing
        );
        assert_eq!(
            db.game("new").unwrap().play_status,
            crate::domain::PlayStatus::Unplayed
        );
        let mut edit = draft(&db.game("new").unwrap());
        edit.play_status = crate::domain::PlayStatus::Completed;
        db.edit_game(edit).unwrap();
        assert_eq!(
            db.record_launch("new").unwrap().play_status,
            crate::domain::PlayStatus::Completed
        );
        let mut edit = draft(&db.game("new").unwrap());
        edit.play_status = crate::domain::PlayStatus::Unplayed;
        db.edit_game(edit).unwrap();
        assert_eq!(
            db.record_launch("new").unwrap().play_status,
            crate::domain::PlayStatus::Playing
        );
        drop(db);
        assert_eq!(
            Database::open(&path)
                .unwrap()
                .game("new")
                .unwrap()
                .play_status,
            crate::domain::PlayStatus::Playing
        );
        assert!(crate::domain::PlayStatus::parse("OTHER").is_err());
    }
    #[test]
    fn schema_seven_preserves_every_old_game_column_and_related_records() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("migration.db");
        let old = Connection::open(&path).unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_working_directory.sql"),
            include_str!("../migrations/003_launch_history.sql"),
            include_str!("../migrations/004_scan_workers.sql"),
            include_str!("../migrations/005_engine_source.sql"),
            include_str!("../migrations/006_launch_source.sql"),
        ] {
            old.execute_batch(migration).unwrap();
        }
        old.execute_batch("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,current_version,version_source,main_executable,engine,launch_type,mtool_target_exe,mtool_loader,working_directory,last_launched_at,engine_source,launch_source) VALUES('old','彼女 中文','Game','fixture','fixture','Final','manual','包装/game.exe','Unity','MTOOL','包装/game.exe','loaders/custom.dll','包装','2020-01-01','manual','manual'); INSERT INTO aliases VALUES('a','old','別名','別名','manual'); INSERT INTO save_paths VALUES('s','old','Save'); INSERT INTO launch_history VALUES('h','old','2020-01-01'); UPDATE settings SET scan_workers=4,game_root='fixture';").unwrap();
        let columns = "SELECT json_array(id,canonical_title,display_title,install_path,install_path_key,current_version,version_source,main_executable,engine,launch_type,mtool_target_exe,mtool_loader,created_at,updated_at,working_directory,last_launched_at,engine_source,launch_source) FROM games";
        let before: String = old.query_row(columns, [], |r| r.get(0)).unwrap();
        drop(old);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.connection
                .query_row(columns, [], |r| r.get::<_, String>(0))
                .unwrap(),
            before
        );
        assert_eq!(db.game("old").unwrap().aliases, vec!["別名"]);
        assert_eq!(db.game("old").unwrap().save_paths, vec!["Save"]);
        assert_eq!(db.launch_history("old").unwrap(), vec!["2020-01-01"]);
        assert_eq!(db.settings().unwrap().scan_workers, 4);
        assert!(!db
            .connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
        assert_eq!(
            db.connection
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            db.connection
                .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn qsp_relative_config_survives_edit_reopen_relocation_and_reset() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("魔法少女");
        std::fs::create_dir(&root).unwrap();
        for file in ["game.qsp", "mod.qsp", "qspgui.exe", "プレイヤー.exe"] {
            std::fs::write(root.join(file), b"fixture").unwrap();
        }
        let mut candidate = analyze_directory(&root).unwrap();
        candidate.qsp.as_mut().unwrap().game_files = vec!["game.qsp".into()];
        candidate.qsp.as_mut().unwrap().recommended_player = Some("qspgui.exe".into());
        let path = temp.path().join("test.db");
        let mut db = Database::open(&path).unwrap();
        let id = db.register(&[candidate]).unwrap().remove(0);
        let mut edit = draft(&db.game(&id).unwrap());
        edit.main_executable = Some("プレイヤー.exe".into());
        edit.external_player.as_mut().unwrap().game_file = Some("mod.qsp".into());
        edit.save_paths = vec!["Save".into()];
        let saved = db.edit_game(edit).unwrap();
        assert_eq!(saved.launch_type, "EXTERNAL_PLAYER");
        assert_eq!(crate::maintenance::check_game(&saved).state, "available");
        std::fs::remove_file(root.join("mod.qsp")).unwrap();
        assert_eq!(
            crate::maintenance::check_game(&saved).state,
            "missing_launch"
        );
        std::fs::write(root.join("mod.qsp"), b"fixture").unwrap();
        drop(db);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.game(&id).unwrap().external_player, saved.external_player);
        let new_root = temp.path().join("renamed");
        std::fs::rename(&root, &new_root).unwrap();
        let plan = crate::maintenance::relocation_plan(&saved, &new_root).unwrap();
        let moved = db.relocate_game(plan).unwrap();
        assert_eq!(moved.external_player, saved.external_player);
        assert_eq!(moved.id, id);
        assert_eq!(crate::maintenance::check_game(&moved).state, "available");
        db.clear_library().unwrap();
        assert!(db.games().unwrap().is_empty());
        assert!(new_root.join("mod.qsp").is_file());
    }
    use super::*;
    use crate::scanner::analyze_directory;
    #[test]
    fn relocation_preserves_identity_manual_metadata_history_and_rebases_only_internal_saves() {
        let temp = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(temp.path()).unwrap();
        let old = base.join("旧游戏 v1.2");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("Game.exe"), b"game fixture").unwrap();
        std::fs::write(old.join("save.dat"), b"save fixture").unwrap();
        let mut db = Database::open(&temp.path().join("library.db")).unwrap();
        let id = db
            .register(&[analyze_directory(&old).unwrap()])
            .unwrap()
            .remove(0);
        let mut edit = draft(&db.game(&id).unwrap());
        edit.engine = "QSP".into();
        edit.current_version = "Manual".into();
        edit.aliases = vec!["Alias".into()];
        let external = temp.path().join("external-save").display().to_string();
        edit.save_paths = vec![
            old.join("save.dat").display().to_string(),
            external.clone(),
            "<GAME>\\save".into(),
        ];
        db.edit_game(edit).unwrap();
        db.record_launch(&id).unwrap();
        let before = db.game(&id).unwrap();
        let history = db.launch_history(&id).unwrap();
        let new = base.join("新目录 & 测试");
        // Simulate a user moving the folder before opening the manager's preview.
        std::fs::rename(&old, &new).unwrap();
        assert_eq!(
            crate::maintenance::check_game(&before).state,
            "missing_directory"
        );
        let plan = crate::maintenance::relocation_plan(&before, &new).unwrap();
        assert_eq!(db.game(&id).unwrap().install_path, before.install_path);
        let saved = db.relocate_game(plan).unwrap();
        assert_eq!(saved.id, before.id);
        assert_eq!(saved.aliases, before.aliases);
        assert_eq!(saved.current_version, before.current_version);
        assert_eq!(saved.engine, "QSP");
        assert_eq!(saved.created_at, before.created_at);
        assert_eq!(saved.last_launched_at, before.last_launched_at);
        assert_eq!(db.launch_history(&id).unwrap(), history);
        assert!(saved
            .save_paths
            .contains(&new.join("save.dat").display().to_string()));
        assert!(saved.save_paths.contains(&external));
        assert!(saved.save_paths.contains(&"<GAME>\\save".into()));
        assert_eq!(crate::maintenance::check_game(&saved).state, "available");
        assert_eq!(
            std::fs::read(new.join("save.dat")).unwrap(),
            b"save fixture"
        );
        assert_eq!(
            std::fs::read(new.join("Game.exe")).unwrap(),
            b"game fixture"
        );
        assert_eq!(
            db.connection
                .query_row("SELECT engine_source FROM games WHERE id=?1", [&id], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
            "manual"
        );
        drop(db);
        assert_eq!(
            Database::open(&temp.path().join("library.db"))
                .unwrap()
                .game(&id)
                .unwrap()
                .install_path,
            saved.install_path
        );
    }
    #[test]
    fn relocation_rejects_duplicate_paths_stale_previews_and_escaping_launch_files_without_changes()
    {
        let temp = tempfile::tempdir().unwrap();
        let mut db = Database::open(&temp.path().join("library.db")).unwrap();
        let mut ids = vec![];
        for name in ["First", "Second"] {
            let root = temp.path().join(name);
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("Game.exe"), b"fixture").unwrap();
            ids.extend(db.register(&[analyze_directory(&root).unwrap()]).unwrap());
        }
        let first = db.game(&ids[0]).unwrap();
        let second = db.game(&ids[1]).unwrap();
        let duplicate =
            crate::maintenance::relocation_plan(&first, Path::new(&second.install_path)).unwrap();
        assert!(db.relocate_game(duplicate).is_err());
        let new = temp.path().join("New");
        std::fs::create_dir(&new).unwrap();
        let mut escaping = crate::maintenance::relocation_plan(&first, &new).unwrap();
        escaping.main_executable = Some("../First/Game.exe".into());
        assert!(db.relocate_game(escaping).is_err());
        let mut stale = crate::maintenance::relocation_plan(&first, &new).unwrap();
        stale.expected_install_path = "Old preview".into();
        assert!(db.relocate_game(stale).is_err());
        let after = db.game(&first.id).unwrap();
        assert_eq!(after.install_path, first.install_path);
        assert_eq!(after.updated_at, first.updated_at);
        assert_eq!(db.games().unwrap().len(), 2);
    }
    #[test]
    fn removing_one_record_preserves_other_records_settings_and_all_game_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.db");
        let mut db = Database::open(&path).unwrap();
        let mut ids = vec![];
        for name in ["First", "Second"] {
            let root = temp.path().join(name);
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("Game.exe"), b"game").unwrap();
            std::fs::write(root.join("save.dat"), b"save").unwrap();
            ids.extend(db.register(&[analyze_directory(&root).unwrap()]).unwrap());
        }
        let mut edit = draft(&db.game(&ids[0]).unwrap());
        edit.save_paths = vec!["save.dat".into()];
        db.edit_game(edit).unwrap();
        db.record_launch(&ids[0]).unwrap();
        let mut settings = db.settings().unwrap();
        settings.game_root = dunce::canonicalize(temp.path())
            .unwrap()
            .display()
            .to_string();
        db.save_settings(settings.clone()).unwrap();
        db.remove_game(&ids[0]).unwrap();
        assert!(db.game(&ids[0]).is_err());
        assert!(db.remove_game(&ids[0]).is_err());
        for table in ["aliases", "save_paths", "launch_history"] {
            assert_eq!(
                db.connection
                    .query_row(
                        &format!("SELECT COUNT(*) FROM {table} WHERE game_id=?1"),
                        [&ids[0]],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                0
            );
        }
        assert_eq!(db.games().unwrap().len(), 1);
        assert_eq!(db.settings().unwrap().game_root, settings.game_root);
        for name in ["First", "Second"] {
            assert_eq!(
                std::fs::read(temp.path().join(name).join("save.dat")).unwrap(),
                b"save"
            );
            assert_eq!(
                std::fs::read(temp.path().join(name).join("Game.exe")).unwrap(),
                b"game"
            );
        }
        drop(db);
        assert_eq!(
            Database::open(&path).unwrap().games().unwrap()[0].id,
            ids[1]
        );
    }
    #[test]
    fn engine_migration_and_manual_override_survive_refresh_and_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.db");
        let connection = Connection::open(&path).unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_working_directory.sql"),
            include_str!("../migrations/003_launch_history.sql"),
            include_str!("../migrations/004_scan_workers.sql"),
        ] {
            connection.execute_batch(migration).unwrap();
        }
        connection.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,engine,version_source) VALUES('old','Old','Old',?1,'fixture','Unity','unknown')", [temp.path().to_str().unwrap()]).unwrap();
        drop(connection);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.game("old").unwrap().engine, "Unity");
        let created_at = db.game("old").unwrap().created_at;
        let mut edit = draft(&db.game("old").unwrap());
        edit.engine = "QSP".into();
        edit.save_paths = vec![temp.path().join("save").display().to_string()];
        db.edit_game(edit).unwrap();
        let mut candidate = analyze_directory(temp.path()).unwrap();
        candidate.engine = "Ren'Py".into();
        candidate.suggested_version = "v1.2.3".into();
        candidate.version_source = "folder_name".into();
        db.supplement_metadata(&[("old".into(), candidate.clone())], &|| false)
            .unwrap();
        assert_eq!(db.game("old").unwrap().engine, "QSP");
        assert_eq!(db.game("old").unwrap().current_version, "v1.2.3");
        drop(db);
        let mut db = Database::open(&path).unwrap();
        let saved = db.game("old").unwrap();
        assert_eq!(saved.engine, "QSP");
        assert_eq!(saved.created_at, created_at);
        assert_eq!(saved.save_paths.len(), 1);
        let mut edit = draft(&saved);
        edit.display_title = "Renamed".into();
        db.edit_game(edit.clone()).unwrap();
        assert!(db
            .supplement_metadata(&[("old".into(), candidate.clone())], &|| false)
            .unwrap()
            .is_empty());
        edit.engine = "Unknown".into();
        db.edit_game(edit.clone()).unwrap();
        db.supplement_metadata(&[("old".into(), candidate)], &|| false)
            .unwrap();
        assert_eq!(db.game("old").unwrap().engine, "Unknown");
        edit.engine = "bad\nengine".into();
        assert!(db.edit_game(edit).is_err());
        assert_eq!(db.game("old").unwrap().engine, "Unknown");
        db.clear_library().unwrap();
        assert_eq!(
            db.connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            9
        );
    }
    #[test]
    fn document_edit_persists_without_loosening_mtool_exe_requirements() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.db");
        std::fs::write(temp.path().join("game.html"), b"fixture").unwrap();
        let mut db = Database::open(&path).unwrap();
        let id = db
            .register(&[analyze_directory(temp.path()).unwrap()])
            .unwrap()
            .remove(0);
        let mut edit = draft(&db.game(&id).unwrap());
        edit.main_executable = Some("game.html".into());
        db.edit_game(edit.clone()).unwrap();
        edit.launch_type = "MTOOL".into();
        edit.mtool_target_exe = Some("game.html".into());
        edit.mtool_loader = Some("loader.dll".into());
        assert!(db.edit_game(edit).is_err());
        assert_eq!(db.game(&id).unwrap().launch_type, "DIRECT");
        drop(db);
        assert_eq!(
            Database::open(&path)
                .unwrap()
                .game(&id)
                .unwrap()
                .main_executable
                .as_deref(),
            Some("game.html")
        );
    }
    #[test]
    fn version_two_migration_history_and_full_reset_preserve_game_files() {
        let root = tempfile::tempdir().unwrap();
        let game_path = root.path().join("游戏Ver1.06");
        std::fs::create_dir(&game_path).unwrap();
        std::fs::write(game_path.join("Game.exe"), b"fixture").unwrap();
        std::fs::write(game_path.join("save.dat"), b"save-data").unwrap();
        let path = root.path().join("library.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/002_working_directory.sql"))
            .unwrap();
        connection.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,created_at) VALUES('old','Old','Old','fixture','fixture','2020-01-01T00:00:00.000Z')", []).unwrap();
        drop(connection);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(
            db.game("old").unwrap().created_at,
            "2020-01-01T00:00:00.000Z"
        );
        assert!(db.game("old").unwrap().last_launched_at.is_none());
        assert!(db.launch_history("old").unwrap().is_empty());
        let id = db
            .register(&[analyze_directory(&game_path).unwrap()])
            .unwrap()
            .remove(0);
        let added = db.game(&id).unwrap().created_at;
        let launched = db.record_launch(&id).unwrap();
        assert_eq!(launched.created_at, added);
        assert_eq!(
            db.launch_history(&id).unwrap(),
            vec![launched.last_launched_at.unwrap()]
        );
        db.record_launch(&id).unwrap();
        assert_eq!(db.launch_history(&id).unwrap().len(), 2);
        assert!(db.record_launch("missing").is_err());
        let mut edit = draft(&db.game(&id).unwrap());
        edit.aliases = vec!["Alias".into()];
        edit.save_paths = vec!["save.dat".into()];
        db.edit_game(edit).unwrap();
        let mut settings = db.settings().unwrap();
        settings.game_root = path_text(root.path()).unwrap();
        db.save_settings(settings).unwrap();
        drop(db);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.launch_history(&id).unwrap().len(), 2);
        let report = db.clear_library().unwrap();
        assert!(report.warning.is_none());
        let defaults = report.settings;
        assert!(defaults.game_root.is_empty() && defaults.mtool_root.is_empty());
        assert_eq!(defaults.mtool_injector, "loaders/inject.exe");
        for table in ["games", "aliases", "save_paths", "launch_history"] {
            assert_eq!(
                db.connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            std::fs::read(game_path.join("save.dat")).unwrap(),
            b"save-data"
        );
        assert!(game_path.join("Game.exe").is_file());
        drop(db);
        let mut db = Database::open(&path).unwrap();
        assert!(db.games().unwrap().is_empty());
        assert_eq!(
            db.register(&[analyze_directory(&game_path).unwrap()])
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn reset_reports_committed_empty_state_when_another_reader_blocks_compaction() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.db");
        let mut db = Database::open(&path).unwrap();
        db.register(&[analyze_directory(temp.path()).unwrap()])
            .unwrap();
        let reader = Connection::open(&path).unwrap();
        reader.execute_batch("BEGIN;").unwrap();
        assert_eq!(
            reader
                .query_row("SELECT COUNT(*) FROM games", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.connection
            .busy_timeout(std::time::Duration::from_millis(1))
            .unwrap();
        let report = db.clear_library().unwrap();
        assert!(report.warning.is_some());
        assert!(db.games().unwrap().is_empty());
        assert!(report.settings.game_root.is_empty());
        reader.execute_batch("ROLLBACK;").unwrap();
        assert!(db.clear_library().unwrap().warning.is_none());
    }
    #[test]
    fn supplementation_is_atomic_and_preserves_manual_versions_and_added_time() {
        let temp = tempfile::tempdir().unwrap();
        let candidate = analyze_directory(temp.path()).unwrap();
        let mut db = Database::open(&temp.path().join("test.db")).unwrap();
        let id = db
            .register(std::slice::from_ref(&candidate))
            .unwrap()
            .remove(0);
        let added = db.game(&id).unwrap().created_at;
        let mut changed = candidate;
        changed.suggested_version = "Ver1.06".into();
        changed.version_source = "folder_name".into();
        changed.engine = "Ren'Py".into();
        let values = vec![(id.clone(), changed.clone())];
        let calls = std::cell::Cell::new(0);
        assert!(db
            .supplement_metadata(&values, &|| {
                calls.set(calls.get() + 1);
                calls.get() > 1
            })
            .is_err());
        assert_eq!(db.game(&id).unwrap().current_version, "Unknown");
        assert_eq!(db.game(&id).unwrap().engine, "Unknown");
        assert_eq!(
            db.supplement_metadata(&values, &|| false).unwrap(),
            vec![id.clone()]
        );
        assert_eq!(db.game(&id).unwrap().current_version, "Ver1.06");
        let mut edit = draft(&db.game(&id).unwrap());
        edit.current_version = "Final manual".into();
        db.edit_game(edit).unwrap();
        changed.suggested_version = "v2.0".into();
        changed.engine = "Unity".into();
        db.supplement_metadata(&[(id.clone(), changed)], &|| false)
            .unwrap();
        assert_eq!(db.game(&id).unwrap().current_version, "Final manual");
        let mut unknown = analyze_directory(temp.path()).unwrap();
        unknown.engine = "Unknown".into();
        db.supplement_metadata(&[(id.clone(), unknown)], &|| false)
            .unwrap();
        assert_eq!(db.game(&id).unwrap().engine, "Unknown");
        assert_eq!(db.game(&id).unwrap().current_version, "Final manual");
        assert_eq!(db.game(&id).unwrap().created_at, added);
        assert!(db.game(&id).unwrap().last_launched_at.is_none());
    }
    #[test]
    fn version_one_migration_preserves_existing_library_and_settings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("old.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,current_version) VALUES('old','旧游戏','旧游戏','fixture','fixture','Final')", []).unwrap();
        connection
            .execute(
                "INSERT INTO aliases VALUES('alias','old','日本語','日本語','manual')",
                [],
            )
            .unwrap();
        connection
            .execute("UPDATE settings SET game_root='saved-root' WHERE id=1", [])
            .unwrap();
        drop(connection);
        let db = Database::open(&path).unwrap();
        let game = db.game("old").unwrap();
        assert_eq!(game.current_version, "Final");
        assert_eq!(game.working_directory, ".");
        assert_eq!(game.aliases, vec!["日本語"]);
        assert_eq!(db.settings().unwrap().game_root, "saved-root");
        drop(db);
        assert!(Database::open(&path).is_ok());
    }
    #[test]
    fn batch_cancel_rolls_back_and_retry_uses_selected_exe_version_and_directory() {
        let temp = tempfile::tempdir().unwrap();
        let mut entries = vec![];
        for i in 0..3 {
            let path = temp.path().join(format!("game{i}"));
            std::fs::create_dir_all(path.join("包装")).unwrap();
            std::fs::write(path.join("包装/任意.exe"), b"fixture").unwrap();
            entries.push(RegistrationEntry {
                candidate: analyze_directory(&path).unwrap(),
                executable: Some("包装/任意.exe".into()),
                external_player: None,
                version: "Final / manual".into(),
                version_source: "manual".into(),
                working_directory: "包装".into(),
            });
        }
        let mut db = Database::open(&temp.path().join("test.db")).unwrap();
        let stop = std::cell::Cell::new(false);
        assert!(db
            .register_entries(&entries, &|| stop.get(), &|_, _| stop.set(true))
            .is_err());
        assert!(db.games().unwrap().is_empty());
        let ids = db
            .register_entries(&entries, &|| false, &|_, _| {})
            .unwrap();
        let games = db.games_by_ids(&ids).unwrap();
        assert_eq!(games.len(), 3);
        for game in games {
            assert_eq!(game.working_directory, "包装");
            assert_eq!(game.main_executable.as_deref(), Some("包装/任意.exe"));
            assert_eq!(game.current_version, "Final / manual");
            assert_eq!(game.aliases.len(), 1);
        }
        assert_eq!(
            db.register_entries(&entries, &|| false, &|_, _| {})
                .unwrap(),
            ids
        );
    }
    fn draft(game: &Game) -> GameEdit {
        GameEdit {
            id: game.id.clone(),
            canonical_title: game.canonical_title.clone(),
            display_title: game.display_title.clone(),
            current_version: game.current_version.clone(),
            engine: game.engine.clone(),
            play_status: game.play_status.clone(),
            main_executable: game.main_executable.clone(),
            working_directory: game.working_directory.clone(),
            launch_type: game.launch_type.clone(),
            external_player: game.external_player.clone(),
            mtool_target_exe: None,
            mtool_loader: None,
            aliases: game.aliases.clone(),
            save_paths: vec![],
        }
    }

    #[test]
    fn qsp_scan_refreshes_existing_engine_without_overwriting_launch_or_manual_engine() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ETO");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("Qqsp.exe"), b"fixture").unwrap();
        let old = analyze_directory(&root).unwrap();
        let mut db = Database::open(&temp.path().join("db.sqlite")).unwrap();
        let id = db.register(&[old]).unwrap().remove(0);
        let original = db.game(&id).unwrap();
        std::fs::write(root.join("game.qsp"), b"fixture").unwrap();
        std::fs::write(root.join("start with tool.bat"), b"fixture").unwrap();
        let mut scan = crate::scanner::analyze_quick_controlled(&root, &|| false, &|_| {}).unwrap();
        scan.registered_id = Some(id.clone());
        let saved = db.sync_mtool_defaults(&[scan.clone()]).unwrap().remove(0);
        assert_eq!(saved.engine, "QSP");
        assert_eq!(saved.launch_type, original.launch_type);
        assert_eq!(saved.main_executable, original.main_executable);
        assert!(saved.external_player.is_none());
        assert!(db.sync_mtool_defaults(&[scan.clone()]).unwrap().is_empty());
        let mut edit = draft(&saved);
        edit.engine = "Custom Engine".into();
        db.edit_game(edit).unwrap();
        assert!(db.sync_mtool_defaults(&[scan]).unwrap().is_empty());
        assert_eq!(db.game(&id).unwrap().engine, "Custom Engine");
    }

    #[test]
    fn generated_bat_defaults_register_and_sync_mtool_without_overwriting_manual_launch_choices() {
        let temp = tempfile::tempdir().unwrap();
        let game_dir = temp.path().join("game");
        std::fs::create_dir_all(game_dir.join("wrapper")).unwrap();
        std::fs::write(game_dir.join("wrapper/任意.exe"), b"fixture").unwrap();
        let candidate = analyze_directory(&game_dir).unwrap();
        let mut db = Database::open(&temp.path().join("db.sqlite")).unwrap();
        let id = db
            .register(std::slice::from_ref(&candidate))
            .unwrap()
            .remove(0);
        let original = db.game(&id).unwrap();
        db.record_launch(&id).unwrap();
        db.connection
            .execute("UPDATE games SET launch_source='legacy' WHERE id=?1", [&id])
            .unwrap();
        std::fs::write(
            game_dir.join("Start With Tool.bat"),
            b"not-read-by-quick-scan",
        )
        .unwrap();
        let mut scan =
            crate::scanner::analyze_quick_controlled(&game_dir, &|| false, &|_| {}).unwrap();
        scan.registered_id = Some(id.clone());
        let changed = db.sync_mtool_defaults(&[scan.clone()]).unwrap();
        assert_eq!(changed.len(), 1);
        let saved = &changed[0];
        assert_eq!(saved.launch_type, "MTOOL");
        assert_eq!(saved.mtool_target_exe, saved.main_executable);
        assert!(saved.mtool_loader.is_none());
        assert_eq!(saved.working_directory, ".");
        assert_eq!(saved.id, original.id);
        assert_eq!(saved.aliases, original.aliases);
        assert_eq!(saved.current_version, original.current_version);
        assert_eq!(saved.created_at, original.created_at);
        assert_eq!(db.launch_history(&id).unwrap().len(), 1);
        assert!(db.sync_mtool_defaults(&[scan.clone()]).unwrap().is_empty());
        let mut edit = draft(saved);
        edit.launch_type = "DIRECT".into();
        edit.mtool_target_exe = None;
        db.edit_game(edit).unwrap();
        assert!(db.sync_mtool_defaults(&[scan.clone()]).unwrap().is_empty());
        drop(db);
        let mut db = Database::open(&temp.path().join("db.sqlite")).unwrap();
        assert!(db.sync_mtool_defaults(&[scan]).unwrap().is_empty());
        assert_eq!(db.game(&id).unwrap().launch_type, "DIRECT");

        let second = temp.path().join("second");
        std::fs::create_dir(&second).unwrap();
        std::fs::write(second.join("非Game.exe"), b"fixture").unwrap();
        std::fs::write(second.join("与工具一同启动.bat"), b"not-read").unwrap();
        let detected =
            crate::scanner::analyze_quick_controlled(&second, &|| false, &|_| {}).unwrap();
        let id = db.register(&[detected]).unwrap().remove(0);
        let new = db.game(&id).unwrap();
        assert_eq!(new.launch_type, "MTOOL");
        assert_eq!(new.main_executable.as_deref(), Some("非Game.exe"));
        assert!(new.mtool_loader.is_none());
        let mut edit = draft(&new);
        edit.mtool_target_exe = None;
        assert_eq!(
            db.edit_game(edit).unwrap().mtool_target_exe,
            new.main_executable
        );
    }

    #[test]
    fn version_five_migration_adds_launch_source_without_changing_existing_game_fields() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("old.sqlite");
        let connection = Connection::open(&file).unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_working_directory.sql"),
            include_str!("../migrations/003_launch_history.sql"),
            include_str!("../migrations/004_scan_workers.sql"),
            include_str!("../migrations/005_engine_source.sql"),
        ] {
            connection.execute_batch(migration).unwrap();
        }
        connection.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,current_version,main_executable,launch_type,mtool_loader) VALUES('identity','old','old','missing','missing','Final','任意.exe','MTOOL','custom.dll')", []).unwrap();
        drop(connection);
        let db = Database::open(&file).unwrap();
        let game = db.game("identity").unwrap();
        assert_eq!(game.launch_type, "MTOOL");
        assert_eq!(game.current_version, "Final");
        assert_eq!(game.mtool_loader.as_deref(), Some("custom.dll"));
        assert_eq!(
            db.connection
                .query_row("SELECT launch_source FROM games", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "legacy"
        );
    }
    #[test]
    fn migration_registration_edit_and_reopen() {
        let root = tempfile::tempdir().unwrap();
        let game_dir = root.path().join("ゲーム v1.02");
        std::fs::create_dir(&game_dir).unwrap();
        std::fs::write(game_dir.join("主程序.exe"), b"fixture").unwrap();
        let candidate = analyze_directory(&game_dir).unwrap();
        let db_path = root.path().join("library.db");
        let mut db = Database::open(&db_path).unwrap();
        let ids = db.register(std::slice::from_ref(&candidate)).unwrap();
        assert_eq!(db.register(&[candidate]).unwrap(), ids);
        assert_eq!(db.games().unwrap().len(), 1);
        let game = db.game(&ids[0]).unwrap();
        let mut edit = draft(&game);
        edit.current_version = "Final / build 20261004".into();
        edit.aliases = vec!["中文名".into(), "日本語".into(), "English title".into()];
        edit.save_paths = vec!["<GAME>\\save".into(), "%APPDATA%\\测试".into()];
        db.edit_game(edit).unwrap();
        drop(db);
        let db = Database::open(&db_path).unwrap();
        let loaded = db.game(&ids[0]).unwrap();
        assert_eq!(loaded.current_version, "Final / build 20261004");
        assert_eq!(loaded.save_paths.len(), 2);
        assert!(loaded.aliases.contains(&"日本語".into()));
        assert!(game_dir.join("主程序.exe").is_file());
    }
    #[test]
    fn failed_metadata_transaction_rolls_back() {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(&root.path().join("library.db")).unwrap();
        let candidate = analyze_directory(root.path()).unwrap();
        let id = db.register(&[candidate]).unwrap().remove(0);
        let game = db.game(&id).unwrap();
        let mut edit = draft(&game);
        edit.display_title = "should roll back".into();
        edit.save_paths = vec!["invalid\0path".into()];
        assert!(db.edit_game(edit).is_err());
        assert_eq!(db.game(&id).unwrap().display_title, game.display_title);
        assert_eq!(db.game(&id).unwrap().aliases, game.aliases);
    }
    #[test]
    fn settings_validate_roots_and_relative_programs() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(&root.path().join("library.db")).unwrap();
        let mut settings = db.settings().unwrap();
        settings.game_root = path_text(root.path()).unwrap();
        settings.mtool_injector = "../inject.exe".into();
        assert!(db.save_settings(settings).is_err());
        assert!(db.settings().unwrap().game_root.is_empty());
    }
    #[test]
    fn scan_workers_migrate_persist_validate_and_reset_without_changing_games() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("v3.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/002_working_directory.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/003_launch_history.sql"))
            .unwrap();
        connection.execute("INSERT INTO games(id,canonical_title,display_title,install_path,install_path_key,current_version) VALUES('old','旧游戏','旧游戏','fixture','fixture','Final')", []).unwrap();
        connection
            .execute(
                "UPDATE settings SET game_root=?1 WHERE id=1",
                [path_text(root.path()).unwrap()],
            )
            .unwrap();
        drop(connection);
        let db = Database::open(&path).unwrap();
        assert_eq!(db.settings().unwrap().scan_workers, 2);
        assert_eq!(
            db.settings().unwrap().game_root,
            path_text(root.path()).unwrap()
        );
        assert_eq!(db.game("old").unwrap().current_version, "Final");
        let mut settings = db.settings().unwrap();
        settings.scan_workers = 4;
        db.save_settings(settings.clone()).unwrap();
        settings.scan_workers = 8;
        assert!(db.save_settings(settings).is_err());
        assert_eq!(db.settings().unwrap().scan_workers, 4);
        drop(db);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.settings().unwrap().scan_workers, 4);
        assert_eq!(db.games().unwrap().len(), 1);
        assert_eq!(db.clear_library().unwrap().settings.scan_workers, 2);
        assert!(db.games().unwrap().is_empty());
        drop(db);
        assert_eq!(
            Database::open(&path)
                .unwrap()
                .settings()
                .unwrap()
                .scan_workers,
            2
        );
    }
}
