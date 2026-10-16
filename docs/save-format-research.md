# 存档修改格式扩展调研

> 调研后的实施决定：v1.1 只做 **ES3 与 Naninovel**。当前开发分支的支持与限制以 [编辑器说明](save-editor-lite.md)为准；下文保留立项前调研，其他格式未实现。

调研日期：**2026-10-16**。仓库基线：main `8c03e83`；当前正式版本 **1.0.0**。本文是 v1.1 的可行性与投入评估，未实现新适配器，未验证真实游戏存档，也不是兼容性或交付日期承诺。

## 结论与优先级

用户已明确 **Unity 游戏数量多，v1.1 应优先 Unity**。对照开源实现后，建议先验证 **Easy Save 3（ES3）** 与 **Naninovel `.nson`**，随后推进 **NRBF / BinaryFormatter**；普通 JSON 的保真字段能力作为共用底座。不是放弃 Unity，而是按保存方案逐步增加可验证的覆盖。具体项目、许可证及原型顺序见 [Unity 专项调研](unity-save-research.md)。

TyranoScript 文件存档、QSP 和 SugarCube 仍具可行性，但已调到 Unity 首批之后。用户游戏数量是优先级依据，不是已验证的格式分布；目前没有读取正式游戏库，不能给出 ES3 或 NRBF 的实际覆盖百分比。

现有「RPG Maker」编辑实际只覆盖 MV / MZ，XP / VX / VX Ace 与 2000 / 2003 都是新增格式工作。Unity / Godot 没有一个可据引擎名称直接开启的通用存档编辑器；应按具体文件格式和已验证配置分别支持。

复杂度为本项目的工程判断，不是上游给出的评级。以下人日只表示一名熟悉仓库的开发者完成**限定标准格式、现有 UI 接入和合成回归**的粗估，不包含等待 Windows 实机 / 真实游戏验收、未知加密格式研究或每款游戏脚本兼容。每项有共用工作，不宜相加成排期；原型后应重新估算。

| 候选 | 可先支持的范围 | 复杂度 / 粗估人日 | 主要成本与建议 |
| --- | --- | --- | --- |
| **Unity / ES3** | 明文 / GZip，随后为已知密码的标准 AES 容器；类型明确的已有叶子 | **中高；明文约 5–8，已知密码加密路径另增约 3–5** | 已有 MIT / GPL 参考实现和合成向量；密码、非标准 JSON 与类型保留仍需验证。Unity 首批，详见 [专项调研](unity-save-research.md)。 |
| **Unity / Naninovel** | 明确版本的 raw DEFLATE + JSON `.nson`，已有简单自定义变量 | **中，4–8** | 可复用已有 flate2，但须区分局部 / 全局状态及嵌套序列化字段。Unity 首批候选，详见 [专项调研](unity-save-research.md)。 |
| **Unity / NRBF** | 有明确类型的原始 BinaryFormatter 记录；不加载游戏 DLL | **高，约 12–25** | 存在 MIT JS reader / writer 和 Rust reader；Rust 写回的记录类型、对象引用及未知内容保留仍是主要工作。排在 ES3 / Naninovel 后。 |
| **TyranoScript / TyranoBuilder** | `configSave=file` 的标准 `.sav`；已定位的槽位 `stat.f` 简单变量 | **中，5–9** | 文件模式使用 JSON 加 `escape` 编码；一个文件可含多个槽位，须保留场景 / 图层 / 截图等状态。排在 Unity 首批后。[T1][T2] |
| **QSP** | 明确版本的标准 `.sav`；已有数值 / 文本变量及数组元素 | **中，7–12** | 需要版本化解码、字符编码、变量 / 数组键结构及游戏 CRC 保留；不能把所有 `.sav` 当成同一格式。排在 Unity 首批后。[Q1][Q2] |
| **Twine / SugarCube 2** | 游戏自行导出的单份文件存档；当前历史节点中的简单故事变量 | **中高，8–15** | 复用现有 LZString 基础，但还要处理历史差分、活动索引、故事 ID 和特殊序列化标记。不是整个 Twine 平台的统一格式。[S1][S2] |
| **RPG Maker XP / VX / VX Ace** | 标准 `.rxdata` / `.rvdata` / `.rvdata2` 中的变量、开关等 | **高，15–25** | Ruby Marshal 对象图、符号 / 对象引用、字符串编码、Table 等 RGSS 类型和脚本定制。首个具体版本应单独立项；现有 MV/MZ 的 JSON 编解码器不能直接复用。[R1][R2] |
| **RPG Maker 2000 / 2003** | 标准 `Save*.lsd` 的变量 / 开关及已确认基础字段 | **中高，10–18** | 有 LCF 结构资料和 liblcf 读写实现，仍需保持未知块、默认字段、文本编码及扩展差异；不能把读取成功等同于保真写回。[L1][L2] |
| **Unity / Godot 的已确认文本格式** | 特定游戏配置下的未加密 JSON / INI 等，且有明确可写字段 | **单一配置低至中，4–8；通用支持无法这样估算** | 识别 JSON 容易，确认它是存档、类型 / 数值约束和游戏身份更难。Godot ConfigFile 也不等于普通 INI；需保留 Variant 文本语法。只作为具体配置候选。[U1][G1] |
| **Godot Variant 二进制** | 明确引擎版本、无对象构造的标量 / 数组 / 字典子集 | **高，12–20 / 限定版本** | 类型编码、对齐、精度、Typed / Packed 容器及版本差异；官方二进制说明也有版本未更新提示。需对照目标版本源码，不按一张文档表支持所有 Godot。[G2] |
| **KiriKiri / KAG** | 已确认标准 KAG 的数据文件及普通变量 | **高；纯文本子集约 12–20，其他模式另估** | TJS 结构不是 JSON；`saveStruct` 有文本、简易编码、压缩、二进制模式，存档还可能附于 BMP。先验证封装与受限语法，不运行 TJS、不将 BMP 当普通图片重写。[K1][K2] |
| **WOLF RPG Editor** | 先限定引擎代际与封装，能确认解码及完整写回的格式才进入候选 | **很高，暂不能可靠估期** | 官方存在不同加密选项，且部分选项会改变存档加密；还涉及可变数据库结构。不能把旧工具支持个别版本推广成所有 WOLF。建议暂缓，先做版本化可行性验证。[W1][W2] |

