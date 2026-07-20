-- WI-9: active-session progress management
ALTER TABLE tasks ADD COLUMN quantity_total INTEGER;
ALTER TABLE tasks ADD COLUMN quantity_done INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN quantity_unit TEXT;
ALTER TABLE tasks ADD COLUMN completed_at TEXT;
ALTER TABLE tasks ADD COLUMN split_from_task_id TEXT REFERENCES tasks(id);
ALTER TABLE tasks ADD COLUMN original_quantity_total INTEGER;

CREATE TABLE IF NOT EXISTS task_work_sessions (
    id         TEXT PRIMARY KEY,
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    ended_at   TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_task_work_sessions_task ON task_work_sessions(task_id);

-- Only one open session per task to avoid duplicate work sessions from races.
CREATE UNIQUE INDEX IF NOT EXISTS idx_task_work_sessions_open_per_task
    ON task_work_sessions(task_id) WHERE ended_at IS NULL;

CREATE TABLE IF NOT EXISTS progress_events (
    id             TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    at             TEXT NOT NULL DEFAULT (datetime('now')),
    quantity_done  INTEGER,
    delta_quantity INTEGER,
    active_minutes INTEGER NOT NULL,
    note           TEXT
);

CREATE INDEX IF NOT EXISTS idx_progress_events_task ON progress_events(task_id);

-- Idempotency receipts for active-session progress operations.
CREATE TABLE IF NOT EXISTS progress_operations (
    operation_id     TEXT PRIMARY KEY,
    request_hash     TEXT NOT NULL,
    response_json    TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_progress_operations_created_at
    ON progress_operations(created_at);
