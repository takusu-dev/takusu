-- Habit-specific display_id sequences (#380).
-- Each habit has its own monotonic sequence for task display_ids,
-- so habit tasks can use the format "h{habit_display_id}#{task_display_id}"
-- without colliding with normal task display_ids or other habits.
CREATE TABLE IF NOT EXISTS habit_task_display_id_seq (
    habit_id TEXT NOT NULL PRIMARY KEY,
    next_id INTEGER NOT NULL
);

-- Indexes scoped to each domain:
-- - non-habit tasks: unique on display_id alone
-- - habit tasks: unique on (habit_id, display_id)
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_display_id
    ON tasks(display_id)
    WHERE display_id != 0 AND habit_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_habit_tasks_display_id
    ON tasks(habit_id, display_id)
    WHERE habit_id IS NOT NULL AND display_id != 0;
