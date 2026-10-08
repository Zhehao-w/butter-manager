ALTER TABLE games ADD COLUMN last_launched_at TEXT;
CREATE TABLE launch_history (
    id TEXT PRIMARY KEY,
    game_id TEXT NOT NULL REFERENCES games(id),
    launched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX launch_history_game_time ON launch_history(game_id, launched_at DESC);
PRAGMA user_version = 3;
