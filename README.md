<p align="center">
  <img src="src/assets/app-icon-new.png" width="96" height="96" alt="butter-manager 应用图标" />
</p>

<h1 align="center">butter-manager</h1>

<p align="center">把本地游戏、版本更新和存档管理放在一起。</p>

<p align="center">
  <a href="package.json"><img src="https://img.shields.io/github/package-json/v/Zhehao-w/butter-manager?label=source&amp;color=5b8def" alt="源码版本" /></a>
  <a href="https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml"><img src="https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml/badge.svg?branch=main" alt="Linux / Windows CI" /></a>
  <a href="https://github.com/Zhehao-w/butter-manager/releases/latest"><img src="https://img.shields.io/github/v/release/Zhehao-w/butter-manager?label=release&amp;color=ed91b2" alt="最新正式发布" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Zhehao-w/butter-manager?color=5b8def" alt="开源许可" /></a>
</p>

<p align="center">
  <a href="#开始使用">开始使用</a> ·
  <a href="https://github.com/Zhehao-w/butter-manager/releases">下载程序</a> ·
  <a href="CHANGELOG.md">更新记录</a> ·
  <a href="docs/roadmap.md">路线图</a> ·
  <a href="https://github.com/Zhehao-w/butter-manager/issues">反馈问题</a>
</p>

轻量的 **Windows x64 本地游戏库与启动器**。扫描已有游戏，搜索并启动；导入新版时预览文件与存档处理；在同一个地方整理重复游戏、游玩进度和版本记录。现有游戏扫描入库后保持原位，管理资料保存在程序旁的 `data/`。

