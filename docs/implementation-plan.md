# v0.1 / v0.2 历史实施记录

本文保留当时的开发范围与方案，不代表当前行为。当前版本为 0.3.0，导入更新、内部存档保留、旧版回收及回退均已实现；以 [README](../README.md) 和 [v0.3 实施说明](v0.3-implementation.md) 为准。

## v0.2.0 当时的实施范围

已按用户确认完成批量新游戏导入，默认移动。导入复用扫描分析，不新增独立识别引擎；名称 / 别名 / 明确版本规范化用于只读匹配。已有记录只展示关联并阻止执行更新。存档候选识别、旧游戏替换、存档保护和更新历史集中到 v0.3。

操作边界、恢复语义和验证见 [v0.2-implementation.md](v0.2-implementation.md)。以下为 v0.1 历史实施记录。

# butter-manager · v0.1

历史版本 v0.1.17，已根据 [roadmap.md](roadmap.md) 实现首轮扫描性能与交互改进。实现边界见 [scan-optimization-plan.md](scan-optimization-plan.md)，录入与弹窗设计见 [v0.1-usability-plan.md](v0.1-usability-plan.md)，验证见 [validation.md](validation.md)。

新增 migration 002 为 games 增加 working_directory；旧记录保持原根目录启动行为，新登记从所选 EXE 的父目录推断。扫描 / 登记由独立任务管理，状态不依赖数据库锁。默认只检查游戏根目录及一层子目录；更深分析显式发起。设置由侧栏进入页面，详情使用模态弹窗，库与扫描采用连续虚拟列表，Play 的等待状态保存在应用会话并按游戏区分。

当前阶段：本地游戏库和启动器。扫描及 BAT 分析只读取游戏文件，用户选择“加入游戏库”后仅写入本地数据库。没有更新、移动、删除、隔离或自动清理命令。

1. Tauri 2 + React + TypeScript 骨架；Rust domain 与 Tauri adapter 分离。
2. SQLite migration 001：games(UUID)、aliases、save_paths、单例 settings。保存非 SemVer 版本字符串、版本来源、相对 EXE 和明确的 MTool loader。
3. 只扫描 Game Root 的直属游戏目录；有限深度查找 EXE，排除辅助程序和 Tool；保留可编辑候选，跳过链接。
4. 表格、搜索、扫描预览、详情编辑、别名和手工存档路径；重复扫描按规范化安装路径幂等保存。
5. Rust 直接启动已登记的 EXE；可打开登记目录。路径必须落在游戏目录内。
6. 共享 MTool 设置、保守 BAT parser；只认已知模式，对未知命令及编码返回 unsupported，永不执行下载 BAT。当前默认按 PE 架构选择公共 32 / 64 位 loader，显式 BAT / 手工配置保留。
7. parser、扫描、路径验证、SQLite 幂等和事务测试；前端类型检查及构建、Rust 测试及桌面构建、人工审阅。

## 数据约定

- 游戏安装路径规范化后持久化；内部 identity 永远是 UUID，路径只是重复登记约束。
- EXE、MTool target、loader、injector、runtime 保存相对路径；运行时使用 PathBuf 组合并验证边界。
- 别名使用 Unicode NFKC 和小写进行基本归一化。本阶段不猜测不同标题属于同一游戏，不做跨语言匹配、模糊合并或版本大小比较。
- save_paths 保存用户输入字符串（例如 `<GAME>\save`、`%APPDATA%\xxx`）；本阶段仅配置，不读取、备份或搬移存档。
- SQLite 在 Tauri app_local_data_dir 下，启用 foreign_keys 和 WAL。扫描错误逐项展示，不无声丢弃。

## 后续边界

v0.2 才做 identity matching 与 Import Plan；v0.3 才做有恢复能力的更新事务；v0.4 才做 bundled MTool cleanup。当前 parser 提供 recipe，不据此清理任何文件。


## v0.1.2 增补迭代（2026-10-05）

