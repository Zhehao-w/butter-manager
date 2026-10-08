use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use windows::{
    core::{implement, Ref, HRESULT, PCWSTR},
    Win32::{
        Foundation::E_ABORT,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::*,
    },
};
type WResult = windows::core::Result<()>;

fn apartment<T>(action: impl FnOnce() -> windows::core::Result<T>) -> windows::core::Result<T> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
    let _apartment = Apartment;
    action()
}

unsafe fn display_name(item: &IShellItem, kind: SIGDN) -> windows::core::Result<String> {
    let name = item.GetDisplayName(kind)?;
    let result = name.to_string();
    CoTaskMemFree(Some(name.0.cast()));
    Ok(result?)
}

/// Shell enumeration, never raw edits to $Recycle.Bin or its metadata.
unsafe fn recycled_items() -> windows::core::Result<Vec<(PathBuf, IShellItem)>> {
    let bin: IShellItem =
        SHCreateItemFromParsingName(windows::core::w!("shell:RecycleBinFolder"), None)?;
    let entries: IEnumShellItems = bin.BindToHandler(None, &BHID_EnumItems)?;
    let mut result = vec![];
    loop {
        let mut next = [None];
        let mut fetched = 0;
        entries.Next(&mut next, Some(&mut fetched))?;
        if fetched == 0 {
            break;
        }
        if let Some(item) = next[0].take() {
            if let Ok(path) = display_name(&item, SIGDN_FILESYSPATH) {
                result.push((PathBuf::from(path), item));
            }
        }
    }
    Ok(result)
}

pub fn recycled_paths() -> crate::domain::Result<Vec<PathBuf>> {
    apartment(|| unsafe {
        Ok(recycled_items()?
            .into_iter()
            .map(|(path, _)| path)
            .collect())
    })
    .map_err(|e| crate::domain::Error::Validation(format!("无法读取回收站：{e}")))
}

pub fn restore(path: &Path, target: &Path) -> crate::domain::Result<()> {
    if target.exists() {
        return Err(crate::domain::Error::Validation(
            "回收站恢复目标已存在".into(),
        ));
    }
    apartment(|| unsafe {
        let (_, item) = recycled_items()?
            .into_iter()
            .find(|(candidate, _)| candidate == path)
            .ok_or_else(|| windows::core::Error::from(E_ABORT))?;
        let parent = target
            .parent()
            .ok_or_else(|| windows::core::Error::from(E_ABORT))?;
        let wide: Vec<u16> = parent.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
        let name: Vec<u16> = target
            .file_name()
            .ok_or_else(|| windows::core::Error::from(E_ABORT))?
            .encode_wide()
            .chain(Some(0))
            .collect();
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
        operation.SetOperationFlags(
            FOFX_EARLYFAILURE
                | FOF_NOERRORUI
                | FOF_SILENT
                | FOF_NOCONFIRMATION
                | FOF_NO_CONNECTED_ELEMENTS,
        )?;
        let sink: IFileOperationProgressSink = RecycleOnly {
            restore_target: Some(target.to_path_buf()),
        }
        .into();
        operation.MoveItem(&item, &destination, PCWSTR(name.as_ptr()), &sink)?;
        operation.PerformOperations()?;
        if operation.GetAnyOperationsAborted()?.as_bool() {
            return Err(E_ABORT.into());
        }
        Ok(())
    })
    .map_err(|e| {
        crate::domain::Error::Validation(format!("无法从回收站恢复旧版本（可能已被清空）：{e}"))
    })
}

