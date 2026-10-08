//! Read-only mutation guards. A play-status label is never a process-liveness signal.
use crate::domain::{Error, Game, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::Mutex;

pub struct ProcessLease {
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
    #[cfg(not(windows))]
    child: Child,
}
impl ProcessLease {
    pub fn from_child(child: Child) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle};
            use windows_sys::Win32::{
                Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS},
                System::Threading::GetCurrentProcess,
            };
            let mut handle = std::ptr::null_mut();
            // SAFETY: Child owns a live process handle; the duplicate is independently owned.
            if unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    child.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut handle,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self {
                handle: unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) },
            })
        }
        #[cfg(not(windows))]
        {
            Ok(Self { child })
        }
    }
    #[cfg(windows)]
    pub fn from_handle(handle: std::os::windows::io::OwnedHandle) -> Self {
        Self { handle }
    }
    fn exited(&mut self) -> Result<bool> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{
                Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::WaitForSingleObject,
            };
            // SAFETY: the owned process handle remains live; timeout zero never blocks.
            match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(std::io::Error::last_os_error().into()),
            }
        }
        #[cfg(not(windows))]
        {
            Ok(self.child.try_wait()?.is_some())
        }
    }
}

#[derive(Default)]
pub struct RunningGames(Mutex<HashMap<String, Vec<ProcessLease>>>);
impl RunningGames {
    pub fn observe(&self, id: &str, process: ProcessLease) -> Result<()> {
        let mut games = self
            .0
            .lock()
            .map_err(|_| Error::Validation("运行状态不可用".into()))?;
        for processes in games.values_mut() {
            processes.retain_mut(|p| !p.exited().unwrap_or(false));
        }
        games.entry(id.to_owned()).or_default().push(process);
        Ok(())
    }
    pub fn ensure_tracked_idle(&self, game: &Game) -> Result<()> {
        let mut games = self
            .0
            .lock()
            .map_err(|_| Error::Validation("运行状态不可用".into()))?;
        if let Some(processes) = games.get_mut(&game.id) {
            let mut live = vec![];
            for mut process in processes.drain(..) {
                match process.exited() {
                    Ok(true) => {}
                    _ => live.push(process), // Unreadable state must not authorize mutation.
                }
            }
            *processes = live;
            if !processes.is_empty() {
                return Err(Error::Validation(
                    "游戏或播放器仍在运行，请先关闭后重试；不会自动结束进程".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn ensure_idle(&self, game: &Game) -> Result<()> {
        self.ensure_tracked_idle(game)?;
        let mut paths = vec![PathBuf::from(&game.install_path)];
        for save in &game.save_paths {
            paths.push(crate::deletion::resolve_save(game, save)?);
        }
        ensure_paths_idle(&paths)
    }
}

/// Check again immediately before each destructive file stage. This is a preflight,
/// not a promise that another application cannot acquire a file after the check.
pub fn ensure_paths_idle(paths: &[PathBuf]) -> Result<()> {
    check_paths(paths, true, &|| false)
}

/// Recheck before switching/recycling without opening every unchanged resource again.
pub fn ensure_processes_idle(paths: &[PathBuf]) -> Result<()> {
    ensure_processes_idle_controlled(paths, &|| false)
}
pub fn ensure_processes_idle_controlled(
    paths: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    check_paths(paths, false, cancelled)
}

fn check_paths(paths: &[PathBuf], probe_files: bool, cancelled: &dyn Fn() -> bool) -> Result<()> {
    if cancelled() {
        return Err(Error::Validation("占用检查已取消".into()));
    }
    #[cfg(windows)]
    {
        windows::check(paths, probe_files, cancelled)
    }
    #[cfg(not(windows))]
    {
        let _ = (paths, probe_files);
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::fs::OpenOptions;
    use std::os::windows::{
        ffi::OsStringExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };
    struct Process {
        image: Option<PathBuf>,
        name: String,
    }
    fn processes(cancelled: &dyn Fn() -> bool) -> Result<Vec<Process>> {
        // SAFETY: a process-only snapshot takes no process pointer arguments.
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if raw == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut values = vec![];
        let mut found = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
        while found != 0 {
            if cancelled() {
                return Err(Error::Validation("占用检查已取消".into()));
            }
            if entry.th32ProcessID != 0 {
                let count = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..count]).to_lowercase();
                let raw = unsafe {
                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID)
                };
                let image = if raw.is_null() {
                    None
                } else {
                    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
                    let mut buffer = vec![0u16; 32768];
                    let mut size = buffer.len() as u32;
                    if unsafe {
                        QueryFullProcessImageNameW(
                            handle.as_raw_handle(),
                            0,
                            buffer.as_mut_ptr(),
                            &mut size,
                        )
                    } == 0
                    {
                        None
                    } else {
                        let image =
                            PathBuf::from(std::ffi::OsString::from_wide(&buffer[..size as usize]));
                        // Resolve an executable alias once per process, not once per checked root.
                        let image = dunce::canonicalize(&image).unwrap_or(image);
                        Some(PathBuf::from(crate::paths::path_key(&image)?))
                    }
                };
                values.push(Process { image, name });
            }
            found = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
        }
        if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(values)
    }
    pub(super) fn check(
        paths: &[PathBuf],
        probe_files: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        let processes = processes(cancelled)?;
        let mut queue = vec![];
        for path in paths {
            if cancelled() {
                return Err(Error::Validation("占用检查已取消".into()));
            }
            if !path.try_exists()? {
                continue;
            }
            if crate::scanner::is_link(&std::fs::symlink_metadata(path)?) {
                return Err(Error::Validation("占用检查遇到链接路径，请手工处理".into()));
            }
            let path = dunce::canonicalize(path)?;
            let key = PathBuf::from(crate::paths::path_key(&path)?);
            for process in &processes {
                if let Some(image) = &process.image {
                    if image.starts_with(&key) {
                        return Err(Error::Validation(format!(
                            "游戏或播放器仍在运行：{}。请关闭后重试",
                            process.name
                        )));
                    }
                }
            }
            queue.push(path);
        }
        if !probe_files {
            return Ok(());
        }
        // A configured save inside the game root must not scan the same subtree twice.
        queue.sort_by_key(|path| path.components().count());
        let mut roots: Vec<(PathBuf, PathBuf)> = vec![];
        for path in queue {
            let key = PathBuf::from(crate::paths::path_key(&path)?);
            if !roots.iter().any(|(_, root)| key.starts_with(root)) {
                roots.push((path, key));
            }
        }
        let mut queue = roots.into_iter().map(|(path, _)| path).collect::<Vec<_>>();
        let mut examined = 0;
        while let Some(path) = queue.pop() {
            if cancelled() {
                return Err(Error::Validation("占用检查已取消".into()));
            }
            examined += 1;
            if examined > 1_000_000 {
                return Err(Error::Validation(
                    "占用检查超过文件数量上限，未执行文件操作".into(),
                ));
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            if crate::scanner::is_link(&metadata) {
                return Err(Error::Validation("占用检查遇到链接路径，请手工处理".into()));
            }
            if metadata.is_dir() {
                for entry in std::fs::read_dir(&path)? {
                    queue.push(entry?.path());
                }
            } else if metadata.is_file() {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if processes
                    .iter()
                    .any(|p| p.image.is_none() && p.name == name)
                {
                    return Err(Error::Validation(format!(
                        "无法确认同名游戏进程是否已退出：{name}，未执行文件操作"
                    )));
                }
                // Exclusive read detects open save/resource handles, including writers.
                // The probe is immediately dropped; it does not lock the user's game.
                OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(&path)
                    .map_err(|e| {
                        Error::Validation(format!(
                            "文件被占用或无法读取：{}：{e}。请关闭游戏或播放器后重试",
                            path.display()
                        ))
                    })?;
            } else {
                return Err(Error::Validation(
                    "占用检查遇到特殊文件，未执行文件操作".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
    use std::process::{Command, Stdio};
    #[test]
    fn detects_external_process_after_restart_and_releases_tracked_process_after_exit() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ゲーム 中文 spaces");
        std::fs::create_dir(&root).unwrap();
        let exe = root.join("fixture.exe");
        std::fs::copy(std::env::var("ComSpec").unwrap(), &exe).unwrap();
        let mut child = Command::new(&exe)
            .args(["/d", "/q", "/c", "set /p fixture="])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let tracked = RunningGames::default();
        let game = crate::maintenance::tests::fixture_game(&root);
        assert!(ensure_processes_idle(std::slice::from_ref(&root)).is_err());
        assert!(ensure_processes_idle_controlled(std::slice::from_ref(&root), &|| true).is_err());
        tracked
            .observe(&game.id, ProcessLease::from_child(child).unwrap())
            .unwrap();
        assert!(tracked.ensure_tracked_idle(&game).is_err());
        drop(input); // End only this test's process through its input; no user process is killed.
        for _ in 0..100 {
            if tracked.ensure_tracked_idle(&game).is_ok() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("fixture process did not exit");
    }
    #[test]
    fn detects_locked_save_then_allows_closed_file_without_modifying_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("存档.sav");
        std::fs::write(&file, b"save fixture").unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&file)
            .unwrap();
        assert!(ensure_paths_idle(std::slice::from_ref(&file)).is_err());
        // Imports check processes; save-copy and deletion guards still reject file locks.
        ensure_processes_idle(std::slice::from_ref(&file)).unwrap();
        drop(lock);
        ensure_paths_idle(std::slice::from_ref(&file)).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"save fixture");
    }
}
