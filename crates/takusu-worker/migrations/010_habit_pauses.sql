-- Legacy habit pause periods (#303).
-- This migration creates the original `habit_pauses` table. The table is
-- renamed to `habit_scheduled_spans` by migration `016` (#503).
--
-- Semantics after rename depend on `habits.active`:
-- - `active = true`: span dates suppress task generation (a pause).
-- - `active = false`: only span dates enable task generation (an activation
--   window); dates outside the span generate nothing.
CREATE TABLE IF NOT EXISTS habit_pauses (
    id         TEXT PRIMARY KEY,
    habit_id   TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    start_date TEXT NOT NULL,  -- YYYY-MM-DD (inclusive, user-tz local date)
    end_date   TEXT NOT NULL,
    reason     TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_habit_pauses_habit ON habit_pauses(habit_id);
