-- ================================================================================
-- Migration: 20260807105735-add-challenge-solves-event-id
-- Created at: 2026-08-07 10:57:35 +0800
--
-- Restore `challenge_solves.event_id`, consumed by:
--   - GET /api/challenge_solves  (filter by event_id)
--   - GET /api/challenge_solves/top15users  (EventId.is_null() -> practice only)
--   - event submission service (record solve against an event)
-- The column was present in the entity/code but never existed in the schema.
-- Safe to re-run (IF NOT EXISTS).
-- ================================================================================

BEGIN;

ALTER TABLE "challenge_solves"
    ADD COLUMN IF NOT EXISTS "event_id" UUID REFERENCES "events" ("id") ON DELETE CASCADE;

COMMENT ON COLUMN "challenge_solves"."event_id" IS
    '所属赛事 ID（NULL=独立/练习解题）';

COMMENT ON TABLE "challenge_solves" IS
    '独立解题记录：练习模式的解题流水（event_id 为空）；赛事解题另有 event_challenge_solves';

COMMIT;
