ALTER TABLE games ADD COLUMN launch_arguments TEXT NOT NULL DEFAULT '';
PRAGMA user_version = 14;
