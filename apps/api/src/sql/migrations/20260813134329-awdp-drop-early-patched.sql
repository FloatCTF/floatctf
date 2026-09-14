-- ---------------------------------------------------------------------------
-- 删除 practice early_check 遗留列 early_patched_seq（plan §21/§73）
--
-- early_check（提前 Check）产品能力已移除：玩家主动操作只有 Test Check
-- （Healthcheck + Judge）；official exploit 只能由 official Turn evaluation 执行。
-- 每 Turn 的正常 patch eligibility 不得被 early_patched_seq 绕过。
-- ---------------------------------------------------------------------------

ALTER TABLE public.awdp_runs DROP COLUMN IF EXISTS early_patched_seq;
