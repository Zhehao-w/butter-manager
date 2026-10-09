use super::*;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use serde_json::json;
use std::collections::BTreeMap;

pub(super) struct Decoded {
    pub value: Value,
    format: String,
    binary: bool,
}
pub(super) fn decode(data: &[u8], format: &str) -> Result<Decoded> {
    let (json, binary) = if format == "MV" {
        let input = std::str::from_utf8(data).map_err(format_error)?;
        (
            String::from_utf16(&lz_decode(input.trim())?)
                .map_err(format_error)?
                .into_bytes(),
            false,
        )
    } else {
        // MZ fsWriteFile writes pako's binary string as UTF-8, not a ZIP archive.
        let converted = std::str::from_utf8(data).ok().and_then(|s| {
            s.chars()
                .map(|c| u8::try_from(c as u32).ok())
                .collect::<Option<Vec<_>>>()
        });
        if let Some(bytes) = converted {
            match inflate(&bytes) {
                Ok(json) => (json, false),
                Err(_) => (inflate(data)?, true),
            }
        } else {
            (inflate(data)?, true)
        }
    };
    let value: Value = serde_json::from_slice(&json).map_err(format_error)?;
    if !value.get("variables").is_some_and(Value::is_object)
        || !value.get("party").is_some_and(Value::is_object)
        || !value.get("switches").is_some_and(Value::is_object)
    {
        return Err(invalid(
            "非标准 RPG Maker 存档，缺少 variables / party / switches；禁止写入",
        ));
    }
    Ok(Decoded {
        value,
        format: format.into(),
        binary,
    })
}
pub(super) fn inflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut json = Vec::new();
    (&mut decoder)
        .take(DECODE_LIMIT as u64 + 1)
        .read_to_end(&mut json)
        .map_err(format_error)?;
    if json.len() > DECODE_LIMIT || decoder.total_in() as usize != data.len() {
        return Err(invalid("zlib 数据过大、不完整或包含异常尾部"));
    }
    Ok(json)
}
pub(super) fn deflate(data: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}
pub(super) fn encode(decoded: &Decoded) -> Result<Vec<u8>> {
    let json = serde_json::to_string(&decoded.value).map_err(format_error)?;
    if json.len() > DECODE_LIMIT {
        return Err(invalid("修改后的 RPG Maker 数据超过 64 MiB"));
    }
    if decoded.format == "MV" {
        return Ok(lz_str::compress_to_base64(json.as_str()).into_bytes());
    }
    let zipped = deflate(json.as_bytes(), 1)?;
    if decoded.binary {
        Ok(zipped)
    } else {
        Ok(zipped
            .into_iter()
            .map(char::from)
            .collect::<String>()
            .into_bytes())
    }
}

