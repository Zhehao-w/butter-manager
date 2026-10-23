//! External translation evidence recommends shared MTool; it is never a cleanup target.
use crate::engine_detection::{bounded_prefix, bounded_text, local};
use serde::de::{Deserializer, Error, MapAccess, Visitor};
use std::{fmt, fs, path::Path};

const READ_LIMIT: usize = 64 * 1024;
const ENTRY_LIMIT: usize = 256;
const NOTE_COUNT: usize = 4;
const DICTIONARY_COUNT: usize = 4;
const MIN_PAIRS: usize = 4;
const SAMPLE_PAIRS: usize = 32;

fn translation_name(name: &str) -> bool {
    ["翻译", "翻譯", "translation", "translate"]
        .iter()
        .any(|word| name.contains(word))
}

fn negative_actions(lower: &str) -> bool {
    // Keep polarity local to the tool and its action; "not included" and warnings
    // about directly launching the game are valid external-MTool instructions.
    lower
        .split(['\n', '\r', '。', '.', '!', '?', '！', '？', ';', '；'])
        .any(|sentence| {
            let compact: String = sentence.chars().filter(|c| !c.is_whitespace()).collect();
            compact.match_indices("mtool").any(|(index, _)| {
                let before = &compact[..index];
                let forbidden_use = [
                    "",
                    "公共",
                    "共享",
                    "已有",
                    "已有的",
                    "现有",
                    "現有",
                    "the",
                    "shared",
                    "theshared",
                    "existing",
                    "theexisting",
                ]
                .iter()
                .filter_map(|modifier| before.strip_suffix(modifier))
                .any(|before| {
                    [
                        "勿用",
                        "勿使用",
                        "勿直接使用",
                        "不要用",
                        "不要使用",
                        "不要直接使用",
                        "禁止用",
                        "禁止使用",
                        "不得使用",
                        "不能用",
                        "不能使用",
                        "无法用",
                        "无法使用",
                        "無法使用",
                        "不建议用",
                        "不建议使用",
                        "不建議使用",
                        "无需",
                        "不需要",
                        "不用",
                        "donotuse",
                        "don'tuse",
                        "don’tuse",
                        "cannotuse",
                        "can'tuse",
                        "can’tuse",
                        "mustnotuse",
                        "shouldnotuse",
                        "shouldn'tuse",
                        "shouldn’tuse",
                        "avoidusing",
                        "donotrun",
                        "don'trun",
                        "don’trun",
                        "donotrecommendusing",
                    ]
                    .iter()
                    .any(|negative| before.ends_with(negative))
                });
                let after =
                    compact[index + "mtool".len()..].trim_start_matches([':', '：', '-', '—']);
                let after = ["目前", "当前", "暂时", "暫時", "currently"]
                    .iter()
                    .find_map(|qualifier| after.strip_prefix(qualifier))
                    .unwrap_or(after);
                let unavailable = [
                    "不兼容",
                    "不支持",
                    "不可用",
                    "isnotcompatible",
                    "isnotsupported",
                ]
                .iter()
                .any(|negative| after.starts_with(negative));
                let forbidden_action = [
                    "不能",
                    "无法",
                    "無法",
                    "不可",
                    "不应",
                    "不應",
                    "cannot",
                    "can't",
                    "can’t",
                    "doesnot",
                    "doesn't",
                    "doesn’t",
                    "willnot",
                    "won't",
                    "won’t",
                    "isunableto",
                    "isnotableto",
                    "shouldnot",
                    "shouldn't",
                    "shouldn’t",
                ]
                .iter()
                .filter_map(|negative| after.strip_prefix(negative))
                .any(|action| {
                    let action: String = action.chars().take(64).collect();
                    [
                        "打开", "打開", "加载", "載入", "挂载", "掛載", "导入", "匯入", "启动",
                        "啟動", "运行", "執行", "使用", "注入", "open", "load", "mount", "import",
                        "launch", "run", "execute", "use", "work", "inject", "support",
                    ]
                    .iter()
                    .any(|verb| action.contains(verb))
                });
                forbidden_use || unavailable || forbidden_action
            })
        })
}

fn explicit_instructions(text: &str) -> bool {
    let lower = text.to_lowercase();
    // The official link is optional. A lookalike URL must not masquerade as that evidence.
    let misleading_link = lower
        .split(|c: char| c.is_whitespace() || "\"'<>（）()，。：".contains(c))
        .filter(|token| token.starts_with("https://") || token.starts_with("http://"))
        .any(|token| {
            token.contains("mtool.app")
                && !token
                    .strip_prefix("https://mtool.app")
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
        });
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    if misleading_link
        || negative_actions(&lower)
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
    compact.contains("mtool")
        && ((["翻译", "翻譯"].iter().any(|word| compact.contains(word))
            && ["游戏", "遊戲"].iter().any(|word| compact.contains(word))
            && [
                "打开", "打開", "加载", "載入", "挂载", "掛載", "导入", "匯入",
            ]
            .iter()
            .any(|word| compact.contains(word)))
            || (compact.contains("translation")
                && compact.contains("game")
                && ["open", "load", "mount", "import"]
                    .iter()
                    .any(|word| compact.contains(word))))
}

