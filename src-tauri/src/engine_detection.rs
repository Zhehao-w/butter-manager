//! Bounded, read-only engine evidence. Archive formats and browser runtimes are not engines.
use crate::{paths, scanner::is_link};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const CONFIG_LIMIT: usize = 64 * 1024;
const HTML_LIMIT: usize = 128 * 1024;
const INDEX_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Default)]
pub(crate) struct Evidence {
    pub engines: BTreeSet<&'static str>,
    pub main: Option<String>,
}
impl Evidence {
    pub fn engine(&self) -> String {
        if self.engines.len() == 1 {
            self.engines.first().unwrap().to_string()
        } else {
            "Unknown".into()
        }
    }
    pub fn conflicting(&self) -> bool {
        self.engines.len() > 1
    }
}

// Check every component, including the base: never follow a resource junction or config escape.
pub(crate) fn local(base: &Path, relative: &str) -> Option<PathBuf> {
    let relative = paths::relative_path(relative).ok()?;
    let mut path = base.to_path_buf();
    let metadata = fs::symlink_metadata(&path).ok()?;
    if is_link(&metadata) || !metadata.is_dir() {
        return None;
    }
    for component in relative.components() {
        path.push(component);
        if is_link(&fs::symlink_metadata(&path).ok()?) {
            return None;
        }
    }
    Some(path)
}
pub(crate) fn file(base: &Path, relative: &str) -> bool {
    local(base, relative).is_some_and(|p| p.is_file())
}
fn dir(base: &Path, relative: &str) -> bool {
    local(base, relative).is_some_and(|p| p.is_dir())
}
fn bytes(base: &Path, relative: &str, limit: usize, prefix: bool) -> Option<Vec<u8>> {
    let path = local(base, relative)?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || (!prefix && metadata.len() > limit as u64) {
        return None;
    }
    let mut result = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(if prefix { limit } else { limit + 1 } as u64)
        .read_to_end(&mut result)
        .ok()?;
    if !prefix && result.len() > limit {
        return None;
    }
    Some(result)
}
fn text(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.len().is_multiple_of(2) {
            return None;
        }
        let le = bytes[0] == 0xff;
        let words = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if le {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect::<Vec<_>>();
        String::from_utf16(&words).ok()
    } else {
        // ASCII INI keys remain usable in CP932 files; never infer engine from a localized title.
        Some(
            String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
                .into_owned(),
        )
    }
}
fn ini_value<'a>(ini: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut inside = false;
    let mut value = None;
    for line in ini.lines().map(str::trim) {
        if line.starts_with([';', '#']) {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            inside = line[1..line.len() - 1].trim().eq_ignore_ascii_case(section);
        } else if inside {
            if let Some((name, item)) = line.split_once('=') {
                if name.trim().eq_ignore_ascii_case(key) {
                    // Ambiguous duplicated keys are not a launch declaration.
                    if value.is_some() {
                        return None;
                    }
                    value = Some(item.trim().trim_matches('"'));
                }
            }
        }
    }
    value
}

