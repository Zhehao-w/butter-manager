pub mod appearance;
pub mod association;
pub mod db;
pub mod deletion;
pub mod domain;
pub mod external_player;
pub mod import_duplicate;
pub mod import_sources;
pub mod importer;
pub mod jobs;
pub mod launcher;
pub mod maintenance;
pub mod mtool;
pub mod paths;
#[cfg(windows)]
mod recycle_windows;
pub mod registration;
pub mod runtime;
pub mod save_detection;
pub mod scanner;
pub mod version;
mod window_placement;

#[cfg(feature = "desktop")]
mod commands;

#[cfg(feature = "desktop")]
pub use commands::run;