## 其他候选需要解决什么

Unity 首批的开源实现比较、具体边界与阶段顺序已单列到 [Unity 专项调研](unity-save-research.md)，以下保留其他格式的可行性分析。

### TyranoScript：优先独立文件，浏览器存储不在首期

上游 `libs.js` 的文件存储分支将 JSON 编码后写为 `<key>.sav`；`kag.menu.js` 按项目 ID 管理普通、快速及自动存档，普通文件内可以包含多个槽位。因而需要把**文件**与**逻辑槽位**分开：编辑一个槽位时，必须保留同一文件的其他槽位以及运行状态元数据。[T1][T2]

首个原型只开放已经存在的 `stat.f` 中数值、布尔、文本字段。系统变量文件、图层 DOM、脚本位置和插件状态先保持只读；不把整个 `.sav` 当作可任意增删键的 JSON。旧版与新版存档结构需分别生成夹具，编辑器不照搬上游加载时的自动转换或备份行为。百分号转义还涉及 UTF-16 码元，不能直接用普通 UTF-8 URL 编码替代。

`webstorage` / 压缩浏览器存储模式与文件模式是不同边界。首期不读取或写入 Chromium 的 Local Storage / IndexedDB，不为此扩大现有路径扫描；只有真实存在、用户明确关联且格式可确认的文件才进入编辑器。

### QSP：通用变量编辑，而非按游戏硬编码金钱 / 道具

上游保存记录包含格式标识、引擎版本、游戏 CRC、运行状态和全局变量；变量还包含多值与索引键。加载检查版本及游戏身份，相关状态不能在重编码时丢失。[Q1] 官方也允许脚本自行指定存档路径。[Q2]

首期限定经过原型验证的版本，显示变量名及已有索引，只改明确的数值 / 文本叶子。不改游戏 CRC，不删除动作、对象、位置、包含文件或数组索引结构；不执行保存内的脚本或游戏加载钩子。超出当前前端安全数值精度的整数先只读，复杂元组 / 新类型另行评估。现有 QSP 启动支持只减少路径与关联成本，并没有现成的存档编解码器。

### SugarCube：压缩层容易，历史状态保持较难

上游导出路径使用 LZString Base64 和特殊 JSON 序列化；保存状态可能是历史差分，也可禁用差分，存在当前活动历史索引与故事 ID。应编辑**当前活动节点**，保留其余历史、身份及用户未选中的字段，不能固定修改第一项或仅搜索某个 `variables` 键。[S1][S2]