pub(crate) fn quick(base: &Path, names: &HashSet<String>) -> Evidence {
    let mut result = Evidence::default();
    for (paths, engine) in [
        (["js/rmmz_core.js", "www/js/rmmz_core.js"], "RPG Maker MZ"),
        (["js/rpg_core.js", "www/js/rpg_core.js"], "RPG Maker MV"),
    ] {
        if (names.contains("js") || names.contains("www")) && paths.iter().any(|p| file(base, p)) {
            result.engines.insert(engine);
        }
    }
    if names.contains("renpy") && names.contains("game") && dir(base, "renpy") && dir(base, "game")
    {
        result.engines.insert("Ren'Py");
    }
    if file(base, "RPG_RT.ldb") && file(base, "RPG_RT.lmt") {
        result.engines.insert("RPG Maker (legacy)");
    }

    if names.contains("game.ini") {
        if let Some(ini) = bytes(base, "Game.ini", CONFIG_LIMIT, false).and_then(|b| text(&b)) {
            let library = ini_value(&ini, "Game", "Library");
            let scripts = ini_value(&ini, "Game", "Scripts");
            if let (Some(library), Some(scripts)) = (library, scripts) {
                let dll = library
                    .replace('\\', "/")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if dll.starts_with("rgss")
                    && dll.ends_with(".dll")
                    && file(base, library)
                    && file(base, scripts)
                {
                    result.engines.insert("RPG Maker (legacy)");
                }
            }
        }
    }
    // Conventional archives + matching runtime are useful even when Game.ini is absent/packed.
    for (archive, dll) in [
        ("Game.rgssad", "RGSS102E.dll"),
        ("Game.rgss2a", "RGSS202E.dll"),
        ("Game.rgss3a", "System/RGSS301.dll"),
        ("Game.rgss3a", "System/RGSS300.dll"),
    ] {
        if file(base, archive) && file(base, dll) {
            result.engines.insert("RPG Maker (legacy)");
        }
    }
    // Preserve previously supported root RGSS layouts, but never a DLL/INI alone.
    if file(base, "Game.ini")
        && names.iter().any(|n| {
            ((n.starts_with("rgss") && n.ends_with(".dll"))
                || n.ends_with(".rgssad")
                || n.ends_with(".rgss2a")
                || n.ends_with(".rgss3a"))
                && file(base, n)
        })
    {
        result.engines.insert("RPG Maker (legacy)");
    }
    if (file(base, "Data.wolf") && file(base, "GuruguruSMF4.dll"))
        || (file(base, "Data/BasicData/CDatabase.dat") && file(base, "Data/BasicData/Game.dat"))
    {
        result.engines.insert("WOLF RPG Editor");
    }
    if ["nscript.dat", "nscript.___", "nscr_sec.dat"]
        .iter()
        .any(|p| file(base, p))
        && names
            .iter()
            .any(|n| (n.ends_with(".nsa") || n.ends_with(".ns2")) && file(base, n))
    {
        result.engines.insert("NScripter");
    }
    for prefix in ["", "resources/app/"] {
        if file(base, &format!("{prefix}tyrano/tyrano.js"))
            && file(base, &format!("{prefix}data/system/Config.tjs"))
        {
            result.engines.insert("TyranoScript");
        }
        for (core, engine) in [
            ("rpg_core.js", "RPG Maker MV"),
            ("rmmz_core.js", "RPG Maker MZ"),
        ] {
            if prefix.is_empty() {
                continue;
            }
            if ["", "www/"].iter().any(|web| {
                file(base, &format!("{prefix}{web}js/{core}"))
                    && file(base, &format!("{prefix}{web}data/System.json"))
            }) {
                result.engines.insert(engine);
            }
        }
    }
    if let Some(package) = bytes(base, "package.json", CONFIG_LIMIT, false)
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    {
        if let Some(main) = package.get("main").and_then(|v| v.as_str()) {
            // Only declared EXEs can change EXE ranking; JS/HTML is not a desktop launcher.
            if main.to_ascii_lowercase().ends_with(".exe")
                && !crate::scanner::helper(main)
                && file(base, main)
            {
                result.main = Some(main.into());
            }
        }
    }
    if result.engines.contains("RPG Maker (legacy)") && file(base, "Game.exe") {
        result.main = Some("Game.exe".into());
    }
    if result.engines.contains("WOLF RPG Editor") && file(base, "Game.exe") {
        result.main = Some("Game.exe".into());
    }
    result
}

pub(crate) fn for_executable(
    base: &Path,
    names: &HashSet<String>,
    filename: &str,
    mut evidence: Evidence,
) -> Evidence {
    let stem = Path::new(filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if names.contains(&format!("{stem}_data"))
        && file(base, "UnityPlayer.dll")
        && dir(base, &format!("{stem}_Data"))
    {
        evidence.engines.insert("Unity");
    }
    if names.contains(&format!("{stem}.pck"))
        && bytes(base, &format!("{stem}.pck"), 20, true).is_some_and(|b| pck_header(&b))
    {
        evidence.engines.insert("Godot");
    }
    if (filename.to_ascii_lowercase().ends_with(".html")
        || filename.to_ascii_lowercase().ends_with(".htm"))
        && evidence.engines.is_empty()
        && bytes(base, filename, HTML_LIMIT, true)
            .is_some_and(|b| html_document(&String::from_utf8_lossy(&b)))
    {
        // An explicitly selected standalone HTML file is the Windows launch format.
        evidence.engines.insert("HTML");
    }
    evidence
}

pub(crate) fn html_engine(base: &Path, filename: &str) -> Option<&'static str> {
    let html = bytes(base, filename, HTML_LIMIT, true)?;
    let head = String::from_utf8_lossy(&html);
    if !html_document(&head) {
        return None;
    }
    if twine_runtime(&head) {
        return Some("HTML");
    }
    // Large standalone stories often place the runtime at the end. Read at most
    // another 128 KiB, rather than loading all embedded images/story content.
    let mut file = fs::File::open(local(base, filename)?).ok()?;
    let len = file.metadata().ok()?.len();
    if len <= HTML_LIMIT as u64 {
        return None;
    }
    file.seek(SeekFrom::Start(len.saturating_sub(HTML_LIMIT as u64)))
        .ok()?;
    let mut tail = Vec::new();
    file.take(HTML_LIMIT as u64).read_to_end(&mut tail).ok()?;
    let tail = String::from_utf8_lossy(&tail);
    (twine_runtime(&tail)
        || (head.contains("SugarCube (v")
            && tail.contains("Object.defineProperty(window,\"SugarCube\"")
            && tail.contains("Story.load")))
    .then_some("HTML")
}
fn html_document(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("<html") || lower.contains("<!doctype html")
}
fn twine(html: &str) -> bool {
    html_document(html) && twine_runtime(html)
}
fn twine_runtime(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    // Require a runtime/story declaration, not a word in a readme.
    (lower.contains("<tw-storydata") && lower.contains("format="))
        || lower.contains("id=\"script-sugarcube\"")
        || (lower.contains("name=\"application-name\"") && lower.contains("content=\"sugarcube\""))
}
fn pck_header(bytes: &[u8]) -> bool {
    bytes.len() >= 20
        && &bytes[..4] == b"GDPC"
        && (1..=4).contains(&u32::from_le_bytes(bytes[4..8].try_into().unwrap()))
        && (1..=4).contains(&u32::from_le_bytes(bytes[8..12].try_into().unwrap()))
}

