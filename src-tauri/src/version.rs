//! Filename-only suggestions. Conflicting or unsupported tokens require manual input.
#[derive(Debug)]
enum Parsed {
    Absent,
    Simple(String),
    Ambiguous,
}
fn parse(value: &str) -> Parsed {
    let archive_free = without_archive_suffix(value);
    let value = archive_free.as_str();
    // Strip supported game-file suffixes; preserve recognized version qualifiers.
    let value = value
        .rsplit_once('.')
        .filter(|(_, extension)| {
            ["exe", "html", "htm", "qsp", "gam", "swf"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
        .map_or(value, |(stem, _)| stem);
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'v' {
            i += 1;
            continue;
        }
        let long = if lower[i..].starts_with("version") {
            7
        } else if lower[i..].starts_with("ver") {
            3
        } else {
            1
        };
        if i > 0 && bytes[i - 1].is_ascii_alphabetic() && (long > 1 || value.as_bytes()[i] != b'V')
        {
            i += 1;
            continue;
        }
        let mut start = i + long;
        if long > 1 {
            while start < bytes.len() && matches!(bytes[start], b'.' | b' ' | b'_' | b'-') {
                start += 1;
            }
        }
        if !bytes.get(start).is_some_and(u8::is_ascii_digit) {
            i += 1;
            continue;
        }
        let (numeric_end, groups) = number(bytes, start);
        if !(2..=4).contains(&groups) && !(long > 1 && groups == 1) {
            return Parsed::Ambiguous;
        }
        let end = qualifier_end(value, numeric_end);
        if !simple_tail(&value[end..]) {
            return Parsed::Ambiguous;
        }
        found.push(value[i..end].to_owned());
        i = end;
    }
    if found.is_empty() {
        let mut start = 0;
        while start < bytes.len() {
            if !bytes[start].is_ascii_digit() {
                start += 1;
                continue;
            }
            let (numeric_end, groups) = number(bytes, start);
            let preceding = value[..start].chars().next_back();
            let boundary = preceding.is_none_or(|c| {
                !c.is_ascii() || c.is_ascii_whitespace() || matches!(c, '-' | '_' | '(' | '[')
            });
            let token = &value[start..numeric_end];
            let first = token.split('.').next().unwrap_or("");
            if boundary && groups >= 2 && first.len() <= 3 && !date_like(token) {
                let end = qualifier_end(value, numeric_end);
                if groups > 4 || !simple_tail(&value[end..]) {
                    return Parsed::Ambiguous;
                }
                found.push(value[start..end].into());
            }
            start = numeric_end;
        }
    }
    let key = |s: &str| {
        s.trim_start_matches(|c: char| !c.is_ascii_digit())
            .to_ascii_lowercase()
    };
    found.dedup_by(|a, b| key(a) == key(b));
    match found.len() {
        0 => Parsed::Absent,
        1 => Parsed::Simple(found.remove(0)),
        _ => Parsed::Ambiguous,
    }
}
fn without_archive_suffix(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let mut result = value.to_owned();
    let mut ranges = Vec::new();
    for suffix in [".7z", ".zip", ".rar"] {
        for (start, _) in lower.match_indices(suffix) {
            let end = start + suffix.len();
            if value[end..].chars().next().is_none_or(|c| {
                !c.is_ascii() || c.is_ascii_whitespace() || matches!(c, ')' | ']' | '}' | '_' | '-')
            }) {
                ranges.push(start..end);
            }
        }
    }
    ranges.sort_by_key(|r| std::cmp::Reverse(r.start));
    for range in ranges {
        // A boundary prevents joining a qualifier across an archive extension.
        result.replace_range(range, "]");
    }
    result
}
// Keep qualifiers verbatim instead of silently turning e.g. 1.2beta into 1.2.
fn qualifier_end(value: &str, numeric_end: usize) -> usize {
    let tail = &value[numeric_end..];
    let trimmed = tail.trim_start_matches(['.', '-', '_', ' ', '\t']);
    let separator = tail.len() - trimmed.len();
    let lower = trimmed.to_ascii_lowercase();
    let length = [
        "public", "final", "beta", "alpha", "demo", "fix", "vip", "rc", "ea",
    ]
    .iter()
    .find(|tag| lower.starts_with(**tag))
    .map(|tag| tag.len())
    .or_else(|| {
        (separator == 0
            && lower
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic))
        .then_some(1)
    });
    let Some(length) = length else {
        return numeric_end;
    };
    let mut end = numeric_end + separator + length;
    if value.as_bytes().get(end) == Some(&b'-')
        && value
            .as_bytes()
            .get(end + 1)
            .is_some_and(u8::is_ascii_digit)
    {
        end += 1;
    }
    while value.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if simple_tail(&value[end..]) {
        end
    } else {
        numeric_end
    }
}
fn date_like(token: &str) -> bool {
    let parts: Vec<_> = token.split('.').collect();
    parts.len() == 3
        && parts[0].parse::<u32>().is_ok_and(|year| year >= 20)
        && parts[1]
            .parse::<u32>()
            .is_ok_and(|month| (1..=12).contains(&month))
        && parts[2]
            .parse::<u32>()
            .is_ok_and(|day| (1..=31).contains(&day))
}
fn number(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut end = start;
    let mut groups = 1;
    while end < bytes.len() {
        if bytes[end].is_ascii_digit() {
            end += 1;
        } else if bytes[end] == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
            groups += 1;
            end += 1;
        } else {
            break;
        }
    }
    (end, groups)
}
fn simple_tail(tail: &str) -> bool {
    if tail.is_empty() || tail.eq_ignore_ascii_case(".exe") {
        return true;
    }
    let trimmed = tail.trim_start_matches([' ', '\t', '_', ')', ']', '}', '】']);
    let lower = trimmed.to_ascii_lowercase();
    if ["beta", "rc", "alpha", "demo", "fix"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        return false;
    }
    let next = tail.chars().next().unwrap();
    !next.is_ascii()
        || matches!(next, ' ' | '\t' | '_' | ')' | ']' | '}' | '+' | '[')
        || lower.starts_with("dlc")
        || (next == '-' && package_tail(tail))
}
// Packaging labels describe the distribution, not a version prerelease suffix.
fn package_tail(tail: &str) -> bool {
    let tail = tail.trim_start_matches(['-', '_', ' ', '\t']);
    if tail.is_empty() || tail.chars().next().is_some_and(|c| !c.is_ascii()) {
        return true;
    }
    let lower = tail.to_ascii_lowercase();
    for label in [
        "windows", "android", "linux", "win", "mac", "pc", "x64", "x86",
    ] {
        if let Some(rest) = lower.strip_prefix(label) {
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            return rest.is_empty()
                || rest.chars().next().is_some_and(|c| !c.is_ascii())
                || rest.starts_with([')', ']', '}', ' '])
                || (rest.starts_with(['-', '_']) && package_tail(rest));
        }
    }
    false
}
pub fn simple_version(value: &str) -> Option<String> {
    match parse(value) {
        Parsed::Simple(v) => Some(v),
        _ => None,
    }
}
pub fn suggest(folder: &str, executable: Option<&str>) -> (String, String) {
    let mut versions = Vec::new();
    for (text, source) in std::iter::once((folder, "folder_name")).chain(
        executable
            .into_iter()
            .flat_map(|p| p.split(['/', '\\']).map(|s| (s, "file_name"))),
    ) {
        match parse(text) {
            Parsed::Simple(v) => versions.push((v, source)),
            Parsed::Ambiguous => return ("Unknown".into(), "unknown".into()),
            Parsed::Absent => {}
        }
    }
    let key = |s: &str| {
        s.trim_start_matches(|c: char| !c.is_ascii_digit())
            .to_ascii_lowercase()
    };
    if let Some((v, source)) = versions.first() {
        if versions.iter().all(|(other, _)| key(other) == key(v)) {
            return (v.clone(), (*source).into());
        }
    }
    ("Unknown".into(), "unknown".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_directory_naming_patterns() {
        for (text, expected) in [
            ("游戏V1.45 官方中文", "V1.45"),
            ("FM2V1.22", "V1.22"),
            ("游戏 Ver1.06", "Ver1.06"),
            ("游戏ver.2.1", "ver.2.1"),
            ("游戏 ver-1.49.0", "ver-1.49.0"),
            ("游戏Ver11136", "Ver11136"),
            ("游戏ver030", "ver030"),
            ("游戏 1.71", "1.71"),
            ("游戏V2.00DLC", "V2.00"),
            ("[v1.2.3.4]", "v1.2.3.4"),
            ("APPENDV2.02", "V2.02"),
            ("游戏v1.2.qsp", "v1.2"),
            ("游戏 1.2.HTML", "1.2"),
            ("游戏 Ver1.06.gam", "Ver1.06"),
            ("v1.2beta", "v1.2beta"),
            ("v1.2 rc1", "v1.2 rc1"),
            ("V1.5.12.Fix4", "V1.5.12.Fix4"),
            ("v1.2-rc1", "v1.2-rc1"),
            ("Ver0.29.3b", "Ver0.29.3b"),
            ("游戏v1.2beta.qsp", "v1.2beta"),
            ("游戏V1.5.12.Fix4.html", "V1.5.12.Fix4"),
        ] {
            assert_eq!(simple_version(text).as_deref(), Some(expected), "{text}");
        }
        for text in [
            "dev1.2",
            "v1",
            "v1.2.3.4.5",
            "v1.2 v2.0",
            "2026.10.05",
            "游戏 26.07.01",
            "游戏2",
        ] {
            assert!(simple_version(text).is_none(), "{text}");
        }
        assert_eq!(suggest("游戏", Some("包装Ver1.06/Game.exe")).0, "Ver1.06");
        assert_eq!(suggest("游戏v1.2", Some("v2.0.exe")).0, "Unknown");
        assert_eq!(suggest("游戏v1.2beta", Some("v1.2.exe")).0, "Unknown");
    }
    #[test]
    fn distribution_labels_and_translated_titles_do_not_hide_decimal_versions() {
        for (name, version) in [
            ("Ripples-0.5.0-pc2", "0.5.0"),
            ("RiseoftheOrcsDarkMemories-3.6-pc", "3.6"),
            ("Sara-0.9 莎拉 第二部", "0.9"),
            ("SaradaRising-1.1.3-pc", "1.1.3"),
            ("Agent 17_0.26.10-pc", "0.26.10"),
            ("Another Chance -v1.66-pc 另一个机会", "v1.66"),
            ("CampMourningWood-0.23.0.4-pc", "0.23.0.4"),
            ("游戏 1.0完结 新鲜姐妹", "1.0"),
            ("游戏1.2-pc", "1.2"),
            ("PathOfDesire-0.4.0-欲望之路", "0.4.0"),
            ("CityDevilRestart-0.3.0- 城市恶魔：重启", "0.3.0"),
            ("LoSeSb-24.11.0-pc", "24.11.0"),
            ("Game [1.2.3-win64]", "1.2.3"),
            ("游戏-v1.2-pc-x64", "v1.2"),
            ("Under_Your_Spell-0.4.0p-pc", "0.4.0p"),
            ("MilaAI-1.5.4public-pc", "1.5.4public"),
            ("That New Teacher-v0.9.0ea-pc 那位新老师", "v0.9.0ea"),
            ("Harem_Hotel-v0.18-BETA-3 官方中文", "v0.18-BETA-3"),
            ("游戏v1.03b", "v1.03b"),
            ("游戏1.0.rar", "1.0"),
            ("SummerClover v1.11.7z 夏色四葉草", "v1.11"),
            ("Game-v1.2.zip-beta1", "v1.2"),
        ] {
            assert_eq!(simple_version(name).as_deref(), Some(version), "{name}");
            assert_eq!(
                suggest(name, Some("Game.exe")),
                (version.into(), "folder_name".into())
            );
        }
        for name in [
            "游戏-v1.2-patch2",
            "游戏-1.2arbitrary-pc",
            "游戏-1.2.3.4.5-pc",
            "游戏-26.07.01-pc",
            "游戏-2026.10.05-pc",
            "游戏2-pc",
            "Chapter1_Ep.2-pc",
            "Game32.exe",
            "游戏-1.2-pcorporate",
            "游戏-1.2 2.0-pc",
        ] {
            assert!(simple_version(name).is_none(), "{name}");
        }
        assert_eq!(
            suggest(
                "Runawaygirl Sweet Days V1.04",
                Some("RunawayGirl_MultiLang_Ver1.06/RunawayGirl.exe")
            ),
            ("Unknown".into(), "unknown".into())
        );
        assert_eq!(suggest("Game-1.2-pc", Some("Game-2.0.exe")).0, "Unknown");
        assert_eq!(
            suggest("Game-1.2-pc", Some("Game-1.2.exe")),
            ("1.2".into(), "folder_name".into())
        );
    }
}
