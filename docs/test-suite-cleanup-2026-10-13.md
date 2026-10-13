# Test Suite Cleanup（2026-10-13）

本次只调整测试支持代码、执行脚本、CI 和说明文档。保留此前详情分析 / 目录重命名的未提交实现；与整理开始时相比，核对的 73 个已有业务源码文件内容一致，没有修改业务、GUI、数据库或真实游戏 / 存档，也没有提交或推送。

## 基线与结果

同一台 Windows 机器、同一工具链，各记录一次整理前后完整运行；Rust 使用已有编译缓存。耗时是命令从启动到结束的墙钟时间，前端和 Rust 独立命令并行运行；不是 CI 实测，也不是多轮统计。

| 项目 | 整理前 | 整理后 |
| --- | ---: | ---: |
| 前端测试文件 | 14 | 14 |
| 前端通过用例 | 195 | 194 |
| 前端测试文件行数 | 6,311 | 5,972 |
| 前端测试支持代码行数 | 18 | 154 |
| 前端测试及支持代码合计 | 6,329 | 6,126 |
| 前端完整运行 | 33.16 秒 | 31.76 秒 |
| Rust 通过 / 默认忽略 | 212 / 1 | 212 / 1 |
| Rust 完整运行 | 22.95 秒 | 20.87 秒 |

行数包含空行，不含文档或临时诊断脚本。前端合计减少 203 行，约 3.2%。Rust 的 213 个测试名称逐项相同，26 个含测试的源码文件未改；这些文件共 17,710 行，同时包含生产实现，不能当作纯测试行数。Rust 的耗时差异属于单次测量波动，不能归因于精简。

## 合并与保留

- `App.test.tsx` 删除单独的 `uses the fixed artwork without an appearance picker`，将立绘来源和没有外观设置的断言并入已有侧栏导航测试。
- `App.test.tsx` 删除 `displays a popup for unsaved settings navigation and changes pages only after discard`，将弹窗打开断言并入原生目录选择器后的导航测试。后者仍验证取消、草稿保留、放弃后导航和没有保存调用。
- `LibraryDuplicates.test.tsx` 将 `loads saved ignores after reopening and can undo them before a new check` 与 `loads persisted confirmation after restarting, even before checking` 改成排除 / 确认的 `it.each`。两项场景分别执行，保留重启读取、UUID 顺序、没有自动检查、撤销和确认后的删除入口断言。
- `useStableVirtualizer.test.tsx` 将原 `holds measurements through a paused scrollbar drag and releases on pointerup or blur` 拆为 `pointerup` / `blur` 两个参数化用例，分别验证释放后重新测量，保留滚动条拖拽和内容变化验证。
- `ImportPage.test.tsx` 删除重复的初始紧凑摘要断言；已有键盘 / Tab 测试继续核对摘要内容及展开期间 DOM 节点保留。没有删除 Import 用例。
- `WindowTitleBar.test.tsx` 只提取初始化 Helper，四项用例均保留，包括关闭委托给保护机制且没有直接调用原生关闭。

共用 `src/testing/fixtures.ts`、`apiMock.ts` 和 `interactions.ts`，集中 Fixture、API Mock 重置和按钮就绪等待。每次返回独立数据；未配置 API 默认拒绝，文件操作成功必须显式配置，避免无意依赖空返回。`useJob.test.tsx` 也复用 Job Fixture。

`library.test.ts` / `scan.test.ts` 的纯数据测试使用 Node 环境；`testSetup.ts` 只在 DOM 存在时加载 React 清理和原生弹窗替身。其余组件测试仍使用原 DOM 环境。审查 48 个组件的引用，没有发现可明确删除的无用组件；保留 ProgressBar、TaskBanner、ImportProgress 及不同业务弹窗，不改主页面结构。

## 风险行为验证

未引入量化覆盖率工具，不宣称覆盖率不变。以下已有验证仍保留：

| 行为 | 保留的验证 |
| --- | --- |
| Import 文件移动、跨盘复制、目标冲突、失败恢复 | 原 importer 临时目录 / 数据库夹具未改；前端部分成功及返回调整保留 |
| Journal 中断、幂等性、撤回 | importer / updater 的阶段恢复和重复执行测试未改 |
| 版本更新、存档迁移、回退 | 原 updater 与数据库回归未改 |
| 回收站、路径保护、其他游戏保护 | 删除后端与 DeleteGameDialog 测试未改；查重两种 UUID 方向的明确删除对象、失败 / 取消 / 预检阻止保留 |
| 随包 MTool 清理、失败重试、目录身份 | 后端清理与恢复测试未改；Import 配置及失败重试验证保留 |
| Ren'Py / RPG Maker 存档完整性 | Codec 与 SaveEditor 测试未改 |
| SQLite Migration 和用户资料 | 原 db 测试未改 |
| 库内查重、确认 / 排除状态持久化 | 原匹配 / DB 测试未改；前端参数化两种状态的重启读取及撤销 |
| 窗口关闭、导航、折叠、虚拟滚动 | App / SaveEditor 保留未保存和关键任务保护；标题栏保留委托检查；Import 保留摘要 / 草稿；Hook 单独验证测量释放 |

原生 Windows 回收站集成测试继续默认忽略；它只适合显式操作独立临时夹具，本次没有运行或触碰正式回收站内容。

## 执行策略

本地新增 `test:ui:changed`、`test:ui:related`、`test:ui:watch`。前两者使用 Vitest 自带依赖图，无匹配时正常结束；CI 始终执行全量测试。`pnpm test <过滤词>` 提供 Rust 局部执行，仍编译完整测试目标，不宣称能够自动分析 Rust 受影响模块。脚本使用 `--lib` 与原 CI 对齐；基线没有文档测试。

| 触发 | 检查与构建 |
| --- | --- |
| Pull Request | 完整前端 / Rust 测试、Prettier、Rustfmt、TypeScript / Vite、所有目标 Clippy、Windows Release EXE；上传 PR 可下载 artifact |
| Push main / master | 相同完整检查和 Windows Release EXE |
| Release published | 相同完整检查和 Windows Release EXE |
| 手动工作流 | 相同完整检查和 Windows Release EXE |

复用单个既有 Windows workflow，无新增项目依赖。EXE 步骤使用 `cargo build --locked --manifest-path src-tauri/Cargo.toml --release --features tauri/custom-protocol`，复用前面已经构建的前端，避免 Tauri CLI 再构建一次前端。PR 上传动作固定到官方 v7 提交，产物不存在时失败；没有自动创建 Release 或上传 Release 附件。

## 验证与产物

完整前端 194 项通过；Rust 212 项通过、1 项既有测试忽略。Prettier、TypeScript / Vite、Rustfmt、所有目标 Clippy（警告视为错误）及差异空白检查通过。增量入口实测：`test:ui:related src/library.ts` 正确选择 6 个文件 / 149 项并通过，`pnpm test db::tests` 24 项通过；关联广泛的公共模块仍可能选中很多测试。

Windows Release EXE 构建通过，构建耗时 39.90 秒；文件为 `src-tauri/target/release/butter-manager.exe`，21,866,496 字节，更新时间 2026-10-13 01:14:17（本机时区）。构建时管理器未运行，无需关闭进程；没有自动操作 GUI 或打开游戏。

上述验证完成后，按用户追加要求取消 PR 跳过 EXE 的规则，并加入 PR artifact 上传及同步文档；没有重新执行已经通过的测试、检查或构建。实际 GitHub 上传行为待未来 CI 验证，本次没有推送或触发 CI。
