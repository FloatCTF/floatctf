-- ================================================================================
-- Migration: 20260811212613-awdp-more-combinations
-- 目标：AWD Plus（awdp）开放三种合法模式组合（与 Jeopardy 同构）：
--   (awdp, practice, individual)
--   (awdp, competition, individual)
--   (awdp, competition, team)
-- 说明：覆盖并替换 20260811212221-awdp-mode-combination 中 awdp 仅 team 的限制。
-- ================================================================================

ALTER TABLE public.events DROP CONSTRAINT IF EXISTS events_mode_combination_check;
ALTER TABLE public.events
    ADD CONSTRAINT events_mode_combination_check CHECK (
        (family = 'jeopardy' AND purpose = 'practice' AND participant_mode = 'individual')
        OR (family = 'jeopardy' AND purpose = 'competition' AND participant_mode IN ('individual', 'team'))
        OR (family = 'awd' AND purpose = 'competition' AND participant_mode = 'team')
        OR (
            family = 'awdp'
            AND (
                (purpose = 'practice' AND participant_mode = 'individual')
                OR (purpose = 'competition' AND participant_mode IN ('individual', 'team'))
            )
        )
    );