已实现组合引擎签名、全库元数据补充、schema 3 运行事件与 last_launched_at、原始 created_at 展示、名称 / 时间排序，以及设置中的数据库清空确认。所有改动仍属于本地游戏库与启动器，不涉及游戏 / 存档文件删除、更新、游玩时长或外部启动跟踪。完整说明见 README、roadmap 和 validation。清空通过 SQLite 事务重建空 schema，随后压缩 / checkpoint；物理数据库文件保留。后台任务及未完成启动请求阻止清空，已提交清空的压缩失败会单独说明，避免界面保留旧记录。

## v0.1.3 界面与启动文件迭代（2026-10-05）

沿用 React / Tauri 和 schema 3，不新增数据库迁移。内部 `main_executable` 字段兼容保存相对启动文件路径，界面统一显示“启动文件”。手选文档贯通扫描覆盖、登记和详情编辑；MTool target 继续验证 EXE，自动扫描仍只建议 EXE。

EXE 通过 Rust 创建进程；文档在独立 STA 线程使用 ShellExecuteExW 的 open verb，等待启动请求结果后记录历史，失败不记录。不修改用户默认关联，QSP / GAM 等由用户已安装的关联播放器处理。使用 NOASYNC 和 FLAG_NO_UI，COM 初始化 / 释放保持在同一线程；实现依据 Microsoft 的 [ShellExecuteExW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw) 与 [SHELLEXECUTEINFOW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow) 文档。

扫描结果使用独立页面状态，顶部操作条保持可见，登记完成且本批查询成功才自动返回游戏库；后台任务在页面外持续显示进度和取消。会话内保留候选、手工覆盖、勾选、分页和页面滚动位置。未知版本只改变展示 / 编辑映射，内部 Unknown 语义和旧资料保持兼容。浅色样式覆盖主页面、详情 / 设置弹窗、输入框、按钮和整行悬停。

## v0.1.4 参考图方向（2026-10-06）

以用户图标与四张界面参考为视觉依据，细节以现有本地游戏管理功能为准。使用代码内的轻量 SVG 线条图标及确定性游戏占位标记，不接入封面抓取。应用图标仅转换格式 / 尺寸，保留原图。保留系统原生窗口控制，初始窗口调整为 1440 × 900，支持较小窗口布局。

主界面固定侧栏，扫描任务状态仍独立于页面显示；列表 / 卡片共用搜索、排序、分页与记录状态。游戏详情继续采用视口内大弹窗，避免选择后突然压缩列表；现有未保存确认、局部启动反馈、按需分析和历史保持。设置分类不卸载草稿字段，保留跨分类编辑；所有配置仍通过原有后端验证保存。

新增导入页只解释未来流程，不执行文件操作。明确后续更新由用户手动导入触发，先匹配已有游戏并确认处理计划，再保留存档更新，失败可恢复。参考图的自动更新 / 下载 / 账号 / 假状态没有引入。数据库 schema 3 和启动后端逻辑保持不变。

## v0.1.5 通用图标与反馈（2026-10-06）

下载 Lucide 官方固定版本 0.468.0 的 18 个 SVG，移除 React 内所有手绘 path。以 CSS mask 使用原始资源，统一 currentColor，不增加图标运行时依赖或外部请求；保存下载地址、SHA-256 和两类许可，并将许可文本打包进关于页。用户提供的应用图标保持。

修正 hover 通用 background shorthand 覆盖 gradient 的问题：统一只改变 background-color，主按钮保留同一透明渐变层。通知改为单个带递增 ID 的消息，普通 6 秒 / 错误 10 秒；暂停交互并在离开后重新计时，回调按 ID 清除，避免旧计时器清除新通知或旧内容再出现。不清空候选 / 手选值，也不自动离开扫描页面。

滚动条使用统一细窄蓝灰样式。Windows 11 原生标题栏由 DwmSetWindowAttribute 设置 caption / text 的 COLORREF，窗口仍保留系统控制；不支持时回退原生默认，窗口背景色与 WebView 浅蓝一致。实现依据 [Microsoft DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute) 的 CAPTION_COLOR / TEXT_COLOR 说明。


## v0.1.6 · 固定库工具栏与扫描线程持久化

