# Windows Portable 数据目录

Windows Release 默认将应用资料放在 `butter-manager.exe` 旁的 `data/`。应放置在用户可写的文件夹；目录不可写时明确报错并退出，不回退到 AppData。

```text
butter-manager/
├── butter-manager.exe
└── data/
    ├── library.sqlite3（以及运行时的 -wal / -shm）
    ├── appearance.json
    ├── imports/
    ├── caches/
    ├── logs/
    └── EBWebView/（WebView2 创建的用户数据，含 localStorage）
```

`caches/` 与 `logs/` 是 Tauri 对应目录 API 的位置，目前没有额外日志插件。WebView2 自己的缓存保存在其用户数据结构中。系统 WebView2 Runtime 本体、Windows 临时文件/崩溃报告等由系统管理，不能保证全部位于应用目录。

在“设置 → 关于”点击“打开数据目录”可打开当前构建实际使用的目录。

## 开发与发布

保持原命令：

```powershell
pnpm tauri dev
pnpm tauri build --no-bundle -- --locked
```

`tauri.windows.conf.json` 使用 Tauri 2.12 的 `app.appDirectoriesOverride: "./data"`。Tauri 相对于 **EXE** 解析此路径，不依赖工作目录；config/data/localData 使用该根目录，cache/log 使用其 `caches/` 和 `logs/`。不设置窗口的 `dataDirectory`，使默认 WebView2 跟随同一个根目录。[官方说明](https://v2.tauri.app/reference/config/#appdirectoriesoverride)

Windows 开发模式或 debug 构建在创建 Tauri 应用之前把覆盖根切为 `./data-dev`。`pnpm tauri dev` 使用 `src-tauri/target/debug/data-dev/`，数据库、外观、恢复记录和浏览器数据全部与正式 Release 隔离，也不读取旧正式资料。`tauri dev --release` 也按开发模式使用 `data-dev/`。正式 Release 构建对应 `src-tauri/target/release/data/`。非 Windows 的目录行为不变。GitHub Windows CI 的命令与工作流保持原样。

发布时复制 EXE 到专用文件夹，已有资料需连同整个 `data/` 一起移动。关闭程序后移动整个文件夹，配置与库记录仍在；游戏库、存档和公共工具的绝对路径保持原值。跨用户/机器移动时，原游戏路径可能需重新关联；Windows 加密的浏览器秘密不保证跨用户可读。

`.butter-import-*` 仍在游戏库根目录，回收站和更新/回退规则保持不变。

## 数据来源

旧版 v0.3 的一次性迁移已完成，迁移询问、旧目录检测、复制流程及标记不再作为应用功能保留。应用只打开 EXE 旁当前的资料；全新发布文件夹没有 `data/` 时建立独立新库，不读取 AppData，也不合并其他库。

现有配置、导入恢复记录和 localStorage 随整个 `data/` 一起保留。移动时先关闭程序，不要只移动 SQLite 主文件而遗漏 WAL/SHM、`imports/` 或 WebView2 用户目录。Windows 加密的浏览器资料不保证跨用户/机器可读。

## 验证

后端使用独立临时夹具，合并验证开发/正式隔离、同目录实例保护、不可写提示与已有内容保留；未完成更新记录在数据文件夹移动后继续执行及回退。前端验证关于区按钮及失败提示。

`portable_probe` 是开发验证例程，不随主应用提供命令。运行 `pnpm build` 后执行 `./scripts/Test-Portable.ps1`，自动构建两种验证程序，在独立的 `target/portable-probe-*` 夹具中完成四个阶段：Release 写入、开发版写入、移动整个文件夹后两者重启读取。隐藏的真实 WebView2 验证 localStorage，同时核对 SQLite、外观、导入记录和 Tauri 目录实际位置；工作目录刻意与 EXE 所在目录不同，不操作 GUI，不读取正式资料。成功后自动删除夹具，失败时保留诊断文件。

人工验收只需确认：关于区的“打开数据目录”打开 `data/`；关闭程序后连同 EXE 移动整个文件夹，再启动确认资料及视图偏好保留。
