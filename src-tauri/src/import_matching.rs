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
        let mut title = Self {
            editions: edition_markers(full),
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
                } else if package_label(token)
                    || package_version(token)
                    || release_date(token)
                    || ["extended", "electron", "gg"].contains(&token)
                    || token == "ai" && next.is_some_and(|next| package_label(next))
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
    ]
    .contains(&token)
}

// A shared original title must not hide sequel/chapter labels in its translation.
fn edition_markers(title: &str) -> Vec<String> {
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
    .filter(|marker| title.contains(marker))
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
            markers.push(characters[start..=end].iter().collect());
        }
    }
    markers.sort();
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
    pub(super) fn new(title: &str) -> Self {
        let raw = paths::normalize_alias(title);
        let simple = version::simple_version(title);
        let versionless = simple.map_or_else(
            || raw.clone(),
            |value| paths::normalize_alias(&title.replacen(&value, " ", 1)),
        );
        let versionless = versionless
            .trim_matches([' ', '_', '-', '[', ']', '(', ')'])
            .to_owned();
        // Separate scripts even without spaces, so translated names can change position.
        let tokens = tokenize(&versionless);
        let title = Title::new(&tokens, &versionless);
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

    /// All relaxed comparisons are recommendations. A subset score never grants auto-association.
    pub(super) fn proposal(&self, old: &Self) -> Option<(u8, String)> {
        if self.raw != old.raw
            && (self.numbers != old.numbers || self.title.editions != old.title.editions)
        {
            return None;
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
        let mut result = [legacy, best]
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