// Bounded LZString Base64 decoding. Dictionary entries are prefix links, so repeated
// references cannot allocate quadratic memory as a Vec<String> dictionary would.
fn lz_decode(input: &str) -> Result<Vec<u16>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let values: Vec<u8> = input
        .bytes()
        .take_while(|c| *c != b'=')
        .map(|c| {
            ALPHABET
                .iter()
                .position(|v| *v == c)
                .map(|v| v as u8)
                .ok_or_else(|| invalid("无效的 MV Base64 编码"))
        })
        .collect::<Result<_>>()?;
    let mut position = 0usize;
    let mut bits = |count: u32| -> Result<u32> {
        if count > 24 {
            return Err(invalid("MV 字典过大"));
        }
        let mut value = 0;
        for shift in 0..count {
            let byte = *values
                .get(position / 6)
                .ok_or_else(|| invalid("MV 存档截断"))?;
            value |= u32::from((byte >> (5 - position % 6)) & 1) << shift;
            position += 1;
        }
        Ok(value)
    };
    let mut dict: Vec<(Option<usize>, u16)> = vec![(None, 0); 3];
    let start = bits(2)?;
    let first = match start {
        0 => bits(8)? as u16,
        1 => bits(16)? as u16,
        _ => return Err(invalid("空或无效的 MV 存档")),
    };
    dict.push((None, first));
    let mut previous = 3usize;
    let mut result = vec![first];
    let mut width = 3;
    let mut enlarge = 4u32;
    loop {
        let mut code = bits(width)? as usize;
        if code == 2 {
            return Ok(result);
        }
        if code < 2 {
            let ch = bits(if code == 0 { 8 } else { 16 })? as u16;
            dict.push((None, ch));
            code = dict.len() - 1;
            enlarge -= 1;
        }
        if enlarge == 0 {
            enlarge = 1 << width;
            width += 1;
        }
        let special = code == dict.len();
        if code > dict.len() {
            return Err(invalid("无效的 MV 字典引用"));
        }
        let mut entry = Vec::new();
        let mut index = if special { previous } else { code };
        loop {
            let (parent, ch) = *dict.get(index).ok_or_else(|| invalid("无效的 MV 字典"))?;
            entry.push(ch);
            if entry.len() + result.len() > DECODE_LIMIT / 2 {
                return Err(invalid("MV 解压数据过大"));
            }
            if let Some(parent) = parent {
                index = parent
            } else {
                break;
            }
        }
        entry.reverse();
        let head = entry[0];
        if special {
            entry.push(head);
        }
        if result.len() + entry.len() > DECODE_LIMIT / 2 || dict.len() > 1_000_000 {
            return Err(invalid("MV 解压数据或字典过大"));
        }
        result.extend(entry);
        dict.push((Some(previous), head));
        enlarge -= 1;
        previous = code;
        if enlarge == 0 {
            enlarge = 1 << width;
            width += 1;
        }
    }
}

