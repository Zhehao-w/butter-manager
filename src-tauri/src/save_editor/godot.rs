//! Godot 4 FileAccess::store_var: little-endian length-prefixed Variants.
use super::{invalid, json_tree, Change, Field, Result, FILE_LIMIT};
use serde_json::Value;
use std::{fs::File, io, io::Read, ops::Range, path::Path};

pub(super) const FORMAT: &str = "Godot 4";
const WIDE: u32 = 1 << 16;
const NIL: u32 = 0;
const BOOL: u32 = 1;
const INT: u32 = 2;
const FLOAT: u32 = 3;
const STRING: u32 = 4;
const STRING_NAME: u32 = 21;
const OBJECT: u32 = 24;
const DICTIONARY: u32 = 27;
const ARRAY: u32 = 28;
const TYPE_NAMES: [&str; 39] = [
    "Nil",
    "bool",
    "int",
    "float",
    "String",
    "Vector2",
    "Vector2i",
    "Rect2",
    "Rect2i",
    "Vector3",
    "Vector3i",
    "Transform2D",
    "Vector4",
    "Vector4i",
    "Plane",
    "Quaternion",
    "AABB",
    "Basis",
    "Transform3D",
    "Projection",
    "Color",
    "StringName",
    "NodePath",
    "RID",
    "Object",
    "Callable",
    "Signal",
    "Dictionary",
    "Array",
    "PackedByteArray",
    "PackedInt32Array",
    "PackedInt64Array",
    "PackedFloat32Array",
    "PackedFloat64Array",
    "PackedStringArray",
    "PackedVector2Array",
    "PackedVector3Array",
    "PackedColorArray",
    "PackedVector4Array",
];

fn word(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.try_into().ok()?))
}

fn variant_type(header: u32) -> Option<u32> {
    let ty = header & 0xff;
    let flags = match ty {
        INT | FLOAT | 5 | 7 | 9 | 11 | 12 | 14..=19 | OBJECT | 35 | 36 | 38 => WIDE,
        DICTIONARY => 15 << 16,
        ARRAY => 3 << 16,
        _ => 0,
    };
    (ty < TYPE_NAMES.len() as u32 && header & !(0xff | flags) == 0).then_some(ty)
}

pub(super) fn probe(path: &Path, size: u64) -> io::Result<bool> {
    if !(8..=FILE_LIMIT as u64).contains(&size) || !size.is_multiple_of(4) {
        return Ok(false);
    }
    let mut file = File::open(path)?;
    let mut prefix = [0; 8];
    if let Err(error) = file.read_exact(&mut prefix) {
        return if error.kind() == io::ErrorKind::UnexpectedEof {
            Ok(false)
        } else {
            Err(error)
        };
    }
    let len = u64::from(word(&prefix[..4]).unwrap());
    if len < 4
        || len > size - 4
        || !len.is_multiple_of(4)
        || word(&prefix[4..]).and_then(variant_type).is_none()
    {
        return Ok(false);
    }
    let mut data = Vec::with_capacity(size as usize);
    data.extend_from_slice(&prefix);
    file.take(FILE_LIMIT as u64 + 1 - prefix.len() as u64)
        .read_to_end(&mut data)?;
    Ok(data.len() == size as usize && records(&data, false).is_some())
}

struct Record<'a> {
    start: usize,
    len: usize,
    root: Node<'a>,
}

struct Node<'a> {
    header: u32,
    span: Range<usize>,
    value: Content<'a>,
}

enum Content<'a> {
    Opaque,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(&'a str),
    Container(Vec<(Option<Node<'a>>, Node<'a>)>),
}

struct Decoder<'a> {
    data: &'a [u8],
    at: usize,
    budget: usize,
}

fn records(mut data: &[u8], collect: bool) -> Option<Vec<Record<'_>>> {
    if data.is_empty() {
        return None;
    }
    let mut result = Vec::new();
    let mut start = 0;
    let mut budget = 100_000;
    while !data.is_empty() {
        let (prefix, tail) = data.split_at_checked(4)?;
        let len = word(prefix)? as usize;
        if len < 4 || !len.is_multiple_of(4) {
            return None;
        }
        let (payload, next) = tail.split_at_checked(len)?;
        let mut decoder = Decoder {
            data: payload,
            at: 0,
            budget,
        };
        let root = decoder.variant(0, collect)?;
        if decoder.at != len {
            return None;
        }
        if collect {
            result.push(Record { start, len, root });
        }
        budget = decoder.budget;
        start += 4 + len;
        data = next;
    }
    Some(result)
}

