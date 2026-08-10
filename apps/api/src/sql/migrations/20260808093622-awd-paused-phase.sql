-- ================================================================================
-- Migration: 20260808093622-awd-paused-phase
-- Created at: 2026-08-08
-- ================================================================================
-- Phase 0 P0-1b：resume 需要恢复暂停前的比赛阶段（原实现硬编码 Attack 是已知缺陷，
-- Phase 4 P4-8 依赖此列）。暂停时经 transition_event 原子写入 paused_phase。
-- ================================================================================


ALTER TABLE "awd_events"
    ADD COLUMN IF NOT EXISTS "paused_phase" "awd_phase";

COMMENT ON COLUMN "awd_events"."paused_phase" IS '暂停前所处的比赛阶段（resume 时恢复，Phase 0 P0-1b 引入）';

