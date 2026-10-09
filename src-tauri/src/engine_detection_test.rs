use super::*;
use crate::scanner;

fn write(root: &Path, name: &str, data: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}
fn names(root: &Path) -> HashSet<String> {
    fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_lowercase())
        .collect()
}

#[test]
fn rgss_system_ini_encodings_and_launch_ranking_reject_escaping_or_oversized_config() {
    for encoding in ["utf8", "utf16-le", "utf16-be"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(root, "A.exe", b"other program");
        write(root, "Game.exe", b"fixture");
        write(root, "System/RGSS301.dll", b"runtime");
        write(root, "Data/Scripts.rvdata2", b"scripts");
        let ini =
            "[Game]\nLibrary=System\\RGSS301.dll\nScripts=Data\\Scripts.rvdata2\nTitle=游戏\n";
        let bytes = match encoding {
            "utf16-le" => [
                vec![0xff, 0xfe],
                ini.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            ]
            .concat(),
            "utf16-be" => [
                vec![0xfe, 0xff],
                ini.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            ]
            .concat(),
            _ => ini.as_bytes().to_vec(),
        };
        write(root, "Game.ini", bytes);
        let candidate = scanner::analyze_quick_controlled(root, &|| false, &|_| {}).unwrap();
        assert_eq!(candidate.engine, "RPG Maker (legacy)");
        assert_eq!(candidate.executables[0].relative_path, "Game.exe");
        write(
            root,
            "Game.ini",
            b"[Game]\nLibrary=../System/RGSS301.dll\nScripts=Data/Scripts.rvdata2",
        );
        assert_eq!(quick(root, &names(root)).engine(), "Unknown");
        write(root, "Game.ini", vec![b'x'; CONFIG_LIMIT + 1]);
        assert_eq!(quick(root, &names(root)).engine(), "Unknown");
        write(
            root,
            "Game.ini",
            b"[Game]\nLibrary=System/RGSS301.dll\nLibrary=other.dll\nScripts=Data/Scripts.rvdata2",
        );
        assert_eq!(quick(root, &names(root)).engine(), "Unknown");
    }
}

#[test]
fn common_rules_require_combined_features_and_conflicts_stay_unknown() {
    for (first, second, expected) in [
        ("Data.wolf", "GuruguruSMF4.dll", "WOLF RPG Editor"),
        (
            "Data/BasicData/CDatabase.dat",
            "Data/BasicData/Game.dat",
            "WOLF RPG Editor",
        ),
        ("nscript.dat", "00.ns2", "NScripter"),
        ("tyrano/tyrano.js", "data/system/Config.tjs", "TyranoScript"),
        (
            "resources/app/www/js/rmmz_core.js",
            "resources/app/www/data/System.json",
            "RPG Maker MZ",
        ),
        ("Game.rgss3a", "System/RGSS301.dll", "RPG Maker (legacy)"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(root, "Game.exe", b"fixture");
        write(root, "Config.exe", b"fixture");
        write(root, first, b"fixture");
        assert_eq!(
            scanner::analyze_quick_controlled(root, &|| false, &|_| {})
                .unwrap()
                .engine,
            "Unknown"
        );
        write(root, second, b"fixture");
        let result = scanner::analyze_quick_controlled(root, &|| false, &|_| {}).unwrap();
        assert_eq!(result.engine, expected, "{first}");
        if expected == "WOLF RPG Editor" {
            assert_eq!(result.executables[0].relative_path, "Game.exe");
        }
        write(root, "renpy/runtime.py", b"fixture");
        write(root, "game/script.rpyc", b"fixture");
        let conflict = scanner::analyze_directory(root).unwrap();
        assert_eq!(conflict.engine, "Unknown");
        assert!(conflict.warnings.iter().any(|w| w.contains("多个引擎")));
    }
}

#[test]
fn xp3_or_browser_runtime_is_not_an_engine_and_metadata_needs_resource_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "Game.exe", b"fixture");
    write(root, "data.xp3", b"XP3\r\n \n\x1a\x8bg\x01");
    write(root, "nw.dll", b"fixture");
    write(root, "package.json", b"{\"main\":\"index.html\"}");
    assert_eq!(scanner::analyze_directory(root).unwrap().engine, "Unknown");
    let mut detected = Evidence::default();
    metadata_evidence(root, &names(root), "Electron\nNW.js", &mut detected);
    assert_eq!(detected.engine(), "Unknown");
    metadata_evidence(root, &names(root), "TVP(KIRIKIRI) Z CORE", &mut detected);
    assert_eq!(detected.engine(), "KiriKiri");
    fs::remove_file(root.join("data.xp3")).unwrap();
    let mut no_resource = Evidence::default();
    metadata_evidence(root, &names(root), "TVP(KIRIKIRI) Z CORE", &mut no_resource);
    assert_eq!(no_resource.engine(), "Unknown");
    // A confirmed RPG runtime wins over an unrelated compatibility resource pack.
    write(root, "www/js/rpg_core.js", b"fixture");
    write(root, "data.xp3", b"fixture");
    assert_eq!(
        scanner::analyze_directory(root).unwrap().engine,
        "RPG Maker MV"
    );
}

