-- Partial index for split_from_task_id lookups during delete/split cleanup.
-- NULL values are excluded because most tasks are not split off from another task.
CREATE INDEX IF NOT EXISTS idx_tasks_split_from
    ON tasks(split_from_task_id) WHERE split_from_task_id IS NOT NULL;
