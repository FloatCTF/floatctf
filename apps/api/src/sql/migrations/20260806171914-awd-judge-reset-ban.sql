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


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_judge_batches" IS 'AWD 判题批次：每回合发起的一批判题任务的汇总（进度与结果统计）';
COMMENT ON COLUMN "awd_judge_batches"."id" IS '主键';
COMMENT ON COLUMN "awd_judge_batches"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_judge_batches"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_judge_batches"."total_tasks" IS '总任务数';
COMMENT ON COLUMN "awd_judge_batches"."completed_tasks" IS '已完成任务数';
COMMENT ON COLUMN "awd_judge_batches"."failed_tasks" IS '失败任务数';
COMMENT ON COLUMN "awd_judge_batches"."status" IS '批次状态（默认 pending）';
COMMENT ON COLUMN "awd_judge_batches"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_judge_tasks" IS 'AWD 判题任务：对每个靶机实例执行健康/服务判定的单个任务（含重试与输出记录）';
COMMENT ON COLUMN "awd_judge_tasks"."id" IS '主键';
COMMENT ON COLUMN "awd_judge_tasks"."batch_id" IS '所属判题批次 ID';
COMMENT ON COLUMN "awd_judge_tasks"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_judge_tasks"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_judge_tasks"."gamebox_instance_id" IS '被判定靶机实例 ID';
COMMENT ON COLUMN "awd_judge_tasks"."template_id" IS '靶机模板 ID';
COMMENT ON COLUMN "awd_judge_tasks"."team_id" IS '所属队伍 ID';
COMMENT ON COLUMN "awd_judge_tasks"."status" IS '任务状态：pending/running/up/down/judge_error/judge_timeout/skipped_resetting/skipped_banned';
COMMENT ON COLUMN "awd_judge_tasks"."attempt_count" IS '已尝试次数';
COMMENT ON COLUMN "awd_judge_tasks"."max_attempts" IS '最大尝试次数（默认 2）';
COMMENT ON COLUMN "awd_judge_tasks"."deadline_at" IS '执行截止时间';
COMMENT ON COLUMN "awd_judge_tasks"."started_at" IS '开始执行时间';
COMMENT ON COLUMN "awd_judge_tasks"."finished_at" IS '执行完成时间';
COMMENT ON COLUMN "awd_judge_tasks"."exit_code" IS '判题脚本退出码';
COMMENT ON COLUMN "awd_judge_tasks"."stdout_limited" IS '截断后的标准输出';
COMMENT ON COLUMN "awd_judge_tasks"."stderr_limited" IS '截断后的标准错误输出';
COMMENT ON COLUMN "awd_judge_tasks"."duration_ms" IS '执行耗时（毫秒）';
COMMENT ON COLUMN "awd_judge_tasks"."callback_idempotency_key" IS '回调幂等键（防止判题回调重复处理）';
COMMENT ON COLUMN "awd_judge_tasks"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_reset_records" IS 'AWD 靶机重置记录：队伍请求重置靶机的流水（含免费/惩罚、执行状态）';
COMMENT ON COLUMN "awd_reset_records"."id" IS '主键';
COMMENT ON COLUMN "awd_reset_records"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_reset_records"."team_id" IS '请求队伍 ID';
COMMENT ON COLUMN "awd_reset_records"."gamebox_instance_id" IS '被重置的靶机实例 ID';
COMMENT ON COLUMN "awd_reset_records"."round_id" IS '请求所在回合 ID（可为空）';
COMMENT ON COLUMN "awd_reset_records"."requested_by" IS '请求用户 ID（可为空）';
COMMENT ON COLUMN "awd_reset_records"."free_reset" IS '是否免费重置（超出免费次数则扣分）';
COMMENT ON COLUMN "awd_reset_records"."penalty_score_event_id" IS '扣除的惩罚积分事件 ID';
COMMENT ON COLUMN "awd_reset_records"."status" IS '重置状态（默认 pending）';
COMMENT ON COLUMN "awd_reset_records"."started_at" IS '开始执行时间';
COMMENT ON COLUMN "awd_reset_records"."completed_at" IS '完成时间';
COMMENT ON COLUMN "awd_reset_records"."error_msg" IS '失败原因';
COMMENT ON COLUMN "awd_reset_records"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_team_bans" IS 'AWD 队伍封禁表：因违规被封禁的队伍（含申请解封与生效回合）';
COMMENT ON COLUMN "awd_team_bans"."id" IS '主键';
COMMENT ON COLUMN "awd_team_bans"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_team_bans"."team_id" IS '被封禁队伍 ID';
COMMENT ON COLUMN "awd_team_bans"."status" IS '封禁状态：active / pending_unban 待解封 / unbanned';
COMMENT ON COLUMN "awd_team_bans"."reason" IS '封禁原因';
COMMENT ON COLUMN "awd_team_bans"."effective_round_id" IS '封禁生效回合（可为空）';
COMMENT ON COLUMN "awd_team_bans"."banned_by" IS '封禁人（超级管理员）';
COMMENT ON COLUMN "awd_team_bans"."banned_at" IS '封禁时间';
COMMENT ON COLUMN "awd_team_bans"."unban_requested_at" IS '申请解封时间';
COMMENT ON COLUMN "awd_team_bans"."unban_effective_round_id" IS '解封生效回合（可为空）';
COMMENT ON COLUMN "awd_team_bans"."unbanned_by" IS '解封人（超级管理员）';
COMMENT ON COLUMN "awd_team_bans"."unbanned_at" IS '解封时间';
