# Unity 存档编辑：开源实现与 v1.1 优先方向

> 调研后的实施决定：v1.1 只做 **ES3 与 Naninovel**。1.1.0 的支持与限制以 [编辑器说明](save-editor-lite.md)为准；下文保留立项前调研，其他格式未实现。

调研日期：**2026-10-16**。配套 [总体格式调研](save-format-research.md)与 [v1.1 路线图](roadmap.md)。用户已明确 Unity 游戏数量多，因此按实际需求提高 Unity 优先级；本文没有读取正式游戏库或存档，没有推算格式占比，也没有实现适配器。

## 建议：Unity 优先，按保存方案交付

1. **首批原型：ES3 与 Naninovel。** ES3 有独立编辑器、公开容器实现和合成兼容向量；Naninovel `.nson` 有简洁的 raw DEFLATE 编解码参考。先验证这两类，再以一种格式的可靠读写和本机读档完成首个交付闭环。
2. **第二批：NRBF / BinaryFormatter。** 存在不加载游戏 DLL 的 reader / writer 实现，已经不是只能依赖游戏反射的方向；但类型和对象引用的保真写回仍明显复杂。
3. **JSON 是底座，不是“所有 Unity 存档通用”。** 首批支持能确认结构的文件、数值 / 布尔 / 文本叶子；XML、MessagePack、Odin 等按后续明确需求评估。
4. **Windows PlayerPrefs 单独评估。** 注册表和文件是两种存储后端；不能把支持 macOS plist / Android XML 的项目当作 Windows PlayerPrefs 编辑器。首期文件适配不写注册表。

当前顺序为 **Unity ES3 / Naninovel → NRBF → 其他引擎候选**。具体格式边界用于拆分适配工作，优先级按用户的 Unity 使用需求确定。

## 实际查阅的项目

本轮查看了 13 个公开仓库的目录、有关核心源码及许可信息，未运行它们的 GUI、未下载所附游戏 DLL / 存档或安装外部编辑器。以下区分“可参考实现”与“能直接作为当前应用依赖”：

