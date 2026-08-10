-- ================================================================================
-- Migration: 20260808183946-awd-gamebox-domain-c
-- ================================================================================
-- GameBox 领域模型重构 — Migration C：回填后的约束收紧。
--
-- 仅在 Migration B 回填完成后执行（依赖所有旧行已获得 event_gamebox_id）。
-- ================================================================================


-- ──────────────────────────────────────────────────────────────────────────────
-- 1. awd_gamebox_instances：event_gamebox_id 强制 + FK + 新 UNIQUE
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_gamebox_instances"
    ALTER COLUMN "event_gamebox_id" SET NOT NULL;

ALTER TABLE "awd_gamebox_instances"
    ADD CONSTRAINT "awd_gamebox_instances_event_gamebox_fk"
    FOREIGN KEY ("event_gamebox_id")
    REFERENCES "awd_event_gameboxes" ("id")
    ON DELETE RESTRICT;

-- 旧 UNIQUE(event_id, template_id, team_id) 被新 UNIQUE(event_id, event_gamebox_id, team_id) 取代
ALTER TABLE "awd_gamebox_instances"
    DROP CONSTRAINT IF EXISTS "awd_gamebox_instances_event_id_template_id_team_id_key";

ALTER TABLE "awd_gamebox_instances"
    ADD CONSTRAINT "awd_gamebox_instances_event_gamebox_team_key"
    UNIQUE ("event_id", "event_gamebox_id", "team_id");

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. awd_judge_tasks / awd_score_events：FK（SET NULL 保历史，§57）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_judge_tasks"
    ADD CONSTRAINT "awd_judge_tasks_event_gamebox_fk"
    FOREIGN KEY ("event_gamebox_id")
    REFERENCES "awd_event_gameboxes" ("id")
    ON DELETE SET NULL;

ALTER TABLE "awd_score_events"
    ADD CONSTRAINT "awd_score_events_event_gamebox_fk"
    FOREIGN KEY ("event_gamebox_id")
    REFERENCES "awd_event_gameboxes" ("id")
    ON DELETE SET NULL;

COMMENT ON COLUMN "awd_judge_tasks"."event_gamebox_id" IS '判题目标 EventGameBox（SET NULL：EventGameBox 删除后保留历史行）';
COMMENT ON COLUMN "awd_score_events"."event_gamebox_id" IS '计分作用域 EventGameBox（SET NULL：EventGameBox 删除后保留历史行）';

