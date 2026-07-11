-- #459: 1日あたりの作業負荷設定を追加する
ALTER TABLE settings ADD COLUMN comfortable_minutes INTEGER;
ALTER TABLE settings ADD COLUMN maximum_minutes INTEGER;
