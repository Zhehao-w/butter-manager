# Development policy

- 默认开发环境为 Linux 云端，Windows GitHub Actions 负责完整原生检查及 Release EXE；本机只使用和验收。先读 `docs/cloud-development.md`，在 Linux 使用 `bash scripts/Check-Cloud.sh`，不要调用 Windows 工具链脚本或假定存在 E 盘。
- PR、main 和手动构建都必须保留可下载 EXE artifact；发布必须等待 Linux 和 Windows 检查通过。Linux 核心测试不替代 Windows 原生文件保护和实机验收。
- 不因完成开发自动 Commit、Push、创建 PR 或 Tag，等待用户明确安排。用户明确推送匹配源码版本的 `vX.Y.Z` Tag 后，由 CI 在 Linux / Windows 检查及资产校验通过后自动创建并发布 Release；开发会话不自行创建或发布 Release。不将真实数据库、游戏、存档或 `data/` 放入云端、缓存或构建资产。
- 开发和覆盖升级不备份正式数据库。用户已要求删除历史数据库备份；不要重新创建数据库、WAL、SHM 的副本或可恢复的记录导出，除非用户之后明确要求。
- 升级前后可用只读查询检查记录数量、schema、完整性，以及按旧列计算的不可恢复校验摘要。测试使用独立临时夹具。
- 优先用终端、脚本、API、文件和进程检查完成操作。GUI 自动化必须获得用户对当前任务的明确授权。
- 终止可能影响用户工作的进程需先获得授权；覆盖升级仅关闭本项目的指定应用进程。

## 仅限现有 Windows 本机工作区

- 本机日常程序为 `E:\butter-manager\butter-manager.exe`，正式资料为旁边完整的 `data\`；开发仓库在 `E:\butter-manager\dev\`。这些路径不是云端前提，不要从构建输出启动新的空库代替正式库。
- 本机成功构建后，用户已授权覆盖根目录 EXE 并重新打开；先按可执行文件路径确认管理器归属，尝试正常关闭，必要时可强制退出指定管理器，不终止游戏或其他应用。只替换 EXE，不碰正式 `data\`；构建失败不覆盖。云端只交付 artifact，不尝试远程覆盖本机程序。
- Rust 与 Windows 编译工具使用全局安装；开发缓存留在仓库 `.tools\` / `node_modules\` / `src-tauri\target\`。不主动清理整个本机开发目录，需用户另行安排。
