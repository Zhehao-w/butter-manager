use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("数据库操作失败：{0}")]
    Database(#[from] rusqlite::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

/// The executable stays in main_executable; all paths remain relative to the game root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalPlayer {
    pub player_type: String,
    pub scope: String,
    pub game_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QspDetection {
    pub game_files: Vec<String>,
    pub players: Vec<String>,
    pub recommended_player: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub game_root: String,
    pub mtool_root: String,
    pub mtool_injector: String,
    pub mtool_runtime: String,
    #[serde(default = "default_scan_workers")]
    pub scan_workers: usize,
}

fn default_scan_workers() -> usize {
    2
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_root: String::new(),
            mtool_root: String::new(),
            mtool_injector: String::new(),
            mtool_runtime: String::new(),
            scan_workers: default_scan_workers(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetReport {
    pub settings: Settings,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayStatus {
    #[default]
    Unplayed,
    Playing,
    Completed,
}
impl PlayStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unplayed => "UNPLAYED",
            Self::Playing => "PLAYING",
            Self::Completed => "COMPLETED",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "UNPLAYED" => Ok(Self::Unplayed),
            "PLAYING" => Ok(Self::Playing),
            "COMPLETED" => Ok(Self::Completed),
            _ => Err(Error::Validation("无效的游玩状态".into())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub canonical_title: String,
    pub display_title: String,
    pub install_path: String,
    pub working_directory: String,
    pub current_version: String,
    pub version_source: String,
    pub main_executable: Option<String>,
    pub engine: String,
    pub launch_type: String,
    #[serde(default)]
    pub external_player: Option<ExternalPlayer>,
    pub mtool_target_exe: Option<String>,
    pub mtool_loader: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_launched_at: Option<String>,
    pub play_status: PlayStatus,
    pub aliases: Vec<String>,
    pub save_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEdit {
    pub id: String,
    pub canonical_title: String,
    pub display_title: String,
    pub current_version: String,
    pub engine: String,
    #[serde(default)]
    pub play_status: PlayStatus,
    pub main_executable: Option<String>,
    pub working_directory: String,
    pub launch_type: String,
    #[serde(default)]
    pub external_player: Option<ExternalPlayer>,
    pub mtool_target_exe: Option<String>,
    pub mtool_loader: Option<String>,
    pub aliases: Vec<String>,
    pub save_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPathCheck {
    pub id: String,
    pub install_path: String,
    pub state: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocateGame {
    pub id: String,
    pub expected_install_path: String,
    pub install_path: String,
    pub main_executable: Option<String>,
    pub working_directory: String,
    pub launch_type: String,
    #[serde(default)]
    pub external_player: Option<ExternalPlayer>,
    pub mtool_target_exe: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCheck {
    pub label: String,
    pub path: String,
    pub available: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MToolLaunchPreview {
    pub shared_root: String,
    pub target_exe: String,
    pub architecture: String,
    pub loader: String,
    pub runtime: String,
    pub working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExeCandidate {
    pub relative_path: String,
    pub architecture: String,
    pub score: i32,
    pub size_bytes: u64,
    pub modified_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MToolRecipe {
    pub target_exe: String,
    pub loader: String,
    pub injector: String,
    pub runtime: String,
    pub observed_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatAnalysis {
    pub path: String,
    pub status: String,
    pub recipe: Option<MToolRecipe>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCandidate {
    pub install_path: String,
    #[serde(default)]
    pub directory_modified_ms: Option<u128>,
    pub suggested_title: String,
    pub engine: String,
    #[serde(default)]
    pub save_paths: Vec<String>,
    #[serde(default)]
    pub qsp: Option<QspDetection>,
    pub executables: Vec<ExeCandidate>,
    pub bats: Vec<BatAnalysis>,
    pub bundled_tool: bool,
    #[serde(default)]
    pub mtool_detected: bool,
    pub warnings: Vec<String>,
    pub registered_id: Option<String>,
    pub suggested_version: String,
    pub version_source: String,
    pub working_directory: String,
    pub status: String,
    pub entries_scanned: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationSelection {
    pub install_path: String,
    pub executable: Option<String>,
    #[serde(default)]
    pub external_player: Option<ExternalPlayer>,
    #[serde(default)]
    pub exe_override: bool,
    pub version: String,
    #[serde(default)]
    pub version_override: bool,
}

#[derive(Debug, Clone)]
pub struct RegistrationEntry {
    pub candidate: ScanCandidate,
    pub executable: Option<String>,
    pub external_player: Option<ExternalPlayer>,
    pub version: String,
    pub version_source: String,
    pub working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub candidates: Vec<ScanCandidate>,
    pub warnings: Vec<String>,
}
