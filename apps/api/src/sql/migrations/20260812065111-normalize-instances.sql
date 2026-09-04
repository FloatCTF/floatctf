-- ================================================================================
-- Migration: 20260812065111-normalize-instances
-- 目标：落实「归一化 Instance」设计——instances 是唯一 runtime 身份根表，
--       各赛制用自己的关联表挂上去（AWDP 已如此：instances + awdp_instances）。
--       本迁移把两条 legacy 自包含表也改造成「关联表」形态：
--
--   1) challenge_instances（Jeopardy）
--      - id 即 instances.id（1:1，id 上新增 FK → instances ON DELETE CASCADE）；
--      - status / identifier / destroy_at 移入 instances
--        （runtime_state / container_name / expires_at），本表只留领域数据
--        （flag / content / challenge_id / user_id / event_id / team_id）；
--      - 存量回填：每行按其 identifier 在 instances 建对应行（completed 旧实例也保留）。
--
--   2) awd_gamebox_instances（AWD）
--      - 新增 instance_id UUID NOT NULL UNIQUE，FK → instances ON DELETE CASCADE；
--      - container_name / current_container_id / runtime_generation 移入 instances
--        （container_name / container_id / runtime_generation），本表只留 AWD 领域状态
--        （status=GameboxStatus、gamebox_ip、health_status、reset_protection 等）；
--      - status 保持 11 态 GameboxStatus（AWD 编排语义），instances.runtime_state
--        存粗粒度通用生命周期（backfill 映射见下）。
--
--   instances.runtime_state CHECK 扩展 'completed'（challenge 终态）。
--   幂等：所有 DDL 带 IF [NOT] EXISTS / 先检查再操作；可重复执行。
-- ================================================================================

-- ── 0) instances.runtime_state 扩展 'completed' ────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'instances_runtime_state_check'
          AND pg_get_constraintdef(oid) LIKE '%completed%'
    ) THEN
        ALTER TABLE public.instances DROP CONSTRAINT IF EXISTS instances_runtime_state_check;
        ALTER TABLE public.instances
            ADD CONSTRAINT instances_runtime_state_check
            CHECK (runtime_state IN ('pending', 'starting', 'running', 'stopped', 'failed', 'completed'));
    END IF;
END $$;

-- ── 1) challenge_instances ─────────────────────────────────────────────────────
-- 回填 instances：id 复用 challenge_instances.id（1:1），identifier → container_name，
-- status → runtime_state（completed 保留），destroy_at → expires_at。
-- legacy 数据存在相同 identifier 的 completed 行（如 JP-xxx × 6），
-- container_name 需去重：首行保留原名，重复行追加 id 前 8 位（completed 行无容器，后缀无副作用）。
INSERT INTO public.instances (
    id, owner_user_id, owner_team_id, image_ref, container_id, container_name,
    runtime_state, runtime_generation, created_at, started_at, stopped_at,
    expires_at, updated_at
)
SELECT
    ci.id,
    ci.user_id,
    NULL,
    COALESCE(c.image_repo_digest, c.image_id),
    NULL,
    CASE WHEN ci.rn = 1
         THEN ci.identifier
         ELSE ci.identifier || '-' || substring(ci.id::text, 1, 8)
    END,
    ci.status::text,
    1,
    ci.created_at,
    ci.created_at,
    CASE WHEN ci.status = 'completed' THEN ci.updated_at ELSE NULL END,
    ci.destroy_at,
    ci.updated_at
FROM (
    SELECT *, row_number() OVER (PARTITION BY identifier ORDER BY created_at, id) AS rn
    FROM public.challenge_instances
) ci
JOIN public.challenges c ON c.id = ci.challenge_id
WHERE NOT EXISTS (SELECT 1 FROM public.instances i WHERE i.id = ci.id)
  AND NOT EXISTS (
      SELECT 1 FROM public.instances i
      WHERE i.container_name = CASE WHEN ci.rn = 1
                                    THEN ci.identifier
                                    ELSE ci.identifier || '-' || substring(ci.id::text, 1, 8)
                               END
  );

-- id → instances FK（challenge 行 = 1:1 关联行）。
ALTER TABLE public.challenge_instances
    DROP CONSTRAINT IF EXISTS challenge_instances_instance_fk;
ALTER TABLE public.challenge_instances
    ADD CONSTRAINT challenge_instances_instance_fk
    FOREIGN KEY (id) REFERENCES public.instances (id) ON DELETE CASCADE;

