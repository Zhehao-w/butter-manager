# Linux 云端开发与 Windows 交付

日常修改、前端和核心 Rust 验证在 Linux 完成；Windows CI 负责桌面原生检查和 EXE。本机保留 `butter-manager.exe` 与完整 `data/`，用于实际使用和验收。真实游戏、数据库、存档及私钥不上传云端，不复制到测试或 CI。

## 准备环境

使用仓库 `.node-version`（Node.js 24.19.0）、`package.json`（pnpm 11.25.0）与 `rust-toolchain.toml`（Rust 1.99.0）固定版本。环境需要 Bash、Git、rustup、Node.js、pnpm，以及 C 编译器、make 等基础编译工具（SQLite 使用 bundled C 源码）。全局工具由云端环境提供，项目依赖按锁文件安装。

在仓库根目录运行：

```bash
bash scripts/Check-Cloud.sh
```

此入口安装锁定的前端依赖，依次运行 Prettier、Rustfmt、完整前端测试、TypeScript / Vite 和无桌面依赖的完整 Rust 核心测试，并用 Node crypto / zlib 独立核验仅合成的 Unity 存档输出。它把 Cargo、pnpm 与临时文件放在项目 `.tools/`，Rust target 留在 `src-tauri/target/`，不使用本机 Windows 路径。不要将 `.tools/`、`node_modules/`、target 或真实 `data/` 提交到 Git。

无需为了核心检查安装 WebKit/GTK 或尝试 Linux 编译 Windows EXE。`pnpm dev` 可用于浏览器前端开发，但目前没有完整演示数据，浏览器不能执行桌面文件操作。默认的 `pnpm check` 包含桌面目标，应由 Windows CI 执行；Linux 入口不将无原生保护的结果冒充 Windows 验证。

增量测试使用 [CONTRIBUTING](../CONTRIBUTING.md) 中的 `test:ui:changed`、`test:ui:related` 与 `pnpm test <过滤词>`。涉及数据库、路径、导入或存档的共享逻辑时执行完整入口；测试只使用隔离临时夹具。

## CI 与下载

现有 `check.yml` 内 Linux 和 Windows 两个 job 并行执行，PR、main（兼容 master）、版本 Tag、手动构建和正式 Release 均保留检查：

| job     | 检查与交付                                                                                         |
| ------- | -------------------------------------------------------------------------------------------------- |
| Linux   | 使用上述入口验证前端与可移植 Rust 核心，不构建桌面程序                                             |
| Windows | 完整前端 / Rust 核心测试、格式、TypeScript / Vite、所有桌面目标 Clippy，并构建 Windows Release EXE |
| Publish | Tag 推送自动发布；也支持已有 Release / 显式手动上传。等待两端成功后校验、上传并复核资产            |

所有 Windows 构建都上传 `butter-manager-windows-<完整提交 SHA>` artifact；PR 名称为 `butter-manager-windows-pr-<编号>`，保存 14 天。确认整个运行成功后下载与目标提交对应的 artifact，解压取得单 EXE。过期后可以对目标提交手动重跑构建，普通手动运行保持 `publish_release=false`，不会创建 Tag 或 Release。

下载也可通过 GitHub CLI 完成：

```bash
gh run list --workflow check.yml --branch main
gh run view <run-id>
gh run download <run-id> --name butter-manager-windows-<完整提交SHA> --dir ./download
```

正式发布另保留版本化 Portable ZIP 与 SHA-256，规则见 [Portable 说明](portable.md)。下载 GitHub artifact 得到的外层 ZIP 是 Actions 包装，不是应用需要的运行时依赖。

CI 仅缓存前端下载、Cargo registry / git 和编译 target；不缓存用户资料或 `node_modules`。前端按系统、架构与锁定工具配置隔离。Rust 另按固定工具链和 runner 镜像版本隔离，Windows Release 仍使用独立 `portable-release` target，保留完整测试、默认桌面所有目标严格 Clippy 和 Release EXE 构建。

Rust 完整缓存按 Cargo manifest / lock 的依赖集合保存，**不再每个提交保存一份约 800 MB 的相同依赖副本**。优先恢复相同依赖的完整缓存，其次恢复同依赖的失败部分缓存，再回退到同系统 / 架构 / 镜像 / 工具链的其他完整依赖缓存；Cargo 继续按锁文件、源码、特性和构建模式校验并补编译，缓存命中不跳过任何检查。过渡期也可恢复旧格式的相同依赖缓存。镜像或 Rust 工具链改变时保持隔离，可能需要冷构建。

