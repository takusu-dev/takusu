-- Normalize legacy timestamp strings to whole-second RFC 3339 (YYYY-MM-DDTHH:MM:SSZ).
-- This covers the legacy `datetime('now')` output (`YYYY-MM-DD HH:MM:SS`) and
-- jiff fractional RFC 3339 output (`YYYY-MM-DDTHH:MM:SS.fffffffffZ`).

UPDATE tasks       SET created_at   = substr(replace(created_at,   ' ', 'T'), 1, 19) || 'Z' WHERE created_at   IS NOT NULL AND (created_at   GLOB '*[0-9] [0-9]*' OR created_at   GLOB '*.*Z');
UPDATE tasks       SET updated_at   = substr(replace(updated_at,   ' ', 'T'), 1, 19) || 'Z' WHERE updated_at   IS NOT NULL AND (updated_at   GLOB '*[0-9] [0-9]*' OR updated_at   GLOB '*.*Z');
UPDATE tasks       SET completed_at = substr(replace(completed_at, ' ', 'T'), 1, 19) || 'Z' WHERE completed_at IS NOT NULL AND (completed_at GLOB '*[0-9] [0-9]*' OR completed_at GLOB '*.*Z');

UPDATE habits       SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
UPDATE habits       SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE schedules    SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
UPDATE schedules    SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE tokens       SET created_at  = substr(replace(created_at,  ' ', 'T'), 1, 19) || 'Z' WHERE created_at  IS NOT NULL AND (created_at  GLOB '*[0-9] [0-9]*' OR created_at  GLOB '*.*Z');
UPDATE tokens       SET revoked_at  = substr(replace(revoked_at,  ' ', 'T'), 1, 19) || 'Z' WHERE revoked_at  IS NOT NULL AND (revoked_at  GLOB '*[0-9] [0-9]*' OR revoked_at  GLOB '*.*Z');

UPDATE settings     SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
UPDATE settings     SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE google_cal_settings SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
UPDATE google_cal_settings SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE google_cal_events SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE skills       SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
UPDATE skills       SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE memories     SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
UPDATE memories     SET updated_at = substr(replace(updated_at, ' ', 'T'), 1, 19) || 'Z' WHERE updated_at IS NOT NULL AND (updated_at GLOB '*[0-9] [0-9]*' OR updated_at GLOB '*.*Z');

UPDATE habit_scheduled_spans SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');

UPDATE habit_steps  SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');

UPDATE task_work_sessions SET started_at = substr(replace(started_at, ' ', 'T'), 1, 19) || 'Z' WHERE started_at IS NOT NULL AND (started_at GLOB '*[0-9] [0-9]*' OR started_at GLOB '*.*Z');
UPDATE task_work_sessions SET ended_at   = substr(replace(ended_at,   ' ', 'T'), 1, 19) || 'Z' WHERE ended_at   IS NOT NULL AND (ended_at   GLOB '*[0-9] [0-9]*' OR ended_at   GLOB '*.*Z');
UPDATE task_work_sessions SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');

UPDATE progress_events SET at = substr(replace(at, ' ', 'T'), 1, 19) || 'Z' WHERE at IS NOT NULL AND (at GLOB '*[0-9] [0-9]*' OR at GLOB '*.*Z');

UPDATE progress_operations SET created_at = substr(replace(created_at, ' ', 'T'), 1, 19) || 'Z' WHERE created_at IS NOT NULL AND (created_at GLOB '*[0-9] [0-9]*' OR created_at GLOB '*.*Z');