| 项目与核对位置 | 实际能力 / 许可证据 | 对 butter-manager 的价值与限制 |
| --- | --- | --- |
| [Yoruxyv/RepoDitor](https://github.com/Yoruxyv/RepoDitor/tree/2d47a22b1d0bb6f58f3dbc2160596492aa34ee25)，`core/crypto.py`、Web `es3.ts`、`compatibility/es3` | 根 LICENSE 为 **MIT**；Python / Web 版本及合成固定向量。实现 R.E.P.O. 对应的 ES3 AES 容器 | **优先参考。** 可参考兼容参数、双实现对照与合成向量方法；其游戏密码和字段 schema 不能直接变成所有 ES3 游戏的默认规则 |
| [crtdll/es3-modifier](https://github.com/crtdll/es3-modifier/tree/44f4b79672205333c196fa5f392609d565226dd1)，`es3_modifier/main.py` | 根 LICENSE 为 **GPL-3.0**；参数化密码的 AES 读写 | 证明容器可由独立工具实现；不能当 MIT 代码无条件并入。`beautify` 会改写 `__type` 表示，不应照搬进保真保存路径 |
| [Eduwilll/ES3-save-editor](https://github.com/Eduwilll/ES3-save-editor/tree/b49d9f1ad8aa4bef10ce63e824585e88aac2bd17)，`main.py` | 桌面 UI、明文 / 已知密码、GZip、类型和原始数据块；当前树未找到独立 LICENSE，依赖 `es3-modifier` | 可参考“简单叶子可改、复杂块保留”交互和封装顺序。不能只根据项目说明把整套实现视为宽松许可依赖；它的 `.bak`、记住密码和外部服务行为也不是本项目要照搬的流程 |
| [fushi/ms-nrbf-js](https://github.com/fushi/ms-nrbf-js/tree/1876b1fcf9ddb000d671548e9e566f157683f381)，`reader.ts`、`writer.ts`、测试 | 根 LICENSE 为 **MIT**；JS / TS reader + writer，记录类型、成员原始数值类型、类 / 库名和引用；npm 0.1.0 也声明 MIT | **NRBF 的重要参考。** 可作为合成夹具与独立往返的对照工具；不是原生 Rust 写回的现成替代，仍须核验异常输入和未覆盖记录 |
| [reitermarkus/nrbf-rs](https://github.com/reitermarkus/nrbf-rs/tree/9cf684c166c72248a38b9d428db03f957f94fd78)，`Cargo.toml`、`lib.rs` | Cargo 声明 **MIT OR Apache-2.0**，0.2.2；公开 API 以解析 / `from_slice` 为主 | 可评估原生 Rust 读取和类型检查；本次未发现可直接完成保真 NRBF 二进制写回的公开 writer，不能将“支持 serde”误当成 NRBF 写出 |
| [driedpampas/nrbf-parser](https://github.com/driedpampas/nrbf-parser/tree/df49dafdff55ea4813e6595d9b22e8128bed41d3)，`Cargo.toml`、`encoder.rs` | Cargo 与源文件声明 **GPL-3.0-or-later**，0.1.1；有 Rust reader / encoder | Rust 方向确实有 writer，但需另行决定是否接受许可条件，以及验证完整对象图 / 未知记录；不因有 encoder 就视为生产可用的通用适配 |
| [paradoxie/saveeditor](https://github.com/paradoxie/saveeditor/tree/67c47f217b4e683408fda17adf627dfdbdaed783)，`unity.ts`、`nrbf.ts`、`naninovel.ts` 和 parser 测试 | NRBF 模块调用 `ms-nrbf-js`，有类型 / 结构复核及标量编辑；Unity 模块主要是 XML / plist；有 Naninovel 多封装探测。README 标注 MIT，但当前树未找到 README 链接所指的根 LICENSE | 可参考后端格式分层、可写能力标记与测试案例；**不将其“Unity 支持”理解为 Windows 注册表或所有 Unity 二进制支持**。直接复制前先确认授权文本，NRBF 库本身的 MIT 许可可单独核对 |
| [p-toy-factory/nson](https://github.com/p-toy-factory/nson/tree/5efc88cc55bb837feacaa2ada37c8b4706951454) 与 [TCRoid/nson-save-editor](https://github.com/TCRoid/nson-save-editor/tree/2fd3d856fcdebfb2f779a40afa2890a54fafe361) | 前者 CLI 使用 Node `createInflateRaw` / `createDeflateRaw`，后者网页使用 fflate inflate / deflate；两棵树本次均未找到 LICENSE | 提供 `.nson` 封装的独立参考；参考格式并用现有 Rust flate2 自行实现，不把缺少明确许可的网页代码直接打包进应用 |
| [dshook/UnitySaveEditor](https://github.com/dshook/UnitySaveEditor/tree/899a7f995499bccdfb6b7405d6e9055c3e690f20) 与 [sabresaurus/PlayerPrefsEditor](https://github.com/sabresaurus/PlayerPrefsEditor/tree/710e074d16ed1c7f2fefa7f8e7912a05ecf25969) | 均有 MIT 文本；是 Unity Editor 工作流 / 窗口 | 可参考树形编辑和类型显示，不能要求使用者安装 Unity Editor 来满足单 EXE 使用流程 |
| [ret-0/unity-save-editor](https://github.com/ret-0/unity-save-editor/tree/099dd024305412a6c6637e824c2876dc2c1cb2f2)，`decoder/Program.cs` | 需要 `Assembly-CSharp.dll`，调用 `Assembly.LoadFrom` 与 `BinaryFormatter.Deserialize`；当前树未找到 LICENSE | 不采用加载游戏程序集 / 执行反序列化对象的方式；NRBF 按数据记录静态解析更符合当前受控编辑器设计 |
| [TeamSirenix/odin-serializer](https://github.com/TeamSirenix/odin-serializer/tree/b3c42223a09cd48160509d3b8202764b010351b1) | 根 LICENSE 为 **Apache-2.0**；完整 C# 序列化库，涉及类型、格式及 Unity 对象引用 | 有公开源码不等于低成本的外部存档编辑器。接入库或重写有限解析均需单独估算；不作为 v1.1 首批必做项 |

许可结论以查阅版本的文件为据，不仅依赖 GitHub 自动标签。MIT / Apache 的具体复用仍要保留所需声明；GPL 方案有对应分发条件。缺少授权文本的项目保留为格式 / 行为参考，当前不直接引入其代码。Easy Save 商业插件本体与第三方独立开源工具也不是同一份授权，不复制第三方游戏仓库附带的商业插件源码。

## ES3：第一优先，但要分容器和内容两层

参考 [RepoDitor 的原生实现](https://github.com/Yoruxyv/RepoDitor/blob/2d47a22b1d0bb6f58f3dbc2160596492aa34ee25/desktop/python/repo_save_editor/core/crypto.py)、[Web 实现](https://github.com/Yoruxyv/RepoDitor/blob/2d47a22b1d0bb6f58f3dbc2160596492aa34ee25/web/src/features/save-file/es3.ts)和 [合成向量说明](https://github.com/Yoruxyv/RepoDitor/blob/2d47a22b1d0bb6f58f3dbc2160596492aa34ee25/compatibility/es3/README.md)，有证据的容器使用前置 IV、PBKDF2-HMAC-SHA1 派生、AES-CBC 与 PKCS#7；这是特定兼容合同，不是所有 Unity 或所有 ES3 配置的定律。具体参数应写入版本化的合成兼容测试，不能自行改成另一种加密模式。

[Easy Save 官方文档](https://docs.moodkie.com/easy-save-3/es3-guides/es3-encryption-compression/)允许配置加密及压缩。[ES3-save-editor 的打开 / 保存路径](https://github.com/Eduwilll/ES3-save-editor/blob/b49d9f1ad8aa4bef10ce63e824585e88aac2bd17/main.py)对其支持的组合采用解密后 GZip 解压，写回反向处理。应逐项验证明文、GZip、AES、AES + GZip，不把“能解密”当成能够保持全部 ES3 内容。

内容层必须保持 `__type`、`value`、引用 ID 和未知块。ES3 可能含不能直接交给普通 JSON 解析器重建的字典 / 复杂类型。首期开放已确认的标量叶子，普通 JSON 子集先行；非标准数据如需支持，应保留原始片段并做受限补丁，不用全局正则“修成 JSON”，不美化或缩写类型名称后再保存。整个文件无法保真时明确拒绝写入。

加密并不是天然无法做，但需要正确密码 / 已核验的游戏配置。一个开源工具内的游戏专用密码不能作为全 Unity 默认密码。首期可支持已知密码的会话内输入 / 经核验配置，不自动扫描程序集猜密码、不批量尝试、不默认记住密码或接入外部密码库。CBC 解密和 padding 成功也不足以确认文件归属，仍要验证内容结构及目标游戏关联。

ES3 编解码建议留在 Rust 后端，继续复用文件发现、字段选择和保存保护；AES / CBC / PBKDF2 可评估成熟 [RustCrypto](https://github.com/RustCrypto/block-ciphers)组件，GZip 已有 flate2。JS / Python 实现优先作为**开发时的独立合成对照**，不在应用运行时增加 Python、.NET 或外部编辑器。

## Naninovel：适合 Unity 视觉小说的第二条首批路径

[Naninovel 官方状态管理文档](https://naninovel.com/guide/state-management)说明游戏状态、全局状态及设置分开管理，内容基于 JSON，文件可为 `.nson` / `.json`，也允许自定义序列化处理器。公开 `nson` CLI 的 raw DEFLATE 处理与另一个网页实现一致，因此应优先对这一明确封装做原型，而不是把它视为未知 Unity 二进制。

可以复用现有 flate2，不需要引入 Naninovel 商业 SDK。首期仅修改明确定位的已有简单自定义变量，保留脚本位置、演员 / 场景状态、缩略图和未知服务字段。服务状态还可能包含嵌套的 JSON 字符串，不能只把最外层对象拍平。

不能根据某个开源解析器“尝试 raw DEFLATE / zlib / GZip / Base64 都成功”的启发式就承诺这些都是官方标准版本。原型须记录实际封装，核对完整压缩流与结构；自定义加密或 handler 另列不支持，拒绝把随机二进制解码失败一概标成“加密”。普通槽位与全局状态分别验收，不做自动全解锁或多文件联动写回。

## NRBF：有不加载 DLL 的路线，读取和写回分开验收

`ms-nrbf-js` 是本次重要发现：有 MIT 的 reader / writer 及成员原始类型保留，不需要加载游戏程序集。[参考编辑器的 NRBF 适配](https://github.com/paradoxie/saveeditor/blob/67c47f217b4e683408fda17adf627dfdbdaed783/src/lib/parsers/nrbf.ts)将结构投影为可编辑字段，然后对原记录树应用有限修改，序列化后复核字段及结构；这比“二进制转普通 JSON 再随便重建”更值得参考。

仍需验证 Int32 / Int64、Single / Double、类与程序集元信息、对象 ID、数组、重复 / 循环引用、未知记录与文件外层封装。首期只写明确类型且不影响共享对象的标量叶子，不能顺便重命名类、删除成员或创建任意对象。

[Microsoft 的 NrbfDecoder 文档](https://learn.microsoft.com/en-us/dotnet/standard/serialization/binaryformatter-migration-guide/read-nrbf-payloads)证实可以只解析记录而不加载类型；它不是保真 writer。Rust `nrbf` 同样优先用于读取；Rust `nrbf-parser` 虽有 encoder，但许可与保真覆盖需另评估。可比较有限记录补丁和新的类型保留 writer，不能将“接入一个 parser”估成完整 NRBF 支持。

`ms-nrbf-js` 可作为开发期独立 reader / writer 对照；最终文件写回授权及结构检查仍应由后端掌握。当前不决定将 JS 编解码搬到前端或引入嵌入式 JS / .NET 运行时。

## PlayerPrefs、Odin 和其他格式的边界

- [Windows PlayerPrefs](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PlayerPrefs.html)涉及注册表。项目中的“支持 PlayerPrefs XML / plist”通常不是在操作 Windows 注册表；而 Unity Editor 插件掌握开发项目的 company / product 资料，也不等于能够自动识别所有已安装游戏的键。后续若做，需单独设计明确游戏键范围、类型和并发检查，不把它塞进现有文件路径接口。
- Odin 有 Apache-2.0 源码，但对象解析及 Unity 引用并非简单 JSON；MessagePack / MemoryPack / Protobuf 等也不共享 schema。可保留明确方案，不能通过放宽 JSON / 字符串扫描覆盖它们。
- 没有依据给出“覆盖多少百分比 Unity 游戏”的数字。首期按实际验证的**格式 / 版本 / 配置**公布支持列表，真实覆盖由用户本机使用逐步确认，不上传正式库或游戏程序集来做云端分类。

## 投入评估与原型顺序

以下为基于当前代码复用程度的工程粗估，**不是开源项目的性能 / 兼容性证明，也不是承诺工期**。人日包含有限格式的后端、现有 UI 接入及合成回归，不包含等待 Windows 实机验收；共用工作不可重复相加。

| 阶段 | 交付范围 | 复杂度 / 粗估 |
| --- | --- | --- |
| Unity 文件底座 | 有界候选识别、格式状态、保真标量补丁，复用既有槽位 / revision / 写入保护 | 中，约 3–5 人日；与后续格式工作重叠 |
| ES3 明文 / GZip | 确认的 typed-entry 子集，已有简单值修改；复杂内容保留或拒绝 | 中，约 5–8 人日 |
| ES3 已知密码 AES | 对有证据的容器加入解密 / 再加密，密码错误与损坏边界、独立向量检查 | 在 ES3 基础上约增 3–5 人日；未知密码不计入可交付范围 |
| Naninovel `.nson` | raw DEFLATE + JSON，明确版本的简单变量，保留服务状态及所属槽位 | 中，约 4–8 人日 |
| NRBF | 静态记录解析、明确类型标量修改、结构及引用保留、独立读写核对 | 高，约 12–25 人日，原型后再估 |
| PlayerPrefs / Odin / 未知自定义格式 | 各自独立评估 | 当前不承诺排期 |

建议 ES3 和 Naninovel 各先做一个**只读 / 合成原型**，验证识别与封装、选中字段修改和独立解码，再确定先交付哪一个；不必等所有 Unity 方案一起完成。NRBF 排在后续，不因其困难阻塞 ES3 / Naninovel。

统一验收条件：无修改的应用路径直接保留原文件字节；修改后只改变选中字段，保留其他槽位、类型与未知内容，原封装可被独立解析。加密新编码使用新 IV 时比较解密后的语义 / 结构，不要求密文完全相同。缺少密码、类型含糊、文件变化、截断、超限和写入失败均不覆盖原文件；不生成持久备份。继续运行完整 Linux / Windows 回归，由用户在 Windows 完成真实读档验收。

以上为立项前调研记录。随后选定 ES3 / Naninovel 进入 v1.1 开发，当前实现、限制和检查结果分别见编辑器说明、路线图与验证记录。