pub(crate) fn detailed(
    base: &Path,
    filename: &str,
    names: &HashSet<String>,
    evidence: &mut Evidence,
    stop: &dyn Fn() -> bool,
) {
    if stop() {
        return;
    }
    if let Some(path) = local(base, filename).filter(|p| p.is_file()) {
        let metadata = version_strings(&path);
        if stop() {
            return;
        }
        metadata_evidence(base, names, &metadata, evidence);
        if stop() {
            return;
        }
        if embedded_pck(&path) {
            evidence.engines.insert("Godot");
        }
    }
    for archive in ["resources/app.asar", "app.asar"] {
        if stop() {
            return;
        }
        if let Some(path) = local(base, archive).filter(|p| p.is_file()) {
            if let Some(engines) = asar_engines(&path) {
                evidence.engines.extend(engines);
            }
        }
    }
}

fn metadata_evidence(
    base: &Path,
    names: &HashSet<String>,
    metadata: &str,
    evidence: &mut Evidence,
) {
    let lower = metadata.to_ascii_lowercase();
    if (lower.contains("kirikiri") || lower.contains("吉里吉里"))
        && names.iter().any(|n| n.ends_with(".xp3") && file(base, n))
    {
        evidence.engines.insert("KiriKiri");
    }
    if lower.contains("nscripter")
        && ["nscript.dat", "nscript.___", "nscr_sec.dat"]
            .iter()
            .any(|p| file(base, p))
    {
        evidence.engines.insert("NScripter");
    }
    if lower.contains("wolf rpg")
        && (file(base, "Data.wolf") || file(base, "Data/BasicData/CDatabase.dat"))
    {
        evidence.engines.insert("WOLF RPG Editor");
    }
}

// Seek to the bounded Godot trailer/header; never load an entire EXE or execute it.
fn embedded_pck(path: &Path) -> bool {
    let check = || -> Option<bool> {
        let mut file = fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        if len < 76 {
            return None;
        }
        let mut dos = [0; 64];
        file.read_exact(&mut dos).ok()?;
        if &dos[..2] != b"MZ" {
            return None;
        }
        let pe = u32::from_le_bytes(dos[60..64].try_into().ok()?) as u64;
        if pe > len.checked_sub(4)? {
            return None;
        }
        file.seek(SeekFrom::Start(pe)).ok()?;
        let mut signature = [0; 4];
        file.read_exact(&mut signature).ok()?;
        if &signature != b"PE\0\0" {
            return None;
        }
        file.seek(SeekFrom::End(-12)).ok()?;
        let mut trailer = [0; 12];
        file.read_exact(&mut trailer).ok()?;
        if &trailer[8..] != b"GDPC" {
            return None;
        }
        let size = u64::from_le_bytes(trailer[..8].try_into().ok()?);
        if size < 20 {
            return None;
        }
        let start = len.checked_sub(12)?.checked_sub(size)?;
        if start <= pe + 4 {
            return None;
        }
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut header = [0; 20];
        file.read_exact(&mut header).ok()?;
        Some(pck_header(&header))
    };
    check().unwrap_or(false)
}

