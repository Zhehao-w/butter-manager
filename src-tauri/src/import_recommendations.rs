//! Batch-local candidate index and bounded evidence. Never chooses a fuzzy update automatically.
use super::{import_matching::Name, import_memory::MemoryIndex, ImportMemory, Match};
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
    game_positions: HashMap<String, usize>,
    postings: HashMap<String, Vec<usize>>,
    word_frequency: HashMap<String, usize>,
    identities: HashMap<(String, String, Option<String>), Vec<String>>,
    memories: MemoryIndex,
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

fn auxiliary_score(
    score: &mut u8,
    reason: &mut String,
    exe: Option<&str>,
    saves: &[String],
    old: &Game,
) {
    let (same_exe, same_saves) = auxiliary_evidence(exe, saves, old);
    if same_exe {
        *score += 10;
        reason.push_str(" · 特色启动文件一致");
    }
    if same_saves {
        *score += 12;
        reason.push_str(" · 游戏外部存档路径一致");
    }
    *score = (*score).min(94);
}

fn auxiliary_evidence(exe: Option<&str>, saves: &[String], old: &Game) -> (bool, bool) {
    let old_exe = old
        .mtool_target_exe
        .as_deref()
        .or(old.main_executable.as_deref());
    let same_exe = exe
        .and_then(distinctive_exe)
        .zip(old_exe.and_then(distinctive_exe))
        .is_some_and(|(a, b)| a == b);
    (
        same_exe,
        !external_saves(saves).is_disjoint(&external_saves(&old.save_paths)),
    )
}

