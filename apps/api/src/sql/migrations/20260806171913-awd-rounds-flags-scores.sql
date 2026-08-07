-- ============================================================
-- AWD Rounds, Flags, Submissions, and Score Events
-- ============================================================

-- 5. Rounds
CREATE TYPE "round_status" AS ENUM (
    'active', 'grace', 'completed', 'paused'
);

CREATE TABLE IF NOT EXISTS "awd_rounds" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_number" INTEGER NOT NULL,
    "status" "round_status" NOT NULL DEFAULT 'active',
    "phase" "awd_phase" NOT NULL DEFAULT 'attack',
    "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "scheduled_end_at" TIMESTAMPTZ NOT NULL,
    "grace_ends_at" TIMESTAMPTZ,
    "paused_at" TIMESTAMPTZ,
    "remaining_secs" INTEGER,
    "completed_at" TIMESTAMPTZ,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_number")
);

-- At most one active round per event
CREATE UNIQUE INDEX IF NOT EXISTS "idx_awd_rounds_one_active"
    ON "awd_rounds" ("event_id", "status")
    WHERE "status" IN ('active', 'grace', 'paused');

-- 6. Flag issues (deterministic, per GameBox per round)
CREATE TABLE IF NOT EXISTS "awd_flag_issues" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "flag_hash" VARCHAR(128) NOT NULL,              -- SHA-256 hash of the flag
    "issued_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_id", "gamebox_instance_id"),
    UNIQUE ("event_id", "round_id", "flag_hash")
);

-- 7. Flag submissions
CREATE TABLE IF NOT EXISTS "awd_flag_submissions" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "flag_issue_id" UUID NOT NULL REFERENCES "awd_flag_issues" ("id") ON DELETE CASCADE,
    "attacker_team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "victim_team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "submitted_by_user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "submitted_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_id", "attacker_team_id", "gamebox_instance_id")
);

-- 8. Score events (append-only ledger)
CREATE TYPE "score_event_type" AS ENUM (
    'attack', 'victim_loss', 'judge_fix', 'judge_down',
    'first_bonus', 'reset_penalty', 'adjustment'
);

CREATE TABLE IF NOT EXISTS "awd_score_events" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "event_type" "score_event_type" NOT NULL,
    "delta" BIGINT NOT NULL,
    "idempotency_key" VARCHAR(300) NOT NULL UNIQUE,
    "related_team_id" UUID REFERENCES "event_teams" ("id") ON DELETE SET NULL,
    "gamebox_instance_id" UUID REFERENCES "awd_gamebox_instances" ("id") ON DELETE SET NULL,
    "gamebox_template_id" UUID REFERENCES "awd_gamebox_templates" ("id") ON DELETE SET NULL,
    "reference_id" UUID,
    "reason" TEXT,
    "metadata_json" JSONB NOT NULL DEFAULT '{}',
    "created_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_rounds" IS 'AWD 回合表：比赛按固定时长推进的回合（含宽限期、暂停与完成状态）';
COMMENT ON COLUMN "awd_rounds"."id" IS '主键';
COMMENT ON COLUMN "awd_rounds"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_rounds"."round_number" IS '回合序号（赛事内唯一）';
COMMENT ON COLUMN "awd_rounds"."status" IS '回合状态：active / grace 宽限 / completed / paused';
COMMENT ON COLUMN "awd_rounds"."phase" IS '回合阶段（默认 attack）';
COMMENT ON COLUMN "awd_rounds"."started_at" IS '回合开始时间';
COMMENT ON COLUMN "awd_rounds"."scheduled_end_at" IS '计划结束时间';
COMMENT ON COLUMN "awd_rounds"."grace_ends_at" IS '宽限期结束时间（可为空）';
COMMENT ON COLUMN "awd_rounds"."paused_at" IS '暂停时间（可为空）';
COMMENT ON COLUMN "awd_rounds"."remaining_secs" IS '暂停时剩余秒数（恢复时续走）';
COMMENT ON COLUMN "awd_rounds"."completed_at" IS '完成时间';
COMMENT ON COLUMN "awd_rounds"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_flag_issues" IS 'AWD Flag 发放表：每轮每靶机确定性生成 flag（只存哈希，防泄密）';
COMMENT ON COLUMN "awd_flag_issues"."id" IS '主键';
COMMENT ON COLUMN "awd_flag_issues"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_flag_issues"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_flag_issues"."gamebox_instance_id" IS '靶机实例 ID';
COMMENT ON COLUMN "awd_flag_issues"."flag_hash" IS 'flag 的 SHA-256 哈希';
COMMENT ON COLUMN "awd_flag_issues"."issued_at" IS '发放时间';

COMMENT ON TABLE "awd_flag_submissions" IS 'AWD Flag 提交表：攻击方提交对方靶机 flag 的记录';
COMMENT ON COLUMN "awd_flag_submissions"."id" IS '主键';
COMMENT ON COLUMN "awd_flag_submissions"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_flag_submissions"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_flag_submissions"."flag_issue_id" IS '对应的 flag 发放记录 ID';
COMMENT ON COLUMN "awd_flag_submissions"."attacker_team_id" IS '攻击方队伍 ID';
COMMENT ON COLUMN "awd_flag_submissions"."victim_team_id" IS '受害方队伍 ID';
COMMENT ON COLUMN "awd_flag_submissions"."gamebox_instance_id" IS '被攻击的靶机实例 ID';
COMMENT ON COLUMN "awd_flag_submissions"."submitted_by_user_id" IS '提交用户 ID';
COMMENT ON COLUMN "awd_flag_submissions"."submitted_at" IS '提交时间';

COMMENT ON TABLE "awd_score_events" IS 'AWD 积分事件账本：只追加（append-only），所有得分/扣分/调整的审计轨迹';
COMMENT ON COLUMN "awd_score_events"."id" IS '主键';
COMMENT ON COLUMN "awd_score_events"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_score_events"."round_id" IS '回合 ID（可为空）';
COMMENT ON COLUMN "awd_score_events"."team_id" IS '产生积分变化的队伍 ID';
COMMENT ON COLUMN "awd_score_events"."event_type" IS '事件类型：attack 攻击得分 / victim_loss 受害失分 / judge_fix 修复 / judge_down 宕机 / first_bonus 首破 / reset_penalty 重置惩罚 / adjustment 人工调整';
COMMENT ON COLUMN "awd_score_events"."delta" IS '积分变化量（正为得分，负为扣分）';
COMMENT ON COLUMN "awd_score_events"."idempotency_key" IS '幂等键（唯一，防止重复记账）';
COMMENT ON COLUMN "awd_score_events"."related_team_id" IS '关联队伍（如攻击/受害中的另一方，可为空）';
COMMENT ON COLUMN "awd_score_events"."gamebox_instance_id" IS '关联靶机实例（可为空）';
COMMENT ON COLUMN "awd_score_events"."gamebox_template_id" IS '关联靶机模板（可为空）';
COMMENT ON COLUMN "awd_score_events"."reference_id" IS '参考 ID（如关联的重置记录，可为空）';
COMMENT ON COLUMN "awd_score_events"."reason" IS '事件原因说明';
COMMENT ON COLUMN "awd_score_events"."metadata_json" IS '附加元数据（JSON）';
COMMENT ON COLUMN "awd_score_events"."created_by" IS '创建人（超级管理员，人工调整时有值）';
COMMENT ON COLUMN "awd_score_events"."created_at" IS '创建时间';
