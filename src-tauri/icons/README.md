# Application icon

The original user-provided image is preserved as `source-icon.jpg`.
Zhehao-w states that the original application icon and sidebar artwork were generated using GPT, and confirmed on 2026-10-08 that they may be publicly distributed with this project. See [third-party and artwork notices](../../THIRD_PARTY_NOTICES.md).
The transparent master is `source-icon-transparent.png`, approved by the user on 2026-10-06 after preview.
It is an unchanged copy of the second built-in imagegen background-extraction output.
Prompt intent: remove white canvas outside the original rounded square, preserve internal artwork and white details, retain genuine alpha transparency.

Approved master SHA-256: 78289afdecb4c619e6d2d5f26742fba305eae442212b8569314c79305c140ecf

The original UI PNG (`src/assets/app-icon.png`, 256px) and runtime RGBA (`icon-original.rgba`, 128px) are size/format conversions of that approved master.

`source-icon-new.png` is the unchanged transparent PNG provided by the user on 2026-10-08 for the new appearance option. `src/assets/app-icon-new.png` (256px), `icon.png` (128px), `icon.ico` (16/24/32/48/64/128/256px), and `icon-new.rgba` (128px) are size/format conversions preserving alpha. No artwork editing is applied. RGBA files contain raw 128 × 128 × 4 bytes for the native window icon, avoiding an additional runtime PNG decoder.

The executable embeds the new icon. Runtime window and in-app icons can independently select the original or new option in Settings. See [appearance behavior and future file-icon plan](../../docs/appearance.md). On 2026-10-08, the provider requested publishing all changes to GitHub, including the new assets. The new attachments' generation method has not been separately recorded.
