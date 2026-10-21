//! Name-only recommendations. Package labels never determine an automatic association.
use crate::{paths, version};
use std::collections::HashSet;

pub(super) struct Name {
    raw: String,
    versionless: String,
    words: Vec<String>,
    other: String,
    japanese: String,
    translation: String,
    numbers: Vec<String>,
    title: Title,
    package: PackageTitle,
}

#[derive(Default)]
struct PackageTitle {
    latin: String,
    chapters: Vec<String>,
    re: bool,
    release: bool,
}

impl PackageTitle {
    fn new(value: &str, release: bool) -> Self {
        let mut result = Self {
            release,
            ..Self::default()
        };
        let tokens = tokenize(value);
        for token in tokens.iter().map(|token| token.trim_matches('.')) {
            if !token.is_ascii()
                || package_label(token)
                || package_version(token)
                || release_date(token)
                || token.chars().all(|c| c.is_ascii_digit())
            {
                continue;
            }
            // Ren'Py packages sometimes attach the decimal version to the Re label.
            // Keep Re as part of the title; never strip arbitrary title digits.
            if token == "re"
                || token.strip_prefix("re").is_some_and(|suffix| {
                    let parts: Vec<_> = suffix.split('.').collect();
                    (2..=4).contains(&parts.len())
                        && parts.iter().all(|part| {
                            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())
                        })
                })
            {
                result.re = true;
                result.release |= token != "re";
                result.latin.push_str("re");
                continue;
            }
            // Chapter is an identity guard; Up is a release revision within that chapter.
            if let Some(chapter) = token.strip_prefix("ch").map(|s| s.trim_start_matches('.')) {
                let (chapter, update) = chapter
                    .split_once("up")
                    .map_or((chapter, None), |(a, b)| (a, Some(b)));
                if !chapter.is_empty()
                    && chapter.chars().all(|c| c.is_ascii_digit())
                    && update.is_none_or(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_digit()))
                {
                    result.chapters.push(chapter.into());
                    result.release |= update.is_some();
                    continue;
                }
            }
            if !result.chapters.is_empty()
                && token
                    .strip_prefix("up")
                    .map(|s| s.trim_start_matches('.'))
                    .is_some_and(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_digit()))
            {
                result.release = true;
                continue;
            }
            result.latin.push_str(&token.replace('.', ""));
        }
        result
    }
}

#[derive(Default)]
struct Title {
    words: Vec<String>,
    chinese: String,
    japanese: Vec<String>,
    editions: Vec<String>,
    dlc: bool,
}

fn package_label(token: &str) -> bool {
    package_label_v1(token)
        || [
            "官中无修",
            "官中无修版",
            "官方中文无修版",
            "官中动态步兵正式版",
            "官中动态步兵版",
            "官中步兵正式版",
            "官中步兵版",
        ]
        .contains(&token)
}

// Persisted v1 memory keys must keep their original normalization contract.
fn package_label_v1(token: &str) -> bool {
    if token
        .strip_prefix("pc")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
    {
        return true;
    }
    if translation_package_label(token)
        || [
            "pc", "windows", "win", "win32", "win64", "x86", "x64", "android", "linux", "mac", "ai",
        ]
        .contains(&token)
    {
        return token != "ai";
    }
    // Consume complete packaging descriptions, rather than removing substrings from titles.
    let mut rest = token;
    let mut describes_package = false;
    while !rest.is_empty() {
        let Some(word) = [
            "内嵌", "内置", "内部", "赞助", "付费", "完结", "汉化", "机翻", "精翻", "去码", "补丁",
            "中文", "简体", "繁体", "版",
        ]
        .into_iter()
        .find(|word| rest.starts_with(word)) else {
            return false;
        };
        describes_package |= ["赞助", "付费", "汉化", "机翻", "精翻", "去码"].contains(&word);
        rest = &rest[word.len()..];
    }
    describes_package
}

impl Title {
    fn new(tokens: &[String], full: &str) -> Self {
        Self::parse(tokens, full, false)
    }

