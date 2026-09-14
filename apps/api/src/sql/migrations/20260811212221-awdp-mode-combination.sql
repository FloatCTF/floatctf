-- ================================================================================
-- Migration: 20260811212221-awdp-mode-combination
-- 目标：events_mode_combination_check 追加 AWD Plus 合法模式组合。
-- 依赖：20260811212105-add-awdp-family（枚举值 awdp 已在上一事务提交）。
-- ================================================================================

-- AWD Plus 与 AWD 同属攻防类：仅允许 competition + team。
-- 先删旧约束再重建（PostgreSQL 无 ALTER CONSTRAINT 追加子句）。
ALTER TABLE public.events DROP CONSTRAINT IF EXISTS events_mode_combination_check;
ALTER TABLE public.events
    ADD CONSTRAINT events_mode_combination_check CHECK (
        (family = 'jeopardy' AND purpose = 'practice' AND participant_mode = 'individual')
        OR (family = 'jeopardy' AND purpose = 'competition' AND participant_mode IN ('individual', 'team'))
        OR (family = 'awd' AND purpose = 'competition' AND participant_mode = 'team')
        OR (family = 'awdp' AND purpose = 'competition' AND participant_mode = 'team')
    );
