-- Rename historical misspelled scheduler task key to the correct spelling.
UPDATE scheduled_tasks
SET task_key = 'CHECK_PRACTICE_EVENT'
WHERE task_key = 'CHECK_PRATICE_EVENT';
