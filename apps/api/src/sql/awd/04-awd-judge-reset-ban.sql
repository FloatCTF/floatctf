-- ============================================================
-- AWD Judge, Reset, and Ban Tables
-- ============================================================

-- 9. Judge batches
CREATE TABLE IF NOT EXISTS "awd_judge_batches" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "total_tasks" INTEGER NOT NULL DEFAULT 0,
    "completed_tasks" INTEGER NOT NULL DEFAULT 0,
    "failed_tasks" INTEGER NOT NULL DEFAULT 0,
    "status" VARCHAR(20) NOT NULL DEFAULT 'pending',
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 10. Judge tasks
CREATE TYPE "judge_task_status" AS ENUM (
    'pending', 'running', 'up', 'down',
    'judge_error', 'judge_timeout',
    'skipped_resetting', 'skipped_banned'
);

CREATE TABLE IF NOT EXISTS "awd_judge_tasks" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "batch_id" UUID NOT NULL REFERENCES "awd_judge_batches" ("id") ON DELETE CASCADE,
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "template_id" UUID NOT NULL REFERENCES "awd_gamebox_templates" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "status" "judge_task_status" NOT NULL DEFAULT 'pending',
    "attempt_count" INTEGER NOT NULL DEFAULT 0,
    "max_attempts" INTEGER NOT NULL DEFAULT 2,
    "deadline_at" TIMESTAMPTZ NOT NULL,
    "started_at" TIMESTAMPTZ,
    "finished_at" TIMESTAMPTZ,
    "exit_code" INTEGER,
    "stdout_limited" TEXT,
    "stderr_limited" TEXT,
    "duration_ms" INTEGER,
    "callback_idempotency_key" VARCHAR(300),
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_id", "gamebox_instance_id", "template_id")
);

-- 11. Reset records
CREATE TABLE IF NOT EXISTS "awd_reset_records" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "requested_by" UUID REFERENCES "users" ("id") ON DELETE SET NULL,
    "free_reset" BOOLEAN NOT NULL DEFAULT TRUE,
    "penalty_score_event_id" UUID,
    "status" VARCHAR(20) NOT NULL DEFAULT 'pending',
    "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "completed_at" TIMESTAMPTZ,
    "error_msg" TEXT,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 12. Team bans
CREATE TYPE "ban_status" AS ENUM (
    'active', 'pending_unban', 'unbanned'
);

CREATE TABLE IF NOT EXISTS "awd_team_bans" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "status" "ban_status" NOT NULL DEFAULT 'active',
    "reason" TEXT,
    "effective_round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "banned_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "banned_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "unban_requested_at" TIMESTAMPTZ,
    "unban_effective_round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "unbanned_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "unbanned_at" TIMESTAMPTZ
);

-- At most one active ban per team per event
CREATE UNIQUE INDEX IF NOT EXISTS "idx_awd_team_bans_one_active"
    ON "awd_team_bans" ("event_id", "team_id")
    WHERE "status" = 'active';
