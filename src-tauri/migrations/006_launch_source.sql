ALTER TABLE games ADD COLUMN launch_source TEXT NOT NULL DEFAULT 'legacy' CHECK(launch_source IN ('legacy','detected','manual'));
PRAGMA user_version = 6;
