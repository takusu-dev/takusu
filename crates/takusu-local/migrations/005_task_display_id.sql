-- Short numeric display ID for tasks (issue #42).
-- The internal id stays a UUID v7 (sortable, globally unique);
-- display_id is a short sequential number for human/voice reference.
ALTER TABLE tasks ADD COLUMN display_id INTEGER NOT NULL DEFAULT 0;

-- Backfill existing rows with sequential numbers ordered by creation time.
UPDATE tasks SET display_id = (
    SELECT COUNT(*) + 1 FROM tasks t2
    WHERE t2.created_at < tasks.created_at
       OR (t2.created_at = tasks.created_at AND t2.id < tasks.id)
);

-- Unique only for real (non-zero) display_ids.  The default 0 is used by
-- direct SQL inserts that bypass the application layer (e.g. test fixtures).
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_display_id ON tasks(display_id) WHERE display_id != 0;
