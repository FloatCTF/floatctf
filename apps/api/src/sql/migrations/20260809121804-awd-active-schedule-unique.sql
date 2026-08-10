-- ================================================================================
-- Migration: 20260809121804-awd-active-schedule-unique
-- Created at: 2026-08-09 12:18:04 +0800
-- ================================================================================


-- Event 级一次性任务只允许一个 active 实例。上线前若已存在重复任务必须停止迁移，
-- 禁止静默删除，因为无法判断哪一个 execute_at 才是管理员期望值。
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM "scheduled_tasks"
        WHERE "group_id" IS NOT NULL
          AND "task_key" IN ('awd.event.start', 'awd.event.auto_precheck')
          AND "status" IN ('pending', 'running')
        GROUP BY "group_id", "task_key"
        HAVING count(*) > 1
    ) THEN
        RAISE EXCEPTION 'MIGRATION CONFLICT: duplicate active AWD event scheduled tasks';
    END IF;
END
$$;

CREATE UNIQUE INDEX IF NOT EXISTS "idx_scheduled_tasks_awd_event_active_unique"
    ON "scheduled_tasks" ("group_id", "task_key")
    WHERE "group_id" IS NOT NULL
      AND "task_key" IN ('awd.event.start', 'awd.event.auto_precheck')
      AND "status" IN ('pending', 'running');

COMMENT ON INDEX "idx_scheduled_tasks_awd_event_active_unique" IS
    '每个 AWD 赛事的自动预检/定时开赛任务最多存在一个 pending 或 running 实例，防止并发重复执行';

