CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_habit_id ON tasks(habit_id);
CREATE INDEX IF NOT EXISTS idx_habits_created_at ON habits(created_at);
