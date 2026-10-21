//! Parse a parameter field, never an executable path or shell command.
use crate::domain::{Error, Result};

pub fn parse(value: &str) -> Result<Vec<String>> {
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(Error::Validation("启动参数不能包含换行或控制字符".into()));
    }
    let chars: Vec<_> = value.chars().collect();
    let (mut index, mut quoted, mut started) = (0, false, false);
    let mut argument = String::new();
    let mut arguments = Vec::new();
    while index < chars.len() {
        match chars[index] {
            '\\' => {
                let start = index;
                while index < chars.len() && chars[index] == '\\' {
                    index += 1;
                }
                let count = index - start;
                if chars.get(index) == Some(&'"') {
                    argument.extend(std::iter::repeat_n('\\', count / 2));
                    started = true;
                    if !count.is_multiple_of(2) {
                        argument.push('"');
                        index += 1;
                    }
                    // Even backslashes leave the quote for the normal quote rules.
                } else {
                    argument.extend(std::iter::repeat_n('\\', count));
                    started = true;
                }
            }
            '"' => {
                started = true;
                if quoted && chars.get(index + 1) == Some(&'"') {
                    argument.push('"');
                    index += 2;
                } else {
                    quoted = !quoted;
                    index += 1;
                }
            }
            ' ' | '\t' if !quoted => {
                if started {
                    arguments.push(std::mem::take(&mut argument));
                    started = false;
                }
                index += 1;
            }
            c => {
                argument.push(c);
                started = true;
                index += 1;
            }
        }
    }
    if quoted {
        return Err(Error::Validation(
            "启动参数的双引号未闭合，请补全双引号".into(),
        ));
    }
    if started {
        arguments.push(argument);
    }
    Ok(arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_multiple_quoted_empty_unicode_and_windows_path_arguments() {
        assert!(parse(" \t").unwrap().is_empty());
        assert_eq!(
            parse(r#"-dx11 --profile "含空格 名称" "" --path "C:\Game Saves\\""#).unwrap(),
            vec![
                "-dx11",
                "--profile",
                "含空格 名称",
                "",
                "--path",
                "C:\\Game Saves\\"
            ]
        );
        assert_eq!(
            parse(r#"--name="a b" a\"b "a""b""#).unwrap(),
            vec!["--name=a b", "a\"b", "a\"b"]
        );
    }
    #[test]
    fn rejects_unfinished_quotes_and_controls_without_interpreting_shell_syntax() {
        for value in ["\"oops", "arg \"oops", "\"trailing\\\""] {
            assert!(parse(value)
                .unwrap_err()
                .to_string()
                .contains("双引号未闭合"));
        }
        for value in ["a\0b", "a\nb", "a\rb"] {
            assert!(parse(value).is_err());
        }
        assert_eq!(
            parse("& > %PATH% $(whoami) 'a b'").unwrap(),
            vec!["&", ">", "%PATH%", "$(whoami)", "'a", "b'"]
        );
    }
}
