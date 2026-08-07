-- Incremental update (existing DBs): scheduler reliability columns (R5-B).
-- Apply manually or via floatctf-migration m0101.
-- After applying, regenerate Entity with sea-orm-cli — do NOT hand-edit entity/.
--
--   sea-orm-cli generate entity -o src/entity --with-serde both
--   (or project-standard entity regen command)
--
-- Safe to re-run (IF NOT EXISTS).

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "attempt_count" INTEGER NOT NULL DEFAULT 0;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "max_attempts" INTEGER NOT NULL DEFAULT 3;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "timeout_secs" INTEGER;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "last_error" TEXT;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "locked_at" TIMESTAMPTZ;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "heartbeat_at" TIMESTAMPTZ;

COMMENT ON COLUMN "scheduled_tasks"."attempt_count" IS '已尝试执行次数';
COMMENT ON COLUMN "scheduled_tasks"."max_attempts" IS '最大重试次数，超过则判定永久失败';
COMMENT ON COLUMN "scheduled_tasks"."timeout_secs" IS '单次执行超时时间（秒）';
COMMENT ON COLUMN "scheduled_tasks"."last_error" IS '最近一次失败信息（重试诊断用）';
COMMENT ON COLUMN "scheduled_tasks"."locked_at" IS '工作进程执行锁时间';
COMMENT ON COLUMN "scheduled_tasks"."heartbeat_at" IS '工作进程心跳时间（执行期间定期更新）';
