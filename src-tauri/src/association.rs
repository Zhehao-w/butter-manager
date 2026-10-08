use crate::domain::{Error, Result};
use std::path::Path;

/// Use the native file association without constructing a command line or invoking a shell script.
pub fn open_file(file: &Path, working: &Path) -> Result<()> {
    open_file_tracked(file, working).map(|_| ())
}
pub fn open_file_tracked(
    file: &Path,
    working: &Path,
) -> Result<Option<crate::runtime::ProcessLease>> {
    #[cfg(windows)]
    {
        let file = file.to_owned();
        let working = working.to_owned();
        // A fresh thread guarantees STA COM without changing Tauri's worker apartment.
        std::thread::spawn(move || open_windows(&file, &working))
            .join()
            .map_err(|_| Error::Validation("默认应用启动任务异常退出".into()))?
    }
    #[cfg(not(windows))]
    {
        let _ = (file, working);
        Err(Error::Validation("默认应用启动仅支持 Windows".into()))
    }
}
#[cfg(windows)]
fn open_windows(file: &Path, working: &Path) -> Result<Option<crate::runtime::ProcessLease>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NO_ASSOCIATION},
        System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        },
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
                SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    // SAFETY: this is a fresh thread; initialization/uninitialization stay on this thread.
    let hr = unsafe {
        CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    };
    if hr < 0 {
        return Err(Error::Validation(format!(
            "无法初始化默认应用启动服务：{hr:#x}"
        )));
    }
    let _com = Com;
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let file = wide(file);
    let working = wide(working);
    let verb = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpDirectory: working.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: null-terminated UTF-16 buffers and the structure remain alive through this
    // synchronous request. A returned process handle is transferred to an RAII owner below.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_NO_ASSOCIATION {
            return Err(Error::Validation(
                "没有关联此类文件的应用。请先安装对应播放器，并在 Windows 中设置默认打开方式。"
                    .into(),
            ));
        }
        return Err(Error::Validation(format!(
            "默认应用无法打开启动文件：{}",
            std::io::Error::from_raw_os_error(code as i32)
        )));
    }
    if info.hProcess.is_null() {
        return Ok(None);
    } // Existing association apps can reuse a process.
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    Ok(Some(crate::runtime::ProcessLease::from_handle(unsafe {
        OwnedHandle::from_raw_handle(info.hProcess)
    })))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn missing_file_returns_a_native_error_without_opening_an_application() {
        let directory = tempfile::tempdir().unwrap();
        assert!(open_file(
            &directory.path().join("missing-game.html"),
            directory.path()
        )
        .is_err());
    }
}
