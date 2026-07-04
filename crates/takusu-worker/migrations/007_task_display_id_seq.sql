-- Monotonic display_id sequence — prevents reuse after task deletion (#186).
-- The sequence only moves forward; deleted display_ids are never recycled.
CREATE TABLE IF NOT EXISTS task_display_id_seq (
    next_id INTEGER NOT NULL
);

-- Initialize from the current maximum display_id (or 1 if no tasks exist).
-- Use COUNT(*) = 0 as the guard (not NOT EXISTS): an aggregate without
-- GROUP BY always returns one row even when WHERE is false, so NOT EXISTS
-- would let the INSERT through on every restart and accumulate duplicates.
INSERT INTO task_display_id_seq (next_id)
SELECT COALESCE(MAX(display_id), 0) + 1 FROM tasks
WHERE (SELECT COUNT(*) FROM task_display_id_seq) = 0;
