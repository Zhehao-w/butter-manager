use crate::paths::{path_text, relative_path};
use crate::scanner::{hidden_or_system, is_link};
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Dictionary {
    common_directories: Vec<String>,
    engines: Vec<EngineRule>,
}
#[derive(Deserialize)]
struct EngineRule {
    engine: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    slot_prefix: Option<String>,
    #[serde(default)]
    external: Option<String>,
    #[serde(default)]
    custom_external: Option<String>,
    #[serde(default)]
    automatic_external: Option<bool>,
    #[serde(default)]
    preferred_local: Option<String>,
}
fn dictionary() -> &'static Dictionary {
    static RULES: OnceLock<Dictionary> = OnceLock::new();
    RULES.get_or_init(|| {
        serde_json::from_str(include_str!("save_dictionary.json"))
            .expect("bundled save dictionary must be valid")
    })
}
pub(crate) fn save_name(name: &str) -> bool {
    dictionary()
        .common_directories
        .iter()
        .any(|item| item == name)
}

/// Only inspect conventional directories beside the game/launcher and in www/game.
/// Never enumerate save contents or guess an AppData company/product name.
pub fn detect(root: &Path, working_directory: &str) -> Vec<String> {
    let Ok(root) = dunce::canonicalize(root) else {
        return vec![];
    };
    let mut bases = vec![PathBuf::new()];
    if working_directory != "." {
        if let Ok(relative) = relative_path(working_directory) {
            if local_directory(&root, &relative) {
                bases.push(relative);
            }
        }
    }
    let mut found = vec![];
    for base in bases {
        let children = visible_directories(&root, &base);
        for (name, relative) in children {
            if save_name(&name) {
                found.push(relative);
            } else if ["www", "game"].contains(&name.as_str()) {
                for (name, relative) in visible_directories(&root, &relative) {
                    if save_name(&name) {
                        found.push(relative);
                    }
                }
            }
        }
    }
    let mut result = found
        .into_iter()
        .filter_map(|path| path_text(&path).ok())
        .map(|path| format!("<GAME>/{}", path.replace('\\', "/")))
        .collect::<Vec<_>>();
    result.sort();
    result.dedup();
    result
}

fn visible_directories(root: &Path, base: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(root.join(base)) else {
        return vec![];
    };
    entries
        .take(512)
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_str()?.to_ascii_lowercase();
            if !save_name(&name) && !["www", "game"].contains(&name.as_str()) {
                return None;
            }
            let relative = base.join(entry.file_name());
            local_directory(root, &relative).then_some((name, relative))
        })
        .collect()
}

fn local_directory(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            return false;
        };
        if !metadata.is_dir() || is_link(&metadata) || hidden_or_system(&metadata) {
            return false;
        }
    }
    dunce::canonicalize(current).is_ok_and(|path| path.starts_with(root))
}

pub fn valid_detected_path(root: &Path, value: &str) -> bool {
    let Some(relative) = value.strip_prefix("<GAME>/") else {
        return false;
    };
    let Ok(relative) = relative_path(relative) else {
        return false;
    };
    let Ok(root) = dunce::canonicalize(root) else {
        return false;
    };
    local_directory(&root, &relative) || safe_file(&root, &relative)
}

pub fn detect_for_engine(
    root: &Path,
    work: &str,
    engine: &str,
    executable: Option<&str>,
) -> Vec<String> {
    detect_with_environment(root, work, engine, executable, &|name| {
        std::env::var_os(name).map(PathBuf::from)
    })
}

fn safe_file(root: &Path, relative: &Path) -> bool {
    let parent = relative.parent().unwrap_or(Path::new(""));
    local_directory(root, parent)
        && fs::symlink_metadata(root.join(relative)).is_ok_and(|metadata| {
            metadata.is_file() && !is_link(&metadata) && !hidden_or_system(&metadata)
        })
}

