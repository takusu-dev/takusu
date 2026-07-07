-- window_mode (#window_mode): controls how the habit task window is computed.
-- 'day'    = (default, backward-compatible) occurrence day's start_time..end_time
-- 'period' = occurrence start_time .. next occurrence's start_time (the whole
--            interval is the schedulable window, so weekly/monthly habits can
--            be placed anywhere within their period).
ALTER TABLE habits ADD COLUMN window_mode TEXT NOT NULL DEFAULT 'day';
