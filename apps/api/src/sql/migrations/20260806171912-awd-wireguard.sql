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


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_wireguard_peers" IS 'AWD WireGuard 对等端：队伍成员接入靶机网络的 VPN 客户端（密钥加密存储，支持轮换/吊销）';
COMMENT ON COLUMN "awd_wireguard_peers"."id" IS '主键';
COMMENT ON COLUMN "awd_wireguard_peers"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_wireguard_peers"."team_id" IS '所属队伍 ID';
COMMENT ON COLUMN "awd_wireguard_peers"."user_id" IS '成员用户 ID';
COMMENT ON COLUMN "awd_wireguard_peers"."status" IS '状态：active 生效 / revoked 已吊销 / rotating 轮换中';
COMMENT ON COLUMN "awd_wireguard_peers"."assigned_ip" IS '分配的 /32 对端 IP';
COMMENT ON COLUMN "awd_wireguard_peers"."public_key" IS '对端公钥（唯一）';
COMMENT ON COLUMN "awd_wireguard_peers"."private_key_ciphertext" IS '对端私钥密文';
COMMENT ON COLUMN "awd_wireguard_peers"."private_key_nonce" IS '私钥加密 nonce';
COMMENT ON COLUMN "awd_wireguard_peers"."key_version" IS '密钥版本';
COMMENT ON COLUMN "awd_wireguard_peers"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_wireguard_peers"."rotated_at" IS '最近密钥轮换时间';
COMMENT ON COLUMN "awd_wireguard_peers"."revoked_at" IS '吊销时间';
