-- ============================================================
-- AWD WireGuard Tables
-- ============================================================

CREATE TYPE "wg_peer_status" AS ENUM (
    'active', 'revoked', 'rotating'
);

CREATE TABLE IF NOT EXISTS "awd_wireguard_peers" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "status" "wg_peer_status" NOT NULL DEFAULT 'active',
    "assigned_ip" VARCHAR(15) NOT NULL,             -- /32 assigned IP
    "public_key" VARCHAR(44) NOT NULL,
    "private_key_ciphertext" BYTEA NOT NULL,
    "private_key_nonce" BYTEA NOT NULL,
    "key_version" INTEGER NOT NULL DEFAULT 1,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "rotated_at" TIMESTAMPTZ,
    "revoked_at" TIMESTAMPTZ,
    UNIQUE ("event_id", "user_id"),
    UNIQUE ("event_id", "assigned_ip"),
    UNIQUE ("public_key")
);
