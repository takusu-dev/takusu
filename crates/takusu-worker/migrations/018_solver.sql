-- #772: solver / time budget / seed / warm start 設定を追加
ALTER TABLE settings ADD COLUMN solver TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE settings ADD COLUMN time_budget_ms INTEGER;
ALTER TABLE settings ADD COLUMN seed INTEGER;
ALTER TABLE settings ADD COLUMN warm_start BOOLEAN NOT NULL DEFAULT 0;
