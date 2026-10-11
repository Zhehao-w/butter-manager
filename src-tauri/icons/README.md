# Application icon

The application uses the new user-provided icon consistently in the UI, native window and EXE. There is no appearance picker or persisted icon preference.

`source-icon-new.png` is the unchanged transparent PNG provided by the user on 2026-10-08. `src/assets/app-icon-new.png` (256px), `icon.png` (128px), `icon.ico` (16/24/32/48/64/128/256px), and `icon-new.rgba` (128px) are size/format conversions preserving alpha. RGBA stores raw 128 × 128 × 4 bytes for the native window icon, avoiding a runtime PNG decoder. No artwork editing is applied.

The provider requested publishing all changes, including this resource, on 2026-10-08. The attachment's generation method was not separately recorded. See [artwork notices](../../THIRD_PARTY_NOTICES.md) and [current appearance](../../docs/appearance.md).

Retired original icon conversions and sources have been removed. The original approved transparent master's SHA-256 was `78289afdecb4c619e6d2d5f26742fba305eae442212b8569314c79305c140ecf`; it is retained here as provenance only.
