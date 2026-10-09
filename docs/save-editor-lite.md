# Save Editor Lite v0.1

入口：游戏详情 → 存档位置 → **编辑存档**。只发现已保存的 `save_paths` 中的槽位；支持关联目录或单个存档文件，没有配置时复用现有存档位置识别。新增存档目录应先保存游戏资料。关联位置可以在游戏内，也可以在 AppData；编辑器不移动游戏或存档。

选择槽位后可搜索名称、ID、字段路径，按分类编辑多个字段后统一保存。超过 80 行使用虚拟滚动。关闭、切换槽位或刷新时，有未保存修改会先询问是否放弃。不可安全编辑的字段显示原因。

## 支持范围

| 格式 | 支持 | 限制 |
| --- | --- | --- |
| RPG Maker MV | 标准 `file*.rpgsave`，LZString Base64；变量数字/布尔/字符串、开关、金钱、道具/武器/防具数量 | 不提供任意 JSON 编辑；复杂变量只读 |
| RPG Maker MZ | 标准 `file*.rmmzsave`，pako zlib 的 UTF-8 二进制字符串，兼容原始 zlib 字节并保留原编码 | ZIP、加密或插件自定义封装拒绝写入 |
| Ren’Py `.save` | ZIP 的 `log`、`signatures`；唯一的 `store.*` 整数、小数、布尔、Unicode 字符串；元信息与 PNG 截图 | None、列表、字典、自定义对象、内建/私有状态、共享引用、重复名称只读 |
| Ren’Py `persistent` | zlib pickle + 尾部签名；顶层公开简单字段；同步标准 `_changed` 修改时间戳 | 复杂对象原字节保留，不重建；无法安全维护时间戳时只读 |

RPG Maker 辅助 JSON 只读加载 `System`、`Items`、`Weapons`、`Armors`、`Actors`、`Classes`。优先启动文件旁的 `data/`、`www/data/`，再检查游戏根目录及至多 64 个直接包装目录；不无界递归。数据库缺失时按 ID 显示，不允许凭空创建未知物品。System 声明但未赋值的变量默认为 0，开关为 false，可以赋值；数据库存在但背包没有的物品可以增加。角色展示名称/职业，HP/MP、等级、经验、装备与技能首版全部只读。

MV/MZ 修改保留 JsonEx 元数据和未知 JSON 字段。兼容旧版 MV 的 `@a` 数组包装，编辑时保留 `@c` 身份标记和 `@r` 引用；变量/开关容器存在共享引用时保持只读，避免连带修改其他对象。Ren’Py 通过符号化 pickle 栈、memo 与对象依赖解析定位标量，处理常用协议 0–5；不会执行 GLOBAL/REDUCE、实例化 Python 类或载入游戏脚本。补丁只改标量定义和必要 FRAME 长度，persistent 另外维护标准时间戳。Python 2 字节字符串、外部缓冲区、未知 opcode 或无法明确定位 roots/state 的结构不开放编辑。

## Ren’Py 签名

参考官方 [savetoken.py](https://github.com/renpy/renpy/blob/master/renpy/savetoken.py)、[persistent.py](https://github.com/renpy/renpy/blob/master/renpy/persistent.py)、[loadsave.py](https://github.com/renpy/renpy/blob/master/renpy/loadsave.py)、[存档根目录](https://github.com/renpy/renpy/blob/master/renpy.py)。当前标准实现是 NIST P-256 ECDSA、SHA-1、raw `r || s` 签名；DER 编码用于私钥/公钥，行内容使用 Base64。

只读查找 `%APPDATA%/RenPy/tokens/security_keys.txt`，兼容 `RENPY_PATH_TO_SAVES`、已关联槽位的上层存档根目录和可识别的 `Ren'Py Data`。区分 `signing-key` 和仅验证用的 `verifying-key`；仅使用能验证原签名且公钥与原签名一致的私钥。修改 `.save` 后签新 `log` 并更新 `signatures`；persistent 对解压的原始 pickle 签名，再 zlib 压缩并追加签名。写回前再次验证。

缺少私钥、无效私钥、其他曲线、原签名不匹配或无签名的旧版存档会明确提示并保持只读。不会为绕过验证删除签名、生成密钥、修改游戏信任设置，也不会复制真实私钥到 Portable `data/`。合成测试夹具中的固定私钥仅用于测试，不是真实用户密钥。

## 写入保护与边界

- 不检查游戏进程或要求关闭游戏。保存后回游戏重新读档；persistent 可能需要重启，正在运行的游戏也可能再次覆盖它。
- 前端只能提交游戏 ID、发现得到的槽位 ID、revision 和明确字段的值，不能指定任意写入路径。关联路径复用现有解析并拒绝链接/重解析点。
- revision 同时检查内容摘要、大小和修改时间；生成完整且验证通过的同目录临时文件后，提交前再检查源文件。Windows 使用同盘替换；文件占用或写入失败报错，清理临时文件，不生成持久备份。
- 此检查不锁定运行中的游戏。游戏若恰好在最后一次检查与替换之间更新，无法提供跨进程事务保证；游戏保存操作与编辑器保存应错开。
- 原文件最大 32 MiB，解压数据最大 64 MiB，最多 1024 槽位/256 ZIP 成员，单次最多修改 2000 个字段。超出范围明确拒绝，不部分写入。数据库 schema、更新/导入暂存目录、回收站和 Portable 数据路径均不变。

## 验证与真实游戏验收

自动化覆盖官方引擎生成的合成 MV/MZ 编码、未知 JsonEx 字段、数据库缺失/未赋值变量/新增道具；合成 pickle 协议 0–5、共享 memo、重复名称、复杂对象、正常与异常签名、persistent 时间戳；Unicode 路径、运行状态为 Playing、关联槽位约束、文件变化与 Windows 占用、其他槽位不变与无备份；前端批量保存、分类/搜索、未保存确认和错误保留。

开发阶段另用标准 pickle 与独立密码库核验**仅合成样本**的写回和签名；此验证工具不打包、不作为应用依赖。应用运行只需要现有 Tauri/WebView2 与 Rust 实现。

上述验证不代表真实游戏已成功读档。请在 Windows 依次验收：

1. 在 MV 和 MZ 各选一个已保存槽位，从游戏详情打开编辑器，改一个可辨认的数值/开关和道具数量，保存后在游戏内重新读同一槽位。
2. 在 Ren’Py 已保存且带签名的槽位修改一个简单 `store.*` 值，再读档；应正常加载且无需新增信任提示。缺少签名条件时应显示原因并禁止写入。
3. 在 Ren’Py Persistent 页修改一个公开简单字段，重启游戏后核对；确认游戏自定义逻辑是否会主动重新赋值或覆盖它。
4. 编辑未保存时切换槽位，检查放弃确认。再打开槽位并暂留修改，让游戏重新保存同槽位，编辑器保存应报“文件已变化”，且可刷新重做。

若读不到字段、格式报错或游戏拒绝加载，可提供对应 `.rpgsave` / `.rmmzsave` / `.save` / `persistent` 样本与错误文字；RPG Maker 附上上述辅助 JSON，Ren’Py 说明版本。**无需提供私钥文件**。提交样本前自行确认其中没有不想分享的信息。
