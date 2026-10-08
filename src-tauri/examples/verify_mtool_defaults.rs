//! Read-only launch-plan verification; never creates a process or opens the application database.
use butter_manager::{
    domain::{Game, Settings},
    launcher,
};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Sample {
    bat: String,
    text: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: verify_mtool_defaults samples.json shared-root".into());
    }
    let samples: Vec<Sample> = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let settings = Settings {
        mtool_root: args[2].clone(),
        mtool_injector: "loaders/inject.exe".into(),
        mtool_runtime: "MTool.exe".into(),
        ..Settings::default()
    };
    for (index, sample) in samples.iter().enumerate() {
        let target = sample
            .text
            .split("%~dp0\\")
            .nth(1)
            .and_then(|value| value.split('"').next())
            .ok_or("sample does not have a literal game target")?;
        let game = Game {
            id: "read-only".into(),
            canonical_title: "sample".into(),
            display_title: "sample".into(),
            install_path: Path::new(&sample.bat)
                .parent()
                .unwrap()
                .display()
                .to_string(),
            main_executable: Some(target.into()),
            mtool_target_exe: None,
            mtool_loader: None,
            launch_type: "MTOOL".into(),
            external_player: None,
            working_directory: ".".into(),
            current_version: "Unknown".into(),
            version_source: "manual".into(),
            engine: "Unknown".into(),
            created_at: String::new(),
            updated_at: String::new(),
            last_launched_at: None,
            play_status: Default::default(),
            aliases: vec![],
            save_paths: vec![],
        };
        let preview = launcher::preview_mtool(&game, &settings)?;
        let observed = if sample.text.to_lowercase().contains("mzhook32.dll") {
            "loaders/mzHook32.dll"
        } else {
            "loaders/mzHook.dll"
        };
        assert_eq!(preview.loader, observed);
        println!(
            "sample {}: {} -> {}; shared files accessible",
            index + 1,
            preview.architecture,
            preview.loader
        );
    }
    Ok(())
}