impl<'a> Decoder<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len)?;
        let bytes = self.data.get(self.at..end)?;
        self.at = end;
        Some(bytes)
    }

    fn word(&mut self) -> Option<u32> {
        word(self.take(4)?)
    }

    fn wide(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<&'a str> {
        let len = self.word()? as usize;
        let bytes = self.take(len.checked_next_multiple_of(4)?)?;
        std::str::from_utf8(&bytes[..len]).ok()
    }

    fn declaration(&mut self, kind: u32) -> Option<u32> {
        match kind {
            0 => Some(NIL),
            1 => self.word().filter(|ty| *ty < TYPE_NAMES.len() as u32),
            _ => {
                self.string()?;
                Some(OBJECT)
            }
        }
    }

    fn typed_variant(&mut self, ty: u32, depth: usize, collect: bool) -> Option<Node<'a>> {
        let node = self.variant(depth, collect)?;
        let actual = node.header & 0xff;
        (ty == NIL || ty == actual || (ty == OBJECT && actual == NIL)).then_some(node)
    }

    fn variant(&mut self, depth: usize, collect: bool) -> Option<Node<'a>> {
        if depth > 64 {
            return None;
        }
        self.budget = self.budget.checked_sub(1)?;
        let start = self.at;
        let header = self.word()?;
        let ty = variant_type(header)?;
        let wide = header & WIDE != 0;
        let real = if wide { 8 } else { 4 };
        let value = match ty {
            BOOL => Content::Bool(match self.word()? {
                0 => false,
                1 => true,
                _ => return None,
            }),
            INT => Content::Int(if wide {
                self.wide()? as i64
            } else {
                i64::from(self.word()? as i32)
            }),
            FLOAT => Content::Float(if wide {
                f64::from_bits(self.wide()?)
            } else {
                f64::from(f32::from_bits(self.word()?))
            }),
            STRING | STRING_NAME => Content::Text(self.string()?),
            DICTIONARY | ARRAY => {
                let first = self.declaration((header >> 16) & 3)?;
                let dictionary = ty == DICTIONARY;
                let second = self.declaration((header >> 18) & 3)?;
                let count = self.word()? & 0x7fff_ffff;
                let mut children = Vec::new();
                for _ in 0..count {
                    let (key, value) = if dictionary {
                        (
                            Some(self.typed_variant(first, depth + 1, false)?),
                            self.typed_variant(second, depth + 1, collect)?,
                        )
                    } else {
                        (None, self.typed_variant(first, depth + 1, collect)?)
                    };
                    if collect {
                        children.push((key, value));
                    }
                }
                Content::Container(children)
            }
            _ => {
                self.skip_opaque(ty, real, wide)?;
                Content::Opaque
            }
        };
        Some(Node {
            header,
            span: start..self.at,
            value,
        })
    }

    fn skip_opaque(&mut self, ty: u32, real: usize, wide: bool) -> Option<()> {
        let len = match ty {
            NIL | 25 => 0, // Callable has no payload.
            5 => 2 * real,
            6 => 8,
            7 | 12 | 14 | 15 => 4 * real,
            8 | 13 | 20 => 16,
            9 => 3 * real,
            10 => 12,
            11 | 16 => 6 * real,
            17 => 9 * real,
            18 => 12 * real,
            19 => 16 * real,
            22 => {
                // NodePath
                let names = self.word()?;
                let subnames = self.word()?;
                let flags = self.word()?;
                if names & 0x8000_0000 == 0 || flags & !3 != 0 {
                    return None;
                }
                self.strings(
                    (names & 0x7fff_ffff)
                        .checked_add(subnames)?
                        .checked_add((flags >> 1) & 1)?,
                )?;
                0
            }
            23 => 8,             // RID
            OBJECT if wide => 8, // Object ID; full objects are outside this codec.
            26 => {
                // Signal
                self.string()?;
                8
            }
            29 => (self.word()? as usize).checked_next_multiple_of(4)?,
            30 | 32 => (self.word()? as usize).checked_mul(4)?,
            31 | 33 => (self.word()? as usize).checked_mul(8)?,
            34 => {
                let count = self.word()?;
                self.strings(count)?;
                0
            }
            35 => (self.word()? as usize).checked_mul(2 * real)?,
            36 => (self.word()? as usize).checked_mul(3 * real)?,
            37 => (self.word()? as usize).checked_mul(16)?,
            38 => (self.word()? as usize).checked_mul(4 * real)?,
            _ => return None,
        };
        self.take(len)?;
        Some(())
    }

    fn strings(&mut self, count: u32) -> Option<()> {
        for _ in 0..count {
            self.string()?;
        }
        Some(())
    }
}

fn decoded(data: &[u8]) -> Result<Vec<Record<'_>>> {
    records(data, true).ok_or_else(|| invalid("无效或不支持的 Godot 4 存档记录"))
}

pub(super) fn fields(data: &[u8]) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    for (index, record) in decoded(data)?.into_iter().enumerate() {
        collect_fields(
            &record.root,
            index.to_string(),
            format!("/{}", index + 1),
            format!("记录 {}", index + 1),
            None,
            &mut fields,
        );
    }
    Ok(fields)
}

