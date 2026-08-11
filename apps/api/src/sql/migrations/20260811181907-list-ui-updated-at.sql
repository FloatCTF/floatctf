-- ================================================================================
-- Migration: 20260811181907-list-ui-updated-at
-- 列表页时间列补充 updated_at
-- ================================================================================

-- 1. challenge_sets 增加 updated_at（列表页用 updated_at 替代 created_at 展示）
ALTER TABLE public.challenge_sets
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- 2. challenge_writeup 增加 updated_at（writeup 列表页用 updated_at 替代 created_at）
ALTER TABLE public.challenge_writeup
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- 3. jeopardy_challenge_solves 增加 updated_at（solves 列表页用 updated_at 替代 created_at）
ALTER TABLE public.jeopardy_challenge_solves
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- 4. 已有行回填：将 updated_at 初始化为 created_at（保持历史语义，不把旧数据标为"刚更新"）
UPDATE public.challenge_sets
   SET updated_at = created_at
 WHERE updated_at <> created_at;

UPDATE public.challenge_writeup
   SET updated_at = created_at
 WHERE updated_at <> created_at;

UPDATE public.jeopardy_challenge_solves
   SET updated_at = created_at
 WHERE updated_at <> created_at;

-- 5. 增加 updated_at 自动维护触发器（复用既有 update_updated_at_column 函数）
DROP TRIGGER IF EXISTS trg_challenge_sets_updated_at ON public.challenge_sets;
CREATE TRIGGER trg_challenge_sets_updated_at
    BEFORE UPDATE ON public.challenge_sets
    FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

DROP TRIGGER IF EXISTS trg_challenge_writeup_updated_at ON public.challenge_writeup;
CREATE TRIGGER trg_challenge_writeup_updated_at
    BEFORE UPDATE ON public.challenge_writeup
    FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

DROP TRIGGER IF EXISTS trg_jeopardy_challenge_solves_updated_at ON public.jeopardy_challenge_solves;
CREATE TRIGGER trg_jeopardy_challenge_solves_updated_at
    BEFORE UPDATE ON public.jeopardy_challenge_solves
    FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();
