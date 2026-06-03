CREATE TABLE IF NOT EXISTS tokens (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    token_hash  TEXT NOT NULL UNIQUE,
    label       TEXT,
    created_by  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at  TEXT
);

CREATE TABLE IF NOT EXISTS habits (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT,
    recurrence  TEXT NOT NULL,
    start_time  TEXT NOT NULL,
    end_time    TEXT NOT NULL,
    avg_minutes INTEGER NOT NULL,
    sigma_minutes INTEGER NOT NULL DEFAULT 0,
    parallelizable   BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel  BOOLEAN NOT NULL DEFAULT 0,
    abandonability   REAL NOT NULL DEFAULT 0.0,
    active      BOOLEAN NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tasks (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT,
    start_at    TEXT,
    end_at      TEXT NOT NULL,
    avg_minutes INTEGER NOT NULL,
    sigma_minutes INTEGER NOT NULL DEFAULT 0,
    depends     TEXT NOT NULL DEFAULT '[]',
    parallelizable   BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel  BOOLEAN NOT NULL DEFAULT 0,
    abandonability   REAL NOT NULL DEFAULT 0.5,
    status      TEXT NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending','scheduled','in_progress','completed','skipped')),
    habit_id    TEXT REFERENCES habits(id),
    ical_uid    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_ical_uid ON tasks(ical_uid) WHERE ical_uid IS NOT NULL;

CREATE TABLE IF NOT EXISTS schedules (
    id          TEXT PRIMARY KEY DEFAULT 'active',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    schedule    TEXT NOT NULL
);