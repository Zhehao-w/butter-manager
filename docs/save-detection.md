# 存档扫描规则

扫描目录、单独分析和新游戏导入共用 `src-tauri/src/save_detection.rs`。路径字典在 `src-tauri/src/save_dictionary.json`；不改变数据库 schema，复用已有 `save_paths` 表。

游戏内部路径使用 `<GAME>/相对路径`。登记/导入时重新确认实际目标目录；手动完整扫描会按已登记游戏实际生效的引擎、所选启动文件及工作目录补全空存档配置，不依赖扫描默认候选是否发现存档。只补空配置，保留已有手动路径；不会在每次打开管理器时自动扫描，也不会自动替换已有存档关联。

详情页的「分析游戏配置」也识别启动文件、引擎和存档位置，分析后保留当前 Tab。有效的已选启动文件按已保存的引擎及工作目录检测存档；找不到有效启动文件时沿用重新识别结果。分析可填入空草稿。对于已有存档位置的 Ren’Py 游戏，分析也可补入新识别的位置，保留手动配置。点击「保存资料」后生效。打开存档编辑器时临时识别的有效位置也会补入空详情草稿，即使槽位加密或无法解码，位置仍可显示。

| 引擎                              | 扫描规则                                                                                                                                                                            |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 通用 / Unity / 吉里吉里等本地分发 | 主目录、启动文件目录下实际存在的 `save`、`saves`、`SaveData`、`save_data`、`SaveGame(s)`、`SavedGames`；同时检查 `www` / `game` 下一层                                              |
| RPG Maker MV / MZ                 | `save` / `www/save` 等本地目录；主目录和启动目录已有的 `.rpgsave` / `.rmmzsave` 文件                                                                                                |
| RPG Maker XP / VX / VX Ace / 2000 | `Save数字.rxdata` / `.rvdata` / `.rvdata2` / `.lsd` 文件；不把 Actors/Map 等游戏数据当存档                                                                                          |
| QSP                               | 常规本地存档目录和实际存在的 `.sav` 文件；QSP 可以自定义路径，不推断 `.qsp` 脚本内部路径                                                                                            |
| Ren’Py                            | 同时识别游戏内的 `game/saves` 和已有的 `%APPDATA%/RenPy/标识`。标识取自 `game/options.rpy` 中明确的 `config.save_directory`。编辑器优先打开游戏目录中的存档。 |
| Godot                             | 从未打包的 `project.godot` 读取 `[application]` 的项目名称及 custom user dir 设置，匹配已有 `%APPDATA%/Godot/app_userdata/项目名` 或明确的自定义目录                                |
| Unity                             | 游戏包含与所选 EXE 对应的 `*_Data/app.info`，且两行明确表示厂商 / 产品时，使用其作为路径线索，匹配已有 `%USERPROFILE%/AppData/LocalLow/厂商/产品`；忽略 DefaultCompany 等公共默认名 |
| NW.js                             | 字典记录 `%LOCALAPPDATA%/package.json 的 name`，但不自动把整个 Chromium 数据目录视为存档，因为它包含缓存及其他应用数据                                                              |

Unity 还会根据所选 EXE 精确检查同目录下的 `EXE名_Data/SaveData`、`Saves` 等白名单子目录，支持启动文件位于游戏内子目录的情况。不需要 app.info，不检查其他 EXE 的 `_Data` 或资源深层目录，不改变现有 LocalLow 识别规则。

只认实际存在的路径；不全盘查找，不执行游戏，不读取存档内容，不解析 `.rpyc` / `.pck`，不处理注册表 PlayerPrefs、Steam Cloud 或浏览器 IndexedDB。只读配置文本上限 256 KiB；跳过链接 / junction，外部路径须由明确元数据组成。Unity app.info 是分发包线索，不等于所有 Unity 游戏都采用该默认目录；定制游戏仍可手工修改。

### 在详情中查找已游玩游戏的存档

「存档位置 → 快速查找存档」与「分析游戏配置」共用检测规则。快速查找读取游戏已保存的配置，返回引擎匹配的目录或槽位。「加入位置」会立即保存该候选，详情里其他手动修改仍可继续编辑。手工选择目录和逐行编辑路径沿用「保存资料」流程。

已有 Ren’Py 记录也可以通过分析把缺少的位置补进存档草稿。若发行包没有可读取的存档标识，详情查找会比对本地槽位与 Roaming 中同名槽位的文件大小和写入时间，匹配项供手动确认。删除游戏目录时，确认框会一并列出明确识别的专属 AppData 目录和已关联的位置。共享存档会阻止回收，单独从库中移除记录会保留两处文件。

Unity 优先核对与所选 EXE 匹配的 LocalLow 产品目录，包括 DefaultCompany 下有明确产品名的目录。精确目录缺少存档文件时，再限量读取其他产品目录的 Player.log 开头并核对安装路径。同等匹配依据下优先展示 Save、SaveData 等目录。只有日志的目录与 PlayerPrefs 注册表项会列出线索。

自定义路径可用「存档位置 → 通过一次存档定位」。先在游戏内准备好保存，开始定位后保存一次，再点「完成定位」。管理器使用系统文件通知记录游戏目录、LocalLow、Local 和 Roaming 的同期变化，过滤常见日志及缓存。界面显示实际已监听的范围。换槽后再定位一次，两次均有写入的位置优先。监测在三分钟后自动结束，取消或关闭详情也会释放监听。文件通知无法识别写入进程，选择前可核对路径和文件名。

来源（2026-10-07 查阅）：

- [RPG Maker MV 官方 CoreScript：StorageManager](https://github.com/rpgtkoolmv/corescript/blob/master/js/rpg_managers/StorageManager.js)：本地 save 目录、rpgsave 文件与 www 目录限制回退。
- [Ren’Py config.save_directory](https://www.renpy.org/doc/html/config.html#var-config.save_directory)：本地 game/saves 及 Windows RenPy 用户目录。
- [Godot data paths](https://docs.godotengine.org/en/stable/tutorials/io/data_paths.html)：默认 user:// 与 custom user dir。
- [Unity persistentDataPath](https://docs.unity.com/en-us/engine/6000.0/script-reference/unityengine/application/persistentdatapath)：Windows LocalLow/Company/Product 默认路径。
- [QSP SAVEGAME](https://dev.qsp.org/docs/language/qsp-keywords/qsp-keywords-statements/#savegame)：游戏或播放器指定保存文件，示例 .sav。
- [EasyRPG LcfSaveData](https://wiki.easyrpg.org/development/data-structure-reference/lcfsavedata)：Save%02d.lsd。
- [NW.js App.dataPath](https://docs.nwjs.io/References/App/#appdatapath)：package name 对应的应用数据目录。
