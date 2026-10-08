//! Name-only recommendations. Package labels never determine an automatic association.
use crate::{paths, version};

pub(super) struct Name {
    raw: String,
    versionless: String,
    words: Vec<String>,
    other: String,
    numbers: Vec<String>,
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
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut ascii = false;
        for character in versionless.chars() {
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
        let mut words = Vec::new();
        let mut other = String::new();
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
            numbers,
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
        None
    }
}
