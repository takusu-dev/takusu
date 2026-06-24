CREATE TABLE IF NOT EXISTS settings (
    id          TEXT PRIMARY KEY DEFAULT 'active',
    tz          TEXT NOT NULL DEFAULT 'UTC',
    sleep_start TEXT NOT NULL DEFAULT '22:00',
    sleep_end   TEXT NOT NULL DEFAULT '06:00',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO settings (id, tz, sleep_start, sleep_end)
    VALUES ('active', 'UTC', '22:00', '06:00');