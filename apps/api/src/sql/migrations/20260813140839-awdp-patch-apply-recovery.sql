-- ---------------------------------------------------------------------------
-- patch applying stale recovery（plan §43）+ applied_at cutoff 资格
--
-- 背景：patch exec 成功但 DB 未记 APPLIED（exec 与 finish_apply 非同一事务，crash 窗口）
-- 会永久 applying。增加 apply_started_at：
--   - 持久化时记录开始时间；
--   - 下一次 apply 前回收 stale applying（apply_started_at 超过 exec 超时 + 裕量）→ failed
--     + reason「stale applying recovered」；绝不静默视为 APPLIED（无法证明容器 mutation 完整，
--     不自动给本 Turn eligibility），允许用户重新上传。
--
-- 资格语义（plan §45）：has_applied_patch 要求 applied_at <= round.cutoff_at（APPLIED-AT 语义）。
-- ---------------------------------------------------------------------------

ALTER TABLE public.awdp_patch_submissions
    ADD COLUMN IF NOT EXISTS apply_started_at TIMESTAMPTZ NULL;

COMMENT ON COLUMN public.awdp_patch_submissions.apply_started_at IS
    'apply 开始时间（stale applying 回收判定用；exec 超时 + 裕量后视为平台崩溃残留）';
