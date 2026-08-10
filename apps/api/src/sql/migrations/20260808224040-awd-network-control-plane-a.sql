-- ================================================================================
-- Migration: 20260808224040-awd-network-control-plane-a
-- ================================================================================
-- AWD Network Control Plane 重构 — Migration A：新增领域表。
--
-- 分层模型（禁止 Network Profile 引用模型）：
--   awd_network_settings（平台资源池，singleton）
--     └─ awd_network_allocations（独占分配账本）
--          └─ awd_event_networks（赛事已固化网络配置）
--               └─ awd_team_networks（Team 子网，本迁移不涉及）
--
-- 本迁移只做「新增」，数据回填在 Migration B，类型转换在 Migration C，
-- 删除 legacy 字段在 Migration D。严格按 A → B → C → D 顺序执行。
-- ================================================================================


-- ──────────────────────────────────────────────────────────────────────────────
-- 0. text → inet/cidr 隐式转换
--    SeaORM 1.1.20 将 inet/cidr 列映射为 String 并以 text 绑定参数，
--    PostgreSQL 默认无 text→inet/cidr 隐式转换（已在 dev DB 实证报错）。
--    注册 WITH INOUT 隐式转换后，实体 INSERT/UPDATE/WHERE 均可用（已实证）。
--    非法文本仍会被 PG 的 inet/cidr 解析拒绝，类型校验不丢失。
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_cast
        WHERE castsource = 'text'::regtype AND casttarget = 'inet'::regtype
    ) THEN
        CREATE CAST (text AS inet) WITH INOUT AS IMPLICIT;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_cast
        WHERE castsource = 'text'::regtype AND casttarget = 'cidr'::regtype
    ) THEN
        CREATE CAST (text AS cidr) WITH INOUT AS IMPLICIT;
    END IF;
END $$;

-- ──────────────────────────────────────────────────────────────────────────────
-- 1. enums（DO block 幂等）
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'awd_network_allocation_mode') THEN
        CREATE TYPE "awd_network_allocation_mode" AS ENUM (
            'automatic',
            'manual'
        );
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'awd_network_allocation_kind') THEN
        CREATE TYPE "awd_network_allocation_kind" AS ENUM (
            'gamebox',
            'wireguard'
        );
    END IF;
END $$;

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. awd_network_settings：平台资源池（singleton，id 恒为 1）
-- ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS "awd_network_settings" (
    "id" SMALLINT PRIMARY KEY DEFAULT 1,

    -- GameBox 地址池
    "gamebox_pool" CIDR NOT NULL,
    "gamebox_event_prefix" SMALLINT NOT NULL,
    "gamebox_team_prefix" SMALLINT NOT NULL,

    -- WireGuard 地址池
    "wireguard_pool" CIDR NOT NULL,
    "wireguard_event_prefix" SMALLINT NOT NULL,
    "wireguard_team_prefix" SMALLINT NOT NULL,

    -- WG 端口池与公网入口
    "wireguard_port_min" INTEGER NOT NULL,
    "wireguard_port_max" INTEGER NOT NULL,
    "wireguard_public_endpoint" TEXT,

    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- singleton 约束
    CONSTRAINT "awd_network_settings_singleton" CHECK (id = 1),

    -- prefix 顺序：pool ≤ event ≤ team（masklen 取 pool 前缀长度）
    CONSTRAINT "awd_network_settings_gb_prefix_order" CHECK (
        masklen(gamebox_pool) <= gamebox_event_prefix
        AND gamebox_event_prefix <= gamebox_team_prefix
    ),
    CONSTRAINT "awd_network_settings_wg_prefix_order" CHECK (
        masklen(wireguard_pool) <= wireguard_event_prefix
        AND wireguard_event_prefix <= wireguard_team_prefix
    ),

    -- 两个 pool 不允许重叠（application 层同样校验，此处 DB 兜底）
    CONSTRAINT "awd_network_settings_pools_no_overlap" CHECK (
        NOT (gamebox_pool && wireguard_pool)
    ),

    -- WG 端口范围合法性
    CONSTRAINT "awd_network_settings_port_range" CHECK (
        wireguard_port_min >= 1 AND wireguard_port_max <= 65535
        AND wireguard_port_min <= wireguard_port_max
    )
);

-- 默认平台配置（与旧开发库的 10.42/172.31 等历史值无关，属全新默认）
INSERT INTO "awd_network_settings" (
    id, gamebox_pool, gamebox_event_prefix, gamebox_team_prefix,
    wireguard_pool, wireguard_event_prefix, wireguard_team_prefix,
    wireguard_port_min, wireguard_port_max, wireguard_public_endpoint
) VALUES (
    1, '10.0.0.0/8', 16, 24,
    '172.16.0.0/12', 16, 24,
    30000, 40000, NULL
)
ON CONFLICT (id) DO NOTHING;

COMMENT ON COLUMN "awd_network_settings"."gamebox_pool" IS 'GameBox 地址池（CIDR）';
COMMENT ON COLUMN "awd_network_settings"."gamebox_event_prefix" IS '每场 Event 分配的 GameBox 前缀长度';
COMMENT ON COLUMN "awd_network_settings"."gamebox_team_prefix" IS '每 Team 分配的 GameBox 前缀长度';
COMMENT ON COLUMN "awd_network_settings"."wireguard_pool" IS 'WireGuard 地址池（CIDR）';
COMMENT ON COLUMN "awd_network_settings"."wireguard_event_prefix" IS '每场 Event 分配的 WG 前缀长度';
COMMENT ON COLUMN "awd_network_settings"."wireguard_team_prefix" IS '每 Team 分配的 WG 前缀长度';
COMMENT ON COLUMN "awd_network_settings"."wireguard_port_min" IS 'WG 监听端口池下限（含）';
COMMENT ON COLUMN "awd_network_settings"."wireguard_port_max" IS 'WG 监听端口池上限（含）';
COMMENT ON COLUMN "awd_network_settings"."wireguard_public_endpoint" IS '平台 WG 公网入口（hostname/IP，不含端口；端口来自 Event）';
COMMENT ON COLUMN "awd_network_settings"."updated_at" IS '最近一次修改时间';

