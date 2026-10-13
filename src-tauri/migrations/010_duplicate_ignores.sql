CREATE TABLE duplicate_ignores (
    left_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    right_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (left_id, right_id),
    CHECK (left_id < right_id)
);
CREATE INDEX duplicate_ignores_right ON duplicate_ignores(right_id);
PRAGMA user_version = 10;
