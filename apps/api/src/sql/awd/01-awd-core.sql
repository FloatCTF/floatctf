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
