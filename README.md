<p align="center">
  <img src="src/assets/app-icon-new.png" width="96" height="96" alt="butter-manager 应用图标" />
</p>

<h1 align="center">butter-manager</h1>

<p align="center">本地游戏，一处整理。</p>

<p align="center">
  <img src="https://img.shields.io/github/v/release/Zhehao-w/butter-manager?label=release&amp;color=ed91b2" alt="最新正式版本" />
  <a href="https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml"><img src="https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml/badge.svg?branch=main" alt="Linux / Windows CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Zhehao-w/butter-manager?color=5b8def" alt="开源许可" /></a>
</p>

<p align="center">
  <a href="https://github.com/Zhehao-w/butter-manager/releases/latest"><strong>下载 Windows 版</strong></a> ·
  <a href="#文档">使用文档</a> ·
  <a href="https://github.com/Zhehao-w/butter-manager/issues">反馈问题</a>
</p>

轻量的 **Windows x64 本地游戏库与启动器**。扫描已有游戏，搜索并启动；导入新版时处理版本与存档；在同一个地方管理重复游戏、游玩进度和历史记录。浅蓝主界面配合柔和粉色点缀，资料保存在程序旁的 `data/`。

## 功能

| 功能           | 可以做什么                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------ |
| 游戏库         | 列表 / 卡片视图，名称、别名和路径搜索，按游玩状态、引擎与启动方式组合筛选                  |
| 收藏与标签     | 手动收藏、自定义标签与组合筛选；批量修改游玩状态、收藏 / 取消收藏                          |
| 启动与配置     | 直接 EXE、Windows 默认应用、共享 MTool（普通注入 / Bakin）、本地 QSP；可保存 EXE 参数           |
| 导入与更新     | 批量导入、关联已有游戏，预览名称、目录和存档处理，保留游戏标识及历史；可记住人工确认的关联 |
| 恢复与回退     | 中断任务继续 / 撤回，从仍在回收站中的原旧版本回退，并带回当前内部存档                      |
| 查重与工具清理 | 查看重复推荐，确认 / 排除 / 撤销；导入后回收已识别的随包 MTool，切换公共工具               |
| 存档编辑       | 编辑受支持的 RPG Maker MV / MZ、Ren’Py、ES3 与 Naninovel 存档，支持字段和值搜索            |
| 路径与资料管理 | 目录重命名、整库路径重新关联、Ren’Py AppData 专属存档目录手动回收                          |

[直接 EXE 启动参数](docs/launch-arguments.md)支持多参数和双引号分组；MTool / QSP 模式隐藏参数，切回直接 EXE 时保留原值。分析结果可直接选用发现的文件，高级选项位于末尾；Import 顶部标记生效的启动参数。标准名称用于识别与关联，目录名在「记录与管理 → 重命名目录」修改。

[MTool Bakin 专用启动](docs/mtool-bakin.md)按游戏保存策略，使用公共 `BakinLauncher.exe`，不更改全局 injector。已有 Bakin + MTool 游戏可完成一次手动目录扫描，补齐引擎识别及专用启动策略。

[外置 MTool 翻译包推荐](docs/mtool-external-translation.md)结合明确使用说明、翻译文件名或引用及字典结构，在 Import 默认推荐公共工具，官网链接可选；翻译材料保留，字典由用户自行挂载。

