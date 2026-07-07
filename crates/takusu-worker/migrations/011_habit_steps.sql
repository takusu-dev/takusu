-- Habit steps (#95): generate multiple dependent tasks from a single habit.
-- A habit with one or more steps ignores its own window/avg/sigma/flags and
-- instead emits one task per step per occurrence, each with its own
-- start_time/end_time/avg/sigma/flags. Steps within a habit form a DAG via
-- `depends_on` (JSON array of step ids within the same habit).
CREATE TABLE IF NOT EXISTS habit_steps (
    id              TEXT PRIMARY KEY,
    habit_id        TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,      -- display order
    title           TEXT NOT NULL,
    description     TEXT,
    start_time      TEXT NOT NULL,         -- HH:MM (per-step window)
    end_time        TEXT NOT NULL,
    avg_minutes     INTEGER NOT NULL,
    sigma_minutes   INTEGER NOT NULL DEFAULT 0,
    parallelizable  BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel BOOLEAN NOT NULL DEFAULT 0,
    abandonability  REAL NOT NULL DEFAULT 0.0,
    fixed           BOOLEAN NOT NULL DEFAULT 0,
    depends_on      TEXT NOT NULL DEFAULT '[]',  -- JSON array of step ids (DAG)
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_habit_steps_habit ON habit_steps(habit_id);

-- Link generated tasks back to the step that produced them. NULL for simple
-- (step-less) habits and manually created tasks.
ALTER TABLE tasks ADD COLUMN habit_step_id TEXT REFERENCES habit_steps(id);
