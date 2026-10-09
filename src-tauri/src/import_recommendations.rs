//! Batch-local candidate index and bounded evidence. Never chooses a fuzzy update automatically.
use super::{import_matching::Name, Match};
use crate::{
    domain::{Game, ScanCandidate},
    paths, save_detection, version,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

struct Prepared<'a> {
    game: &'a Game,
    names: Vec<Name>,
}

pub struct MatchIndex<'a> {
    games: Vec<Prepared<'a>>,
    postings: HashMap<String, Vec<usize>>,
    word_frequency: HashMap<String, usize>,
    identities: HashMap<(String, String, Option<String>), Vec<String>>,
}

fn distinctive_exe(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if !Path::new(&path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return None;
    }
    let stem = Path::new(&path).file_stem()?.to_str()?;
    let stem = paths::normalize_alias(stem);
    let stem = stem
        .strip_suffix("-32")
        .or_else(|| stem.strip_suffix("-64"))
        .or_else(|| stem.strip_suffix("_32"))
        .or_else(|| stem.strip_suffix("_64"))
        .unwrap_or(&stem);
    let stem =
        version::simple_version(stem).map_or_else(|| stem.to_owned(), |v| stem.replacen(&v, "", 1));
    let stem: String = stem.chars().filter(|c| c.is_alphanumeric()).collect();
    (stem.chars().count() >= 6
        && ![
            "game",
            "nw",
            "launcher",
            "launch",
            "launchgame",
            "gamelauncher",
            "start",
            "renpy",
            "python",
            "pythonw",
            "uninstall",
            "unins000",
            "keyconfig",
            "settings",
        ]
        .contains(&stem.as_str()))
    .then_some(stem)
}

fn candidate_exe(candidate: &ScanCandidate) -> Option<&str> {
    let best = candidate.executables.first()?;
    if candidate.executables.len() == 1
        || candidate
            .executables
            .get(1)
            .is_some_and(|next| best.score > next.score)
    {
        Some(&best.relative_path)
    } else {
        None
    }
}

fn external_saves(paths: &[String]) -> HashSet<String> {
    paths
        .iter()
        .filter_map(|path| {
            let normalized = paths::normalize_alias(path).replace('\\', "/");
            let last = normalized.rsplit('/').next().unwrap_or("");
            (!normalized.contains("<game>")
                && (normalized.starts_with('%') || Path::new(path).is_absolute())
                && last.chars().count() >= 6
                && ![
                    "save",
                    "saves",
                    "savedata",
                    "savegames",
                    "renpy",
                    "unity",
                    "games",
                ]
                .contains(&last))
            .then_some(normalized)
        })
        .collect()
}

impl<'a> MatchIndex<'a> {
    pub fn new(games: &'a [Game]) -> Self {
        let mut index = Self {
            games: Vec::new(),
            postings: HashMap::new(),
            word_frequency: HashMap::new(),
            identities: HashMap::new(),
        };
        for (position, game) in games.iter().enumerate() {
            let folder = game.install_path.replace('\\', "/");
            let folder = Path::new(&folder)
                .file_name()
                .map(|part| part.to_string_lossy());
            let names: Vec<_> = [&game.canonical_title, &game.display_title]
                .into_iter()
                .chain(game.aliases.iter())
                .map(String::as_str)
                .chain(folder.as_deref())
                .map(Name::new)
                .collect();
            let keys: HashSet<_> = names.iter().flat_map(Name::keys).collect();
            for key in keys {
                index.postings.entry(key).or_default().push(position);
            }
            let words: HashSet<_> = names
                .iter()
                .flat_map(|name| name.words().iter().cloned())
                .collect();
            for word in words {
                *index.word_frequency.entry(word).or_default() += 1;
            }
            index.games.push(Prepared { game, names });
        }
        index
    }

    fn identity(&mut self, root: &str, work: &str, exe: Option<&str>) -> Vec<String> {
        self.identities
            .entry((root.to_owned(), work.to_owned(), exe.map(str::to_owned)))
            .or_insert_with(|| save_detection::matching_identity(Path::new(root), work, exe))
            .clone()
    }