窗口使用固定 viewport 高度，workspace/main 传递 min-height:0；游戏列表独立 flex 滚动容器，列头 sticky，其他页面在 main 中滚动。列表和扫描各自保存元素 scrollTop，返回页面恢复；筛选 / 排序 / 页码变化清零，滚动不会移动工具栏或分页。

线程设置属于游戏库设置而非每次扫描操作；新增 schema 4 的 scan_workers 列，默认 2 且 CHECK / Rust 验证限定 1 / 2 / 4。start_scan 从数据库读取已保存线程数，不接受前端临时覆盖。旧记录与历史不改变，清空库重建最新 schema 并恢复默认。

外部目录变更暂不做自动协调：扫描以安装路径判断已登记，新候选需确认入库；不自动移除缺失路径、不自动按名称合并，不覆盖已有资料。


## v0.1.7 · 表头固定与圆角

v0.1.6 的 th sticky 位于带内边距 / border-spacing 的滚动表格内，首次滚动会先吃掉顶部留白。本轮改为原生 table 的 thead 固定、tbody flex 滚动。thead 和每一行共享 CSS grid 列定义，媒体查询同步隐藏列并调整定义，scrollbar-gutter 对齐留位，横向滚动同步 thead.scrollLeft。卡片统一 overflow:hidden / 圆角裁切表头与内容，不再依赖 sticky 阈值。滚动记忆 ref 覆盖 tbody / 卡片区域，并在视图切换后恢复。


## v0.1.8 · 紧凑库底栏

此前底部分页上下 margin、root-path 的 20px 上边距、app-footer 的 24px 上边距与 workspace 底部 padding 叠加。改用 library-footer 一行承载目录 / 分页，清除内部旧 margin，目录标签 / 路径拆分，标签保留、路径 min-width:0 / ellipsis / title，增加 4px 横向安全内边距。库页不再另显示本地管理 footer，扫描 / 导入页保留说明但减小间距；viewport 和表头固定逻辑保持。


## v0.1.9 · 用户批准图标 / 连续滚动

以用户批准的第二版内置 imagegen 输出为唯一透明 master，原样复制到项目；PNG / ICO 转换尺寸和格式，保留 alpha，不再生成或改绘图像。原 JPEG 保留。ICO 有 16 / 24 / 32 / 48 / 64 / 128 / 256 尺寸，16 像素的 Lanczos 角点仅有 1/255 的抗锯齿 alpha，其余角点为 0。

游戏库删除 libraryPage 及上下分页，仅保留扫描结果分页。采用官方 @tanstack/react-virtual 3.14.13（核心 3.17.11，MIT 通知打包并在关于可查看）；超过 50 个时虚拟化，小结果直接渲染。列表滚动主体保持 tbody，使用不参与交互的高度占位行与绝对定位可见行，真实测量行高且 UUID 作为缓存 key；thead 仍为独立固定区域。卡片以窗口宽度 / 210px 最小宽度 / 16px 间距确定每行列数，以整行最高卡片测量高度，ResizeObserver / resize 改变列数并重新测量。搜索与排序清零滚动，页面 / 视图返回恢复位置。底栏显示库与筛选数量。

参考官方说明：https://tanstack.com/virtual/latest/docs/introduction 和 https://tanstack.com/virtual/latest/docs/api/virtualizer 。


## v0.1.10 · 详情与扫描圆角、设置页面

详情 dialog 采用 flex 固定标题与内部滚动：外壳 overflow hidden，内层有左右 8 / 底部 12 像素安全边距。扫描页为高度受控的 flex 外壳；工具栏、两组分页在滚动列表之外，取消 sticky，不再与候选重叠。扫描滚动记忆迁移到候选列表本身，翻页重置滚动。库底栏为计数增加安全边距与明确行高。

设置内容移入 main，MTool / 游戏库设置 / 关于共用同一份草稿并同步侧栏选择，保存保留页面，规范化后的结果回写草稿；离开页面经导航保护确认未保存编辑。原生目录选择器复用于存档位置，取消不改变草稿，失败显示详情消息，重复路径去重。

