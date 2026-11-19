//! Bounded Unity discovery for a single game's detail analysis.
use crate::{save_detection::UnityLayout, scanner::is_link};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveHint {
    pub path: String,
    pub reason: String,
    pub files: Vec<String>,
    pub can_add: bool,
}

pub(crate) fn noise_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "cache"
            | "caches"
            | "code cache"
            | "gpucache"
            | "shadercache"
            | "logs"
            | "crashes"
            | "crashpad"
            | "temp"
            | "tmp"
            | "ebwebview"
            | "unitycrashhandler"
            | "blob_storage"
            | "node_modules"
            | ".git"
    )
}

pub(crate) fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !is_link(&m))
}

pub(crate) fn save_filename(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "es3" | "nson" | "sav" | "save" | "rpgsave" | "rmmzsave" | "rvdata2"
    ) || (["slot", "save", "autosave", "quicksave"]
        .iter()
        .any(|s| stem.starts_with(s))
        && matches!(ext.as_str(), "bin" | "dat" | "json" | "xml" | ""))
}

fn prefix(path: &Path, limit: u64) -> Option<String> {
    let m = fs::symlink_metadata(path).ok()?;
    if !m.is_file() || is_link(&m) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn evidence(product: &Path, stop: &dyn Fn() -> bool) -> BTreeMap<PathBuf, Vec<String>> {
    let mut found: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut dirs = std::collections::VecDeque::from([(product.to_owned(), 0)]);
    let mut remaining = 1024;
    while let Some((dir, depth)) = dirs.pop_front() {
        if remaining == 0 || stop() {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.take(remaining.min(256)).flatten() {
            remaining -= 1;
            let path = entry.path();
            let Ok(m) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_link(&m) {
                continue;
            }
            if m.is_dir() && depth < 3 && !noise_directory(&entry.file_name().to_string_lossy()) {
                dirs.push_back((path, depth + 1));
            } else if m.is_file() && save_filename(&path) {
                let names = found.entry(dir.clone()).or_default();
                if names.len() < 4 {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    found
}

fn log_matches(log: &str, needle: &str) -> bool {
    let log = log.replace('\\', "/").to_lowercase();
    log.match_indices(needle).any(|(index, matched)| {
        let left = log[..index].chars().next_back();
        let right = log[index + matched.len()..].chars().next();
        left.is_none_or(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '=' | '('))
            && right.is_none_or(|c| c.is_whitespace() || matches!(c, '/' | '\'' | '"' | ')' | ']'))
    })
}

fn display(anchor: &Path, path: &Path) -> String {
    format!(
        "%USERPROFILE%/AppData/LocalLow/{}",
        path.strip_prefix(anchor)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    )
}

pub(crate) fn unity(
    root: &Path,
    layout: &UnityLayout,
    low: &Path,
    product: Option<&Path>,
    stop: &dyn Fn() -> bool,
) -> Vec<SaveHint> {
    let data = root
        .join(&layout.data)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    let stem = std::ffi::OsStr::new(&layout.stem);
    let distinctive_stem = crate::save_detection::identity_path(&layout.stem).is_some();
    let identity = &layout.identity;
    let mut hints = vec![];
    #[cfg(windows)]
    if let Some((company, product)) = identity {
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
        };
        let subkey = format!("Software\\{company}\\{product}");
        let wide: Vec<_> = subkey.encode_utf16().chain(Some(0)).collect();
        let mut key = std::ptr::null_mut();
        // Both pointers remain valid through the call. Query-only handle, no registry values read.
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut key,
            )
        } == 0
        {
            unsafe { RegCloseKey(key) };
            hints.push(SaveHint {
                path: format!("HKCU\\{subkey}"),
                reason: "存在 Unity PlayerPrefs 注册表项，可能保存设置或进度".into(),
                files: vec![],
                can_add: false,
            });
        }
    }
    if !plain_dir(low) || crate::deletion::plain_path(low).is_err() {
        return hints;
    }
    let mut matches = BTreeMap::new();
    if let Some(dir) = product {
        matches.insert(dir.to_path_buf(), "厂商名和产品名与 app.info 一致");
    }
    let mut exact_files = product.map(|dir| evidence(dir, stop));
    let search_logs = exact_files.as_ref().is_none_or(BTreeMap::is_empty);
    let mut budget = 384;
    if let Some(companies) = search_logs.then(|| fs::read_dir(low).ok()).flatten() {
        for company in companies.take(256).flatten() {
            if stop() || budget == 0 {
                break;
            }
            if !plain_dir(&company.path()) {
                continue;
            }
            let Ok(products) = fs::read_dir(company.path()) else {
                continue;
            };
            for entry in products.take(budget.min(128)).flatten() {
                budget -= 1;
                if stop() {
                    break;
                }
                let dir = entry.path();
                if !plain_dir(&dir)
                    || matches
                        .keys()
                        .any(|p| dir.as_os_str().eq_ignore_ascii_case(p.as_os_str()))
                {
                    continue;
                }
                if prefix(&dir.join("Player.log"), 32 * 1024)
                    .is_some_and(|s| log_matches(&s, &data))
                {
                    matches.insert(dir, "Player.log 指向当前游戏的安装目录");
                } else if distinctive_stem && entry.file_name().eq_ignore_ascii_case(stem) {
                    matches.insert(dir, "目录名与启动文件同名，请核对");
                } else if identity.as_ref().is_some_and(|(_, product)| {
                    entry
                        .file_name()
                        .eq_ignore_ascii_case(std::ffi::OsStr::new(product))
                }) {
                    matches.insert(dir, "目录与产品同名，请核对厂商");
                }
            }
        }
    }
    for (dir, reason) in matches {
        if stop() {
            break;
        }
        let path = display(low, &dir);
        let found = if Some(dir.as_path()) == product {
            exact_files.take().unwrap_or_default()
        } else {
            evidence(&dir, stop)
        };
        if found.is_empty() {
            hints.push(SaveHint {
                path,
                reason: format!("{reason}，尚未发现可辨认的存档文件"),
                files: vec![],
                can_add: false,
            });
        } else {
            for (subdir, mut files) in found {
                if subdir
                    .strip_prefix(low)
                    .is_ok_and(|p| p.to_string_lossy().contains('%'))
                {
                    continue;
                }
                files.sort();
                hints.push(SaveHint {
                    path: display(low, &subdir),
                    reason: reason.into(),
                    files,
                    can_add: true,
                });
            }
        }
    }
    hints.sort_by_cached_key(|h| {
        let save_folder = Path::new(&h.path).file_name().is_some_and(|name| {
            crate::save_detection::save_name(&name.to_string_lossy().to_ascii_lowercase())
        });
        (
            !h.can_add,
            h.reason.contains("同名"),
            !save_folder,
            h.path.clone(),
        )
    });
    hints.truncate(12);
    hints
}