还存在旧版 / 新版保存 API、单存档 / 批量导出包与 Map / Set / 日期 / 自定义恢复标记的区别。首期只处理确定版本的单存档文件；对 `(revive:)` 等数据仅做静态解析和保留，不调用 `Serial.parse` 内可能执行表达式的恢复逻辑。Twine 的其他 story format 不自动继承 SugarCube 支持。

## 通用引擎与对象序列化的边界

- Unity 的 Windows PlayerPrefs 使用注册表，游戏也可自行用 JSON、第三方库或自定义二进制保存；现有路径识别不意味着知道文件内容结构。[U1] Easy Save 3 又可开启加密和压缩，应作为单独格式候选，未加密结构可先验证；未知密码或自定义封装不在首期范围。[U2]
- Godot 官方示例既有逐行 JSON，也提供 Variant 二进制序列化；文件可能同样叫 `.save`，不能套用 Ren’Py 的 ZIP/Pickle 解析。即便文件是纯 JSON，也不能从字段名证明修改后一定符合游戏逻辑。[G1][G2]
- Ruby Marshal 与 Ren’Py Pickle 同属对象序列化问题，但格式和类型系统完全不同。需要新的保真解析 / 写回实现，不能调用 Ruby `Marshal.load` 去实例化未知类，更不能运行游戏脚本来“补齐”自定义类型。[R1]
- KAG 与 WOLF 都有引擎标准之外的版本 / 封装差异。可读出几个字符串或数值不足以证明能修改、重建并被原游戏接受。[K1][K2][W1]

这些限制用于确定首期边界，不代表以后不能做某个明确版本或具体游戏配置。

## v1.1 建议实施顺序与通过条件

1. **明确一个目标格式和版本。** 优先完成 Unity ES3 / Naninovel 的独立只读探针、无修改保留及字段修改往返；NRBF 紧随其后评估。用公开文档 / 源码及自己的最小合成项目验证，不能以别人的网页编辑器能打开为依据。
2. **补最少的适配接口。** 在现有 Rust 编辑模块中按引擎证据、文件头、版本及结构选编解码器；不要仅靠 `.sav` / `.save` / `.dat` 后缀，更不能将文件解析失败后的内容当另一种格式强行写回。一个文件多个槽位时，后端标识绑定文件及槽位索引，revision 覆盖整个所属文件。
3. **只开放可证实的简单字段。** 复用当前搜索、字段类型、草稿和关闭确认，不为新引擎新增独立大页面。未知节点原样保留；重复键、精度溢出、共享引用或无法保真的状态拒绝修改，不自动修复或迁移。
4. **覆盖写回与失败边界。** 无修改的应用路径直接保留原字节；编解码往返检查语义和结构，加密重编码使用新 IV 时不要求密文逐字节相同。目标字段修改后以独立解析 / 原引擎的合成项目验证；确认其他字段 / 槽位不变、误判和损坏输入拒绝写入、大小受限、过期 revision 和写入失败保留原文件。复用现有同目录临时替换、任务互斥、路径及身份保护；不创建持久备份，不改变既有游戏进程检查策略。
5. **完成双平台检查与本机验收。** 云端使用 `bash scripts/Check-Cloud.sh`，Windows CI 保留完整原生回归、所有桌面目标严格 Clippy 与 EXE。用户在本机验证实际读档、字段效果与游戏重新保存，不把“能解析 / 能写回”当作真实游戏验收成功；不上传、复制或导出正式资料。

原「手动重新检测存档路径」已取消，PR #2 已关闭未合并。本次只更改计划与调研说明，保留现有手动全库扫描补全空配置的行为，不新增开机扫描或自动替换。

## 核对来源

以下为直接查阅的上游文档 / 源码。GitHub 主要证据固定到当日提交；`master` / `develop` 用于理解格式，**不代表首期承诺支持所有已发行版本**。如采用上游库或代码，还需另行核对许可证、依赖和 Windows 构建成本；本次未将外部实现加入应用。