引擎编辑使用常见引擎 datalist 并允许自定义，数据库验证非空、无控制字符、最长 80 字符。迁移 005 增加 engine_source，现有记录默认 detected；引擎值变更标为 manual，自动补充保留 manual（包括人工 Unknown），不影响自动补齐版本。清空库重建到 schema 5。图标仅用既有 Lucide SVG，由引擎映射浅色背景；未知与自定义其他引擎灰色。


## v0.1.11 · 统一导航 / 扫描虚拟列表

移除所有返回按钮、上次扫描快捷入口、设置横向分类和库中批量补充按钮。MTool / 设置共用草稿，侧栏区分当前页；关于不显示表单。扫描侧栏只打开页面，工具栏按钮启动或重扫。入库后不跳页，用返回记录更新库和候选的登记标志、清除已提交选择，并刷新扫描快照。

ScanCandidate 增加可空 directory_modified_ms，来自对应游戏目录的 metadata.modified，serde default 保持旧数据兼容，不增加数据库列。前端 scanResults 纯函数实现规范化搜索与六种排序；默认未入库优先、同组目录修改倒序与自然名称稳定排序，无修改时间放末尾。超过 50 条使用 TanStack Virtual，同库一样仅渲染附近条目，按 install_path 稳定键测量行高；手动展开状态从行组件提升到页面，避免虚拟卸载丢失。查询和排序变化回顶部，换页导航保留滚动；选择不受过滤影响并提示可见勾选数。已入库候选只读显示 saved Game 中的实际版本 / 启动文件。移除分页组件与无用样式。

完整 UI 逻辑 review 见 ui-review-v0.1.11.md。本轮不实现自动更新、目录迁移、库缺失检测或真实 MTool 兼容规则。没有使用 GUI 自动化。

## v0.1.12 · 扫描工具栏、路径维护与独立 MTool

搜索容器增加 position relative，扫描筛选搜索去掉通用底距；排序组 flex none / nowrap，选择器固定同输入高度并允许整组换行。

maintenance 模块进行目录与有效启动路径检查、关联计划预览、根目录内绝对存档路径重定位及 MTool 文件检查 / 独立进程启动。路径检查使用两个后台 worker、复用任务进度和取消机制；仅完成 / 取消后发送检查结果，不在每次轮询重复发送全库。启动 / 扫描完成自动检查，也可主动触发；未检查条目保留旧状态，不写 schema 或游戏文件。

单条删除在事务内清除 games 与该条 aliases / save_paths / launch_history。目录关联验证预览旧路径、规范化新目录唯一性及内部启动路径后，事务内仅更新目录 / 启动字段、时间戳与内部绝对存档路径；保留 ID 与其他资料。活动任务和启动请求期间禁止维护。前端提交前有明确确认，维护后递增库读取代次，刷新权威库快照与扫描标记，防止晚到登记查询恢复旧状态。关联入口既可在详情浏览目录，也可在扫描候选显式选择旧记录。

MTool 页面使用已保存配置，运行根目录固定 MTool.exe，以共享根目录为工作目录且不传游戏参数。检查按钮只验证主程序、injector、runtime 文件；游戏 loader 仍逐游戏验证，不引入自动兼容规则。关联目录不等于手动版本更新，不复制或移动存档文件。schema 保持 5。

## v0.1.13 · 公共 MTool 与离开确认

快速扫描只检查根目录生成 BAT 文件名，不解析内容、不读 PE。新登记选中 EXE 时设 MTOOL / target=main / loader=NULL / working_directory=.；文档仍用 Direct。完成扫描后同步已有默认 Direct 候选；迁移 006 增加 launch_source=legacy/detected/manual，启动相关人工编辑标记 manual，后续同步不覆盖。迁移本身不改旧配置，metadata-only 编辑不改变启动来源；不能追溯旧版本的人工启动选择。

详情使用全局设置的公共注入器和运行程序，移除重复 target 文本输入。loader 自动读取 PE：x86→loaders/mzHook32.dll，x64→loaders/mzHook.dll，其他架构需手选。只读预览验证目录边界和实际文件；真实启动先等待注入器成功退出，再运行公共 runtime，两个进程使用游戏的启动工作目录。标准 BAT 的工作目录为 BAT 所在目录，显式采用 recipe 时保留此语义。旧显式 target / loader 兼容保留，重新选择启动文件同步目标；高级工作目录折叠显示。

