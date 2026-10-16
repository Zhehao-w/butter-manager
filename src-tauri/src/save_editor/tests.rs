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

fn unity_fixtures() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/unity.json")).unwrap()
}
fn unity_changes(fields: &[Field], old: bool) -> Vec<Change> {
    vec![
        change(
            field(fields, "coins"),
            if old { json!("99") } else { json!(99) },
        ),
        change(field(fields, "player_name"), json!("测试 / 😀")),
        change(
            field(fields, "met"),
            if old { json!("true") } else { json!(true) },
        ),
    ]
}

#[test]
fn unity_independent_vectors_patch_only_selected_tokens_preserving_types_and_wrappers() {
    for f in unity_fixtures() {
        let input = raw(&f, "data");
        let format = f["format"].as_str().unwrap();
        let password = f["password"].as_str();
        let decoded = unity::decode(&input, format, password).unwrap();
        let fields = decoded.fields(format).unwrap();
        if format == unity::ES3 {
            assert!(!field(&fields, "large").editable);
            assert!(!field(&fields, "custom").editable);
        }
        let unchanged = decoded
            .apply(
                &input,
                format,
                &[change(
                    field(&fields, "coins"),
                    field(&fields, "coins").value.clone(),
                )],
                password,
            )
            .unwrap();
        assert_eq!(
            unchanged, input,
            "no-op must keep exact bytes, including IV / compression"
        );
        let changes = unity_changes(&fields, f["id"].as_str().unwrap().contains("old-global"));
        let output = decoded.apply(&input, format, &changes, password).unwrap();
        let after = unity::decode(&output, format, password)
            .unwrap()
            .fields(format)
            .unwrap();
        for original in &fields {
            let expected = changes
                .iter()
                .find(|c| c.id == original.id)
                .map(|c| &c.value)
                .unwrap_or(&original.value);
            assert!(
                same_value(&field(&after, &original.name).value, expected),
                "{} / {}",
                f["id"],
                original.name
            );
        }
        export_fixture(&format!("unity-{}.bin", f["id"].as_str().unwrap()), &output);
    }
}

#[test]
fn unity_slot_writes_are_isolated_revision_checked_and_never_back_up_sources() {
    for f in unity_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dunce::canonicalize(dir.path()).unwrap();
        fs::create_dir(dir.join("save")).unwrap();
        let game = game(&dir);
        let format = f["format"].as_str().unwrap();
        let extension = if format == unity::ES3 {
            "es3"
        } else if f["wrapper"] == "plain" {
            "json"
        } else {
            "nson"
        };
        let path = dir.join(format!("save/GameSave001.{extension}"));
        let other = dir.join(format!("save/GameSave002.{extension}"));
        let bytes = raw(&f, "data");
        fs::write(&path, &bytes).unwrap();
        fs::write(&other, &bytes).unwrap();
        fs::write(dir.join("save/settings.json"), b"{\"coins\":7}").unwrap();
        let catalog = list(&game).unwrap();
        assert_eq!(catalog.slots.len(), 2);
        let slot = catalog
            .slots
            .iter()
            .find(|s| s.name.starts_with("GameSave001."))
            .unwrap();
        let password = f["password"].as_str();
        if password.is_some() {
            assert!(read(&game, &slot.id).unwrap().password_required);
            let locked = read(&game, &slot.id).unwrap();
            assert!(apply_with_password(
                &game,
                &slot.id,
                &locked.revision,
                &[Change {
                    id: "es3/coins".into(),
                    value: json!(99)
                }],
                false,
                Some("wrong-password")
            )
            .is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let doc = read_with_password(&game, &slot.id, password).unwrap();
        let changes = unity_changes(
            &doc.fields,
            f["id"].as_str().unwrap().contains("old-global"),
        );
        let next =
            apply_with_password(&game, &slot.id, &doc.revision, &changes, false, password).unwrap();
        assert_ne!(next.revision, doc.revision);
        let updated = fs::read(&path).unwrap();
        assert!(
            apply_with_password(&game, &slot.id, &doc.revision, &changes, false, password).is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), updated);
        assert_eq!(fs::read(&other).unwrap(), bytes);
        assert_eq!(fs::read_dir(dir.join("save")).unwrap().count(), 3);
        assert!(apply_with_password(
            &game,
            "not-a-grant",
            &next.revision,
            &changes,
            false,
            password
        )
        .is_err());
    }
}

