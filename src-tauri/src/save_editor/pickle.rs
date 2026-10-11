//! A symbolic pickle stack machine. GLOBAL/REDUCE/BUILD never execute code.
//! Scalar patches preserve other opcodes; known set edits replace only the BUILD state.
use super::*;
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug)]
enum Node {
    Scalar(Value),
    Sequence(Vec<usize>),
    Dict(Vec<(usize, usize)>),
    Global {
        module: String,
        name: String,
        dependencies: Vec<usize>,
    },
    Object {
        dependencies: Vec<usize>,
        state: Option<usize>,
    },
    Opaque,
}
static EMPTY_LIST: Node = Node::Sequence(Vec::new());
static EMPTY_DICT: Node = Node::Dict(Vec::new());
struct Entry {
    node: Node,
    span: std::ops::Range<usize>,
    dictionary_checkpoint: usize,
    type_name: &'static str,
    collection: Option<Node>,
    build_position: Option<usize>,
}
pub(super) struct Pickle {
    entries: Vec<Entry>,
    root: usize,
    protocol: u8,
    frames: Vec<(usize, usize)>,
    references: Vec<usize>,
}
struct FieldTarget {
    field: Field,
    entry: usize,
    top_name: String,
}
struct FieldLocation {
    id: String,
    path: String,
    parent: Option<String>,
    top_name: String,
    safe: bool,
    ancestors: Vec<usize>,
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
                        Node::Object { .. } => {
                            let collection = entries[target]
                                .collection
                                .get_or_insert_with(|| Node::Sequence(Vec::new()));
                            let Node::Sequence(items) = collection else {
                                return Err(invalid("混合 pickle 容器类型"));
                            };
                            items.extend(ids);
                        }
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
                        Node::Object { .. } => {
                            let collection = entries[target]
                                .collection
                                .get_or_insert_with(|| Node::Dict(Vec::new()));
                            let Node::Dict(items) = collection else {
                                return Err(invalid("混合 pickle 容器类型"));
                            };
                            items.extend(
                                ids.as_chunks::<2>().0.iter().map(|pair| (pair[0], pair[1])),
                            );
                        }
                        _ => return Err(invalid("非标准 pickle SETITEMS 目标")),
                    }
                    continue;
                }
                b'c' => {
                    let module = std::str::from_utf8(cursor.line()?)
                        .map_err(format_error)?
                        .to_owned();
                    let name = std::str::from_utf8(cursor.line()?)
                        .map_err(format_error)?
                        .to_owned();
                    Node::Global {
                        module,
                        name,
                        dependencies: Vec::new(),
                    }
                }
                0x93 => {
                    let name = pop(&mut stack)?;
                    let module = pop(&mut stack)?;
                    match (&entries[module].node, &entries[name].node) {
                        (
                            Node::Scalar(Value::String(module_name)),
                            Node::Scalar(Value::String(class_name)),
                        ) => Node::Global {
                            module: module_name.clone(),
                            name: class_name.clone(),
                            dependencies: vec![module, name],
                        },
                        _ => return Err(invalid("STACK_GLOBAL 模块或名称不是字符串")),
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
                            entries[object].build_position = Some(start);
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
                type_name: match op {
                    b'N' => "NoneType",
                    b']' | b'l' => "list",
                    b')' | b't' | 0x85..=0x87 => "tuple",
                    0x8f => "set",
                    0x91 => "frozenset",
                    b'}' | b'd' => "dict",
                    b'S' | b'T' | b'U' | b'B' | b'C' | 0x8e => "bytes",
                    0x96 => "bytearray",
                    b'L' | b'I' | 0x8a | 0x8b => "integer",
                    b'F' | b'G' => "float",
                    b'c' | 0x93 => "global",
                    _ => "object",
                },
                collection: None,
                build_position: None,
            });
            stack.push(Some(id));
        };
        let mut references = vec![0; entries.len()];
        references[root] += 1;
        for entry in &entries {
            for node in std::iter::once(&entry.node).chain(entry.collection.iter()) {
                match node {
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
                    Node::Global { dependencies, .. } => {
                        for id in dependencies {
                            references[*id] += 1;
                        }
                    }
                    _ => {}
                }
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
    fn object_class(&self, id: usize) -> Option<(&str, &str)> {
        let Node::Object { dependencies, .. } = &self.entries[id].node else {
            return None;
        };
        let Node::Global { module, name, .. } = &self.entries[*dependencies.first()?].node else {
            return None;
        };
        if matches!(module.as_str(), "copy_reg" | "copyreg") && name == "_reconstructor" {
            let Node::Sequence(args) = &self.entries[*dependencies.get(1)?].node else {
                return None;
            };
            let Node::Global { module, name, .. } = &self.entries[*args.first()?].node else {
                return None;
            };
            return Some((module, name));
        }
        Some((module, name))
    }
    fn standard_wrapper(&self, id: usize, class: &str) -> bool {
        self.object_class(id).is_some_and(|(module, name)| {
            matches!(module, "renpy.revertable" | "renpy.python") && name == class
        })
    }
    fn empty_constructor(&self, id: usize) -> bool {
        matches!(&self.entries[id].node, Node::Object { dependencies, .. }
            if dependencies.len() == 2 && matches!(&self.entries[dependencies[1]].node, Node::Sequence(args) if args.is_empty()))
    }
    fn string_set_state(&self, id: usize) -> Option<(usize, usize, Vec<String>)> {
        if !self.standard_wrapper(id, "RevertableSet") || !self.empty_constructor(id) {
            return None;
        }
        let Node::Object {
            state: Some(state), ..
        } = &self.entries[id].node
        else {
            return None;
        };
        let Node::Sequence(parts) = &self.entries[*state].node else {
            return None;
        };
        if parts.len() != 1 || self.entries[*state].type_name != "tuple" {
            return None;
        }
        let dict = parts[0];
        let Node::Dict(items) = &self.entries[dict].node else {
            return None;
        };
        let mut members = Vec::new();
        for (key, value) in items {
            let Node::Scalar(Value::String(name)) = &self.entries[*key].node else {
                return None;
            };
            if !matches!(&self.entries[*value].node, Node::Scalar(Value::Bool(true))) {
                return None;
            }
            members.push(name.clone());
        }
        members.sort();
        if members.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        Some((*state, dict, members))
    }
    fn collection(&self, id: usize) -> Option<&Node> {
        match &self.entries[id].node {
            node @ (Node::Sequence(_) | Node::Dict(_)) => Some(node),
            Node::Object { state: None, .. }
                if self.standard_wrapper(id, "RevertableList")
                    || self.standard_wrapper(id, "RevertableDict") =>
            {
                // Only known containers with empty constructor arguments may expose writable children.
                if !self.empty_constructor(id) {
                    return None;
                }
                self.entries[id]
                    .collection
                    .as_ref()
                    .or(Some(if self.standard_wrapper(id, "RevertableList") {
                        &EMPTY_LIST
                    } else {
                        &EMPTY_DICT
                    }))
                    .filter(|node| {
                        matches!(node, Node::Sequence(_))
                            && self.standard_wrapper(id, "RevertableList")
                            || matches!(node, Node::Dict(_))
                                && self.standard_wrapper(id, "RevertableDict")
                    })
            }
            _ => None,
        }
    }
    fn visit(&self, id: usize, location: FieldLocation, out: &mut Vec<FieldTarget>) -> Result<()> {
        if out.len() >= 60_000 {
            return Err(invalid("存档字段数量超过轻量编辑器限制"));
        }
        let entry = &self.entries[id];
        let cycle = location.ancestors.contains(&id);
        let bounded = location.ancestors.len() < 12;
        let safe = location.safe && self.references[id] == 1 && !cycle && bounded;
        let value = match &entry.node {
            Node::Scalar(value) => value.clone(),
            _ => Value::Null,
        };
        let mut field = Field::new(
            location.id.clone(),
            location.path.clone(),
            "variables",
            value,
        );
        field.path = location.path.clone();
        field.parent_id = location.parent.clone();
        field.type_name = Some(match &entry.node {
            Node::Scalar(Value::String(_)) => "str".into(),
            Node::Scalar(Value::Bool(_)) => "bool".into(),
            Node::Scalar(Value::Number(value)) => if value.is_i64() || value.is_u64() {
                "int"
            } else {
                "float"
            }
            .into(),
            Node::Object { .. } => self
                .object_class(id)
                .map(|(m, n)| format!("{m}.{n}"))
                .unwrap_or_else(|| "未知对象".into()),
            _ => entry.type_name.into(),
        });
        if matches!(&entry.node, Node::Scalar(Value::Null)) && entry.type_name == "NoneType" {
            field.kind = "null".into();
            field.editable = true;
            field.reason = None;
        } else if let Some((state, dict, members)) = self.string_set_state(id) {
            field.kind = "string_set".into();
            field.child_count = Some(members.len());
            field.value = json!(members);
            field.editable =
                self.protocol >= 2 && self.references[state] == 1 && self.references[dict] == 1;
            field.reason =
                (!field.editable).then(|| "集合状态存在共享引用或协议不支持，只读".into());
        } else if let Some(collection) = self.collection(id) {
            field.kind = "container".into();
            field.child_count = Some(match collection {
                Node::Sequence(ids) => ids.len(),
                Node::Dict(items) => items.len(),
                _ => 0,
            });
            field.readonly("展开查看子字段");
        } else if !matches!(
            entry.node,
            Node::Scalar(Value::Number(_) | Value::String(_) | Value::Bool(_))
        ) {
            field.readonly("此类型暂未开放编辑");
        }
        if !safe {
            field.readonly(if cycle {
                "循环引用，只读"
            } else if !bounded {
                "嵌套超过 12 层，只读"
            } else if self.references[id] != 1 {
                "值存在共享引用，修改可能影响其他对象"
            } else {
                "所属容器或字段不允许安全修改"
            });
        }
        out.push(FieldTarget {
            field,
            entry: id,
            top_name: location.top_name.clone(),
        });
        if cycle || !bounded {
            return Ok(());
        }
        let Some(collection) = self.collection(id) else {
            return Ok(());
        };
        let mut ancestors = location.ancestors.clone();
        ancestors.push(id);
        let child = |suffix: String, path: String, writable: bool| FieldLocation {
            id: format!("{}{}", location.id, suffix),
            path,
            parent: Some(location.id.clone()),
            top_name: location.top_name.clone(),
            safe: safe && writable,
            ancestors: ancestors.clone(),
        };
        match collection {
            Node::Sequence(ids) => {
                let writable = !matches!(entry.type_name, "set" | "frozenset");
                for (index, value) in ids.iter().enumerate() {
                    self.visit(
                        *value,
                        child(
                            format!("/i{index}"),
                            format!("{}[{index}]", location.path),
                            writable,
                        ),
                        out,
                    )?;
                }
            }
            Node::Dict(items) => {
                let mut keys = HashMap::new();
                for (key, _) in items {
                    if let Node::Scalar(value) = &self.entries[*key].node {
                        if value.is_string() || value.is_boolean() || value.is_number() {
                            *keys.entry(dictionary_key(value)).or_insert(0usize) += 1;
                        }
                    }
                }
                for (index, (key, value)) in items.iter().enumerate() {
                    let Node::Scalar(key) = &self.entries[*key].node else {
                        continue;
                    };
                    if !(key.is_string() || key.is_boolean() || key.is_number()) {
                        continue;
                    }
                    let unique = keys.get(&dictionary_key(key)) == Some(&1);
                    let key = key.to_string();
                    self.visit(
                        *value,
                        child(
                            format!("/d{index}"),
                            format!("{}[{key}]", location.path),
                            unique,
                        ),
                        out,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn targets(&self, persistent: bool) -> Result<Vec<FieldTarget>> {
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
                *counts.entry(name).or_insert(0usize) += 1;
            }
        }
        let mut targets = Vec::new();
        for (index, (key, id)) in items.iter().enumerate() {
            let Node::Scalar(Value::String(name)) = &self.entries[*key].node else {
                continue;
            };
            if persistent && name.starts_with('_') || !persistent && !name.starts_with("store.") {
                continue;
            }
            let private = !persistent
                && (name.split('.').skip(1).any(|part| part.starts_with('_'))
                    || name.starts_with("store.renpy.")
                    || name.starts_with("store.config."));
            let start = targets.len();
            self.visit(
                *id,
                FieldLocation {
                    id: format!("pickle:{index}"),
                    path: name.clone(),
                    parent: None,
                    top_name: name.clone(),
                    safe: !private && counts[name] == 1,
                    ancestors: Vec::new(),
                },
                &mut targets,
            )?;
            for target in &mut targets[start..] {
                target.field.category = if persistent {
                    "persistent"
                } else {
                    "variables"
                }
                .into();
                if private {
                    target.field.readonly("Ren’Py 内建或私有状态，只读");
                } else if counts[name] != 1 {
                    target.field.readonly("变量名称重复，目标不唯一");
                }
                if persistent && target.field.editable {
                    if let Err(error) = &timestamp_dict {
                        target.field.readonly(&error.to_string());
                    } else if timestamp_values
                        .as_ref()
                        .and_then(|map| map.get(name.as_str()))
                        .is_some_and(Option::is_none)
                    {
                        target
                            .field
                            .readonly("此字段的 persistent 时间戳不唯一或存在共享引用");
                    }
                }
            }
        }
        Ok(targets)
    }
    pub fn fields(&self, persistent: bool) -> Result<Vec<Field>> {
        Ok(self
            .targets(persistent)?
            .into_iter()
            .map(|target| target.field)
            .collect())
    }
    pub fn patch(&self, bytes: &[u8], persistent: bool, changes: &[Change]) -> Result<Vec<u8>> {
        let targets = self.targets(persistent)?;
        let fields: Vec<_> = targets.iter().map(|target| target.field.clone()).collect();
        validate_changes(&fields, changes)?;
        let by_id: HashMap<_, _> = targets
            .iter()
            .map(|target| (target.field.id.as_str(), target))
            .collect();
        let mut replacements = Vec::new();
        for change in changes {
            let target = by_id
                .get(change.id.as_str())
                .ok_or_else(|| invalid("无效 pickle 字段"))?;
            let entry = &self.entries[target.entry];
            if target.field.kind == "string_set" {
                let position = entry
                    .build_position
                    .ok_or_else(|| invalid("集合缺少 BUILD 位置"))?;
                let mut state = vec![b'0', b'}']; // Drop the old BUILD state, preserve all original memo definitions.
                for member in change
                    .value
                    .as_array()
                    .ok_or_else(|| invalid("集合成员无效"))?
                {
                    state.extend(unicode_opcode(
                        member
                            .as_str()
                            .ok_or_else(|| invalid("集合成员不是字符串"))?,
                        self.protocol,
                    ));
                    state.extend([0x88, b's']);
                }
                state.push(0x85); // RevertableSet.__setstate__ expects (dict-of-members,).
                replacements.push((position..position, state));
                continue;
            }
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
                    let integer = match &entry.node {
                        Node::Scalar(Value::Number(n)) => n.is_i64() || n.is_u64(),
                        Node::Scalar(Value::Null) if target.field.kind == "null" => {
                            number.is_i64() || number.is_u64()
                        }
                        _ => return Err(invalid("原字段非数值")),
                    };
                    if integer {
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
                Value::Null if target.field.kind == "null" => out.push(b'N'),
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
            let names: std::collections::BTreeSet<_> = changes
                .iter()
                .map(|change| by_id[change.id.as_str()].top_name.as_str())
                .collect();
            for name in names {
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
                .is_some_and(|f| same_field_value(f, expected) && f.path == field.path)
            {
                return Err(invalid("pickle 修改后变量或引用校验失败"));
            }
        }
        Ok(output)
    }
}

fn dictionary_key(value: &Value) -> String {
    match value {
        Value::Bool(value) => format!("number:{}", u8::from(*value)),
        Value::Number(value) => {
            let n = value.as_f64().unwrap_or(f64::NAN);
            format!("number:{}", if n == 0.0 { 0.0 } else { n })
        }
        _ => format!("string:{value}"),
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
