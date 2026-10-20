ALTER TABLE games ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1));
CREATE TABLE library_tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE
);
CREATE TABLE game_tags (
    game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES library_tags(id) ON DELETE CASCADE,
    PRIMARY KEY(game_id,tag_id)
);
CREATE INDEX game_tags_by_tag ON game_tags(tag_id,game_id);
PRAGMA user_version = 13;
