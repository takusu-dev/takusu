-- Habit-specific display_id sequences (#380).
-- Each habit has its own monotonic sequence for task display_ids,
-- so habit tasks can use the format "h{habit_display_id}#{task_display_id}"
-- without colliding with normal task display_ids or other habits.
CREATE TABLE IF NOT EXISTS habit_task_display_id_seq (
    habit_id TEXT NOT NULL PRIMARY KEY,
    next_id INTEGER NOT NULL
);

-- Drop the old global unique index so habit tasks can use per-habit sequences.
DROP INDEX IF EXISTS idx_tasks_display_id;

-- Renumber existing habit tasks so each habit starts from 1, ordered by
-- creation time (then id as tiebreaker). This gives the clean h1#1, h1#2, ...
-- numbering instead of retaining old global-sequence values (e.g. h1#47).
UPDATE tasks SET display_id = (
    SELECT COUNT(*) + 1 FROM tasks t2
    WHERE t2.habit_id = tasks.habit_id
      AND (t2.created_at < tasks.created_at
           OR (t2.created_at = tasks.created_at AND t2.id < tasks.id))
) WHERE habit_id IS NOT NULL;

-- Initialize sequences for existing habits based on max display_id.
-- Uses MAX (not COUNT) to avoid reusing display_ids after task deletion (#186).
INSERT OR IGNORE INTO habit_task_display_id_seq (habit_id, next_id)
SELECT
    habit_id,
    COALESCE(MAX(display_id), 0) + 1
FROM tasks
WHERE habit_id IS NOT NULL
GROUP BY habit_id;

-- Recreate indexes scoped to each domain:
-- - non-habit tasks: unique on display_id alone
-- - habit tasks: unique on (habit_id, display_id)
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_display_id
    ON tasks(display_id)
    WHERE display_id != 0 AND habit_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_habit_tasks_display_id
    ON tasks(habit_id, display_id)
    WHERE habit_id IS NOT NULL AND display_id != 0;
