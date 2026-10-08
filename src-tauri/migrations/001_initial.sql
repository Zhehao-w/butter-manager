CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    game_root TEXT NOT NULL DEFAULT '',
    mtool_root TEXT NOT NULL DEFAULT '',
    mtool_injector TEXT NOT NULL DEFAULT 'loaders/inject.exe',
    mtool_runtime TEXT NOT NULL DEFAULT 'MTool.exe'
);
INSERT OR IGNORE INTO settings(id) VALUES (1);

CREATE TABLE IF NOT EXISTS games (
    id TEXT PRIMARY KEY,
    canonical_title TEXT NOT NULL,
    display_title TEXT NOT NULL,
    install_path TEXT NOT NULL,
    install_path_key TEXT NOT NULL UNIQUE,
    current_version TEXT NOT NULL DEFAULT 'Unknown',
    version_source TEXT NOT NULL DEFAULT 'manual',
    main_executable TEXT,
    engine TEXT NOT NULL DEFAULT 'Unknown',
    launch_type TEXT NOT NULL DEFAULT 'DIRECT' CHECK (launch_type IN ('DIRECT', 'MTOOL', 'CUSTOM_BAT')),
    mtool_target_exe TEXT,
    mtool_loader TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE IF NOT EXISTS aliases (
    id TEXT PRIMARY KEY,
    game_id TEXT NOT NULL REFERENCES games(id),
    alias TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'manual',
    UNIQUE(game_id, alias)
);
CREATE INDEX IF NOT EXISTS aliases_normalized ON aliases(normalized_alias);
CREATE TABLE IF NOT EXISTS save_paths (
    id TEXT PRIMARY KEY,
    game_id TEXT NOT NULL REFERENCES games(id),
    path TEXT NOT NULL,
    UNIQUE(game_id, path)
);
PRAGMA user_version = 1;
