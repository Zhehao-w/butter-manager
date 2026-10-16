//! File-based Unity formats. No SDK, game assemblies, registry or arbitrary JSON editing.
use super::{
    json_tree::{self, Node},
    *,
};
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use flate2::{
    bufread::GzDecoder,
    write::{DeflateEncoder, GzEncoder},
    Compression,
};
use rand_core::{OsRng, RngCore};
use std::ops::Range;
use zeroize::Zeroizing;

pub(super) const ES3: &str = "ES3";
pub(super) const NANI: &str = "Naninovel";
pub(super) fn supports(format: &str) -> bool {
    matches!(format, ES3 | NANI)
}
pub(super) fn recognize_json(data: &[u8]) -> bool {
    Node::parse(data)
        .ok()
        .and_then(|root| {
            root.child("objectJsonMap")
                .and_then(|m| m.child("keys"))
                .map(|keys| {
                    keys.children.iter().any(|(_, k)| {
                        k.value
                            .as_str()
                            .is_some_and(|s| s.starts_with("Naninovel.CustomVariableManager+"))
                    })
                })
        })
        .unwrap_or(false)
}

#[derive(Clone, Copy)]
enum Wrapper {
    Plain,
    Gzip,
    Deflate,
}
pub(super) struct Decoded {
    bytes: Vec<u8>,
    root: Node,
    wrapper: Wrapper,
    encrypted: bool,
}
pub(super) fn locked(data: &[u8], format: &str) -> bool {
    if format != ES3 || Node::parse(data).is_ok() {
        return false;
    }
    // IVs are random bytes, so a leading '{' or GZip magic is not an encoding decision.
    !(data.starts_with(&[0x1f, 0x8b]) && inflate(data, true).is_ok())
}
pub(super) fn decode(data: &[u8], format: &str, password: Option<&str>) -> Result<Decoded> {
    let encrypted = locked(data, format);
    let decrypted;
    let payload = if encrypted {
        let password =
            password.ok_or_else(|| invalid("此 ES3 文件需要已知密码，或使用了未支持的封装"))?;
        if password.len() > 1024 {
            return Err(invalid("ES3 密码过长"));
        }
        if data.len() < 32 || !(data.len() - 16).is_multiple_of(16) {
            return Err(invalid("不是标准 ES3 AES 容器"));
        }
        let key = key(password, &data[..16]);
        decrypted = Zeroizing::new(
            cbc::Decryptor::<aes::Aes128>::new_from_slices(&*key, &data[..16])
                .map_err(format_error)?
                .decrypt_padded_vec_mut::<Pkcs7>(&data[16..])
                .map_err(|_| invalid("ES3 密码错误、文件损坏或封装不支持"))?,
        );
        decrypted.as_slice()
    } else {
        data
    };
    let (bytes, wrapper) = if payload.starts_with(&[0x1f, 0x8b]) && format == ES3 {
        (inflate(payload, true)?, Wrapper::Gzip)
    } else if format == NANI && Node::parse(payload).is_err() {
        (inflate(payload, false)?, Wrapper::Deflate)
    } else {
        (payload.to_vec(), Wrapper::Plain)
    };
    if bytes.len() > DECODE_LIMIT {
        return Err(invalid("解码数据超过 64 MiB"));
    }
    let root = Node::parse(&bytes)
        .map_err(|_| invalid("无法解析此存档：密码错误、JSON 损坏或封装不支持"))?;
    if !root.value.is_object() {
        return Err(invalid("存档顶层必须是对象"));
    }
    let decoded = Decoded {
        bytes,
        root,
        wrapper,
        encrypted,
    };
    // Structure checks apply on both read and write; extension alone is never sufficient.
    decoded.targets(format)?;
    Ok(decoded)
}
fn key(password: &str, iv: &[u8]) -> Zeroizing<[u8; 16]> {
    let mut key = Zeroizing::new([0; 16]);
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password.as_bytes(), iv, 100, &mut *key);
    key
}
fn inflate(data: &[u8], gzip: bool) -> Result<Vec<u8>> {
    let mut input = data;
    let mut output = Vec::new();
    if gzip {
        let mut decoder = GzDecoder::new(&mut input);
        (&mut decoder)
            .take(DECODE_LIMIT as u64 + 1)
            .read_to_end(&mut output)
            .map_err(format_error)?;
    } else {
        let mut decoder = flate2::Decompress::new(false);
        loop {
            let mut chunk = [0; 8192];
            let before_in = decoder.total_in();
            let before_out = decoder.total_out();
            let status = decoder
                .decompress(
                    &data[before_in as usize..],
                    &mut chunk,
                    flate2::FlushDecompress::None,
                )
                .map_err(format_error)?;
            output.extend_from_slice(&chunk[..(decoder.total_out() - before_out) as usize]);
            if output.len() > DECODE_LIMIT {
                return Err(invalid("解压数据超过 64 MiB"));
            }
            if status == flate2::Status::StreamEnd {
                input = &data[decoder.total_in() as usize..];
                break;
            }
            if decoder.total_in() == before_in && decoder.total_out() == before_out {
                return Err(invalid("DEFLATE 流不完整"));
            }
        }
    }
    if output.len() > DECODE_LIMIT || !input.is_empty() {
        return Err(invalid("压缩流过大、不完整或包含异常尾部"));
    }
    Ok(output)
}