    fn parse(tokens: &[String], full: &str, memory_v1: bool) -> Self {
        let is_package = if memory_v1 {
            package_label_v1
        } else {
            package_label
        };
        let mut title = Self {
            editions: edition_markers_with_mode(full, memory_v1),
            ..Self::default()
        };
        let mut index = 0;
        for chunk in full.split_whitespace() {
            // Han-only subtitles following kana belong to the same original-title phrase.
            let mut japanese_started = false;
            for token in tokenize(chunk) {
                let token = token.trim_matches('.');
                let next = tokens.get(index + 1);
                index += 1;
                if token == "dlc" {
                    title.dlc = true;
                } else if is_package(token)
                    || package_version(token)
                    || release_date(token)
                    || ["extended", "electron", "gg"].contains(&token)
                    || token == "ai" && next.is_some_and(|next| is_package(next))
                {
                    continue;
                } else if token.is_ascii() {
                    if !token.is_empty() && !token.chars().all(|c| c.is_ascii_digit()) {
                        title.words.push(token.replace('.', ""));
                    }
                } else if token.chars().any(kana) || japanese_started {
                    japanese_started = true;
                    title
                        .japanese
                        .push(token.chars().filter(|c| c.is_alphanumeric()).collect());
                } else {
                    title
                        .chinese
                        .extend(token.chars().filter(|c| c.is_alphanumeric()));
                }
            }
        }
        title.words.sort();
        title
    }
}

fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut ascii = false;
    for character in value.chars() {
        if !character.is_alphanumeric() && character != '.' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        if !current.is_empty() && character.is_ascii() != ascii {
            tokens.push(std::mem::take(&mut current));
        }
        ascii = character.is_ascii();
        current.push(character);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn bigrams(value: &str) -> HashSet<(char, char)> {
    let characters: Vec<_> = value.chars().collect();
    characters
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

fn similarity(left: &str, right: &str) -> f64 {
    let a = bigrams(left);
    let b = bigrams(right);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    2.0 * a.intersection(&b).count() as f64 / (a.len() + b.len()) as f64
}

fn kana(character: char) -> bool {
    matches!(character as u32, 0x3041..=0x3096 | 0x30a1..=0x30fa | 0x31f0..=0x31ff)
}

fn han(character: char) -> bool {
    matches!(character as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0x20000..=0x2fa1f)
}

// Ignore only complete, separated package labels in recommendations, never inside a title.
fn translation_package_label(token: &str) -> bool {
    [
        "汉化",
        "汉化版",
        "汉化补丁",
        "内嵌汉化",
        "内嵌汉化版",
        "中文汉化版",
        "简体中文版",
        "繁体中文版",
        "去码",
        "去码版",
        "去码补丁",
        "无码补丁",
        "解码版",
        "解码补丁",
        "官中",
        "官中版",
        "官中正式版",
        "官方中文版",
        "官方中文正式版",
    ]
    .contains(&token)
}

// A shared original title must not hide sequel/chapter labels in its translation.
fn edition_markers(title: &str) -> Vec<String> {
    edition_markers_with_mode(title, false)
}

fn edition_markers_with_mode(title: &str, memory_v1: bool) -> Vec<String> {
    let tokens = tokenize(title);
    let bounded = |marker: &str| {
        title.match_indices(marker).any(|(start, _)| {
            title[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_ascii_alphabetic())
                && title[start + marker.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_ascii_alphabetic())
        })
    };
    let mut markers = [
        "外传",
        "外傳",
        "番外",
        "续作",
        "續作",
        "前传",
        "前傳",
        "后传",
        "後傳",
        "重制版",
        "重製版",
        "remake",
        "remaster",
        "试玩",
        "demo",
    ]
    .into_iter()
    .filter(|marker| {
        if memory_v1 || !marker.is_ascii() {
            title.contains(marker)
        } else {
            bounded(marker) || *marker == "remaster" && bounded("remastered")
        }
    })
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let characters = title.chars().collect::<Vec<_>>();
    for (start, character) in characters.iter().enumerate() {
        if *character != '第' {
            continue;
        }
        let mut end = start + 1;
        while characters
            .get(end)
            .is_some_and(|c| c.is_ascii_digit() || "一二三四五六七八九十百零〇两兩".contains(*c))
        {
            end += 1;
        }
        if end > start + 1
            && characters
                .get(end)
                .is_some_and(|c| "部章作季集篇巻卷".contains(*c))
        {
            let number: String = characters[start + 1..end].iter().collect();
            if characters[end] == '季' {
                let number = match number.as_str() {
                    "一" => "1",
                    "二" => "2",
                    "三" => "3",
                    "四" => "4",
                    "五" => "5",
                    "六" => "6",
                    "七" => "7",
                    "八" => "8",
                    "九" => "9",
                    "十" => "10",
                    other => other,
                };
                markers.push(format!("season:{number}"));
            } else {
                markers.push(characters[start..=end].iter().collect());
            }
        }
    }
    for (index, token) in tokens.iter().enumerate() {
        let number = token
            .strip_prefix("season")
            .or_else(|| token.strip_prefix('s'));
        let number = if token == "season" {
            tokens.get(index + 1).map(String::as_str)
        } else {
            number
        };
        if let Some(number) =
            number.filter(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        {
            markers.push(format!("season:{number}"));
        }
    }
    markers.sort();
    markers.dedup();
    markers
}

fn package_version(token: &str) -> bool {
    let Some(number) = token
        .strip_prefix("version")
        .or_else(|| token.strip_prefix("ver"))
        .or_else(|| token.strip_prefix('v'))
    else {
        return false;
    };
    let number = number.strip_suffix(".gg").unwrap_or(number);
    !number.is_empty()
        && number.split('.').all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

fn release_date(token: &str) -> bool {
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || !matches!(parts[0].len(), 2 | 4) {
        return false;
    }
    let values = parts
        .iter()
        .map(|part| part.parse::<u32>())
        .collect::<std::result::Result<Vec<_>, _>>();
    values.is_ok_and(|values| {
        (parts[0].len() == 2 || (1900..=2099).contains(&values[0]))
            && (1..=12).contains(&values[1])
            && (1..=31).contains(&values[2])
    })
}

impl Name {
    fn compact_match(&self, old: &Self) -> bool {
        self.package.latin.len() >= 7
            && !["princess", "school", "sister", "adventure"].contains(&self.package.latin.as_str())
            && self.package.latin == old.package.latin
            && self.package.chapters == old.package.chapters
            && !if self.package.re && old.package.re {
                let base = |name: &Self| {
                    // Reparse the full name so version qualifiers retain edition guards.
                    Name::new(&name.raw.replace("重制版", "").replace("重製版", ""))
                };
                base(self).library_conflict(&base(old))
            } else {
                self.library_conflict(old)
            }
            && (self.title.japanese.is_empty()
                || old.title.japanese.is_empty()
                || self.title.japanese.concat() == old.title.japanese.concat())
    }

    pub(super) fn package_proposal(&self, old: &Self) -> Option<(u8, String)> {
        // Legacy library entries may have no executable metadata. Both names must carry
        // explicit release information, and the entire compact title/chapter must agree.
        (self.package.release
            && old.package.release
            && self.compact_match(old)
            && (self.title.chinese.is_empty()
                || old.title.chinese.is_empty()
                || self.title.chinese == old.title.chinese))
            .then(|| {
                (
                    72,
                    "英文完整标题与章节一致，空格、连字符或包版本不同；请确认".into(),
                )
            })
    }

    // This fallback is gated by matching distinctive executable / external save metadata.
    // Relax packaging/separators and missing first-season annotations, never conflicting works.
    pub(super) fn metadata_proposal(&self, old: &Self) -> Option<(u8, String)> {
        // Separator/packaging normalization remains gated by auxiliary game metadata.
        // It never establishes automatic association, even when the compact title is exact.
        if self.compact_match(old) {
            return Some((
                60,
                "忽略英文空格、连字符及明确包版本后标题一致；结合项目资料确认".into(),
            ));
        }
        let base = |name: &Self| {
            let raw = name.raw.replace("第一季", " ").replace("第1季", " ");
            let tokens = tokenize(&raw);
            let mut kept = Vec::new();
            let mut index = 0;
            while index < tokens.len() {
                if tokens[index] == "season"
                    && tokens.get(index + 1).is_some_and(|next| next == "1")
                {
                    index += 2;
                    continue;
                }
                if !["s1", "season1"].contains(&tokens[index].as_str()) {
                    kept.push(tokens[index].as_str());
                }
                index += 1;
            }
            Name::new(&kept.join(" "))
        };
        let a = base(self);
        let b = base(old);
        if a.library_conflict(&b) {
            return None;
        }
        let japanese = a.title.japanese.concat();
        let old_japanese = b.title.japanese.concat();
        if !japanese.is_empty() && !old_japanese.is_empty() && japanese != old_japanese {
            // Some translated packages replace Japanese Han characters with Simplified
            // Chinese. Restrict this fallback to these variants within a kana title;
            // callers still require matching distinctive executable/external-save evidence.
            let variant = |s: &str| s.replace('过', "過").replace('间', "間");
            if japanese.chars().count() >= 8 && variant(&japanese) == variant(&old_japanese) {
                return Some((70, "日文标题的简繁文字变体一致；结合项目资料确认".into()));
            }
            return None;
        }
        let common_words = a
            .title
            .words
            .iter()
            .filter(|word| b.title.words.contains(word))
            .collect::<HashSet<_>>();
        let chinese = &a.title.chinese;
        let old_chinese = &b.title.chinese;
        let chinese_support = chinese.chars().count() >= 5
            && old_chinese.chars().count() >= 5
            && chinese.chars().all(han)
            && old_chinese.chars().all(han)
            && (chinese == old_chinese
                || (common_words.len() >= 3
                    && similarity(chinese, old_chinese) >= 0.6
                    && similarity(&a.title.words.concat(), &b.title.words.concat()) >= 0.75));
        let latin_support = common_words.len() >= 3
            && a.title.words.iter().map(String::len).sum::<usize>() >= 12
            && similarity(&a.title.words.concat(), &b.title.words.concat()) >= 0.85;
        (chinese_support || latin_support).then(|| {
            (
                60,
                "主要标题相近，季标记或英文别称不同；结合项目资料确认".into(),
            )
        })
    }

    pub(super) fn new(title: &str) -> Self {
        let raw = paths::normalize_alias(title);
        let simple = version::simple_version(title);
        let release = simple.is_some();
        let versionless = simple.map_or_else(
            || raw.clone(),
            |value| paths::normalize_alias(&title.replacen(&value, " ", 1)),
        );
        let versionless = versionless
            .trim_matches([' ', '_', '-', '[', ']', '(', ')'])
            .to_owned();
        // Separate scripts even without spaces, so translated names can change position.
        let tokens = tokenize(&versionless);
        let mut title = Title::new(&tokens, &versionless);
        // A version qualifier can consume Demo; retain edition guards from the full name.
        title.editions = edition_markers(&raw);
        let package = PackageTitle::new(&versionless, release);
        let mut words = Vec::new();
        let mut other = String::new();
        let mut japanese = String::new();
        let mut translation = String::new();
        let mut numbers = Vec::new();
        for token in tokens {
            let token = token.trim_matches('.');
            if token.is_empty()
                || package_version(token)
                || release_date(token)
                || ["extended", "electron", "gg"].contains(&token)
            {
                continue;
            }
            if token.is_ascii() {
                if token.chars().all(|character| character.is_ascii_digit()) {
                    numbers.push(token.to_owned());
                } else {
                    words.push(token.replace('.', ""));
                }
            } else {
                if token.chars().any(kana) {
                    japanese.extend(
                        token
                            .chars()
                            .filter(|character| character.is_alphanumeric()),
                    );
                } else if !translation_package_label(token) {
                    translation.extend(
                        token
                            .chars()
                            .filter(|character| character.is_alphanumeric()),
                    );
                }
                other.extend(
                    token
                        .chars()
                        .filter(|character| character.is_alphanumeric()),
                );
            }
        }
        words.sort();
        numbers.sort();
        Self {
            raw,
            versionless,
            words,
            other,
            japanese,
            translation,
            numbers,
            title,
            package,
        }
    }

    pub(super) fn compare(&self, old: &Self) -> Option<(u8, &'static str)> {
        if self.raw == old.raw && !self.raw.is_empty() {
            return Some((100, "名称或别名一致"));
        }
        if self.versionless.chars().count() >= 3 && self.versionless == old.versionless {
            return Some((95, "去除明确版本后名称一致"));
        }
        // Never discard plain sequel numbers or contradicting numbers in a translation.
        if self.numbers != old.numbers {
            return None;
        }
        let latin_size = self.words.iter().map(String::len).sum::<usize>();
        let other_size = self.other.chars().count();
        if self.words == old.words
            && self.other == old.other
            && (latin_size >= 8 || other_size >= 4)
        {
            return Some((85, "名称词段一致，版本、包描述或顺序不同；请确认"));
        }
        if self.words.len() >= 2 && latin_size >= 12 && self.words == old.words {
            return Some((75, "英文名称一致，译名或包描述不同；请确认"));
        }
        if other_size >= 4
            && self.other == old.other
            && (self.words.is_empty() || old.words.is_empty())
        {
            return Some((75, "中文或日文名称一致，其中一份附有英文名；请确认"));
        }
        if self.japanese.chars().count() >= 8
            && self.japanese == old.japanese
            && self.words == old.words
            && edition_markers(&self.versionless) == edition_markers(&old.versionless)
        {
            return Some((75, "完整日文原名一致，中文译名不同；请确认"));
        }
        if self.translation.chars().count() >= 5
            && self.translation.chars().all(han)
            && self.translation == old.translation
            && self.words == old.words
            && (self.japanese.is_empty()
                || old.japanese.is_empty()
                || self.japanese == old.japanese)
            && edition_markers(&self.versionless) == edition_markers(&old.versionless)
        {
            return Some((75, "中文标题一致，日文原名或汉化、去码描述不同；请确认"));
        }
        None
    }

    pub(super) fn keys(&self) -> HashSet<String> {
        let mut keys = HashSet::from([
            format!("exact:{}", self.raw),
            format!("versionless:{}", self.versionless),
        ]);
        if self.package.latin.len() >= 8 {
            keys.insert(format!("compact:{}", self.package.latin));
        }
        keys.extend(
            self.title
                .words
                .iter()
                .filter(|word| word.len() >= 3)
                .map(|word| format!("word:{word}")),
        );
        for text in [&self.other, &self.title.chinese]
            .into_iter()
            .chain(self.title.japanese.iter())
        {
            keys.extend(
                bigrams(text)
                    .into_iter()
                    .map(|(a, b)| format!("gram:{a}{b}")),
            );
        }
        keys
    }

    pub(super) fn words(&self) -> &[String] {
        &self.title.words
    }

    pub(super) fn memory_key(&self) -> Option<String> {
        // Freeze v1 keys so parser fixes do not invalidate already-confirmed memories.
        // Recommendation/conflict checks still use the corrected title parser.
        let title = Title::parse(&tokenize(&self.versionless), &self.versionless, true);
        let has_title = title.words.iter().any(|word| {
            ![
                "game",
                "games",
                "launcher",
                "launch",
                "start",
                "application",
                "project",
                "mygame",
                "newgame",
                "testgame",
                "myproject",
                "newproject",
                "new",
                "my",
                "test",
                "sample",
                "example",
                "unity",
                "renpy",
                "visualnovel",
                "untitled",
                "unknown",
                "setup",
                "installer",
                "uninstaller",
                "data",
                "save",
                "saves",
                "tools",
                "mtool",
            ]
            .contains(&word.as_str())
        }) || (!title.chinese.is_empty()
            && !["游戏", "游戏资源", "游戏下载"].contains(&title.chinese.as_str()))
            || !title.japanese.is_empty();
        let distinctive = title.words.iter().map(String::len).sum::<usize>() >= 6
            || title.chinese.chars().count() >= 3
            || title.japanese.concat().chars().count() >= 3;
        (has_title && distinctive && self.useful_title(&title)).then(|| {
            serde_json::to_string(&(
                &title.words,
                &title.chinese,
                &title.japanese,
                &self.numbers,
                &title.editions,
                title.dlc,
                &self.package.chapters,
            ))
            .unwrap()
        })
    }

    pub(super) fn memory_conflict(&self, old: &Self) -> bool {
        self.library_conflict(old) || self.package.chapters != old.package.chapters
    }

    pub(super) fn useful_library_name(&self) -> bool {
        self.useful_title(&self.title)
    }

    fn useful_title(&self, title: &Title) -> bool {
        self.raw.chars().count() >= 3
            && ![
                "game",
                "games",
                "data",
                "www",
                "tool",
                "tools",
                "mtool",
                "patch",
                "update",
                "windows",
                "汉化",
                "汉化版",
                "汉化包",
                "游戏",
                "游戏资源",
            ]
            .contains(&self.raw.as_str())
            && (!title.words.is_empty() || !title.chinese.is_empty() || !title.japanese.is_empty())
    }

    /// Import-only fallback for old zero-padded package suffixes. Never a version parser
    /// or a library duplicate rule: the entire bilingual title must agree, with no other numbers.
    pub(super) fn legacy_numeric_proposal(&self, old: &Self) -> Option<(u8, String)> {
        let legacy_suffix = |name: &Self| {
            let tokens = tokenize(&name.versionless);
            let last = tokens
                .iter()
                .rev()
                .map(|s| s.trim_matches('.'))
                .find(|token| !token.is_empty() && !package_label(token));
            name.numbers.len() == 1
                && last.is_some_and(|token| {
                    (3..=4).contains(&token.len())
                        && token.starts_with('0')
                        && token.chars().all(|c| c.is_ascii_digit())
                        && token == name.numbers[0]
                })
        };
        let left = legacy_suffix(self);
        let right = legacy_suffix(old);
        if !(left || right)
            || (!left && !self.numbers.is_empty())
            || (!right && !old.numbers.is_empty())
            || self.title.chinese.chars().count() < 5
            || !self.title.chinese.chars().all(han)
            || self.title.chinese != old.title.chinese
            || self.title.words.len() < 3
            || self.title.words.iter().map(String::len).sum::<usize>() < 18
            || self.title.words != old.title.words
            || self.title.japanese != old.title.japanese
            || self.title.editions != old.title.editions
            || self.title.dlc != old.title.dlc
            || self.package.chapters != old.package.chapters
        {
            return None;
        }
        Some((
            70,
            "完整中英标题一致，末尾数字可能是版本或整理编号；请确认".into(),
        ))
    }

    // Library recommendations are stricter than Import: a DLC bundle may update a game
    // during Import, but it must not make two existing works appear interchangeable.
    pub(super) fn library_conflict(&self, old: &Self) -> bool {
        self.title_conflict(old) || self.title.dlc != old.title.dlc
    }

    fn title_conflict(&self, old: &Self) -> bool {
        let markers = |name: &Self| {
            let mut markers = name.title.editions.clone();
            for word in [
                "ii",
                "iii",
                "iv",
                "v",
                "vi",
                "vii",
                "viii",
                "ix",
                "expansion",
                "spinoff",
                "gaiden",
            ] {
                if name.title.words.iter().any(|token| token == word) {
                    markers.push(word.into());
                }
            }
            for marker in [
                "外伝",
                "続編",
                "後日談",
                "スピンオフ",
                "side story",
                "spin-off",
            ] {
                if name.versionless.contains(marker) {
                    markers.push(marker.into());
                }
            }
            markers.sort();
            markers
        };
        self.numbers != old.numbers || markers(self) != markers(old)
    }

    /// Import-only recommendations for package revisions and added original titles.
    /// These do not change memory keys, version parsing or library duplicate rules.
    pub(super) fn import_proposal(&self, old: &Self, supported: bool) -> Option<(u8, String)> {
        if self.title_conflict(old) || self.package.chapters != old.package.chapters {
            return None;
        }
        let words = |name: &Self| name.title.words.iter().cloned().collect::<HashSet<_>>();
        let left = words(self);
        let right = words(old);
        let compatible_japanese = self.title.japanese.is_empty()
            || old.title.japanese.is_empty()
            || self.title.japanese.concat() == old.title.japanese.concat();
        let repeated_alias = (left.len() < self.title.words.len()
            || right.len() < old.title.words.len())
            && left == right
            && left.iter().map(String::len).sum::<usize>() >= 6
            && self.title.chinese.chars().count() >= 6
            && self.title.chinese.chars().all(han)
            && self.title.chinese == old.title.chinese
            && compatible_japanese;
        let same_bilingual_title = (left.is_empty() || right.is_empty())
            && self.title.chinese.chars().count() >= 6
            && self.title.chinese.chars().all(han)
            && self.title.chinese == old.title.chinese
            && self.title.japanese.concat().chars().count() >= 8
            // Only this visually identical hiragana/katakana pair is relaxed, and only
            // with a complete matching Chinese title. Do not transliterate Japanese.
            && self.title.japanese.concat().replace('ヘ', "へ")
                == old.title.japanese.concat().replace('ヘ', "へ");
        let result = if same_bilingual_title {
            Some((72, "完整中日标题一致，一方附有英文名或日文形近假名；请确认"))
        } else if repeated_alias {
            Some((72, "中英文主标题一致，一方补充原名或重复别称；请确认"))
        } else if supported && (self.subtitle_of(old) || old.subtitle_of(self)) {
            Some((60, "中文主标题一致，一方附有副标题；结合项目资料确认"))
        } else {
            self.revision_proposal(old, supported)
        };
        let (score, reason) = result?;
        let mut reason = reason.to_owned();
        if self.title.dlc != old.title.dlc {
            reason.push_str(" · DLC 标记不同，请确认是否为完整更新包");
        }
        Some((score, reason))
    }

    fn subtitle_of(&self, other: &Self) -> bool {
        // Only one side may add a tilde-delimited subtitle. Never compare two different
        // subtitles by dropping both, and never discard arbitrary Chinese suffixes.
        let Some((head, _)) = self.versionless.split_once(['~', '～', '〜']) else {
            return false;
        };
        if other.versionless.contains(['~', '～', '〜']) {
            return false;
        }
        let main = Title::new(&tokenize(head), head);
        main.chinese.chars().count() >= 6
            && main.chinese.chars().all(han)
            && main.chinese == other.title.chinese
            && self.title.chinese != other.title.chinese
            && main.words == other.title.words
            && main.japanese.is_empty()
            && other.title.japanese.is_empty()
    }

    fn revision_proposal(&self, old: &Self, supported: bool) -> Option<(u8, &'static str)> {
        let without_revision = |name: &Self| {
            let mut revisions = 0;
            let kept = tokenize(&name.versionless)
                .into_iter()
                .filter(|token| {
                    let revision = token.strip_prefix('r').is_some_and(|number| {
                        (1..=4).contains(&number.len())
                            && number.chars().all(|c| c.is_ascii_digit())
                    });
                    revisions += usize::from(revision);
                    !revision
                })
                .collect::<Vec<_>>();
            (revisions == 1).then(|| Name::new(&kept.join(" ")))
        };
        let a = without_revision(self)?;
        let b = without_revision(old)?;
        if a.title_conflict(&b)
            || a.package.chapters != b.package.chapters
            || (!a.title.chinese.is_empty()
                && !b.title.chinese.is_empty()
                && a.title.chinese != b.title.chinese)
            || (!a.title.japanese.is_empty()
                && !b.title.japanese.is_empty()
                && a.title.japanese.concat() != b.title.japanese.concat())
        {
            return None;
        }
        let abbreviation = |short: &Self, long: &Self| {
            short.title.words.len() == 1
                && short.title.words[0].len() >= 8
                && ![
                    "adventure",
                    "princess",
                    "visualnovel",
                    "application",
                    "collection",
                    "compilation",
                    "launcher",
                ]
                .contains(&short.title.words[0].as_str())
                && long.title.words.contains(&short.title.words[0])
                && (short.title.words == long.title.words
                    || supported && long.title.words.len() >= 3)
        };
        (abbreviation(&a, &b) || abbreviation(&b, &a))
            .then_some((70, "英文名称标识一致，包修订号或别称不同；请确认"))
    }

    /// All relaxed comparisons are recommendations. A subset score never grants auto-association.
    pub(super) fn proposal(&self, old: &Self) -> Option<(u8, String)> {
        let packaged = self.package_proposal(old);
        if self.raw != old.raw
            && (self.numbers != old.numbers || self.title.editions != old.title.editions)
        {
            return packaged;
        }
        let dlc_difference = self.title.dlc != old.title.dlc;
        let legacy = self
            .compare(old)
            .map(|(score, reason)| (score, reason.to_owned()));
        let same_words = self.title.words == old.title.words;
        let compatible_words =
            same_words || self.title.words.is_empty() || old.title.words.is_empty();
        let japanese = self.title.japanese.concat();
        let old_japanese = old.title.japanese.concat();
        let same_japanese = japanese == old_japanese;
        let main_japanese = self
            .title
            .japanese
            .first()
            .zip(old.title.japanese.first())
            .is_some_and(|(a, b)| a == b && a.chars().count() >= 8);
        let missing_subtitle =
            main_japanese && (self.title.japanese.len() == 1 || old.title.japanese.len() == 1);
        let japanese_conflict =
            !japanese.is_empty() && !old_japanese.is_empty() && !same_japanese && !missing_subtitle;
        if japanese_conflict {
            return None;
        }
        let mut evidence = Vec::new();
        let words_subset = !self.title.words.is_empty()
            && !old.title.words.is_empty()
            && (self
                .title
                .words
                .iter()
                .all(|word| old.title.words.contains(word))
                || old
                    .title
                    .words
                    .iter()
                    .all(|word| self.title.words.contains(word)));
        if same_japanese
            && japanese.chars().count() >= 4
            && words_subset
            && self
                .title
                .words
                .iter()
                .any(|word| word.len() >= 3 && old.title.words.contains(word))
        {
            evidence.push((60, "日文短标题与英文标记一致，一方附有英文别称"));
        }
        if compatible_words {
            if japanese.chars().count() >= 8 && same_japanese {
                evidence.push((78, "日文原名一致"));
            } else if missing_subtitle {
                evidence.push((70, "日文主标题一致，一方缺少副标题"));
            }
            let chinese = &self.title.chinese;
            let old_chinese = &old.title.chinese;
            if chinese.chars().count() >= 5 && chinese.chars().all(han) && chinese == old_chinese {
                evidence.push((78, "中文标题一致，附加描述不同"));
            } else if chinese.chars().count() >= 6
                && old_chinese.chars().count() >= 6
                && chinese.chars().all(han)
                && old_chinese.chars().all(han)
                && similarity(chinese, old_chinese)
                    >= if same_words && !self.title.words.is_empty() {
                        0.75
                    } else {
                        0.85
                    }
            {
                evidence.push((
                    if same_words && !self.title.words.is_empty() {
                        76
                    } else {
                        55
                    },
                    "中文标题相近",
                ));
            }
            let latin_length: usize = self.title.words.iter().map(String::len).sum();
            if same_words && self.title.words.len() >= 2 && latin_length >= 8 {
                evidence.push((78, "英文标题词组一致"));
            } else if same_words && self.title.words.len() == 1 && latin_length >= 6 {
                evidence.push((55, "特色英文名称一致"));
            }
        }
        let best = evidence
            .into_iter()
            .max_by_key(|(score, _)| *score)
            .map(|(score, reason)| (score, format!("{reason}；请确认")));
        let mut result = [legacy, best, packaged]
            .into_iter()
            .flatten()
            .max_by_key(|(score, _)| *score)?;
        if dlc_difference {
            result.0 = result.0.min(85);
            result.1.push_str(" · DLC 标记不同，请确认是否为完整更新包");
        }
        Some(result)
    }
}
