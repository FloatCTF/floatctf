-- ================================================================================
-- Migration: 20260827083920-awd-wave4-score-semantics
-- Wave 4: 最终计分语义 — 移除旧列与 JudgeFix 枚举
-- ================================================================================

-- 1. 移除废弃的计分列（attack_score 已在 Wave 1 添加，替代 break_points/loss_points）
ALTER TABLE awd_event_gameboxes DROP COLUMN IF EXISTS break_points;
ALTER TABLE awd_event_gameboxes DROP COLUMN IF EXISTS loss_points;
ALTER TABLE awd_event_gameboxes DROP COLUMN IF EXISTS fix_points;

-- 2. 清理所有残留的 JudgeFix 计分事件（当前 DB 已为 0 行，保留此清理确保幂等）
-- JudgeFix 语义已被产品规范废弃：Up 不再产生分数
DELETE FROM awd_score_events WHERE event_type = 'judge_fix';

-- 3. 移除 JudgeFix 枚举值
-- PostgreSQL 不直接支持 DROP ENUM VALUE，通过重建实现
ALTER TYPE score_event_type RENAME TO score_event_type_old;

CREATE TYPE score_event_type AS ENUM (
    'attack',
    'victim_loss',
    'judge_down',
    'first_bonus',
    'reset_penalty',
    'adjustment',
    'initial_score'
);

-- 更新 awd_score_events 列到新枚举类型
ALTER TABLE awd_score_events
    ALTER COLUMN event_type TYPE score_event_type
    USING event_type::text::score_event_type;

-- 删除旧枚举类型
DROP TYPE score_event_type_old;