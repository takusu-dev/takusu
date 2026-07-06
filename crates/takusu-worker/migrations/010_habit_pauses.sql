-- Habit pause periods (#303).
-- A pause suppresses task generation for a habit during a (start_date,
-- end_date) range (inclusive, user-tz local YYYY-MM-DD). Multiple pauses
-- per habit are allowed. Pauses are independent of the habit's `active`
-- flag — an inactive habit generates nothing regardless of pauses.
CREATE TABLE IF NOT EXISTS habit_pauses (
    id         TEXT PRIMARY KEY,
    habit_id   TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    start_date TEXT NOT NULL,  -- YYYY-MM-DD (inclusive, user-tz local date)
    end_date   TEXT NOT NULL,
    reason     TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_habit_pauses_habit ON habit_pauses(habit_id);