fn mentions_file(text: &str, filename: &str) -> bool {
    text.match_indices(filename).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + filename.len()..].chars().next();
        // Only sibling filenames, never a substring of another name or an escaped path.
        !before.is_some_and(|c| c.is_ascii_alphanumeric() || "_-.\\/:".contains(c))
            && !after.is_some_and(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
    })
}

#[derive(Default)]
struct DictionarySample {
    pairs: usize,
    reached_limit: bool,
}
impl<'de> Visitor<'de> for &mut DictionarySample {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a flat original-to-translation string dictionary")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while map.next_entry::<String, String>()?.is_some() {
            self.pairs += 1;
            if self.pairs == SAMPLE_PAIRS {
                // Stop intentionally: deserialize_map otherwise consumes the complete object.
                self.reached_limit = true;
                return Err(A::Error::custom("translation sample complete"));
            }
        }
        Ok(())
    }
}

fn dictionary_prefix(bytes: &[u8]) -> bool {
    let prefix_limited = bytes.len() == READ_LIMIT;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let mut sample = DictionarySample::default();
    let result = (&mut parser).deserialize_map(&mut sample);
    sample.pairs >= MIN_PAIRS
        && (sample.reached_limit
            || match result {
                Ok(()) => parser.end().is_ok(),
                Err(error) => prefix_limited && error.is_eof(),
            })
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
        let hint = translation_name(&lower);
        let mtool = lower.contains("mtool");
        let note = lower.ends_with(".txt")
            && (mtool || hint || lower.contains("readme") || lower.contains("说明"));
        let dictionary = lower.ends_with(".json");
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
            dictionaries.push((!hint, lower, relative.to_owned()));
        } else if note && metadata.len() <= READ_LIMIT as u64 {
            notes.push((!mtool, !hint, relative.to_owned()));
        }
    }
    notes.sort();
    let instructions = notes
        .iter()
        .take(NOTE_COUNT)
        .filter_map(|(_, _, relative)| {
            if stop() {
                return None;
            }
            bounded_text(root, relative, READ_LIMIT)
                .filter(|text| explicit_instructions(text))
                .map(|text| text.to_lowercase())
        })
        .collect::<Vec<_>>();
    if instructions.is_empty() || stop() {
        return false;
    }
    dictionaries.sort();
    dictionaries
        .iter()
        .filter(|(unnamed, name, _)| {
            !unnamed || instructions.iter().any(|text| mentions_file(text, name))
        })
        .take(DICTIONARY_COUNT)
        .any(|(_, _, relative)| {
            !stop()
                && bounded_prefix(root, relative, READ_LIMIT)
                    .is_some_and(|bytes| dictionary_prefix(&bytes))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "MTool挂载翻译(翻译说明).txt";
    const TEXT: &str = "本包不附带工具，请使用已有 MTool 打开游戏及翻译文件。";
    const DICTIONARY: &str =
        r#"{"New Game":"开始游戏","Continue":"继续","EV001":"EV001","10":"10"}"#;

    fn fixture(root: &Path) {
        fs::write(root.join("Game.exe"), b"synthetic").unwrap();
        fs::write(root.join(NOTE), TEXT).unwrap();
        fs::write(root.join("翻译文件.json"), DICTIONARY).unwrap();
    }

    #[test]
    fn external_translation_requires_explicit_instructions_and_a_named_or_referenced_dictionary() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        for (note, dictionary, text) in [
            (NOTE, "翻译文件.json", TEXT),
            (
                "README.txt",
                "Game_translation_v2.JSON",
                "Use MTool to open the game and load its translation.",
            ),
            (
                "使用说明.txt",
                "中文翻譯_v3.json",
                "請用 MTool 打開遊戲並載入翻譯。",
            ),
            ("MTool.txt", "Game_TRANSLATE.json", TEXT),
            (
                "翻译说明.txt",
                "Custom_Patch.json",
                "用 MTool 打开游戏，加载翻译文件 Custom_Patch.json。",
            ),
        ] {
            if note != NOTE {
                fs::rename(root.join(NOTE), root.join(note)).unwrap();
            }
            if dictionary != "翻译文件.json" {
                fs::rename(root.join("翻译文件.json"), root.join(dictionary)).unwrap();
            }
            fs::write(root.join(note), text).unwrap();
            assert!(detect(root, "Game.exe", &|| false), "{note}, {dictionary}");
            if note != NOTE {
                fs::rename(root.join(note), root.join(NOTE)).unwrap();
            }
            if dictionary != "翻译文件.json" {
                fs::rename(root.join(dictionary), root.join("翻译文件.json")).unwrap();
            }
        }
        fs::write(root.join(NOTE), TEXT).unwrap();
        fs::rename(root.join("翻译文件.json"), root.join("settings.json")).unwrap();
        for text in [
            TEXT.to_owned(),
            format!("{TEXT} ../settings.json"),
            format!("{TEXT} settings.json.bak"),
        ] {
            fs::write(root.join(NOTE), text).unwrap();
            assert!(!detect(root, "Game.exe", &|| false));
        }
        fs::write(root.join(NOTE), format!("{TEXT} 加载 settings.json。")).unwrap();
        assert!(detect(root, "Game.exe", &|| false));
        fs::remove_file(root.join("settings.json")).unwrap();
        assert!(!detect(root, "Game.exe", &|| false));
    }

    #[test]
    fn external_translation_rejects_weak_notes_and_misleading_links_but_does_not_require_a_link() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        for text in [
            TEXT.to_owned(),
            format!("{TEXT} https://mtool.app/?lang=chs"),
        ] {
            fs::write(root.join(NOTE), &text).unwrap();
            assert!(detect(root, "Game.exe", &|| false));
        }
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
            .set_len(READ_LIMIT as u64 + 1)
            .unwrap();
        assert!(!detect(root, "Game.exe", &|| false));
        fs::write(root.join(NOTE), TEXT).unwrap();
        assert!(!detect(root, "Game.exe", &|| true));
        assert!(!detect(root, "../Game.exe", &|| false));
        assert_eq!(fs::read(root.join("Game.exe")).unwrap(), b"synthetic");
    }

    #[test]
    fn external_translation_rejects_negative_actions_without_blocking_external_tool_instructions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        for text in [
            "请勿使用 MTool 打开游戏并加载翻译",
            "不要用公共 MTool 打开游戏和翻译文件",
            "禁止使用已有的 MTool 打开游戏及翻译文件",
            "不建议使用 MTool 打开游戏并加载翻译",
            "請勿使用 MTool 打開遊戲並載入翻譯",
            "MTool 无法正确打开游戏或加载翻译文件",
            "MTool：目前不能打开游戏或加载翻译",
            "MTool 不支持此游戏，不能加载翻译",
            "MTool cannot open the game or load the translation",
            "MTool can't open the game or load its translation",
            "MTool can’t load the translation for this game",
            "MTool currently does not load the translation for this game",
            "MTool is unable to open the game and load its translation",
            "Do not use the shared MTool to open the game and load the translation",
            "Don't use MTool to open the game and load the translation",
            "You should not use MTool to open this game and load its translation",
            "Avoid using MTool to open the game and load its translation",
        ] {
            fs::write(root.join(NOTE), text).unwrap();
            assert!(!detect(root, "Game.exe", &|| false), "{text}");
        }
        for text in [
            TEXT,
            "不要直接打开游戏，请使用 MTool 打开游戏并加载翻译文件。",
            "请勿删除翻译文件，用已有 MTool 打开游戏并加载翻译。",
            "Use your existing MTool to open the game and load the translation; MTool is not included.",
            "Do not open the game directly; use MTool to open the game and load the translation.",
            "Do not delete the translation files. Use MTool to open the game and load the translation.",
        ] {
            fs::write(root.join(NOTE), text).unwrap();
            assert!(detect(root, "Game.exe", &|| false), "{text}");
        }
    }

    #[test]
    fn external_translation_samples_flat_string_pairs_without_loading_the_full_dictionary() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        let dictionary = root.join("翻译文件.json");
        for text in [
            "",
            "{}",
            "[]",
            "not JSON",
            r#"{"one":"一"}"#,
            r#"{"width":1280,"height":720,"fullscreen":true}"#,
            r#"{"config":{"language":"zh"},"translation":["a","b"]}"#,
            r#"{"a":"A","b":"B","c":"C","d":"D","nested":{}}"#,
            r#"{"a":"A","b":"B","c":"C","d":"D""#,
            r#"{"a":"A","b":"B","c":"C","d":"D"} junk"#,
        ] {
            fs::write(&dictionary, text).unwrap();
            assert!(!detect(root, "Game.exe", &|| false), "{text}");
        }
        fs::write(&dictionary, format!("\u{feff}{DICTIONARY}")).unwrap();
        assert!(detect(root, "Game.exe", &|| false));
        // A large synthetic file proves recognition uses only a bounded prefix and sample.
        let entries = (0..SAMPLE_PAIRS)
            .map(|i| format!("\"line{i}\":\"译文{i}\","))
            .collect::<String>();
        fs::write(&dictionary, format!("{{{entries}")).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&dictionary)
            .unwrap()
            .set_len(2 * READ_LIMIT as u64)
            .unwrap();
        assert!(detect(root, "Game.exe", &|| false));
        // The prefix can end in the middle of a long string, after enough complete pairs.
        let prefix = format!(
            "{},\"long\":\"{}",
            DICTIONARY.trim_end_matches('}'),
            "a".repeat(2 * READ_LIMIT)
        );
        fs::write(&dictionary, prefix).unwrap();
        assert!(detect(root, "Game.exe", &|| false));
        assert_eq!(
            fs::metadata(&dictionary).unwrap().len(),
            (DICTIONARY.len() - 1 + 9 + 2 * READ_LIMIT) as u64
        );
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
