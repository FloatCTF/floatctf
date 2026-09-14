-- ================================================================================
-- Migration: 20260812070138-event-instances-single-root
-- 目标：落实「单一归一化实例根 event_instances」——event 维度的实例只有一张根表，
--       普通 instances 退役（数据迁入后 DROP）。
--       背景：jeopardy 练习本就挂在 event 上；AWDP 训练场（practice）挂「虚拟 event」，
--       因此不需要非 event 的通用 instances 根。
--
-- 结构变化：
--   1) events 增加 is_virtual 标记（虚拟训练 event，列表过滤）。
--   2) AWDP practice run 挂虚拟 event：每 (owner_user_id, gamebox_id) 一个，
--      system_key = 'awdp-practice:{uid}:{gid}'（幂等）；awdp_runs.event_id 回填后
--      NOT NULL，CHECK 重写为「practice=gamebox+owner_user」/「competition=纯 event」
--      二选一；活跃唯一索引收敛为 awdp_runs_event_active_uidx（event_id WHERE active）。
--   3) awdp_instances.event_id 回填后 NOT NULL。
--   4) 新建 event_instances 根表（event_id + owner_user/team + 运行时字段）。
--   5) instances → event_instances 数据迁移（三 family join 解析 event_id/owner），
--      6 个 FK 改挂 event_instances，随后 DROP instances。
--   6) challenge_instances → event_challenge_instance；awd_gamebox_instances → event_gamebox_instances。
--
-- 幂等：所有 DDL 带 IF [NOT] EXISTS / 先检查再操作；可重复执行。
-- 守卫：instances 若存在无 family 引用的孤儿行则 RAISE（数据会丢失，不允许静默）。
-- ================================================================================

-- ── 1) events.is_virtual ───────────────────────────────────────────────────────
ALTER TABLE public.events ADD COLUMN IF NOT EXISTS is_virtual BOOLEAN NOT NULL DEFAULT false;
COMMENT ON COLUMN public.events.is_virtual IS '虚拟训练场事件（AWDP practice 挂靠），不出现在赛事列表/管理列表';

-- ── 2) AWDP practice run 挂虚拟 event ──────────────────────────────────────────
-- 2.1 为每个 (owner_user_id, gamebox_id) 创建虚拟训练 event（幂等：system_key 唯一）。
INSERT INTO public.events (
    id, title, description, hidden, start_time, rules, allow_join,
    flag_prefix, end_time, created_at, updated_at,
    family, purpose, participant_mode, system_key, is_virtual
)
SELECT
    public.uuid_generate_v4(),
    u.nickname || ' 的 ' || g.name || ' 训练场',
    'AWDP 训练（虚拟赛事）',
    true, now(), '', false,
    NULL, NULL, now(), now(),
    'awdp', 'practice', 'individual',
    'awdp-practice:' || r.owner_user_id::text || ':' || r.gamebox_id::text,
    true
FROM (
    SELECT DISTINCT owner_user_id, gamebox_id
    FROM public.awdp_runs
    WHERE event_id IS NULL
) r
JOIN public.users u ON u.id = r.owner_user_id
JOIN public.gameboxes g ON g.id = r.gamebox_id
WHERE NOT EXISTS (
    SELECT 1 FROM public.events e
    WHERE e.system_key = 'awdp-practice:' || r.owner_user_id::text || ':' || r.gamebox_id::text
);

-- 2.2 旧 CHECK 先撤（practice 要求 event_id IS NULL，回填会违反）；回填后收紧 NOT NULL 并加新 CHECK。
ALTER TABLE public.awdp_runs
    DROP CONSTRAINT IF EXISTS awdp_runs_exactly_one_scope_check;

-- 2.3 practice run 回填 event_id。
UPDATE public.awdp_runs r
SET event_id = e.id
FROM public.events e
WHERE r.event_id IS NULL
  AND e.system_key = 'awdp-practice:' || r.owner_user_id::text || ':' || r.gamebox_id::text;

-- 2.4 event_id 收紧 NOT NULL；CHECK 重写（practice=gamebox+owner_user / competition=纯 event）。
ALTER TABLE public.awdp_runs ALTER COLUMN event_id SET NOT NULL;

ALTER TABLE public.awdp_runs
    ADD CONSTRAINT awdp_runs_exactly_one_scope_check CHECK (
        -- practice：虚拟 event + gamebox + owner_user（AWDP practice 仅 individual）
        (gamebox_id IS NOT NULL AND owner_user_id IS NOT NULL AND owner_team_id IS NULL)
        OR
        -- competition：纯 event 级共享 run（主体在域表）
        (gamebox_id IS NULL AND owner_user_id IS NULL AND owner_team_id IS NULL)
    );