**当前源码版本：1.0.0**。正式安装包以 [Releases](https://github.com/Zhehao-w/butter-manager/releases) 为准；真实游戏读档与干净 Windows 环境的验收进度见 [路线图](docs/roadmap.md)。

## 能做什么

| 功能               | 用法                                                                 |
| ------------------ | -------------------------------------------------------------------- |
| 游戏库与启动       | 列表 / 卡片视图，名称、别名与路径搜索，状态、引擎及启动方式组合筛选  |
| 批量导入与更新     | 预览关联、名称、启动配置与存档处理，确认后执行，保留原游戏标识和历史 |
| 中断恢复与版本回退 | 继续或撤回未完成任务，从仍在回收站中的原旧版本回退                   |
| 查重与整理         | 查看匹配理由，确认 / 排除 / 撤销，选择保留或回收哪一份               |
| 随包 MTool 清理    | 导入后回收确认的工具包装目录，切换公共 MTool；失败可重试             |
| 存档编辑           | 在详情中编辑受支持的 RPG Maker / Ren’Py 存档，按字段或值搜索         |
| 路径与资料管理     | 目录重命名、整库路径重新关联、Ren’Py AppData 专属存档目录手动回收    |

启动支持直接 EXE、Windows 默认应用、共享 MTool 和本地 QSP 播放器。未识别或多启动文件的游戏可手动配置；“详情 → 分析启动配置”将建议填入草稿，由你检查并保存。别名仅由用户维护，不自动累积版本名称。

## 开始使用

1. **下载并解压**：从 [Releases](https://github.com/Zhehao-w/butter-manager/releases) 选择已发布版本，下载 `butter-manager-v1.0.0-windows-x64-portable.zip`，解压到可写的专用目录。1.0.0 尚未发布时，可从成功的 [GitHub Actions](https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml) 运行中下载对应提交的 EXE artifact 进行验收。
2. **打开程序**：运行 `butter-manager.exe`，首次启动自动创建旁边的 `data/`。在“设置”中选择游戏库根目录并保存。
3. **登记已有游戏**：点击“扫描目录”，或进入“游戏整理 → 目录扫描”，选择要入库的游戏。扫描不搬动现有游戏。
4. **开始游玩**：从游戏库启动，或打开详情调整启动配置。成功提交启动请求后自动标记为“正在玩”；手动设置的“已通关”保留。

Windows 需要 [WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)。正式发布同时提供独立 EXE 和 `SHA256SUMS.txt`；ZIP 内只有程序。未签名 EXE 可能显示 [SmartScreen 提示](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/)，请核对下载来源与同一发布页的校验值。

```text
butter-manager/
├── butter-manager.exe
└── data/                 # 游戏库、设置、恢复记录与 WebView2 用户资料
```

**升级**：关闭管理器，只替换 `butter-manager.exe`，保留整个现有 `data/`。不要在下载目录启动后，用新建的空 `data/` 替换原有资料。可在“设置 → 关于应用”核对版本和构建提交。

**迁移到新电脑**：先完成或撤回文件任务，关闭程序，复制整个程序文件夹及游戏目录，再在“设置 → 高级维护 → 批量迁移游戏库路径”预览并确认新旧根目录。迁移按相对路径重新关联已有记录，不搬动文件；失败项保留原记录。原电脑回收站中的版本不会随之迁移。详见 [Portable 与迁移说明](docs/portable.md)。

## 导入、更新与整理

**已有游戏用目录扫描，新下载的游戏用导入。** 导入支持一个或多个文件夹；先确认已有游戏关联、目标名称和启动方式，再选择“保留旧存档”或“使用新包存档”。弱关联和多个候选由你决定，不自动判断哪个版本更高。

- 更新按计划同步新名称、目录与版本，保留 UUID、手动别名、状态和历史。成功更新会刷新入库时间；普通资料编辑、重试和回退不刷新。
- 保留旧存档时，将已关联的内部存档带到新版相同相对位置；AppData 等外部存档在更新与回退时保持原位。
- 任务按“新的导入 → 待确认 → 导入记录”流转。未完成项可继续或撤回，已经成功的项目保留；回退入口位于导入记录详情。
- “游戏整理 → 重复检查”只比较已入库资料，不自动合并或删除。可确认、排除或撤销，再选择回收哪一份；删除前展示完整路径。
- 随包 MTool 只清理可识别的标准包装目录与关联脚本，未知混合目录保留；BAT 优先作为启动标记，仅在无法确定启动文件时使用经验证的回退配置。

**回收站清空后无法回退，游戏自身决定存档能否兼容新旧版本。** 文件操作保留路径边界、共享存档、目录身份及任务互斥保护；回收失败不转为永久删除。

详细规则：[更新与恢复](docs/v0.3-implementation.md) · [游戏库查重](docs/library-duplicates.md) · [随包 MTool 清理](docs/import-mtool-cleanup.md)。

## 存档编辑与文件管理

在游戏详情点击粉色 **编辑存档**，选择槽位，按字段名称、路径、类型或变量值搜索，修改后统一保存。

| 格式                          | 支持范围                                                               |
| ----------------------------- | ---------------------------------------------------------------------- |
| RPG Maker MV / MZ             | 标准存档的简单变量、开关、金钱、道具 / 武器 / 防具数量；角色信息只读   |
| Ren’Py `.save` / `persistent` | 受支持的简单字段、已知字典 / 列表 / 元组子字段、明确类型的 None 初始化 |
| Ren’Py 标准 `RevertableSet`   | 受支持结构的字符串成员查看、增加与删除                                 |

未知自定义对象、不支持的共享或循环引用保持只读。Ren’Py 支持使用目标游戏的本机密钥重签；外来或原签名不可信的文件，首次写入前需确认来源。保存前检查文件变化；保存后回游戏重新读档，`persistent` 可能需要重启游戏。**合成样本验证不替代真实游戏写回与读档验收**，格式限制见 [存档编辑器说明](docs/save-editor-lite.md)。

| 操作                    | 影响                                                           |
| ----------------------- | -------------------------------------------------------------- |
| 从库中移除              | 只移除记录，游戏和存档文件保持原位                             |
| 移入回收站              | 回收游戏目录及已关联存档，包含外部关联存档                     |
| 重命名目录              | 同父目录改名，同步名称与内部绝对存档路径，保留原记录           |
| Ren’Py AppData 手动清理 | 一次确认后回收已明确识别的专属目录，保持游戏目录存档与资料关联 |
| 高级维护 → 清空游戏库   | 清空管理记录与库设置，游戏和存档文件保持原位                   |

手动改名后，依赖原路径的历史回退可能被安全校验阻止。未保存资料在关闭时提供保存、放弃或取消；文件任务运行期间需等待完成。

目前不包含压缩包解压、游戏自动下载或云同步。

## 构建与参与开发

技术栈：**Tauri · Rust · React · SQLite**。默认采用 **Codex Cloud Linux 开发 → GitHub Windows CI 构建 → 本机下载 EXE 使用和验收**。固定工具链为 Node.js **24.19.0**、pnpm **11.25.0**、Rust **1.99.0**，依赖由锁文件固定，SQLite 使用 bundled C 源码。

Linux 安装上述工具链、rustfmt / clippy 和基础 C 编译工具后，在仓库根目录执行：

```bash
bash scripts/Check-Cloud.sh
```

入口安装锁定依赖，执行格式、完整前端测试、TypeScript / Vite 和 Rust 核心回归，无需 GTK / WebKit。Windows CI 另保留全部桌面目标 Clippy、原生回归与 Release EXE 构建；Linux 结果不能替代 Windows 检查。

Windows 本地开发另需 Visual Studio 2026 C++ Build Tools、Windows SDK、WebView2 和 Rust MSVC 工具链。安装锁定依赖后，可用 `pnpm tauri dev` 开发，构建命令为：

```powershell
. ./scripts/Use-Toolchain.ps1
./scripts/Build-PortableRelease.ps1
# 准备正式发布包及 SHA-256：
./scripts/Build-PortableRelease.ps1 -PortableZip -ExpectedTag v1.0.0
```

输出位于 `artifacts/windows/`；打包脚本检查 package、Cargo、Tauri 三处版本一致，EXE 文件名始终为 `butter-manager.exe`。开发模式使用独立 `data-dev/`，浏览器前端不能执行桌面文件操作。

PR、main 和普通手动 CI 均保留 EXE artifact **14 天**，下载前应确认整轮运行成功。**推送与源码版本一致的 `v1.0.0` Tag 会自动发布 Release**：Linux / Windows 完整检查通过后，校验 EXE、Portable ZIP 与 SHA-256，创建草稿、上传并复核资产，再公开发布；失败保持草稿供重跑继续。`v1.0.0-rc.1` 这类 Tag 按预发布处理，也须与源码版本一致。工作流不创建 Tag 或自动修改版本。

发布前先提交并推送包含版本号及工作流的源码，再在目标提交创建、推送 Tag；只在本机创建 Tag 不会触发。已有 Release 的 published 事件和显式手动上传入口仍保留。详见 [Tag 自动发布流程](docs/cloud-development.md#tag-自动发布)。

开发及升级不备份正式数据库、WAL、SHM，也不导出可恢复记录；测试仅使用隔离临时数据库、目录与合成存档。真实游戏、存档、数据库和私钥不要上传到仓库或 CI。

[云端开发与下载](docs/cloud-development.md) · [贡献约定与测试](CONTRIBUTING.md) · [验证记录](docs/validation.md) · [Tauri Windows 环境](https://v2.tauri.app/start/prerequisites/#windows)。

## 项目与许可

由 **Zhehao-w** 开发，使用 [MIT 许可](LICENSE)。通用图标来自 Lucide；应用图标与插画由 Zhehao-w 使用 GPT 生成并授权随项目发布，第三方许可见 [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md)。

反馈问题时请附应用版本、构建提交、复现步骤与脱敏错误信息；不要提交真实数据库、游戏文件、存档或私钥。
