-- Rename legacy habit_pauses table to habit_scheduled_spans (#503).
-- The semantics are no longer purely "pause": for active habits the span is a
-- pause, for inactive habits it is an activation window.
ALTER TABLE habit_pauses RENAME TO habit_scheduled_spans;

-- Recreate the index under the new table name.
DROP INDEX IF EXISTS idx_habit_pauses_habit;
CREATE INDEX IF NOT EXISTS idx_habit_scheduled_spans_habit ON habit_scheduled_spans(habit_id);
