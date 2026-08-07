-- ============================================================
-- AWD (Attack With Defense) Core Tables
-- ============================================================

-- 1. AWD Events — extends the existing events table with AWD-specific configuration
CREATE TYPE "awd_event_status" AS ENUM (
    'draft', 'configuring', 'deploying', 'deployed',
    'prechecking', 'verified', 'running', 'paused',
    'network_error', 'start_blocked', 'finished',
    'archived', 'deploy_failed', 'verification_failed'
);

CREATE TYPE "awd_phase" AS ENUM (
    'hardening', 'attack', 'pause'
);

CREATE TABLE IF NOT EXISTS "awd_events" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL UNIQUE REFERENCES "events" ("id") ON DELETE CASCADE,
    "status" "awd_event_status" NOT NULL DEFAULT 'draft',
    "phase" "awd_phase" NOT NULL DEFAULT 'hardening',

    -- Network configuration (locked after first deployment)
    "gamebox_cidr" VARCHAR(18) NOT NULL,           -- e.g. "10.0.0.0/16"
    "wireguard_cidr" VARCHAR(18) NOT NULL,          -- e.g. "10.1.0.0/16"
    "wireguard_interface_name" VARCHAR(15) NOT NULL UNIQUE,
    "wireguard_listen_port" INTEGER NOT NULL UNIQUE,
    "flagserver_ip" VARCHAR(15) NOT NULL,
    "judgeserver_ip" VARCHAR(15) NOT NULL,
    "docker_network_id" VARCHAR(64),
    "docker_network_name" VARCHAR(64),

    -- Encrypted secrets
    "event_secret_ciphertext" BYTEA NOT NULL,
    "event_secret_nonce" BYTEA NOT NULL,
    "flagserver_token_ciphertext" BYTEA,
    "flagserver_token_nonce" BYTEA,
    "judgeserver_token_ciphertext" BYTEA,
    "judgeserver_token_nonce" BYTEA,
    "wg_server_private_key_ciphertext" BYTEA,
    "wg_server_private_key_nonce" BYTEA,
    "wg_server_public_key" VARCHAR(44),
    "key_version" INTEGER NOT NULL DEFAULT 1,

    -- Scoring & reset
    "free_reset_count" INTEGER NOT NULL DEFAULT 3,
    "extra_reset_penalty" BIGINT NOT NULL DEFAULT 100,
    "reset_protection_secs" INTEGER NOT NULL DEFAULT 120,

    -- Judge configuration
    "judge_max_concurrency" INTEGER NOT NULL DEFAULT 10,
    "judge_default_timeout_secs" INTEGER NOT NULL DEFAULT 30,
    "judge_retry_interval_secs" INTEGER NOT NULL DEFAULT 5,
    "judge_grace_period_secs" INTEGER NOT NULL DEFAULT 30,
    "round_duration_secs" INTEGER NOT NULL DEFAULT 300,

    -- Archive
    "archive_retention_hours" INTEGER NOT NULL DEFAULT 168,

    -- Verification
    "verified_at" TIMESTAMPTZ,
    "verified_revision" TEXT,

    -- Timing
    "pause_remaining_secs" INTEGER,
    "started_at" TIMESTAMPTZ,
    "finished_at" TIMESTAMPTZ,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 2. Team network assignments per event
CREATE TABLE IF NOT EXISTS "awd_team_networks" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "gamebox_subnet" VARCHAR(18) NOT NULL,          -- e.g. "10.0.1.0/24"
    "wireguard_subnet" VARCHAR(18) NOT NULL,         -- e.g. "10.1.1.0/24"
    "ssh_password_ciphertext" BYTEA NOT NULL,
    "ssh_password_nonce" BYTEA NOT NULL,
    "key_version" INTEGER NOT NULL DEFAULT 1,
    "next_gamebox_host" INTEGER NOT NULL DEFAULT 2,  -- next host byte to allocate
    "next_wireguard_host" INTEGER NOT NULL DEFAULT 2,
    "status" VARCHAR(20) NOT NULL DEFAULT 'active',
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "team_id"),
    UNIQUE ("event_id", "gamebox_subnet"),
    UNIQUE ("event_id", "wireguard_subnet")
);

