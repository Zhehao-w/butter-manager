use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use p256::ecdsa::{
    signature::hazmat::{PrehashSigner, PrehashVerifier},
    Signature, SigningKey, VerifyingKey,
};
use p256::pkcs8::{DecodePublicKey, EncodePublicKey};
use std::io::Cursor;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

type View = (
    Vec<Field>,
    Vec<String>,
    Option<String>,
    Vec<String>,
    SignatureInfo,
);
#[derive(Debug, Clone, Serialize)]
pub struct SignatureInfo {
    pub status: String,
    pub can_resign: bool,
    pub reason: Option<String>,
}
struct Container {
    log: Vec<u8>,
    signatures: String,
    metadata: Vec<String>,
    screenshot: Option<String>,
}
fn zip_member(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut member = archive.by_name(name).map_err(format_error)?;
    if member.size() > limit as u64 {
        return Err(invalid(format!("Ren’Py {name} 数据过大")));
    }
    let mut output = Vec::new();
    member
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .map_err(format_error)?;
    if output.len() > limit {
        return Err(invalid("ZIP 解压数据过大"));
    }
    Ok(output)
}
fn unpack(data: &[u8], persistent: bool) -> Result<Container> {
    if persistent {
        let mut decoder = flate2::Decompress::new(true);
        let mut log = Vec::new();
        loop {
            let before = (decoder.total_in(), decoder.total_out());
            let mut buffer = [0; 16384];
            let status = decoder
                .decompress(
                    &data[decoder.total_in() as usize..],
                    &mut buffer,
                    flate2::FlushDecompress::None,
                )
                .map_err(format_error)?;
            log.extend_from_slice(&buffer[..(decoder.total_out() - before.1) as usize]);
            if log.len() > DECODE_LIMIT {
                return Err(invalid("persistent 解压数据过大"));
            }
            if status == flate2::Status::StreamEnd {
                break;
            }
            if before == (decoder.total_in(), decoder.total_out()) {
                return Err(invalid("persistent zlib 数据不完整"));
            }
        }
        let consumed = decoder.total_in() as usize;
        let signatures = String::from_utf8_lossy(
            data.get(consumed..)
                .ok_or_else(|| invalid("persistent 签名位置无效"))?,
        )
        .into_owned();
        if signatures.len() > 256 * 1024 {
            return Err(invalid("persistent 签名过大"));
        }
        return Ok(Container {
            log,
            signatures,
            metadata: vec!["Ren’Py persistent（跨槽位持久数据）".into()],
            screenshot: None,
        });
    }
    let mut archive = ZipArchive::new(Cursor::new(data)).map_err(format_error)?;
    if archive.len() > 256 {
        return Err(invalid("Ren’Py ZIP 文件数量过多"));
    }
    let mut names = HashSet::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(format_error)?;
        if !names.insert(file.name().to_owned()) {
            return Err(invalid("Ren’Py ZIP 中存在重复文件名，禁止修改"));
        }
        if file.size() > DECODE_LIMIT as u64 {
            return Err(invalid("Ren’Py ZIP 成员过大"));
        }
    }
    let log = zip_member(&mut archive, "log", DECODE_LIMIT)?;
    let signatures = if names.contains("signatures") {
        String::from_utf8_lossy(&zip_member(&mut archive, "signatures", 256 * 1024)?).into_owned()
    } else {
        String::new()
    };
    let screenshot = if names.contains("screenshot.png") {
        let image = zip_member(&mut archive, "screenshot.png", 4 * 1024 * 1024)?;
        image
            .starts_with(b"\x89PNG\r\n\x1a\n")
            .then(|| format!("data:image/png;base64,{}", STANDARD.encode(image)))
    } else {
        None
    };
    let mut metadata = Vec::new();
    if names.contains("json") {
        if let Ok(info) = zip_member(&mut archive, "json", 1024 * 1024)
            .and_then(|data| serde_json::from_slice::<Value>(&data).map_err(format_error))
        {
            for (key, label) in [
                ("_save_name", "章节"),
                ("_version", "游戏版本"),
                ("_ctime", "存档时间"),
            ] {
                if let Some(value) = info.get(key).filter(|v| v.is_string() || v.is_number()) {
                    metadata.push(format!("{label}：{value}"));
                }
            }
        }
    }
    if names.contains("extra_info") {
        if let Ok(info) = zip_member(&mut archive, "extra_info", 1024 * 1024)
            .and_then(|v| String::from_utf8(v).map_err(format_error))
        {
            if !info.trim().is_empty() {
                metadata.push(info);
            }
        }
    }
    Ok(Container {
        log,
        signatures,
        metadata,
        screenshot,
    })
}
// Key discovery is based on the target game, never on a selected external save.
fn key_paths(game: &Game) -> Result<Vec<PathBuf>> {
    let saves = std::env::var_os("RENPY_PATH_TO_SAVES").map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    key_paths_with(game, saves.as_deref(), appdata.as_deref())
}
pub(super) fn key_paths_with(
    game: &Game,
    saves: Option<&Path>,
    appdata: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    if let Some(root) = saves {
        return Ok(vec![root.join("tokens/security_keys.txt")]);
    }
    let base = game
        .main_executable
        .as_deref()
        .and_then(|exe| crate::paths::relative_path(exe).ok())
        .and_then(|exe| {
            exe.parent()
                .map(|parent| Path::new(&game.install_path).join(parent))
        })
        .unwrap_or_else(|| PathBuf::from(&game.install_path));
    for parent in base.ancestors() {
        let data = parent.join("Ren'Py Data");
        match fs::metadata(&data) {
            Ok(info) if info.is_dir() => return Ok(vec![data.join("tokens/security_keys.txt")]),
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    // A manually associated save directory does not establish a tokens directory.
    // Do not fall back to unrelated stores when the actual store has missing/bad keys.
    Ok(appdata
        .map(|root| vec![PathBuf::from(root).join("RenPy/tokens/security_keys.txt")])
        .unwrap_or_default())
}
type TokenLine<'a> = (&'a str, Vec<u8>, Option<Vec<u8>>);
fn decode_line(line: &str) -> Result<Option<TokenLine<'_>>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let parts: Vec<_> = line.split_whitespace().collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(invalid("Ren’Py 签名行格式无效"));
    }
    let first = STANDARD.decode(parts[1]).map_err(format_error)?;
    let second = parts
        .get(2)
        .map(|s| STANDARD.decode(s).map_err(format_error))
        .transpose()?;
    Ok(Some((parts[0], first, second)))
}
fn verify(key: &VerifyingKey, log: &[u8], signature: &[u8]) -> Result<()> {
    // Ren'Py: SHA-1, raw 64-byte r||s signature, SPKI public / SEC1 private DER.
    let signature = Signature::from_slice(signature).map_err(format_error)?;
    key.verify_prehash(&sha1::Sha1::digest(log), &signature)
        .map_err(|_| invalid("Ren’Py 签名校验失败"))
}
struct LocalKeys {
    signing: Vec<SigningKey>,
    trusted: Vec<VerifyingKey>,
}
fn local_keys(paths: &[PathBuf]) -> Result<LocalKeys> {
    let mut keys = LocalKeys {
        signing: Vec::new(),
        trusted: Vec::new(),
    };
    let mut found = false;
    for path in paths {
        match fs::metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
            Ok(_) => (),
        }
        plain(path)?;
        found = true;
        let text = String::from_utf8(read_limited(path, 256 * 1024)?).map_err(format_error)?;
        for line in text.lines() {
            // Like Ren'Py load_tokens, ignore bad entries without discarding valid keys.
            let Ok(Some((kind, der, _))) = decode_line(line) else {
                continue;
            };
            if kind == "signing-key" {
                if let Ok(secret) = p256::SecretKey::from_sec1_der(&der) {
                    let signing = SigningKey::from(secret);
                    keys.trusted.push(*signing.verifying_key());
                    keys.signing.push(signing);
                }
            } else if kind == "verifying-key" {
                if let Ok(key) = VerifyingKey::from_public_key_der(&der) {
                    keys.trusted.push(key);
                }
            }
        }
    }
    if !found {
        return Err(invalid("未找到目标游戏的本机 Ren’Py security_keys.txt；请先在本机运行目标游戏，不会创建或复制密钥"));
    }
    Ok(keys)
}
fn get_local_signing_key(keys: &LocalKeys) -> Result<&SigningKey> {
    keys.signing
        .iter()
        .find(|key| keys.trusted.contains(key.verifying_key()))
        .ok_or_else(|| {
            invalid("本机 tokens 中缺少有效 signing-key，无法重新签名；不会生成或修改私钥")
        })
}
fn inspect_signature(log: &[u8], signatures: &str, local: &Result<LocalKeys>) -> SignatureInfo {
    let mut valid = false;
    let mut local_valid = false;
    let mut unsupported = false;
    for line in signatures.lines() {
        let Ok(Some(("signature", der, Some(signature)))) = decode_line(line) else {
            continue;
        };
        let Ok(key) = VerifyingKey::from_public_key_der(&der) else {
            unsupported = true;
            continue;
        };
        if verify(&key, log, &signature).is_ok() {
            valid = true;
            local_valid |= local.as_ref().is_ok_and(|keys| keys.trusted.contains(&key));
        }
    }
    let status = if signatures.trim().is_empty() {
        "unsigned"
    } else if local_valid {
        "local"
    } else if valid && local.is_ok() {
        "foreign"
    } else if valid || unsupported {
        "unknown"
    } else {
        "invalid"
    };
    let reason = match local {
        Ok(keys) => get_local_signing_key(keys).err().map(|e| e.to_string()),
        Err(error) => Some(error.to_string()),
    };
    SignatureInfo {
        status: status.into(),
        can_resign: reason.is_none(),
        reason,
    }
}
fn sign_with_local_key(key: &SigningKey, log: &[u8]) -> Result<String> {
    let signature: Signature = key
        .sign_prehash(&sha1::Sha1::digest(log))
        .map_err(format_error)?;
    verify(key.verifying_key(), log, &signature.to_bytes())?;
    let der = key
        .verifying_key()
        .to_public_key_der()
        .map_err(format_error)?;
    Ok(format!(
        "signature {} {}\n",
        STANDARD.encode(der.as_bytes()),
        STANDARD.encode(signature.to_bytes())
    ))
}
pub(super) fn require_source_trust(
    game: &Game,
    data: &[u8],
    persistent: bool,
    confirmed: bool,
) -> Result<()> {
    if !confirmed {
        // Inspect the container/signature without requiring pickle deserialization.
        let container = unpack(data, persistent)?;
        let keys = key_paths(game).and_then(|paths| local_keys(&paths));
        if inspect_signature(&container.log, &container.signatures, &keys).status != "local" {
            return Err(invalid(
                "请先确认此版本的存档来源可信；重新签名后游戏可能不再提示外来存档警告",
            ));
        }
    }
    Ok(())
}
pub(super) fn read(game: &Game, data: &[u8], persistent: bool) -> Result<View> {
    let container = unpack(data, persistent)?;
    let keys = key_paths(game).and_then(|paths| local_keys(&paths));
    let signature = inspect_signature(&container.log, &container.signatures, &keys);
    let mut warnings = Vec::new();
    let mut fields = match pickle::Pickle::parse(&container.log).and_then(|p| p.fields(persistent))
    {
        Ok(fields) => fields,
        Err(error) => {
            warnings.push(format!(
                "变量仅支持受控解析：{error}。仍可独立重新签名；未执行 pickle。"
            ));
            Vec::new()
        }
    };
    if let Some(reason) = &signature.reason {
        for field in &mut fields {
            if field.editable {
                field.readonly(reason);
            }
        }
        warnings.push(reason.clone());
    }
    Ok((
        fields,
        container.metadata,
        container.screenshot,
        warnings,
        signature,
    ))
}
fn pack(data: &[u8], persistent: bool, log: &[u8], signatures: &str) -> Result<Vec<u8>> {
    if persistent {
        let mut compressed = rpg::deflate(log, 3)?;
        compressed.extend_from_slice(signatures.as_bytes());
        return Ok(compressed);
    }
    let mut archive = ZipArchive::new(Cursor::new(data)).map_err(format_error)?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer.set_raw_comment(archive.comment().to_vec().into_boxed_slice());
    let mut has_signatures = false;
    for i in 0..archive.len() {
        let member = archive.by_index(i).map_err(format_error)?;
        has_signatures |= member.name() == "signatures";
        if member.name() == "log" || member.name() == "signatures" {
            let content = if member.name() == "log" {
                log
            } else {
                signatures.as_bytes()
            };
            let mut options = SimpleFileOptions::default().compression_method(member.compression());
            if let Some(time) = member.last_modified() {
                options = options.last_modified_time(time);
            }
            if let Some(mode) = member.unix_mode() {
                options = options.unix_permissions(mode);
            }
            writer
                .start_file(member.name(), options)
                .map_err(format_error)?;
            writer.write_all(content)?;
        } else {
            writer.raw_copy_file(member).map_err(format_error)?;
        }
    }
    if !has_signatures {
        writer
            .start_file("signatures", SimpleFileOptions::default())
            .map_err(format_error)?;
        writer.write_all(signatures.as_bytes())?;
    }
    Ok(writer.finish().map_err(format_error)?.into_inner())
}
pub(super) fn apply(
    game: &Game,
    data: &[u8],
    persistent: bool,
    changes: &[Change],
) -> Result<Vec<u8>> {
    let container = unpack(data, persistent)?;
    let keys = local_keys(&key_paths(game)?)?;
    let key = get_local_signing_key(&keys)?;
    let log = if changes.is_empty() {
        container.log
    } else {
        pickle::Pickle::parse(&container.log)?.patch(&container.log, persistent, changes)?
    };
    let signatures = sign_with_local_key(key, &log)?;
    let output = pack(data, persistent, &log, &signatures)?;
    let check = unpack(&output, persistent)?;
    if check.log != log
        || inspect_signature(&check.log, &check.signatures, &Ok(keys)).status != "local"
    {
        return Err(invalid("Ren’Py 容器或本机签名往返校验失败"));
    }
    Ok(output)
}
#[cfg(test)]
pub(super) fn fixture_signature(key: &SigningKey, log: &[u8]) -> Result<String> {
    sign_with_local_key(key, log)
}
#[cfg(test)]
pub(super) fn fixture_keys(
    paths: &[PathBuf],
    _log: &[u8],
    _signatures: &str,
) -> Result<SigningKey> {
    Ok(get_local_signing_key(&local_keys(paths)?)?.clone())
}
#[cfg(test)]
pub(super) fn fixture_inspect(paths: &[PathBuf], log: &[u8], signatures: &str) -> SignatureInfo {
    inspect_signature(log, signatures, &local_keys(paths))
}
#[cfg(test)]
pub(super) fn fixture_log(data: &[u8], persistent: bool) -> Result<Vec<u8>> {
    Ok(unpack(data, persistent)?.log)
}
#[cfg(test)]
pub(super) fn fixture_signatures(data: &[u8], persistent: bool) -> Result<String> {
    Ok(unpack(data, persistent)?.signatures)
}
