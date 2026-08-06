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