    pub fn matches(
        &mut self,
        candidate: &ScanCandidate,
        cancelled: &impl Fn() -> bool,
    ) -> Vec<Match> {
        let incoming = Name::new(&candidate.suggested_title);
        let positions: HashSet<_> = incoming
            .keys()
            .iter()
            .filter_map(|key| self.postings.get(key))
            .flatten()
            .copied()
            .collect();
        let mut scores = Vec::new();
        for position in positions {
            if cancelled() {
                return vec![];
            }
            let prepared = &self.games[position];
            let Some((mut score, mut reason)) = prepared
                .names
                .iter()
                .filter_map(|name| incoming.proposal(name))
                .max_by_key(|(score, _)| *score)
            else {
                continue;
            };
            if score < 95 {
                // Distinctive words help ranking, but can't establish a match by themselves.
                if incoming.words().iter().any(|word| {
                    word.len() >= 6
                        && !["princess", "school", "sister", "adventure"].contains(&word.as_str())
                        && self
                            .word_frequency
                            .get(word)
                            .is_some_and(|count| *count <= 2)
                }) {
                    score += 3;
                }
                let old_exe = prepared
                    .game
                    .mtool_target_exe
                    .as_deref()
                    .or(prepared.game.main_executable.as_deref());
                if candidate_exe(candidate)
                    .and_then(distinctive_exe)
                    .zip(old_exe.and_then(distinctive_exe))
                    .is_some_and(|(a, b)| a == b)
                {
                    score += 10;
                    reason.push_str(" · 特色启动文件一致");
                }
                if !external_saves(&candidate.save_paths)
                    .is_disjoint(&external_saves(&prepared.game.save_paths))
                {
                    score += 12;
                    reason.push_str(" · 游戏外部存档路径一致");
                }
                score = score.min(94);
            }
            scores.push((position, score, reason));
        }
        scores.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| self.games[a.0].game.id.cmp(&self.games[b.0].game.id))
        });
        // Enrich only the leading relaxed candidates; one read per game in this batch.
        let relaxed: Vec<_> = scores
            .iter()
            .filter(|(_, score, _)| *score < 95)
            .take(5)
            .map(|(position, _, _)| *position)
            .collect();
        if !relaxed.is_empty() && !cancelled() {
            let identity = self.identity(
                &candidate.install_path,
                &candidate.working_directory,
                candidate_exe(candidate),
            );
            if !identity.is_empty() {
                for position in relaxed {
                    if cancelled() {
                        return vec![];
                    }
                    let game = self.games[position].game;
                    let old_identity = self.identity(
                        &game.install_path,
                        &game.working_directory,
                        game.mtool_target_exe
                            .as_deref()
                            .or(game.main_executable.as_deref()),
                    );
                    if identity.iter().any(|key| old_identity.contains(key)) {
                        let (_, score, reason) =
                            scores.iter_mut().find(|(p, _, _)| *p == position).unwrap();
                        *score = (*score + 16).min(94);
                        reason.push_str(" · 本地项目/存档标识一致");
                    }
                }
            }
        }
        scores.retain(|(_, score, _)| *score >= 68);
        scores.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| self.games[a.0].game.id.cmp(&self.games[b.0].game.id))
        });
        let ambiguous = scores
            .get(1)
            .is_some_and(|second| scores[0].1.saturating_sub(second.1) < 8 || second.1 >= 95);
        scores
            .into_iter()
            .take(5)
            .map(|(position, score, mut reason)| {
                let game = self.games[position].game;
                if ambiguous {
                    reason.push_str(" · 多个候选接近，请手动选择");
                }
                if candidate.engine == "QSP" && game.engine == "QSP" {
                    reason.push_str(" · 同为 QSP");
                }
                Match {
                    id: game.id.clone(),
                    title: game.display_title.clone(),
                    version: game.current_version.clone(),
                    path: game.install_path.clone(),
                    reason,
                    auto_associate: score >= 95 && !ambiguous,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{ExeCandidate, PlayStatus},
        scanner,
    };
    use std::{fs, time::Instant};

    fn game(id: &str, title: &str, root: &Path) -> Game {
        Game {
            id: id.into(),
            canonical_title: title.into(),
            display_title: title.into(),
            install_path: root.to_string_lossy().into_owned(),
            working_directory: ".".into(),
            current_version: "v1.0".into(),
            version_source: "manual".into(),
            main_executable: None,
            engine: "Unknown".into(),
            launch_type: "DIRECT".into(),
            external_player: None,
            mtool_target_exe: None,
            mtool_loader: None,
            created_at: String::new(),
            updated_at: String::new(),
            last_launched_at: None,
            play_status: PlayStatus::Unplayed,
            aliases: vec![],
            save_paths: vec![],
        }
    }
    fn exe(path: &str) -> ExeCandidate {
        ExeCandidate {
            relative_path: path.into(),
            architecture: "unknown".into(),
            score: 100,
            size_bytes: 0,
            modified_ms: 0,
        }
    }
    fn options(root: &Path, contents: &str) {
        fs::create_dir_all(root.join("game")).unwrap();
        fs::write(root.join("game/options.rpy"), contents).unwrap();
    }

    #[test]
    fn identities_rank_candidates_cache_reads_and_do_not_bypass_name_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        let incoming = temp.path().join("ReSister 入库新名称 v1.2");
        let correct = temp.path().join("correct");
        let unrelated = temp.path().join("unrelated");
        let opts = "define config.save_directory = 'ReSisterProject-12345'\ndefine build.name = 'ReSister'";
        options(&incoming, opts);
        options(&correct, opts);
        options(
            &unrelated,
            "define config.save_directory = 'OtherProject-12345'",
        );
        let games = [
            game("correct", "ReSister 原版名称 v1.0", &correct),
            game("unrelated", "ReSister 其他名称 v1.0", &unrelated),
        ];
        let mut candidate = scanner::pending_candidate(&incoming).unwrap();
        candidate.save_paths = vec!["<GAME>/game/saves".into()];
        let mut index = MatchIndex::new(&games);
        let matches = index.matches(&candidate, &|| false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "correct");
        assert!(!matches[0].auto_associate);
        assert!(matches[0].reason.contains("标识一致"));
        let reads = index.identities.len();
        fs::write(correct.join("game/options.rpy"), "changed after snapshot").unwrap();
        assert_eq!(index.matches(&candidate, &|| false)[0].id, "correct");
        assert_eq!(index.identities.len(), reads);
        assert!(MatchIndex::new(&games)
            .matches(&candidate, &|| false)
            .is_empty());
        candidate.suggested_title = "ReSister 2 v1.2".into();
        assert!(index.matches(&candidate, &|| false).is_empty());
        assert!(index.matches(&candidate, &|| true).is_empty());
    }

    #[test]
    fn distinctive_exes_and_saves_help_but_generic_paths_never_establish_identity() {
        let temp = tempfile::tempdir().unwrap();
        let mut old = game("old", "DistinctiveTitle 旧名称", temp.path());
        let mut candidate =
            scanner::pending_candidate(&temp.path().join("DistinctiveTitle 新名称")).unwrap();
        for (exe_name, expected) in [
            ("Game.exe", false),
            ("nw.exe", false),
            ("launcher.exe", false),
            ("DistinctiveTitle-32.exe", true),
        ] {
            old.main_executable = Some(exe_name.into());
            candidate.executables = vec![exe(if expected {
                "DistinctiveTitle-64.exe"
            } else {
                exe_name
            })];
            let matched =
                MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
            assert_eq!(!matched.is_empty(), expected, "{exe_name}");
            if expected {
                assert!(!matched[0].auto_associate);
            }
        }
        candidate.executables.clear();
        old.main_executable = None;
        for (save, expected) in [
            ("<GAME>/game/saves", false),
            ("%APPDATA%/RenPy/saves", false),
            ("%APPDATA%/RenPy/DistinctiveTitle-123456", true),
        ] {
            old.save_paths = vec![save.into()];
            candidate.save_paths = old.save_paths.clone();
            assert_eq!(
                !MatchIndex::new(std::slice::from_ref(&old))
                    .matches(&candidate, &|| false)
                    .is_empty(),
                expected,
                "{save}"
            );
        }
        assert_eq!(old.engine, "Unknown");
    }

    #[test]
    fn ambiguous_exact_matches_are_bounded_and_never_auto_associate() {
        let temp = tempfile::tempdir().unwrap();
        let mut games: Vec<_> = (0..9)
            .map(|n| {
                game(
                    &n.to_string(),
                    "Exact Title v1.2",
                    &temp.path().join(n.to_string()),
                )
            })
            .collect();
        let candidate = scanner::pending_candidate(&temp.path().join("Exact Title v1.3")).unwrap();
        let mut index = MatchIndex::new(&games);
        let results = index.matches(&candidate, &|| false);
        assert_eq!(results.len(), 5);
        assert!(results
            .iter()
            .all(|matched| !matched.auto_associate && matched.reason.contains("多个候选")));
        games.truncate(1);
        assert!(MatchIndex::new(&games).matches(&candidate, &|| false)[0].auto_associate);
        games[0].aliases.push("完全不同的已确认标题 v1.3".into());
        let alias =
            scanner::pending_candidate(&temp.path().join("完全不同的已确认标题 v1.4")).unwrap();
        assert!(MatchIndex::new(&games).matches(&alias, &|| false)[0].auto_associate);
    }

    #[test]
    fn batch_index_keeps_correct_candidates_on_500_and_2000_game_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let cases = [
            ("妹系罗盘～在无人岛的悠闲慢活～シスターズコンパス～妹たちと無人島でラブラブスローライフ", "姊妹指南针～与姊妹们在无人岛上的甜蜜慢生活～シスターズコンパス～ AI汉化 Ver1.02"),
            ("鸣人的假期 v1.0 内部赞助付费完结版", "鸣人的假期 v1.0"),
            ("My Best Deal v4.8 女神的最佳交易", "My_Best_Deal-4.3.5-pc"),
            ("ReSister ― 与妹妹两人的秘密同居生活 ― v1.1.0 DLC", "ReSister-与妹妹的秘密同居生活 v1.03"),
        ];
        for count in [500, 2000] {
            let mut games: Vec<_> = (0..count)
                .map(|n| {
                    game(
                        &format!("other-{n}"),
                        &format!("Other Example Game {n} v1.0"),
                        &temp.path().join(format!("other-{n}")),
                    )
                })
                .collect();
            for (n, (_, title)) in cases.iter().enumerate() {
                games[n] = game(
                    &format!("case-{n}"),
                    title,
                    &temp.path().join(format!("case-{n}")),
                );
            }
            let started = Instant::now();
            let mut index = MatchIndex::new(&games);
            let prepared = started.elapsed();
            let queries = Instant::now();
            for _ in 0..25 {
                for (n, (title, _)) in cases.iter().enumerate() {
                    let candidate = scanner::pending_candidate(&temp.path().join(title)).unwrap();
                    let matched = index.matches(&candidate, &|| false);
                    assert_eq!(matched[0].id, format!("case-{n}"));
                    assert!(!matched[0].auto_associate);
                    assert!(matched.len() <= 5);
                    if n == 3 {
                        assert!(matched[0].reason.contains("DLC"));
                    }
                }
            }
            // Missing source metadata is memoized too; no repeated directory reads.
            assert_eq!(index.identities.len(), 4);
            eprintln!(
                "matching fixture: {count} games, prepare={prepared:?}, 100 queries={:?}",
                queries.elapsed()
            );
        }
    }

    #[test]
    fn static_identity_handles_multiple_engines_and_rejects_expressions_oversize_and_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        options(root, "define config.save_directory = 'TrustedGame-123456'\ndefine build.name = 'TrustedGame'");
        fs::write(
            root.join("project.godot"),
            "[application]\nconfig/name=\"TrustedGodot\"\n[other]\nconfig/name=\"irrelevant\"",
        )
        .unwrap();
        fs::create_dir(root.join("TrustedGame_Data")).unwrap();
        fs::write(
            root.join("TrustedGame_Data/app.info"),
            "TrustedCompany\nTrustedGame\n",
        )
        .unwrap();
        let keys = save_detection::matching_identity(root, ".", Some("TrustedGame.exe"));
        assert_eq!(keys.len(), 4);
        assert!(keys.iter().any(|key| key.starts_with("unity:")));
        assert!(keys.iter().any(|key| key.starts_with("godot:")));
        assert!(save_detection::matching_identity(root, "../escape", None).is_empty());
        fs::remove_file(root.join("project.godot")).unwrap();
        for contents in ["define config.save_directory = title + '-12345'", "define config.save_directory = 'Game'", "define config.save_directory = 'FirstGame'\ndefine config.save_directory = 'SecondGame'", "define config.save_directory = 'TrustedGame'\nconfig.savedir = custom_dir"] {
            options(root, contents);
            assert!(save_detection::matching_identity(root, ".", None).is_empty(), "{contents}");
        }
        options(
            root,
            &format!(
                "define config.save_directory = 'TrustedGame'\n{}",
                "#".repeat(256 * 1024)
            ),
        );
        assert!(save_detection::matching_identity(root, ".", None).is_empty());
        #[cfg(windows)]
        {
            fs::remove_dir_all(root.join("game")).unwrap();
            let outside = tempfile::tempdir().unwrap();
            fs::write(
                outside.path().join("options.rpy"),
                "define config.save_directory = 'ForeignGame'",
            )
            .unwrap();
            let linked = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(root.join("game"))
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(linked.status.success());
            assert!(save_detection::matching_identity(root, ".", None).is_empty());
            fs::remove_dir(root.join("game")).unwrap();
        }
    }
}
