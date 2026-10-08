ALTER TABLE games ADD COLUMN play_status TEXT NOT NULL DEFAULT 'UNPLAYED'
    CHECK (play_status IN ('UNPLAYED', 'PLAYING', 'COMPLETED'));
UPDATE games SET play_status='PLAYING' WHERE last_launched_at IS NOT NULL;
PRAGMA user_version = 8;