#[derive(Clone, Copy)]
enum Kind {
    Bool,
    Text,
    Signed(u32),
    Unsigned(u32),
    Float(bool),
    Unknown,
}
impl Kind {
    fn es3(name: &str) -> Self {
        match name.split(',').next().unwrap_or("").trim() {
            "bool" | "System.Boolean" => Self::Bool,
            "string" | "System.String" => Self::Text,
            "sbyte" | "System.SByte" => Self::Signed(8),
            "short" | "System.Int16" => Self::Signed(16),
            "int" | "System.Int32" => Self::Signed(32),
            "long" | "System.Int64" => Self::Signed(64),
            "byte" | "System.Byte" => Self::Unsigned(8),
            "ushort" | "System.UInt16" => Self::Unsigned(16),
            "uint" | "System.UInt32" => Self::Unsigned(32),
            "ulong" | "System.UInt64" => Self::Unsigned(64),
            "float" | "System.Single" => Self::Float(true),
            "double" | "System.Double" => Self::Float(false),
            _ => Self::Unknown,
        }
    }
    fn encode(self, value: &Value) -> Result<Vec<u8>> {
        let valid = match self {
            Self::Bool => value.is_boolean(),
            Self::Text => value.as_str().is_some_and(|s| s.len() <= 1024 * 1024),
            Self::Signed(bits) => value.as_f64().is_some_and(|n| {
                n.is_finite()
                    && n.fract() == 0.0
                    && n.abs() <= 9_007_199_254_740_991.0
                    && n >= -(2f64).powi(bits as i32 - 1)
                    && n < (2f64).powi(bits as i32 - 1)
            }),
            Self::Unsigned(bits) => value.as_f64().is_some_and(|n| {
                n.is_finite()
                    && n.fract() == 0.0
                    && (0.0..=9_007_199_254_740_991.0).contains(&n)
                    && n < (2f64).powi(bits as i32)
            }),
            Self::Float(single) => value.as_f64().is_some_and(|n| {
                n.is_finite()
                    && n.abs() <= 9_007_199_254_740_991.0
                    && (!single || (n as f32).is_finite())
            }),
            Self::Unknown => false,
        };
        if !valid {
            return Err(invalid("值不符合存档声明的类型或数值范围"));
        }
        let normalized = match self {
            Self::Signed(_) => Value::from(value.as_f64().unwrap() as i64),
            Self::Unsigned(_) => Value::from(value.as_f64().unwrap() as u64),
            _ => value.clone(),
        };
        serde_json::to_vec(&normalized).map_err(format_error)
    }
}
struct Target {
    field: Field,
    span: Range<usize>,
    kind: Kind,
    // A service is itself a JSON-encoded string. Patch inside it, then replace its outer token.
    embedded: Option<usize>,
}
struct Service {
    span: Range<usize>,
    bytes: Vec<u8>,
    root: Node,
}
struct Targets {
    fields: Vec<Target>,
    services: Vec<Service>,
}
fn target(
    id: String,
    name: String,
    node: &Node,
    kind: Kind,
    type_name: &str,
    embedded: Option<usize>,
) -> Target {
    let mut field = Field::new(id, name, "variables", node.value.clone());
    if node.value.is_object() || node.value.is_array() {
        field.value = Value::Null;
    }
    field.type_name = Some(type_name.into());
    if kind.encode(&node.value).is_err() {
        field.readonly("类型未支持、结构复杂或值超出安全范围");
    }
    Target {
        field,
        span: node.span.clone(),
        kind,
        embedded,
    }
}
impl Decoded {
    fn targets(&self, format: &str) -> Result<Targets> {
        let mut targets = Targets {
            fields: Vec::new(),
            services: Vec::new(),
        };
        if format == ES3 {
            if self.root.children.is_empty() {
                return Err(invalid("未发现 ES3 类型条目"));
            }
            for (name, entry) in &self.root.children {
                let type_name = entry
                    .child("__type")
                    .and_then(|n| n.value.as_str())
                    .ok_or_else(|| invalid("不是标准 ES3 typed-entry 存档"))?;
                let value = entry
                    .child("value")
                    .ok_or_else(|| invalid("ES3 条目缺少 value"))?;
                targets.fields.push(target(
                    format!("es3/{name}"),
                    name.clone(),
                    value,
                    Kind::es3(type_name),
                    type_name,
                    None,
                ));
            }
        } else {
            let map = self
                .root
                .child("objectJsonMap")
                .ok_or_else(|| invalid("不是标准 Naninovel 状态：缺少 objectJsonMap"))?;
            let keys = map
                .child("keys")
                .filter(|n| n.value.is_array())
                .ok_or_else(|| invalid("Naninovel 服务键结构无效"))?;
            let values = map
                .child("values")
                .filter(|n| n.value.is_array())
                .ok_or_else(|| invalid("Naninovel 服务值结构无效"))?;
            if keys.children.len() != values.children.len() {
                return Err(invalid("Naninovel 服务键值数量不一致"));
            }
            let mut seen = HashSet::new();
            for ((_, key), (_, value)) in keys.children.iter().zip(&values.children) {
                let key = key
                    .value
                    .as_str()
                    .ok_or_else(|| invalid("服务键不是字符串"))?;
                if !seen.insert(key) {
                    return Err(invalid("Naninovel 服务键重复"));
                }
                let class = key.split(',').next().unwrap_or("");
                if !matches!(
                    class,
                    "Naninovel.CustomVariableManager+GameState"
                        | "Naninovel.CustomVariableManager+GlobalState"
                ) {
                    continue;
                }
                let bytes = value
                    .value
                    .as_str()
                    .ok_or_else(|| invalid("变量服务状态不是 JSON 字符串"))?
                    .as_bytes()
                    .to_vec();
                let root = Node::parse(&bytes)?;
                let service = Service {
                    span: value.span.clone(),
                    bytes,
                    root,
                };
                let index = targets.services.len();
                nani_fields(
                    &service.root,
                    class.ends_with("+GlobalState"),
                    index,
                    &mut targets.fields,
                )?;
                targets.services.push(service);
            }
            if targets.services.is_empty() {
                return Err(invalid("未发现支持的 Naninovel 自定义变量服务"));
            }
        }
        if targets.fields.len() > 20_000 {
            return Err(invalid("变量超过 20000 项"));
        }
        let mut ids = HashSet::new();
        if targets.fields.iter().any(|t| !ids.insert(&t.field.id)) {
            return Err(invalid("变量标识重复，禁止编辑"));
        }
        Ok(targets)
    }
    pub fn fields(&self, format: &str) -> Result<Vec<Field>> {
        Ok(self
            .targets(format)?
            .fields
            .into_iter()
            .map(|t| t.field)
            .collect())
    }
    pub fn metadata(&self, format: &str) -> Vec<String> {
        let mut metadata = vec![
            format!(
                "{}{}",
                match self.wrapper {
                    Wrapper::Plain => "明文 JSON",
                    Wrapper::Gzip => "GZip",
                    Wrapper::Deflate => "raw DEFLATE",
                },
                if self.encrypted {
                    " · AES 已解锁"
                } else {
                    ""
                }
            ),
            if format == NANI {
                "仅编辑标准自定义变量；其他服务与回滚状态保留".into()
            } else {
                "保留原始类型与未修改条目".into()
            },
        ];
        if format == NANI
            && self
                .root
                .child("objectJsonMap")
                .and_then(|m| m.child("keys"))
                .is_some_and(|keys| {
                    keys.children.iter().any(|(_, key)| {
                        key.value.as_str().is_some_and(|s| {
                            s.split(',').next()
                                == Some("Naninovel.CustomVariableManager+GlobalState")
                        })
                    })
                })
        {
            metadata.push("全局变量：可能影响所有槽位；保存后可能需要重启游戏".into());
        }
        metadata
    }
    pub fn apply(
        &self,
        original: &[u8],
        format: &str,
        changes: &[Change],
        password: Option<&str>,
    ) -> Result<Vec<u8>> {
        let targets = self.targets(format)?;
        let fields: Vec<_> = targets.fields.iter().map(|t| t.field.clone()).collect();
        validate_changes(&fields, changes)?;
        let mut patches = Vec::new();
        let mut embedded: std::collections::BTreeMap<usize, Vec<_>> = Default::default();
        for change in changes {
            let t = targets
                .fields
                .iter()
                .find(|t| t.field.id == change.id)
                .ok_or_else(|| invalid("未知字段"))?;
            if same_field_value(&t.field, &change.value) {
                continue;
            }
            let patch = (t.span.clone(), t.kind.encode(&change.value)?);
            if let Some(index) = t.embedded {
                embedded.entry(index).or_default().push(patch);
            } else {
                patches.push(patch);
            }
        }
        if patches.is_empty() && embedded.is_empty() {
            return Ok(original.to_vec());
        }
        for (index, inner) in embedded {
            let service = &targets.services[index];
            let bytes = json_tree::patch(&service.bytes, inner)?;
            Node::parse(&bytes)?;
            patches.push((
                service.span.clone(),
                serde_json::to_vec(std::str::from_utf8(&bytes).map_err(format_error)?)
                    .map_err(format_error)?,
            ));
        }
        let bytes = json_tree::patch(&self.bytes, patches)?;
        let packed = match self.wrapper {
            Wrapper::Plain => bytes.clone(),
            Wrapper::Gzip => {
                let mut e = GzEncoder::new(Vec::new(), Compression::default());
                e.write_all(&bytes)?;
                e.finish()?
            }
            Wrapper::Deflate => {
                let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
                e.write_all(&bytes)?;
                e.finish()?
            }
        };
        let output = if self.encrypted {
            let mut iv = [0; 16];
            OsRng
                .try_fill_bytes(&mut iv)
                .map_err(|_| invalid("无法生成 ES3 加密随机数"))?;
            let key = key(password.ok_or_else(|| invalid("ES3 密码未提供"))?, &iv);
            let mut out = iv.to_vec();
            out.extend(
                cbc::Encryptor::<aes::Aes128>::new_from_slices(&*key, &iv)
                    .map_err(format_error)?
                    .encrypt_padded_vec_mut::<Pkcs7>(&packed),
            );
            out
        } else {
            packed
        };
        let verified = decode(&output, format, password)?;
        if verified.bytes != bytes {
            return Err(invalid("Unity 存档封装往返校验失败"));
        }
        Ok(output)
    }
}
fn nani_fields(root: &Node, global: bool, service: usize, fields: &mut Vec<Target>) -> Result<()> {
    let list_name = if global {
        "GlobalVariables"
    } else {
        "LocalVariables"
    };
    if let Some(list) = root.child(list_name) {
        if !list.value.is_array() {
            return Err(invalid("Naninovel 变量列表结构无效"));
        }
        let mut names = HashSet::new();
        for (_, entry) in &list.children {
            let name = entry
                .child("name")
                .and_then(|n| n.value.as_str())
                .ok_or_else(|| invalid("变量缺少名称"))?;
            if !names.insert(name) {
                return Err(invalid("Naninovel 变量名称重复"));
            }
            let value = entry
                .child("value")
                .ok_or_else(|| invalid("变量缺少 value"))?;
            let (member, kind, type_name) = match value.child("type").and_then(|n| n.value.as_i64())
            {
                Some(0) => ("stringValue", Kind::Text, "String"),
                Some(1) => ("numericValue", Kind::Float(true), "Numeric (Single)"),
                Some(2) => ("booleanValue", Kind::Bool, "Boolean"),
                _ => {
                    fields.push(target(
                        format!("nani/{service}/{name}"),
                        name.into(),
                        value,
                        Kind::Unknown,
                        "未知类型",
                        Some(service),
                    ));
                    continue;
                }
            };
            let node = value
                .child(member)
                .ok_or_else(|| invalid("变量声明类型与数据不一致"))?;
            fields.push(target(
                format!("nani/{service}/{name}"),
                name.into(),
                node,
                kind,
                type_name,
                Some(service),
            ));
        }
        return Ok(());
    }
    // Older Naninovel string-map schema: preserve strings; never guess numbers or booleans.
    let map = root
        .child(if global {
            "GlobalVariableMap"
        } else {
            "LocalVariableMap"
        })
        .ok_or_else(|| invalid("此 Naninovel 变量服务版本尚未支持"))?;
    let keys = map
        .child("keys")
        .filter(|n| n.value.is_array())
        .ok_or_else(|| invalid("变量键结构无效"))?;
    let values = map
        .child("values")
        .filter(|n| n.value.is_array())
        .ok_or_else(|| invalid("变量值结构无效"))?;
    if keys.children.len() != values.children.len() {
        return Err(invalid("变量键值数量不一致"));
    }
    for ((_, key), (_, value)) in keys.children.iter().zip(&values.children) {
        let name = key
            .value
            .as_str()
            .ok_or_else(|| invalid("变量名不是字符串"))?;
        fields.push(target(
            format!("nani/{service}/{name}"),
            name.into(),
            value,
            Kind::Text,
            "String",
            Some(service),
        ));
    }
    Ok(())
}
