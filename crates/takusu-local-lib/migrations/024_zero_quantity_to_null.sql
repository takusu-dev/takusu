-- quantity_total == 0 is semantically the same as unset (NULL).
-- Normalize any legacy rows so the server-side code only deals with NULL or positive totals.

UPDATE tasks SET quantity_total = NULL WHERE quantity_total = 0;
UPDATE tasks SET original_quantity_total = NULL WHERE original_quantity_total = 0;
