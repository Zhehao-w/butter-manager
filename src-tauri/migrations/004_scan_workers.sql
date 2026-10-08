ALTER TABLE settings ADD COLUMN scan_workers INTEGER NOT NULL DEFAULT 2 CHECK (scan_workers IN (1, 2, 4));
PRAGMA user_version = 4;