还原与保存拆为两个步骤：整轮 job 成功后才写入完整缓存；检查失败时，可单独保存已完成的依赖编译供修复重跑使用，不覆盖完整缓存。不为取消的运行保存缓存。每个依赖集合的完整 / 部分键分别不可变，已有键不重复上传；失败部分缓存也不会阻止后续成功结果升级为完整缓存。

实际耗时应区分冷构建与缓存命中。优化前本 PR 的 Windows 成功运行 `38962634723` 共约 **12 分 52 秒**：后端测试 2 分 45 秒、默认桌面 Clippy 2 分 23 秒、Release EXE 5 分 18 秒；该轮 Rust 缓存未命中。优化结果以本 PR 后续相同配置的实际 CI 记录为准，不承诺固定提速倍数。

## Tag 自动发布

先提交并推送包含工作流、版本配置及锁文件的目标源码，再推送该提交的版本 Tag。例如应用的 package、Tauri、Cargo 版本均为 `1.3.1` 时，使用 `v1.3.1`。只在本机创建 Tag 不触发远端 CI；推送 Tag 才是自动发布入口，**无需提前创建 Release**。工作流不创建 Tag、不自动改版，普通 main / PR / 手动构建不会创建 Release。

1. Linux 与 Windows 并行执行完整检查，Windows 额外生成版本化 Portable ZIP 和 `<ZIP 文件名>.sha256`，同时保留通常的 EXE artifact。
2. 只有两端成功，发布 job 才在 Ubuntu 上运行；核对两端实际检出提交一致，并解析远端轻量 / 附注 Tag，确认仍指向该提交。Windows 仍负责全部 EXE 编译及原生回归。
3. Windows 发布构建复用已有产物运行 Portable 专项，核对 ZIP 白名单、EXE 内容、同名 `.zip.sha256` 及失败清理；不重新编译。发布 job 再校验中间产物只包含构建 EXE、ZIP 和 SHA-256，ZIP 内只有与构建 EXE 内容一致的程序；校验文件仅列 ZIP。创建带 GitHub 自动生成说明的 Release 草稿，**只上传 ZIP 和 `<ZIP 文件名>.sha256`**，上传后复核远端两项资产的大小和 SHA-256，再公开发布。独立 EXE 继续作为 CI artifact 提供下载，不作为 Release 附件。Tag 含预发布后缀时使用 prerelease。
4. 上传或校验失败时保留草稿，不公开发布；可在 Actions 中重跑原失败运行，继续同一 Tag 的草稿。已经成功且资产一致的 Release 重跑不重复上传；发现已公开的同名资产内容不同会停止，不自动覆盖。

发布运行不被同 Tag 的后续运行自动取消；分支 / PR 的重复检查仍可自动取消。发布期间 Tag 被移动会阻止旧构建公开发布，不要通过改指向同名 Tag 来替换正式版本。

已有 Release 的 published 事件仍可执行检查并补齐资产。手动上传要求 `publish_release=true`、填写已有 `release_tag` 且 Release 已存在；手动入口不自动创建或公开草稿。发布写权限只提供给等待两端成功的发布 job。发布脚本的隔离测试在 Linux CI 执行，不接触真实 Release 或资料。

## 本机验收与后续清理

关闭管理器，只替换本机 `E:\butter-manager\butter-manager.exe`，保留旁边完整 `data/` 后重开。不要在下载目录启动后用新建的空 `data/` 替换正式资料。GUI 验收由用户操作，包括运行任务时系统关闭不会取消任务、未保存资料的保存 / 放弃 / 取消，以及真实游戏启动和读档。

设置 → 关于应用的版本下显示构建提交短号，来自实际检出的 Git HEAD；本地未提交构建附带“本地修改”，缺少 Git 元数据时显示“未知”。标识在构建时嵌入，不在运行时读取 Git；EXE 文件名仍为 `butter-manager.exe`。

只有所需提交及未提交工作全部转移、首次 Linux / Windows CI 和下载 EXE 验收通过后，才安排清理本机 `dev/`；这轮准备不删除开发目录或全局工具。
