use crate::domain::{BatAnalysis, Error, ExeCandidate, Result, ScanCandidate, ScanReport};
use crate::mtool::parse_bat;
use crate::paths::{executable_directory, path_text};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant, UNIX_EPOCH};

const MAX_ENTRIES: usize = 20_000;
const MAX_BAT_BYTES: u64 = 256 * 1024;

pub fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(crate) fn helper(name: &str) -> bool {
    let name = name.to_lowercase();
    let stem = Path::new(&name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    // Exact tool stems only: a game title containing "config" is still selectable.
    [
        "keyconfig",
        "key_config",
        "config",
        "configuration",
        "settings",
        "setting",
        "option",
        "options",
        "configure",
        "configtool",
        "キー設定",
        "環境設定",
        "按键设置",
        "键位设置",
    ]
    .contains(&stem.as_ref())
        || [
            "unins",
            "uninstall",
            "setup",
            "install",
            "update",
            "patcher",
            "crash",
            "vc_redist",
            "vcredist",
            "dxsetup",
            "anticheat",
            "unitycrash",
            "inject",
            "mtool",
            "helper",
        ]
        .iter()
        .any(|s| name.contains(s))
}

pub(crate) fn excluded_name(name: &str) -> bool {
    name.starts_with('.')
        || ["$recycle.bin", "system volume information"].contains(&name.to_lowercase().as_str())
}

pub(crate) fn hidden_or_system(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & (0x2 | 0x4) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}
pub(crate) fn resource_directory(name: &str) -> bool {
    name.ends_with("_data")
        || crate::save_detection::save_name(name)
        || [
            "tool",
            "save",
            "saves",
            "node_modules",
            "redist",
            "_commonredist",
            "data",
            "www",
            "js",
            "img",
            "audio",
            "video",
            "images",
            "sounds",
            "movies",
            "assets",
            "textures",
            "content",
            "fonts",
            "locales",
            "renpy",
            "system",
            "resources",
            "tyrano",
        ]
        .contains(&name)
}

pub fn architecture(path: &Path) -> String {
    fn read_machine(path: &Path) -> std::io::Result<u16> {
        let mut file = fs::File::open(path)?;
        let mut dos = [0u8; 64];
        file.read_exact(&mut dos)?;
        if &dos[0..2] != b"MZ" {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
        file.seek(SeekFrom::Start(u64::from(offset)))?;
        let mut pe = [0u8; 6];
        file.read_exact(&mut pe)?;
        if &pe[0..4] != b"PE\0\0" {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(u16::from_le_bytes([pe[4], pe[5]]))
    }
    match read_machine(path) {
        Ok(0x14c) => "x86",
        Ok(0x8664) => "x64",
        Ok(0xaa64) => "arm64",
        _ => "Unknown",
    }
    .into()
}

fn analyze_bat(path: &Path, relative: String) -> BatAnalysis {
    let result = (|| -> std::result::Result<String, String> {
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        if metadata.len() > MAX_BAT_BYTES {
            return Err("BAT 超过 256 KiB，不自动解析".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_BAT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BAT_BYTES {
            return Err("BAT 超过读取上限".into());
        }
        if bytes.starts_with(&[0xff, 0xfe]) {
            if bytes.len() % 2 != 0 {
                return Err("损坏的 UTF-16 BAT".into());
            }
            let words: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            return String::from_utf16(&words).map_err(|e| e.to_string());
        }
        String::from_utf8(bytes)
            .map_err(|_| "BAT 非 UTF-8 / UTF-16 LE；编码不确定，需人工检查".into())
    })();
    match result {
        Ok(content) => parse_bat(&relative, &content),
        Err(reason) => BatAnalysis {
            path: relative,
            status: "unsupported".into(),
            recipe: None,
            messages: vec![reason],
        },
    }
}

pub fn pending_candidate(path: &Path) -> Result<ScanCandidate> {
    Ok(ScanCandidate {
        install_path: path_text(path)?,
        directory_modified_ms: fs::symlink_metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis()),
        suggested_title: path
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or("Unknown")
            .into(),
        engine: "Unknown".into(),
        save_paths: vec![],
        qsp: None,
        executables: vec![],
        bats: vec![],
        bundled_tool: false,
        mtool_detected: false,
        warnings: vec![],
        registered_id: None,
        suggested_version: "Unknown".into(),
        version_source: "unknown".into(),
        working_directory: ".".into(),
        status: "pending".into(),
        entries_scanned: 0,
        elapsed_ms: 0,
    })
}

pub fn discover_root(
    path: &Path,
    stop: &dyn Fn() -> bool,
    publish: &mut dyn FnMut(&Path),
) -> Result<(Vec<PathBuf>, Vec<String>)> {
    let root = dunce::canonicalize(path)?;
    let mut directories = vec![];
    let mut warnings = vec![];
    for (index, entry) in fs::read_dir(root)?.enumerate() {
        if stop() {
            break;
        }
        if index >= MAX_ENTRIES || directories.len() >= 5000 {
            warnings.push("达到目录发现上限，部分目录尚未扫描".into());
            break;
        }
        let entry = match entry {
            Ok(v) => v,
            Err(e) => {
                if warnings.len() < 20 {
                    warnings.push(e.to_string());
                }
                continue;
            }
        };
        if excluded_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(v) => v,
            Err(e) => {
                if warnings.len() < 20 {
                    warnings.push(e.to_string());
                }
                continue;
            }
        };
        if stop() {
            break;
        }
        if metadata.is_dir()
            && !is_link(&metadata)
            && !hidden_or_system(&metadata)
            && !entry.file_name().eq_ignore_ascii_case("Tool")
            && !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".butter-import-")
        {
            let path = entry.path();
            publish(&path);
            directories.push(path);
        }
    }
    Ok((directories, warnings))
}

pub fn analyze_controlled(
    path: &Path,
    deep: bool,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(&Path),
) -> Result<ScanCandidate> {
    analyze_inner(path, deep, true, None, stop, progress)
}

pub fn analyze_selected_controlled(
    path: &Path,
    executable: Option<&str>,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(&Path),
) -> Result<ScanCandidate> {
    analyze_inner(path, false, true, executable, stop, progress)
}

pub fn analyze_quick_controlled(
    path: &Path,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(&Path),
) -> Result<ScanCandidate> {
    analyze_inner(path, false, false, None, stop, progress)
}

pub fn analyze_quick_selected(
    path: &Path,
    executable: Option<&str>,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(&Path),
) -> Result<ScanCandidate> {
    analyze_inner(path, false, false, executable, stop, progress)
}

fn analyze_inner(
    path: &Path,
    deep: bool,
    inspect_files: bool,
    preferred_executable: Option<&str>,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(&Path),
) -> Result<ScanCandidate> {
    let started = Instant::now();
    if is_link(&fs::symlink_metadata(path)?) {
        return Err(Error::Validation("游戏目录是链接，请选择实际目录".into()));
    }
    let root = dunce::canonicalize(path)?;
    if !root.is_dir() {
        return Err(Error::Validation("游戏路径不是目录".into()));
    }
    let mut candidate = pending_candidate(&root)?;
    candidate.status = "ready".into();
    let max_entries = if deep { 20_000 } else { 2000 };
    let budget = if deep {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(5)
    };
    let mut signals = std::collections::HashMap::new();
    let mut qsp_files = vec![];
    let mut html_files = vec![];
    let mut stack = vec![(root.clone(), 0)];
    while let Some((directory, depth)) = stack.pop() {
        if stop() {
            candidate.status = "skipped".into();
            break;
        }
        if candidate.entries_scanned >= max_entries || started.elapsed() >= budget {
            candidate.status = "incomplete".into();
            candidate
                .warnings
                .push("已达到分析预算，可手工选择 EXE 或单独重试".into());
            break;
        }
        progress(&directory);
        if is_link(&fs::symlink_metadata(&directory)?)
            || !dunce::canonicalize(&directory)?.starts_with(&root)
        {
            candidate.status = "incomplete".into();
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(v) => v,
            Err(e) => {
                candidate.status = "incomplete".into();
                if candidate.warnings.len() < 20 {
                    candidate.warnings.push(format!("目录不可访问：{e}"));
                }
                continue;
            }
        };
        let mut children = vec![];
        let mut names = std::collections::HashSet::new();
        for entry in entries {
            if stop() {
                candidate.status = "skipped".into();
                break;
            }
            if candidate.entries_scanned >= max_entries || started.elapsed() >= budget {
                candidate.status = "incomplete".into();
                break;
            }
            candidate.entries_scanned += 1;
            let entry = match entry {
                Ok(v) => v,
                Err(_) => {
                    candidate.status = "incomplete".into();
                    continue;
                }
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if excluded_name(&name) {
                continue;
            }
            // read_dir already provides type information on Windows. Ignore resource files
            // before asking for metadata; keep the checks for directories and files we read.
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => {
                    candidate.status = "incomplete".into();
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            let generated_launcher =
                depth == 0 && file_type.is_file() && crate::mtool::generated_launcher_name(&name);
            if file_type.is_file()
                && !generated_launcher
                && ((!name.ends_with(".exe")
                    && !name.ends_with(".qsp")
                    && !name.ends_with(".html")
                    && !name.ends_with(".htm")
                    && (!name.ends_with(".bat") || !inspect_files))
                    || (name.ends_with(".exe") && helper(&name)))
            {
                names.insert(name.clone());
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(v) => v,
                Err(_) => {
                    candidate.status = "incomplete".into();
                    continue;
                }
            };
            if stop() {
                candidate.status = "skipped".into();
                break;
            }
            if is_link(&metadata) || hidden_or_system(&metadata) {
                continue;
            }
            if generated_launcher {
                candidate.mtool_detected = true;
            }
            names.insert(name.clone());
            if metadata.is_dir() {
                if name == "tool" {
                    candidate.bundled_tool = true;
                    continue;
                }
                if resource_directory(&name) {
                    continue;
                }
                if depth < if deep { 4 } else { 1 } {
                    if children.len() < 64 {
                        children.push((path, depth + 1));
                    } else {
                        candidate.status = "incomplete".into();
                    }
                }
            } else if metadata.is_file() {
                let relative = path_text(
                    path.strip_prefix(&root)
                        .map_err(|e| Error::Validation(e.to_string()))?,
                )?;
                if name.ends_with(".exe") && !helper(&name) && candidate.executables.len() < 32 {
                    candidate.executables.push(ExeCandidate {
                        relative_path: relative,
                        architecture: "Unknown".into(),
                        score: if depth == 0 { 100 } else { 70 - depth },
                        size_bytes: metadata.len(),
                        modified_ms: metadata
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis())
                            .unwrap_or(0),
                    });
                } else if name.ends_with(".qsp") {
                    if qsp_files.len() < 64 {
                        qsp_files.push(relative);
                    } else {
                        candidate.status = "incomplete".into();
                    }
                } else if (name.ends_with(".html") || name.ends_with(".htm"))
                    && html_files.len() < 8
                {
                    html_files.push(ExeCandidate {
                        relative_path: relative,
                        architecture: "Unknown".into(),
                        score: 60 - depth,
                        size_bytes: metadata.len(),
                        modified_ms: metadata
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_millis())
                            .unwrap_or(0),
                    });
                } else if inspect_files && name.ends_with(".bat") && candidate.bats.len() < 8 {
                    let file = crate::paths::contained_file(&root, &relative, "bat")?;
                    let mut bat = analyze_bat(&file, relative);
                    if let Some(recipe) = &mut bat.recipe {
                        if directory != root {
                            let prefix = directory
                                .strip_prefix(&root)
                                .map_err(|e| Error::Validation(e.to_string()))?;
                            recipe.target_exe = path_text(
                                &prefix.join(crate::paths::relative_path(&recipe.target_exe)?),
                            )?;
                        }
                    }
                    candidate.bats.push(bat);
                }
            }
        }
        if stop() {
            candidate.status = "skipped".into();
            break;
        }
        // Ren'Py's game directory is a resource tree, not a wrapper.
        if names.contains("renpy") {
            children.retain(|(p, _)| {
                !p.file_name()
                    .is_some_and(|s| s.eq_ignore_ascii_case("game"))
            });
        }
        signals.insert(directory, names);
        stack.extend(children);
    }
    let stopped = || stop() || started.elapsed() >= budget;
    let mut evidence = std::collections::HashMap::new();
    for (directory, names) in &signals {
        if stopped() {
            break;
        }
        evidence.insert(
            directory.clone(),
            crate::engine_detection::quick(directory, names),
        );
    }
    // HTML is a launch recommendation only when it contains a Twine runtime declaration.
    // Do not suggest readmes, arbitrary web pages, or Electron/NW.js internal pages.
    for html in html_files {
        if stopped() {
            break;
        }
        let path = root.join(crate::paths::relative_path(&html.relative_path)?);
        let directory = path.parent().unwrap_or(&root);
        // A packaged browser game should launch its wrapper EXE, not its internal HTML.
        if signals.get(directory).is_some_and(|names| {
            ["nw.dll", "nw.exe", "electron.exe", "resources"]
                .iter()
                .any(|name| names.contains(*name))
        }) && candidate
            .executables
            .iter()
            .any(|exe| root.join(&exe.relative_path).parent() == Some(directory))
        {
            continue;
        }
        if crate::engine_detection::html_engine(
            path.parent().unwrap_or(&root),
            path.file_name().unwrap().to_str().unwrap_or(""),
        )
        .is_some()
            && candidate.executables.len() < 32
        {
            candidate.executables.push(html);
        }
    }
    for executable in &mut candidate.executables {
        if stopped() {
            break;
        }
        let path = root.join(crate::paths::relative_path(&executable.relative_path)?);
        let directory = path.parent().unwrap_or(&root);
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if stem.chars().count() >= 4 && candidate.suggested_title.to_lowercase().contains(&stem) {
            // Localized game titles outrank unrelated launchers without reading EXE contents.
            executable.score += 20;
        }
        if let (Some(names), Some(base)) = (signals.get(directory), evidence.get(directory)) {
            let detected =
                crate::engine_detection::for_executable(directory, names, filename, base.clone());
            if detected.engines.len() == 1 {
                executable.score += 30;
            }
            if base
                .main
                .as_deref()
                .is_some_and(|main| main.eq_ignore_ascii_case(filename))
            {
                executable.score += 70;
            }
        }
        if evidence
            .get(&root)
            .and_then(|e| e.main.as_deref())
            .is_some_and(|main| {
                main.replace('\\', "/")
                    .eq_ignore_ascii_case(&executable.relative_path.replace('\\', "/"))
            })
            && directory != root
        {
            executable.score += 70;
        }
    }
    candidate.executables.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.relative_path.cmp(&b.relative_path))
    });
    for executable in &mut candidate.executables {
        if !inspect_files {
            break;
        }
        if stop() {
            candidate.status = "skipped".into();
            break;
        }
        if started.elapsed() >= budget {
            candidate.status = "incomplete".into();
            break;
        }
        match crate::paths::launch_file(&root, &executable.relative_path) {
            Ok(file)
                if executable
                    .relative_path
                    .to_ascii_lowercase()
                    .ends_with(".exe") =>
            {
                executable.architecture = architecture(&file)
            }
            Ok(_) => {}
            Err(_) => candidate.status = "incomplete".into(),
        }
    }
    let executable = candidate
        .executables
        .first()
        .map(|v| v.relative_path.as_str());
    candidate.working_directory = executable_directory(preferred_executable.or(executable))?;
    let mut engine_conflict = false;
    if let Some(exe) = preferred_executable.or(executable) {
        let file = root.join(crate::paths::relative_path(exe)?);
        let directory = file.parent().unwrap_or(&root);
        if let Some(names) = signals.get(directory) {
            let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let mut detected = crate::engine_detection::for_executable(
                directory,
                names,
                filename,
                evidence.get(directory).cloned().unwrap_or_default(),
            );
            if inspect_files
                && detected.engines.is_empty()
                && filename.to_ascii_lowercase().ends_with(".exe")
            {
                progress(&file);
                crate::engine_detection::detailed(
                    directory,
                    filename,
                    names,
                    &mut detected,
                    &stopped,
                );
            }
            if detected.conflicting() {
                engine_conflict = true;
                candidate
                    .warnings
                    .push("发现多个引擎特征，保留未识别；请核对启动文件或手动填写引擎".into());
            }
            candidate.engine = detected.engine();
        }
    }
    (candidate.suggested_version, candidate.version_source) =
        crate::version::suggest(&candidate.suggested_title, executable);
    if !qsp_files.is_empty() {
        (candidate.suggested_version, candidate.version_source) = crate::version::suggest(
            &candidate.suggested_title,
            if qsp_files.len() == 1 {
                qsp_files.first().map(String::as_str)
            } else {
                None
            },
        );
        // Ranking is a recommendation only. Multiple game files are never auto-selected.
        qsp_files.sort_by_key(|file| {
            let path = Path::new(file);
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            (
                path.components().count(),
                !["game", "start"].contains(&stem.as_str()),
                file.clone(),
            )
        });
        let mut players = candidate.executables.iter().collect::<Vec<_>>();
        players.sort_by_key(|exe| {
            (
                std::cmp::Reverse(qsp_player_rank(&exe.relative_path)),
                std::cmp::Reverse(exe.score),
                exe.relative_path.clone(),
            )
        });
        let recognized = players
            .iter()
            .filter(|exe| qsp_player_rank(&exe.relative_path) > 0)
            .collect::<Vec<_>>();
        let recommended_player = if candidate.status == "ready" && recognized.len() == 1 {
            Some(recognized[0].relative_path.clone())
        } else {
            None
        };
        candidate.qsp = Some(crate::domain::QspDetection {
            game_files: qsp_files,
            players: players
                .iter()
                .map(|exe| exe.relative_path.clone())
                .collect(),
            recommended_player,
        });
        if candidate.engine != "Unknown" && candidate.engine != "QSP" {
            candidate
                .warnings
                .push("QSP 文件与其他引擎特征冲突，保留未识别".into());
            candidate.engine = "Unknown".into();
        } else if !engine_conflict {
            candidate.engine = "QSP".into();
        }
        candidate.working_directory = ".".into();
    }
    if !stop() {
        candidate.save_paths = crate::save_detection::detect_for_engine(
            &root,
            &candidate.working_directory,
            &candidate.engine,
            executable,
        );
    }
    candidate.elapsed_ms = started.elapsed().as_millis() as u64;
    if started.elapsed() >= budget && candidate.status == "ready" {
        candidate.status = "incomplete".into();
        candidate
            .warnings
            .push("已达到分析预算，保留已有结果；可单独重试或手动配置".into());
    }
    if stop() {
        candidate.status = "skipped".into();
    }
    Ok(candidate)
}