[详情状态自动保存与快捷筛选样式](docs/library-organization.md#详情状态与快捷筛选)随 1.5.2 交付：游玩状态即改即存，其他草稿保留；收藏与正在玩采用不同配色，快捷筛选交互统一。

## 开始使用

**当前版本 1.5.2**：详情游玩状态独立自动保存，统一收藏、游玩状态和引擎快捷筛选交互。正式下载以 Release 页面为准。

1. **下载程序**：通过页首下载入口获取 `butter-manager-v1.5.2-windows-x64-portable.zip`，解压到可写的专用目录。
2. **配置游戏库**：运行 `butter-manager.exe`，在“设置”中选择游戏库根目录并保存。首次启动自动创建旁边的 `data/`。
3. **登记已有游戏**：点击“扫描目录”，选择需要入库的游戏。扫描登记保持游戏原位；完整扫描也会按已保存配置补齐已有游戏的空存档路径，不覆盖已有路径。新下载的游戏从“导入”添加。
4. **开始游玩**：从游戏库启动，或在详情中调整资料与启动配置。“正在玩”随成功提交的启动请求更新，手动设置的“已通关”保留。

Windows 需要 [WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)。发布页提供 Portable ZIP 和 `butter-manager-v1.5.2-windows-x64-portable.zip.sha256`；独立 EXE 仅保留在 CI artifact 中。未签名程序可能出现 SmartScreen 提示，请核对来源与校验值。

```text
butter-manager/
├── butter-manager.exe
└── data/                 # 游戏库、设置、恢复记录与 WebView2 用户资料
```

**升级**：关闭管理器，从新版 ZIP 提取 EXE 并替换旧 EXE，保留整个现有 `data/`；不要用下载目录中新建的空资料覆盖原库。“设置 → 关于应用”可查看应用版本与构建提交。**1.5.2 沿用 schema 15，从 1.5.0 / 1.5.1 升级无需迁移；从 1.4.0 及更早版本升级时自动迁移。已使用 schema 15 的开发构建无需再次迁移；1.4.0 及更早 EXE 无法打开升级后的库，请勿直接降级。**

**迁移**：先完成或撤回文件任务，关闭程序，再复制整个程序文件夹和游戏目录。路径变化后，在“设置 → 高级维护”预览并确认新旧根目录映射；此功能重新关联记录，不搬动文件。

## 存档与文件操作

游戏详情中的粉色 **编辑存档** 入口支持以下范围：

| 格式                          | 可编辑内容                                                                 |
| ----------------------------- | -------------------------------------------------------------------------- |
| RPG Maker MV / MZ             | 标准存档中的简单变量、开关、金钱、道具 / 武器 / 防具数量；角色信息只读     |
| Ren’Py `.save` / `persistent` | 受支持的简单字段、已知字典 / 列表 / 元组子字段，以及明确类型的 None 初始化 |
| Ren’Py 标准 `RevertableSet`   | 受支持结构中的字符串成员查看、增加与删除                                   |
| Unity ES3 `.es3`              | 明文 / GZip 与标准 AES（需已知密码）的已有简单数值、布尔、文本；保留原类型 |
| Naninovel `.nson` / `.json`   | 已识别状态结构中的标准本地 / 全局自定义变量，保留原始封装                  |

ES3 的复杂对象、数组、引用与未知类型保持只读；加密存档需要已知密码，密码仅在当前编辑会话中保留。Naninovel 只编辑标准自定义变量，保留游戏自定义服务、脚本位置、场景和回滚历史；普通 JSON 文件不作为通用编辑入口。

**每次只保存选中的槽位文件。** `GameSave001` 和 `GameSave002` 无需同步修改；`Global` 是独立全局存档，其中的变量可能影响多个槽位。完整支持边界见 [存档编辑说明](docs/save-editor-lite.md)。

未知自定义对象、不支持的共享或循环引用保持只读。Ren’Py 支持使用目标游戏的本机密钥重签；外来或原签名不可信的文件首次写入前需确认来源。保存前检查文件变化；保存后回游戏重新读档，`persistent` 可能需要重启游戏。真实游戏写回及新旧版本兼容性仍需按目标游戏验收。

导入更新可选择保留内部旧存档或使用新包存档，外部存档在更新与回退时保持原位。新版本按计划同步名称、目录和版本，手动别名、游玩状态和历史保留；别名不会自动累积版本名称。弱关联、重复推荐和未知混合 MTool 目录均由用户决定如何处理。

**“从库中移除”只删除记录；“移入回收站”会包含游戏及已关联存档，包括外部关联存档。** 文件操作保留共享路径、目录身份和任务互斥保护；回收失败不转为永久删除。版本回退依赖回收站中的原旧版，清空回收站后不可回退；手动改名或跨电脑迁移也可能使历史回退不可用。

目前不包含压缩包解压、游戏自动下载或云同步。

## 文档

| 想了解什么       | 入口                                                                                                                                              |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| 安装、升级与迁移 | [Portable 与数据目录](docs/portable.md)                                                                                                           |
| 导入更新与回退   | [更新与恢复说明](docs/v0.3-implementation.md) · [人工关联记忆](docs/import-association-memory.md)                                                 |
| 游戏整理         | [收藏、标签与批量操作](docs/library-organization.md) · [游戏库查重](docs/library-duplicates.md) · [随包 MTool 清理](docs/import-mtool-cleanup.md) |
| 存档支持         | [存档编辑器](docs/save-editor-lite.md) · [存档识别](docs/save-detection.md)                                                                       |
| 开发与交付       | [云端开发](docs/cloud-development.md) · [贡献与测试](CONTRIBUTING.md)                                                                             |
| 版本与验收进度   | [更新记录](CHANGELOG.md) · [路线图](docs/roadmap.md) · [验证记录](docs/validation.md)                                                             |

## 开发

技术栈为 **Tauri · Rust · React · SQLite**。默认流程：**Codex Cloud Linux 开发 → GitHub Windows CI 构建 → 本机下载 EXE 使用和验收**。

固定工具链：Node.js **24.19.0**、pnpm **11.25.0**、Rust **1.99.0**。依赖由锁文件固定，SQLite 使用 bundled C 源码。Linux 安装这些工具、rustfmt / clippy 和基础 C 编译工具后，在仓库根目录执行：

```bash
bash scripts/Check-Cloud.sh
```

入口安装锁定依赖，执行格式、完整前端测试、TypeScript / Vite 和 Rust 核心回归，无需 GTK / WebKit。Windows 本地开发另需 Visual Studio 2026 C++ Build Tools、Windows SDK、WebView2 及 Rust MSVC 工具链；安装锁定依赖后可用 `pnpm tauri dev` 启动开发模式。

```powershell
. ./scripts/Use-Toolchain.ps1
./scripts/Build-PortableRelease.ps1
```

默认输出 `artifacts/windows/butter-manager.exe`，正式打包可加 `-PortableZip`。开发使用独立 `data-dev/`；浏览器前端不能执行桌面文件操作。

CI 保留 Linux 核心检查与 Windows 全部桌面目标 Clippy、原生回归和 Release 构建。main / PR / 普通手动构建保留 EXE artifact **14 天**；推送与源码版本一致的 `vX.Y.Z` Tag 后，检查及资产校验通过会自动发布 Release。Linux 结果不能替代 Windows 检查与实机验收。

测试只使用隔离临时数据库、目录与合成存档。真实游戏、存档、数据库、私钥和 `data/` 不上传到仓库或 CI；开发与升级不备份正式数据库、WAL、SHM 或导出可恢复记录。

## 许可

由 **Zhehao-w** 开发，采用 MIT 许可。通用图标来自 Lucide；应用图标与插画由 Zhehao-w 使用 GPT 生成并授权随项目发布，详见 [第三方许可](THIRD_PARTY_NOTICES.md)。

反馈时请附应用版本、构建提交、复现步骤与脱敏错误信息。
