use super::{format_error, invalid, json_tree, Change, Field, Result};
use json_tree::Node;
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
        || progress.start < TABLE + RECORD
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

pub(super) fn fields(data: &[u8]) -> Result<Vec<Field>> {
    let root = Node::parse(&data[progress(data)?])?;
    let mut fields = Vec::new();
    collect_fields(&root, &mut String::new(), &mut fields);
    Ok(fields)
}

fn collect_fields(node: &Node, path: &mut String, fields: &mut Vec<Field>) {
    if !node.value.is_object() && !node.value.is_array() {
        let mut field = Field::new(path.clone(), path.clone(), "variables", node.value.clone());
        if node.value.is_null() {
            field.readonly("空值没有可保留的类型");
        }
        fields.push(field);
    }
    for (key, child) in &node.children {
        let parent_len = path.len();
        path.push('/');
        path.push_str(&key.replace('~', "~0").replace('/', "~1"));
        collect_fields(child, path, fields);
        path.truncate(parent_len);
    }
}

pub(super) fn apply(data: &[u8], changes: &[Change]) -> Result<Vec<u8>> {
    let progress = progress(data)?;
    let root = Node::parse(&data[progress.clone()])?;
    let mut patches = Vec::with_capacity(changes.len());
    for change in changes {
        let node = change
            .id
            .strip_prefix('/')
            .and_then(|path| {
                path.split('/').try_fold(&root, |node, key| {
                    node.child(&key.replace("~1", "/").replace("~0", "~"))
                })
            })
            .ok_or_else(|| invalid("找不到 VNEM/NGAM 字段"))?;
        let span = progress.start + node.span.start..progress.start + node.span.end;
        patches.push((
            span,
            serde_json::to_vec(&change.value).map_err(format_error)?,
        ));
    }
    let mut output = json_tree::patch(data, patches)?;
    if output.len() != data.len() {
        let length = output.len() - (data.len() - progress.len());
        output[TABLE + LENGTH..TABLE + RECORD].copy_from_slice(&(length as u64).to_le_bytes());
        let progress_end = progress.start + length;
        for record in (TABLE + RECORD..progress.start).step_by(RECORD) {
            let start = block(data, record).expect("validated record").start;
            let offset = progress_end - NGAM + (start - progress.end);
            output[record + OFFSET..record + LENGTH]
                .copy_from_slice(&(offset as u64).to_le_bytes());
        }
    }
    Ok(output)
}
