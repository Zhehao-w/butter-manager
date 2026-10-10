# 应用图像与图标

- `app-icon.png`：原版应用图标透明 master 的 256px 转换。完整来源和 master 哈希见 [原生图标说明](../../src-tauri/icons/README.md)。
- `app-icon-new.png`：2026-10-08 用户提供的新版透明 PNG 的 256px 转换，与新版原生窗口和 EXE 图标使用同一来源。
- `sidebar-character-new.png`：2026-10-10 使用内置 imagegen，参考用户提供的主界面图片生成的第二套透明立绘，包含粉发人物、蓝白服装、蝶翼和爱心装饰。用户选择保留第一版蝴蝶立绘；应用现固定使用新版图标和此立绘，不再提供外观切换。
- `header-decoration.png`：同日使用内置 imagegen 生成的第一版透明顶部装饰，包含蝴蝶、爱心、手写 butter 与浅色光晕。
- `header-butterfly.png`：同日从第一版顶部装饰参考生成的独立透明蝴蝶，用于放大右侧装饰而不改变原图中的文字。
- 以上三张生成素材的最终提示词见 [生成提示词](art-prompts.json)。透明度和位置由样式控制，不从远端下载资源。
- `sidebar-character.png`：Zhehao-w 提供的 GPT 生成侧栏插画。
- 原版两张应用图像的提供者于 2026-10-08 确认允许随本项目公开发布。新版应用图标为本次功能开发提供的附件，提供者于同日要求将全部改动推送至 GitHub，包含该资源；新版应用图标的生成方式未单独记录。程序不从远端下载这些图像。
- `lucide/`：官方开源 SVG、下载来源清单和上游许可，见 [Lucide 说明](lucide/README.md)。
- `third-party/`：随程序打包的其他必要许可文本。

第三方资源与项目自有图像分别记录来源，详见 [第三方说明](../../THIRD_PARTY_NOTICES.md)。
