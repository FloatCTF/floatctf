-- ---------------------------------------------------------------------------
-- 移除 AWDP 练习 Judge 旧 push/sweep 调度任务
--
-- Pull + Lease 模型（plan §12-§19）下，JudgeServer 主动拉取评估作业，
-- 平台不再运行 `awdp.practice.judge` 例行检查 cron（sweep → POST /batch → 回调）。
-- 该任务行已无处理器注册（validate_enabled_task_keys 会拒启），直接删除。
-- 不复活：init_and_recover 只恢复 protected+enabled 的 failed cron 行，删除后无行可恢复。
-- ---------------------------------------------------------------------------

DELETE FROM public.scheduled_tasks
WHERE task_key = 'awdp.practice.judge';