- **[T1]** [TyranoScript libs.js：存储分支与文件读写](https://github.com/ShikemokuMK/tyranoscript/blob/c8dbfd492afd3d79b0954fcf4477236f5c6c4830/tyrano/libs.js)。核对 `setStorage`、`setStorageFile`、`getStorageFile`。
- **[T2]** [TyranoScript kag.menu.js：槽位与运行状态](https://github.com/ShikemokuMK/tyranoscript/blob/c8dbfd492afd3d79b0954fcf4477236f5c6c4830/tyrano/plugins/kag/kag.menu.js)。核对 `getSaveData`、普通 / 自动 / 快速保存及 `stat.f`。
- **[Q1]** [QSP game.c：保存结构、版本及游戏 CRC 检查](https://github.com/QSPFoundation/qsp/blob/54f19bd1dffa16b2b69bb12dda362697c5c5baef/qsp/game.c)；[coding.c：值和字符编码](https://github.com/QSPFoundation/qsp/blob/54f19bd1dffa16b2b69bb12dda362697c5c5baef/qsp/coding.c)。
- **[Q2]** [QSP 官方语句参考：SAVEGAME / OPENGAME](https://dev.qsp.org/docs/language/qsp-keywords/qsp-keywords-statements/)。
- **[S1]** [SugarCube 保存实现](https://github.com/tmedwards/sugarcube-2/blob/ba17b12343728d191b5cc6a82e4dc5713379863d/src/save.js)；[状态及历史差分](https://github.com/tmedwards/sugarcube-2/blob/ba17b12343728d191b5cc6a82e4dc5713379863d/src/state.js)；[特殊序列化](https://github.com/tmedwards/sugarcube-2/blob/ba17b12343728d191b5cc6a82e4dc5713379863d/src/lib/serial.js)。
- **[S2]** [SugarCube 2 官方 API 文档](https://www.motoslave.net/sugarcube/2/docs/)，核对 disk / base64 / browser 保存 API 的区别及旧 API 弃用记录。
- **[R1]** [Ruby 官方 Marshal 格式](https://docs.ruby-lang.org/en/2.7.0/marshal_rdoc.html)及 [Marshal 对象 / 自定义序列化说明](https://docs.ruby-lang.org/en/3.3/Marshal.html)。格式版本与 Ruby 版本是不同概念；真实 RGSS 子集仍须专门验证。
- **[R2]** [mkxp-z 的 RGSS 兼容文件接口实现](https://github.com/mkxp-z/mkxp-z/blob/dev/binding/filesystem-binding.cpp)，核对 `load_data` / `save_data` 及 Marshal 调用；这是兼容运行时的上游实现，不是 RPG Maker 官方对第三方脚本的兼容保证。
- **[L1]** [EasyRPG LcfSaveData 结构参考](https://wiki.easyrpg.org/development/data-structure-reference/lcfsavedata)。
- **[L2]** [liblcf 项目与读写依赖](https://github.com/EasyRPG/liblcf/blob/6854310c3432e553fd4ae672ce861899c80c3bd0/README.md)。采用库与自行实现有限补丁的投入需分别评估。
- **[U1]** [Unity 6 PlayerPrefs 官方说明](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PlayerPrefs.html)。
- **[U2]** [Easy Save 3 官方加密与压缩说明](https://docs.moodkie.com/easy-save-3/es3-guides/es3-encryption-compression/)。
- **[G1]** [Godot 官方保存游戏示例](https://docs.godotengine.org/en/stable/tutorials/io/saving_games.html)。
- **[G2]** [Godot Variant 二进制说明](https://docs.godotengine.org/en/stable/tutorials/io/binary_serialization_api.html)及 [FileAccess API](https://docs.godotengine.org/en/4.6/classes/class_fileaccess.html)。stable 二进制说明的版本过期提示本身也是待核验项。
- **[K1]** [KAG 官方栞 / 存档说明](https://krkrz.github.io/krkr2doc/kag3doc/contents/SaveData.html)；[KAG3 保存实现](https://github.com/krkrz/kag3/blob/1f3ab309106d210e3169bbbe0fb4e066ae463b42/data/system/MainWindow.tjs)。
- **[K2]** [Dictionary.saveStruct 官方模式说明](https://krkrz.github.io/multi_platform_design/apiref/func_Dictionary_saveStruct.html)。
- **[W1]** [WOLF 官方游戏数据制作与加密说明](https://silversecond.com/WolfRPGEditor/Help/02gamemake.html)，核对加密选项对存档加密的影响。
- **[W2]** [WOLF 官方 FAQ](https://silversecond.com/WolfRPGEditor/Help/01FAQ.html)，核对存档与可变数据库结构及版本差异。