#[implement(IFileOperationProgressSink)]
struct RecycleOnly {
    restore_target: Option<PathBuf>,
}
#[allow(non_snake_case, unused_variables)]
impl IFileOperationProgressSink_Impl for RecycleOnly_Impl {
    fn StartOperations(&self) -> WResult {
        Ok(())
    }
    fn FinishOperations(&self, hr: HRESULT) -> WResult {
        hr.ok()
    }
    fn PreDeleteItem(&self, flags: u32, item: Ref<IShellItem>) -> WResult {
        // Abort any permanent-delete fallback, including a disabled/full recycle bin.
        // https://learn.microsoft.com/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-predeleteitem
        if self.restore_target.is_some() || flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            return Err(E_ABORT.into());
        }
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        hr: HRESULT,
        new: Ref<IShellItem>,
    ) -> WResult {
        hr.ok()
    }
    fn PreRenameItem(&self, flags: u32, item: Ref<IShellItem>, name: &PCWSTR) -> WResult {
        Err(E_ABORT.into())
    }
    fn PostRenameItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        name: &PCWSTR,
        hr: HRESULT,
        new: Ref<IShellItem>,
    ) -> WResult {
        Err(E_ABORT.into())
    }
    fn PreMoveItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
    ) -> WResult {
        let target = self
            .restore_target
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_ABORT))?;
        let dest = dest
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_ABORT))?;
        let destination = unsafe { PathBuf::from(display_name(dest, SIGDN_FILESYSPATH)?) };
        let name = unsafe { name.to_string()? };
        if destination.join(name) != *target || target.exists() {
            return Err(E_ABORT.into());
        }
        Ok(())
    }
    fn PostMoveItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
        hr: HRESULT,
        new: Ref<IShellItem>,
    ) -> WResult {
        if self.restore_target.is_some() {
            hr.ok()
        } else {
            Err(E_ABORT.into())
        }
    }
    fn PreCopyItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
    ) -> WResult {
        Err(E_ABORT.into())
    }
    fn PostCopyItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
        hr: HRESULT,
        new: Ref<IShellItem>,
    ) -> WResult {
        Err(E_ABORT.into())
    }
    fn PreNewItem(&self, flags: u32, dest: Ref<IShellItem>, name: &PCWSTR) -> WResult {
        Err(E_ABORT.into())
    }
    fn PostNewItem(
        &self,
        flags: u32,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
        template: &PCWSTR,
        attrs: u32,
        hr: HRESULT,
        new: Ref<IShellItem>,
    ) -> WResult {
        Err(E_ABORT.into())
    }
    fn UpdateProgress(&self, total: u32, done: u32) -> WResult {
        Ok(())
    }
    fn ResetTimer(&self) -> WResult {
        Ok(())
    }
    fn PauseTimer(&self) -> WResult {
        Ok(())
    }
    fn ResumeTimer(&self) -> WResult {
        Ok(())
    }
}
pub fn recycle(path: &Path) -> crate::domain::Result<()> {
    fn run(path: &Path) -> WResult {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            struct Apartment;
            impl Drop for Apartment {
                fn drop(&mut self) {
                    unsafe { CoUninitialize() }
                }
            }
            let _apartment = Apartment;
            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            operation.SetOperationFlags(
                FOFX_RECYCLEONDELETE
                    | FOFX_ADDUNDORECORD
                    | FOFX_EARLYFAILURE
                    | FOF_NOERRORUI
                    | FOF_SILENT
                    | FOF_NOCONFIRMATION
                    | FOF_NO_CONNECTED_ELEMENTS,
            )?;
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
            let sink: IFileOperationProgressSink = RecycleOnly {
                restore_target: None,
            }
            .into();
            operation.DeleteItem(&item, &sink)?;
            operation.PerformOperations()?;
            if operation.GetAnyOperationsAborted()?.as_bool() {
                return Err(E_ABORT.into());
            }
            Ok(())
        }
    }
    run(path).map_err(|e| {
        crate::domain::Error::Validation(format!("无法送入回收站（不会转为永久删除）：{e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sink_vetoes_permanent_deletion_without_touching_files() {
        let sink: IFileOperationProgressSink = RecycleOnly {
            restore_target: None,
        }
        .into();
        unsafe {
            assert!(sink.PreDeleteItem(0, None).is_err());
            assert!(sink
                .PreDeleteItem(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, None)
                .is_ok());
        }
    }

    #[test]
    #[ignore = "native Shell roundtrip uses only an isolated temporary fixture"]
    fn native_unicode_recycle_and_restore_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("魔法少女 日本語 old version");
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("進行.sav"), b"fixture").unwrap();
        let id = crate::importer::identity(&original).unwrap();
        recycle(&original).unwrap();
        assert!(!original.exists());
        let recycled = recycled_paths()
            .unwrap()
            .into_iter()
            .find(|p| crate::importer::identity(p).ok().as_deref() == Some(&id))
            .unwrap();
        restore(&recycled, &original).unwrap();
        assert_eq!(
            std::fs::read(original.join("進行.sav")).unwrap(),
            b"fixture"
        );
        assert_eq!(crate::importer::identity(&original).unwrap(), id);
    }
}