#[test]
fn unity_naninovel_refuses_ambiguous_maps_and_unsupported_values_without_editing_other_services() {
    let f = unity_fixtures().remove(4);
    let bytes = raw(&f, "data");
    let mut outer: Value = serde_json::from_slice(&bytes).unwrap();
    outer["objectJsonMap"]["keys"].as_array_mut().unwrap().pop();
    assert!(unity::decode(&serde_json::to_vec(&outer).unwrap(), unity::NANI, None).is_err());
    let mut outer: Value = serde_json::from_slice(&bytes).unwrap();
    outer["objectJsonMap"]["keys"][1] = outer["objectJsonMap"]["keys"][0].clone();
    assert!(unity::decode(&serde_json::to_vec(&outer).unwrap(), unity::NANI, None).is_err());
    outer["objectJsonMap"]["keys"][1] =
        "Naninovel.CustomVariableManager+GameState, Different.Assembly".into();
    outer["objectJsonMap"]["values"][1] = outer["objectJsonMap"]["values"][0].clone();
    assert!(unity::decode(&serde_json::to_vec(&outer).unwrap(), unity::NANI, None).is_err());
    let mut outer: Value = serde_json::from_slice(&bytes).unwrap();
    let mut service: Value =
        serde_json::from_str(outer["objectJsonMap"]["values"][0].as_str().unwrap()).unwrap();
    service["LocalVariables"][0]["name"] = service["LocalVariables"][1]["name"].clone();
    outer["objectJsonMap"]["values"][0] = serde_json::to_string(&service).unwrap().into();
    assert!(unity::decode(&serde_json::to_vec(&outer).unwrap(), unity::NANI, None).is_err());
    service["LocalVariables"][0]["name"] = "coins".into();
    service["LocalVariables"][0]["value"]["type"] = 999.into();
    outer["objectJsonMap"]["values"][0] = serde_json::to_string(&service).unwrap().into();
    let fields = unity::decode(&serde_json::to_vec(&outer).unwrap(), unity::NANI, None)
        .unwrap()
        .fields(unity::NANI)
        .unwrap();
    assert!(!field(&fields, "coins").editable);
    assert!(field(&fields, "player_name").editable);
    assert_eq!(
        fields.len(),
        3,
        "unknown game-specific services must not become editable variables"
    );
}

#[test]
fn unity_rejects_wrong_password_truncation_trailing_bytes_duplicates_and_type_changes() {
    for f in unity_fixtures() {
        let bytes = raw(&f, "data");
        let format = f["format"].as_str().unwrap();
        let password = f["password"].as_str();
        if password.is_some() {
            assert!(unity::decode(&bytes, format, Some("wrong-password")).is_err());
            assert!(unity::decode(&bytes, format, None).is_err());
        }
        for cut in [1, bytes.len() / 2, bytes.len() - 1] {
            assert!(
                unity::decode(&bytes[..cut], format, password).is_err(),
                "truncated {} at {cut}",
                f["id"]
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0x01);
        assert!(unity::decode(&trailing, format, password).is_err());
        let decoded = unity::decode(&bytes, format, password).unwrap();
        let fields = decoded.fields(format).unwrap();
        let coins = field(&fields, "coins");
        if coins.kind == "number" {
            for value in [json!("oops"), json!(true), json!(1e30)] {
                assert!(decoded
                    .apply(&bytes, format, &[change(coins, value)], password)
                    .is_err());
            }
        }
        if format == unity::ES3 {
            for value in [json!(1.5), json!(2147483648i64)] {
                assert!(decoded
                    .apply(&bytes, format, &[change(coins, value)], password)
                    .is_err());
            }
            assert!(decoded
                .apply(
                    &bytes,
                    format,
                    &[change(field(&fields, "custom"), json!(1))],
                    password
                )
                .is_err());
        }
    }
    for bytes in [
        br#"{"x":1,"x":2}"#.as_slice(),
        br#"{"x":{"__type":"int","value":1,"value":2}}"#,
        br#"{"x":{"__type":"int","value":1,}}"#,
    ] {
        assert!(unity::decode(bytes, unity::ES3, None).is_err());
    }
    assert!(json_tree::Node::parse(b"{\x0b\"x\":1}").is_err());
    let deep = format!("{}0{}", "[".repeat(66), "]".repeat(66));
    assert!(json_tree::Node::parse(deep.as_bytes()).is_err());
}

#[test]
fn unity_discovers_only_associated_files_and_one_saves_level_without_following_links() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dunce::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(dir.join("save/Saves/deeper")).unwrap();
    fs::create_dir_all(dir.join("save/other")).unwrap();
    let bytes = raw(&unity_fixtures()[4], "data");
    fs::write(dir.join("save/Saves/GameSave001.json"), &bytes).unwrap();
    fs::write(dir.join("save/Saves/deeper/GameSave002.json"), &bytes).unwrap();
    fs::write(dir.join("save/other/GameSave003.json"), &bytes).unwrap();
    let mut game = game(&dir);
    game.save_paths.push("<GAME>/save/Saves".into());
    let catalog = list(&game).unwrap();
    assert_eq!(catalog.slots.len(), 1);
    let doc = read(&game, &catalog.slots[0].id).unwrap();
    apply(
        &game,
        &doc.slot.id,
        &doc.revision,
        &unity_changes(&doc.fields, false),
        false,
    )
    .unwrap();
    assert_eq!(
        fs::read(dir.join("save/Saves/deeper/GameSave002.json")).unwrap(),
        bytes
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            dir.join("save/Saves/GameSave001.json"),
            dir.join("save/linked.nson"),
        )
        .unwrap();
        assert_eq!(list(&game).unwrap().slots.len(), 1);
        let linked = dir.join("linked");
        fs::create_dir(&linked).unwrap();
        std::os::unix::fs::symlink(dir.join("save/Saves"), linked.join("Saves")).unwrap();
        game.save_paths = vec!["<GAME>/linked".into()];
        assert!(list(&game).unwrap().slots.is_empty());
    }
}

