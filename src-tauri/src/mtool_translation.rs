//! External translation evidence recommends shared MTool; it is never a cleanup target.
use crate::engine_detection::{bounded_text, local};
use std::{fs, path::Path};

const NOTE_LIMIT: usize = 64 * 1024;
const ENTRY_LIMIT: usize = 256;
const NOTE_COUNT: usize = 4;

fn explicit_instructions(text: &str) -> bool {
    let lower = text.to_lowercase();
    let official = lower
        .split(|c: char| c.is_whitespace() || "\"'<>（）()，。：".contains(c))
        .any(|token| {
            token
                .strip_prefix("https://mtool.app")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
        });
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    if !official
        || [
            "无需mtool",
            "不用mtool",
            "不需要mtool",
            "donotusemtool",
            "mtoolisnotrequired",
        ]
        .iter()
        .any(|word| compact.contains(word))
    {
        return false;
    }
    let external = [
        "已有mtool",
        "已有的mtool",
        "现有mtool",
        "现有的mtool",
        "自己的mtool",
        "自备mtool",
        "不附带mtool",
        "不包含mtool",
        "未附带mtool",
        "不附带工具",
        "不包含工具",
        "existingmtool",
        "yourmtool",
        "notincludemtool",
    ]
    .iter()
    .any(|word| compact.contains(word));
    external
        && compact.contains("mtool")
        && ((compact.contains("翻译文件")
            && compact.contains("游戏")
            && ["打开", "加载", "挂载", "导入"]
                .iter()
                .any(|word| compact.contains(word)))
            || (compact.contains("translation")
                && compact.contains("game")
                && ["open", "load", "mount", "import"]
                    .iter()
                    .any(|word| compact.contains(word))))
}

pub(crate) fn detect(root: &Path, executable: &str, stop: &dyn Fn() -> bool) -> bool {
    let Some(exe) = local(root, executable)
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
    else {
        return false;
    };
    let Some(directory) = exe.parent() else {
        return false;
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    let mut notes = vec![];
    let mut dictionaries = vec![];
    for (index, entry) in entries.enumerate() {
        if stop() || index >= ENTRY_LIMIT {
            return false;
        }
        let Ok(entry) = entry else { return false };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let lower = name.to_lowercase();
        let note = lower.ends_with(".txt")
            && lower.contains("mtool")
            && (lower.contains("翻译") || lower.contains("translation"));
        let dictionary = ["翻译文件.json", "translation.json"].contains(&lower.as_str());
        if !note && !dictionary {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root).map(Path::to_path_buf) else {
            continue;
        };
        let Some(relative) = relative.to_str() else {
            continue;
        };
        let Some(path) = local(root, relative) else {
            continue;
        };
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if !metadata.is_file() || crate::scanner::hidden_or_system(&metadata) {
            continue;
        }
        if dictionary && metadata.len() > 0 {
            dictionaries.push(lower);
        } else if note && metadata.len() <= NOTE_LIMIT as u64 {
            notes.push(relative.to_owned());
        }
    }
    if dictionaries.is_empty() {
        return false;
    }
    notes.sort();
    notes.iter().take(NOTE_COUNT).any(|relative| {
        if stop() {
            return false;
        }
        bounded_text(root, relative, NOTE_LIMIT).is_some_and(|text| {
            explicit_instructions(&text)
                && dictionaries
                    .iter()
                    .any(|file| file == "翻译文件.json" || text.to_lowercase().contains(file))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "MTool挂载翻译(翻译说明).txt";
    const TEXT: &str =
        "本包不附带工具，请使用已有 MTool 打开游戏及翻译文件。\nhttps://mtool.app/?lang=chs";

    fn fixture(root: &Path) {
        fs::write(root.join("Game.exe"), b"synthetic").unwrap();
        fs::write(root.join(NOTE), TEXT).unwrap();
        // A large, deliberately invalid dictionary must not be read or JSON-parsed.
        let dictionary = fs::File::create(root.join("翻译文件.json")).unwrap();
        dictionary.set_len(512 * 1024 * 1024).unwrap();
    }

    #[test]
    fn external_translation_requires_complete_explicit_evidence_and_bounded_notes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        assert!(detect(root, "Game.exe", &|| false));
        let utf16 = [0xff, 0xfe]
            .into_iter()
            .chain(TEXT.encode_utf16().flat_map(u16::to_le_bytes))
            .collect::<Vec<_>>();
        fs::write(root.join(NOTE), utf16).unwrap();
        assert!(detect(root, "Game.exe", &|| false));
        for text in [
            "MTool 翻译文件介绍：https://mtool.app/?lang=chs",
            "请使用已有 MTool 打开游戏及翻译文件：https://mtool.app.evil.test/",
            "请使用已有 MTool 打开游戏及翻译文件：https://mtool.app@evil.test/",
            "请使用已有 MTool 打开游戏及翻译文件：https://evil.test/?next=https://mtool.app/",
            "无需 MTool。已有 MTool 用户可打开游戏及翻译文件：https://mtool.app/",
        ] {
            fs::write(root.join(NOTE), text).unwrap();
            assert!(!detect(root, "Game.exe", &|| false), "{text}");
        }
        fs::write(root.join(NOTE), TEXT).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(root.join(NOTE))
            .unwrap()
            .set_len(NOTE_LIMIT as u64 + 1)
            .unwrap();
        assert!(!detect(root, "Game.exe", &|| false));
        fs::write(root.join(NOTE), TEXT).unwrap();
        fs::rename(root.join("翻译文件.json"), root.join("settings.json")).unwrap();
        assert!(!detect(root, "Game.exe", &|| false));
        fs::rename(root.join("settings.json"), root.join("翻译文件.json")).unwrap();
        assert!(!detect(root, "Game.exe", &|| true));
        assert!(!detect(root, "../Game.exe", &|| false));
        assert_eq!(fs::read(root.join("Game.exe")).unwrap(), b"synthetic");
    }

    #[cfg(unix)]
    #[test]
    fn external_translation_never_follows_linked_notes_dictionaries_or_game_directories() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        let outside = temp.path().join("outside");
        fs::create_dir(&game).unwrap();
        fs::create_dir(&outside).unwrap();
        fixture(&game);
        for name in [NOTE, "翻译文件.json"] {
            fs::rename(game.join(name), outside.join(name)).unwrap();
            symlink(outside.join(name), game.join(name)).unwrap();
            assert!(!detect(&game, "Game.exe", &|| false));
            fs::remove_file(game.join(name)).unwrap();
            fs::rename(outside.join(name), game.join(name)).unwrap();
        }
        symlink(&game, temp.path().join("linked-game")).unwrap();
        assert!(!detect(
            &temp.path().join("linked-game"),
            "Game.exe",
            &|| false
        ));
        assert!(detect(&game, "Game.exe", &|| false));
    }
}
