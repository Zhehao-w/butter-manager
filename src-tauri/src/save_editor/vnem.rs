use super::{format_error, invalid, json_tree, Field, Result};
use serde_json::Value;
use std::{fs::File, io, io::Read, ops::Range, path::Path};

pub(super) const FORMAT: &str = "VNEM/NGAM v1";
const NGAM: usize = 0x80;
const TABLE: usize = 0x88;
const RECORD: usize = 32;
const OFFSET: usize = 16;
const LENGTH: usize = 24;

fn has_header(data: &[u8]) -> bool {
    data.starts_with(b"VNEM") && data.get(NGAM..NGAM + 4) == Some(b"NGAM")
}

pub(super) fn probe(path: &Path) -> io::Result<bool> {
    let mut header = [0; NGAM + 4];
    match File::open(path)?.read_exact(&mut header) {
        Ok(()) => Ok(has_header(&header)),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error),
    }
}

fn block(data: &[u8], record: usize) -> Option<Range<usize>> {
    let offset = u64::from_le_bytes(
        data.get(record + OFFSET..record + LENGTH)?
            .try_into()
            .ok()?,
    );
    let length = u64::from_le_bytes(
        data.get(record + LENGTH..record + RECORD)?
            .try_into()
            .ok()?,
    );
    let start = NGAM.checked_add(usize::try_from(offset).ok()?)?;
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    (end <= data.len()).then_some(start..end)
}

fn progress(data: &[u8]) -> Result<Range<usize>> {
    let bad_table = || invalid("VNEM/NGAM 区块表无效");
    let progress = block(data, TABLE).ok_or_else(bad_table)?;
    // In v1 the first payload starts immediately after the 32-byte records.
    if !has_header(data)
        || progress.start < TABLE + 2 * RECORD
        || !(progress.start - TABLE).is_multiple_of(RECORD)
    {
        return Err(bad_table());
    }
    for record in (TABLE + RECORD..progress.start).step_by(RECORD) {
        if block(data, record).is_none_or(|range| range.start < progress.end) {
            return Err(bad_table());
        }
    }
    Ok(progress)
}

fn agency(root: &json_tree::Node) -> Option<&json_tree::Node> {
    root.child("idol")?.child("agency")
}

pub(super) fn fields(data: &[u8]) -> Result<Vec<Field>> {
    let root = json_tree::Node::parse(&data[progress(data)?])?;
    let mut fields = Vec::new();
    if let Some(agency) = agency(&root) {
        for (key, name) in [("seed", "余额"), ("total_seed", "累计值")] {
            if let Some(node) = agency.child(key).filter(|node| node.value.is_number()) {
                let mut field = Field::new(
                    format!("idol.agency.{key}"),
                    name.into(),
                    "variables",
                    node.value.clone(),
                );
                if key == "total_seed" {
                    field.readonly("累计值仅供查看");
                }
                fields.push(field);
            }
        }
    }
    Ok(fields)
}

pub(super) fn apply(data: &[u8], value: &Value) -> Result<Vec<u8>> {
    let progress = progress(data)?;
    let root = json_tree::Node::parse(&data[progress.clone()])?;
    let seed = agency(&root)
        .and_then(|agency| agency.child("seed"))
        .ok_or_else(|| invalid("找不到 VNEM/NGAM 余额字段"))?;
    let encoded = serde_json::to_vec(value).map_err(format_error)?;
    let old_len = seed.span.len();
    let new_len = encoded.len();
    let span = progress.start + seed.span.start..progress.start + seed.span.end;
    let mut output = json_tree::patch(data, vec![(span, encoded)])?;
    if old_len != new_len {
        let length = progress.len() - old_len + new_len;
        output[TABLE + LENGTH..TABLE + RECORD].copy_from_slice(&(length as u64).to_le_bytes());
        for record in (TABLE + RECORD..progress.start).step_by(RECORD) {
            let start = block(data, record).expect("validated record").start;
            let offset = start - NGAM - old_len + new_len;
            output[record + OFFSET..record + LENGTH]
                .copy_from_slice(&(offset as u64).to_le_bytes());
        }
    }
    Ok(output)
}
