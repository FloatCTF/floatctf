-- AWDP 练习阶段控制 + 提前 Check（练习模式手动 break/fix + 一次 check 成功 → 该轮起自动计分）。

-- awdp_runs.early_patched_seq：练习模式「提前 Check」确认修复（PATCHED）后，
-- 记录从哪个回合序号起自动计分（含该轮）。NULL = 未提前确认。
-- 仅练习 run 使用；competition run 恒为 NULL。Fix→Break 回退时清零（新一轮 fix 会话）。
ALTER TABLE public.awdp_runs ADD COLUMN IF NOT EXISTS early_patched_seq INTEGER NULL;

COMMENT ON COLUMN public.awdp_runs.early_patched_seq IS
    '练习模式提前 Check 确认修复的起始回合序号（该轮起自动计分；NULL=未确认；Fix→Break 回退清零）';
