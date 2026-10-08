CREATE TABLE version_history (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  operation TEXT NOT NULL UNIQUE,
  game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
  old_version TEXT NOT NULL,
  new_version TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('committed','rolled_back')),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX version_history_game ON version_history(game_id,sequence);
PRAGMA user_version = 9;
