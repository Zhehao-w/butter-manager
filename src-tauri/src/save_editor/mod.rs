//! Save Editor Lite: only discovered, associated slots and explicit scalar paths are writable.
mod pickle;
mod renpy;
mod rpg;
#[cfg(test)]
mod tests;

use crate::domain::{Error, Game, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FILE_LIMIT: usize = 32 * 1024 * 1024;
const DECODE_LIMIT: usize = 64 * 1024 * 1024;
static WRITES: Mutex<()> = Mutex::new(());

// The callback and acquisition share the same gate as import/update/rollback startup.
pub struct WritePermit(Arc<Mutex<usize>>);
impl WritePermit {
    pub fn acquire<T>(
        activity: Arc<Mutex<usize>>,
        check: impl FnOnce() -> Result<T>,
    ) -> Result<(T, Self)> {
        let mut count = activity.lock().map_err(|_| invalid("操作锁不可用"))?;
        let value = check()?;
        *count += 1;
        drop(count);
        Ok((value, Self(activity)))
    }
}
impl Drop for WritePermit {
    fn drop(&mut self) {
        if let Ok(mut count) = self.0.lock() {
            *count = count.saturating_sub(1);
        }
    }
}
fn transient<T>(value: std::io::Result<T>) -> Result<Option<T>> {
    vanished(value.map_err(Error::from))
}
fn vanished<T>(value: Result<T>) -> Result<Option<T>> {
    match value {
        Ok(value) => Ok(Some(value)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn format_error(error: impl std::fmt::Display) -> Error {
    invalid(format!("无法解析标准存档：{error}"))
}
fn same_value(a: &Value, b: &Value) -> bool {
    a == b || (a.is_number() && b.is_number() && a.as_f64() == b.as_f64())
}
fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(invalid("文件超过轻量编辑器的大小限制"));
    }
    Ok(data)
}
fn plain(path: &Path) -> Result<()> {
    for part in path.ancestors() {
        if crate::scanner::is_link(&fs::symlink_metadata(part)?) {
            return Err(invalid("存档路径不能包含链接或重解析点"));
        }
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: String,
    pub kind: String,
    pub value: Value,
    pub editable: bool,
    pub reason: Option<String>,
    pub description: Option<String>,
}
impl Field {
    fn new(path: String, name: String, category: &str, value: Value) -> Self {
        let kind = match &value {
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            Value::String(_) => "string",
            _ => "readonly",
        };
        let editable = kind != "readonly";
        let mut field = Self {
            id: path.clone(),
            path,
            name,
            category: category.into(),
            kind: kind.into(),
            value,
            editable,
            reason: (!editable).then(|| "复杂对象或未支持的类型，只读".into()),
            description: None,
        };
        if field
            .value
            .as_f64()
            .is_some_and(|n| !n.is_finite() || n.abs() > 9_007_199_254_740_991.0)
        {
            field.readonly("数值超出安全精度范围");
        }
        field
    }
    fn readonly(&mut self, reason: &str) {
        self.editable = false;
        self.reason = Some(reason.into());
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Slot {
    pub id: String,
    pub name: String,
    pub format: String,
    pub modified: u64,
    pub external: bool,
}
#[derive(Debug, Serialize)]
pub struct Catalog {
    pub slots: Vec<Slot>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Document {
    pub slot: Slot,
    pub revision: String,
    pub fields: Vec<Field>,
    pub metadata: Vec<String>,
    pub screenshot: Option<String>,
    pub warnings: Vec<String>,
    pub signature: Option<renpy::SignatureInfo>,
}
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub id: String,
    pub value: Value,
}

fn discover(game: &Game) -> Result<(Catalog, Vec<(Slot, PathBuf)>)> {
    discover_with(game, |_| ())
}
fn discover_with(
    game: &Game,
    before_file: impl Fn(&Path),
) -> Result<(Catalog, Vec<(Slot, PathBuf)>)> {
    let locations = if game.save_paths.is_empty() {
        crate::save_detection::detect_for_engine(
            Path::new(&game.install_path),
            &game.working_directory,
            &game.engine,
            game.main_executable.as_deref(),
        )
    } else {
        game.save_paths.clone()
    };
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for location in locations {
        let root = match crate::deletion::resolve_save(game, &location) {
            Ok(path) => path,
            Err(error) => {
                warnings.push(error.to_string());
                continue;
            }
        };
        let Some(root_info) = transient(fs::metadata(&root))? else {
            warnings.push(format!("存档目录不存在：{}", root.display()));
            continue;
        };
        if root_info.is_file() {
            if vanished(plain(&root))?.is_none() {
                continue;
            }
        } else {
            plain(&root)?;
        }
        let directory = if root_info.is_file() {
            root.parent()
                .ok_or_else(|| invalid("无效存档路径"))?
                .to_owned()
        } else {
            root.clone()
        };
        let mut candidates = if root_info.is_file() {
            vec![root.clone()]
        } else {
            fs::read_dir(&root)?
                .take(2049)
                .filter_map(|entry| match transient(entry) {
                    Ok(Some(entry)) => Some(Ok(entry.path())),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>>>()?
        };
        if candidates.len() == 2049 {
            candidates.pop();
            warnings.push(format!(
                "存档目录超过 2048 项，仅检查前 2048 项：{}",
                root.display()
            ));
        }
        for path in candidates {
            before_file(&path);
            let Some(info) = transient(fs::symlink_metadata(&path))? else {
                continue;
            };
            if !info.is_file() || crate::scanner::is_link(&info) {
                continue;
            }
            let name = path
                .file_name()
                .ok_or_else(|| invalid("无效存档文件名"))?
                .to_string_lossy()
                .into_owned();
            let lower = name.to_ascii_lowercase();
            let format = if lower.ends_with(".rpgsave") && lower.starts_with("file") {
                "MV"
            } else if lower.ends_with(".rmmzsave") && lower.starts_with("file") {
                "MZ"
            } else if lower.ends_with(".save") {
                "RenPy"
            } else if lower == "persistent" {
                "Persistent"
            } else {
                continue;
            };
            let Some(path) = transient(dunce::canonicalize(path))? else {
                continue;
            };
            if vanished(plain(&path))?.is_none() {
                continue;
            }
            if path.parent().map(crate::paths::path_key).transpose()?
                != Some(crate::paths::path_key(&directory)?)
            {
                return Err(invalid("存档路径已改变或越出关联目录"));
            }
            if !seen.insert(crate::paths::path_key(&path)?) {
                continue;
            }
            if entries.len() >= 1024 {
                warnings.push("最多显示 1024 个存档，请缩小关联的存档目录范围".into());
                break;
            }
            let id = format!(
                "{:x}",
                Sha256::digest(format!("{}\0{}", game.id, crate::paths::path_key(&path)?))
            );
            let modified = info
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            entries.push((
                Slot {
                    id,
                    name: format!("{name} · {}", directory.display()),
                    format: format.into(),
                    modified,
                    external: false,
                },
                path,
            ));
        }
    }
    entries.sort_by(|a, b| {
        b.0.modified
            .cmp(&a.0.modified)
            .then(a.0.name.cmp(&b.0.name))
    });
    Ok((
        Catalog {
            slots: entries.iter().map(|(s, _)| s.clone()).collect(),
            warnings,
        },
        entries,
    ))
}
pub fn list(game: &Game) -> Result<Catalog> {
    Ok(discover(game)?.0)
}
fn resolve(game: &Game, id: &str) -> Result<(Slot, PathBuf)> {
    discover(game)?
        .1
        .into_iter()
        .find(|(slot, _)| slot.id == id)
        .ok_or_else(|| invalid("存档不属于此游戏，或已被移动；请刷新列表"))
}
fn revision(path: &Path, data: &[u8]) -> Result<String> {
    let info = fs::metadata(path)?;
    let mut hash = Sha256::new();
    hash.update(data);
    hash.update(format!("{:?}:{}", info.modified()?, info.len()));
    Ok(format!("{:x}", hash.finalize()))
}
fn document(game: &Game, mut slot: Slot, path: &Path, data: &[u8]) -> Result<Document> {
    slot.modified = fs::metadata(path)?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (fields, metadata, screenshot, warnings, signature) =
        if slot.format == "MV" || slot.format == "MZ" {
            let decoded = rpg::decode(data, &slot.format)?;
            let auxiliary = rpg::Auxiliary::load(game);
            (
                rpg::fields(&decoded.value, &auxiliary)?,
                rpg::metadata(&decoded.value),
                None,
                auxiliary.warnings,
                None,
            )
        } else {
            let (fields, metadata, screenshot, warnings, signature) =
                renpy::read(game, data, slot.format == "Persistent")?;
            (fields, metadata, screenshot, warnings, Some(signature))
        };
    Ok(Document {
        slot,
        revision: revision(path, data)?,
        fields,
        metadata,
        screenshot,
        warnings,
        signature,
    })
}
pub fn read(game: &Game, id: &str) -> Result<Document> {
    let (slot, path) = resolve(game, id)?;
    let data = read_limited(&path, FILE_LIMIT)?;
    document(game, slot, &path, &data)
}
fn validate_changes(fields: &[Field], changes: &[Change]) -> Result<()> {
    if changes.is_empty() || changes.len() > 2000 {
        return Err(invalid("请选择 1 到 2000 个修改项"));
    }
    if changes
        .iter()
        .filter_map(|change| change.value.as_str())
        .map(str::len)
        .sum::<usize>()
        > 8 * 1024 * 1024
    {
        return Err(invalid("本次字符串修改总量超过 8 MiB"));
    }
    let mut seen = HashSet::new();
    let by_id: std::collections::HashMap<_, _> = fields
        .iter()
        .map(|field| (field.id.as_str(), field))
        .collect();
    for change in changes {
        if !seen.insert(&change.id) {
            return Err(invalid("同一字段不能重复提交"));
        }
        let field = by_id
            .get(change.id.as_str())
            .ok_or_else(|| invalid("不存在的字段"))?;
        if !field.editable {
            return Err(invalid(
                field.reason.clone().unwrap_or_else(|| "字段只读".into()),
            ));
        }
        let compatible = match field.kind.as_str() {
            "number" => change
                .value
                .as_f64()
                .is_some_and(|n| n.is_finite() && n.abs() <= 9_007_199_254_740_991.0),
            "boolean" => change.value.is_boolean(),
            "string" => change
                .value
                .as_str()
                .is_some_and(|s| s.len() <= 1024 * 1024),
            _ => false,
        };
        if !compatible {
            return Err(invalid("修改值的类型或范围无效"));
        }
    }
    Ok(())
}
pub fn apply(game: &Game, id: &str, expected: &str, changes: &[Change]) -> Result<Document> {
    let _guard = WRITES.lock().map_err(|_| invalid("存档写入锁不可用"))?;
    let (slot, path) = resolve(game, id)?;
    write_document(game, slot, &path, expected, changes, false)
}
pub fn resign(game: &Game, id: &str, expected: &str) -> Result<Document> {
    let _guard = WRITES.lock().map_err(|_| invalid("存档写入锁不可用"))?;
    let (slot, path) = resolve(game, id)?;
    write_document(game, slot, &path, expected, &[], true)
}
fn write_document(
    game: &Game,
    slot: Slot,
    path: &Path,
    expected: &str,
    changes: &[Change],
    resign: bool,
) -> Result<Document> {
    plain(path)?;
    let original = read_limited(path, FILE_LIMIT)?;
    if revision(path, &original)? != expected {
        return Err(invalid("存档已被游戏或其他程序更新；请刷新后重新修改"));
    }
    if resign {
        if slot.format != "RenPy" && slot.format != "Persistent" {
            return Err(invalid("仅支持重新签名 Ren’Py 存档"));
        }
    } else {
        let before = document(game, slot.clone(), path, &original)?;
        validate_changes(&before.fields, changes)?;
    }
    let output = if slot.format == "MV" || slot.format == "MZ" {
        let mut decoded = rpg::decode(&original, &slot.format)?;
        rpg::apply(&mut decoded.value, changes)?;
        let output = rpg::encode(&decoded)?;
        if rpg::decode(&output, &slot.format)?.value != decoded.value {
            return Err(invalid("存档往返校验失败"));
        }
        output
    } else {
        renpy::apply(game, &original, slot.format == "Persistent", changes)?
    };
    if output.len() > FILE_LIMIT {
        return Err(invalid("修改后的存档超过 32 MiB，未写入"));
    }
    if !changes.is_empty() {
        let after = document(game, slot.clone(), path, &output)?;
        let by_id: std::collections::HashMap<_, _> = after
            .fields
            .iter()
            .map(|field| (field.id.as_str(), field))
            .collect();
        for change in changes {
            if !by_id
                .get(change.id.as_str())
                .is_some_and(|f| same_value(&f.value, &change.value))
            {
                return Err(invalid("修改后的字段校验失败"));
            }
        }
    }
    // A complete, validated sibling temporary file; never a backup of the source.
    let temp = path.with_file_name(format!(".butter-save-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&output)?;
        file.sync_all()?;
        drop(file);
        plain(path)?;
        if revision(path, &read_limited(path, FILE_LIMIT)?)? != expected {
            return Err(invalid("存档在保存过程中发生变化；未覆盖，请刷新"));
        }
        replace(&temp, path)?;
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    document(game, slot, path, &read_limited(path, FILE_LIMIT)?)
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)?;
    Ok(())
}
#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(invalid(format!(
            "无法写入存档（文件可能正被占用）：{}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

// Native-picker grants are process-local, bounded, short-lived and bound to a target game.
// Neither IPC nor the UI can create a grant by supplying a path.
#[derive(Default)]
pub struct ExternalSaves(Mutex<std::collections::HashMap<String, ExternalGrant>>);
struct ExternalGrant {
    game: String,
    path: PathBuf,
    slot: Slot,
    revision: String,
    created: std::time::Instant,
}
impl ExternalSaves {
    pub fn choose(&self, game: &Game, path: &Path) -> Result<Document> {
        plain(path)?;
        let path = dunce::canonicalize(path)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(invalid("请选择 Ren’Py 存档文件"));
        }
        let name = path
            .file_name()
            .ok_or_else(|| invalid("无效存档文件名"))?
            .to_string_lossy();
        let format = if name.eq_ignore_ascii_case("persistent") {
            "Persistent"
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("save"))
        {
            "RenPy"
        } else {
            return Err(invalid("只支持 .save 或 persistent 文件"));
        };
        let slot = Slot {
            id: format!("external-{}", uuid::Uuid::new_v4()),
            name: format!("外部 · {}", path.display()),
            format: format.into(),
            modified: fs::metadata(&path)?
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            external: true,
        };
        let doc = document(game, slot.clone(), &path, &read_limited(&path, FILE_LIMIT)?)?;
        let mut grants = self.0.lock().map_err(|_| invalid("外部存档授权锁不可用"))?;
        grants.retain(|_, grant| grant.created.elapsed().as_secs() < 7200);
        if grants.len() >= 32 {
            return Err(invalid("外部存档过多，请关闭编辑器后重新选择"));
        }
        grants.insert(
            slot.id.clone(),
            ExternalGrant {
                game: game.id.clone(),
                path,
                slot,
                revision: doc.revision.clone(),
                created: std::time::Instant::now(),
            },
        );
        Ok(doc)
    }
    fn resolve(&self, game: &Game, id: &str) -> Result<(Slot, PathBuf, String)> {
        let grants = self.0.lock().map_err(|_| invalid("外部存档授权锁不可用"))?;
        let grant = grants
            .get(id)
            .filter(|g| g.game == game.id && g.created.elapsed().as_secs() < 7200)
            .ok_or_else(|| invalid("外部存档授权已失效，请重新通过文件选择器选择"))?;
        Ok((
            grant.slot.clone(),
            grant.path.clone(),
            grant.revision.clone(),
        ))
    }
    pub fn read(&self, game: &Game, id: &str) -> Result<Document> {
        let (slot, path, expected) = self.resolve(game, id)?;
        plain(&path)?;
        let data = read_limited(&path, FILE_LIMIT)?;
        if revision(&path, &data)? != expected {
            return Err(invalid("外部存档已发生变化，请重新选择文件"));
        }
        document(game, slot, &path, &data)
    }
    pub fn write(
        &self,
        game: &Game,
        id: &str,
        expected: &str,
        changes: Option<&[Change]>,
        trusted: bool,
    ) -> Result<Document> {
        if !trusted {
            return Err(invalid(
                "请先确认外部存档来源可信；重新签名后游戏可能不再提示外来存档警告",
            ));
        }
        let _guard = WRITES.lock().map_err(|_| invalid("存档写入锁不可用"))?;
        let (slot, path, granted_revision) = self.resolve(game, id)?;
        if granted_revision != expected {
            return Err(invalid("外部存档版本与授权不一致，请重新选择"));
        }
        let doc = write_document(
            game,
            slot,
            &path,
            expected,
            changes.unwrap_or_default(),
            changes.is_none(),
        )?;
        if let Some(grant) = self
            .0
            .lock()
            .map_err(|_| invalid("外部存档授权锁不可用"))?
            .get_mut(id)
        {
            grant.revision = doc.revision.clone();
            grant.slot = doc.slot.clone();
        }
        Ok(doc)
    }
    pub fn release(&self, game_id: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| invalid("外部存档授权锁不可用"))?
            .retain(|_, grant| grant.game != game_id);
        Ok(())
    }
}
