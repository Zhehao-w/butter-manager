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

Windows 自动检查还会执行 `pnpm tauri build --no-bundle -- --locked`，不生成 Release 或上传产物。

## 修改约定

- 复用现有控件、任务进度和滚动机制；界面文案解释用户需要做的选择。
- 为改变业务行为的修复保留有意义的测试；简单样式调整不新增重复测试。
- 文件操作测试只使用独立临时数据库与目录，不能操作开发者真实游戏、存档或回收站内容。原生回收站往返集成测试默认跳过，显式运行也仅操作自己的临时项目，不清空回收站。
- 不备份用户正式数据库、WAL、SHM，不导出可恢复记录。升级检查可使用只读数量、完整性与不可恢复摘要；测试夹具独立。
- 游戏文件的移动、存档保留、回收及中断恢复语义见 [v0.3 实施说明](docs/v0.3-implementation.md)。
- 提交新资源时记录来源、许可及必要署名，保留第三方许可文本。

问题反馈请提供版本、复现步骤和脱敏后的错误信息；不要提交真实数据库或游戏文件。PR 应说明最终行为、修改原因和实际运行的检查。