-- 3. GameBox templates (per event)
CREATE TABLE IF NOT EXISTS "awd_gamebox_templates" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "challenge_id" UUID REFERENCES "challenges" ("id") ON DELETE SET NULL,
    "name" VARCHAR(200) NOT NULL,
    "image_ref" VARCHAR(500) NOT NULL,
    "username" VARCHAR(100) NOT NULL DEFAULT 'ctf',
    "meta_json" JSONB NOT NULL DEFAULT '{}',
    "cpu_millis" BIGINT NOT NULL DEFAULT 1000,
    "memory_bytes" BIGINT NOT NULL DEFAULT 536870912,    -- 512MB
    "pids_limit" BIGINT NOT NULL DEFAULT 100,
    "healthcheck_override_json" JSONB,
    "judge_script_name" VARCHAR(200),
    "judge_script_content" TEXT,
    "judge_args_json" JSONB,
    "judge_timeout_secs" INTEGER,
    "judge_retry_interval_secs" INTEGER,
    "break_points" BIGINT NOT NULL DEFAULT 100,
    "loss_points" BIGINT NOT NULL DEFAULT 100,
    "fix_points" BIGINT NOT NULL DEFAULT 100,
    "down_points" BIGINT NOT NULL DEFAULT 200,
    "first_bonus" BIGINT NOT NULL DEFAULT 20,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "name")
);

-- 4. GameBox instances (per team per template)
CREATE TYPE "gamebox_status" AS ENUM (
    'pending', 'creating', 'running', 'ready',
    'resetting', 'missing', 'orphan', 'conflict',
    'start_failed', 'reset_failed', 'stopped'
);