-- ──────────────────────────────────────────────────────────────────────────────
-- 3. awd_event_networks：一场 AWD Event 已分配并固化的网络配置
-- ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS "awd_event_networks" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    "event_id" UUID NOT NULL UNIQUE
        REFERENCES "events" ("id") ON DELETE CASCADE,

    "allocation_mode" "awd_network_allocation_mode" NOT NULL,

    "gamebox_cidr" CIDR NOT NULL,
    "wireguard_cidr" CIDR NOT NULL,

    "infrastructure_subnet" CIDR NOT NULL,

    "flagserver_ip" INET NOT NULL,
    "judgeserver_ip" INET NOT NULL,

    "wireguard_interface_name" VARCHAR(15) NOT NULL UNIQUE,
    "wireguard_listen_port" INTEGER NOT NULL UNIQUE,

    "docker_network_name" VARCHAR(64) NOT NULL UNIQUE,

    "locked_at" TIMESTAMPTZ,

    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- infra 子网必须属于 gamebox CIDR，flag/judge IP 必须位于 infra 子网内
    CONSTRAINT "awd_event_networks_infra_inside_gb" CHECK (
        infrastructure_subnet <<= gamebox_cidr
    ),
    CONSTRAINT "awd_event_networks_flag_inside_infra" CHECK (
        flagserver_ip << infrastructure_subnet
    ),
    CONSTRAINT "awd_event_networks_judge_inside_infra" CHECK (
        judgeserver_ip << infrastructure_subnet
    ),
    CONSTRAINT "awd_event_networks_flag_ne_judge" CHECK (
        flagserver_ip <> judgeserver_ip
    ),
    CONSTRAINT "awd_event_networks_gb_wg_no_overlap" CHECK (
        NOT (gamebox_cidr && wireguard_cidr)
    ),
    CONSTRAINT "awd_event_networks_locked_at" CHECK (
        locked_at IS NULL OR locked_at >= created_at
    )
);

COMMENT ON COLUMN "awd_event_networks"."allocation_mode" IS '分配模式：automatic（平台池自动 reserve）/ manual（管理员指定，仍走同一套 overlap 校验）';
COMMENT ON COLUMN "awd_event_networks"."gamebox_cidr" IS '赛事 GameBox 网段（CIDR）';
COMMENT ON COLUMN "awd_event_networks"."wireguard_cidr" IS '赛事 WireGuard 网段（CIDR）';
COMMENT ON COLUMN "awd_event_networks"."infrastructure_subnet" IS '基础设施子网（gamebox CIDR 的第一块 team-size 子网）';
COMMENT ON COLUMN "awd_event_networks"."flagserver_ip" IS 'FlagServer 固定 IP（位于 infrastructure_subnet 内）';
COMMENT ON COLUMN "awd_event_networks"."judgeserver_ip" IS 'JudgeServer 固定 IP（位于 infrastructure_subnet 内）';
COMMENT ON COLUMN "awd_event_networks"."wireguard_interface_name" IS 'WG 接口名（deterministic，<= 15 字符，Linux 限制）';
COMMENT ON COLUMN "awd_event_networks"."wireguard_listen_port" IS 'WG 监听端口（平台端口池内分配，UNIQUE 兜底并发）';
COMMENT ON COLUMN "awd_event_networks"."docker_network_name" IS 'Docker 网络逻辑名（desired identity；实际 network ID 属 Observed，存 awd_runtime_resources）';
COMMENT ON COLUMN "awd_event_networks"."locked_at" IS 'Deploy 后置锁时间（锁定后 addressing 禁止修改）';

CREATE INDEX IF NOT EXISTS "idx_awd_event_networks_event_id"
    ON "awd_event_networks" ("event_id");

-- ──────────────────────────────────────────────────────────────────────────────
-- 4. awd_network_allocations：平台地址池独占分配账本
-- ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS "awd_network_allocations" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    "event_id" UUID NOT NULL
        REFERENCES "events" ("id") ON DELETE CASCADE,

    "kind" "awd_network_allocation_kind" NOT NULL,

    "cidr" CIDR NOT NULL,

    "allocated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "released_at" TIMESTAMPTZ,

    CONSTRAINT "awd_network_allocations_release_order" CHECK (
        released_at IS NULL OR released_at >= allocated_at
    )
);

COMMENT ON COLUMN "awd_network_allocations"."kind" IS '分配种类：gamebox / wireguard';
COMMENT ON COLUMN "awd_network_allocations"."cidr" IS '被占用的 CIDR 块';
COMMENT ON COLUMN "awd_network_allocations"."allocated_at" IS '分配时间';
COMMENT ON COLUMN "awd_network_allocations"."released_at" IS '释放时间（仅 Event Archive runtime cleanup 成功后写入；NULL=仍占用）';

-- 分配器查询路径索引（active allocations / 按 event / 按 cidr）
CREATE INDEX IF NOT EXISTS "idx_awd_network_allocations_active"
    ON "awd_network_allocations" ("kind", "released_at");
CREATE INDEX IF NOT EXISTS "idx_awd_network_allocations_event"
    ON "awd_network_allocations" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_network_allocations_cidr"
    ON "awd_network_allocations" ("cidr");