fn collect_fields(
    node: &Node<'_>,
    id: String,
    path: String,
    name: String,
    parent: Option<&str>,
    fields: &mut Vec<Field>,
) {
    let value = match node.value {
        Content::Bool(value) => Value::from(value),
        Content::Int(value) => Value::from(value),
        Content::Float(value) if value.is_finite() => Value::from(value),
        Content::Text(value) => Value::from(value),
        _ => Value::Null,
    };
    let mut field = Field::new(path.clone(), name, "variables", value);
    field.id = id.clone();
    field.parent_id = parent.map(str::to_owned);
    field.type_name = Some(TYPE_NAMES[(node.header & 0xff) as usize].into());
    if let Content::Container(children) = &node.value {
        field.kind = "container".into();
        field.reason = None;
        field.child_count = Some(children.len());
        fields.push(field);
        for (index, (key, child)) in children.iter().enumerate() {
            let name = match key.as_ref().map(|key| &key.value) {
                Some(Content::Text(key)) => (*key).into(),
                Some(Content::Int(key)) => format!("[int {key}]"),
                Some(Content::Bool(key)) => format!("[bool {key}]"),
                Some(Content::Float(key)) => format!("[float {key}]"),
                Some(_) => format!("[键 {}]", index + 1),
                None => index.to_string(),
            };
            let child_path = format!("{path}/{}", name.replace('~', "~0").replace('/', "~1"));
            collect_fields(
                child,
                format!("{id}/{index}"),
                child_path,
                name,
                Some(&id),
                fields,
            );
        }
    } else {
        fields.push(field);
    }
}

pub(super) fn apply(data: &[u8], changes: &[Change]) -> Result<Vec<u8>> {
    let records = decoded(data)?;
    let mut deltas = vec![0isize; records.len()];
    let mut patches = Vec::new();
    for change in changes {
        let mut indices = change.id.split('/');
        let index = indices
            .next()
            .and_then(|part| part.parse::<usize>().ok())
            .filter(|&index| index < records.len())
            .ok_or_else(|| invalid("找不到 Godot 4 记录"))?;
        let record = &records[index];
        let node = indices
            .try_fold(&record.root, |node, part| {
                let Content::Container(children) = &node.value else {
                    return None;
                };
                children
                    .get(part.parse::<usize>().ok()?)
                    .map(|(_, child)| child)
            })
            .ok_or_else(|| invalid("找不到 Godot 4 字段"))?;
        let encoded = encode(node.header, &change.value)?;
        let payload = record.start + 4;
        deltas[index] += encoded.len() as isize - node.span.len() as isize;
        patches.push((payload + node.span.start..payload + node.span.end, encoded));
    }
    for (record, delta) in records.iter().zip(deltas) {
        if delta != 0 {
            let len = record
                .len
                .checked_add_signed(delta)
                .and_then(|len| u32::try_from(len).ok())
                .ok_or_else(|| invalid("Godot 4 记录长度无效"))?;
            patches.push((record.start..record.start + 4, len.to_le_bytes().to_vec()));
        }
    }
    json_tree::patch(data, patches)
}

fn encode(header: u32, value: &Value) -> Result<Vec<u8>> {
    let invalid_value = || invalid("修改值与原 Godot 4 类型不兼容");
    let mut bytes = header.to_le_bytes().to_vec();
    match header & 0xff {
        BOOL => bytes.extend_from_slice(
            &u32::from(value.as_bool().ok_or_else(invalid_value)?).to_le_bytes(),
        ),
        INT => {
            let number = value
                .as_f64()
                .filter(|number| number.fract() == 0.0)
                .ok_or_else(invalid_value)? as i64;
            if let Some(narrow) = i32::try_from(number).ok().filter(|_| header & WIDE == 0) {
                bytes.extend_from_slice(&narrow.to_le_bytes());
            } else {
                bytes[..4].copy_from_slice(&(header | WIDE).to_le_bytes());
                bytes.extend_from_slice(&number.to_le_bytes());
            }
        }
        FLOAT => {
            let number = value.as_f64().ok_or_else(invalid_value)?;
            let narrow = number as f32;
            if header & WIDE == 0 && f64::from(narrow) == number {
                bytes.extend_from_slice(&narrow.to_le_bytes());
            } else {
                bytes[..4].copy_from_slice(&(header | WIDE).to_le_bytes());
                bytes.extend_from_slice(&number.to_le_bytes());
            }
        }
        STRING | STRING_NAME => {
            let text = value.as_str().ok_or_else(invalid_value)?;
            bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
            bytes.extend_from_slice(text.as_bytes());
            bytes.resize(bytes.len().next_multiple_of(4), 0);
        }
        _ => return Err(invalid_value()),
    }
    Ok(bytes)
}
