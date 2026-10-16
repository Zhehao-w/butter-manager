//! Strict, bounded JSON with original byte ranges. Only selected scalar tokens are replaced.
use super::*;
use std::ops::Range;

pub(super) struct Node {
    pub span: Range<usize>,
    pub value: Value,
    pub children: Vec<(String, Node)>,
}
impl Node {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut parser = Parser {
            data,
            at: 0,
            count: 0,
        };
        let root = parser.node(0)?;
        parser.space();
        if parser.at != data.len() {
            return Err(invalid("JSON 包含异常尾部"));
        }
        if !root.value.is_object() && !root.value.is_array() {
            return Err(invalid("存档 JSON 顶层必须是对象或数组"));
        }
        Ok(root)
    }
    pub fn child(&self, key: &str) -> Option<&Node> {
        self.children.iter().find(|(k, _)| k == key).map(|(_, n)| n)
    }
}
struct Parser<'a> {
    data: &'a [u8],
    at: usize,
    count: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .data
            .get(self.at)
            .is_some_and(|b| b" \r\n\t".contains(b))
        {
            self.at += 1;
        }
    }
    fn string(&mut self) -> Result<String> {
        let start = self.at;
        if self.data.get(self.at) != Some(&b'"') {
            return Err(invalid("JSON 键不是字符串"));
        }
        self.at += 1;
        while let Some(&byte) = self.data.get(self.at) {
            self.at += 1;
            match byte {
                b'\\' => {
                    self.at += 1;
                }
                b'"' => {
                    return serde_json::from_slice(&self.data[start..self.at]).map_err(format_error)
                }
                _ => (),
            }
        }
        Err(invalid("JSON 字符串截断"))
    }
    fn node(&mut self, depth: usize) -> Result<Node> {
        self.count += 1;
        if depth > 64 || self.count > 100_000 {
            return Err(invalid("JSON 层数或节点数量超限"));
        }
        self.space();
        let start = self.at;
        let token = *self.data.get(self.at).ok_or_else(|| invalid("JSON 截断"))?;
        let mut children = Vec::new();
        let value = match token {
            b'{' | b'[' => {
                self.at += 1;
                let object = token == b'{';
                let end = if object { b'}' } else { b']' };
                let mut keys = HashSet::new();
                self.space();
                if self.data.get(self.at) != Some(&end) {
                    loop {
                        self.space();
                        let key = if object {
                            let key = self.string()?;
                            if !keys.insert(key.clone()) {
                                return Err(invalid("JSON 包含重复键，禁止编辑"));
                            }
                            self.space();
                            if self.data.get(self.at) != Some(&b':') {
                                return Err(invalid("JSON 缺少冒号"));
                            }
                            self.at += 1;
                            key
                        } else {
                            children.len().to_string()
                        };
                        children.push((key, self.node(depth + 1)?));
                        self.space();
                        if self.data.get(self.at) == Some(&end) {
                            break;
                        }
                        if self.data.get(self.at) != Some(&b',') {
                            return Err(invalid("JSON 缺少分隔符"));
                        }
                        self.at += 1;
                    }
                }
                self.at += 1;
                // Container values are markers; original content lives in children and bytes.
                if object {
                    Value::Object(Default::default())
                } else {
                    Value::Array(Vec::new())
                }
            }
            b'"' => Value::String(self.string()?),
            _ => {
                while self
                    .data
                    .get(self.at)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}".contains(b))
                {
                    self.at += 1;
                }
                serde_json::from_slice(&self.data[start..self.at]).map_err(format_error)?
            }
        };
        Ok(Node {
            span: start..self.at,
            value,
            children,
        })
    }
}

pub(super) fn patch(data: &[u8], mut patches: Vec<(Range<usize>, Vec<u8>)>) -> Result<Vec<u8>> {
    patches.sort_by_key(|(range, _)| range.start);
    let mut output = Vec::new();
    let mut at = 0;
    for (range, bytes) in patches {
        if range.start < at || range.end > data.len() {
            return Err(invalid("修改字段范围重叠或无效"));
        }
        output.extend_from_slice(&data[at..range.start]);
        output.extend_from_slice(&bytes);
        at = range.end;
        if output.len() > DECODE_LIMIT {
            return Err(invalid("修改后的数据超过大小限制"));
        }
    }
    output.extend_from_slice(&data[at..]);
    if output.len() > DECODE_LIMIT {
        return Err(invalid("修改后的数据超过大小限制"));
    }
    Ok(output)
}
