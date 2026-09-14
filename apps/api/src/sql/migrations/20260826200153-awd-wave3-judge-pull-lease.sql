-- ================================================================================
-- Migration: 20260826200153-awd-wave3-judge-pull-lease
-- Wave 3: Judge Pull + Lease — 移除 judge_timeout 枚举值 + 更新宽限期注释
-- ================================================================================

-- 1. 将已有的 judge_timeout 行转为 judge_error（防止 USING 转换失败）
UPDATE awd_judge_tasks
   SET status = 'judge_error'::judge_task_status
 WHERE status = 'judge_timeout'::judge_task_status;

-- 2. 重建 judge_task_status 枚举，移除 judge_timeout
ALTER TYPE judge_task_status RENAME TO judge_task_status_old;

CREATE TYPE judge_task_status AS ENUM (
    'pending',
    'running',
    'up',
    'down',
    'judge_error',
    'skipped_resetting',
    'skipped_banned'
);

ALTER TABLE awd_judge_tasks
    ALTER COLUMN status DROP DEFAULT;

ALTER TABLE awd_judge_tasks
    ALTER COLUMN status TYPE judge_task_status
    USING status::text::judge_task_status;

ALTER TABLE awd_judge_tasks
    ALTER COLUMN status SET DEFAULT 'pending'::judge_task_status;

DROP TYPE judge_task_status_old;

-- 3. 更新 judge_grace_period_secs 注释：Judge 工作截止预算
COMMENT ON COLUMN public.awd_events.judge_grace_period_secs IS
    '判题工作截止预算（秒，默认 30）：Judge 完成本轮全部任务的 deadline，超时后平台不再等待';