-- ================================================================================
-- Migration: 20260808124905-awd-configuration-generation
-- ================================================================================
-- Phase 2 P2-9：configuration_generation / verified_generation 机制。
-- 所有影响 runtime 的配置写入口调用 touch_configuration → configuration_generation += 1；
-- Precheck 成功：verified_generation = configuration_generation；
-- Start 校验两者相等，不匹配 → StartBlocked（AWD_CONFIG_CHANGED）。
-- ================================================================================


ALTER TABLE "awd_events"
    ADD COLUMN IF NOT EXISTS "configuration_generation" BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS "verified_generation" BIGINT NULL;

COMMENT ON COLUMN "awd_events"."configuration_generation" IS '配置代数：影响 runtime 的配置每次变更 +1（Phase 2 P2-9）';
COMMENT ON COLUMN "awd_events"."verified_generation" IS '已验证代数：Precheck 成功时记录当时的 configuration_generation（Phase 2 P2-9）';