fn asar_engines(path: &Path) -> Option<BTreeSet<&'static str>> {
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut prefix = [0u8; 16];
    file.read_exact(&mut prefix).ok()?;
    if u32::from_le_bytes(prefix[..4].try_into().ok()?) != 4 {
        return None;
    }
    let size = u32::from_le_bytes(prefix[4..8].try_into().ok()?) as usize;
    let payload = u32::from_le_bytes(prefix[8..12].try_into().ok()?) as usize;
    let json_len = u32::from_le_bytes(prefix[12..16].try_into().ok()?) as usize;
    if !(8..=INDEX_LIMIT).contains(&size)
        || payload != size - 4
        || json_len > size - 8
        || (size + 8) as u64 > len
    {
        return None;
    }
    let mut json = vec![0; json_len];
    file.read_exact(&mut json).ok()?;
    let header: serde_json::Value = serde_json::from_slice(&json).ok()?;
    let data_start = (size + 8) as u64;
    let entry = |relative: &str| -> Option<(u64, u64)> {
        let mut node = &header;
        for part in relative.split('/') {
            if node.get("link").is_some() {
                return None;
            }
            let files = node.get("files")?.as_object()?;
            node = files.get(part).or_else(|| {
                files
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(part))
                    .map(|(_, v)| v)
            })?;
        }
        if node.get("link").is_some()
            || node.get("unpacked").and_then(|v| v.as_bool()) == Some(true)
        {
            return None;
        }
        let offset = node.get("offset")?.as_str()?.parse::<u64>().ok()?;
        let size = node.get("size")?.as_u64()?;
        let start = data_start.checked_add(offset)?;
        (start.checked_add(size)? <= len).then_some((start, size))
    };
    let mut engines = BTreeSet::new();
    for prefix in ["", "www/"] {
        if entry(&format!("{prefix}index.html")).is_some()
            && entry(&format!("{prefix}data/System.json")).is_some()
        {
            for (core, engine) in [
                ("rpg_core.js", "RPG Maker MV"),
                ("rmmz_core.js", "RPG Maker MZ"),
            ] {
                if entry(&format!("{prefix}js/{core}")).is_some() {
                    engines.insert(engine);
                }
            }
        }
        if entry(&format!("{prefix}tyrano/tyrano.js")).is_some()
            && entry(&format!("{prefix}data/system/Config.tjs")).is_some()
        {
            engines.insert("TyranoScript");
        }
        if let Some((offset, size)) = entry(&format!("{prefix}index.html")) {
            file.seek(SeekFrom::Start(offset)).ok()?;
            let mut html = vec![0; size.min(HTML_LIMIT as u64) as usize];
            file.read_exact(&mut html).ok()?;
            if twine(&String::from_utf8_lossy(&html)) {
                engines.insert("Twine");
            }
        }
    }
    Some(engines)
}

#[cfg(windows)]
fn version_strings(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // Version APIs read resources as data; never LoadLibrary/launch an untrusted executable.
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut()) };
    if size == 0 || size as usize > CONFIG_LIMIT {
        return String::new();
    }
    let mut buffer = vec![0u8; size as usize];
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) } == 0 {
        return String::new();
    }
    let query = |key: &str| -> Option<&[u8]> {
        let key = key.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut ptr = std::ptr::null_mut();
        let mut len = 0;
        if unsafe { VerQueryValueW(buffer.as_ptr().cast(), key.as_ptr(), &mut ptr, &mut len) } == 0
            || ptr.is_null()
        {
            return None;
        }
        let offset = (ptr as usize).checked_sub(buffer.as_ptr() as usize)?;
        buffer.get(offset..offset.checked_add(len as usize)?)
    };
    let translations = query("\\VarFileInfo\\Translation").unwrap_or(&[]).to_vec();
    let mut result = String::new();
    for pair in translations.as_chunks::<4>().0.iter().take(8) {
        let language = u16::from_le_bytes([pair[0], pair[1]]);
        let codepage = u16::from_le_bytes([pair[2], pair[3]]);
        for field in ["FileDescription", "ProductName", "OriginalFilename"] {
            let key = format!("\\StringFileInfo\\{language:04x}{codepage:04x}\\{field}");
            let wide_key = key.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let mut ptr = std::ptr::null_mut();
            let mut count = 0u32;
            if unsafe {
                VerQueryValueW(
                    buffer.as_ptr().cast(),
                    wide_key.as_ptr(),
                    &mut ptr,
                    &mut count,
                )
            } == 0
                || ptr.is_null()
            {
                continue;
            }
            let Some(offset) = (ptr as usize).checked_sub(buffer.as_ptr() as usize) else {
                continue;
            };
            let Some(length) = (count as usize).checked_mul(2) else {
                continue;
            };
            let Some(end) = offset.checked_add(length) else {
                continue;
            };
            let Some(data) = buffer.get(offset..end) else {
                continue;
            };
            let words = data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            result.push_str(&String::from_utf16_lossy(&words));
            result.push('\n');
        }
    }
    result
}
#[cfg(not(windows))]
fn version_strings(_path: &Path) -> String {
    String::new()
}

#[cfg(test)]
#[path = "engine_detection_test.rs"]
mod tests;
