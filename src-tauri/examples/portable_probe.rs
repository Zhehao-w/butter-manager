//! Isolated CLI verification using a hidden real WebView, never the production profile.
//! Copy this executable into a disposable folder before invoking `write` then `read`.
use butter_manager::{appearance, data_directory, db::Database, domain::Settings};
use std::path::PathBuf;
use tauri::{Manager, State};

struct Probe {
    data: PathBuf,
    read: bool,
}

#[tauri::command]
fn probe_done(
    app: tauri::AppHandle,
    state: State<'_, Probe>,
    before: Option<String>,
    value: String,
    origin: String,
) -> Result<(), String> {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        if value != "grid" || state.read && before.as_deref() != Some("grid") {
            return Err("localStorage did not survive restart/move".into());
        }
        let database = Database::open(&state.data.join(data_directory::DATABASE))?;
        if database.settings()?.scan_workers != 4
            || appearance::load(&state.data)?.icon != appearance::Choice::Original
        {
            return Err("application preferences did not persist".into());
        }
        let report = serde_json::json!({
            "data": app.path().app_local_data_dir()?, "config": app.path().app_config_dir()?,
            "cache": app.path().app_cache_dir()?, "logs": app.path().app_log_dir()?,
            "before": before, "localStorage": value, "origin": origin,
        });
        std::fs::write(
            state.data.join("probe-result.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            app.exit(0);
            Ok(())
        }
        Err(error) => {
            eprintln!("{error}");
            app.exit(1);
            Err(error.to_string())
        }
    }
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .expect("Pass write or read in an isolated fixture folder");
    assert!(mode == "write" || mode == "read");
    let read = mode == "read";
    let mut context = tauri::generate_context!();
    // Serve bundled assets in the debug probe too; it needs no Vite server.
    context.config_mut().build.dev_url = None;
    let development = cfg!(debug_assertions) || tauri::is_dev();
    if let Some(root) = data_directory::development_override(cfg!(windows), development) {
        context.config_mut().app.app_directories_override = Some(
            tauri::utils::config::AppDirectoriesOverride::Root(root.into()),
        );
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![probe_done])
        .setup(move |app| {
            let data = app.path().app_local_data_dir()?;
            let expected = std::env::current_exe()?.parent().unwrap().join(if development { "data-dev" } else { "data" });
            data_directory::ensure_writable(&data)?;
            assert_eq!(dunce::canonicalize(&data)?, dunce::canonicalize(&expected)?);
            if !read {
                // Never overwrite an existing library/profile: write mode must start empty.
                assert!(data_directory::is_empty(&data)?);
                let db = Database::open(&data.join(data_directory::DATABASE))?;
                db.save_settings(Settings { scan_workers: 4, mtool_injector: "loaders/inject.exe".into(), mtool_runtime: "MTool.exe".into(), ..Settings::default() })?;
                appearance::save(&data, appearance::Appearance { icon: appearance::Choice::Original, illustration: appearance::Choice::New })?;
                std::fs::create_dir(data.join("imports"))?;
                std::fs::write(data.join("imports/probe.manifest"), b"isolated fixture")?;
            }
            std::fs::create_dir_all(app.path().app_cache_dir()?)?;
            std::fs::create_dir_all(app.path().app_log_dir()?)?;
            app.manage(Probe { data, read });
            let script = format!(r#"
                const before = localStorage.getItem('butter-manager.view.library');
                if (!{read}) localStorage.setItem('butter-manager.view.library', 'grid');
                setTimeout(() => window.__TAURI_INTERNALS__.invoke('probe_done', {{
                    before, value: localStorage.getItem('butter-manager.view.library'), origin: location.origin
                }}), 4000);
            "#);
            tauri::WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .visible(false).initialization_script(script).build()?;
            // A stalled browser fails the probe instead of keeping a hidden process indefinitely.
            let handle = app.handle().clone();
            std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_secs(30)); handle.exit(2); });
            Ok(())
        })
        .run(context).expect("Portable probe failed");
}
