CREATE TABLE import_memories (
    id TEXT PRIMARY KEY,
    game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    profile_key TEXT NOT NULL,
    profile TEXT NOT NULL,
    confirmed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(game_id, profile_key)
);
CREATE INDEX import_memories_game ON import_memories(game_id);
PRAGMA user_version = 12;