impl<'a> MatchIndex<'a> {
    pub fn new(games: &'a [Game]) -> Self {
        let mut index = Self {
            games: Vec::new(),
            game_positions: HashMap::new(),
            postings: HashMap::new(),
            word_frequency: HashMap::new(),
            identities: HashMap::new(),
            memories: MemoryIndex::default(),
        };
        for (position, game) in games.iter().enumerate() {
            index.game_positions.insert(game.id.clone(), position);
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

    pub fn with_memories(games: &'a [Game], memories: &[ImportMemory]) -> Self {
        let mut index = Self::new(games);
        index.memories = MemoryIndex::new(memories);
        index
    }

    fn identity(&mut self, root: &str, work: &str, exe: Option<&str>) -> Vec<String> {
        self.identities
            .entry((root.to_owned(), work.to_owned(), exe.map(str::to_owned)))
            .or_insert_with(|| save_detection::matching_identity(Path::new(root), work, exe))
            .clone()
    }

    fn distinctive_word(&self, incoming: &Name) -> bool {
        incoming.words().iter().any(|word| {
            word.len() >= 6
                && !["princess", "school", "sister", "adventure"].contains(&word.as_str())
                && self
                    .word_frequency
                    .get(word)
                    .is_some_and(|count| *count <= 2)
        })
    }

    /// Uses the same prepared names, postings and auxiliary metadata as Import, without
    /// local identity enrichment, the five-result limit, or automatic association.
    pub fn library_pairs(
        &self,
        cancelled: &impl Fn() -> bool,
        progress: &impl Fn(&str),
    ) -> crate::domain::Result<Vec<crate::library_duplicates::DuplicatePair>> {
        let mut pairs = Vec::new();
        for (left, incoming) in self.games.iter().enumerate() {
            let positions: HashSet<_> = incoming
                .names
                .iter()
                .flat_map(Name::keys)
                .filter_map(|key| self.postings.get(&key))
                .flatten()
                .copied()
                .filter(|position| *position > left)
                .collect();
            for position in positions {
                if cancelled() {
                    return Err(crate::domain::Error::Validation("重复检查已取消".into()));
                }
                let old = &self.games[position];
                let exe = incoming
                    .game
                    .mtool_target_exe
                    .as_deref()
                    .or(incoming.game.main_executable.as_deref());
                let (same_exe, same_saves) =
                    auxiliary_evidence(exe, &incoming.game.save_paths, old.game);
                let supported = same_exe || same_saves;
                if incoming.game.id == old.game.id
                    || incoming
                        .names
                        .iter()
                        .take(2)
                        .zip(old.names.iter().take(2))
                        .any(|(a, b)| {
                            a.library_conflict(b)
                                && a.package_proposal(b).is_none()
                                && !(supported && a.metadata_proposal(b).is_some())
                        })
                {
                    continue;
                }
                let Some((mut score, mut reason)) = incoming
                    .names
                    .iter()
                    .filter(|name| name.useful_library_name())
                    .flat_map(|name| {
                        old.names
                            .iter()
                            .filter(|other| other.useful_library_name())
                            .filter_map(move |other| {
                                name.proposal(other).or_else(|| {
                                    supported.then(|| name.metadata_proposal(other)).flatten()
                                })
                            })
                    })
                    .max_by_key(|(score, _)| *score)
                else {
                    continue;
                };
                if score < 95 {
                    if incoming
                        .names
                        .iter()
                        .any(|name| self.distinctive_word(name))
                    {
                        score += 3;
                    }
                    auxiliary_score(
                        &mut score,
                        &mut reason,
                        incoming
                            .game
                            .mtool_target_exe
                            .as_deref()
                            .or(incoming.game.main_executable.as_deref()),
                        &incoming.game.save_paths,
                        old.game,
                    );
                }
                if score < 68 {
                    continue;
                }
                if incoming.game.engine != old.game.engine
                    && incoming.game.engine != "Unknown"
                    && old.game.engine != "Unknown"
                {
                    score = score.min(94);
                    reason.push_str(" · 引擎不同，请确认是否为不同作品或移植版本");
                }
                if incoming.game.current_version != old.game.current_version {
                    reason.push_str(" · 记录版本不同");
                }
                let (a, b) =
                    crate::library_duplicates::ordered_pair(&incoming.game.id, &old.game.id)?;
                pairs.push(crate::library_duplicates::DuplicatePair {
                    left_id: a.into(),
                    right_id: b.into(),
                    score,
                    confidence: if score >= 95 { "high" } else { "review" }.into(),
                    reason,
                });
            }
            if cancelled() {
                return Err(crate::domain::Error::Validation("重复检查已取消".into()));
            }
            progress(&incoming.game.id);
        }
        pairs.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.left_id.cmp(&b.left_id))
                .then_with(|| a.right_id.cmp(&b.right_id))
        });
        Ok(pairs)
    }

    pub fn matches(
        &mut self,
        candidate: &ScanCandidate,
        cancelled: &impl Fn() -> bool,
    ) -> Vec<Match> {
        let mut normal = self.name_matches(candidate, cancelled);
        if cancelled() {
            return vec![];
        }
        let identities = if self.memories.needs_identity() {
            self.identity(
                &candidate.install_path,
                &candidate.working_directory,
                candidate_exe(candidate),
            )
        } else {
            vec![]
        };
        let incoming = Name::new(&candidate.suggested_title);
        let mut remembered: Vec<_> = self
            .memories
            .matches(&candidate.suggested_title, &identities)
            .into_iter()
            .filter_map(|(id, reason)| {
                let game = self.games[*self.game_positions.get(&id)?].game;
                if incoming.memory_conflict(&Name::new(&game.canonical_title)) {
                    return None;
                }
                Some(Match {
                    id,
                    title: game.display_title.clone(),
                    version: game.current_version.clone(),
                    path: game.install_path.clone(),
                    reason,
                    auto_associate: false,
                    remembered: true,
                })
            })
            .collect();
        if remembered.is_empty() {
            return normal;
        }
        if remembered.len() > 1
            || normal
                .iter()
                .any(|matched| !remembered.iter().any(|memory| memory.id == matched.id))
        {
            for matched in &mut remembered {
                matched.reason.push_str(" · 多个候选，请手动选择");
            }
        }
        normal.retain(|matched| !remembered.iter().any(|memory| memory.id == matched.id));
        for matched in &mut normal {
            matched.auto_associate = false;
        }
        remembered.extend(normal);
        remembered.truncate(5);
        if cancelled() {
            vec![]
        } else {
            remembered
        }
    }

    fn name_matches(
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
            let (same_exe, same_saves) = auxiliary_evidence(
                candidate_exe(candidate),
                &candidate.save_paths,
                prepared.game,
            );
            let supported = same_exe || same_saves;
            let Some((mut score, mut reason)) = prepared
                .names
                .iter()
                .filter_map(|name| {
                    incoming
                        .proposal(name)
                        .or_else(|| {
                            supported
                                .then(|| incoming.metadata_proposal(name))
                                .flatten()
                        })
                        .or_else(|| incoming.legacy_numeric_proposal(name))
                        .or_else(|| incoming.import_proposal(name, supported))
                })
                .max_by_key(|(score, _)| *score)
            else {
                continue;
            };
            if score < 95 {
                // Distinctive words help ranking, but can't establish a match by themselves.
                if self.distinctive_word(&incoming) {
                    score += 3;
                }
                auxiliary_score(
                    &mut score,
                    &mut reason,
                    candidate_exe(candidate),
                    &candidate.save_paths,
                    prepared.game,
                );
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
                    remembered: false,
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

    #[test]
    fn memory_recalls_renamed_uuid_deduplicates_and_requires_confirmation_even_against_exact_name()
    {
        let temp = tempfile::tempdir().unwrap();
        let mut candidate = scanner::pending_candidate(temp.path()).unwrap();
        candidate.suggested_title = "Moonlight Garden v0.8 汉化版".into();
        let profile =
            super::super::MemoryProfile::from_candidate(&candidate, "Moonlight.exe").unwrap();
        let memory = ImportMemory {
            id: "memory".into(),
            game_id: "old".into(),
            profile,
            confirmed_at: String::new(),
        };
        let old = game("old", "月光庭院", Path::new("Z:/unavailable/moved"));
        candidate.suggested_title = "Moonlight Garden v0.9 汉化版".into();
        assert!(MatchIndex::new(std::slice::from_ref(&old))
            .matches(&candidate, &|| false)
            .is_empty());
        let exact = game(
            "exact",
            &candidate.suggested_title,
            Path::new("Z:/unavailable/another"),
        );
        let games = [old, exact];
        let mut index = MatchIndex::with_memories(&games, std::slice::from_ref(&memory));
        let results = index.matches(&candidate, &|| false);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "old");
        assert!(results[0].remembered);
        assert!(results.iter().all(|result| !result.auto_associate));
        assert!(index.matches(&candidate, &|| true).is_empty());
        assert!(MatchIndex::with_memories(&[], &[memory])
            .matches(&candidate, &|| false)
            .is_empty());
        let mut renamed = games[0].clone();
        renamed.canonical_title = candidate.suggested_title.clone();
        let remembered = ImportMemory {
            id: "memory".into(),
            game_id: "old".into(),
            profile: super::super::MemoryProfile::from_candidate(&candidate, "Moonlight.exe")
                .unwrap(),
            confirmed_at: String::new(),
        };
        let results =
            MatchIndex::with_memories(&[renamed], &[remembered]).matches(&candidate, &|| false);
        assert_eq!(results.len(), 1);
        assert!(results[0].remembered && !results[0].auto_associate);
        assert!(games[0].aliases.is_empty());
    }

    #[test]
    fn remembered_project_identity_recalls_without_a_common_title_and_preserves_links_and_collisions(
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Moonlight Garden v0.8");
        fs::create_dir_all(root.join("Moonlight_Data")).unwrap();
        fs::write(root.join("Moonlight.exe"), b"synthetic").unwrap();
        fs::write(
            root.join("Moonlight_Data/app.info"),
            "Fixture Studio\nMoonlight Garden\n",
        )
        .unwrap();
        let mut candidate = scanner::pending_candidate(&root).unwrap();
        candidate.suggested_title = "Moonlight Garden v0.8".into();
        candidate.executables = vec![exe("Moonlight.exe")];
        let profile =
            super::super::MemoryProfile::from_candidate(&candidate, "Moonlight.exe").unwrap();
        let memories: Vec<_> = ["a", "b"]
            .iter()
            .map(|id| ImportMemory {
                id: (*id).into(),
                game_id: (*id).into(),
                profile: profile.clone(),
                confirmed_at: String::new(),
            })
            .collect();
        candidate.suggested_title = "月光庭院 v0.9".into();
        let games = [
            game("a", "曾经的译名", Path::new("missing/a")),
            game("b", "另一条记录", Path::new("missing/b")),
        ];
        let results = MatchIndex::with_memories(&games, &memories).matches(&candidate, &|| false);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|matched| matched.remembered
            && !matched.auto_associate
            && matched.reason.contains("多个候选")));
        candidate.suggested_title = "Game v0.9".into();
        assert!(super::super::MemoryProfile::from_candidate(&candidate, "Moonlight.exe").is_none());
        let results = MatchIndex::with_memories(&games, &memories).matches(&candidate, &|| false);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|matched| matched.remembered
            && !matched.auto_associate
            && matched.reason.contains("静态项目标识一致")
            && matched.reason.contains("多个候选")));
        fs::remove_file(root.join("Moonlight_Data/app.info")).unwrap();
        assert!(MatchIndex::with_memories(&games, &memories)
            .matches(&candidate, &|| false)
            .is_empty());
        // A generic executable by itself never becomes a memory identity.
        candidate.executables = vec![exe("Game.exe")];
        assert!(MatchIndex::with_memories(&games, &memories)
            .matches(&candidate, &|| false)
            .is_empty());
    }

    #[test]
    fn memory_batch_index_handles_500_and_2000_games_without_learning_or_unbounded_results() {
        for size in [500, 2000] {
            let temp = tempfile::tempdir().unwrap();
            let mut candidate = scanner::pending_candidate(temp.path()).unwrap();
            let games: Vec<_> = (0..size)
                .map(|n| {
                    game(
                        &format!("game-{n}"),
                        &format!("库内游戏 {n}"),
                        Path::new("missing"),
                    )
                })
                .collect();
            let memories: Vec<_> = (0..size)
                .map(|n| {
                    let title = format!("Incoming Title {n} v0.8 汉化版");
                    ImportMemory {
                        id: format!("memory-{n}"),
                        game_id: format!("game-{n}"),
                        profile: super::super::MemoryProfile {
                            format_version: 1,
                            name_key: Name::new(&title).memory_key().unwrap(),
                            title,
                            identities: vec![],
                        },
                        confirmed_at: String::new(),
                    }
                })
                .collect();
            let started = Instant::now();
            let mut index = MatchIndex::with_memories(&games, &memories);
            let prepared = started.elapsed();
            for n in 0..100 {
                candidate.suggested_title = format!("Incoming Title {n} v0.9 汉化版");
                let results = index.matches(&candidate, &|| false);
                assert_eq!(results[0].id, format!("game-{n}"));
                assert!(
                    results.len() <= 5 && results.iter().all(|matched| !matched.auto_associate)
                );
            }
            assert_eq!(memories.len(), size);
            eprintln!(
                "memory fixture: {size} games, prepare={prepared:?}, 100 queries={:?}",
                started.elapsed() - prepared
            );
        }
    }

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
            favorite: false,
            tags: vec![],
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
    fn subtitle_revision_and_repeated_alias_imports_offer_manual_recommendations() {
        let mut missing = Vec::new();
        for (old_title, incoming_title, executable) in [
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～NTR客房的秘密～ V1.02 官中动态步兵正式版",
                "MushroomsIsland.exe",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL 无限：爱或欲望",
                "InfinityLOL.exe",
            ),
            (
                "ReSister — 与妹妹两人的秘密同居生活 — v1.1.0 DLC",
                "ReSister — 与妹妹两人的秘密同居生活— ReSister—妹とふたり、ひみつの同棲生活— v1.1.1",
                "ReSister.exe",
            ),
        ] {
            let mut old = game("old", old_title, Path::new("Z:/missing/old"));
            old.main_executable = Some(executable.into());
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = incoming_title.into();
            candidate.executables = vec![exe(executable)];
            let results = MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
            if results.len() != 1 {
                missing.push(format!("{old_title} / {incoming_title}: {} results", results.len()));
            }
            assert!(results.iter().all(|r| r.id == "old" && !r.auto_associate));
            if old_title.contains("DLC") && !results.is_empty() {
                assert!(results[0].reason.contains("DLC"));
            }
            let mut incoming = game("incoming", incoming_title, Path::new("Z:/missing/new"));
            incoming.main_executable = old.main_executable.clone();
            assert!(MatchIndex::new(&[old, incoming])
                .library_pairs(&|| false, &|_| {})
                .unwrap().is_empty(), "library rules must stay conservative: {incoming_title}");
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    #[test]
    fn new_import_title_rules_preserve_sequels_chapters_editions_and_original_titles() {
        for (old_title, incoming_title) in [
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～第二章：客房的秘密～ V1.02 官中动态步兵正式版",
            ),
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～外传：客房的秘密～ V1.02",
            ),
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～NTR客房的秘密～ V1.02 Demo",
            ),
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～NTR客房的秘密～ V1.02 Remastered",
            ),
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～NTR客房的秘密2～ V1.02",
            ),
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛冒险日记～NTR客房的秘密～ V1.02",
            ),
            ("蘑菇岛打工日记V1.0.2", "蘑菇岛打工日记NTR客房的秘密 V1.02"),
            (
                "蘑菇岛打工日记～海边的休假旅行～V1.0.2",
                "蘑菇岛打工日记～NTR客房的秘密～ V1.02",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL II",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL Season2",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL Ch2Up1",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL Demo",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 OtherProject",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust ProjectR25 InfinityLOL",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 R26 InfinityLOL",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL 2",
            ),
            ("Adventure-R15-pc", "A Different Adventure R25"),
            (
                "ReSister 与妹妹两人的秘密同居生活 v1.1.0 DLC",
                "ReSister 与妹妹两人的秘密同居生活 ReSister II v1.1.1",
            ),
            (
                "ReSister 与妹妹两人的秘密同居生活 v1.1.0 DLC",
                "ReSister 与妹妹两人的秘密同居生活 ReSister Demo v1.1.1",
            ),
            (
                "ReSister 与妹妹两人的秘密同居生活 v1.1.0 DLC",
                "ReSister 完全不同的冒险生活 ReSister v1.1.1",
            ),
            (
                "ReSister 与妹妹两人的秘密同居生活 妹とふたり v1.1.0 DLC",
                "ReSister 与妹妹两人的秘密同居生活 ReSister 森へ出かける旅 v1.1.1",
            ),
        ] {
            let mut old = game("old", old_title, Path::new("Z:/missing/old"));
            old.main_executable = Some("SharedProjectLauncher.exe".into());
            old.save_paths = vec!["%APPDATA%/FixtureStudio/SharedProjectSaves".into()];
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = incoming_title.into();
            candidate.executables = vec![exe("SharedProjectLauncher.exe")];
            candidate.save_paths = old.save_paths.clone();
            assert!(
                MatchIndex::new(&[old])
                    .matches(&candidate, &|| false)
                    .is_empty(),
                "{old_title} / {incoming_title}"
            );
        }
    }

    #[test]
    fn subtitle_and_revision_abbreviations_require_distinctive_metadata() {
        for (old_title, incoming_title) in [
            (
                "蘑菇岛打工日记V1.0.2",
                "蘑菇岛打工日记～NTR客房的秘密～ V1.02 官中动态步兵正式版",
            ),
            (
                "InfinityLOL-R15-pc",
                "Infinity Love or Lust R25 InfinityLOL 无限：爱或欲望",
            ),
        ] {
            for mode in [
                "exe",
                "save",
                "generic",
                "different",
                "ambiguous",
                "missing",
            ] {
                let mut old = game("old", old_title, Path::new("Z:/missing/old"));
                let mut candidate =
                    scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
                candidate.suggested_title = incoming_title.into();
                if mode == "save" {
                    old.save_paths = vec!["%APPDATA%/FixtureStudio/DistinctiveProjectSaves".into()];
                    candidate.save_paths = old.save_paths.clone();
                } else {
                    let executable = if mode == "generic" {
                        "Game.exe"
                    } else {
                        "DistinctiveProject.exe"
                    };
                    if mode != "missing" {
                        old.main_executable = Some(executable.into());
                    }
                    candidate.executables = vec![exe(if mode == "different" {
                        "UnrelatedProject.exe"
                    } else {
                        executable
                    })];
                    if mode == "ambiguous" {
                        candidate.executables.push(exe("AnotherProject.exe"));
                    }
                    old.save_paths = vec!["<GAME>/save".into()];
                    candidate.save_paths = old.save_paths.clone();
                }
                let results = MatchIndex::new(&[old]).matches(&candidate, &|| false);
                assert_eq!(
                    results.len(),
                    usize::from(mode == "exe" || mode == "save"),
                    "{mode}: {incoming_title}"
                );
                assert!(results.iter().all(|result| !result.auto_associate));
            }
        }
        let old = game(
            "old",
            "ReSister 与妹妹两人的秘密同居生活 v1.1.0 DLC",
            Path::new("Z:/missing/old"),
        );
        let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
        candidate.suggested_title =
            "ReSister 与妹妹两人的秘密同居生活 ReSister 妹とふたり、ひみつの同棲生活 v1.1.1".into();
        let results = MatchIndex::new(&[old]).matches(&candidate, &|| false);
        assert_eq!(results.len(), 1);
        assert!(!results[0].auto_associate && results[0].reason.contains("DLC"));
    }

    #[test]
    fn revision_recommendations_remain_bounded_ambiguous_and_cancellable() {
        let games = (0..8)
            .map(|n| {
                let mut old = game(
                    &format!("game-{n}"),
                    &format!("InfinityLOL-R{}-pc", 10 + n),
                    Path::new("Z:/missing/old"),
                );
                old.main_executable = Some("InfinityLOL.exe".into());
                old
            })
            .collect::<Vec<_>>();
        let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
        candidate.suggested_title = "Infinity Love or Lust R25 InfinityLOL 无限：爱或欲望".into();
        candidate.executables = vec![exe("InfinityLOL.exe")];
        let mut index = MatchIndex::new(&games);
        let results = index.matches(&candidate, &|| false);
        assert_eq!(results.len(), 5);
        assert!(results
            .iter()
            .all(|r| !r.auto_associate && r.reason.contains("多个候选")));
        assert!(index.matches(&candidate, &|| true).is_empty());
    }

    #[test]
    fn bilingual_legacy_numeric_suffixes_offer_import_confirmation_only() {
        let title = "断角魔王艾莉丝的重生谭 The Rebirth Tale of Broken Horn Demon Lord Alice";
        let old = game("old", &format!("{title} 072"), Path::new("Z:/missing/old"));
        for suffix in [
            "官中无修版",
            "073 官中无修版",
            "0073 官中无修版",
            "v0.73 官中版",
        ] {
            let incoming_title = format!("{title} {suffix}");
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = incoming_title.clone();
            candidate.executables = vec![exe("Game.exe")];
            let matches =
                MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
            assert_eq!(matches.len(), 1, "{suffix}");
            assert_eq!(matches[0].id, "old");
            assert!(!matches[0].auto_associate);
            assert!(matches[0].reason.contains("末尾数字"));
            let incoming = game("new", &incoming_title, Path::new("Z:/missing/new"));
            assert!(MatchIndex::new(&[old.clone(), incoming])
                .library_pairs(&|| false, &|_| {})
                .unwrap()
                .is_empty());
        }
        let games = [
            old.clone(),
            game(
                "another",
                &format!("{title} 071"),
                Path::new("Z:/missing/another"),
            ),
        ];
        let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
        candidate.suggested_title = format!("{title} 官中无修版");
        let matches = MatchIndex::new(&games).matches(&candidate, &|| false);
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| !m.auto_associate));

        for incoming in [
            format!("{title} 2 官中无修版"),
            format!("{title} 72 官中无修版"),
            format!("{title} 2026 官中无修版"),
            format!("{title} 00073 官中无修版"),
            format!("{title} 073 第二章"),
            format!("{title} 073 Season 2"),
            format!("{title} 073 DLC"),
            format!("{title} 073 Demo"),
            format!("{title} 073 II"),
            format!("{title} 073 外传"),
            "另一部作品 The Rebirth Tale of Broken Horn Demon Lord Alice 073".into(),
            "断角魔王艾莉丝的重生谭 Another Broken Horn Adventure 073".into(),
            "断角魔王艾莉丝的重生谭官中无修版 The Rebirth Tale of Broken Horn Demon Lord Alice 073"
                .into(),
            "断角魔王艾莉丝的重生谭 073 The Rebirth Tale of Broken Horn Demon Lord Alice 官中无修版".into(),
        ] {
            candidate.suggested_title = incoming.clone();
            assert!(
                MatchIndex::new(std::slice::from_ref(&old))
                    .matches(&candidate, &|| false)
                    .is_empty(),
                "{incoming}"
            );
        }
    }

    #[test]
    fn demon_is_not_demo_but_explicit_edition_markers_remain_protected() {
        let title = "断角魔王艾莉丝的重生谭 The Rebirth Tale of Broken Horn Demon Lord Alice";
        let mut old = game(
            "old",
            &format!("{title} v0.72"),
            Path::new("Z:/missing/old"),
        );
        old.main_executable = Some("AliceAdventure.exe".into());
        let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
        candidate.executables = vec![exe("AliceAdventure.exe")];
        candidate.suggested_title = format!("{title} v0.73 官中无修版");
        let matches = MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].auto_associate);
        for edition in [
            "v0.73 Demo",
            "v0.73Demo1",
            "v0.73 Demo.2",
            "v0.73 Remake",
            "v0.73 Remastered",
            "v0.73 试玩版",
        ] {
            candidate.suggested_title = format!("{title} {edition} 官中无修版");
            assert!(
                MatchIndex::new(std::slice::from_ref(&old))
                    .matches(&candidate, &|| false)
                    .is_empty(),
                "{edition}"
            );
        }
        // The compact Re-package fallback must preserve qualifiers as well.
        old.canonical_title = "Supower-Re v0.72".into();
        old.display_title = old.canonical_title.clone();
        candidate.suggested_title = "Supower-Re v0.73Demo".into();
        assert!(MatchIndex::new(&[old])
            .matches(&candidate, &|| false)
            .is_empty());
    }

    #[test]
    fn compact_package_names_match_spaced_titles_in_import_and_library() {
        let mut missing = Vec::new();
        for (old_title, new_title, executable) in [
            (
                "和妹妹一起的一个月 妹と過ごす1ヵ月間 Ver2026.09 AI汉化版 去码版+全画廊",
                "和妹妹共度的一个月 妹と过ごす1ヵ月间 v26.10",
                "妹と過ごす1ヵ月間_Re.exe",
            ),
            (
                "PaleCarnations-Ch4Up4-pc",
                "PaleCarnations Ch.4 Up.7 康乃馨俱乐部",
                "PaleCarnations.exe",
            ),
            (
                "PublicSexLifeH-v0.83a-pc",
                "Public Sex Life H v0.87.0 公共性生活公共生活 调教大师",
                "PublicSexLifeH.exe",
            ),
            ("Ripples-0.5.0-pc2", "Ripples V0.9.22 涟漪", "Ripples.exe"),
            (
                "Supower-Re0.45-pc",
                "Supower-Re 0.69 超能力重制版",
                "Supower.exe",
            ),
        ] {
            let mut old = game("old", old_title, Path::new("Z:/missing/old"));
            // These four legacy records really have no launch metadata configured.
            if !old_title.is_ascii() {
                old.main_executable = Some(executable.into());
                old.engine = "Unity".into();
            }
            let mut incoming = game("new", new_title, Path::new("Z:/missing/new"));
            incoming.main_executable = Some(executable.into());
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = new_title.into();
            candidate.executables = vec![exe(executable)];
            let matches =
                MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
            if matches.len() != 1 {
                missing.push(format!("Import: {old_title} / {new_title}"));
            }
            assert!(matches.iter().all(|matched| !matched.auto_associate));
            let index_games = [old, incoming];
            let index = MatchIndex::new(&index_games);
            let pairs = index.library_pairs(&|| false, &|_| {}).unwrap();
            if pairs.len() != 1 {
                missing.push(format!("library: {old_title} / {new_title}"));
            }
            assert!(pairs.iter().all(|pair| pair.confidence == "review"));
            assert!(index.identities.is_empty());
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    #[test]
    fn package_only_recommendations_require_release_evidence_and_preserve_name_conflicts() {
        for (old_title, new_title, old_exe, incoming_exe) in [
            (
                "PublicSexLifeH",
                "Public Sex Life H v0.87.0",
                None,
                "PublicSexLifeH.exe",
            ),
            (
                "PaleCarnations-Ch4Up4-pc",
                "PaleCarnations Ch.5 Up.7",
                None,
                "PaleCarnations.exe",
            ),
            (
                "Supower-Re0.45-pc",
                "Supower-Re 0.69 外传",
                None,
                "Supower.exe",
            ),
            (
                "Princess v1.0 旧译名",
                "Princess v2.0 新译名",
                None,
                "Princess.exe",
            ),
            (
                "和妹妹一起的一个月 妹と過ごす1ヵ月間 Ver2026.09",
                "和妹妹共度的一个月 妹と过ごす1ヵ月间 v26.10",
                None,
                "妹と過ごす1ヵ月間_Re.exe",
            ),
            (
                "和妹妹一起的一个月 妹と過ごす1ヵ月間 Ver2026.09",
                "和妹妹共度的一个月 妹と遊ぶ1ヵ月間 v26.10",
                Some("SharedProjectLauncher.exe"),
                "SharedProjectLauncher.exe",
            ),
        ] {
            let mut old = game("old", old_title, Path::new("Z:/missing/old"));
            old.main_executable = old_exe.map(str::to_owned);
            let mut incoming = game("new", new_title, Path::new("Z:/missing/new"));
            incoming.main_executable = Some(incoming_exe.into());
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = new_title.into();
            candidate.executables = vec![exe(incoming_exe)];
            assert!(
                MatchIndex::new(std::slice::from_ref(&old))
                    .matches(&candidate, &|| false)
                    .is_empty(),
                "Import: {old_title} / {new_title}"
            );
            assert!(
                MatchIndex::new(&[old, incoming])
                    .library_pairs(&|| false, &|_| {})
                    .unwrap()
                    .is_empty(),
                "library: {old_title} / {new_title}"
            );
        }
    }

    #[test]
    fn screenshot_titles_match_in_import_and_library_without_auto_associating() {
        for (old_title, new_title, executable, save) in [
            (
                "爱欲之都 第一季 再启S1 v1.1.4.3 City Light sand Love Bites Season1",
                "爱欲之都 中场季 - 再启 City Lights Love Bites v1.1.7",
                "CityLightsandLoveBitesSeason1.exe",
                "<GAME>/game/saves",
            ),
            (
                "NTR足球 NTRサッカー NTR_Soccer ver1.1.1",
                "NTR足球 NTRサッカー v1.1.2官中正式版",
                "NTR_Soccer.exe",
                "%USERPROFILE%/AppData/LocalLow/Hizure/NTR_Soccer",
            ),
        ] {
            let mut old = game("old", old_title, Path::new("Z:/missing/old"));
            old.main_executable = Some(executable.into());
            old.save_paths = vec![save.into()];
            let mut incoming = game("new", new_title, Path::new("Z:/missing/new"));
            incoming.main_executable = old.main_executable.clone();
            incoming.save_paths = old.save_paths.clone();
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = new_title.into();
            candidate.executables = vec![exe(executable)];
            candidate.save_paths = old.save_paths.clone();
            let matches =
                MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
            assert_eq!(matches.len(), 1, "Import: {new_title}");
            assert!(!matches[0].auto_associate);
            assert!(matches[0].reason.contains("特色启动文件一致"));
            let library = [old, incoming];
            let index = MatchIndex::new(&library);
            let pairs = index.library_pairs(&|| false, &|_| {}).unwrap();
            assert_eq!(pairs.len(), 1, "library: {new_title}");
            assert_eq!(pairs[0].confidence, "review");
            assert!(index.identities.is_empty());
        }
    }

    #[test]
    fn metadata_fallback_never_overrides_sequels_seasons_dlc_or_unrelated_titles() {
        for (old_title, new_title) in [
            (
                "爱欲之都 第一季 再启 City Lights Love Bites Season1",
                "爱欲之都 第二季 再启 City Lights Love Bites Season2",
            ),
            ("Moonlight Garden Season1", "Moonlight Garden Season 2"),
            ("Moonlight Garden S1", "Moonlight Garden S2"),
            ("Moonlight Garden", "Moonlight Garden II"),
            ("Moonlight Garden", "Moonlight Garden DLC"),
            ("Moonlight Garden", "Moonlight Garden 外传"),
            ("PaleCarnations-Ch4Up4-pc", "PaleCarnations Ch.5 Up.7"),
            ("PublicSexLifeH-v0.83a-pc", "Public Sex Life H 2 v0.87.0"),
            ("Supower-Re0.45-pc", "Supower-Re 0.69 外传"),
            ("Moonlight Garden II", "Moonlight Garden I I"),
            ("NTR足球 NTRサッカー", "NTR篮球 NTRバスケット"),
            (
                "爱欲之都 中场季 - 再启 City Lights Love Bites",
                "无关作品 Moonlight School",
            ),
        ] {
            let mut old = game("old", old_title, Path::new("Z:/missing/old"));
            old.main_executable = Some("SharedProjectLauncher.exe".into());
            old.save_paths = vec!["%APPDATA%/SharedProjectSave-123456".into()];
            let mut incoming = game("new", new_title, Path::new("Z:/missing/new"));
            incoming.main_executable = old.main_executable.clone();
            incoming.save_paths = old.save_paths.clone();
            let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
            candidate.suggested_title = new_title.into();
            candidate.executables = vec![exe("SharedProjectLauncher.exe")];
            candidate.save_paths = old.save_paths.clone();
            let matches =
                MatchIndex::new(std::slice::from_ref(&old)).matches(&candidate, &|| false);
            // Import may still recommend a DLC package as an explicit full update.
            if !new_title.contains("DLC") {
                assert!(matches.is_empty(), "Import: {old_title} / {new_title}");
            } else {
                assert!(matches.iter().all(|matched| !matched.auto_associate));
            }
            assert!(
                MatchIndex::new(&[old, incoming])
                    .library_pairs(&|| false, &|_| {})
                    .unwrap()
                    .is_empty(),
                "library: {old_title} / {new_title}"
            );
        }
        let mut old = game(
            "old",
            "爱欲之都 第一季 再启S1 v1.1.4.3 City Light sand Love Bites Season1",
            Path::new("Z:/missing/old"),
        );
        old.main_executable = Some("Game.exe".into());
        let mut candidate = scanner::pending_candidate(Path::new("Z:/missing/source")).unwrap();
        candidate.suggested_title = "爱欲之都 中场季 - 再启 City Lights Love Bites v1.1.7".into();
        candidate.executables = vec![exe("Game.exe")];
        candidate.save_paths = vec!["<GAME>/save".into()];
        assert!(MatchIndex::new(&[old])
            .matches(&candidate, &|| false)
            .is_empty());
    }

    #[test]
    fn library_pairs_are_unique_unbounded_and_use_aliases_and_directories_without_file_reads() {
        let mut games: Vec<_> = (0..7)
            .map(|n| {
                game(
                    &format!("id-{n}"),
                    "Moonlight Garden v1.0",
                    Path::new(&format!("Z:/not-present/{n}")),
                )
            })
            .collect();
        games[1].display_title = "月光庭院".into();
        games[1].canonical_title = "月光庭院".into();
        games[1].aliases = vec!["Moonlight Garden v1.2".into()];
        games[1].current_version = "v1.2".into();
        let index = MatchIndex::new(&games);
        let pairs = index.library_pairs(&|| false, &|_| {}).unwrap();
        assert_eq!(pairs.len(), 21);
        let unique: HashSet<_> = pairs.iter().map(|p| (&p.left_id, &p.right_id)).collect();
        assert_eq!(unique.len(), pairs.len());
        assert!(pairs
            .iter()
            .all(|p| p.left_id < p.right_id && p.confidence == "high"));
        assert!(pairs.iter().any(|p| p.reason.contains("记录版本不同")));
        assert!(index.identities.is_empty());
        games.reverse();
        assert_eq!(
            MatchIndex::new(&games)
                .library_pairs(&|| false, &|_| {})
                .unwrap()
                .len(),
            21
        );
        let folder_games = [
            game(
                "folder-a",
                "自定义甲",
                Path::new("Z:/one/Special Garden v1.0"),
            ),
            game(
                "folder-b",
                "自定义乙",
                Path::new("Z:/two/Special Garden v1.2"),
            ),
        ];
        assert_eq!(
            MatchIndex::new(&folder_games)
                .library_pairs(&|| false, &|_| {})
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn library_multilingual_and_metadata_evidence_remain_recommendations() {
        for (a, b) in [
            (
                "忍者归来 A Ninja's Return v1.08",
                "A Ninja's Return v1.09 忍者归来 汉化版",
            ),
            (
                "译名甲 佐谷霞已はあなたがいないと生きられない v1.0",
                "译名乙 佐谷霞已はあなたがいないと生きられない v1.2",
            ),
        ] {
            let games = [
                game("a", a, Path::new("Z:/unavailable/a")),
                game("b", b, Path::new("Z:/unavailable/b")),
            ];
            let pairs = MatchIndex::new(&games)
                .library_pairs(&|| false, &|_| {})
                .unwrap();
            assert_eq!(pairs.len(), 1, "{a} / {b}");
            assert_eq!(pairs[0].confidence, "review");
        }
        let mut a = game(
            "a",
            "DistinctiveTitle 旧译名",
            Path::new("Z:/unavailable/a"),
        );
        let mut b = game(
            "b",
            "DistinctiveTitle 新译名",
            Path::new("Z:/unavailable/b"),
        );
        a.main_executable = Some("DistinctiveTitle-32.exe".into());
        b.mtool_target_exe = Some("DistinctiveTitle-64.exe".into());
        a.save_paths = vec!["%APPDATA%/RenPy/DistinctiveTitle-123456".into()];
        b.save_paths = a.save_paths.clone();
        let games = [a, b];
        let pairs = MatchIndex::new(&games)
            .library_pairs(&|| false, &|_| {})
            .unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].confidence, "review");
        assert!(pairs[0].reason.contains("特色启动文件一致"));
        assert!(pairs[0].reason.contains("外部存档路径一致"));
    }

    #[test]
    fn library_sequels_spinoffs_and_dlc_cannot_bypass_protection_with_shared_aliases() {
        for title in [
            "Moonlight Garden 2",
            "Moonlight Garden II",
            "Moonlight Garden DLC",
            "Moonlight Garden 外传",
            "Moonlight Garden 外伝",
            "Moonlight Garden 第一章",
            "Moonlight Garden Side Story",
            "Moonlight Garden Expansion",
        ] {
            let a = game("a", "Moonlight Garden", Path::new("Z:/absent/a"));
            let mut b = game("b", title, Path::new("Z:/absent/b"));
            b.aliases.push("Moonlight Garden".into());
            assert!(
                MatchIndex::new(&[a, b])
                    .library_pairs(&|| false, &|_| {})
                    .unwrap()
                    .is_empty(),
                "{title}"
            );
        }
        let mut games = [
            game("a", "Different Story", Path::new("Z:/absent/a")),
            game("b", "Another Adventure", Path::new("Z:/absent/b")),
        ];
        for game in &mut games {
            game.aliases.extend(["Game".into(), "汉化版".into()]);
        }
        assert!(MatchIndex::new(&games)
            .library_pairs(&|| false, &|_| {})
            .unwrap()
            .is_empty());
        let mut different_engines = [
            game("a", "Same Garden", Path::new("Z:/absent/a")),
            game("b", "Same Garden", Path::new("Z:/absent/b")),
        ];
        different_engines[0].engine = "Unity".into();
        different_engines[1].engine = "Ren'Py".into();
        let pairs = MatchIndex::new(&different_engines)
            .library_pairs(&|| false, &|_| {})
            .unwrap();
        assert_eq!(pairs[0].confidence, "review");
        assert!(pairs[0].reason.contains("引擎不同"));
    }

    #[test]
    fn library_matching_is_cancellable_and_500_to_2000_game_libraries_are_bounded() {
        for size in [500, 1000, 2000] {
            let games: Vec<_> = (0..size)
                .map(|n| {
                    game(
                        &format!("game-{n:04}"),
                        &format!("Library Story {:04} v1.{}", n / 2, n % 2),
                        Path::new(&format!("Z:/never-read/game-{n}")),
                    )
                })
                .collect();
            let start = Instant::now();
            let index = MatchIndex::new(&games);
            let pairs = index.library_pairs(&|| false, &|_| {}).unwrap();
            assert_eq!(pairs.len(), size / 2);
            assert!(index.identities.is_empty());
            eprintln!("library {size}: {} ms", start.elapsed().as_millis());
            assert!(
                start.elapsed().as_secs() < 10,
                "metadata-only duplicate check exceeded budget"
            );
            let calls = std::cell::Cell::new(0);
            assert!(index
                .library_pairs(
                    &|| {
                        calls.set(calls.get() + 1);
                        calls.get() > 8
                    },
                    &|_| {}
                )
                .is_err());
        }
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
