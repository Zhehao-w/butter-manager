ALTER TABLE duplicate_ignores RENAME TO duplicate_reviews;
ALTER TABLE duplicate_reviews ADD COLUMN status TEXT NOT NULL DEFAULT 'excluded'
    CHECK (status IN ('confirmed', 'excluded'));
DROP INDEX duplicate_ignores_right;
CREATE INDEX duplicate_reviews_right ON duplicate_reviews(right_id);
PRAGMA user_version = 11;
