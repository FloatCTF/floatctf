-- ============================================================
-- AWD Precheck and Runtime Tables
-- ============================================================

-- 13. Precheck runs
CREATE TYPE "precheck_status" AS ENUM (
    'pending', 'running', 'passed', 'failed', 'error'
);

CREATE TABLE IF NOT EXISTS "awd_precheck_runs" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "status" "precheck_status" NOT NULL DEFAULT 'pending',
    "trigger" VARCHAR(20) NOT NULL DEFAULT 'manual',  -- manual, auto_t_minus_1h
    "revision" TEXT,
    "config_check" JSONB,
    "container_check" JSONB,
    "wireguard_check" JSONB,
    "network_check" JSONB,
    "flag_check" JSONB,
    "judge_check" JSONB,
    "error_msg" TEXT,
    "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "completed_at" TIMESTAMPTZ
);

-- 14. Runtime resources (for reconciliation)
CREATE TABLE IF NOT EXISTS "awd_runtime_resources" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "resource_type" VARCHAR(50) NOT NULL,           -- docker_network, container, wireguard_iface
    "resource_id" VARCHAR(200) NOT NULL,
    "resource_name" VARCHAR(200),
    "observed_state" JSONB,
    "last_seen_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "resource_type", "resource_id")
);

-- 15. Orphan resources (DB has no record, Docker/WG has resource)
CREATE TABLE IF NOT EXISTS "awd_orphan_resources" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID REFERENCES "events" ("id") ON DELETE SET NULL,
    "resource_type" VARCHAR(50) NOT NULL,
    "resource_id" VARCHAR(200) NOT NULL,
    "resource_name" VARCHAR(200),
    "observed_state" JSONB,
    "discovered_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "resolved_at" TIMESTAMPTZ,
    "resolution" VARCHAR(20) DEFAULT 'pending'     -- pending, adopted, cleaned
);

-- 16. Internal token rotations (audit trail)
CREATE TABLE IF NOT EXISTS "awd_internal_token_rotations" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "token_type" VARCHAR(30) NOT NULL,              -- flagserver, judgeserver, event_secret
    "rotated_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "rotated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_precheck_runs" IS 'AWD 赛前检查：比赛开始前对配置/容器/WireGuard/网络/flag/判题的整体体检记录';
COMMENT ON COLUMN "awd_precheck_runs"."id" IS '主键';
COMMENT ON COLUMN "awd_precheck_runs"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_precheck_runs"."status" IS '检查状态：pending / running / passed / failed / error';
COMMENT ON COLUMN "awd_precheck_runs"."trigger" IS '触发方式：manual 手动 / auto_t_minus_1h 开赛前 1 小时自动';
COMMENT ON COLUMN "awd_precheck_runs"."revision" IS '被检查的配置版本';
COMMENT ON COLUMN "awd_precheck_runs"."config_check" IS '配置检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."container_check" IS '容器检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."wireguard_check" IS 'WireGuard 检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."network_check" IS '网络检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."flag_check" IS 'flag 检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."judge_check" IS '判题检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."error_msg" IS '检查失败原因';
COMMENT ON COLUMN "awd_precheck_runs"."started_at" IS '检查开始时间';
COMMENT ON COLUMN "awd_precheck_runs"."completed_at" IS '检查完成时间';

COMMENT ON TABLE "awd_runtime_resources" IS 'AWD 运行时资源：系统实际创建的 Docker 网络/容器/WireGuard 网卡等资源（用于对账）';
COMMENT ON COLUMN "awd_runtime_resources"."id" IS '主键';
COMMENT ON COLUMN "awd_runtime_resources"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_runtime_resources"."resource_type" IS '资源类型：docker_network / container / wireguard_iface';
COMMENT ON COLUMN "awd_runtime_resources"."resource_id" IS '资源 ID（Docker 网络 ID/容器 ID 等）';
COMMENT ON COLUMN "awd_runtime_resources"."resource_name" IS '资源名称';
COMMENT ON COLUMN "awd_runtime_resources"."observed_state" IS '观察到的资源状态（JSON）';
COMMENT ON COLUMN "awd_runtime_resources"."last_seen_at" IS '最近一次观察到的时间';

COMMENT ON TABLE "awd_orphan_resources" IS 'AWD 孤儿资源：数据库无记录但 Docker/WireGuard 中实际存在的资源（泄漏检测与清理）';
COMMENT ON COLUMN "awd_orphan_resources"."id" IS '主键';
COMMENT ON COLUMN "awd_orphan_resources"."event_id" IS '赛事 ID（可为空，删除时置 NULL）';
COMMENT ON COLUMN "awd_orphan_resources"."resource_type" IS '资源类型';
COMMENT ON COLUMN "awd_orphan_resources"."resource_id" IS '资源 ID';
COMMENT ON COLUMN "awd_orphan_resources"."resource_name" IS '资源名称';
COMMENT ON COLUMN "awd_orphan_resources"."observed_state" IS '观察到的状态（JSON）';
COMMENT ON COLUMN "awd_orphan_resources"."discovered_at" IS '发现时间';
COMMENT ON COLUMN "awd_orphan_resources"."resolved_at" IS '处理完成时间';
COMMENT ON COLUMN "awd_orphan_resources"."resolution" IS '处理结果：pending 待处理 / adopted 已接管 / cleaned 已清理';

COMMENT ON TABLE "awd_internal_token_rotations" IS 'AWD 内部令牌轮换审计：flagserver/judgeserver 令牌与事件密钥的轮换记录';
COMMENT ON COLUMN "awd_internal_token_rotations"."id" IS '主键';
COMMENT ON COLUMN "awd_internal_token_rotations"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_internal_token_rotations"."token_type" IS '令牌类型：flagserver / judgeserver / event_secret';
COMMENT ON COLUMN "awd_internal_token_rotations"."rotated_by" IS '轮换操作人（超级管理员）';
COMMENT ON COLUMN "awd_internal_token_rotations"."rotated_at" IS '轮换时间';