#[test]
fn trusted_html_and_declared_exe_improve_recommendations_without_changing_selected_context() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        "guide.html",
        b"<!doctype html><html>How to use SugarCube</html>",
    );
    write(
        root,
        "story.html",
        b"<!doctype html><html><script id=\"script-sugarcube\"></script></html>",
    );
    let result = scanner::analyze_directory(root).unwrap();
    assert_eq!(result.status, "ready");
    assert_eq!(result.engine, "HTML");
    assert_eq!(result.executables.len(), 1);
    assert_eq!(result.executables[0].relative_path, "story.html");
    write(root, "nw.dll", b"fixture");
    write(root, "A.exe", b"fixture");
    let wrapped = scanner::analyze_directory(root).unwrap();
    assert_eq!(wrapped.executables.len(), 1);
    assert_eq!(wrapped.executables[0].relative_path, "A.exe");
    fs::remove_file(root.join("nw.dll")).unwrap();
    write(root, "A.exe", b"fixture");
    write(root, "launcher/Play.exe", b"fixture");
    write(root, "package.json", b"{\"main\":\"launcher/Play.exe\"}");
    write(root, "launcher/renpy/runtime.py", b"fixture");
    write(root, "launcher/game/script.rpyc", b"fixture");
    let inferred = scanner::analyze_directory(root).unwrap();
    assert_eq!(
        inferred.executables[0].relative_path.replace('\\', "/"),
        "launcher/Play.exe"
    );
    assert_eq!(inferred.engine, "Ren'Py");
    let selected =
        scanner::analyze_selected_controlled(root, Some("A.exe"), &|| false, &|_| {}).unwrap();
    assert_eq!(selected.engine, "Unknown");
    assert_eq!(selected.working_directory, ".");
    write(root, "package.json", b"{\"main\":\"../outside.exe\"}");
    assert!(quick(root, &names(root)).main.is_none());
}

#[test]
fn large_html_reads_bounded_tail_and_explicit_html_does_not_recommend_guides() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut html = b"<!doctype html><html><title>Story</title>".to_vec();
    html.resize(HTML_LIMIT * 4, b' ');
    html.extend(b"<script id=\"script-sugarcube\"></script></html>");
    write(root, "Story.html", html);
    let result = scanner::analyze_quick_controlled(root, &|| false, &|_| {}).unwrap();
    assert_eq!(result.engine, "HTML");
    assert_eq!(result.executables[0].relative_path, "Story.html");
    // Older SugarCube embeds a large runtime script spanning the read boundary.
    let mut html = b"<!doctype html><html><!-- SugarCube (v2.36.1) -->".to_vec();
    html.resize(HTML_LIMIT * 4, b' ');
    html.extend(b"Story.load();Object.defineProperty(window,\"SugarCube\",{});</script></html>");
    write(root, "Story.html", html);
    assert_eq!(html_engine(root, "Story.html"), Some("HTML"));
    write(
        root,
        "Story.html",
        b"<!doctype html><html><!-- SugarCube (v2.36.1) --></html>",
    );
    assert_eq!(html_engine(root, "Story.html"), None);
    fs::remove_file(root.join("Story.html")).unwrap();
    write(
        root,
        "guide.html",
        b"<!doctype html><html>SugarCube guide</html>",
    );
    assert!(scanner::analyze_directory(root)
        .unwrap()
        .executables
        .is_empty());
    let selected =
        scanner::analyze_selected_controlled(root, Some("guide.html"), &|| false, &|_| {}).unwrap();
    assert_eq!(selected.engine, "HTML");
    write(root, "guide.html", b"SugarCube text without HTML structure");
    assert_eq!(
        scanner::analyze_selected_controlled(root, Some("guide.html"), &|| false, &|_| {})
            .unwrap()
            .engine,
        "Unknown"
    );
}

#[test]
fn launch_recommendations_skip_settings_tools_and_prefer_localized_game_title() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("贝尔海姆的迷途猫 ベルヘイムの迷い猫");
    for name in [
        "keyconfig.exe",
        "settings.exe",
        "ベルヘイムの迷い猫.exe",
        "Launcher.exe",
    ] {
        write(&root, name, b"fixture");
    }
    let result = scanner::analyze_quick_controlled(&root, &|| false, &|_| {}).unwrap();
    assert_eq!(result.executables.len(), 2);
    assert_eq!(
        result.executables[0].relative_path,
        "ベルヘイムの迷い猫.exe"
    );
    assert!(!scanner::helper("ConfigurationQuest.exe"));
    assert!(!scanner::helper("KeyconfigAdventure.exe"));
    // Exclusion affects recommendations only; explicitly chosen files remain valid.
    assert!(crate::paths::launch_file(&root, "keyconfig.exe").is_ok());
}