-- 2.5 活跃唯一索引收敛：practice 也由 event_id 唯一保证（每虚拟 event 至多一个活跃 run）。
DROP INDEX IF EXISTS awdp_runs_practice_active_uidx;
-- awdp_runs_event_active_uidx（event_id WHERE phase IN (pending,break,fix)）保留并覆盖 practice。

-- ── 3) awdp_instances.event_id 回填 + NOT NULL ─────────────────────────────────
UPDATE public.awdp_instances ai
SET event_id = r.event_id
FROM public.awdp_runs r
WHERE ai.event_id IS NULL
  AND r.id = ai.run_id;

ALTER TABLE public.awdp_instances ALTER COLUMN event_id SET NOT NULL;

-- ── 4) event_instances 归一化根 ────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.event_instances (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    owner_user_id UUID NULL,
    owner_team_id UUID NULL,
    image_ref TEXT NULL,
    container_id TEXT NULL,
    container_name TEXT NOT NULL,
    runtime_state TEXT NOT NULL DEFAULT 'pending',
    runtime_generation BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ NULL,
    stopped_at TIMESTAMPTZ NULL,
    expires_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT event_instances_at_least_one_owner_check
        CHECK (owner_user_id IS NOT NULL OR owner_team_id IS NOT NULL),
    CONSTRAINT event_instances_runtime_state_check
        CHECK (runtime_state IN ('pending', 'starting', 'running', 'stopped', 'failed', 'completed')),
    CONSTRAINT event_instances_runtime_generation_positive_check
        CHECK (runtime_generation >= 1),
    CONSTRAINT event_instances_container_name_uidx UNIQUE (container_name)
);

ALTER TABLE public.event_instances
    ADD CONSTRAINT event_instances_owner_user_fk
    FOREIGN KEY (owner_user_id) REFERENCES public.users (id) ON DELETE CASCADE;
ALTER TABLE public.event_instances
    ADD CONSTRAINT event_instances_owner_team_fk
    FOREIGN KEY (event_id, owner_team_id)
    REFERENCES public.event_teams (event_id, id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_event_instances_event ON public.event_instances (event_id);
CREATE INDEX IF NOT EXISTS idx_event_instances_owner_user ON public.event_instances (owner_user_id);
CREATE INDEX IF NOT EXISTS idx_event_instances_owner_team ON public.event_instances (owner_team_id);

COMMENT ON TABLE public.event_instances IS
    'event 维度单一归一化实例根：容器/镜像/代际/生命周期/过期；各赛制经关联表挂靠（event_challenge_instance / event_gamebox_instances / awdp_instances）';
COMMENT ON COLUMN public.event_instances.event_id IS '所属赛事（虚拟训练 event 也在此）';
COMMENT ON COLUMN public.event_instances.owner_user_id IS '个人归属（可空；与 owner_team_id 至少一个非空，允许双 owner=战队模式个人启动）';
COMMENT ON COLUMN public.event_instances.owner_team_id IS '战队归属（event_teams 复合 FK）';
COMMENT ON COLUMN public.event_instances.container_name IS '逻辑实例稳定容器名；reset 后同名重建（public endpoint 不变）';
COMMENT ON COLUMN public.event_instances.runtime_generation IS 'runtime 代际：初始 1，reset/recreate +1，同容器 restart 不变';

-- ── 5) instances → event_instances 数据迁移 ────────────────────────────────────
-- 5.1 challenge（jeopardy）：id 复用 challenge_instances.id。
INSERT INTO public.event_instances (
    id, event_id, owner_user_id, owner_team_id, image_ref, container_id,
    container_name, runtime_state, runtime_generation,
    created_at, started_at, stopped_at, expires_at, updated_at
)
SELECT
    i.id, ci.event_id, ci.user_id, ci.team_id, i.image_ref, i.container_id,
    i.container_name, i.runtime_state, i.runtime_generation,
    i.created_at, i.started_at, i.stopped_at, i.expires_at, i.updated_at
FROM public.instances i
JOIN public.challenge_instances ci ON ci.id = i.id
ON CONFLICT (id) DO NOTHING;

-- 5.2 awd：instance_id = awd_gamebox_instances.instance_id（team 归属）。
INSERT INTO public.event_instances (
    id, event_id, owner_user_id, owner_team_id, image_ref, container_id,
    container_name, runtime_state, runtime_generation,
    created_at, started_at, stopped_at, expires_at, updated_at
)
SELECT
    i.id, gi.event_id, NULL, gi.team_id, i.image_ref, i.container_id,
    i.container_name, i.runtime_state, i.runtime_generation,
    i.created_at, i.started_at, i.stopped_at, i.expires_at, i.updated_at