#[test]
fn unity_cipher_iv_cannot_be_confused_with_json_or_gzip_and_large_deflate_is_complete() {
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    let f = unity_fixtures().remove(0);
    let plain = raw(&f, "data");
    for prefix in [b"{ ".as_slice(), &[0x1f, 0x8b]] {
        let mut iv = [0; 16];
        iv[..prefix.len()].copy_from_slice(prefix);
        let mut key = [0; 16];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(b"test", &iv, 100, &mut key);
        let mut bytes = iv.to_vec();
        bytes.extend(
            cbc::Encryptor::<aes::Aes128>::new_from_slices(&key, &iv)
                .unwrap()
                .encrypt_padded_vec_mut::<Pkcs7>(&plain),
        );
        assert!(unity::locked(&bytes, unity::ES3));
        assert!(unity::decode(&bytes, unity::ES3, Some("test")).is_ok());
    }
    let f = unity_fixtures().remove(4);
    let bytes = raw(&f, "data");
    let text = String::from_utf8(bytes)
        .unwrap()
        .replace("synthetic-placeholder", &"a".repeat(40_000));
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(text.as_bytes()).unwrap();
    let bytes = e.finish().unwrap();
    assert!(unity::decode(&bytes, unity::NANI, None).is_ok());
    assert!(unity::decode(&bytes[..bytes.len() - 1], unity::NANI, None).is_err());
}

#[test]
fn renpy_collections_edit_children_nulls_and_known_sets_preserving_types_and_aliases() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/renpy-collections.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let protocol = fixture["protocol"].as_u64().unwrap();
        let bytes = raw(fixture, "persistent");
        let pickle = pickle::Pickle::parse(&bytes).unwrap();
        let fields = pickle.fields(true).unwrap();
        assert_eq!(field(&fields, "data").kind, "container");
        assert_eq!(
            field(&fields, "data[\"list\"]").type_name.as_deref(),
            Some("list")
        );
        assert_eq!(
            field(&fields, "data[\"tuple\"]").type_name.as_deref(),
            Some("tuple")
        );
        assert!(field(&fields, "data[\"list\"][0]").editable);
        assert!(!field(&fields, "shared_a[0][\"score\"]").editable);
        assert!(
            fields
                .iter()
                .any(|f| f.path.starts_with("cycle[")
                    && f.reason.as_deref() == Some("循环引用，只读"))
        );
        assert!(!field(&fields, "custom").editable);
        assert!(!field(&fields, "large").editable);
        let mut changes = vec![
            change(field(&fields, "data[\"list\"][0]"), json!(42)),
            change(field(&fields, "data[\"list\"][2]"), json!("initialized")),
            change(field(&fields, "data[\"tuple\"][1]"), json!(3.75)),
            change(field(&fields, "null_value"), json!(true)),
        ];
        if protocol >= 2 {
            assert!(field(&fields, "gallery_unlocked").editable);
            assert_eq!(field(&fields, "gallery_unlocked").kind, "string_set");
            assert_eq!(
                field(&fields, "gallery_unlocked").value,
                json!(["cg_a", "cg_b"])
            );
            assert_eq!(field(&fields, "empty_list").child_count, Some(0));
            changes.extend([
                change(
                    field(&fields, "gallery_unlocked"),
                    json!(["cg_new", "cg_b"]),
                ),
                change(field(&fields, "wrapper[\"flags\"][0]"), json!(true)),
            ]);
        } else {
            assert!(!field(&fields, "gallery_unlocked").editable);
        }
        let next = pickle.patch(&bytes, true, &changes).unwrap();
        let after = pickle::Pickle::parse(&next).unwrap().fields(true).unwrap();
        for change in changes {
            let field = after.iter().find(|f| f.id == change.id).unwrap();
            assert!(same_field_value(field, &change.value));
        }
        export_fixture(&format!("collections-{protocol}.pickle"), &next);
        if protocol >= 2 {
            for invalid in [
                json!(["duplicate", "duplicate"]),
                json!([42]),
                json!(["line\nbreak"]),
            ] {
                assert!(pickle
                    .patch(
                        &bytes,
                        true,
                        &[change(field(&fields, "gallery_unlocked"), invalid)]
                    )
                    .is_err());
            }
            let empty = pickle
                .patch(
                    &bytes,
                    true,
                    &[change(field(&fields, "gallery_unlocked"), json!([]))],
                )
                .unwrap();
            assert_eq!(
                field(
                    &pickle::Pickle::parse(&empty).unwrap().fields(true).unwrap(),
                    "gallery_unlocked"
                )
                .value,
                json!([])
            );
        }
    }
}

