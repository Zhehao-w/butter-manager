ALTER TABLE games ADD COLUMN mtool_strategy TEXT NOT NULL DEFAULT 'INJECT' CHECK (mtool_strategy IN ('INJECT', 'BAKIN'));
PRAGMA user_version = 15;
