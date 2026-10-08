ALTER TABLE games ADD COLUMN engine_source TEXT NOT NULL DEFAULT 'detected'
    CHECK (engine_source IN ('detected', 'manual'));
PRAGMA user_version = 5;