-- 运行时列移出 challenge_instances（幂等：存在才删）。
DO $$
DECLARE col TEXT;
BEGIN
    FOREACH col IN ARRAY ARRAY['status', 'identifier', 'destroy_at']
    LOOP
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'challenge_instances' AND column_name = col
        ) THEN
            EXECUTE format('ALTER TABLE public.challenge_instances DROP COLUMN %I', col);
        END IF;
    END LOOP;
END $$;

COMMENT ON TABLE public.challenge_instances IS
    'Jeopardy 实例关联表：id = instances.id（1:1）；运行时身份在 instances，本表只存题目领域数据';
COMMENT ON COLUMN public.challenge_instances.id IS '实例 id，同时是 instances.id（1:1 关联）';

-- ── 2) awd_gamebox_instances ──────────────────────────────────────────────────
-- 2.1 新增 instance_id（先可空，回填后收紧）。
ALTER TABLE public.awd_gamebox_instances
    ADD COLUMN IF NOT EXISTS instance_id UUID NULL;

-- 2.2 回填：为每行 AWD 实例建 instances 行（新 uuid），
--     container_name → container_name，current_container_id → container_id，
--     runtime_generation 保留，status → 粗粒度 runtime_state。
INSERT INTO public.instances (
    id, owner_user_id, owner_team_id, image_ref, container_id, container_name,
    runtime_state, runtime_generation, created_at, started_at, stopped_at,
    expires_at, updated_at
)
SELECT
    public.uuid_generate_v4(),
    NULL,
    gi.team_id,
    COALESCE(g.image_repo_digest, g.image_id),
    gi.current_container_id,
    gi.container_name,
    CASE gi.status::text
        WHEN 'pending'       THEN 'pending'
        WHEN 'creating'      THEN 'starting'
        WHEN 'running'       THEN 'running'
        WHEN 'ready'         THEN 'running'
        WHEN 'resetting'     THEN 'starting'
        WHEN 'stopped'       THEN 'stopped'
        WHEN 'missing'       THEN 'failed'
        WHEN 'orphan'        THEN 'failed'
        WHEN 'conflict'      THEN 'failed'
        WHEN 'start_failed'  THEN 'failed'
        WHEN 'reset_failed'  THEN 'failed'
    END,
    gi.runtime_generation,
    gi.created_at,
    gi.created_at,
    NULL,
    NULL,
    gi.updated_at
FROM public.awd_gamebox_instances gi
JOIN public.awd_event_gameboxes eg ON eg.id = gi.event_gamebox_id
JOIN public.gameboxes g ON g.id = eg.gamebox_id
WHERE NOT EXISTS (SELECT 1 FROM public.instances i WHERE i.container_name = gi.container_name);

-- 2.3 instance_id 回填（幂等：只填为空的行）。
UPDATE public.awd_gamebox_instances gi
SET instance_id = i.id
FROM public.instances i
WHERE gi.instance_id IS NULL
  AND i.container_name = gi.container_name;

-- 2.4 instance_id 收紧：NOT NULL + UNIQUE + FK。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'awd_gamebox_instances_instance_uidx'
    ) THEN
        ALTER TABLE public.awd_gamebox_instances
            ADD CONSTRAINT awd_gamebox_instances_instance_uidx UNIQUE (instance_id);
    END IF;
END $$;

ALTER TABLE public.awd_gamebox_instances ALTER COLUMN instance_id SET NOT NULL;

ALTER TABLE public.awd_gamebox_instances
    DROP CONSTRAINT IF EXISTS awd_gamebox_instances_instance_fk;
ALTER TABLE public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_instance_fk
    FOREIGN KEY (instance_id) REFERENCES public.instances (id) ON DELETE CASCADE;

-- 2.5 运行时列移出 awd_gamebox_instances（container_name UNIQUE 约束随列删除）。
DO $$
DECLARE col TEXT;
BEGIN
    FOREACH col IN ARRAY ARRAY['container_name', 'current_container_id', 'runtime_generation']
    LOOP
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'awd_gamebox_instances' AND column_name = col
        ) THEN
            EXECUTE format('ALTER TABLE public.awd_gamebox_instances DROP COLUMN %I', col);
        END IF;
    END LOOP;
END $$;

COMMENT ON TABLE public.awd_gamebox_instances IS
    'AWD 实例关联表：instance_id = instances.id（1:1）；运行时在 instances，本表只存 AWD 领域状态（GameboxStatus/gamebox_ip/health）';
COMMENT ON COLUMN public.awd_gamebox_instances.instance_id IS '关联的通用 instances 行（容器/镜像/代际/生命周期）';
