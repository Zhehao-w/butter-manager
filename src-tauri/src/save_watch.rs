//! On-demand filesystem notifications. Only filenames from the capture window are retained.
use crate::save_discovery::{noise_directory, plain_dir, save_filename, SaveHint};
use notify::{
    event::{ModifyKind, RenameMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

const WINDOW: Duration = Duration::from_secs(180);
type Root = (String, PathBuf);

fn relevant(root: &Root, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(&root.1) else {
        return false;
    };
    let parent = relative.parent().unwrap_or(Path::new(""));
    if parent
        .iter()
        .any(|part| noise_directory(&part.to_string_lossy()))
    {
        return false;
    }
    if root.0 != "<GAME>"
        && relative.iter().next().is_some_and(|p| {
            matches!(
                p.to_string_lossy().to_ascii_lowercase().as_str(),
                "microsoft"
                    | "google"
                    | "mozilla"
                    | "packages"
                    | "nvidia"
                    | "nvidia corporation"
                    | "discord"
            )
        })
    {
        return false;
    }
    let ext = relative
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let name = relative
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    !matches!(
        ext.as_str(),
        "log" | "tmp" | "crash" | "dmp" | "dll" | "exe" | "pak" | "pdb" | "etl" | "lnk" | "lock"
    ) && !matches!(
        name.as_str(),
        "lock" | "lockfile" | "preferences" | "local state" | "desktop.ini" | "player-prev.log"
    )
}

#[derive(Default)]
struct Changes {
    paths: BTreeSet<PathBuf>,
    partial: bool,
}
impl Changes {
    fn observe(&mut self, event: notify::Result<Event>, roots: &[Root], excluded: &Path) {
        let Ok(event) = event else {
            self.partial = true;
            return;
        };
        if event.need_rescan() {
            self.partial = true;
        }
        if !matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Modify(
                    ModifyKind::Any
                        | ModifyKind::Data(_)
                        | ModifyKind::Name(RenameMode::To | RenameMode::Both | RenameMode::Any)
                )
        ) {
            return;
        }
        for path in event.paths {
            if path.starts_with(excluded) {
                continue;
            }
            if !roots
                .iter()
                .find(|r| path.starts_with(&r.1))
                .is_some_and(|r| relevant(r, &path))
            {
                continue;
            }
            if self.paths.len() < 4096 {
                self.paths.insert(path);
            } else {
                self.partial = true;
            }
        }
    }
    fn report(&self, roots: &[Root]) -> WatchReport {
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in &self.paths {
            if !path.is_file() || crate::deletion::plain_path(path).is_err() {
                continue;
            }
            let Some((label, anchor)) = roots.iter().find(|r| path.starts_with(&r.1)) else {
                continue;
            };
            let Ok(rel) = path.strip_prefix(anchor) else {
                continue;
            };
            let Some(dir) = rel.parent() else { continue };
            // Do not offer a game root, AppData root or a bare publisher directory for deletion/backup.
            let target = if dir.as_os_str().is_empty()
                || (label != "<GAME>" && dir.components().count() == 1)
            {
                rel
            } else {
                dir
            };
            let relative = target.to_string_lossy().replace('\\', "/");
            if relative.contains('%') {
                continue;
            }
            let names = groups.entry(format!("{label}/{relative}")).or_default();
            names.push(
                rel.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let mut ranked: Vec<_> = groups
            .into_iter()
            .map(|(path, mut files)| {
                files.sort_by_key(|s| (!save_filename(Path::new(s)), s.clone()));
                let score = files.iter().any(|f| save_filename(Path::new(f)));
                let reason = format!("记录期间有 {} 个文件写入", files.len());
                files.truncate(4);
                (
                    score,
                    SaveHint {
                        path,
                        reason,
                        files,
                        can_add: true,
                    },
                )
            })
            .collect();
        ranked.sort_by_key(|(known, h)| (!*known, !h.path.starts_with("<GAME>"), h.path.clone()));
        WatchReport {
            candidates: ranked.into_iter().map(|(_, h)| h).collect(),
            partial: self.partial,
        }
    }
}

#[derive(Serialize)]
pub struct WatchReport {
    pub candidates: Vec<SaveHint>,
    pub partial: bool,
}
#[derive(Serialize)]
pub struct Started {
    pub token: String,
    pub roots: Vec<String>,
}
pub struct Session {
    token: String,
    finish: mpsc::Sender<()>,
    worker: thread::JoinHandle<()>,
    changes: Arc<Mutex<Changes>>,
    roots: Vec<Root>,
}

impl Session {
    pub fn start(game: &Path, excluded: &Path) -> Result<(Self, Started), String> {
        let mut roots = vec![("<GAME>".to_string(), game.to_path_buf())];
        for (var, label, suffix) in [
            (
                "USERPROFILE",
                "%USERPROFILE%/AppData/LocalLow",
                "AppData/LocalLow",
            ),
            ("LOCALAPPDATA", "%LOCALAPPDATA%", ""),
            ("APPDATA", "%APPDATA%", ""),
        ] {
            if let Some(p) = std::env::var_os(var) {
                roots.push((label.into(), PathBuf::from(p).join(suffix)));
            }
        }
        Self::watch(roots, excluded, WINDOW)
    }
    fn watch(
        roots: Vec<Root>,
        excluded: &Path,
        timeout: Duration,
    ) -> Result<(Self, Started), String> {
        let roots: Vec<_> = roots
            .into_iter()
            .filter_map(|(name, p)| {
                (plain_dir(&p) && crate::deletion::plain_path(&p).is_ok())
                    .then(|| dunce::canonicalize(p).ok().map(|v| (name, v)))
                    .flatten()
            })
            .collect();
        let changes = Arc::new(Mutex::new(Changes::default()));
        let captured = changes.clone();
        let locations = roots.clone();
        let excluded = dunce::canonicalize(excluded).unwrap_or_else(|_| excluded.to_owned());
        let mut watcher = notify::recommended_watcher(move |event| {
            if let Ok(mut seen) = captured.lock() {
                seen.observe(event, &locations, &excluded);
            }
        })
        .map_err(|e| format!("无法启动文件监测：{e}"))?;
        let roots: Vec<_> = roots
            .into_iter()
            .filter(|(_, p)| watcher.watch(p, RecursiveMode::Recursive).is_ok())
            .collect();
        if roots.is_empty() {
            return Err("游戏目录及 AppData 均无法监测，请检查路径权限。".into());
        }
        let (finish, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = rx.recv_timeout(timeout);
            drop(watcher);
        });
        let token = uuid::Uuid::new_v4().to_string();
        let started = Started {
            token: token.clone(),
            roots: roots.iter().map(|r| r.0.clone()).collect(),
        };
        Ok((
            Self {
                token,
                finish,
                worker,
                changes,
                roots,
            },
            started,
        ))
    }
    pub fn matches(&self, token: &str) -> bool {
        self.token == token
    }
    pub fn expired(&self) -> bool {
        self.worker.is_finished()
    }
    pub fn report(self) -> Result<WatchReport, String> {
        let _ = self.finish.send(());
        self.worker.join().map_err(|_| "文件监测中断")?;
        Ok(self
            .changes
            .lock()
            .map_err(|_| "无法读取文件记录")?
            .report(&self.roots))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Instant};
    #[test]
    fn native_watcher_observes_atomic_save_and_ignores_cache_and_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let slots = root.join("Publisher/Title");
        fs::create_dir_all(slots.join("Cache")).unwrap();
        let (session, _) = Session::watch(
            vec![("%APPDATA%".into(), root.clone())],
            &root.join("manager"),
            Duration::from_secs(10),
        )
        .unwrap();
        fs::write(slots.join("Player.log"), "fixture").unwrap();
        fs::write(slots.join("Cache/slot01.es3"), "cache").unwrap();
        fs::write(slots.join("working.tmp"), "new slot").unwrap();
        fs::rename(slots.join("working.tmp"), slots.join("001.bin")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !session
            .changes
            .lock()
            .unwrap()
            .paths
            .contains(&slots.join("001.bin"))
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(15));
        }
        let report = session.report().unwrap();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].path, "%APPDATA%/Publisher/Title");
        assert_eq!(report.candidates[0].files, ["001.bin"]);
        assert!(!report.partial);
    }
}
