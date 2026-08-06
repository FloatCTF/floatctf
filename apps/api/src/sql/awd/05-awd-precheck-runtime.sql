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
