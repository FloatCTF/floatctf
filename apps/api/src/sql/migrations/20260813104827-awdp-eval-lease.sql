-- ---------------------------------------------------------------------------
-- AWDP 评估 durable lease 元数据（Pull + Lease worker 模型）
--
-- 背景：当前评估只有 pending → running → terminal，running 无 stale recovery：
-- worker 被 kill / API crash 后 status=running 的行永远不被重领，对应 round
-- 卡 evaluating、分数永久缺失、后续 patch 被 409 拒绝。
--
-- 本迁移在 awdp_evaluations 增加 durable worker metadata：
--   attempt_count    领取次数（每次 claim +1）
--   claimed_by       当前 lease 持有 worker（worker_id，明文；仅用于归属校验）
--   claimed_at       claim 时间
--   heartbeat_at     最近心跳（worker 存活证据）
--   lease_expires_at lease 到期时间；到期后 running 可被回收重领
--   lease_token_hash lease token 的 sha256 哈希（token 明文不落库）
--
-- status 保持：pending / running / 终态。超过 max_attempts 的评估终态
-- PLATFORM_ERROR（不再重领），不允许永久 running。
-- ---------------------------------------------------------------------------

ALTER TABLE public.awdp_evaluations
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS claimed_by TEXT NULL,
    ADD COLUMN IF NOT EXISTS claimed_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS heartbeat_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS lease_token_hash TEXT NULL;

-- attempt_count 不可为负（防御性 CHECK）。
ALTER TABLE public.awdp_evaluations
    ADD CONSTRAINT awdp_evaluations_attempt_count_nonneg_check
    CHECK (attempt_count >= 0);

-- 终态不应再持有 lease（防御性一致性约束；允许旧行宽松过渡期由应用层收敛）。
ALTER TABLE public.awdp_evaluations
    ADD CONSTRAINT awdp_evaluations_lease_consistency_check
    CHECK (
        status NOT IN ('no_patch', 'service_down', 'functional_broken', 'vulnerable',
                       'patched', 'platform_error')
        OR lease_token_hash IS NULL
    );

COMMENT ON COLUMN public.awdp_evaluations.attempt_count IS
    '领取次数（每次 claim +1）；超过 max_attempts 终态 PLATFORM_ERROR，不再重领';
COMMENT ON COLUMN public.awdp_evaluations.claimed_by IS '当前 lease 持有 worker（worker_id）';
COMMENT ON COLUMN public.awdp_evaluations.claimed_at IS '最近一次 claim 时间';
COMMENT ON COLUMN public.awdp_evaluations.heartbeat_at IS '最近一次心跳时间（worker 存活证据）';
COMMENT ON COLUMN public.awdp_evaluations.lease_expires_at IS
    'lease 到期时间；到期后 status=running 可被回收为 pending 重领';
COMMENT ON COLUMN public.awdp_evaluations.lease_token_hash IS
    'lease token 的 sha256 哈希（token 明文不落库不落日志）';
