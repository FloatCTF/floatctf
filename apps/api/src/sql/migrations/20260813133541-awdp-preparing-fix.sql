-- ---------------------------------------------------------------------------
-- awdp_phase 增加 preparing_fix —— Break→Fix 的 durable 过渡态（plan §41）
--
-- 背景：旧实现 Break→Fix 的 DB 阶段提交与 Docker reset 分离：
-- phase 先提交为 Fix，随后 reset 全部实例；crash 后残留 Break 可写层进入 Fix，无 reconcile。
--
-- 新 lifecycle：Pending → Break → PreparingFix → Fix → Ended
--   PreparingFix：source 仍锁定、patch 仍禁止、Break flag 禁止；
--   tick 对该阶段所有实例 reconcile pristine（reset 幂等），全部完成才物化回合 + 转 Fix；
--   crash 后下次 tick 继续 reconcile。
-- ---------------------------------------------------------------------------

ALTER TYPE public.awdp_phase ADD VALUE IF NOT EXISTS 'preparing_fix';
