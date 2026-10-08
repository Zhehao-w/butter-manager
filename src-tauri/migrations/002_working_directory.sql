ALTER TABLE games ADD COLUMN working_directory TEXT NOT NULL DEFAULT '.';
PRAGMA user_version = 2;
