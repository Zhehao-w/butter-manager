use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::io::Cursor;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

fn game(root: &Path) -> Game {
    serde_json::from_value(json!({"id":"game","canonical_title":"fixture","display_title":"fixture","install_path":root.display().to_string(),"working_directory":".","current_version":"1","version_source":"manual","main_executable":null,"engine":"RPG Maker MV","launch_type":"DIRECT","mtool_target_exe":null,"mtool_loader":null,"created_at":"0","updated_at":"0","last_launched_at":null,"play_status":"PLAYING","aliases":[],"save_paths":["<GAME>/save"]})).unwrap()
}
fn rpg_value() -> Value {
    json!({"@":"Object","variables":{"@":"Game_Variables","_data":[null,5,"text",true,{"@":"Custom","keep":[1,2]}]},"switches":{"@":"Game_Switches","_data":[null,false,true]},"party":{"@":"Game_Party","_gold":120,"_items":{"1":2},"_weapons":{},"_armors":{}},"actors":{"_data":[null,{"_name":"勇者","_level":5,"_hp":100}]},"unknown":{"@":["custom"],"preserve":42}})
}
fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/renpy.json")).unwrap()
}
fn raw(fixture: &Value, name: &str) -> Vec<u8> {
    STANDARD.decode(fixture[name].as_str().unwrap()).unwrap()
}
// Optional independent interoperability probe: only synthetic fixtures, never game data.
fn export_fixture(name: &str, bytes: &[u8]) {
    if let Some(folder) = std::env::var_os("BUTTER_SAVE_EDITOR_VERIFY_DIR") {
        let folder = PathBuf::from(folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(name), bytes).unwrap();
    }
}
fn change(field: &Field, value: Value) -> Change {
    Change {
        id: field.id.clone(),
        value,
    }
}
fn field<'a>(fields: &'a [Field], name: &str) -> &'a Field {
    fields.iter().find(|f| f.name == name).unwrap()
}
fn zip(log: &[u8], signatures: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer.set_comment("keep archive comment");
    for (name, bytes) in [
        ("log", log),
        ("signatures", signatures.as_bytes()),
        ("json", b"{\"_save_name\":\"Chapter 2\"}".as_slice()),
        ("extra_info", b"keep metadata".as_slice()),
        ("unknown.bin", b"\x00\xffuntouched".as_slice()),
        ("screenshot.png", b"\x89PNG\r\n\x1a\nfixture".as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn rpg_codec_roundtrips_all_encodings_and_unknown_jsonex() {
    let official: Value = serde_json::from_str(include_str!("fixtures/rpg.json")).unwrap();
    for (key, format) in [("mv", "MV"), ("mz", "MZ")] {
        let bytes = raw(&official, key);
        let decoded = rpg::decode(&bytes, format).unwrap();
        assert_eq!(decoded.value, official["value"]);
        assert_eq!(
            rpg::decode(&rpg::encode(&decoded).unwrap(), format)
                .unwrap()
                .value,
            official["value"]
        );
    }
    let value = rpg_value();
    for (format, binary) in [("MV", false), ("MZ", false), ("MZ", true)] {
        let bytes = if format == "MV" {
            lz_str::compress_to_base64(value.to_string().as_str()).into_bytes()
        } else {
            let bytes = rpg::deflate(value.to_string().as_bytes(), 1).unwrap();
            if binary {
                bytes
            } else {
                bytes
                    .into_iter()
                    .map(char::from)
                    .collect::<String>()
                    .into_bytes()
            }
        };
        let decoded = rpg::decode(&bytes, format).unwrap();
        assert_eq!(decoded.value, value);
        assert_eq!(
            rpg::decode(&rpg::encode(&decoded).unwrap(), format)
                .unwrap()
                .value,
            value
        );
        assert!(rpg::decode(&bytes[..bytes.len() / 2], format).is_err());
    }
    assert!(rpg::decode(b"not a save", "MV").is_err());
    assert!(rpg::decode(b"PK\x03\x04", "MZ").is_err());
}
#[test]
fn rpg_legacy_jsonex_wrappers_preserve_identities_and_shared_references() {
    let dir = tempfile::tempdir().unwrap();
    let game = game(dir.path());
    let mut value = rpg_value();
    for (kind, identity) in [("variables", 10), ("switches", 20), ("actors", 30)] {
        let array = value[kind]["_data"].take();
        value[kind]["@c"] = json!(identity);
        value[kind]["_data"] = json!({"@c":identity + 1,"@a":array});
    }
    value["unknown"]["reference"] = json!({"@r":30});
    let before = value.clone();
    let fields = rpg::fields(&value, &rpg::Auxiliary::default()).unwrap();
    let changes = vec![
        Change {
            id: "/variables/_data/@a/1".into(),
            value: json!(25),
        },
        Change {
            id: "/variables/_data/@a/2".into(),
            value: json!("新文字"),
        },
        Change {
            id: "/switches/_data/@a/1".into(),
            value: json!(true),
        },
    ];
    validate_changes(&fields, &changes).unwrap();
    rpg::apply(&mut value, &changes).unwrap();
    let mut expected = before.clone();
    expected["variables"]["_data"]["@a"][1] = json!(25);
    expected["variables"]["_data"]["@a"][2] = json!("新文字");
    expected["switches"]["_data"]["@a"][1] = json!(true);
    assert_eq!(value, expected);
    assert!(fields
        .iter()
        .any(|f| f.id == "/actors/_data/@a/1" && !f.editable));

    // Exercise discovery/read/atomic write using a synthetic save, never a user's save.
    fs::create_dir_all(dir.path().join("save")).unwrap();
    let save = dir.path().join("save/file1.rpgsave");
    fs::write(
        &save,
        lz_str::compress_to_base64(before.to_string().as_str()),
    )
    .unwrap();
    let slot = list(&game).unwrap().slots.remove(0);
    let document = read(&game, &slot.id).unwrap();
    apply(&game, &slot.id, &document.revision, &changes).unwrap();
    let output = fs::read(&save).unwrap();
    assert_eq!(rpg::decode(&output, "MV").unwrap().value, expected);
    export_fixture("legacy.rpgsave", &output);

    for identity in [10, 11] {
        let mut shared = before.clone();
        shared["unknown"]["reference"] = json!({"@r":identity});
        let fields = rpg::fields(&shared, &rpg::Auxiliary::default()).unwrap();
        assert!(fields
            .iter()
            .filter(|f| f.id.starts_with("/variables/"))
            .all(|f| !f.editable));
        assert!(validate_changes(&fields, &changes[..1]).is_err());
        assert!(fields
            .iter()
            .any(|f| f.id == "/switches/_data/@a/1" && f.editable));
    }
    value["variables"]["_data"]["@c"] = json!("invalid identity");
    assert!(rpg::fields(&value, &rpg::Auxiliary::default()).is_err());
}
#[test]
fn rpg_declared_unassigned_and_inventory_additions_preserve_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let game = game(dir.path());
    let data = dir.path().join("wrapper/www/data");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("System.json"),json!({"variables":["","Coin","Text","Flag","Object","Unset"],"switches":["","A","B","Unset"],"currencyUnit":"G"}).to_string()).unwrap();
    fs::write(
        data.join("Items.json"),
        json!([null,{"id":1,"name":"Potion","description":"Heal"},{"id":2,"name":"New Item"}])
            .to_string(),
    )
    .unwrap();
    let auxiliary = rpg::Auxiliary::load(&game);
    let mut value = rpg_value();
    let before = value.clone();
    let fields = rpg::fields(&value, &auxiliary).unwrap();
    let changes = vec![
        change(field(&fields, "#5 Unset"), json!(9)),
        change(field(&fields, "#3 Unset"), json!(true)),
        change(field(&fields, "道具 #2 New Item"), json!(4)),
        Change {
            id: "/party/_gold".into(),
            value: json!(999),
        },
    ];
    validate_changes(&fields, &changes).unwrap();
    rpg::apply(&mut value, &changes).unwrap();
    assert_eq!(value["variables"]["_data"][5], 9);
    assert_eq!(value["switches"]["_data"][3], true);
    assert_eq!(value["party"]["_items"]["2"], 4);
    assert_eq!(value["unknown"], before["unknown"]);
    assert_eq!(value["actors"], before["actors"]);
    assert_eq!(
        value["variables"]["_data"][4],
        before["variables"]["_data"][4]
    );
    assert!(validate_changes(
        &fields,
        &[Change {
            id: "/party/_items/999".into(),
            value: json!(2)
        }]
    )
    .is_err());
    assert!(validate_changes(
        &fields,
        &[Change {
            id: "/variables/_data/2".into(),
            value: json!(3)
        }]
    )
    .is_err());
    assert!(rpg::apply(
        &mut value,
        &[Change {
            id: "/party/_items/1".into(),
            value: json!(-1)
        }]
    )
    .is_err());
    assert!(
        !fields
            .iter()
            .find(|f| f.category == "actors")
            .unwrap()
            .editable
    );
    let missing = rpg::fields(&before, &rpg::Auxiliary::default()).unwrap();
    assert!(missing
        .iter()
        .any(|f| f.id == "/variables/_data/1" && f.editable));
    assert!(
        !missing
            .iter()
            .find(|f| f.id == "/variables/_data/4")
            .unwrap()
            .editable
    );
}
#[test]
fn pickle_protocols_scalar_changes_complex_objects_and_shared_memo() {
    for fixture in fixtures()["fixtures"].as_array().unwrap() {
        let bytes = raw(fixture, "log");
        let pickle = pickle::Pickle::parse(&bytes).unwrap();
        let fields = pickle.fields(false).unwrap();
        assert_eq!(field(&fields, "store.name").value, "勇者");
        assert!(!field(&fields, "store.list").editable);
        assert!(!field(&fields, "store.none").editable);
        assert!(!field(&fields, "store.shared").editable);
        let ids: HashSet<_> = fields.iter().map(|field| &field.id).collect();
        assert_eq!(ids.len(), fields.len());
        let changes = vec![
            change(field(&fields, "store.money"), json!(321)),
            change(field(&fields, "store.rate"), json!(3.75)),
            change(field(&fields, "store.ready"), json!(true)),
            change(
                field(&fields, "store.name"),
                json!("新勇者\n长字符串\\unicode 🎮"),
            ),
        ];
        let next = pickle.patch(&bytes, false, &changes).unwrap();
        export_fixture(&format!("log-{}.pickle", fixture["protocol"]), &next);
        let next_fields = pickle::Pickle::parse(&next).unwrap().fields(false).unwrap();
        assert_eq!(field(&next_fields, "store.money").value, 321);
        assert_eq!(field(&next_fields, "store.name").value, changes[3].value);
        assert!(pickle
            .patch(
                &bytes,
                false,
                &[change(field(&fields, "store.shared"), json!("unsafe"))]
            )
            .is_err());
        assert!(pickle
            .patch(
                &bytes,
                false,
                &[change(field(&fields, "store.money"), json!(3.5))]
            )
            .is_err());
        assert!(pickle::Pickle::parse(&bytes[..bytes.len() - 1]).is_err());
        let persistent = raw(fixture, "persistent");
        let pickle = pickle::Pickle::parse(&persistent).unwrap();
        let fields = pickle.fields(true).unwrap();
        assert_eq!(field(&fields, "money").value, 12);
        assert!(!field(&fields, "complex").editable);
        assert!(!fields.iter().any(|f| f.name.starts_with('_')));
        let next = pickle
            .patch(
                &persistent,
                true,
                &[
                    change(field(&fields, "name"), json!("changed")),
                    change(field(&fields, "money"), json!(25)),
                ],
            )
            .unwrap();
        export_fixture(&format!("persistent-{}.pickle", fixture["protocol"]), &next);
        assert_eq!(
            field(
                &pickle::Pickle::parse(&next).unwrap().fields(true).unwrap(),
                "name"
            )
            .value,
            "changed"
        );
    }
    let duplicate = b"(dVstore.money\nI1\nsVstore.money\nI2\nsN\x86.";
    let fields = pickle::Pickle::parse(duplicate)
        .unwrap()
        .fields(false)
        .unwrap();
    assert_eq!(fields.len(), 2);
    assert!(fields.iter().all(|f| !f.editable));
    assert!(pickle::Pickle::parse(b"\x80\x05\x97.").is_err());
    assert!(pickle::Pickle::parse(b"\x80\x04\x95\xff\xff\xff\xff\xff\xff\xff\xffN.").is_err());
}
#[test]
fn signing_matches_independent_python_sha1_raw_ecdsa_and_rejects_untrusted_keys() {
    let fixtures = fixtures();
    let fixture = &fixtures["fixtures"][5];
    let log = raw(fixture, "log");
    let signature = fixture["signatures"].as_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("security_keys.txt");
    assert!(renpy::fixture_keys(std::slice::from_ref(&path), &log, signature).is_err());
    fs::write(&path, fixtures["verifyingKey"].as_str().unwrap()).unwrap();
    assert!(renpy::fixture_keys(std::slice::from_ref(&path), &log, signature).is_err());
    fs::write(&path, "signing-key invalid\n").unwrap();
    assert!(renpy::fixture_keys(std::slice::from_ref(&path), &log, signature).is_err());
    fs::write(&path, fixtures["signingKey"].as_str().unwrap()).unwrap();
    let key = renpy::fixture_keys(std::slice::from_ref(&path), &log, signature).unwrap();
    let signed = renpy::fixture_signature(&key, &log).unwrap();
    renpy::fixture_keys(std::slice::from_ref(&path), &log, &signed).unwrap();
    assert!(renpy::fixture_keys(std::slice::from_ref(&path), b"changed", signature).is_err());
    assert!(renpy::fixture_keys(std::slice::from_ref(&path), &log, "").is_err());
    let other = p256::SecretKey::from_slice(&[9; 32]).unwrap();
    let other = p256::ecdsa::SigningKey::from(other);
    let foreign = renpy::fixture_signature(&other, &log).unwrap();
    assert!(renpy::fixture_keys(std::slice::from_ref(&path), &log, &foreign).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        fixtures["signingKey"].as_str().unwrap()
    );
}
#[test]
fn renpy_zip_and_persistent_edits_keep_metadata_and_resign_original_data() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixtures();
    let sample = &fixture["fixtures"][5];
    let saves = dir.path().join("game");
    let tokens = dir.path().join("tokens");
    fs::create_dir_all(&saves).unwrap();
    fs::create_dir_all(&tokens).unwrap();
    fs::write(
        tokens.join("security_keys.txt"),
        fixture["signingKey"].as_str().unwrap(),
    )
    .unwrap();
    let log = raw(sample, "log");
    let original = zip(&log, sample["signatures"].as_str().unwrap());
    let path = saves.join("1-1.save");
    let (fields, metadata, image, warnings) = renpy::read(&path, &original, false).unwrap();
    assert!(warnings.is_empty());
    assert!(image.is_some());
    assert!(metadata.iter().any(|m| m.contains("Chapter 2")));
    let updated = renpy::apply(
        &path,
        &original,
        false,
        &[change(field(&fields, "store.money"), json!(7))],
    )
    .unwrap();
    export_fixture("signed.save", &updated);
    let (next, _, _, warnings) = renpy::read(&path, &updated, false).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(field(&next, "store.money").value, 7);
    let mut old = ZipArchive::new(Cursor::new(&original)).unwrap();
    let mut new = ZipArchive::new(Cursor::new(&updated)).unwrap();
    assert_eq!(old.comment(), new.comment());
    assert_eq!(old.len(), new.len());
    for name in ["unknown.bin", "screenshot.png", "json", "extra_info"] {
        let mut a = Vec::new();
        let mut b = Vec::new();
        old.by_name(name).unwrap().read_to_end(&mut a).unwrap();
        new.by_name(name).unwrap().read_to_end(&mut b).unwrap();
        assert_eq!(a, b);
    }
    let raw = raw(sample, "persistent");
    let mut original = rpg::deflate(&raw, 3).unwrap();
    original.extend_from_slice(sample["persistentSignatures"].as_str().unwrap().as_bytes());
    let path = saves.join("persistent");
    let (fields, _, _, warnings) = renpy::read(&path, &original, true).unwrap();
    assert!(warnings.is_empty());
    let updated = renpy::apply(
        &path,
        &original,
        true,
        &[change(field(&fields, "money"), json!(25))],
    )
    .unwrap();
    export_fixture("signed-persistent", &updated);
    assert_eq!(
        field(&renpy::read(&path, &updated, true).unwrap().0, "money").value,
        25
    );
}
#[test]
fn associated_slot_write_revision_running_game_unicode_and_no_backups() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("中文 🎮");
    fs::create_dir_all(root.join("save")).unwrap();
    let game = game(&root);
    let slot = root.join("save/file1.rpgsave");
    let original = lz_str::compress_to_base64(rpg_value().to_string().as_str());
    fs::write(&slot, &original).unwrap();
    fs::write(root.join("save/file2.rpgsave"), &original).unwrap();
    let catalog = list(&game).unwrap();
    assert_eq!(catalog.slots.len(), 2);
    let id = catalog
        .slots
        .iter()
        .find(|s| s.name.contains("file1"))
        .unwrap()
        .id
        .clone();
    let mut single = game.clone();
    single.save_paths = vec!["<GAME>/save/file1.rpgsave".into()];
    assert_eq!(list(&single).unwrap().slots.len(), 1);
    assert_eq!(read(&single, &id).unwrap().slot.id, id);
    assert!(read(&game, &slot.display().to_string()).is_err());
    let mut other = game.clone();
    other.id = "other game".into();
    assert!(read(&other, &id).is_err());
    let doc = read(&game, &id).unwrap();
    let changes = [Change {
        id: "/party/_gold".into(),
        value: json!(777),
    }];
    let next = apply(&game, &id, &doc.revision, &changes).unwrap();
    assert_eq!(field(&next.fields, "金钱 ").value, 777);
    assert_eq!(
        fs::read_to_string(root.join("save/file2.rpgsave")).unwrap(),
        original
    );
    assert!(apply(&game, &id, &doc.revision, &changes).is_err());
    assert_eq!(fs::read_dir(root.join("save")).unwrap().count(), 2);
    fs::write(&slot, &original).unwrap();
    assert!(apply(&game, &id, &next.revision, &changes).is_err());
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let doc = read(&game, &id).unwrap();
        let _held = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&slot)
            .unwrap();
        assert!(apply(&game, &id, &doc.revision, &changes).is_err());
        assert_eq!(fs::read_to_string(&slot).unwrap(), original);
        assert_eq!(fs::read_dir(root.join("save")).unwrap().count(), 2);
    }
}
