-- Rename historical misspelled scheduler task key to the correct spelling.
-- 修正历史拼写错误的调度任务键名：CHECK_PRATICE_EVENT → CHECK_PRACTICE_EVENT
UPDATE scheduled_tasks
SET task_key = 'CHECK_PRACTICE_EVENT'
WHERE task_key = 'CHECK_PRATICE_EVENT';
