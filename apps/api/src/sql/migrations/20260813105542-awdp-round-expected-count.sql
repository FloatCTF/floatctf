-- ---------------------------------------------------------------------------
-- awdp_fix_rounds.expected_eval_count —— 回合评估数量的 durable 快照
--
-- 背景：complete_finished_rounds 此前以「无 pending/running 评估」直接判 completed，
-- 0 条评估也满足 → crash 窗口（round 已置 evaluating 但评估未物化）会整轮跳过。
--
-- 修复：round 物化评估时原子记录 expected_eval_count（该轮应物化的 official 评估数，
-- 即当时已启动实例数）。完成判定必须满足：
--
--   actual_count == expected_eval_count AND 全部终态 AND expected_eval_count > 0
--
-- 不允许 0 条评估的假完成；Fix 中途新启动的实例不属于本轮（快照语义）。
-- ---------------------------------------------------------------------------

ALTER TABLE public.awdp_fix_rounds
    ADD COLUMN IF NOT EXISTS expected_eval_count INTEGER NULL;

COMMENT ON COLUMN public.awdp_fix_rounds.expected_eval_count IS
    '本轮 cutoff 时物化的 official 评估数快照（>=1 才允许判 completed；NULL = 旧数据回退宽松判定）';
