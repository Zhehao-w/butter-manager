use super::*;

#[test]
fn windows_release_and_development_keep_app_webview_and_instances_isolated() {
    let release: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.windows.conf.json")).unwrap();
    assert_eq!(release["app"]["appDirectoriesOverride"], "./data");
    assert_eq!(development_override(true, true), Some("./data-dev"));
    assert_eq!(development_override(true, false), None);
    assert_eq!(development_override(false, true), None);
    let base: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(base["app"]["windows"][0]["create"], false);
    assert!(base["app"]["windows"][0].get("dataDirectory").is_none());

    #[cfg(windows)]
    {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let guard = InstanceGuard::acquire(&data).unwrap();
        assert!(InstanceGuard::acquire(&data).is_err());
        assert!(InstanceGuard::acquire(&temp.path().join("data-dev")).is_ok());
        drop(guard);
        assert!(InstanceGuard::acquire(&data).is_ok());
    }
}

#[test]
fn data_directory_preserves_existing_content_and_reports_unwritable_paths() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    fs::write(&data, b"path occupied").unwrap();
    let error = ensure_writable(&data).unwrap_err().to_string();
    assert!(error.contains("不会改用 AppData"));
    assert_eq!(fs::read(&data).unwrap(), b"path occupied");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);

    fs::remove_file(&data).unwrap();
    ensure_writable(&data).unwrap();
    fs::write(data.join("existing.json"), b"existing settings").unwrap();
    ensure_writable(&data).unwrap();
    assert_eq!(
        fs::read(data.join("existing.json")).unwrap(),
        b"existing settings"
    );
    assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
}
