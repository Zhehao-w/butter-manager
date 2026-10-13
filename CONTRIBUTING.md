# 参与开发

当前主要支持 Windows；开发环境与构建步骤见 [README](README.md)。

## 验证

使用仓库指定的 Node.js、pnpm 和 Rust 版本，提交 pnpm-lock.yaml 与 src-tauri/Cargo.lock。

```powershell
pnpm install --frozen-lockfile
pnpm format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
pnpm test:ui
pnpm build
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --lib
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

PR 执行完整前端 / Rust 回归、Prettier / Rustfmt、TypeScript / Vite 和所有目标 Clippy，构建 Windows Release EXE，并上传可下载的 PR artifact。Push 到 main（兼容原 master）、手动工作流及正式 Release 的 published 事件执行相同完整检查和 EXE 构建。工作流不自动创建 Release 或上传 Release 附件。

EXE 构建复用前面已经生成的前端资源，使用 `cargo build --locked --manifest-path src-tauri/Cargo.toml --release --features tauri/custom-protocol`，避免再次运行 Tauri 的前端构建步骤。

局部修改优先运行相关测试；发布或整合较大功能时再执行上面的完整检查。例如：

```powershell
pnpm test:ui src/ImportPage.test.tsx src/LibraryDuplicates.test.tsx
pnpm test:ui:changed
pnpm test:ui:related src/library.ts
pnpm test:ui:watch src/ImportPage.test.tsx
pnpm test db::tests
pnpm test importer::updater_test
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --lib mtool_cleanup::tests
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --lib duplicate
```

`test:ui:changed` 通过 Vitest 依赖图选择相对 HEAD 的改动涉及的测试；`related` 按给定源码查找关联测试。两者在没有匹配项时正常结束，仅用于本地增量检查，CI 始终使用完整 `test:ui`。Rust 的 `pnpm test <过滤词>` 仍编译测试目标，只筛选执行指定模块 / 用例；Cargo 没有跨模块的受影响测试分析，公共路径、数据库、Import 或 Codec 改动应执行完整回归。不要把过滤成功当作全量验证。

共用 Fixture 和 API Mock 位于 `src/testing/`。Mock 每项测试独立重置，未配置的方法默认报错，文件操作须明确设置成功或失败响应；不要依赖未配置的空返回。纯排序 / 扫描数据测试使用 Node 环境，其余 React 测试保留 DOM 环境。

测试应覆盖用户可观察行为和失败边界，避免只镜像实现或为相同交互重复建立夹具。查重状态和 UUID 组合、删除对象保护、导入部分成功及返回调整、清理失败与中断恢复不可因合并测试而丢失覆盖。当前数量与整理明细见 [本次测试精简记录](docs/test-suite-cleanup-2026-10-13.md)；历史记录见 [验证记录](docs/validation.md) 和 [测试审查](docs/test-audit-2026-10-08.md)。

## 修改约定

- 复用现有控件、任务进度和滚动机制；界面文案解释用户需要做的选择。
- 为改变业务行为的修复保留有意义的测试；简单样式调整不新增重复测试。
- 文件操作测试只使用独立临时数据库与目录，不能操作开发者真实游戏、存档或回收站内容。原生回收站往返集成测试默认跳过，显式运行也仅操作自己的临时项目，不清空回收站。
- 不备份用户正式数据库、WAL、SHM，不导出可恢复记录。升级检查可使用只读数量、完整性与不可恢复摘要；测试夹具独立。
- 游戏文件的移动、存档保留、回收及中断恢复语义见 [v0.3 实施说明](docs/v0.3-implementation.md)。
- Import 与库内查重复用名称和推荐逻辑，新增宽松推荐不能自动放宽关联；明确续作冲突和通用启动文件反例须保留。查重持久化扩展现有 UUID 关系，保留 schema 10 到 11 的忽略迁移测试。
- 用户流程变更同步更新 README、路线图及对应专项说明；历史验证保留日期，不将旧测试数量或旧流程写为当前状态。
- 提交新资源时记录来源、许可及必要署名，保留第三方许可文本。

问题反馈请提供版本、复现步骤和脱敏后的错误信息；不要提交真实数据库或游戏文件。PR 应说明最终行为、修改原因和实际运行的检查。
