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

COMMENT ON COLUMN "scheduled_tasks"."attempt_count" IS 'Times this task execution has been attempted';
COMMENT ON COLUMN "scheduled_tasks"."max_attempts" IS 'Max attempts before permanent failure';
COMMENT ON COLUMN "scheduled_tasks"."timeout_secs" IS 'Optional per-task execution timeout in seconds';
COMMENT ON COLUMN "scheduled_tasks"."last_error" IS 'Last failure message for retry diagnostics';
COMMENT ON COLUMN "scheduled_tasks"."locked_at" IS 'Worker lock time while running';
COMMENT ON COLUMN "scheduled_tasks"."heartbeat_at" IS 'Last worker heartbeat while running';