fn small_text(root: &Path, relative: &str) -> Option<String> {
    let relative = relative_path(relative).ok()?;
    if !safe_file(root, &relative) {
        return None;
    }
    let mut text = String::new();
    fs::File::open(root.join(relative))
        .ok()?
        .take(256 * 1024 + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= 256 * 1024).then_some(text)
}

// Read a single static assignment only, never evaluate Python/JS or expand script expressions.
fn assignment<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut matches = text.lines().filter_map(|line| {
        let line = line.trim().trim_start_matches("define ").trim();
        let (left, right) = line.split_once('=')?;
        (left.trim() == key).then_some(right.trim())
    });
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(value)
}
fn literal(value: &str) -> Option<String> {
    let quote = value.chars().next()?;
    if !['\'', '"'].contains(&quote) {
        return None;
    }
    let rest = &value[1..];
    let end = rest.find(quote)?;
    let tail = rest[end + 1..].trim();
    if !tail.is_empty() && !tail.starts_with('#') && !tail.starts_with(';') {
        return None;
    }
    let result = &rest[..end];
    if result.is_empty()
        || result.len() > 240
        || result.contains(['\\', '%', ':', '*', '?', '\0', '<', '>', '|'])
    {
        return None;
    }
    Some(result.into())
}
fn identity_path(name: &str) -> Option<PathBuf> {
    if name.contains(['%', ':', '*', '?', '\0', '<', '>', '|', '"'])
        || name.chars().any(char::is_control)
    {
        return None;
    }
    let relative = relative_path(name).ok()?;
    // Common defaults and shared application/system namespaces are not game identities.
    let forbidden = [
        "game",
        "new project",
        "new unity project",
        "defaultcompany",
        "godot",
        "renpy",
        "unity",
        "microsoft",
        "windows",
        "temp",
        "cache",
    ];
    if relative.components().any(|part| {
        let name = part.as_os_str().to_string_lossy();
        name.trim() != name
            || name.ends_with('.')
            || forbidden.contains(&name.to_ascii_lowercase().as_str())
    }) {
        return None;
    }
    Some(relative)
}
fn external_rule(
    engine: &str,
    custom: bool,
    values: &[(&str, &Path)],
) -> Option<(&'static str, PathBuf)> {
    let rule = dictionary()
        .engines
        .iter()
        .find(|rule| rule.engine == engine)?;
    if rule.automatic_external == Some(false) {
        return None;
    }
    let mut template = if custom {
        rule.custom_external.as_ref()?
    } else {
        rule.external.as_ref()?
    }
    .clone();
    for (key, value) in values {
        template = template.replace(
            &format!("{{{key}}}"),
            &path_text(value).ok()?.replace('\\', "/"),
        );
    }
    if template.contains(['{', '}']) {
        return None;
    }
    for variable in ["APPDATA", "USERPROFILE", "LOCALAPPDATA"] {
        if let Some(relative) = template.strip_prefix(&format!("%{variable}%/")) {
            return Some((variable, relative_path(relative).ok()?));
        }
    }
    None
}
fn existing_external(anchor: &Path, relative: &Path) -> bool {
    let Ok(anchor) = dunce::canonicalize(anchor) else {
        return false;
    };
    let mut current = anchor.clone();
    // AppData itself is hidden. Reject links, but allow hidden ordinary data directories.
    for part in relative.components() {
        current.push(part);
        if !fs::symlink_metadata(&current).is_ok_and(|m| m.is_dir() && !is_link(&m)) {
            return false;
        }
    }
    dunce::canonicalize(current).is_ok_and(|path| path.starts_with(anchor))
}
fn detect_with_environment(
    root: &Path,
    work: &str,
    engine: &str,
    executable: Option<&str>,
    environment: &dyn Fn(&str) -> Option<PathBuf>,
) -> Vec<String> {
    let mut result = detect(root, work);
    let Ok(root) = dunce::canonicalize(root) else {
        return result;
    };
    let base = if work == "." {
        PathBuf::new()
    } else {
        relative_path(work).unwrap_or_default()
    };
    if let Some(preferred) = dictionary()
        .engines
        .iter()
        .find(|rule| rule.engine == engine)
        .and_then(|rule| rule.preferred_local.as_deref())
        .and_then(|value| relative_path(value).ok())
    {
        // The bundled game/saves directory is the authoritative copy when present.
        // Prefer the install root, then a wrapper's game directory; never add AppData duplicates.
        for relative in [preferred.clone(), base.join(&preferred)] {
            if let Ok(path) = path_text(&relative) {
                let expected = format!("<GAME>/{}", path.replace('\\', "/"));
                if let Some(local) = result
                    .iter()
                    .find(|value| value.eq_ignore_ascii_case(&expected))
                {
                    return vec![local.clone()];
                }
            }
        }
    }
    if let Some(rule) = dictionary()
        .engines
        .iter()
        .find(|rule| rule.engine == engine)
    {
        if !rule.extensions.is_empty() {
            let bases = if base.as_os_str().is_empty() {
                vec![PathBuf::new()]
            } else {
                vec![PathBuf::new(), base.clone()]
            };
            for base in bases {
                if !local_directory(&root, &base) {
                    continue;
                }
                let Ok(entries) = fs::read_dir(root.join(&base)) else {
                    continue;
                };
                for entry in entries.take(2000).flatten() {
                    let relative = base.join(entry.file_name());
                    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    let Some((stem, extension)) = name.rsplit_once('.') else {
                        continue;
                    };
                    let is_slot = rule.slot_prefix.as_ref().is_none_or(|prefix| {
                        stem.strip_prefix(prefix).is_some_and(|slot| {
                            !slot.is_empty() && slot.bytes().all(|b| b.is_ascii_digit())
                        })
                    });
                    if is_slot
                        && rule.extensions.iter().any(|ext| ext == extension)
                        && safe_file(&root, &relative)
                    {
                        if let Ok(path) = path_text(&relative) {
                            result.push(format!("<GAME>/{}", path.replace('\\', "/")));
                        }
                    }
                }
            }
        }
    }
    let config_path = |name: &str| path_text(&base.join(name)).ok();
    let mut external: Option<(&str, PathBuf)> = None;
    if engine == "Ren'Py" {
        if let Some(text) = config_path("game/options.rpy").and_then(|p| small_text(&root, &p)) {
            if !text.contains("config.savedir") {
                if let Some(name) = assignment(&text, "config.save_directory")
                    .and_then(literal)
                    .and_then(|name| identity_path(&name))
                {
                    external = external_rule(engine, false, &[("save_directory", &name)]);
                }
            }
        }
    } else if engine == "Godot" {
        if let Some(text) = config_path("project.godot").and_then(|p| small_text(&root, &p)) {
            let application = text
                .split("[application]")
                .nth(1)
                .unwrap_or("")
                .split("\n[")
                .next()
                .unwrap_or("");
            let custom = match assignment(application, "config/use_custom_user_dir") {
                None => Some(false),
                Some(value) => match value.split(['#', ';']).next().unwrap_or("").trim() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                },
            };
            if let Some(custom) = custom {
                let custom_name = assignment(application, "config/custom_user_dir_name")
                    .filter(|value| !["\"\"", "''"].contains(value));
                let key = if custom && custom_name.is_some() {
                    "config/custom_user_dir_name"
                } else {
                    "config/name"
                };
                if let Some(name) = assignment(application, key)
                    .and_then(literal)
                    .and_then(|name| identity_path(&name))
                {
                    external = external_rule(
                        engine,
                        custom,
                        &[(
                            if custom {
                                "custom_user_dir_name"
                            } else {
                                "project_name"
                            },
                            &name,
                        )],
                    );
                }
            }
        }
    } else if engine == "Unity" {
        if let Some(executable) = executable.and_then(|file| relative_path(file).ok()) {
            let stem = executable.file_stem().unwrap_or_default().to_string_lossy();
            let data = executable
                .parent()
                .unwrap_or(Path::new(""))
                .join(format!("{stem}_Data/app.info"));
            if let Some(text) = path_text(&data)
                .ok()
                .and_then(|path| small_text(&root, &path))
            {
                let lines = text
                    .trim_start_matches('\u{feff}')
                    .lines()
                    .collect::<Vec<_>>();
                if lines.len() == 2 {
                    if let (Some(company), Some(product)) =
                        (identity_path(lines[0]), identity_path(lines[1]))
                    {
                        if company.components().count() == 1 && product.components().count() == 1 {
                            external = external_rule(
                                engine,
                                false,
                                &[("company", &company), ("product", &product)],
                            );
                        }
                    }
                }
            }
        }
    }
    if let Some((variable, relative)) = external {
        if environment(variable).is_some_and(|anchor| existing_external(&anchor, &relative)) {
            if let Ok(path) = path_text(&relative) {
                result.push(format!("%{variable}%/{}", path.replace('\\', "/")));
            }
        }
    }
    result.sort();
    result.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_dictionary_identifies_save_files_and_does_not_mark_runtime_assets() {
        let temp = tempfile::tempdir().unwrap();
        for file in [
            "Save01.rxdata",
            "Save02.rvdata",
            "Save03.rvdata2",
            "Save04.lsd",
            "Actors.rxdata",
            "Map001.rvdata2",
            "file1.rpgsave",
            "file0.rmmzsave",
            "1.sav",
        ] {
            fs::write(temp.path().join(file), b"fixture").unwrap();
        }
        let no_env = &|_: &str| None;
        fs::write(temp.path().join("RPG_RT.exe"), b"fixture").unwrap();
        fs::write(temp.path().join("RPG_RT.ldb"), b"fixture").unwrap();
        fs::write(temp.path().join("RPG_RT.lmt"), b"fixture").unwrap();
        assert_eq!(
            crate::scanner::analyze_directory(temp.path())
                .unwrap()
                .engine,
            "RPG Maker (legacy)"
        );
        let paths = detect_with_environment(temp.path(), ".", "RPG Maker (legacy)", None, no_env);
        assert_eq!(
            paths,
            vec![
                "<GAME>/Save01.rxdata",
                "<GAME>/Save02.rvdata",
                "<GAME>/Save03.rvdata2",
                "<GAME>/Save04.lsd"
            ]
        );
        assert_eq!(
            detect_with_environment(temp.path(), ".", "RPG Maker MV", None, no_env),
            vec!["<GAME>/file1.rpgsave"]
        );
        assert_eq!(
            detect_with_environment(temp.path(), ".", "RPG Maker MZ", None, no_env),
            vec!["<GAME>/file0.rmmzsave"]
        );
        assert_eq!(
            detect_with_environment(temp.path(), ".", "QSP", None, no_env),
            vec!["<GAME>/1.sav"]
        );
        assert!(detect_with_environment(temp.path(), ".", "Unknown", None, no_env).is_empty());
    }

    #[test]
    fn external_dictionary_requires_static_identity_and_existing_directory_without_guessing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("游戏");
        let profile = temp.path().join("profile");
        fs::create_dir_all(root.join("game")).unwrap();
        fs::create_dir_all(profile.join("RenPy/彼女-1234")).unwrap();
        fs::create_dir_all(profile.join("Godot/app_userdata/星空")).unwrap();
        fs::create_dir_all(profile.join("Studio/星空")).unwrap();
        fs::create_dir_all(root.join("Game_Data")).unwrap();
        fs::create_dir_all(profile.join("AppData/LocalLow/星空社/魔法少女")).unwrap();
        let environment = &|_: &str| Some(profile.clone());
        fs::write(
            root.join("game/options.rpy"),
            "define config.save_directory = \"彼女-1234\"\n",
        )
        .unwrap();
        assert_eq!(
            detect_with_environment(&root, ".", "Ren'Py", None, environment),
            vec!["%APPDATA%/RenPy/彼女-1234"]
        );
        fs::create_dir_all(root.join("game/saves")).unwrap();
        fs::create_dir_all(root.join("save")).unwrap();
        assert_eq!(
            detect_with_environment(&root, ".", "Ren'Py", None, environment),
            vec!["<GAME>/game/saves"]
        );
        fs::remove_dir(root.join("game/saves")).unwrap();
        fs::remove_dir(root.join("save")).unwrap();
        fs::write(
            root.join("game/options.rpy"),
            "define config.save_directory = title + '-1234'\n",
        )
        .unwrap();
        assert!(detect_with_environment(&root, ".", "Ren'Py", None, environment).is_empty());
        fs::write(
            root.join("project.godot"),
            "[application]\nconfig/name=\"星空\"\n",
        )
        .unwrap();
        assert_eq!(
            detect_with_environment(&root, ".", "Godot", None, environment),
            vec!["%APPDATA%/Godot/app_userdata/星空"]
        );
        fs::write(root.join("project.godot"), "[application]\nconfig/name=\"星空\"\nconfig/use_custom_user_dir=true # static\nconfig/custom_user_dir_name=\"Studio/星空\"\n").unwrap();
        assert_eq!(
            detect_with_environment(&root, ".", "Godot", None, environment),
            vec!["%APPDATA%/Studio/星空"]
        );
        fs::write(root.join("Game_Data/app.info"), "星空社\n魔法少女\n").unwrap();
        assert_eq!(
            detect_with_environment(&root, ".", "Unity", Some("Game.exe"), environment),
            vec!["%USERPROFILE%/AppData/LocalLow/星空社/魔法少女"]
        );
        fs::write(
            root.join("Game_Data/app.info"),
            "DefaultCompany\nNew Unity Project\n",
        )
        .unwrap();
        assert!(
            detect_with_environment(&root, ".", "Unity", Some("Game.exe"), environment).is_empty()
        );
        assert!(identity_path("../wrong").is_none());
        assert!(identity_path("%USERPROFILE%").is_none());
        assert!(identity_path("Microsoft/Windows").is_none());
        assert!(detect_with_environment(&root, ".", "NW.js", None, environment).is_empty());
    }

    #[test]
    fn detects_existing_rpg_unity_and_renpy_folders_preserving_case_and_unicode() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("魔法少女 彼女の冒険");
        for folder in [
            "save",
            "www/Save",
            "saves",
            "SaveData",
            "game/saves",
            "包装/Save_Data",
            "包装/www/save",
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::create_dir_all(root.join("assets/saves")).unwrap();
        fs::write(root.join("SaveGames"), b"not a directory").unwrap();
        assert_eq!(
            detect(&root, "包装"),
            vec![
                "<GAME>/SaveData",
                "<GAME>/game/saves",
                "<GAME>/save",
                "<GAME>/saves",
                "<GAME>/www/Save",
                "<GAME>/包装/Save_Data",
                "<GAME>/包装/www/save"
            ]
        );
        assert!(!valid_detected_path(&root, "<GAME>/../outside"));
        assert!(valid_detected_path(&root, "<GAME>/SaveGames")); // A safe local file, but not a dictionary match.
        assert!(!valid_detected_path(&root, "<GAME>/missing"));
        assert!(!valid_detected_path(&root, "D:/outside/save"));
    }

    #[test]
    fn missing_folders_are_not_invented_and_wrapper_cannot_escape() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("game")).unwrap();
        assert!(detect(temp.path(), "../outside").is_empty());
        assert!(detect(&temp.path().join("missing"), ".").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn ignores_links_to_external_saves() {
        let temp = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(external.path(), temp.path().join("save")).unwrap();
        assert!(detect(temp.path(), ".").is_empty());
        assert!(!valid_detected_path(temp.path(), "<GAME>/save"));
    }
}
