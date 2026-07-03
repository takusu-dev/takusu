-- Track whether a user directly edited a task derived from a habit.
-- If true, sync_habit_tasks will not overwrite the task's content.
ALTER TABLE tasks ADD COLUMN user_edited BOOLEAN NOT NULL DEFAULT 0;