#[test]
fn renpy_nested_duplicates_shared_ancestors_and_globals_cannot_be_modified() {
    // True and 1 compare equal as Python dictionary keys, despite distinct pickle types.
    let bytes = b"\x80\x02}X\x01\x00\x00\x00d}(\x88K\x05K\x01K\x06us.";
    let pickle = pickle::Pickle::parse(bytes).unwrap();
    let fields = pickle.fields(true).unwrap();
    assert!(!field(&fields, "d[true]").editable);
    assert!(!field(&fields, "d[1]").editable);
    // A displayed string referenced by STACK_GLOBAL must stay read-only.
    let bytes = b"\x80\x04}(\x8c\x06module\x8c\x10renpy.revertable\x94\x8c\x05otherh\x00\x8c\x05Dummy\x93)\x81u.";
    let fields = pickle::Pickle::parse(bytes).unwrap().fields(true).unwrap();
    assert!(!field(&fields, "module").editable);
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
    apply(&game, &slot.id, &document.revision, &changes, false).unwrap();
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
        assert!(field(&fields, "store.none").editable);
        assert_eq!(field(&fields, "store.none").kind, "null");
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
fn signing_matches_independent_sha1_fixture_and_inspects_without_requiring_original_key() {
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
    assert_eq!(
        renpy::fixture_inspect(std::slice::from_ref(&path), &log, signature).status,
        "local"
    );
    assert_eq!(
        renpy::fixture_inspect(std::slice::from_ref(&path), b"changed", signature).status,
        "invalid"
    );
    assert_eq!(
        renpy::fixture_inspect(std::slice::from_ref(&path), &log, "").status,
        "unsigned"
    );
    let other = p256::SecretKey::from_slice(&[9; 32]).unwrap();
    let other = p256::ecdsa::SigningKey::from(other);
    let foreign = renpy::fixture_signature(&other, &log).unwrap();
    assert_eq!(
        renpy::fixture_inspect(std::slice::from_ref(&path), &log, &foreign).status,
        "foreign"
    );
    renpy::fixture_keys(std::slice::from_ref(&path), &log, &foreign).unwrap();
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
    let saves = dir.path().join("Ren'Py Data/game");
    let tokens = dir.path().join("Ren'Py Data/tokens");
    fs::create_dir_all(&saves).unwrap();
    fs::create_dir_all(&tokens).unwrap();
    fs::write(
        tokens.join("security_keys.txt"),
        fixture["signingKey"].as_str().unwrap(),
    )
    .unwrap();
    let log = raw(sample, "log");
    let original = zip(&log, sample["signatures"].as_str().unwrap());
    let mut game = game(dir.path());
    game.save_paths = vec![saves.display().to_string()];
    let (fields, metadata, image, warnings, _) = renpy::read(&game, &original, false).unwrap();
    assert!(warnings.is_empty());
    assert!(image.is_some());
    assert!(metadata.iter().any(|m| m.contains("Chapter 2")));
    let updated = renpy::apply(
        &game,
        &original,
        false,
        &[change(field(&fields, "store.money"), json!(7))],
    )
    .unwrap();
    export_fixture("signed.save", &updated);
    let (next, _, _, warnings, _) = renpy::read(&game, &updated, false).unwrap();
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
    let (fields, _, _, warnings, _) = renpy::read(&game, &original, true).unwrap();
    assert!(warnings.is_empty());
    let updated = renpy::apply(
        &game,
        &original,
        true,
        &[change(field(&fields, "money"), json!(25))],
    )
    .unwrap();
    export_fixture("signed-persistent", &updated);
    assert_eq!(
        field(&renpy::read(&game, &updated, true).unwrap().0, "money").value,
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
    let next = apply(&game, &id, &doc.revision, &changes, false).unwrap();
    assert_eq!(field(&next.fields, "金钱 ").value, 777);
    assert_eq!(
        fs::read_to_string(root.join("save/file2.rpgsave")).unwrap(),
        original
    );
    assert!(apply(&game, &id, &doc.revision, &changes, false).is_err());
    assert_eq!(fs::read_dir(root.join("save")).unwrap().count(), 2);
    fs::write(&slot, &original).unwrap();
    assert!(apply(&game, &id, &next.revision, &changes, false).is_err());
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let doc = read(&game, &id).unwrap();
        let _held = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&slot)
            .unwrap();
        assert!(apply(&game, &id, &doc.revision, &changes, false).is_err());
        assert_eq!(fs::read_to_string(&slot).unwrap(), original);
        assert_eq!(fs::read_dir(root.join("save")).unwrap().count(), 2);
    }
}

