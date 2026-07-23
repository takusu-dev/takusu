-- Similar-task pre-filtering (#942): store an NFKC-normalized title so candidate
-- rows can be narrowed in SQL before bigram scoring. D1 cannot run NFKC during a
-- migration, so existing rows keep NULL and the similar-task query falls back to
-- matching the raw title for them; rows written after this migration populate it.
ALTER TABLE tasks ADD COLUMN normalized_title TEXT;
CREATE INDEX IF NOT EXISTS idx_tasks_normalized_title ON tasks(normalized_title);
