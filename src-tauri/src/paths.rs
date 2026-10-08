use crate::domain::{Error, Result};
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub fn normalize_alias(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Validation("路径无法表示为 Unicode".into()))
}

pub fn path_key(path: &Path) -> Result<String> {
    let value = path_text(path)?;
    #[cfg(windows)]
    let value = value.to_lowercase();
    Ok(value)
}

pub fn relative_path(value: &str) -> Result<PathBuf> {
    // Treat Windows separators consistently, including core tests on other platforms.
    let value = value.replace('\\', "/");
    let path = PathBuf::from(&value);
    if value.is_empty()
        || value.contains(':')
        || value.contains('\0')
        || value.starts_with('/')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::Validation(
            "必须使用目录内的相对路径，不能包含 ..、盘符或绝对路径".into(),
        ));
    }
    Ok(path)
}

pub fn contained_file(root: &Path, relative: &str, extension: &str) -> Result<PathBuf> {
    let root = dunce::canonicalize(root)?;
    let path = dunce::canonicalize(root.join(relative_path(relative)?))?;
    if !path.starts_with(&root)
        || !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
    {
        return Err(Error::Validation(format!(
            "文件必须在指定目录内并以 .{extension} 结尾"
        )));
    }
    Ok(path)
}

/// Selected game documents stay inside the install directory. Incoming scripts/shortcuts
/// retain the existing no-script-launch boundary; ordinary documents use their association.
pub fn launch_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = dunce::canonicalize(root)?;
    let path = dunce::canonicalize(root.join(relative_path(relative)?))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(Error::Validation("启动文件必须在游戏目录内".into()));
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if [
        "bat", "cmd", "ps1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "lnk", "url", "reg",
        "msi", "com", "scr", "cpl",
    ]
    .contains(&extension.as_str())
    {
        return Err(Error::Validation(
            "请选择游戏程序或游戏文档；脚本和快捷方式暂不支持作为启动文件".into(),
        ));
    }
    Ok(path)
}

pub fn working_directory(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = dunce::canonicalize(root)?;
    let path = if relative == "." {
        root.clone()
    } else {
        dunce::canonicalize(root.join(relative_path(relative)?))?
    };
    if !path.starts_with(&root) || !path.is_dir() {
        return Err(Error::Validation("启动工作目录必须在游戏目录内".into()));
    }
    Ok(path)
}

pub fn executable_directory(executable: Option<&str>) -> Result<String> {
    let Some(executable) = executable else {
        return Ok(".".into());
    };
    let path = relative_path(executable)?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    parent.map(path_text).unwrap_or_else(|| Ok(".".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_documents_accept_unicode_and_reject_escape_directories_and_scripts() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("包装")).unwrap();
        for file in [
            "游戏 & [1].html",
            "游戏.qsp",
            "story.gam",
            "page.htm",
            "game.EXE",
            "custom.game",
            "download.bat",
            "download.CMD",
            "escape.lnk",
        ] {
            std::fs::write(temp.path().join("包装").join(file), b"fixture").unwrap();
        }
        for file in [
            "游戏 & [1].html",
            "游戏.qsp",
            "story.gam",
            "page.htm",
            "game.EXE",
            "custom.game",
        ] {
            assert!(launch_file(temp.path(), &format!("包装/{file}")).is_ok());
        }
        for file in [
            "包装/download.bat",
            "包装/download.CMD",
            "包装/escape.lnk",
            "包装",
            "missing.html",
            "../outside.html",
            "https://example.com",
        ] {
            assert!(launch_file(temp.path(), file).is_err(), "{file}");
        }
    }
    #[test]
    fn paths_reject_escape_and_accept_unicode() {
        for path in [
            "../other.exe",
            "C:\\other.exe",
            "\\\\server\\game.exe",
            "/game.exe",
            "a\\..\\b.exe",
            "a:stream.exe",
        ] {
            assert!(relative_path(path).is_err(), "{path}");
        }
        assert!(relative_path("ゲーム 新版/主程序.exe").is_ok());
        assert_eq!(normalize_alias(" ＡＢＣ　 游戏 "), "abc 游戏");
    }
    #[test]
    fn files_must_exist_and_match_extension() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("游戏.exe"), b"test").unwrap();
        assert!(contained_file(root.path(), "游戏.exe", "exe").is_ok());
        assert!(contained_file(root.path(), "missing.exe", "exe").is_err());
        assert!(contained_file(root.path(), "游戏.exe", "dll").is_err());
    }
}
