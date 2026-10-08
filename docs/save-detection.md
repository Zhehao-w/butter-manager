# 存档扫描规则

扫描目录、单独分析和新游戏导入共用 `src-tauri/src/save_detection.rs`。路径字典在 `src-tauri/src/save_dictionary.json`；不改变数据库 schema，复用已有 `save_paths` 表。

游戏内部路径使用 `<GAME>/相对路径`。登记/导入时重新确认实际目标目录；完整扫描会补全尚未配置存档的库记录，保留已有手动配置。

| 引擎                              | 扫描规则                                                                                                                                                                            |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 通用 / Unity / 吉里吉里等本地分发 | 主目录、启动文件目录下实际存在的 `save`、`saves`、`SaveData`、`save_data`、`SaveGame(s)`、`SavedGames`；同时检查 `www` / `game` 下一层                                              |
| RPG Maker MV / MZ                 | `save` / `www/save` 等本地目录；主目录和启动目录已有的 `.rpgsave` / `.rmmzsave` 文件                                                                                                |
| RPG Maker XP / VX / VX Ace / 2000 | `Save数字.rxdata` / `.rvdata` / `.rvdata2` / `.lsd` 文件；不把 Actors/Map 等游戏数据当存档                                                                                          |
| QSP                               | 常规本地存档目录和实际存在的 `.sav` 文件；QSP 可以自定义路径，不推断 `.qsp` 脚本内部路径                                                                                            |
| Ren’Py                            | **本地 `game/saves` 存在时，以它为唯一自动识别位置**。否则，只读取 `game/options.rpy` 中唯一的静态 `config.save_directory` 字符串，匹配已有 `%APPDATA%/RenPy/标识`；不会执行脚本    |
| Godot                             | 从未打包的 `project.godot` 读取 `[application]` 的项目名称及 custom user dir 设置，匹配已有 `%APPDATA%/Godot/app_userdata/项目名` 或明确的自定义目录                                |
| Unity                             | 游戏包含与所选 EXE 对应的 `*_Data/app.info`，且两行明确表示厂商 / 产品时，使用其作为路径线索，匹配已有 `%USERPROFILE%/AppData/LocalLow/厂商/产品`；忽略 DefaultCompany 等公共默认名 |
| NW.js                             | 字典记录 `%LOCALAPPDATA%/package.json 的 name`，但不自动把整个 Chromium 数据目录视为存档，因为它包含缓存及其他应用数据                                                              |

只认实际存在的路径；不全盘查找，不执行游戏，不读取存档内容，不解析 `.rpyc` / `.pck`，不处理注册表 PlayerPrefs、Steam Cloud 或浏览器 IndexedDB。只读配置文本上限 256 KiB；跳过链接 / junction，外部路径须由明确元数据组成。Unity app.info 是分发包线索，不等于所有 Unity 游戏都采用该默认目录；定制游戏仍可手工修改。

来源（2026-10-07 查阅）：

- [RPG Maker MV 官方 CoreScript：StorageManager](https://github.com/rpgtkoolmv/corescript/blob/master/js/rpg_managers/StorageManager.js)：本地 save 目录、rpgsave 文件与 www 目录限制回退。
- [Ren’Py config.save_directory](https://www.renpy.org/doc/html/config.html#var-config.save_directory)：本地 game/saves 及 Windows RenPy 用户目录。
- [Godot data paths](https://docs.godotengine.org/en/stable/tutorials/io/data_paths.html)：默认 user:// 与 custom user dir。
- [Unity persistentDataPath](https://docs.unity.com/en-us/engine/6000.0/script-reference/unityengine/application/persistentdatapath)：Windows LocalLow/Company/Product 默认路径。
- [QSP SAVEGAME](https://dev.qsp.org/docs/language/qsp-keywords/qsp-keywords-statements/#savegame)：游戏或播放器指定保存文件，示例 .sav。
- [EasyRPG LcfSaveData](https://wiki.easyrpg.org/development/data-structure-reference/lcfsavedata)：Save%02d.lsd。
- [NW.js App.dataPath](https://docs.nwjs.io/References/App/#appdatapath)：package name 对应的应用数据目录。
