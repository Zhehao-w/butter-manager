-- Extend the launch CHECK; dependent tables retain their records and IDs.
CREATE TABLE games_external (
    id TEXT PRIMARY KEY,
    canonical_title TEXT NOT NULL,
    display_title TEXT NOT NULL,
    install_path TEXT NOT NULL,
    install_path_key TEXT NOT NULL UNIQUE,
    current_version TEXT NOT NULL DEFAULT 'Unknown',
    version_source TEXT NOT NULL DEFAULT 'manual',
    main_executable TEXT,
    engine TEXT NOT NULL DEFAULT 'Unknown',
    launch_type TEXT NOT NULL DEFAULT 'DIRECT' CHECK (launch_type IN ('DIRECT','MTOOL','CUSTOM_BAT','EXTERNAL_PLAYER')),
    mtool_target_exe TEXT,
    mtool_loader TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    working_directory TEXT NOT NULL DEFAULT '.',
    last_launched_at TEXT,
    engine_source TEXT NOT NULL DEFAULT 'detected' CHECK (engine_source IN ('detected','manual')),
    launch_source TEXT NOT NULL DEFAULT 'legacy' CHECK (launch_source IN ('legacy','detected','manual')),
    external_player TEXT
);
INSERT INTO games_external SELECT games.*, NULL FROM games;
DROP TABLE games;
ALTER TABLE games_external RENAME TO games;
PRAGMA user_version = 7;
