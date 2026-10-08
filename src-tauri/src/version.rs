//! Filename-only suggestions. Conflicting or complex tokens require manual input.
#[derive(Debug)]
enum Parsed {
    Absent,
    Simple(String),
    Ambiguous,
}
fn parse(value: &str) -> Parsed {
    // Strip only supported game-file suffixes; complex version suffixes stay ambiguous.
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
        let (end, groups) = number(bytes, start);
        if !(2..=4).contains(&groups) && !(long > 1 && groups == 1) {
            return Parsed::Ambiguous;
        }
        if !simple_tail(&value[end..]) {
            return Parsed::Ambiguous;
        }
        found.push(value[i..end].to_owned());
        i = end;
    }
    if found.is_empty() {
        let text = value.trim_end_matches([' ', ')', ']', '】']);
        let bytes = text.as_bytes();
        let mut start = bytes.len();
        while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            start -= 1;
        }
        if start < bytes.len()
            && bytes[start].is_ascii_digit()
            && (start == 0 || !bytes[start - 1].is_ascii_alphanumeric())
        {
            let (end, groups) = number(bytes, start);
            let first = text[start..].split('.').next().unwrap_or("");
            if end == bytes.len()
                && (2..=4).contains(&groups)
                && first.len() <= 3
                && !(groups == 3 && first.parse::<u32>().unwrap_or(0) >= 20)
            {
                found.push(text[start..].into());
            }
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
        ] {
            assert_eq!(simple_version(text).as_deref(), Some(expected), "{text}");
        }
        for text in [
            "dev1.2",
            "v1",
            "v1.2beta",
            "v1.2 rc1",
            "V1.5.12.Fix4",
            "v1.2-rc1",
            "v1.2.3.4.5",
            "v1.2 v2.0",
            "Ver0.29.3b",
            "2026.10.05",
            "游戏 26.07.01",
            "游戏2",
            "游戏v1.2beta.qsp",
            "游戏V1.5.12.Fix4.html",
        ] {
            assert!(simple_version(text).is_none(), "{text}");
        }
        assert_eq!(suggest("游戏", Some("包装Ver1.06/Game.exe")).0, "Ver1.06");
        assert_eq!(suggest("游戏v1.2", Some("v2.0.exe")).0, "Unknown");
        assert_eq!(suggest("游戏v1.2beta", Some("v1.2.exe")).0, "Unknown");
    }
}