fn asar(root: &Path, paths: &[&str], html: &[u8]) -> PathBuf {
    let mut header = serde_json::json!({"files":{}});
    for path in paths {
        let parts = path.split('/').collect::<Vec<_>>();
        let mut node = &mut header;
        for part in &parts[..parts.len() - 1] {
            let files = node["files"].as_object_mut().unwrap();
            node = files
                .entry(part.to_string())
                .or_insert_with(|| serde_json::json!({"files":{}}));
        }
        node["files"][parts[parts.len() - 1]] = serde_json::json!({"size":html.len(),"offset":"0"});
    }
    let json = serde_json::to_vec(&header).unwrap();
    let payload = (4 + json.len() + 3) & !3;
    let size = payload + 4;
    let mut bytes = Vec::new();
    for value in [4u32, size as u32, payload as u32, json.len() as u32] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(json);
    bytes.resize(8 + size, 0);
    bytes.extend(html);
    write(root, "resources/app.asar", bytes);
    root.join("resources/app.asar")
}

#[test]
fn asar_index_analysis_is_opt_in_bounded_and_keeps_conflicting_engines_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "Game.exe", b"fixture");
    for (paths, expected) in [
        (
            vec![
                "www/index.html",
                "www/data/System.json",
                "www/js/rpg_core.js",
            ],
            "RPG Maker MV",
        ),
        (
            vec![
                "www/index.html",
                "www/data/System.json",
                "www/js/rmmz_core.js",
            ],
            "RPG Maker MZ",
        ),
        (
            vec!["tyrano/tyrano.js", "data/system/Config.tjs"],
            "TyranoScript",
        ),
        (
            vec![
                "www/index.html",
                "www/data/System.json",
                "www/js/rmmz_core.js",
                "www/js/rpg_core.js",
            ],
            "Unknown",
        ),
        (vec!["js/rpg_core.js"], "Unknown"),
    ] {
        asar(root, &paths, b"fixture");
        assert_eq!(
            scanner::analyze_quick_controlled(root, &|| false, &|_| {})
                .unwrap()
                .engine,
            "Unknown"
        );
        assert_eq!(scanner::analyze_directory(root).unwrap().engine, expected);
    }
    let path = asar(
        root,
        &["index.html"],
        b"<!doctype html><html><tw-storydata format=\"SugarCube\"></tw-storydata></html>",
    );
    assert_eq!(asar_engines(&path).unwrap().first(), Some(&"Twine"));
    let mut bytes = fs::read(&path).unwrap();
    bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(asar_engines(&path).is_none());
    write(root, "resources/app.asar", b"corrupt");
    assert!(asar_engines(&path).is_none());
}

#[test]
fn embedded_godot_requires_pe_and_consistent_pack_header_and_trailer_and_honors_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut exe = vec![0u8; 256];
    exe[..2].copy_from_slice(b"MZ");
    exe[60..64].copy_from_slice(&128u32.to_le_bytes());
    exe[128..132].copy_from_slice(b"PE\0\0");
    let mut pack = b"GDPC".to_vec();
    for v in [2u32, 4, 0, 0] {
        pack.extend(v.to_le_bytes());
    }
    exe.extend(&pack);
    exe.extend((pack.len() as u64).to_le_bytes());
    exe.extend(b"GDPC");
    write(root, "Game.exe", &exe);
    assert_eq!(
        scanner::analyze_quick_controlled(root, &|| false, &|_| {})
            .unwrap()
            .engine,
        "Unknown"
    );
    assert_eq!(scanner::analyze_directory(root).unwrap().engine, "Godot");
    let mut evidence = Evidence::default();
    detailed(root, "Game.exe", &names(root), &mut evidence, &|| true);
    assert_eq!(evidence.engine(), "Unknown");
    exe[128] = 0;
    write(root, "Game.exe", exe);
    assert_eq!(scanner::analyze_directory(root).unwrap().engine, "Unknown");
}

#[cfg(windows)]
#[test]
fn junctions_in_config_or_package_paths_are_never_followed() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("game");
    let other = temp.path().join("other");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&other).unwrap();
    write(&root, "Game.exe", b"fixture");
    write(
        &root,
        "Game.ini",
        b"[Game]\nLibrary=System/RGSS301.dll\nScripts=Data/Scripts.rvdata2",
    );
    write(&root, "Data/Scripts.rvdata2", b"fixture");
    write(&other, "RGSS301.dll", b"fixture");
    let linked = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(root.join("System"))
        .arg(&other)
        .output()
        .unwrap();
    assert!(linked.status.success());
    assert_eq!(quick(&root, &names(&root)).engine(), "Unknown");
    fs::remove_dir(root.join("System")).unwrap();
    asar(
        temp.path(),
        &[
            "www/index.html",
            "www/js/rpg_core.js",
            "www/data/System.json",
        ],
        b"fixture",
    );
    let linked = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(root.join("resources"))
        .arg(temp.path().join("resources"))
        .output()
        .unwrap();
    assert!(linked.status.success());
    let mut evidence = Evidence::default();
    detailed(&root, "Game.exe", &names(&root), &mut evidence, &|| false);
    assert_eq!(evidence.engine(), "Unknown");
    fs::remove_dir(root.join("resources")).unwrap();
}
