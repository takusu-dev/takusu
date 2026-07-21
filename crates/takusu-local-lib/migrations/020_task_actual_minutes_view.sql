-- Pre-computed total active work minutes per task.
-- Mirrors takusu_util::minutes_between: every closed or still-open session
-- counts as at least 1 minute, with 'now' used for open sessions.
CREATE VIEW IF NOT EXISTS task_actual_minutes AS
SELECT
    task_id,
    SUM(MAX((strftime('%s', COALESCE(ended_at, 'now')) - strftime('%s', started_at)) / 60, 1)) AS actual_minutes
FROM task_work_sessions
GROUP BY task_id;
