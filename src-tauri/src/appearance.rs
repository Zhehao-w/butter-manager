use serde::{Deserialize, Serialize};
use std::{fs, io, io::Write, path::Path};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Original,
    #[default]
    New,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub icon: Choice,
    pub illustration: Choice,
}

pub fn load(data: &Path) -> io::Result<Appearance> {
    match fs::read(data.join("appearance.json")) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Appearance::default()),
        Err(error) => Err(error),
    }
}

pub fn save(data: &Path, appearance: Appearance) -> io::Result<()> {
    let temporary = data.join(format!(".appearance-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&appearance)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, data.join("appearance.json"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(feature = "desktop")]
pub fn icon(choice: Choice) -> tauri::image::Image<'static> {
    let bytes: &'static [u8; 128 * 128 * 4] = match choice {
        Choice::Original => include_bytes!("../icons/icon-original.rgba"),
        Choice::New => include_bytes!("../icons/icon-new.rgba"),
    };
    tauri::image::Image::new(bytes, 128, 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_defaults_and_independent_choices_survive_restart() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(load(data.path()).unwrap(), Appearance::default());
        for appearance in [
            Appearance {
                icon: Choice::Original,
                illustration: Choice::New,
            },
            Appearance {
                icon: Choice::New,
                illustration: Choice::Original,
            },
        ] {
            save(data.path(), appearance).unwrap();
            assert_eq!(load(data.path()).unwrap(), appearance);
        }
        assert_eq!(fs::read_dir(data.path()).unwrap().count(), 1);
    }

    #[test]
    fn invalid_preferences_are_reported_and_failed_write_cleans_up() {
        let data = tempfile::tempdir().unwrap();
        let path = data.path().join("appearance.json");
        fs::write(&path, br#"{"icon":"unknown"}"#).unwrap();
        assert!(load(data.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), br#"{"icon":"unknown"}"#);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(save(data.path(), Appearance::default()).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(data.path()).unwrap().count(), 1);
    }
}