fn renpy_game(root: &Path) -> Game {
    let mut game = game(root);
    game.engine = "Ren'Py".into();
    game.save_paths = vec!["<GAME>/Ren'Py Data/fixture".into()];
    fs::create_dir_all(root.join("Ren'Py Data/fixture")).unwrap();
    fs::create_dir_all(root.join("Ren'Py Data/tokens")).unwrap();
    fs::write(
        root.join("Ren'Py Data/tokens/security_keys.txt"),
        fixtures()["signingKey"].as_str().unwrap(),
    )
    .unwrap();
    game
}
fn member(data: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(data)).unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}
#[test]
fn associated_renpy_trust_is_required_for_the_checked_revision_only() {
    let dir = tempfile::tempdir().unwrap();
    let game = renpy_game(dir.path());
    let sample = &fixtures()["fixtures"][5];
    let foreign = p256::ecdsa::SigningKey::from(p256::SecretKey::from_slice(&[9; 32]).unwrap());
    for persistent in [false, true] {
        let path = dir.path().join("Ren'Py Data/fixture").join(if persistent {
            "persistent"
        } else {
            "one.save"
        });
        let log = raw(sample, if persistent { "persistent" } else { "log" });
        for (status, signature) in [
            ("foreign", renpy::fixture_signature(&foreign, &log).unwrap()),
            ("unsigned", String::new()),
            ("invalid", "signature damaged damaged\n".into()),
            ("unknown", "signature AAE= AAE=\n".into()),
        ] {
            let original = if persistent {
                let mut bytes = rpg::deflate(&log, 3).unwrap();
                bytes.extend_from_slice(signature.as_bytes());
                bytes
            } else {
                zip(&log, &signature)
            };
            fs::write(&path, &original).unwrap();
            let slot = list(&game)
                .unwrap()
                .slots
                .into_iter()
                .find(|s| s.format == if persistent { "Persistent" } else { "RenPy" })
                .unwrap();
            let doc = read(&game, &slot.id).unwrap();
            assert_eq!(doc.signature.as_ref().unwrap().status, status);
            let changes = [change(
                field(
                    &doc.fields,
                    if persistent { "money" } else { "store.money" },
                ),
                json!(29),
            )];
            for error in [
                apply(&game, &slot.id, &doc.revision, &changes, false).unwrap_err(),
                resign(&game, &slot.id, &doc.revision, false).unwrap_err(),
            ] {
                assert!(error.to_string().contains("来源可信"));
            }
            assert_eq!(fs::read(&path).unwrap(), original);

            // An explicit confirmation can re-sign this revision; new local signatures need none.
            let signed = resign(&game, &slot.id, &doc.revision, true).unwrap();
            assert_eq!(signed.signature.as_ref().unwrap().status, "local");
            assert_eq!(
                renpy::fixture_log(&fs::read(&path).unwrap(), persistent).unwrap(),
                log
            );
            let edited = apply(&game, &slot.id, &signed.revision, &changes, false).unwrap();
            assert_eq!(
                field(
                    &edited.fields,
                    if persistent { "money" } else { "store.money" }
                )
                .value,
                29
            );
            let local = resign(&game, &slot.id, &edited.revision, false).unwrap();
            assert_eq!(local.signature.unwrap().status, "local");

            // Confirmation of the former revision cannot overwrite changed content.
            fs::write(&path, &original).unwrap();
            assert!(resign(&game, &slot.id, &signed.revision, true)
                .unwrap_err()
                .to_string()
                .contains("更新"));
            let changed = read(&game, &slot.id).unwrap();
            assert!(resign(&game, &slot.id, &changed.revision, false).is_err());
            // A timestamp-only change also invalidates the expected revision.
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(
                    fs::FileTimes::new().set_modified(
                        std::time::SystemTime::now() + std::time::Duration::from_secs(5),
                    ),
                )
                .unwrap();
            assert!(apply(&game, &slot.id, &changed.revision, &changes, true)
                .unwrap_err()
                .to_string()
                .contains("更新"));
            let refreshed = read(&game, &slot.id).unwrap();
            assert_ne!(refreshed.revision, changed.revision);
            assert!(apply(&game, &slot.id, &refreshed.revision, &changes, false).is_err());
            apply(&game, &slot.id, &refreshed.revision, &changes, true).unwrap();
        }
    }
}
fn without_signatures(data: &[u8]) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(data)).unwrap();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let member = archive.by_index(i).unwrap();
        if member.name() != "signatures" {
            writer.raw_copy_file(member).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn renpy_resign_local_foreign_unsigned_invalid_and_unsupported_preserves_log_and_members() {
    let dir = tempfile::tempdir().unwrap();
    let game = renpy_game(dir.path());
    let fixtures = fixtures();
    let sample = &fixtures["fixtures"][5];
    let log = raw(sample, "log");
    let foreign = p256::ecdsa::SigningKey::from(p256::SecretKey::from_slice(&[9; 32]).unwrap());
    for (status, signature) in [
        ("local", sample["signatures"].as_str().unwrap().to_owned()),
        ("foreign", renpy::fixture_signature(&foreign, &log).unwrap()),
        ("unsigned", String::new()),
        ("invalid", "signature damaged damaged\n".into()),
    ] {
        let original = zip(&log, &signature);
        assert_eq!(
            renpy::read(&game, &original, false).unwrap().4.status,
            status
        );
        let updated = renpy::apply(&game, &original, false, &[]).unwrap();
        assert_eq!(member(&updated, "log"), log);
        assert_eq!(
            renpy::read(&game, &updated, false).unwrap().4.status,
            "local"
        );
        for name in ["json", "extra_info", "unknown.bin", "screenshot.png"] {
            assert_eq!(member(&updated, name), member(&original, name));
        }
        // Foreign/no/invalid original signatures no longer prevent controlled edits.
        let fields = renpy::read(&game, &original, false).unwrap().0;
        let edited = renpy::apply(
            &game,
            &original,
            false,
            &[change(field(&fields, "store.money"), json!(19))],
        )
        .unwrap();
        assert_eq!(
            field(
                &renpy::read(&game, &edited, false).unwrap().0,
                "store.money"
            )
            .value,
            19
        );
    }
    let original = without_signatures(&zip(&log, ""));
    let updated = renpy::apply(&game, &original, false, &[]).unwrap();
    assert!(!member(&updated, "signatures").is_empty());
    assert_eq!(member(&updated, "log"), log);
    // Protocol 5 out-of-band buffer is deliberately unsupported by the scalar parser.
    let complex = b"\x80\x05\x97.";
    for persistent in [false, true] {
        let original = if persistent {
            rpg::deflate(complex, 3).unwrap()
        } else {
            zip(complex, "")
        };
        assert!(renpy::read(&game, &original, persistent)
            .unwrap()
            .0
            .is_empty());
        let updated = renpy::apply(&game, &original, persistent, &[]).unwrap();
        assert_eq!(renpy::fixture_log(&updated, persistent).unwrap(), complex);
        assert_eq!(
            renpy::read(&game, &updated, persistent).unwrap().4.status,
            "local"
        );
    }
    let signed = renpy::apply(&game, &zip(&log, ""), false, &[]).unwrap();
    export_fixture("resigned.save", &signed);
    export_fixture("save-proof.json", &serde_json::to_vec(&json!({"log":STANDARD.encode(&log), "signatures":renpy::fixture_signatures(&signed, false).unwrap(), "trusted":fixtures["verifyingKey"]})).unwrap());
    assert_eq!(
        fs::read_to_string(dir.path().join("Ren'Py Data/tokens/security_keys.txt")).unwrap(),
        fixtures["signingKey"].as_str().unwrap()
    );
}
#[test]
fn persistent_foreign_unsigned_invalid_resign_and_controlled_edit() {
    let dir = tempfile::tempdir().unwrap();
    let game = renpy_game(dir.path());
    let raw = raw(&fixtures()["fixtures"][5], "persistent");
    let foreign = p256::ecdsa::SigningKey::from(p256::SecretKey::from_slice(&[7; 32]).unwrap());
    for signature in [
        String::new(),
        fixtures()["fixtures"][5]["persistentSignatures"]
            .as_str()
            .unwrap()
            .into(),
        renpy::fixture_signature(&foreign, &raw).unwrap(),
        "corrupt signature\n".into(),
    ] {
        let mut original = rpg::deflate(&raw, 3).unwrap();
        original.extend_from_slice(signature.as_bytes());
        let resigned = renpy::apply(&game, &original, true, &[]).unwrap();
        assert_eq!(renpy::fixture_log(&resigned, true).unwrap(), raw);
        assert_eq!(
            renpy::read(&game, &resigned, true).unwrap().4.status,
            "local"
        );
        let fields = renpy::read(&game, &original, true).unwrap().0;
        let updated = renpy::apply(
            &game,
            &original,
            true,
            &[change(field(&fields, "money"), json!(27))],
        )
        .unwrap();
        assert_eq!(
            field(&renpy::read(&game, &updated, true).unwrap().0, "money").value,
            27
        );
        assert_eq!(
            renpy::read(&game, &updated, true).unwrap().4.status,
            "local"
        );
        export_fixture("resigned-persistent", &updated);
        export_fixture("persistent-proof.json", &serde_json::to_vec(&json!({"log":STANDARD.encode(renpy::fixture_log(&updated, true).unwrap()), "signatures":renpy::fixture_signatures(&updated, true).unwrap(), "trusted":fixtures()["verifyingKey"]})).unwrap());
    }
    let compressed = rpg::deflate(&raw, 3).unwrap();
    assert!(renpy::apply(&game, &compressed[..compressed.len() - 2], true, &[]).is_err());
    let mut invalid_utf8 = compressed;
    invalid_utf8.extend_from_slice(&[255, 254]);
    assert_eq!(
        renpy::read(&game, &invalid_utf8, true).unwrap().4.status,
        "invalid"
    );
    renpy::apply(&game, &invalid_utf8, true, &[]).unwrap();
}
#[test]
fn target_tokens_multiple_keys_missing_invalid_no_fallback_or_external_key_use() {
    let dir = tempfile::tempdir().unwrap();
    let game = renpy_game(dir.path());
    let key_file = dir.path().join("Ren'Py Data/tokens/security_keys.txt");
    let fixture = fixtures();
    let log = raw(&fixture["fixtures"][5], "log");
    let original = zip(&log, "");
    for text in [
        "",
        "signing-key invalid\n",
        fixture["verifyingKey"].as_str().unwrap(),
    ] {
        fs::write(&key_file, text).unwrap();
        assert!(!renpy::read(&game, &original, false).unwrap().4.can_resign);
        assert!(renpy::apply(&game, &original, false, &[]).is_err());
    }
    fs::remove_file(&key_file).unwrap();
    assert!(renpy::apply(&game, &original, false, &[]).is_err());
    fs::write(
        &key_file,
        format!(
            "signing-key invalid\n{}\n{}",
            fixture["verifyingKey"].as_str().unwrap(),
            fixture["signingKey"].as_str().unwrap()
        ),
    )
    .unwrap();
    let updated = renpy::apply(&game, &original, false, &[]).unwrap();
    assert_eq!(
        renpy::read(&game, &updated, false).unwrap().4.status,
        "local"
    );
    let second = p256::SecretKey::from_slice(&[8; 32]).unwrap();
    let second = format!(
        "signing-key {}\n",
        STANDARD.encode(second.to_sec1_der().unwrap().as_slice())
    );
    fs::write(
        &key_file,
        format!("{second}{}", fixture["signingKey"].as_str().unwrap()),
    )
    .unwrap();
    let updated = renpy::apply(&game, &original, false, &[]).unwrap();
    assert_eq!(
        renpy::read(&game, &updated, false).unwrap().4.status,
        "local"
    );
    // Explicit environment override follows official priority and never falls back.
    let custom = dir.path().join("custom");
    assert_eq!(
        renpy::key_paths_with(&game, Some(&custom), Some(dir.path())).unwrap(),
        vec![custom.join("tokens/security_keys.txt")]
    );
    let paths = renpy::key_paths_with(&game, Some(&custom), Some(dir.path())).unwrap();
    assert!(renpy::fixture_keys(&paths, &log, "").is_err());
    fs::create_dir_all(custom.join("tokens")).unwrap();
    fs::write(&paths[0], &second).unwrap();
    let chosen = renpy::fixture_keys(&paths, &log, "").unwrap();
    assert_eq!(
        chosen,
        p256::ecdsa::SigningKey::from(p256::SecretKey::from_slice(&[8; 32]).unwrap())
    );
    assert_eq!(
        renpy::key_paths_with(&game, None, Some(dir.path())).unwrap(),
        vec![key_file.clone()]
    );
    let default_dir = tempfile::tempdir().unwrap();
    let default_game = self::game(default_dir.path());
    assert_eq!(
        renpy::key_paths_with(&default_game, None, Some(dir.path())).unwrap(),
        vec![dir.path().join("RenPy/tokens/security_keys.txt")]
    );
    // Nested executable determines the nearest Ren'Py Data, not a parent/unrelated store.
    fs::create_dir_all(dir.path().join("nested/Ren'Py Data/tokens")).unwrap();
    fs::write(
        dir.path()
            .join("nested/Ren'Py Data/tokens/security_keys.txt"),
        "signing-key invalid",
    )
    .unwrap();
    let mut nested = game.clone();
    nested.main_executable = Some("nested/game.exe".into());
    assert!(renpy::apply(&nested, &original, false, &[]).is_err());
}
#[test]
fn external_picker_grants_game_binding_revision_expiry_failure_and_no_backups() {
    let dir = tempfile::tempdir().unwrap();
    let game = renpy_game(dir.path());
    let external = dir.path().join("external");
    fs::create_dir(&external).unwrap();
    let path = external.join("outside.save");
    let original = zip(&raw(&fixtures()["fixtures"][5], "log"), "");
    fs::write(&path, &original).unwrap();
    let grants = ExternalSaves::default();
    assert!(read(&game, &path.display().to_string()).is_err());
    let doc = grants.choose(&game, &path).unwrap();
    assert!(doc.slot.external);
    let id = &doc.slot.id;
    let mut other = game.clone();
    other.id = "other".into();
    assert!(grants.read(&other, id).is_err());
    assert!(grants.write(&game, id, &doc.revision, None, false).is_err());
    let updated = grants.write(&game, id, &doc.revision, None, true).unwrap();
    assert_eq!(updated.signature.unwrap().status, "local");
    assert_eq!(
        member(&fs::read(&path).unwrap(), "log"),
        member(&original, "log")
    );
    // An external local-trusted save uses the same rules as an associated save.
    let updated = grants
        .write(&game, id, &updated.revision, None, false)
        .unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let held = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let before = fs::read(&path).unwrap();
        assert!(grants
            .write(&game, id, &updated.revision, None, true)
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read_dir(&external).unwrap().count(), 1);
        drop(held);
    }
    fs::write(&path, &original).unwrap();
    assert!(grants.read(&game, id).is_err());
    assert!(grants
        .write(&game, id, &updated.revision, None, true)
        .is_err());
    let refreshed = grants.choose(&game, &path).unwrap();
    grants
        .0
        .lock()
        .unwrap()
        .get_mut(&refreshed.slot.id)
        .unwrap()
        .created -= std::time::Duration::from_secs(7201);
    assert!(grants.read(&game, &refreshed.slot.id).is_err());
    grants.release(&game.id).unwrap();
    assert!(grants.read(&game, id).is_err());
    fs::write(&path, zip(b"\x80\x05\x97.", "")).unwrap();
    let complex = grants.choose(&game, &path).unwrap();
    assert!(complex.fields.is_empty());
    let signed = grants
        .write(&game, &complex.slot.id, &complex.revision, None, true)
        .unwrap();
    assert_eq!(signed.signature.unwrap().status, "local");
    assert_eq!(member(&fs::read(&path).unwrap(), "log"), b"\x80\x05\x97.");
    fs::write(external.join("invalid.save"), b"not a zip").unwrap();
    assert!(grants
        .choose(&game, &external.join("invalid.save"))
        .is_err());
    fs::write(external.join("script.rpy"), b"no").unwrap();
    assert!(grants.choose(&game, &external.join("script.rpy")).is_err());
}
#[test]
fn discovery_skips_disappearing_files_but_propagates_access_errors() {
    let dir = tempfile::tempdir().unwrap();
    let game = game(dir.path());
    fs::create_dir(dir.path().join("save")).unwrap();
    for name in ["file1.rpgsave", "file2.rpgsave"] {
        fs::write(dir.path().join("save").join(name), b"fixture").unwrap();
    }
    let (catalog, _) = discover_with(&game, |path| {
        if path.file_name().unwrap() == "file1.rpgsave" {
            fs::remove_file(path).unwrap();
        }
    })
    .unwrap();
    assert_eq!(catalog.slots.len(), 1);
    assert!(catalog.slots[0].name.contains("file2"));
    assert!(transient::<()>(Err(std::io::Error::from(
        std::io::ErrorKind::PermissionDenied
    )))
    .is_err());
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _held = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .custom_flags(0x02000000)
            .open(dir.path().join("save"))
            .unwrap();
        assert!(list(&game).is_err());
    }
}
#[test]
fn editor_permit_and_file_tasks_exclude_each_other_until_write_finishes() {
    let activity = Arc::new(Mutex::new(0));
    let tasks = Arc::new(crate::jobs::TaskManager::default());
    let dir = tempfile::tempdir().unwrap();
    let imports = Arc::new(crate::importer::ImportStore::open(dir.path().join("imports")).unwrap());
    let game = game(dir.path());
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    let count = activity.clone();
    let jobs = tasks.clone();
    let store = imports.clone();
    let target = game.clone();
    let writer = std::thread::spawn(move || {
        let (_, _permit) = WritePermit::acquire(count, || {
            if jobs.active() || store.blocks_game(&target) {
                return Err(invalid("blocked"));
            }
            Ok(())
        })
        .unwrap();
        started_tx.send(()).unwrap();
        finish_rx.recv().unwrap();
    });
    started_rx.recv().unwrap();
    // Same gate/check used by start_import_apply and start_version_rollback.
    assert_eq!(*activity.lock().unwrap(), 1);
    assert!(
        crate::runtime::ensure_request_idle(*activity.lock().unwrap(), tasks.active()).is_err()
    );
    assert!(WritePermit::acquire(activity.clone(), || -> Result<()> {
        panic!("an active request must reject before acquiring a write")
    })
    .is_err());
    finish_tx.send(()).unwrap();
    writer.join().unwrap();
    assert_eq!(*activity.lock().unwrap(), 0);
    let job = {
        let _gate = activity.lock().unwrap();
        tasks.begin("import_apply", String::new()).unwrap()
    };
    assert!(
        crate::runtime::ensure_request_idle(*activity.lock().unwrap(), tasks.active()).is_err()
    );
    assert!(WritePermit::acquire(activity.clone(), || {
        if tasks.active() || imports.blocks_game(&game) {
            return Err(invalid("blocked"));
        }
        Ok(())
    })
    .is_err());
    assert_eq!(*activity.lock().unwrap(), 0);
    job.finish(Ok(Vec::new()));
    crate::runtime::ensure_request_idle(*activity.lock().unwrap(), tasks.active()).unwrap();
    let (_, permit) = WritePermit::acquire(activity.clone(), || Ok(())).unwrap();
    assert_eq!(*activity.lock().unwrap(), 1);
    drop(permit);
    assert_eq!(*activity.lock().unwrap(), 0);
    fs::write(
        dir.path().join("imports/broken.json"),
        b"invalid synthetic recovery record",
    )
    .unwrap();
    let recovering = crate::importer::ImportStore::open(dir.path().join("imports")).unwrap();
    assert!(WritePermit::acquire(activity.clone(), || {
        if recovering.blocks_game(&game) {
            return Err(invalid("恢复操作未完成"));
        }
        Ok(())
    })
    .is_err());
    assert_eq!(*activity.lock().unwrap(), 0);
}