#[derive(Default)]
pub(super) struct Auxiliary {
    files: BTreeMap<String, Value>,
    pub warnings: Vec<String>,
}
impl Auxiliary {
    pub fn load(game: &Game) -> Self {
        let root = Path::new(&game.install_path);
        let mut roots = vec![root.to_path_buf()];
        if let Some(parent) = game
            .main_executable
            .as_deref()
            .and_then(|s| crate::paths::relative_path(s).ok())
            .and_then(|s| s.parent().map(|s| root.join(s)))
        {
            roots.insert(0, parent);
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.take(64).flatten() {
                if entry
                    .file_type()
                    .is_ok_and(|m| m.is_dir() && !m.is_symlink())
                {
                    roots.push(entry.path());
                }
            }
        }
        let mut auxiliary = Self::default();
        for root in roots {
            for folder in [root.join("data"), root.join("www/data")] {
                if !folder.is_dir() || plain(&folder).is_err() {
                    continue;
                }
                for name in ["System", "Items", "Weapons", "Armors", "Actors", "Classes"] {
                    if auxiliary.files.contains_key(name) {
                        continue;
                    }
                    let path = folder.join(format!("{name}.json"));
                    if !path.is_file() {
                        continue;
                    }
                    let result = plain(&path)
                        .and_then(|_| read_limited(&path, 8 * 1024 * 1024))
                        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(format_error));
                    match result {
                        Ok(json) => {
                            auxiliary.files.insert(name.into(), json);
                        }
                        Err(error) => auxiliary.warnings.push(format!("{name}.json：{error}")),
                    }
                }
            }
        }
        if auxiliary.files.is_empty() {
            auxiliary
                .warnings
                .push("未找到辅助数据库，按字段 ID 显示".into());
        }
        auxiliary
    }
    fn names(&self, kind: &str) -> Option<&Vec<Value>> {
        self.files.get("System")?.get(kind)?.as_array()
    }
    fn records(&self, kind: &str) -> Option<&Vec<Value>> {
        self.files.get(kind)?.as_array()
    }
    fn title(&self, kind: &str, id: usize) -> Option<String> {
        self.records(kind)?
            .get(id)?
            .get("name")?
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }
}
// MV's older JsonEx codec wraps arrays in { "@c": identity, "@a": [...] }.
// Keep the wrapper and its identity in the serialized tree; never flatten it.
fn array_path(value: &Value, path: &str) -> Result<String> {
    let node = value
        .pointer(path)
        .ok_or_else(|| invalid(format!("缺少数组：{path}")))?;
    if node.is_array() {
        return Ok(path.into());
    }
    if node.get("@a").is_some_and(Value::is_array)
        && node.get("@c").is_some_and(Value::is_u64)
        && node.get("@r").is_none()
    {
        return Ok(format!("{path}/@a"));
    }
    Err(invalid(format!(
        "无法识别数组结构：{path}（支持普通数组和 JsonEx @a 数组包装）"
    )))
}
fn referenced_ids(value: &Value) -> HashSet<u64> {
    let mut ids = HashSet::new();
    let mut pending = vec![value];
    while let Some(node) = pending.pop() {
        match node {
            Value::Array(array) => pending.extend(array),
            Value::Object(object) => {
                if let Some(id) = object.get("@r").and_then(Value::as_u64) {
                    ids.insert(id);
                }
                pending.extend(object.values());
            }
            _ => {}
        }
    }
    ids
}
fn shared_container(value: &Value, path: &str, ids: &HashSet<u64>) -> bool {
    let container = path.strip_suffix("/@a").unwrap_or(path);
    [
        Some(container),
        container.rsplit_once('/').map(|(parent, _)| parent),
    ]
    .into_iter()
    .flatten()
    .any(|path| {
        value
            .pointer(path)
            .and_then(|v| v.get("@c"))
            .and_then(Value::as_u64)
            .is_some_and(|id| ids.contains(&id))
    })
}
pub(super) fn fields(value: &Value, auxiliary: &Auxiliary) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    let references = referenced_ids(value);
    for (kind, category, default) in [
        ("variables", "variables", json!(0)),
        ("switches", "switches", json!(false)),
    ] {
        let path = array_path(value, &format!("/{kind}/_data"))?;
        let array = value
            .pointer(&path)
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("无法读取变量或开关数组"))?;
        let names = auxiliary.names(kind);
        let count = array.len().max(names.map_or(0, Vec::len)).min(20_001);
        for id in 1..count {
            let declared = names.and_then(|n| n.get(id)).is_some();
            let current = array.get(id).filter(|v| !v.is_null());
            if !declared && current.is_none() {
                continue;
            }
            let name = names
                .and_then(|n| n.get(id))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(kind);
            let mut field = Field::new(
                format!("{path}/{id}"),
                format!("#{id} {name}"),
                category,
                current.cloned().unwrap_or_else(|| default.clone()),
            );
            if kind == "switches" && !field.value.is_boolean() {
                field.readonly("非布尔开关，不进行类型转换");
            }
            if shared_container(value, &path, &references) {
                field.readonly("JsonEx 数组或容器存在共享引用，当前只读，避免连带修改其他对象");
            }
            fields.push(field);
        }
    }
    if let Some(gold) = value.pointer("/party/_gold") {
        fields.push(Field::new(
            "/party/_gold".into(),
            format!(
                "金钱 {}",
                auxiliary
                    .files
                    .get("System")
                    .and_then(|v| v.get("currencyUnit"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
            ),
            "variables",
            gold.clone(),
        ));
    }
    for (key, db, label) in [
        ("_items", "Items", "道具"),
        ("_weapons", "Weapons", "武器"),
        ("_armors", "Armors", "防具"),
    ] {
        let Some(inventory) = value
            .pointer(&format!("/party/{key}"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let mut ids: std::collections::BTreeSet<usize> =
            inventory.keys().filter_map(|s| s.parse().ok()).collect();
        if let Some(records) = auxiliary.records(db) {
            ids.extend(
                records
                    .iter()
                    .enumerate()
                    .filter(|(id, item)| {
                        *id > 0 && item.get("id").and_then(Value::as_u64) == Some(*id as u64)
                    })
                    .map(|(id, _)| id),
            );
        }
        for id in ids.into_iter().take(20_000) {
            if id == 0 {
                continue;
            }
            let name = auxiliary.title(db, id).unwrap_or_else(|| label.into());
            let mut field = Field::new(
                format!("/party/{key}/{id}"),
                format!("{label} #{id} {name}"),
                "inventory",
                inventory.get(&id.to_string()).cloned().unwrap_or(json!(0)),
            );
            field.description = auxiliary
                .records(db)
                .and_then(|a| a.get(id))
                .and_then(|v| v.get("description"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if !field.value.is_u64() {
                field.readonly("非标准背包数量，只读");
            }
            fields.push(field);
        }
    }
    if let Ok(path) = array_path(value, "/actors/_data") {
        let actors = value
            .pointer(&path)
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("无法读取角色数组"))?;
        for (id, actor) in actors
            .iter()
            .enumerate()
            .take(20_000)
            .filter(|(_, a)| a.is_object())
        {
            let name = actor
                .get("_name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| auxiliary.title("Actors", id))
                .unwrap_or_else(|| format!("角色 #{id}"));
            let mut field = Field::new(
                format!("{path}/{id}"),
                format!("#{id} {name}"),
                "actors",
                Value::String(name),
            );
            field.readonly("角色只读；等级、技能、装备与属性有关联，首版不修改");
            if let Some(class) = actor
                .get("_classId")
                .and_then(Value::as_u64)
                .and_then(|id| auxiliary.title("Classes", id as usize))
            {
                field.description = Some(class);
            }
            fields.push(field);
        }
    }
    if fields.len() > 60_000 {
        return Err(invalid("存档字段数量超过轻量编辑器限制"));
    }
    Ok(fields)
}
pub(super) fn apply(value: &mut Value, changes: &[Change]) -> Result<()> {
    for change in changes {
        if change.id.starts_with("/party/_") {
            if change.id != "/party/_gold"
                && !change.value.as_u64().is_some_and(|n| n <= 999_999_999)
            {
                return Err(invalid("背包数量必须为非负整数且不超过 999999999"));
            }
            let parts: Vec<_> = change.id.split('/').collect();
            if parts.len() == 4 {
                let object = value
                    .pointer_mut(&format!("/party/{}", parts[2]))
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| invalid("背包结构已改变"))?;
                object.insert(parts[3].into(), change.value.clone());
                continue;
            }
        }
        if change.id.starts_with("/variables/_data/") || change.id.starts_with("/switches/_data/") {
            let (parent, index) = change
                .id
                .rsplit_once('/')
                .ok_or_else(|| invalid("无效字段路径"))?;
            let index: usize = index.parse().map_err(format_error)?;
            if index == 0 || index > 20_000 {
                return Err(invalid("变量 ID 超出范围"));
            }
            let array = value
                .pointer_mut(parent)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| invalid("变量结构已改变"))?;
            if array.len() <= index {
                array.resize(index + 1, Value::Null);
            }
            array[index] = change.value.clone();
        } else if change.id == "/party/_gold" {
            value["party"]["_gold"] = change.value.clone();
        } else {
            return Err(invalid("不允许修改此数据路径"));
        }
    }
    Ok(())
}
pub(super) fn metadata(value: &Value) -> Vec<String> {
    let mut metadata = Vec::new();
    for (path, label) in [
        ("/system/_playtime", "游玩时间"),
        ("/map/_mapId", "地图 ID"),
    ] {
        if let Some(value) = value.pointer(path) {
            if value.is_number() || value.is_string() {
                metadata.push(format!("{label}：{value}"));
            }
        }
    }
    metadata
}