CREATE TABLE IF NOT EXISTS "awd_gamebox_instances" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "template_id" UUID NOT NULL REFERENCES "awd_gamebox_templates" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "status" "gamebox_status" NOT NULL DEFAULT 'pending',
    "container_id" VARCHAR(64),
    "container_name" VARCHAR(200) NOT NULL UNIQUE,
    "gamebox_ip" VARCHAR(15) NOT NULL,
    "docker_network_id" VARCHAR(64),
    "health_status" VARCHAR(20) NOT NULL DEFAULT 'unknown',
    "reset_protection_until" TIMESTAMPTZ,
    "last_health_check_at" TIMESTAMPTZ,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "deleted_at" TIMESTAMPTZ,
    UNIQUE ("event_id", "gamebox_ip"),
    UNIQUE ("event_id", "template_id", "team_id")
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_events" IS 'AWD 赛事配置：在 events 表基础上扩展的 AWD 专有配置（网络、加密密钥、计分、判题参数）';
COMMENT ON COLUMN "awd_events"."id" IS '主键';
COMMENT ON COLUMN "awd_events"."event_id" IS '关联赛事 ID（唯一）';
COMMENT ON COLUMN "awd_events"."status" IS 'AWD 赛事状态机：draft→configuring→deploying→deployed→prechecking→verified→running→…';
COMMENT ON COLUMN "awd_events"."phase" IS '当前阶段：hardening 加固 / attack 攻击 / pause 暂停';
COMMENT ON COLUMN "awd_events"."gamebox_cidr" IS '靶机网络网段，如 10.0.0.0/16（首次部署后锁定）';
COMMENT ON COLUMN "awd_events"."wireguard_cidr" IS 'WireGuard 网络网段，如 10.1.0.0/16（首次部署后锁定）';
COMMENT ON COLUMN "awd_events"."wireguard_interface_name" IS 'WireGuard 网卡名（唯一）';
COMMENT ON COLUMN "awd_events"."wireguard_listen_port" IS 'WireGuard 监听端口（唯一）';
COMMENT ON COLUMN "awd_events"."flagserver_ip" IS 'Flag 服务器 IP';
COMMENT ON COLUMN "awd_events"."judgeserver_ip" IS '判题服务器 IP';
COMMENT ON COLUMN "awd_events"."docker_network_id" IS 'Docker 网络 ID（可为空）';
COMMENT ON COLUMN "awd_events"."docker_network_name" IS 'Docker 网络名（可为空）';
COMMENT ON COLUMN "awd_events"."event_secret_ciphertext" IS '事件密钥密文（加密存储）';
COMMENT ON COLUMN "awd_events"."event_secret_nonce" IS '事件密钥加密 nonce';
COMMENT ON COLUMN "awd_events"."flagserver_token_ciphertext" IS 'Flag 服务器令牌密文';
COMMENT ON COLUMN "awd_events"."flagserver_token_nonce" IS 'Flag 服务器令牌 nonce';
COMMENT ON COLUMN "awd_events"."judgeserver_token_ciphertext" IS '判题服务器令牌密文';
COMMENT ON COLUMN "awd_events"."judgeserver_token_nonce" IS '判题服务器令牌 nonce';
COMMENT ON COLUMN "awd_events"."wg_server_private_key_ciphertext" IS 'WireGuard 服务端私钥密文';
COMMENT ON COLUMN "awd_events"."wg_server_private_key_nonce" IS 'WireGuard 服务端私钥 nonce';
COMMENT ON COLUMN "awd_events"."wg_server_public_key" IS 'WireGuard 服务端公钥';
COMMENT ON COLUMN "awd_events"."key_version" IS '密钥版本（轮换时递增）';
COMMENT ON COLUMN "awd_events"."free_reset_count" IS '每队免费重置次数（默认 3）';
COMMENT ON COLUMN "awd_events"."extra_reset_penalty" IS '超出免费次数后的额外重置惩罚分（默认 100）';
COMMENT ON COLUMN "awd_events"."reset_protection_secs" IS '重置保护期（秒）：重置后一段时间内不可再次重置（默认 120）';
COMMENT ON COLUMN "awd_events"."judge_max_concurrency" IS '判题最大并发数（默认 10）';
COMMENT ON COLUMN "awd_events"."judge_default_timeout_secs" IS '判题默认超时（秒，默认 30）';
COMMENT ON COLUMN "awd_events"."judge_retry_interval_secs" IS '判题失败重试间隔（秒，默认 5）';
COMMENT ON COLUMN "awd_events"."judge_grace_period_secs" IS '判题宽限期（秒，默认 30）：回合结束后的判题缓冲时间';
COMMENT ON COLUMN "awd_events"."round_duration_secs" IS '单回合时长（秒，默认 300）';
COMMENT ON COLUMN "awd_events"."archive_retention_hours" IS '归档保留时长（小时，默认 168）';
COMMENT ON COLUMN "awd_events"."verified_at" IS '验证通过时间';
COMMENT ON COLUMN "awd_events"."verified_revision" IS '验证通过的配置版本';
COMMENT ON COLUMN "awd_events"."pause_remaining_secs" IS '暂停时剩余的回合秒数（恢复时续走）';
COMMENT ON COLUMN "awd_events"."started_at" IS '比赛开始时间';
COMMENT ON COLUMN "awd_events"."finished_at" IS '比赛结束时间';
COMMENT ON COLUMN "awd_events"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_events"."updated_at" IS '更新时间';

COMMENT ON TABLE "awd_team_networks" IS 'AWD 队伍网络分配：每赛事每队伍的靶机/ WireGuard 子网与 SSH 凭据';
COMMENT ON COLUMN "awd_team_networks"."id" IS '主键';
COMMENT ON COLUMN "awd_team_networks"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_team_networks"."team_id" IS '队伍 ID';
COMMENT ON COLUMN "awd_team_networks"."gamebox_subnet" IS '队伍靶机子网，如 10.0.1.0/24';
COMMENT ON COLUMN "awd_team_networks"."wireguard_subnet" IS '队伍 WireGuard 子网，如 10.1.1.0/24';
COMMENT ON COLUMN "awd_team_networks"."ssh_password_ciphertext" IS '靶机 SSH 密码密文';
COMMENT ON COLUMN "awd_team_networks"."ssh_password_nonce" IS 'SSH 密码加密 nonce';
COMMENT ON COLUMN "awd_team_networks"."key_version" IS '密钥版本';
COMMENT ON COLUMN "awd_team_networks"."next_gamebox_host" IS '下一个可分配的靶机主机位（从 2 开始）';
COMMENT ON COLUMN "awd_team_networks"."next_wireguard_host" IS '下一个可分配的 WireGuard 主机位（从 2 开始）';
COMMENT ON COLUMN "awd_team_networks"."status" IS '状态（默认 active）';
COMMENT ON COLUMN "awd_team_networks"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_team_networks"."updated_at" IS '更新时间';

