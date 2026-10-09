//! Portable data directory validation and instance isolation.
use crate::domain::{Error, Result};
use std::{fs, io::Write, path::Path};

pub const DATABASE: &str = "library.sqlite3";

pub fn development_override(windows: bool, development: bool) -> Option<&'static str> {
    (windows && development).then_some("./data-dev")
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

fn ordinary(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if crate::scanner::is_link(&metadata)
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(invalid(format!(
            "资料路径不是普通{}：{}",
            if directory { "目录" } else { "文件" },
            path.display()
        )));
    }
    Ok(())
}

pub fn ensure_writable(data: &Path) -> Result<()> {
    fs::create_dir_all(data).map_err(|error| {
        invalid(format!(
            "无法创建数据目录 {}：{error}。请将整个程序文件夹移至可写位置，不会改用 AppData。",
            data.display()
        ))
    })?;
    ordinary(data, true)?;
    let probe = data.join(format!(".write-check-{}", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)?;
        file.write_all(b"writable")?;
        file.sync_all()?;
        drop(file);
        fs::remove_file(&probe)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&probe);
    }
    result.map_err(|error| {
        invalid(format!(
            "数据目录不可写：{}（{error}）。请将整个程序文件夹移至可写位置，不会改用 AppData。",
            data.display()
        ))
    })
}

pub fn is_empty(data: &Path) -> Result<bool> {
    ordinary(data, true)?;
    Ok(fs::read_dir(data)?.next().is_none())
}

#[cfg(windows)]
pub struct InstanceGuard {
    _handle: std::os::windows::io::OwnedHandle,
}

#[cfg(windows)]
impl InstanceGuard {
    pub fn acquire(data: &Path) -> Result<Self> {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::{
            Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
            System::Threading::CreateMutexW,
        };
        let key = data
            .to_string_lossy()
            .to_lowercase()
            .bytes()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        let name: Vec<u16> = format!("Local\\butter-manager-data-{key:016x}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: the name is terminated; no security attributes or ownership are requested.
        let raw = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let handle = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw) };
        if existed {
            return Err(invalid("这个数据目录已有管理器在运行，请先关闭另一个窗口"));
        }
        Ok(Self { _handle: handle })
    }
}

#[cfg(test)]
#[path = "data_directory_test.rs"]
mod tests;
