-- ================================================================================
-- Migration: 20260808120928-awd-wg-config-fetched
-- ================================================================================
-- Phase 1 P1-15：WireGuard 私钥一次性返回。
-- player 首次拉取 WG 配置时返回私钥并记录 config_fetched_at；
-- 之后再次请求不再返回私钥（防 token 泄漏后私钥被反复拉取）。
-- ================================================================================


ALTER TABLE "awd_wireguard_peers"
    ADD COLUMN IF NOT EXISTS "config_fetched_at" TIMESTAMPTZ;

COMMENT ON COLUMN "awd_wireguard_peers"."config_fetched_at" IS 'WG 配置（含私钥）首次拉取时间；NULL=尚未拉取（Phase 1 P1-15）';