COMMENT ON TABLE "awd_gamebox_templates" IS 'AWD 靶机模板：赛事内的靶机定义（镜像、资源限制、计分与判题脚本）';
COMMENT ON COLUMN "awd_gamebox_templates"."id" IS '主键';
COMMENT ON COLUMN "awd_gamebox_templates"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_gamebox_templates"."challenge_id" IS '关联题目 ID（可为空，删除时置 NULL）';
COMMENT ON COLUMN "awd_gamebox_templates"."name" IS '模板名称（赛事内唯一）';
COMMENT ON COLUMN "awd_gamebox_templates"."image_ref" IS '容器镜像引用';
COMMENT ON COLUMN "awd_gamebox_templates"."username" IS '容器 SSH 用户名（默认 ctf）';
COMMENT ON COLUMN "awd_gamebox_templates"."meta_json" IS '模板元数据（JSON）';
COMMENT ON COLUMN "awd_gamebox_templates"."cpu_millis" IS 'CPU 限制（毫核，默认 1000）';
COMMENT ON COLUMN "awd_gamebox_templates"."memory_bytes" IS '内存限制（字节，默认 512MB）';
COMMENT ON COLUMN "awd_gamebox_templates"."pids_limit" IS '进程数上限（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."healthcheck_override_json" IS '健康检查配置覆盖（JSON）';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_script_name" IS '判题脚本文件名';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_script_content" IS '判题脚本内容';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_args_json" IS '判题脚本参数（JSON）';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_timeout_secs" IS '判题超时（秒）';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_retry_interval_secs" IS '判题重试间隔（秒）';
COMMENT ON COLUMN "awd_gamebox_templates"."break_points" IS '被攻破时攻击方得分（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."loss_points" IS '被攻破时防守方失分（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."fix_points" IS '修复得分（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."down_points" IS '宕机扣分（默认 200）';
COMMENT ON COLUMN "awd_gamebox_templates"."first_bonus" IS '首破奖励（默认 20）';
COMMENT ON COLUMN "awd_gamebox_templates"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_gamebox_templates"."updated_at" IS '更新时间';

COMMENT ON TABLE "awd_gamebox_instances" IS 'AWD 靶机实例：每队伍每模板实际部署的容器实例';
COMMENT ON COLUMN "awd_gamebox_instances"."id" IS '主键';
COMMENT ON COLUMN "awd_gamebox_instances"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."template_id" IS '靶机模板 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."team_id" IS '队伍 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."status" IS '实例状态：pending/creating/running/ready/resetting/missing/orphan/conflict/start_failed/reset_failed/stopped';
COMMENT ON COLUMN "awd_gamebox_instances"."container_id" IS 'Docker 容器 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."container_name" IS '容器名（唯一）';
COMMENT ON COLUMN "awd_gamebox_instances"."gamebox_ip" IS '靶机内网 IP（赛事内唯一）';
COMMENT ON COLUMN "awd_gamebox_instances"."docker_network_id" IS '所在 Docker 网络 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."health_status" IS '健康状态（默认 unknown）';
COMMENT ON COLUMN "awd_gamebox_instances"."reset_protection_until" IS '重置保护截止时间（此时间前不可重置）';
COMMENT ON COLUMN "awd_gamebox_instances"."last_health_check_at" IS '最近一次健康检查时间';
COMMENT ON COLUMN "awd_gamebox_instances"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_gamebox_instances"."updated_at" IS '更新时间';
COMMENT ON COLUMN "awd_gamebox_instances"."deleted_at" IS '软删除时间（NULL=未删除）';
