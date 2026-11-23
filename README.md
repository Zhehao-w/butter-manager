<p align="center">
  <img src="src/assets/app-icon-new.png" width="96" height="96" alt="butter-manager 应用图标" />
</p>

<h1 align="center">butter-manager</h1>

<p align="center">本地游戏，一处整理。</p>
<p align="center">Windows x64 游戏库、启动器与存档编辑器</p>

<p align="center">
  <img src="https://img.shields.io/github/v/release/Zhehao-w/butter-manager?label=release&amp;color=ed91b2" alt="最新正式版本" />
  <a href="https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml"><img src="https://github.com/Zhehao-w/butter-manager/actions/workflows/check.yml/badge.svg?branch=main" alt="Linux / Windows CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Zhehao-w/butter-manager?color=5b8def" alt="开源许可" /></a>
</p>

<p align="center">
  <a href="https://github.com/Zhehao-w/butter-manager/releases/latest"><strong>下载 Windows 版</strong></a> ·
  <a href="#开始使用">快速开始</a> ·
  <a href="#存档编辑">支持的存档</a> ·
  <a href="CHANGELOG.md">更新记录</a> ·
  <a href="https://github.com/Zhehao-w/butter-manager/issues">反馈问题</a>
</p>

扫描已有游戏，管理收藏与进度，导入新版时处理目录、存档和旧版本。游戏库资料保存在程序旁的 `data/`。

| 你可以                 | 操作                                                                                                  |
| ---------------------- | ----------------------------------------------------------------------------------------------------- |
| 整理游戏库             | 在列表和卡片间切换，按名称、别名或路径搜索，使用收藏、标签和组合筛选，批量修改游玩状态                |
| 配置并启动游戏         | 使用 EXE、BAT、Windows 默认应用、公共 MTool 或本地 QSP。直接 EXE 支持启动参数，Bakin 支持专用启动策略 |
| 导入新游戏或更新版本   | 在预览中核对关联、目标和存档处理。可记住人工关联，恢复中断任务，或在旧版仍在回收站时回退              |
| 处理重复文件和路径变更 | 查看重复推荐、回收随包 MTool、重命名游戏目录或批量迁移库路径                                          |
| 定位和编辑存档         | 查找本地或 AppData 存档，按名称、路径或值搜索受支持的字段                                             |

## 开始使用

> **1.6.4** 加入 Godot 4 `FileAccess.store_var()` 存档编辑。正式资产见 [GitHub Releases](https://github.com/Zhehao-w/butter-manager/releases/latest)。

1. **下载并解压。** 下载 Windows x64 Portable ZIP，将 `butter-manager.exe` 放在可写的专用目录。
2. **设置游戏库。** 启动管理器，在「设置」中选择游戏库根目录并保存。首次运行会在程序旁创建 `data/`。
3. **添加游戏。** 用「扫描目录」登记已经整理好的游戏，原文件保持原位。新下载的游戏从「导入」添加，核对预览后执行。
4. **开始游玩。** 从游戏库启动，或打开详情调整启动程序、工作目录和参数。成功启动会更新「正在玩」，保留手动设置的「已通关」。

Windows 需要 WebView2 Runtime。发布页提供 Portable ZIP 和同名 `.zip.sha256`，独立 EXE 可从 CI artifact 下载。若未签名程序触发 SmartScreen，可先核对下载来源和 SHA-256。[WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)

```text
butter-manager/
├── butter-manager.exe
└── data/                 # 游戏库、设置、恢复记录与 WebView2 用户资料
```

**升级时关闭管理器，只替换 EXE，保留完整的 `data/`。** 1.6.4 沿用 schema 15，从 1.5.0–1.6.3 升级无需迁移。「设置 → 关于应用」显示版本和构建提交。[旧版升级与数据目录](docs/portable.md)

迁移时先完成或撤回文件任务，再关闭程序并复制程序和游戏目录。游戏路径有变化时，在「设置 → 高级维护」预览并确认新旧根目录的映射。[便携迁移步骤](docs/portable.md)

## 存档编辑

在 **游戏详情 → 存档位置 → 编辑存档** 选择槽位，搜索字段名、路径或值，修改后保存并回游戏重新读档。找不到文件时可用「快速查找存档」，或点击「开始定位」、切回游戏存档，再点击「完成定位」。[存档发现规则](docs/save-detection.md)

| 格式                             | 可编辑内容                                                                         |
| -------------------------------- | ---------------------------------------------------------------------------------- |
| Godot 4 `FileAccess.store_var()` | 单条或多条 Variant 记录，按路径编辑字典与数组中的数字、布尔值和文本                |
| RPG Maker MV / MZ                | 标准存档中的简单变量、开关、金钱、道具 / 武器 / 防具数量；角色信息只读             |
| Ren’Py `.save` / `persistent`    | 简单字段、受支持的字典 / 列表 / 元组、None 初始化和标准 `RevertableSet` 字符串成员 |
| Unity ES3 `.es3`                 | 明文 / GZip 与标准 AES（需已知密码）的已有简单数值、布尔、文本；保留原类型         |
| Naninovel `.nson` / `.json`      | 已识别状态结构中的标准本地 / 全局自定义变量，保留原始封装                          |
| VNEM/NGAM v1 `.save`             | 按路径编辑进度 JSON 中已有的数值、布尔值和文本，支持对象与数组内字段               |

**每次只保存选中的槽位文件。** Naninovel 的 `Global` 是独立槽位，可能影响多个存档。Ren’Py 优先打开游戏目录内的存档，现有记录可用「分析游戏配置」补全 AppData 路径。各格式的字段范围、已知密码要求和 Ren’Py 本机重签见 [存档编辑说明](docs/save-editor-lite.md)。

## 导入、回退与回收

- 导入更新时可保留旧版内部存档或使用新包存档。外部存档留在原路径，游戏的手动别名、游玩状态和历史随原记录保留。
- 完成项进入「导入记录」，中断项可从「恢复未完成导入」继续。回退需要回收站中仍有旧版原目录。
- 「从库中移除」只删记录。「移入回收站」会列出游戏、已关联存档和归属明确的 Ren’Py AppData 存档，确认前可查看实际范围。

## 文档

| 想了解什么       | 入口                                                                                                                                                       |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 安装、升级与迁移 | [Portable 与数据目录](docs/portable.md)                                                                                                                    |
| 导入更新与回退   | [更新与恢复说明](docs/v0.3-implementation.md) · [人工关联记忆](docs/import-association-memory.md)                                                          |
| 启动与翻译工具   | [EXE 参数](docs/launch-arguments.md) · [Bakin](docs/mtool-bakin.md) · [外置 MTool 翻译包](docs/mtool-external-translation.md) · [QSP](docs/qsp-support.md) |
| 游戏整理         | [收藏、标签与批量操作](docs/library-organization.md) · [游戏库查重](docs/library-duplicates.md) · [随包 MTool 清理](docs/import-mtool-cleanup.md)          |
| 存档支持         | [存档编辑器](docs/save-editor-lite.md) · [存档识别](docs/save-detection.md)                                                                                |
| 开发与交付       | [云端开发](docs/cloud-development.md) · [贡献与测试](CONTRIBUTING.md)                                                                                      |
| 版本与验收进度   | [更新记录](CHANGELOG.md) · [路线图](docs/roadmap.md) · [验证记录](docs/validation.md)                                                                      |

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
