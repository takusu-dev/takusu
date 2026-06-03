CREATE TABLE IF NOT EXISTS google_cal_settings (
    id            TEXT PRIMARY KEY DEFAULT 'active',
    enabled       BOOLEAN NOT NULL DEFAULT 0,
    calendar_id   TEXT NOT NULL DEFAULT 'primary',
    client_id     TEXT NOT NULL DEFAULT '',
    client_secret TEXT NOT NULL DEFAULT '',
    refresh_token TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS google_cal_events (
    task_id         TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    google_event_id TEXT NOT NULL,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);