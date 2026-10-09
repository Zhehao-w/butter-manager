//! A symbolic pickle stack machine. GLOBAL/REDUCE/BUILD never execute code.
//! Patches replace a parsed scalar's defining opcode, leaving every other opcode intact.
use super::*;
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug)]
enum Node {
    Scalar(Value),
    Sequence(Vec<usize>),
    Dict(Vec<(usize, usize)>),
    Object {
        dependencies: Vec<usize>,
        state: Option<usize>,
    },
    Opaque,
}
struct Entry {
    node: Node,
    span: std::ops::Range<usize>,
    dictionary_checkpoint: usize,
}
pub(super) struct Pickle {
    entries: Vec<Entry>,
    root: usize,
    protocol: u8,
    frames: Vec<(usize, usize)>,
    references: Vec<usize>,
}
fn pop(stack: &mut Vec<Option<usize>>) -> Result<usize> {
    stack
        .pop()
        .flatten()
        .ok_or_else(|| invalid("pickle 栈无效"))
}
fn marked(stack: &mut Vec<Option<usize>>) -> Result<Vec<usize>> {
    let index = stack
        .iter()
        .rposition(Option::is_none)
        .ok_or_else(|| invalid("pickle 缺少 MARK"))?;
    let items = stack
        .split_off(index + 1)
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("pickle 嵌套 MARK 无效"))?;
    stack.pop();
    Ok(items)
}
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    frame_end: Option<usize>,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|n| *n <= self.bytes.len())
            .ok_or_else(|| invalid("pickle 被截断"))?;
        if self.frame_end.is_some_and(|boundary| end > boundary) {
            return Err(invalid("pickle opcode 跨越 FRAME"));
        }
        let bytes = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn length(&mut self, count: usize) -> Result<usize> {
        let bytes = self.take(count)?;
        let mut size = 0u64;
        for (shift, byte) in bytes.iter().enumerate() {
            size |= u64::from(*byte) << (shift * 8);
        }
        usize::try_from(size).map_err(format_error)
    }
    fn line(&mut self) -> Result<&'a [u8]> {
        let size = self.bytes[self.pos..]
            .iter()
            .position(|c| *c == b'\n')
            .ok_or_else(|| invalid("pickle 文本行被截断"))?;
        let text = self.take(size)?;
        self.byte()?;
        Ok(text)
    }
    fn text(&mut self, len: usize) -> Result<String> {
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(format_error)
    }
}
fn raw_unicode(bytes: &[u8]) -> Result<String> {
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && b"uU".contains(&bytes[i + 1]) {
            let size = if bytes[i + 1] == b'u' { 4 } else { 8 };
            let hex = bytes
                .get(i + 2..i + 2 + size)
                .ok_or_else(|| invalid("pickle Unicode 转义截断"))?;
            let point = u32::from_str_radix(std::str::from_utf8(hex).map_err(format_error)?, 16)
                .map_err(format_error)?;
            result.push(char::from_u32(point).ok_or_else(|| invalid("不支持孤立 Unicode 代理项"))?);
            i += size + 2;
        } else {
            result.push(char::from(bytes[i]));
            i += 1;
        }
    }
    Ok(result)
}
impl Pickle {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > DECODE_LIMIT {
            return Err(invalid("pickle 过大"));
        }
        let mut cursor = Cursor {
            bytes,
            pos: 0,
            frame_end: None,
        };
        let mut entries: Vec<Entry> = Vec::new();
        let mut stack: Vec<Option<usize>> = Vec::new();
        let mut memo = HashMap::<usize, usize>::new();
        let mut frames = Vec::new();
        let mut protocol = 0;
        let root = loop {
            if cursor.frame_end == Some(cursor.pos) {
                cursor.frame_end = None;
            }
            if stack.len() > 500_000 {
                return Err(invalid("pickle 栈过大"));
            }
            let start = cursor.pos;
            let op = cursor.byte()?;
            if frames
                .last()
                .is_some_and(|(s, e)| start > *s && start < *e && cursor.pos > *e)
            {
                return Err(invalid("pickle FRAME 跨界"));
            }
            let node = match op {
                0x80 => {
                    protocol = cursor.byte()?;
                    if protocol > 5 {
                        return Err(invalid("不支持此 pickle 协议"));
                    }
                    continue;
                }
                0x95 => {
                    let length = cursor.length(8)?;
                    let end = cursor
                        .pos
                        .checked_add(length)
                        .filter(|e| *e <= bytes.len())
                        .ok_or_else(|| invalid("pickle FRAME 截断"))?;
                    if frames.last().is_some_and(|(_, e)| start < *e) {
                        return Err(invalid("pickle FRAME 嵌套"));
                    }
                    frames.push((start, end));
                    cursor.frame_end = Some(end);
                    continue;
                }
                b'.' => {
                    let root = pop(&mut stack)?;
                    if !stack.is_empty() || cursor.pos != bytes.len() {
                        return Err(invalid("pickle STOP 后存在额外数据或栈状态"));
                    }
                    break root;
                }
                b'(' => {
                    stack.push(None);
                    continue;
                }
                b'0' => {
                    stack.pop().ok_or_else(|| invalid("pickle POP 空栈"))?;
                    continue;
                }
                b'1' => {
                    marked(&mut stack)?;
                    continue;
                }
                b'2' => {
                    let id = *stack.last().ok_or_else(|| invalid("pickle DUP 空栈"))?;
                    stack.push(id);
                    continue;
                }
                b'p' | b'q' | b'r' | 0x94 => {
                    let index = match op {
                        b'p' => std::str::from_utf8(cursor.line()?)
                            .map_err(format_error)?
                            .parse()
                            .map_err(format_error)?,
                        b'q' => cursor.length(1)?,
                        b'r' => cursor.length(4)?,
                        _ => memo.len(),
                    };
                    if index > 1_000_000 {
                        return Err(invalid("pickle memo 过大"));
                    }
                    let id = stack
                        .last()
                        .copied()
                        .flatten()
                        .ok_or_else(|| invalid("pickle memo 空栈"))?;
                    if memo.insert(index, id).is_some() {
                        return Err(invalid("pickle memo 被重复定义，无法可靠修改"));
                    }
                    continue;
                }
                b'g' | b'h' | b'j' => {
                    let index = match op {
                        b'g' => std::str::from_utf8(cursor.line()?)
                            .map_err(format_error)?
                            .parse()
                            .map_err(format_error)?,
                        b'h' => cursor.length(1)?,
                        _ => cursor.length(4)?,
                    };
                    let id = *memo
                        .get(&index)
                        .ok_or_else(|| invalid("pickle memo 引用不存在"))?;
                    stack.push(Some(id));
                    continue;
                }
                b'N' => Node::Scalar(Value::Null),
                0x88 => Node::Scalar(json!(true)),
                0x89 => Node::Scalar(json!(false)),
                b'I' => {
                    let s = std::str::from_utf8(cursor.line()?).map_err(format_error)?;
                    Node::Scalar(if s == "01" {
                        json!(true)
                    } else if s == "00" {
                        json!(false)
                    } else {
                        s.parse::<i64>().ok().map_or(Value::Null, |v| json!(v))
                    })
                }
                b'L' => {
                    let s = std::str::from_utf8(cursor.line()?)
                        .map_err(format_error)?
                        .trim_end_matches('L');
                    Node::Scalar(s.parse::<i64>().ok().map_or(Value::Null, |v| json!(v)))
                }
                b'J' => {
                    let b: [u8; 4] = cursor.take(4)?.try_into().map_err(format_error)?;
                    Node::Scalar(json!(i32::from_le_bytes(b)))
                }
                b'K' => Node::Scalar(json!(cursor.byte()?)),
                b'M' => Node::Scalar(json!(cursor.length(2)?)),
                0x8a | 0x8b => {
                    let size = cursor.length(if op == 0x8a { 1 } else { 4 })?;
                    let data = cursor.take(size)?;
                    if size > 8 {
                        Node::Opaque
                    } else {
                        let negative = data.last().is_some_and(|v| v & 128 != 0);
                        let mut number = if negative { [255; 8] } else { [0; 8] };
                        number[..size].copy_from_slice(data);
                        Node::Scalar(json!(i64::from_le_bytes(number)))
                    }
                }
                b'F' => {
                    let n: f64 = std::str::from_utf8(cursor.line()?)
                        .map_err(format_error)?
                        .parse()
                        .map_err(format_error)?;
                    Node::Scalar(json!(n))
                }
                b'G' => {
                    let b: [u8; 8] = cursor.take(8)?.try_into().map_err(format_error)?;
                    Node::Scalar(json!(f64::from_be_bytes(b)))
                }
                b'V' => Node::Scalar(json!(raw_unicode(cursor.line()?)?)),
                b'X' | 0x8c | 0x8d => {
                    let len = cursor.length(match op {
                        b'X' => 4,
                        0x8c => 1,
                        _ => 8,
                    })?;
                    Node::Scalar(json!(cursor.text(len)?))
                }
                b'S' => {
                    cursor.line()?;
                    Node::Opaque
                }
                b'T' | b'U' | b'B' | b'C' | 0x8e | 0x96 => {
                    let len = cursor.length(match op {
                        b'U' | b'C' => 1,
                        b'T' | b'B' => 4,
                        _ => 8,
                    })?;
                    cursor.take(len)?;
                    Node::Opaque
                }
                b']' | b')' | 0x8f => Node::Sequence(Vec::new()),
                b'}' => Node::Dict(Vec::new()),
                b'l' | b't' | 0x91 => Node::Sequence(marked(&mut stack)?),
                0x85..=0x87 => {
                    let count = usize::from(op - 0x84);
                    let mut ids = Vec::new();
                    for _ in 0..count {
                        ids.push(pop(&mut stack)?);
                    }
                    ids.reverse();
                    Node::Sequence(ids)
                }
                b'd' => {
                    let ids = marked(&mut stack)?;
                    if ids.len() % 2 != 0 {
                        return Err(invalid("pickle 字典长度无效"));
                    }
                    Node::Dict(
                        ids.as_chunks::<2>()
                            .0
                            .iter()
                            .map(|pair| (pair[0], pair[1]))
                            .collect(),
                    )
                }
                b'a' | b'e' | 0x90 => {
                    let ids = if op == b'a' {
                        vec![pop(&mut stack)?]
                    } else {
                        marked(&mut stack)?
                    };
                    let target = stack
                        .last()
                        .copied()
                        .flatten()
                        .ok_or_else(|| invalid("pickle 缺少列表目标"))?;
                    match &mut entries[target].node {
                        Node::Sequence(items) => items.extend(ids),
                        Node::Object { dependencies, .. } => dependencies.extend(ids),
                        _ => return Err(invalid("非标准 pickle APPENDS 目标")),
                    }
                    continue;
                }
                b's' | b'u' => {
                    let ids = if op == b's' {
                        let value = pop(&mut stack)?;
                        vec![pop(&mut stack)?, value]
                    } else {
                        marked(&mut stack)?
                    };
                    if ids.len() % 2 != 0 {
                        return Err(invalid("pickle SETITEMS 长度无效"));
                    }
                    let target = stack
                        .last()
                        .copied()
                        .flatten()
                        .ok_or_else(|| invalid("pickle 缺少字典目标"))?;
                    match &mut entries[target].node {
                        Node::Dict(items) => {
                            items.extend(
                                ids.as_chunks::<2>().0.iter().map(|pair| (pair[0], pair[1])),
                            );
                            entries[target].dictionary_checkpoint = cursor.pos;
                        }
                        Node::Object { dependencies, .. } => dependencies.extend(ids),
                        _ => return Err(invalid("非标准 pickle SETITEMS 目标")),
                    }
                    continue;
                }
                b'c' => {
                    cursor.line()?;
                    cursor.line()?;
                    Node::Opaque
                }
                0x93 => {
                    let name = pop(&mut stack)?;
                    let module = pop(&mut stack)?;
                    Node::Object {
                        dependencies: vec![module, name],
                        state: None,
                    }
                }
                b'R' | 0x81 | 0x92 => {
                    let kwargs = if op == 0x92 {
                        Some(pop(&mut stack)?)
                    } else {
                        None
                    };
                    let args = pop(&mut stack)?;
                    let callable = pop(&mut stack)?;
                    let mut dependencies = vec![callable, args];
                    dependencies.extend(kwargs);
                    Node::Object {
                        dependencies,
                        state: None,
                    }
                }
                b'b' => {
                    let state = pop(&mut stack)?;
                    let object = stack
                        .last()
                        .copied()
                        .flatten()
                        .ok_or_else(|| invalid("pickle BUILD 缺少对象"))?;
                    match &mut entries[object].node {
                        Node::Object { state: old, .. } => {
                            if old.replace(state).is_some() {
                                return Err(invalid("重复 BUILD 对象不可可靠编辑"));
                            }
                        }
                        _ => return Err(invalid("pickle BUILD 目标无效")),
                    }
                    continue;
                }
                b'i' => {
                    cursor.line()?;
                    cursor.line()?;
                    Node::Object {
                        dependencies: marked(&mut stack)?,
                        state: None,
                    }
                }
                b'o' => Node::Object {
                    dependencies: marked(&mut stack)?,
                    state: None,
                },
                0x82..=0x84 => {
                    cursor.take(match op {
                        0x82 => 1,
                        0x83 => 2,
                        _ => 4,
                    })?;
                    Node::Opaque
                }
                b'P' => {
                    cursor.line()?;
                    Node::Opaque
                }
                b'Q' => {
                    let dependency = pop(&mut stack)?;
                    Node::Object {
                        dependencies: vec![dependency],
                        state: None,
                    }
                }
                _ => {
                    return Err(invalid(format!(
                        "未支持或需要外部缓冲区的 pickle opcode 0x{op:02x}；禁止写入"
                    )))
                }
            };
            if frames
                .last()
                .is_some_and(|(s, e)| start >= *s + 9 && start < *e && cursor.pos > *e)
            {
                return Err(invalid("pickle opcode 跨越 FRAME"));
            }
            if entries.len() >= 500_000 || stack.len() >= 500_000 {
                return Err(invalid("pickle 节点或栈过大"));
            }
            let id = entries.len();
            entries.push(Entry {
                node,
                span: start..cursor.pos,
                dictionary_checkpoint: cursor.pos,
            });
            stack.push(Some(id));
        };
        let mut references = vec![0; entries.len()];
        references[root] += 1;
        for entry in &entries {
            match &entry.node {
                Node::Sequence(ids) => {
                    for id in ids {
                        references[*id] += 1;
                    }
                }
                Node::Dict(items) => {
                    for (k, v) in items {
                        references[*k] += 1;
                        references[*v] += 1;
                    }
                }
                Node::Object {
                    dependencies,
                    state,
                } => {
                    for id in dependencies.iter().chain(state.iter()) {
                        references[*id] += 1;
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            entries,
            root,
            protocol,
            frames,
            references,
        })
    }
    fn dictionary_id(&self, persistent: bool) -> Result<usize> {
        let mut id = self.root;
        if !persistent {
            id = match &self.entries[id].node {
                Node::Sequence(ids) if ids.len() == 2 => ids[0],
                _ => return Err(invalid("Ren’Py log 不是 (roots, rollback log)")),
            };
        } else if let Node::Object {
            state: Some(state), ..
        } = &self.entries[id].node
        {
            id = *state;
            if let Node::Sequence(ids) = &self.entries[id].node {
                id = *ids
                    .first()
                    .ok_or_else(|| invalid("persistent state 为空"))?;
            }
        }
        match &self.entries[id].node {
            Node::Dict(_) => Ok(id),
            _ => Err(invalid("无法明确定位 Ren’Py 顶层变量字典")),
        }
    }
    fn dictionary(&self, persistent: bool) -> Result<&[(usize, usize)]> {
        match &self.entries[self.dictionary_id(persistent)?].node {
            Node::Dict(items) => Ok(items),
            _ => unreachable!("validated dictionary"),
        }
    }
    fn persistent_timestamps(&self) -> Result<Option<usize>> {
        let ids: Vec<_> = self
            .dictionary(true)?
            .iter()
            .filter_map(|(key, value)| match &self.entries[*key].node {
                Node::Scalar(Value::String(name)) if name == "_changed" => Some(*value),
                _ => None,
            })
            .collect();
        match ids.as_slice() {
            [] => Ok(None),
            [id] if matches!(self.entries[*id].node, Node::Dict(_))
                && self.references[*id] == 1 =>
            {
                Ok(Some(*id))
            }
            _ => Err(invalid(
                "persistent _changed 不是唯一的标准字典，无法安全更新时间戳",
            )),
        }
    }
    fn timestamp(&self, dict: usize, name: &str) -> Result<Option<usize>> {
        let Node::Dict(items) = &self.entries[dict].node else {
            return Err(invalid("时间戳不是字典"));
        };
        let matches: Vec<_> = items
            .iter()
            .filter_map(|(key, id)| match &self.entries[*key].node {
                Node::Scalar(Value::String(key)) if key == name => Some(*id),
                _ => None,
            })
            .collect();
        match matches.as_slice() {
            [] => Ok(None),
            [id] if matches!(&self.entries[*id].node,Node::Scalar(v) if v.is_number())
                && self.references[*id] == 1 =>
            {
                Ok(Some(*id))
            }
            _ => Err(invalid("此字段的 persistent 时间戳不唯一或存在共享引用")),
        }
    }
    fn timestamp_index(&self, dict: usize) -> HashMap<&str, Option<usize>> {
        let mut index = HashMap::new();
        if let Node::Dict(items) = &self.entries[dict].node {
            for (key, id) in items {
                if let Node::Scalar(Value::String(name)) = &self.entries[*key].node {
                    let safe = matches!(&self.entries[*id].node,Node::Scalar(v) if v.is_number())
                        && self.references[*id] == 1;
                    match index.entry(name.as_str()) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(safe.then_some(*id));
                        }
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            entry.insert(None);
                        }
                    }
                }
            }
        }
        index
    }
    pub fn fields(&self, persistent: bool) -> Result<Vec<Field>> {
        let items = self.dictionary(persistent)?;
        let timestamp_dict = if persistent {
            self.persistent_timestamps()
        } else {
            Ok(None)
        };
        let timestamp_values = timestamp_dict
            .as_ref()
            .ok()
            .copied()
            .flatten()
            .map(|id| self.timestamp_index(id));
        let mut counts = HashMap::new();
        for (key, _) in items {
            if let Node::Scalar(Value::String(name)) = &self.entries[*key].node {
                *counts.entry(name).or_insert(0) += 1;
            }
        }
        let mut fields = Vec::new();
        for (index, (key, id)) in items.iter().enumerate() {
            let Node::Scalar(Value::String(name)) = &self.entries[*key].node else {
                continue;
            };
            if persistent {
                if name.starts_with('_') {
                    continue;
                }
            } else if !name.starts_with("store.") {
                continue;
            }
            let value = match &self.entries[*id].node {
                Node::Scalar(value) => value.clone(),
                _ => Value::Null,
            };
            let mut field = Field::new(
                format!("pickle:{index}"),
                name.clone(),
                if persistent {
                    "persistent"
                } else {
                    "variables"
                },
                value,
            );
            field.path = name.clone();
            if !persistent
                && (name.split('.').skip(1).any(|part| part.starts_with('_'))
                    || name.starts_with("store.renpy.")
                    || name.starts_with("store.config."))
            {
                field.readonly("Ren’Py 内建或私有状态，只读");
            }
            if counts[name] > 1 {
                field.readonly("变量名称重复，目标不唯一");
            } else if self.references[*id] != 1 {
                field.readonly("值存在共享引用，修改可能影响其他对象");
            } else if field
                .value
                .as_f64()
                .is_some_and(|n| !n.is_finite() || n.abs() > 9_007_199_254_740_991.0)
            {
                field.readonly("数值超出安全精度范围");
            }
            if persistent && field.editable {
                if let Err(error) = &timestamp_dict {
                    field.readonly(&error.to_string());
                } else if timestamp_values
                    .as_ref()
                    .and_then(|map| map.get(name.as_str()))
                    .is_some_and(Option::is_none)
                {
                    field.readonly("此字段的 persistent 时间戳不唯一或存在共享引用");
                }
            }
            fields.push(field);
        }
        if fields.len() > 60_000 {
            return Err(invalid("存档字段数量超过轻量编辑器限制"));
        }
        Ok(fields)
    }
    pub fn patch(&self, bytes: &[u8], persistent: bool, changes: &[Change]) -> Result<Vec<u8>> {
        let fields = self.fields(persistent)?;
        validate_changes(&fields, changes)?;
        let mut replacements = Vec::new();
        for change in changes {
            let index: usize = change
                .id
                .strip_prefix("pickle:")
                .ok_or_else(|| invalid("无效 pickle 字段"))?
                .parse()
                .map_err(format_error)?;
            let id = self
                .dictionary(persistent)?
                .get(index)
                .ok_or_else(|| invalid("无效 pickle 字段"))?
                .1;
            let entry = &self.entries[id];
            let mut out = Vec::new();
            match &change.value {
                Value::Bool(value) => {
                    if self.protocol >= 2 {
                        out.push(if *value { 0x88 } else { 0x89 });
                    } else {
                        out.extend_from_slice(if *value { b"I01\n" } else { b"I00\n" });
                    }
                }
                Value::Number(number) => {
                    let original = match &entry.node {
                        Node::Scalar(Value::Number(n)) => n,
                        _ => return Err(invalid("原字段非数值")),
                    };
                    if original.is_i64() || original.is_u64() {
                        let value = number
                            .as_i64()
                            .ok_or_else(|| invalid("整数变量不能改为小数"))?;
                        if self.protocol < 2 {
                            out.extend_from_slice(format!("L{value}L\n").as_bytes());
                        } else {
                            out.extend_from_slice(&[0x8a, 8]);
                            out.extend_from_slice(&value.to_le_bytes());
                        }
                    } else {
                        out.push(b'G');
                        out.extend_from_slice(
                            &number
                                .as_f64()
                                .ok_or_else(|| invalid("无效小数"))?
                                .to_be_bytes(),
                        );
                    }
                }
                Value::String(text) => {
                    if self.protocol == 0 {
                        out.push(b'V');
                        for ch in text.chars() {
                            if ch == '\\' || ch == '\n' || ch == '\r' || ch as u32 > 127 {
                                out.extend_from_slice(format!("\\U{:08x}", ch as u32).as_bytes());
                            } else {
                                out.push(ch as u8);
                            }
                        }
                        out.push(b'\n');
                    } else {
                        out.push(b'X');
                        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
                        out.extend_from_slice(text.as_bytes());
                    }
                }
                _ => return Err(invalid("不能修改复杂 pickle 对象")),
            }
            replacements.push((entry.span.clone(), out));
        }
        if persistent {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(format_error)?
                .as_secs_f64();
            let mut time = vec![b'G'];
            time.extend_from_slice(&now.to_be_bytes());
            let dict = self.persistent_timestamps()?;
            let mut additions = Vec::new();
            for change in changes {
                let name = &fields
                    .iter()
                    .find(|f| f.id == change.id)
                    .ok_or_else(|| invalid("无效 persistent 字段"))?
                    .path;
                let timestamp = dict
                    .map(|id| self.timestamp(id, name))
                    .transpose()?
                    .flatten();
                if let Some(id) = timestamp {
                    replacements.push((self.entries[id].span.clone(), time.clone()));
                } else {
                    additions.extend(unicode_opcode(name, self.protocol));
                    additions.extend_from_slice(&time);
                    additions.push(b's');
                }
            }
            if !additions.is_empty() {
                let (id, bytes) = if let Some(dict) = dict {
                    (dict, additions)
                } else {
                    let mut bytes = unicode_opcode("_changed", self.protocol);
                    bytes.push(b'}');
                    bytes.extend(additions);
                    bytes.push(b's');
                    (self.dictionary_id(true)?, bytes)
                };
                let position = self.entries[id].dictionary_checkpoint;
                replacements.push((position..position, bytes));
            }
        }
        for (start, end) in &self.frames {
            let delta: i64 = replacements
                .iter()
                .filter(|(span, _)| span.start >= start + 9 && span.end <= *end)
                .map(|(span, bytes)| bytes.len() as i64 - span.len() as i64)
                .sum();
            let length = (*end - *start - 9) as i64 + delta;
            replacements.push((start + 1..start + 9, (length as u64).to_le_bytes().to_vec()));
        }
        replacements.sort_by_key(|(span, _)| span.start);
        let mut output = Vec::new();
        let mut previous = 0;
        for (span, data) in replacements {
            if span.start < previous {
                return Err(invalid("pickle 修改范围重叠"));
            }
            output.extend_from_slice(&bytes[previous..span.start]);
            output.extend(data);
            previous = span.end;
        }
        output.extend_from_slice(&bytes[previous..]);
        let next = Self::parse(&output)?.fields(persistent)?;
        let by_id: HashMap<_, _> = next
            .iter()
            .map(|field| (field.id.as_str(), field))
            .collect();
        let changed: HashMap<_, _> = changes
            .iter()
            .map(|change| (change.id.as_str(), &change.value))
            .collect();
        for field in fields {
            let expected = changed
                .get(field.id.as_str())
                .copied()
                .unwrap_or(&field.value);
            if !by_id
                .get(field.id.as_str())
                .is_some_and(|f| same_value(&f.value, expected) && f.path == field.path)
            {
                return Err(invalid("pickle 修改后变量或引用校验失败"));
            }
        }
        Ok(output)
    }
}

fn unicode_opcode(text: &str, protocol: u8) -> Vec<u8> {
    if protocol == 0 {
        let mut out = vec![b'V'];
        for ch in text.chars() {
            if ch == '\\' || ch == '\n' || ch == '\r' || ch as u32 > 127 {
                out.extend_from_slice(format!("\\U{:08x}", ch as u32).as_bytes());
            } else {
                out.push(ch as u8);
            }
        }
        out.push(b'\n');
        out
    } else {
        let mut out = vec![b'X'];
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
        out
    }
}
