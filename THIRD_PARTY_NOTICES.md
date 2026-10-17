# 资源来源与第三方许可

项目采用 [MIT](LICENSE)。第三方资源继续适用其原许可，不能将项目 MIT 署名替换其原作者署名。

| 资源                      | 来源                                                                             | 许可文本                                                   |
| ------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Lucide 0.468.0 SVG        | [lucide-icons/lucide](https://github.com/lucide-icons/lucide/tree/0.468.0/icons) | [ISC](src/assets/lucide/LICENSE)                           |
| Feather 衍生图标          | [Feather v4.29.2](https://github.com/feathericons/feather/tree/v4.29.2)          | [MIT](src/assets/lucide/FEATHER-LICENSE)                   |
| TanStack Virtual 许可文本 | [TanStack Virtual](https://github.com/TanStack/virtual)                          | [MIT](src/assets/third-party/tanstack-virtual-LICENSE.txt) |

Lucide 图标来源地址与 SHA-256 保存在 [sources.json](src/assets/lucide/sources.json)，图标及许可随程序本地打包。许可文本也可在设置页底部查看。

应用图标原图与侧栏插画由 Zhehao-w 使用 GPT 生成；作者于 2026-10-08 确认允许随 GitHub 项目公开发布。应用图标透明 master 及尺寸转换记录见 [图标说明](src-tauri/icons/README.md)，侧栏插画见 [资源说明](src/assets/README.md)。不包含游戏资源、MTool 或 QSP 播放器程序。

本文记录仓库直接打包的资源。Node.js 与 Rust 依赖各自保留其上游许可，可通过锁文件追溯对应版本。

Save Editor Lite 新增 Rust 依赖 `lz-str`（MIT）、`zip`（MIT）、`flate2`、`base64`、`sha1`、`sha2`、`p256`（后五者 MIT OR Apache-2.0），版本及传递依赖见 `src-tauri/Cargo.lock`。实现参考 RPG Maker StorageManager 与 Ren’Py 存档格式，不打包游戏引擎源代码或 Python 运行环境。测试夹具只包含合成数据与测试专用密钥。

2026-10-08 外观切换功能新增用户提供的两张透明 PNG。当前固定使用新版，旧版素材已清理；提供者于同日要求将全部改动推送至 GitHub，包含这些资源。新版附件的生成方式未单独记录，见 [资源说明](src/assets/README.md)。

Unity 文件编辑新增 RustCrypto `aes` / `cbc` / `pbkdf2`（MIT OR Apache-2.0），以及 `rand_core`（MIT OR Apache-2.0）与 `zeroize`（Apache-2.0 OR MIT），对应版本和传递依赖见锁文件。参考公开 ES3 容器参数、Naninovel 状态格式及 raw DEFLATE 封装，编解码和字段补丁自行实现；未纳入上游编辑器、商业 SDK 源码、游戏程序集或真实存档。新夹具由本仓库 Node 脚本生成人工数据与测试密码。

新增 Unity 编解码及相关依赖的 MIT 许可原文保存在 [unity-save-LICENSES.txt](src/assets/third-party/unity-save-LICENSES.txt)，并随应用嵌入“设置 → 依赖组件 · 许可说明”。
