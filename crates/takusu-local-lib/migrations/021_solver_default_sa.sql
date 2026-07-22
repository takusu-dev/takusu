-- #845: デフォルトの solver を 'auto' から 'sa' に変更する
UPDATE settings SET solver = 'sa' WHERE solver = 'auto' AND id = 'active';
