-- Short numeric display ID for habits (issue #305).
-- Mirrors the tasks.display_id scheme: the internal id stays a UUID v7,
-- display_id is a short sequential number for human/voice reference ("h1").
ALTER TABLE habits ADD COLUMN display_id INTEGER NOT NULL DEFAULT 0;

-- Backfill existing rows with sequential numbers ordered by creation time.
UPDATE habits SET display_id = (
    SELECT COUNT(*) + 1 FROM habits h2
    WHERE h2.created_at < habits.created_at
       OR (h2.created_at = habits.created_at AND h2.id < habits.id)
);

-- Unique only for real (non-zero) display_ids.  The default 0 is used by
-- direct SQL inserts that bypass the application layer (e.g. test fixtures).
CREATE UNIQUE INDEX IF NOT EXISTS idx_habits_display_id ON habits(display_id) WHERE display_id != 0;

-- Monotonic display_id sequence — prevents reuse after habit deletion.
-- The sequence only moves forward; deleted display_ids are never recycled.
CREATE TABLE IF NOT EXISTS habit_display_id_seq (
    next_id INTEGER NOT NULL
);

-- Initialize from the current maximum display_id (or 1 if no habits exist).
INSERT INTO habit_display_id_seq (next_id)
SELECT COALESCE(MAX(display_id), 0) + 1 FROM habits
WHERE (SELECT COUNT(*) FROM habit_display_id_seq) = 0;