FROM public.instances i
JOIN public.awd_gamebox_instances gi ON gi.instance_id = i.id
ON CONFLICT (id) DO NOTHING;

-- 5.3 awdp：event_id 取 awdp_instances 或所属 run（practice 虚拟 event 已在步骤 2 回填）。
INSERT INTO public.event_instances (
    id, event_id, owner_user_id, owner_team_id, image_ref, container_id,
    container_name, runtime_state, runtime_generation,
    created_at, started_at, stopped_at, expires_at, updated_at
)
SELECT
    i.id, COALESCE(ai.event_id, r.event_id), ai.owner_user_id, ai.owner_team_id,
    i.image_ref, i.container_id,
    i.container_name, i.runtime_state, i.runtime_generation,
    i.created_at, i.started_at, i.stopped_at, i.expires_at, i.updated_at
FROM public.instances i
JOIN public.awdp_instances ai ON ai.instance_id = i.id
LEFT JOIN public.awdp_runs r ON r.id = ai.run_id
ON CONFLICT (id) DO NOTHING;

-- ── 6) 6 个 FK 改挂 event_instances ────────────────────────────────────────────
ALTER TABLE public.challenge_instances
    DROP CONSTRAINT IF EXISTS challenge_instances_instance_fk;
ALTER TABLE public.challenge_instances
    ADD CONSTRAINT challenge_instances_instance_fk
    FOREIGN KEY (id) REFERENCES public.event_instances (id) ON DELETE CASCADE;

ALTER TABLE public.awd_gamebox_instances
    DROP CONSTRAINT IF EXISTS awd_gamebox_instances_instance_fk;
ALTER TABLE public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_instance_fk
    FOREIGN KEY (instance_id) REFERENCES public.event_instances (id) ON DELETE CASCADE;

ALTER TABLE public.awdp_instances
    DROP CONSTRAINT IF EXISTS awdp_instances_instance_id_fkey;
ALTER TABLE public.awdp_instances
    ADD CONSTRAINT awdp_instances_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES public.event_instances (id) ON DELETE CASCADE;

ALTER TABLE public.awdp_patch_submissions
    DROP CONSTRAINT IF EXISTS awdp_patch_submissions_instance_id_fkey;
ALTER TABLE public.awdp_patch_submissions
    ADD CONSTRAINT awdp_patch_submissions_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES public.event_instances (id) ON DELETE CASCADE;

ALTER TABLE public.awdp_evaluations
    DROP CONSTRAINT IF EXISTS awdp_evaluations_instance_id_fkey;
ALTER TABLE public.awdp_evaluations
    ADD CONSTRAINT awdp_evaluations_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES public.event_instances (id) ON DELETE CASCADE;

ALTER TABLE public.instance_endpoints
    DROP CONSTRAINT IF EXISTS instance_endpoints_instance_id_fkey;
ALTER TABLE public.instance_endpoints
    ADD CONSTRAINT instance_endpoints_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES public.event_instances (id) ON DELETE CASCADE;

-- ── 7) 守卫 + instances 退役 ───────────────────────────────────────────────────
DO $$
DECLARE orphan_cnt bigint;
BEGIN
    SELECT count(*) INTO orphan_cnt
    FROM public.instances i
    WHERE NOT EXISTS (SELECT 1 FROM public.challenge_instances c WHERE c.id = i.id)
      AND NOT EXISTS (SELECT 1 FROM public.awd_gamebox_instances a WHERE a.instance_id = i.id)
      AND NOT EXISTS (SELECT 1 FROM public.awdp_instances w WHERE w.instance_id = i.id);
    IF orphan_cnt > 0 THEN
        RAISE EXCEPTION 'event-instances-single-root migration: % orphan instances rows (no family ref, cannot migrate)', orphan_cnt;
    END IF;
END $$;

DROP TABLE IF EXISTS public.instances;

-- ── 8) 关联表更名（派生子表，命名对齐 event_instances） ─────────────────────────
ALTER TABLE public.challenge_instances RENAME TO event_challenge_instance;
ALTER TABLE public.awd_gamebox_instances RENAME TO event_gamebox_instances;

COMMENT ON TABLE public.event_challenge_instance IS
    'Jeopardy 实例关联表：id = event_instances.id（1:1）；运行时在 event_instances，本表只存题目领域数据（flag/content/challenge/user/team）';
COMMENT ON TABLE public.event_gamebox_instances IS
    'AWD 实例关联表：instance_id = event_instances.id（1:1）；运行时在 event_instances，本表只存 AWD 领域状态（GameboxStatus/gamebox_ip/health）';