路径原因位于标题下方，筛选按钮使用 aria-pressed 与蓝色选中样式。未保存退出提示是独立原生 HTML dialog，Esc 只关闭确认层并保留草稿；MTool / 设置之间保留共享草稿无需确认。测试验证现有详情关闭及侧栏导航，不扩展为新的系统关窗拦截。

开发与覆盖升级不备份正式数据库，也不保存可恢复的行导出；已有备份按用户要求删除。升级前后以只读事务对全部旧列计算 SHA-256 和计数，迁移新增列单独检查。夹具数据库仍可用于自动化测试。

## v0.1.14 · 页面标题统一与 v0.1 收尾

新增 PageHeader 供游戏库、扫描入库、导入、MTool、设置、关于共用。使用现有 Lucide 图标及导入页的 64px 圆角背景、统一标题 / 说明字号、间距和右侧可换行操作区；不改变固定列表标题和连续虚拟滚动。扫描移除额外卡片标题外壳，与其他页面对齐。

设置 / MTool 的公共说明移入固定标题区，移除滚动区首行重复提示及空的固定高度状态栏。滚动内容增加上 / 左安全边距，说明文字可换行、足够行高；输入区与说明分开留白。无后端逻辑或 schema 改动。

用户报告真实 MTool 启动成功；这证明该游戏与公共配置的实际流程可用，不推断所有引擎 / 位数均兼容。v0.1 原定 Library + Launcher 功能已覆盖，目前没有已知阻止进入 v0.2 的功能 blocker；未知 BAT 编码 / 命令、更多游戏兼容及性能量化作为后续改进。下一阶段先做只读导入分析 / 匹配 / 计划，尚不自动执行文件更新。详细里程碑见 post-v0.1-plan.md。

## v0.1.15 · 滚动区域与内容起点

随后按用户要求扩展复查到所有滚动区域。别名 / 存档多行输入通过 text-scroll-shell 承担外层圆角与焦点框，内层 textarea 去掉圆角，留 8px 边距；许可 / 调试文本共用 CodeBlock 外层圆角背景 + 内层矩形 pre。详情弹窗已有 modal-scroll-shell 内缩，侧栏无圆角冲突，诊断 / 历史沿用已保护的父滚动区。导入标题固定，内容通过独立 import-scroll 滚动。逐项结论见 ui-review-v0.1.15.md。

圆角由实际卡片或表格外壳承担，透明网格容器及各滚动视口不再设置圆角。游戏库 grid 的上下原生滚动条与卡片不再受整片区域圆角裁切。table 本身仍裁切固定表头圆角，在底部留 18px 安全区，tbody 的原生滚动条到不了外壳圆角。虚拟列表的引用、测量和滚动记忆保持。

扫描去掉左侧 8px 内缩，扫描 / 库搜索至内容均留 18px；设置内容去掉 4px 左内缩 / 8px 顶内缩和首卡片 8px 外距，关于去掉首卡片 22px 外距。共用标题后 26px 为导入、MTool、设置、关于首内容起点；库 / 扫描在该起点放置固定搜索栏，卡片从搜索栏后相同间距开始，保留各自必需控件。

## v0.1.16 · 相同功能的 UI 一致性

SearchField / SortField 封装共同标记与回调，保留既有 accessible name 和业务选项。CSS 使用 control-height=40px、toolbar-control-height=44px、control-line-height=20px、control-radius=11px、toolbar-gap=12px；删除库 / 扫描独立的搜索内距、排序高度 / 标签规则。44px 搜索使用 11px 上下内距，明确为 20px 文本留位，不依赖容器继承行高。

BrowseButton 统一原生选择入口的图标、按钮类型、尺寸，实际选择 API 与事件保留。普通按钮、字段、选中态与错误样式复查；特殊的标题按钮 / 通知关闭 / 视图切换继续使用紧凑尺寸，主启动强调按钮保留 44px。没有更改保存、取消、确认或数据库操作流程。详情共用状态行保留稳定位置，避免启动反馈造成布局跳动。
