-- Similar-task pre-filtering (#942): store an NFKC-normalized title so candidate
-- rows can be narrowed in SQL before bigram scoring. Normalization runs in Rust
-- (SQLite has no NFKC), so existing rows are backfilled in init() after this ALTER.
ALTER TABLE tasks ADD COLUMN normalized_title TEXT;
CREATE INDEX IF NOT EXISTS idx_tasks_normalized_title ON tasks(normalized_title);
