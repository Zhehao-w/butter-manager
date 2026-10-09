use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use p256::ecdsa::{
    signature::hazmat::{PrehashSigner, PrehashVerifier},
    Signature, SigningKey, VerifyingKey,
};
use p256::pkcs8::{DecodePublicKey, EncodePublicKey};
use std::io::Cursor;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

type View = (Vec<Field>, Vec<String>, Option<String>, Vec<String>);
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
        let mut decoder = flate2::read::ZlibDecoder::new(data);
        let mut log = Vec::new();
        (&mut decoder)
            .take(DECODE_LIMIT as u64 + 1)
            .read_to_end(&mut log)
            .map_err(format_error)?;
        if log.len() > DECODE_LIMIT {
            return Err(invalid("persistent 解压数据过大"));
        }
        let consumed = decoder.total_in() as usize;
        let signatures = std::str::from_utf8(
            data.get(consumed..)
                .ok_or_else(|| invalid("persistent 签名位置无效"))?,
        )
        .map_err(format_error)?
        .to_owned();
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
        String::from_utf8(zip_member(&mut archive, "signatures", 256 * 1024)?)
            .map_err(format_error)?
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
fn key_paths(path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        paths.push(PathBuf::from(appdata).join("RenPy/tokens/security_keys.txt"));
    }
    if let Some(root) = std::env::var_os("RENPY_PATH_TO_SAVES") {
        paths.push(PathBuf::from(root).join("tokens/security_keys.txt"));
    }
    if let Some(parent) = path.parent().and_then(Path::parent) {
        paths.push(parent.join("tokens/security_keys.txt"));
    }
    for parent in path.ancestors().skip(1).take(10) {
        let data = parent.join("Ren'Py Data");
        if data.is_dir() {
            paths.push(data.join("tokens/security_keys.txt"));
            break;
        }
    }
    paths.sort();
    paths.dedup();
    paths
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
    // Python ecdsa's default hashfunc is SHA-1; signatures are raw r||s, DER is for keys.
    let signature = Signature::from_slice(signature).map_err(format_error)?;
    key.verify_prehash(&sha1::Sha1::digest(log), &signature)
        .map_err(|_| invalid("Ren’Py 原签名不匹配，禁止写入"))
}
fn signing_key(paths: &[PathBuf], log: &[u8], signatures: &str) -> Result<SigningKey> {
    let mut originals = Vec::new();
    for line in signatures.lines() {
        if let Some(("signature", der, Some(signature))) = decode_line(line)? {
            let key = VerifyingKey::from_public_key_der(&der)
                .map_err(|_| invalid("Ren’Py 签名公钥不是支持的 NIST P-256 DER 格式"))?;
            verify(&key, log, &signature)?;
            originals.push(key);
        }
    }
    if originals.is_empty() {
        return Err(invalid(
            "存档无有效原签名，无法确认游戏信任共享密钥；可查看但不能自动写回",
        ));
    }
    let mut has_key = false;
    let mut has_private = false;
    for path in paths {
        if !path.is_file() {
            continue;
        }
        plain(path)?;
        has_key = true;
        let text = String::from_utf8(read_limited(path, 256 * 1024)?).map_err(format_error)?;
        for line in text.lines() {
            let Some((kind, der, _)) = decode_line(line)? else {
                continue;
            };
            if kind == "verifying-key" {
                continue;
            }
            if kind != "signing-key" {
                continue;
            }
            let secret = p256::SecretKey::from_sec1_der(&der)
                .map_err(|_| invalid("Ren’Py signing-key 无效或不是 NIST P-256 私钥"))?;
            let signing = SigningKey::from(secret);
            has_private = true;
            if originals.iter().any(|key| key == signing.verifying_key()) {
                return Ok(signing);
            }
        }
    }
    Err(invalid(if !has_key {
        "未找到 Ren’Py security_keys.txt；不会创建或复制密钥"
    } else if !has_private {
        "密钥文件只有 verifying-key 或缺少有效 signing-key，无法签名"
    } else {
        "共享私钥与原存档签名不匹配，无法确认游戏信任；禁止写入"
    }))
}
fn sign(key: &SigningKey, log: &[u8]) -> Result<String> {
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
pub(super) fn read(path: &Path, data: &[u8], persistent: bool) -> Result<View> {
    let container = unpack(data, persistent)?;
    let pickle = pickle::Pickle::parse(&container.log)?;
    let mut fields = pickle.fields(persistent)?;
    let mut warnings = Vec::new();
    if let Err(error) = signing_key(&key_paths(path), &container.log, &container.signatures) {
        let reason = error.to_string();
        for field in &mut fields {
            if field.editable {
                field.readonly(&reason);
            }
        }
        warnings.push(reason);
    }
    Ok((fields, container.metadata, container.screenshot, warnings))
}
pub(super) fn apply(
    path: &Path,
    data: &[u8],
    persistent: bool,
    changes: &[Change],
) -> Result<Vec<u8>> {
    let container = unpack(data, persistent)?;
    let key = signing_key(&key_paths(path), &container.log, &container.signatures)?;
    let pickle = pickle::Pickle::parse(&container.log)?;
    let log = pickle.patch(&container.log, persistent, changes)?;
    let signatures = sign(&key, &log)?;
    let output = if persistent {
        let mut compressed = rpg::deflate(&log, 3)?;
        compressed.extend_from_slice(signatures.as_bytes());
        compressed
    } else {
        let mut archive = ZipArchive::new(Cursor::new(data)).map_err(format_error)?;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.set_raw_comment(archive.comment().to_vec().into_boxed_slice());
        for i in 0..archive.len() {
            let member = archive.by_index(i).map_err(format_error)?;
            if member.name() == "log" || member.name() == "signatures" {
                let content = if member.name() == "log" {
                    log.as_slice()
                } else {
                    signatures.as_bytes()
                };
                let mut options =
                    SimpleFileOptions::default().compression_method(member.compression());
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
        writer.finish().map_err(format_error)?.into_inner()
    };
    let check = unpack(&output, persistent)?;
    signing_key(&key_paths(path), &check.log, &check.signatures)?;
    if check.log != log {
        return Err(invalid("Ren’Py 容器往返校验失败"));
    }
    Ok(output)
}

#[cfg(test)]
pub(super) fn fixture_signature(key: &SigningKey, log: &[u8]) -> Result<String> {
    sign(key, log)
}
#[cfg(test)]
pub(super) fn fixture_keys(paths: &[PathBuf], log: &[u8], signatures: &str) -> Result<SigningKey> {
    signing_key(paths, log, signatures)
}
