# Windows Portable 数据目录

Windows Release 默认将应用资料放在 `butter-manager.exe` 旁的 `data/`。应放置在用户可写的文件夹；目录不可写时明确报错并退出，不回退到 AppData。

## 安装、升级与迁移

1.2.1 的正式主资产为 `butter-manager-v1.2.1-windows-x64-portable.zip` 和 `SHA256SUMS.txt`；独立 `butter-manager.exe` 通过 Actions artifact 下载，不作为本版本 Release 附件。正式下载以 [Releases](https://github.com/Zhehao-w/butter-manager/releases) 中已发布的资产为准。

首次安装：下载 ZIP，解压到可写的专用目录，运行 EXE。ZIP 内只有 `butter-manager/butter-manager.exe`；首次运行自动建立 `data/`，不携带别人的数据库、导入记录、WebView2 资料、缓存或游戏存档。

升级：关闭管理器，只用新版 EXE 替换旧 EXE，保留 `data/` 及其全部内容。不要把已有用户资料删除或换成空目录。1.2.x 使用 schema 12；从 1.2.0 升级到 1.2.1 不增加数据库迁移。旧版 1.1.0 无法打开升级后的库，请勿直接换回旧 EXE。需要系统 [WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)，不随 ZIP 携带多文件运行时。未签名 EXE 可能触发 [SmartScreen](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/)，请核对官方来源和校验值。

迁移到新电脑：

1. 完成或撤回未结束的文件操作，关闭管理器。
2. 复制整个程序文件夹（EXE 和完整 `data/`），另将游戏目录复制到新位置。
3. 打开新电脑上的管理器，在“设置 → 高级维护”执行“批量迁移游戏库路径”。旧根目录可以不存在，新根目录须已包含复制好的游戏。
4. 预览每项结果，确认后检查公共 MTool、游戏启动与存档；无法迁移的记录保持原样，可用详情中的单游戏目录关联处理。

映射严格按旧根目录下的相对组件进行，不按名称猜测。例如 `E:\Butter\RPG\Game B` 映射到 `D:\Games\Butter\RPG\Game B`，`E:\Butter2` 不属于前者。游戏启动文件、MTool target、QSP 主文件、工作目录、目录链接及 UUID 冲突不通过时跳过该项；原本未配置的启动项保持未配置。

公共 MTool 位于旧根目录内时也按相对位置迁移，无固定文件夹名称。新目录及现有相对程序 / Loader 验证成功才更新；无效时保留旧 MTool 设置，有效游戏可继续迁移。不在旧根目录内的公共工具保持原样。

迁移只在同一个数据库事务中更新安装路径、路径 key、根目录设置及原游戏目录内的绝对存档路径；`<GAME>`、相对路径、AppData 与其他外部路径保持原样。UUID、名字、别名、版本、启动模式、状态、时间、运行 / 版本历史及查重关系不改。不复制 / 移动 / 删除文件，不重新检测存档或重建记录。未完成文件任务或 Import Recovery 阻止迁移；预览期间数据库、目标身份或配置变化需重新预览，提交失败整体回滚。

历史 Import Journal 与导入记录中的原路径不改写。原电脑回收站中的版本不会随 ZIP 或 `data/` 搬走；历史回退可能因此不可用，管理器不会伪造新的回退能力。

```text
butter-manager/
├── butter-manager.exe
└── data/
    ├── library.sqlite3（以及运行时的 -wal / -shm）
    ├── imports/
    ├── caches/
    ├── logs/
    └── EBWebView/（WebView2 创建的用户数据，含 localStorage）
```

`caches/` 与 `logs/` 是 Tauri 对应目录 API 的位置，目前没有额外日志插件。WebView2 自己的缓存保存在其用户数据结构中。系统 WebView2 Runtime 本体、Windows 临时文件/崩溃报告等由系统管理，不能保证全部位于应用目录。

在“设置 → 关于应用”点击“打开数据目录”可打开当前构建实际使用的目录。

## 本机使用与开发分开

当前本机正式程序位于 `E:\butter-manager\butter-manager.exe`，完整正式资料位于 `E:\butter-manager\data\`；开发仓库（包括 `.git`、源码及未提交修改）位于 `E:\butter-manager\dev\`。日常从根目录 EXE 或指向它的快捷方式启动。构建输出旁首次启动会建立独立空库，因此不能以输出目录的新 `data/` 覆盖正式资料。

开发命令在 `dev/` 中执行。Rust 工具链和 Visual Studio 2026 C++ Build Tools 全局共享；`scripts/Use-Toolchain.ps1` 发现已安装的最新编译工具，将 Cargo registry、pnpm、临时文件缓存保留在项目 `.tools/`，`node_modules/` 和 Rust target 也留在仓库内。CI 使用 Windows Server 2025 / Visual Studio 2026 镜像。后续将源码及未提交修改完整转移到云端后，可清理整个 `dev/`，根目录 EXE 与 `data/` 可继续使用；全局编译工具按系统安装方式管理。

本机升级先完成构建，关闭根目录管理器，只将 `dev/artifacts/windows/butter-manager.exe` 替换到根目录，再从根目录重新启动。构建脚本本身不覆盖正式程序；无需为这种单纯替换 EXE 的升级每次执行 Portable 专项测试，不移动或备份正式数据库。

## 开发与发布

开发及正式打包命令：

```powershell
pnpm tauri dev
./scripts/Build-PortableRelease.ps1
```

`tauri.windows.conf.json` 使用 Tauri 2.12 的 `app.appDirectoriesOverride: "./data"`。Tauri 相对于 **EXE** 解析此路径，不依赖工作目录；config/data/localData 使用该根目录，cache/log 使用其 `caches/` 和 `logs/`。不设置窗口的 `dataDirectory`，使默认 WebView2 跟随同一个根目录。[官方说明](https://v2.tauri.app/reference/config/#appdirectoriesoverride)

Windows 开发模式或 debug 构建在创建 Tauri 应用之前把覆盖根切为 `./data-dev`。`pnpm tauri dev` 使用 `src-tauri/target/debug/data-dev/`，数据库、恢复记录和浏览器数据全部与正式 Release 隔离，也不读取旧正式资料。`tauri dev --release` 也按开发模式使用 `data-dev/`。所有正式 EXE 的 `data/` 均相对于自身位置，与构建缓存或启动工作目录无关；非 Windows 行为不变。

脚本检查 `package.json`、Cargo package 与 Tauri 配置版本一致，构建生产前端及 Windows x64 `tauri/custom-protocol` EXE。独立缓存位于 `src-tauri/target/portable-release/`，默认只输出 `artifacts/windows/butter-manager.exe`；正式发布加 `-PortableZip`，另生成版本化 ZIP 及仅包含 ZIP 条目的 SHA-256 校验文件。只打包明确的一份 EXE，不遍历或复制旁边的资料；拒绝覆盖正在使用的构建 / 输出 EXE，不关闭程序或启动 GUI。每轮先清除自身旧分发产物，失败后也清理，不能拿旧成功包冒充新结果；构建缓存可继续复用。

`-SkipFrontendBuild` 仅供已完成前端检查的 CI / 本地流程复用资源，普通打包不要跳过。`-ExpectedTag vX.Y.Z` 要求 Tag 与源码版本完全一致，不自动改版。输出目录只能是本仓库 `artifacts/` 下的专用子目录；`-CargoExecutable` 用于隔离的构建失败回归，不用于正常发布。

现有 `check.yml` 并行验证 Linux 前端 / 核心和 Windows 完整目标。PR、普通 Push 与手动构建均保留 EXE artifact（14 天），普通构建不发布。推送与源码版本一致的 `vX.Y.Z` Tag 后，Windows 加 `-PortableZip`，两端检查通过后由独立发布 job 校验 ZIP 与构建 EXE 内容一致、ZIP SHA-256 正确，自动创建 Release 草稿，仅上传 ZIP 和校验文件并复核远端大小和摘要，再公开发布；预发布后缀 Tag 标记 prerelease。上传失败保持草稿，重跑可继续；已公开且内容不同的同名资产不会自动覆盖。只有发布 job 有 `contents: write`，不创建 Tag、不自动改版、不签名 EXE。

已有 Release 的 published 事件或显式手动上传入口仍保留；手动上传需已有 Tag / Release 且版本一致，取消勾选时只检查、构建和保留 artifact。云端准备、Tag 发布与重跑步骤见 [云端开发说明](cloud-development.md#tag-自动发布)。

发布时复制 EXE 到专用文件夹，已有资料需连同整个 `data/` 一起移动。关闭程序后移动整个文件夹，配置与库记录仍在；游戏库、存档和公共工具的绝对路径保持原值。跨用户/机器移动时，原游戏路径可能需重新关联；Windows 加密的浏览器秘密不保证跨用户可读。

`.butter-import-*` 仍在游戏库根目录，回收站和更新/回退规则保持不变。

## 数据来源

旧版 v0.3 的一次性迁移已完成，迁移询问、旧目录检测、复制流程及标记不再作为应用功能保留。应用只打开 EXE 旁当前的资料；全新发布文件夹没有 `data/` 时建立独立新库，不读取 AppData，也不合并其他库。

现有配置、导入恢复记录和 localStorage 随整个 `data/` 一起保留。移动时先关闭程序，不要只移动 SQLite 主文件而遗漏 WAL/SHM、`imports/` 或 WebView2 用户目录。Windows 加密的浏览器资料不保证跨用户/机器可读。

## 验证

Portable 专项测试是手动回归，不随常规构建或 CI 执行。首次准备分发、数据目录 / 启动协议或打包脚本改变时，按改动范围选择验证：先用 `-PortableZip` 构建，再执行 `./scripts/Test-PortableRelease.ps1` 检查包内结构、哈希与失败清理；涉及资料目录和迁移时执行下面的真实 WebView2 检查。普通业务改动保留对应回归与 EXE 构建即可。

后端使用独立临时夹具，合并验证开发/正式隔离、同目录实例保护、不可写提示与已有内容保留；未完成更新记录在数据文件夹移动后继续执行及回退。前端验证关于区按钮及失败提示。

`portable_probe` 是开发验证例程，不随主应用提供命令。运行 `pnpm build` 后执行 `./scripts/Test-Portable.ps1`，自动构建两种验证程序，在独立的 `target/portable-probe-*` 夹具中完成四个阶段：Release 写入、开发版写入、移动整个文件夹后两者重启读取。隐藏的真实 WebView2 验证 localStorage，同时核对 SQLite、导入记录和 Tauri 目录实际位置；工作目录刻意与 EXE 所在目录不同，不操作 GUI，不读取正式资料。成功后自动删除夹具，失败时保留诊断文件。

人工验收只需确认：关于区的“打开数据目录”打开 `data/`；关闭程序后连同 EXE 移动整个文件夹，再启动确认资料及视图偏好保留。
