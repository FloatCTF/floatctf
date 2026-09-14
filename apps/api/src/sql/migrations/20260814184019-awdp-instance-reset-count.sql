-- AWDP 实例重置次数：比赛（Competition）每人/队每个 gamebox 仅允许有限次手动 Reset
-- （默认 3 次，由业务代码判定上限；系统级 reset——Break→Fix 自动 pristine 重建、
-- 管理端重置——不消耗次数，因此独立于 runtime_generation 计数）。
-- 练习（虚拟训练）实例不受限制。
ALTER TABLE public.awdp_instances
    ADD COLUMN IF NOT EXISTS reset_count BIGINT NOT NULL DEFAULT 0;

COMMENT ON COLUMN public.awdp_instances.reset_count IS
    '玩家手动 Reset 次数（比赛按 subject×gamebox 计，上限由业务判定，默认 3）；系统级 reset 不计入';
