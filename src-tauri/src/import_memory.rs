//! Only explicit, committed Import decisions become recommendations. Never aliases.
use super::import_matching::Name;
use crate::{domain::ScanCandidate, paths, save_detection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryProfile {
    pub format_version: u8,
    pub title: String,
    pub name_key: String,
    pub identities: Vec<String>,
}

impl MemoryProfile {
    pub fn from_candidate(candidate: &ScanCandidate, executable: &str) -> Option<Self> {
        let name = Name::new(&candidate.suggested_title);
        let name_key = name.memory_key()?;
        let identities = save_detection::matching_identity(
            Path::new(&candidate.install_path),
            &candidate.working_directory,
            Some(executable),
        )
        .into_iter()
        .filter(|value| useful_identity(value))
        .collect();
        Some(Self {
            format_version: 1,
            title: candidate.suggested_title.clone(),
            name_key,
            identities,
        })
    }

    pub fn valid(&self) -> bool {
        self.format_version == 1
            && self.title.len() <= 4096
            && Name::new(&self.title).memory_key().as_deref() == Some(self.name_key.as_str())
            && self.identities.len() <= 8
            && self.identities.iter().all(|value| useful_identity(value))
    }

    pub fn key(&self) -> String {
        serde_json::to_string(&(self.format_version, &self.name_key, &self.identities)).unwrap()
    }
}

fn useful_identity(value: &str) -> bool {
    if value.len() > 2048 {
        return false;
    }
    let Some((kind, rest)) = value.split_once(':') else {
        return false;
    };
    let product = match kind {
        "unity" => rest.split_once(':').map(|(_, product)| product),
        "renpy" => rest.split_once(':').and_then(|(field, value)| {
            ["config.save_directory", "build.name"]
                .contains(&field)
                .then_some(value)
        }),
        "godot" => Some(rest),
        _ => None,
    };
    product.is_some_and(|product| {
        let compact: String = paths::normalize_alias(product)
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        compact.chars().count() >= 6
            && ![
                "game",
                "mygame",
                "newgame",
                "project",
                "myproject",
                "newproject",
                "newunityproject",
                "unityproject",
                "default",
                "defaultcompany",
                "renpy",
                "godot",
                "template",
                "visualnovel",
                "untitled",
                "unknown",
                "untitledgame",
                "untitledproject",
                "defaultproject",
            ]
            .contains(&compact.as_str())
    })
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ImportMemory {
    pub id: String,
    pub game_id: String,
    pub profile: MemoryProfile,
    pub confirmed_at: String,
}

#[derive(Default)]
pub(super) struct MemoryIndex {
    profiles: Vec<ImportMemory>,
    names: HashMap<String, Vec<usize>>,
    identities: HashMap<String, Vec<usize>>,
}

impl MemoryIndex {
    pub fn new(memories: &[ImportMemory]) -> Self {
        let mut result = Self::default();
        for memory in memories.iter().filter(|memory| memory.profile.valid()) {
            let position = result.profiles.len();
            result
                .names
                .entry(memory.profile.name_key.clone())
                .or_default()
                .push(position);
            for identity in &memory.profile.identities {
                result
                    .identities
                    .entry(identity.clone())
                    .or_default()
                    .push(position);
            }
            result.profiles.push(memory.clone());
        }
        result
    }

    pub fn needs_identity(&self) -> bool {
        !self.identities.is_empty()
    }

    pub fn matches(&self, title: &str, identities: &[String]) -> Vec<(String, String)> {
        let name = Name::new(title);
        let key = name.memory_key();
        let mut positions: HashSet<_> = key
            .as_ref()
            .and_then(|key| self.names.get(key))
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for identity in identities
            .iter()
            .filter(|identity| useful_identity(identity))
        {
            positions.extend(self.identities.get(identity).into_iter().flatten().copied());
        }
        let mut results = HashMap::new();
        for position in positions {
            let memory = &self.profiles[position];
            if name.memory_conflict(&Name::new(&memory.profile.title)) {
                continue;
            }
            let same_name = key.as_deref() == Some(memory.profile.name_key.as_str());
            let reason = if same_name {
                "来源名称一致"
            } else {
                "静态项目标识一致"
            };
            results
                .entry(memory.game_id.clone())
                .and_modify(|value: &mut String| {
                    if same_name {
                        *value = format!("曾由你确认关联 · {reason}；请确认");
                    }
                })
                .or_insert_with(|| format!("曾由你确认关联 · {reason}；请确认"));
        }
        let mut results: Vec<_> = results.into_iter().collect();
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(game: &str, title: &str, identities: &[&str]) -> ImportMemory {
        ImportMemory {
            id: format!("memory-{game}"),
            game_id: game.into(),
            confirmed_at: String::new(),
            profile: MemoryProfile {
                format_version: 1,
                title: title.into(),
                name_key: Name::new(title).memory_key().unwrap(),
                identities: identities.iter().map(|s| (*s).into()).collect(),
            },
        }
    }

    #[test]
    fn persisted_v1_demon_and_package_profile_remains_valid() {
        let title =
            "断角魔王艾莉丝的重生谭 The Rebirth Tale of Broken Horn Demon Lord Alice 官中无修版";
        let mut old = memory("old", title, &[]);
        // Literal v1 key from 1.3.0, including its historical Demo substring and package parsing.
        old.profile.name_key = r#"[["alice","broken","demon","horn","lord","of","rebirth","tale","the"],"断角魔王艾莉丝的重生谭官中无修版",[],[],["demo"],false,[]]"#.into();
        assert!(old.profile.valid());
        assert_eq!(
            Name::new(title).memory_key().as_ref(),
            Some(&old.profile.name_key)
        );
        let index = MemoryIndex::new(&[old]);
        let matches = index.matches("断角魔王艾莉丝的重生谭 The Rebirth Tale of Broken Horn Demon Lord Alice v0.73 官中无修版", &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "old");
        assert!(index.matches("断角魔王艾莉丝的重生谭 The Rebirth Tale of Broken Horn Demon Lord Alice v0.73 Demo 官中无修版", &[]).is_empty());
    }

    #[test]
    fn remembered_versions_and_project_identity_recall_without_changing_aliases_or_learning() {
        let m = memory(
            "old",
            "Moonlight Garden v0.8 汉化版",
            &["unity:fixture studio:moonlight garden"],
        );
        let index = MemoryIndex::new(std::slice::from_ref(&m));
        assert_eq!(
            index.matches("Moonlight Garden v0.9 汉化版", &[])[0].0,
            "old"
        );
        assert_eq!(
            index.matches(
                "月光庭院 v1.0",
                &["unity:fixture studio:moonlight garden".into()]
            )[0]
            .0,
            "old"
        );
        assert!(index.matches("Unrelated Journey v1.0", &[]).is_empty());
        assert_eq!(index.profiles.len(), 1);
        assert!(Name::new("Game v1.0").memory_key().is_none());
        assert!(Name::new("汉化版 v1.0").memory_key().is_none());
        for generic in [
            "Project v1.0",
            "Launcher v1.0",
            "New Game v1.0",
            "游戏资源 v1.0",
        ] {
            assert!(Name::new(generic).memory_key().is_none(), "{generic}");
        }
    }

    #[test]
    fn generic_incoming_titles_still_recall_existing_memories_through_valid_project_ids() {
        for identity in [
            "unity:fixture studio:moonlight garden",
            "renpy:config.save_directory:moonlight garden",
            "renpy:build.name:moonlight garden",
            "godot:moonlight garden",
        ] {
            let index = MemoryIndex::new(&[memory("old", "Moonlight Garden v0.8", &[identity])]);
            for title in ["Game v0.9", "Project v0.9", "Launcher v0.9"] {
                assert!(Name::new(title).memory_key().is_none());
                let results = index.matches(title, &[identity.into()]);
                assert_eq!(results.len(), 1, "{title} / {identity}");
                assert_eq!(results[0].0, "old");
                assert!(results[0].1.contains("静态项目标识一致"));
                assert!(index.matches(title, &[]).is_empty());
                assert!(index
                    .matches(title, &["godot:another journey".into()])
                    .is_empty());
                assert!(index
                    .matches(title, &["unity:DefaultCompany:New Unity Project".into()])
                    .is_empty());
            }
        }
    }

    #[test]
    fn sequel_chapter_edition_and_dlc_guards_survive_shared_project_identity() {
        for (old, incoming) in [
            ("Moonlight Garden 2 v0.8", "Moonlight Garden 3 v0.9"),
            (
                "Moonlight Garden Season 1 v0.8",
                "Moonlight Garden Season 2 v0.9",
            ),
            ("Moonlight Garden v0.8", "Moonlight Garden DLC v0.9"),
            ("Moonlight Garden Ch.4 Up.1", "Moonlight Garden Ch.5 Up.1"),
            ("Moonlight Garden v0.8", "Moonlight Garden 外传 v0.9"),
        ] {
            let index = MemoryIndex::new(&[memory("old", old, &["godot:moonlight garden"])]);
            assert!(
                index
                    .matches(incoming, &["godot:moonlight garden".into()])
                    .is_empty(),
                "{old} / {incoming}"
            );
        }
    }

    #[test]
    fn ambiguous_manual_decisions_are_not_overwritten_and_template_ids_cannot_recall() {
        let index = MemoryIndex::new(&[
            memory("a", "Moonlight Garden v0.8", &[]),
            memory("b", "Moonlight Garden v0.8", &[]),
        ]);
        assert_eq!(
            index
                .matches("Moonlight Garden v0.9", &[])
                .iter()
                .map(|r| r.0.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        for id in [
            "unity:DefaultCompany:New Unity Project",
            "unity:studio:MyGame",
            "renpy:build.name:template",
            "godot:project",
            "exe:Moonlight.exe",
            "godot:saves",
        ] {
            assert!(!useful_identity(id), "{id}");
            let invalid = MemoryIndex::new(&[memory("a", "Moonlight Garden v0.8", &[id])]);
            assert!(invalid
                .matches("Different Adventure v1.0", &[id.into()])
                .is_empty());
        }
        let mut future = memory("a", "Moonlight Garden v0.8", &[]);
        future.profile.format_version = 2;
        assert!(MemoryIndex::new(&[future])
            .matches("Moonlight Garden v0.9", &[])
            .is_empty());
    }
}
