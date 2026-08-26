-- ================================================================================
-- Migration: 20260826175415-awd-wave1-config-foundation
-- Wave 1: AWD competition config foundation
-- ================================================================================
-- ADDITIONS:
--   awd_events.round_count       — NULL ≈ not configured yet
--   awd_events.initial_score     — team baseline score
--   awd_event_gameboxes.attack_score  — symmetric attack score
--   awd_judge_tasks.lease_*      — pull+lease foundation (nullable)
--   ScoreEventType::InitialScore — new enum variant
-- RENAMES:
--   awd_event_gameboxes.down_points → judge_down_penalty
-- PRESERVED (temporary — removed in later waves):
--   break_points, loss_points, fix_points, reset_protection_secs,
--   reset_protection_until, grace_ends_at, JudgeFix, VictimLoss,
--   timed unban infrastructure
-- ================================================================================

-- ── awd_events ──

ALTER TABLE public.awd_events
    ADD COLUMN round_count integer NULL;

COMMENT ON COLUMN public.awd_events.round_count IS
    'Attack 阶段轮次数（NULL = 未配置；配置后必须 > 0）';

ALTER TABLE public.awd_events
    ADD COLUMN initial_score bigint NOT NULL DEFAULT 0;

COMMENT ON COLUMN public.awd_events.initial_score IS
    '战队初始分数（比赛开始前的基础分）';

-- ── awd_event_gameboxes ──

ALTER TABLE public.awd_event_gameboxes
    ADD COLUMN attack_score bigint;

COMMENT ON COLUMN public.awd_event_gameboxes.attack_score IS
    '对称攻击分数：攻击方 +attack_score，防守方 -attack_score（同一 EventGameBox 复用）';

-- 从既有 break_points 初始化新列
UPDATE public.awd_event_gameboxes
   SET attack_score = break_points
 WHERE attack_score IS NULL;

-- 初始化后设 NOT NULL
ALTER TABLE public.awd_event_gameboxes
    ALTER COLUMN attack_score SET NOT NULL;

-- 语义重命名：宕机扣分
ALTER TABLE public.awd_event_gameboxes
    RENAME COLUMN down_points TO judge_down_penalty;

COMMENT ON COLUMN public.awd_event_gameboxes.judge_down_penalty IS
    'Judge Down 扣分（原 down_points；语义不变）';

-- ── awd_judge_tasks ──

ALTER TABLE public.awd_judge_tasks
    ADD COLUMN worker_id text NULL;

COMMENT ON COLUMN public.awd_judge_tasks.worker_id IS
    'Pull+Lease：领取任务的 worker 标识（NULL = 未领取）';

ALTER TABLE public.awd_judge_tasks
    ADD COLUMN lease_token_hash text NULL;

COMMENT ON COLUMN public.awd_judge_tasks.lease_token_hash IS
    'Pull+Lease：lease token 的 SHA-256 哈希（不存明文）';

ALTER TABLE public.awd_judge_tasks
    ADD COLUMN lease_expires_at timestamp with time zone NULL;

COMMENT ON COLUMN public.awd_judge_tasks.lease_expires_at IS
    'Pull+Lease：lease 到期时间（过期后可由其他 worker 回收）';

ALTER TABLE public.awd_judge_tasks
    ADD COLUMN heartbeat_at timestamp with time zone NULL;

COMMENT ON COLUMN public.awd_judge_tasks.heartbeat_at IS
    'Pull+Lease：最近一次心跳时间';

ALTER TABLE public.awd_judge_tasks
    ADD COLUMN claimed_at timestamp with time zone NULL;

COMMENT ON COLUMN public.awd_judge_tasks.claimed_at IS
    'Pull+Lease：任务被领取的时间';

-- ── ScoreEventType ──

ALTER TYPE public.score_event_type ADD VALUE IF NOT EXISTS 'initial_score';

COMMENT ON TYPE public.score_event_type IS
    '事件类型：attack 攻击得分 / victim_loss 受害失分 / judge_fix 修复 / judge_down 宕机 / first_bonus 首破 / reset_penalty 重置惩罚 / adjustment 人工调整 / initial_score 初始分';