fn qsp_player_rank(file: &str) -> i32 {
    let name = Path::new(file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if name.contains("qsp") || name.contains("questnavigator") || name.contains("questplayer") {
        100
    } else if name.contains("player") || name.contains("播放器") || name.contains("プレイヤー")
    {
        50
    } else {
        0
    }
}

pub fn analyze_directory(path: &Path) -> Result<ScanCandidate> {
    analyze_controlled(path, false, &|| false, &|_| {})
}

pub fn scan_root(path: &Path) -> Result<ScanReport> {
    let (directories, warnings) = discover_root(path, &|| false, &mut |_| {})?;
    let candidates = directories
        .iter()
        .map(|path| analyze_quick_controlled(path, &|| false, &|_| {}))
        .collect::<Result<Vec<_>>>()?;
    Ok(ScanReport {
        candidates,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qsp_detection_recommends_single_file_and_local_player_without_reading_or_running_it() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("彼女の冒険.qsp"), b"game").unwrap();
        fs::write(temp.path().join("qspgui.exe"), b"not executable").unwrap();
        fs::write(temp.path().join("start with tool.bat"), b"never run").unwrap();
        let candidate = analyze_quick_controlled(temp.path(), &|| false, &|_| {}).unwrap();
        assert_eq!(candidate.engine, "QSP");
        assert_eq!(candidate.working_directory, ".");
        assert_eq!(
            candidate
                .qsp
                .as_ref()
                .unwrap()
                .recommended_player
                .as_deref(),
            Some("qspgui.exe")
        );
        assert_eq!(
            crate::external_player::qsp_config(&candidate)
                .unwrap()
                .game_file
                .as_deref(),
            Some("彼女の冒険.qsp")
        );
    }
    #[test]
    fn multiple_qsp_files_and_players_only_return_ranked_candidates() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("standalone_content")).unwrap();
        for file in [
            "game.qsp",
            "mod.qsp",
            "standalone_content/test.qsp",
            "QuestNavigator.exe",
            "customQspPlayer.exe",
            "other.exe",
        ] {
            fs::write(temp.path().join(file), b"fixture").unwrap();
        }
        let candidate = analyze_directory(temp.path()).unwrap();
        let qsp = candidate.qsp.as_ref().unwrap();
        assert_eq!(qsp.game_files.len(), 3);
        assert_eq!(qsp.players.len(), 3);
        assert!(qsp.recommended_player.is_none());
        assert!(crate::external_player::qsp_config(&candidate)
            .unwrap()
            .game_file
            .is_none());
    }
    #[test]
    fn unknown_unicode_executable_is_selectable_but_not_guessed_as_player() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("game.qsp"), b"fixture").unwrap();
        fs::write(temp.path().join("独自の実行程序.exe"), b"fixture").unwrap();
        let candidate = analyze_directory(temp.path()).unwrap();
        let qsp = candidate.qsp.unwrap();
        assert_eq!(qsp.players, vec!["独自の実行程序.exe"]);
        assert!(qsp.recommended_player.is_none());
    }
    #[test]
    fn qsp_version_comes_from_game_not_player_runtime() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("game-v1.2.qsp"), b"fixture").unwrap();
        fs::write(temp.path().join("qspgui-v9.9.exe"), b"fixture").unwrap();
        let candidate = analyze_directory(temp.path()).unwrap();
        assert_eq!(candidate.engine, "QSP");
        assert_eq!(candidate.suggested_version, "v1.2");
        assert_eq!(candidate.version_source, "file_name");
    }
    #[test]
    fn qsp_root_with_mods_and_wrapped_qqsp_player_stays_ambiguous() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("mod")).unwrap();
        fs::create_dir_all(temp.path().join("Qqsp-1.9.0-win64")).unwrap();
        for file in [
            "Girls Life 0.9.5.QSP",
            "mod/addedflavour.qsp",
            "Qqsp-1.9.0-win64/Qqsp.exe",
            "Qqsp-1.9.0-win64/QtWebEngineProcess.exe",
        ] {
            fs::write(temp.path().join(file), b"fixture").unwrap();
        }
        let candidate = analyze_quick_controlled(temp.path(), &|| false, &|_| {}).unwrap();
        assert_eq!(candidate.engine, "QSP");
        let qsp = candidate.qsp.as_ref().unwrap();
        assert_eq!(qsp.game_files.len(), 2);
        assert_eq!(
            qsp.recommended_player
                .as_ref()
                .map(|path| path.replace('\\', "/")),
            Some("Qqsp-1.9.0-win64/Qqsp.exe".into())
        );
        assert!(crate::external_player::qsp_config(&candidate)
            .unwrap()
            .game_file
            .is_none());
    }
    #[test]
    fn quick_scan_detects_generated_root_bat_names_without_reading_scripts_or_pe() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("wrapper")).unwrap();
        std::fs::write(temp.path().join("wrapper/arbitrary.exe"), b"not-a-pe").unwrap();
        std::fs::write(
            temp.path().join("wrapper/与工具一同启动.bat"),
            b"never-executed",
        )
        .unwrap();
        let nested = analyze_quick_controlled(temp.path(), &|| false, &|_| {}).unwrap();
        assert!(!nested.mtool_detected);
        std::fs::write(
            temp.path().join("Start With Tool.bat"),
            b"unread-unsupported-script",
        )
        .unwrap();
        let detected = analyze_quick_controlled(temp.path(), &|| false, &|_| {}).unwrap();
        assert!(detected.mtool_detected);
        assert!(detected.bats.is_empty());
        assert_eq!(detected.executables[0].architecture, "Unknown");
        assert_eq!(
            detected.executables[0].relative_path.replace('\\', "/"),
            "wrapper/arbitrary.exe"
        );
    }
    #[test]
    fn folder_modified_time_is_independent_of_executable_time_and_old_snapshots_deserialize() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Game.exe");
        fs::write(&game, b"fixture").unwrap();
        fs::File::options()
            .write(true)
            .open(&game)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(10)))
            .unwrap();
        let candidate = analyze_directory(temp.path()).unwrap();
        assert!(candidate.directory_modified_ms.unwrap() > 10_000);
        assert_eq!(candidate.executables[0].modified_ms, 10_000);
        let mut old = serde_json::to_value(&candidate).unwrap();
        old.as_object_mut().unwrap().remove("directory_modified_ms");
        let decoded: ScanCandidate = serde_json::from_value(old).unwrap();
        assert!(decoded.directory_modified_ms.is_none());
        assert_eq!(decoded.install_path, candidate.install_path);
    }
    #[test]
    fn engine_signatures_follow_executable_context_and_reject_weak_or_conflicting_markers() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Game.exe"), b"fixture").unwrap();
        fs::create_dir(temp.path().join("random_Data")).unwrap();
        fs::write(temp.path().join("unrelated.pck"), b"fixture").unwrap();
        fs::create_dir_all(temp.path().join("other/www/js")).unwrap();
        fs::write(temp.path().join("other/www/js/rpg_core.js"), b"").unwrap();
        fs::write(temp.path().join("other/Launcher.exe"), b"fixture").unwrap();
        assert_eq!(
            analyze_quick_selected(temp.path(), Some("other/Launcher.exe"), &|| false, &|_| {})
                .unwrap()
                .engine,
            "RPG Maker MV"
        );
        assert_eq!(analyze_directory(temp.path()).unwrap().engine, "Unknown");
        fs::create_dir(temp.path().join("Game_Data")).unwrap();
        fs::write(temp.path().join("UnityPlayer.dll"), b"fixture").unwrap();
        assert_eq!(
            analyze_quick_controlled(temp.path(), &|| false, &|_| {})
                .unwrap()
                .engine,
            "Unity"
        );
        fs::create_dir(temp.path().join("renpy")).unwrap();
        fs::create_dir(temp.path().join("game")).unwrap();
        assert_eq!(analyze_directory(temp.path()).unwrap().engine, "Unknown");
        let renpy = temp.path().join("renpy-fixture");
        fs::create_dir_all(renpy.join("renpy")).unwrap();
        fs::create_dir(renpy.join("game")).unwrap();
        fs::write(renpy.join("Start.exe"), b"fixture").unwrap();
        assert_eq!(analyze_directory(&renpy).unwrap().engine, "Ren'Py");
        let godot = temp.path().join("godot-fixture");
        fs::create_dir(&godot).unwrap();
        fs::write(godot.join("start.exe"), b"fixture").unwrap();
        fs::write(
            godot.join("start.pck"),
            [
                b"GDPC".as_slice(),
                &2u32.to_le_bytes(),
                &4u32.to_le_bytes(),
                &[0u8; 8],
            ]
            .concat(),
        )
        .unwrap();
        assert_eq!(analyze_directory(&godot).unwrap().engine, "Godot");
    }
    #[test]
    fn wrapper_layout_avoids_assets_and_deep_scan_is_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("游戏 v1.2.3");
        fs::create_dir_all(game.join("包装/www/js")).unwrap();
        fs::create_dir_all(game.join("包装/www/img")).unwrap();
        fs::create_dir_all(game.join("extra/bin")).unwrap();
        fs::write(game.join("包装/启动.exe"), b"fixture").unwrap();
        fs::write(game.join("包装/www/js/rmmz_core.js"), b"").unwrap();
        fs::write(game.join("extra/bin/深层.exe"), b"fixture").unwrap();
        for i in 0..1000 {
            fs::write(game.join(format!("包装/www/img/{i}.png")), b"asset").unwrap();
        }
        let shallow = analyze_directory(&game).unwrap();
        assert_eq!(shallow.executables.len(), 1);
        assert_eq!(shallow.working_directory, "包装");
        assert_eq!(shallow.suggested_version, "v1.2.3");
        assert_eq!(shallow.engine, "RPG Maker MZ");
        assert!(shallow.entries_scanned < 15);
        let deep = analyze_controlled(&game, true, &|| false, &|_| {}).unwrap();
        assert_eq!(deep.executables.len(), 2);
        assert!(deep.entries_scanned < 20);
        assert!(game.join("包装/www/img/999.png").is_file());
    }
    #[test]
    fn cancellation_during_directory_visit_returns_partial_state() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("game.exe"), b"fixture").unwrap();
        let cancelled = std::cell::Cell::new(false);
        let result = analyze_controlled(temp.path(), false, &|| cancelled.get(), &|_| {
            cancelled.set(true)
        })
        .unwrap();
        assert_eq!(result.status, "skipped");
        assert!(result.executables.is_empty());
        assert_eq!(result.entries_scanned, 0);
    }
    #[test]
    fn child_bat_target_is_rebased_to_registered_directory() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("包装")).unwrap();
        let bat = "@echo off\ncd /d \"%~dp0\"\n\"E:\\Butter\\Tool\\loaders\\inject.exe\" \"%~dp0任意.exe\" \"E:\\Butter\\Tool\\loaders\\mzHook.dll\"\nstart \"\" \"E:\\Butter\\Tool\\MTool.exe\" \"E:\\Butter\\Tool\"";
        fs::write(temp.path().join("包装/launch.bat"), bat).unwrap();
        let quick = analyze_quick_controlled(temp.path(), &|| false, &|_| {}).unwrap();
        assert!(quick.bats.is_empty());
        let result = analyze_directory(temp.path()).unwrap();
        assert_eq!(
            crate::paths::relative_path(&result.bats[0].recipe.as_ref().unwrap().target_exe)
                .unwrap(),
            PathBuf::from("包装").join("任意.exe")
        );
    }
    #[test]
    fn scans_only_child_games_excludes_helpers_and_preserves_files() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("游戏 1.02");
        fs::create_dir_all(game.join("Tool")).unwrap();
        fs::write(game.join("任意名.exe"), b"not a PE").unwrap();
        fs::write(game.join("uninstall.exe"), b"").unwrap();
        fs::write(game.join("Tool/MTool.exe"), b"").unwrap();
        fs::write(root.path().join("Ignored.exe"), b"").unwrap();
        let report = scan_root(root.path()).unwrap();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].executables.len(), 1);
        assert!(report.candidates[0].bundled_tool);
        assert!(game.join("Tool/MTool.exe").is_file());
    }
    #[test]
    fn detects_pe_architecture_without_inferring_loader() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("game.exe");
        for (machine, expected) in [(0x14cu16, "x86"), (0x8664, "x64"), (0xaa64, "arm64")] {
            let mut bytes = vec![0; 128];
            bytes[..2].copy_from_slice(b"MZ");
            bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
            bytes[64..68].copy_from_slice(b"PE\0\0");
            bytes[68..70].copy_from_slice(&machine.to_le_bytes());
            fs::write(&exe, bytes).unwrap();
            assert_eq!(architecture(&exe), expected);
        }
    }
    #[test]
    fn rejects_non_utf8_bat_and_reads_utf16() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("launch.bat");
        fs::write(&path, [0x81, 0xff]).unwrap();
        assert_eq!(
            analyze_bat(&path, "launch.bat".into()).status,
            "unsupported"
        );
        let mut bytes = vec![0xff, 0xfe];
        for word in "powershell unsafe".encode_utf16() {
            bytes.extend(word.to_le_bytes());
        }
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            analyze_bat(&path, "launch.bat".into()).status,
            "unsupported"
        );
    }
}
