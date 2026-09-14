-- ================================================================================
-- Migration: 20260812002608-awdp-core-domain
-- 目标：AWDP（AWD Plus）核心领域 Schema —— 独立 bounded context。
--
-- 设计约束（chore/plans/implement-awdp.md）：
--   - 不复用 awd_rounds / awd_score_events / awd_flag_submissions / awd_gamebox_instances；
--   - runtime identity 使用通用 instances（Phase A），awdp_instances 仅为 extension；
--   - 所有子表挂 assert_event_family('awdp') trigger；
--   - Practice 语义修正：awdp practice 允许 end_time NOT NULL（plan §4）。
--
-- 时间模型：pending → break → fix → ended（4 态，不复制 AWD 14 状态机）。
-- 默认值：break=3600s / fix=3600s / interval=600s（6 轮）/ break_score=1000 / fix_round_score=150。
-- ================================================================================

-- ---------------------------------------------------------------------------
-- 0. events.end_time CHECK 修正：awdp practice 允许有界（plan §4）
-- ---------------------------------------------------------------------------
ALTER TABLE public.events DROP CONSTRAINT IF EXISTS events_end_time_by_purpose_check;

ALTER TABLE public.events
    ADD CONSTRAINT events_end_time_by_purpose_check CHECK (
        -- 普通 practice（jeopardy）永远不结束
        (purpose = 'practice' AND family <> 'awdp' AND end_time IS NULL)
        OR
        -- awdp practice：可以有界（end_time 与 break+fix 时长一致）也可以无界
        (purpose = 'practice' AND family = 'awdp'
         AND (end_time IS NULL OR (end_time IS NOT NULL AND start_time < end_time)))
        OR
        -- competition 一律必填 end_time
        (purpose = 'competition' AND end_time IS NOT NULL AND start_time < end_time)
    );

COMMENT ON CONSTRAINT events_end_time_by_purpose_check ON public.events IS
    'practice 语义按 family 分支：普通 practice 无界；awdp practice 可有界；competition 必填 end_time';

-- ---------------------------------------------------------------------------
-- 1. 枚举
-- ---------------------------------------------------------------------------
DO $$ BEGIN
    CREATE TYPE public.awdp_phase AS ENUM ('pending', 'break', 'fix', 'ended');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
    CREATE TYPE public.awdp_evaluation_kind AS ENUM ('manual', 'official');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
    CREATE TYPE public.awdp_evaluation_status AS ENUM (
        'pending', 'running',
        'no_patch', 'service_down', 'functional_broken', 'vulnerable', 'patched',
        'platform_error'
    );
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- ---------------------------------------------------------------------------
-- 2. awdp_events（1:1 events，类型化配置 + 阶段状态机）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_events (
    event_id UUID PRIMARY KEY REFERENCES public.events (id) ON DELETE CASCADE,
    phase public.awdp_phase NOT NULL DEFAULT 'pending',
    break_duration_secs INTEGER NOT NULL DEFAULT 3600,
    fix_duration_secs INTEGER NOT NULL DEFAULT 3600,
    fix_round_interval_secs INTEGER NOT NULL DEFAULT 600,
    break_score BIGINT NOT NULL DEFAULT 1000,
    fix_round_score BIGINT NOT NULL DEFAULT 150,
    configuration_generation BIGINT NOT NULL DEFAULT 1,
    started_at TIMESTAMPTZ NULL,
    break_ends_at TIMESTAMPTZ NULL,
    fix_started_at TIMESTAMPTZ NULL,
    fix_ends_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    -- 0 = 尚未开始 Fix；Fix 期间 = 已开始的轮次数
    current_round INTEGER NOT NULL DEFAULT 0,
    -- tick 驱动的下一个动作时间（phase 切换 / round cutoff）
    next_action_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_events_durations_positive_check
        CHECK (break_duration_secs > 0 AND fix_duration_secs > 0 AND fix_round_interval_secs > 0),
    -- V1 强制：fix 时长必须被 interval 整除（不支持 partial round）
    CONSTRAINT awdp_events_interval_divisibility_check
        CHECK (fix_duration_secs % fix_round_interval_secs = 0),
    CONSTRAINT awdp_events_scores_positive_check
        CHECK (break_score >= 0 AND fix_round_score >= 0),
    CONSTRAINT awdp_events_current_round_nonneg_check CHECK (current_round >= 0)
);

COMMENT ON TABLE public.awdp_events IS
    'AWDP 赛事配置 + 阶段状态机（pending/break/fix/ended），1:1 events';
COMMENT ON COLUMN public.awdp_events.phase IS '阶段：pending → break → fix → ended（不可回退）';
COMMENT ON COLUMN public.awdp_events.break_duration_secs IS 'Break 阶段时长（秒），默认 3600';
COMMENT ON COLUMN public.awdp_events.fix_duration_secs IS 'Fix 阶段时长（秒），默认 3600';
COMMENT ON COLUMN public.awdp_events.fix_round_interval_secs IS 'Fix 回合间隔（秒），默认 600；fix_duration % interval = 0';
COMMENT ON COLUMN public.awdp_events.break_score IS '每 EventGameBox Break 一次性得分，默认 1000';
COMMENT ON COLUMN public.awdp_events.fix_round_score IS '每回合 PATCHED 得分，默认 150';
COMMENT ON COLUMN public.awdp_events.configuration_generation IS '配置代数：start 前每次修改 +1（乐观并发）';
COMMENT ON COLUMN public.awdp_events.next_action_at IS 'tick 下一个动作时间（phase 切换或 round cutoff）';

DROP TRIGGER IF EXISTS trg_awdp_events_family ON public.awdp_events;
CREATE TRIGGER trg_awdp_events_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_events
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 3. awdp_event_gameboxes（赛事 GameBox 选择，独立 join 表）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_event_gameboxes (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    gamebox_id UUID NOT NULL REFERENCES public.gameboxes (id) ON DELETE RESTRICT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    hidden BOOLEAN NOT NULL DEFAULT false,
    -- 资源覆盖：attach 时从 GameBox recommended 复制（NOT NULL，与 awd 风格一致）
    cpu_millis BIGINT NOT NULL,
    memory_bytes BIGINT NOT NULL,
    pids_limit BIGINT NOT NULL DEFAULT 100,
    healthcheck_override_json JSONB NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_event_gameboxes_unique UNIQUE (event_id, gamebox_id),
    CONSTRAINT awdp_event_gameboxes_resources_positive_check
        CHECK (cpu_millis > 0 AND memory_bytes > 0 AND pids_limit > 0)
);

COMMENT ON TABLE public.awdp_event_gameboxes IS
    'AWDP 赛事 GameBox 选择（独立于 awd_event_gameboxes）';
COMMENT ON COLUMN public.awdp_event_gameboxes.gamebox_id IS 'GameBox 身份（RESTRICT：被赛事引用后禁止 hard delete）';
COMMENT ON COLUMN public.awdp_event_gameboxes.healthcheck_override_json IS '赛事级 healthcheck 覆盖（NULL=用 GameBox 默认）';

DROP TRIGGER IF EXISTS trg_awdp_event_gameboxes_family ON public.awdp_event_gameboxes;
CREATE TRIGGER trg_awdp_event_gameboxes_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_event_gameboxes
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 4. awdp_instances（instances 的 extension：event/gamebox + 双主体归属镜像）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_instances (
    instance_id UUID PRIMARY KEY REFERENCES public.instances (id) ON DELETE CASCADE,
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    event_gamebox_id UUID NOT NULL REFERENCES public.awdp_event_gameboxes (id) ON DELETE CASCADE,
    -- 归属镜像（与 instances.owner_* 同事务写入，保持一致）
    owner_user_id UUID NULL,
    owner_team_id UUID NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_instances_exactly_one_owner_check
        CHECK ((owner_user_id IS NULL) <> (owner_team_id IS NULL)),
    CONSTRAINT awdp_instances_user_fk
        FOREIGN KEY (owner_user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT awdp_instances_team_fk
        FOREIGN KEY (event_id, owner_team_id)
        REFERENCES public.event_teams (event_id, id) ON DELETE CASCADE
);

-- 每 subject × event_gamebox 至多一个逻辑实例（partial unique 双主体）。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_instances_user_uidx
    ON public.awdp_instances (event_id, event_gamebox_id, owner_user_id)
    WHERE owner_team_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS awdp_instances_team_uidx
    ON public.awdp_instances (event_id, event_gamebox_id, owner_team_id)
    WHERE owner_user_id IS NULL;

COMMENT ON TABLE public.awdp_instances IS
    'AWDP 实例 extension：逻辑实例 ↔ event × event_gamebox × 归属（instances 只负责 runtime）';
COMMENT ON COLUMN public.awdp_instances.owner_user_id IS '归属镜像（Individual）；与 owner_team_id 恰好一个非空';
COMMENT ON COLUMN public.awdp_instances.owner_team_id IS '归属镜像（Team）；Team 成员共享同一实例';

DROP TRIGGER IF EXISTS trg_awdp_instances_family ON public.awdp_instances;
CREATE TRIGGER trg_awdp_instances_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_instances
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 5. awdp_breaks（Break 一次性成功证明；双主体 partial unique）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_breaks (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    event_gamebox_id UUID NOT NULL REFERENCES public.awdp_event_gameboxes (id) ON DELETE CASCADE,
    user_id UUID NULL,
    team_id UUID NULL,
    -- 提交的 flag 哈希（sha256），审计用
    flag_sha256 TEXT NOT NULL,
    broken_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_breaks_exactly_one_owner_check
        CHECK ((user_id IS NULL) <> (team_id IS NULL)),
    CONSTRAINT awdp_breaks_user_fk
        FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT awdp_breaks_team_fk
        FOREIGN KEY (event_id, team_id)
        REFERENCES public.event_teams (event_id, id) ON DELETE CASCADE
);

-- 每 participant × event_gamebox 至多一次（幂等兜底）。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_breaks_user_uidx
    ON public.awdp_breaks (event_id, event_gamebox_id, user_id)
    WHERE team_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS awdp_breaks_team_uidx
    ON public.awdp_breaks (event_id, event_gamebox_id, team_id)
    WHERE user_id IS NULL;

COMMENT ON TABLE public.awdp_breaks IS 'Break 阶段一次性成功记录（flag 提交）；每 participant × gamebox 至多一次';

DROP TRIGGER IF EXISTS trg_awdp_breaks_family ON public.awdp_breaks;
CREATE TRIGGER trg_awdp_breaks_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_breaks
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 6. awdp_fix_rounds（确定性预生成的 Fix 回合时间线，domain 行而非 scheduler task）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_fix_rounds (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    cutoff_at TIMESTAMPTZ NOT NULL,
    -- pending | evaluating | completed
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_fix_rounds_unique UNIQUE (event_id, sequence),
    CONSTRAINT awdp_fix_rounds_status_check
        CHECK (status IN ('pending', 'evaluating', 'completed')),
    CONSTRAINT awdp_fix_rounds_time_order_check CHECK (starts_at < cutoff_at)
);

COMMENT ON TABLE public.awdp_fix_rounds IS
    'Fix 回合时间线（Fix 开始时确定性预生成，不逐轮建 scheduler task）；UNIQUE(event_id,sequence)';

DROP TRIGGER IF EXISTS trg_awdp_fix_rounds_family ON public.awdp_fix_rounds;
CREATE TRIGGER trg_awdp_fix_rounds_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_fix_rounds
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 7. awdp_patch_submissions（patch.sh 提交审计）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_patch_submissions (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    instance_id UUID NOT NULL REFERENCES public.instances (id) ON DELETE CASCADE,
    fix_round_id UUID NULL REFERENCES public.awdp_fix_rounds (id) ON DELETE SET NULL,
    user_id UUID NULL,
    team_id UUID NULL,
    -- patch.sh 内容哈希（sha256 hex）与内容（应用层限制 ≤256KiB）
    script_sha256 TEXT NOT NULL,
    script_content TEXT NOT NULL,
    -- applying | applied | failed
    status TEXT NOT NULL DEFAULT 'applying',
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    apply_started_at TIMESTAMPTZ NULL,
    applied_at TIMESTAMPTZ NULL,
    exit_code INTEGER NULL,
    stdout_limited TEXT NULL,
    stderr_limited TEXT NULL,
    error_message TEXT NULL,
    CONSTRAINT awdp_patch_submissions_exactly_one_owner_check
        CHECK ((user_id IS NULL) <> (team_id IS NULL)),
    CONSTRAINT awdp_patch_submissions_status_check
        CHECK (status IN ('applying', 'applied', 'failed')),
    CONSTRAINT awdp_patch_submissions_user_fk
        FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT awdp_patch_submissions_team_fk
        FOREIGN KEY (event_id, team_id)
        REFERENCES public.event_teams (event_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_awdp_patch_submissions_eligibility
    ON public.awdp_patch_submissions (instance_id, fix_round_id)
    WHERE status = 'applied';

COMMENT ON TABLE public.awdp_patch_submissions IS
    'patch.sh 提交审计；APPLIED 才使该实例在本轮 eligible 评估';

DROP TRIGGER IF EXISTS trg_awdp_patch_submissions_family ON public.awdp_patch_submissions;
CREATE TRIGGER trg_awdp_patch_submissions_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_patch_submissions
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 8. awdp_evaluations（manual / official 评估；official 每 round×instance 唯一）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_evaluations (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    instance_id UUID NOT NULL REFERENCES public.instances (id) ON DELETE CASCADE,
    fix_round_id UUID NULL REFERENCES public.awdp_fix_rounds (id) ON DELETE CASCADE,
    kind public.awdp_evaluation_kind NOT NULL,
    status public.awdp_evaluation_status NOT NULL DEFAULT 'pending',
    healthcheck_result TEXT NULL,
    judge_result TEXT NULL,
    -- 官方 exploit 结果（manual 恒为 NULL）
    exploit_result TEXT NULL,
    stdout_limited TEXT NULL,
    stderr_limited TEXT NULL,
    started_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_evaluations_kind_round_consistent_check CHECK (
        (kind = 'official' AND fix_round_id IS NOT NULL)
        OR (kind = 'manual' AND fix_round_id IS NULL)
    ),
    CONSTRAINT awdp_evaluations_manual_no_exploit_check CHECK (
        kind <> 'manual' OR exploit_result IS NULL
    ),
    CONSTRAINT awdp_evaluations_terminal_status_check CHECK (
        status IN ('pending', 'running', 'no_patch', 'service_down', 'functional_broken',
                   'vulnerable', 'patched', 'platform_error')
    )
);

-- official 每 (fix_round, instance) 唯一（幂等）；manual（fix_round_id NULL）不受影响。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_evaluations_official_uidx
    ON public.awdp_evaluations (fix_round_id, instance_id)
    WHERE kind = 'official';

CREATE INDEX IF NOT EXISTS idx_awdp_evaluations_worker
    ON public.awdp_evaluations (status)
    WHERE status IN ('pending', 'running');

COMMENT ON TABLE public.awdp_evaluations IS
    'AWDP 评估：manual（healthcheck+judge，不计分）与 official（healthcheck→judge→exploit→计分）';
COMMENT ON COLUMN public.awdp_evaluations.status IS
    'pending/running 中间态；no_patch/service_down/functional_broken/vulnerable/patched/platform_error 终态';
COMMENT ON COLUMN public.awdp_evaluations.exploit_result IS '官方 exploit 输出摘要；manual 恒 NULL（CHECK 强制）';

DROP TRIGGER IF EXISTS trg_awdp_evaluations_family ON public.awdp_evaluations;
CREATE TRIGGER trg_awdp_evaluations_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_evaluations
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 9. awdp_score_events（append-only 幂等账本，双主体）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_score_events (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    user_id UUID NULL,
    team_id UUID NULL,
    event_gamebox_id UUID NOT NULL REFERENCES public.awdp_event_gameboxes (id) ON DELETE CASCADE,
    -- break | fix
    score_type TEXT NOT NULL,
    fix_round_id UUID NULL REFERENCES public.awdp_fix_rounds (id) ON DELETE CASCADE,
    delta BIGINT NOT NULL,
    idempotency_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_score_events_exactly_one_owner_check
        CHECK ((user_id IS NULL) <> (team_id IS NULL)),
    CONSTRAINT awdp_score_events_type_check CHECK (score_type IN ('break', 'fix')),
    CONSTRAINT awdp_score_events_fix_round_consistent_check CHECK (
        (score_type = 'fix' AND fix_round_id IS NOT NULL)
        OR (score_type = 'break' AND fix_round_id IS NULL)
    ),
    CONSTRAINT awdp_score_events_user_fk
        FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT awdp_score_events_team_fk
        FOREIGN KEY (event_id, team_id)
        REFERENCES public.event_teams (event_id, id) ON DELETE CASCADE
);

-- 幂等：重复执行不得重复加分（23505 视为成功，照 awd_score_events 模式）。
ALTER TABLE public.awdp_score_events
    ADD CONSTRAINT awdp_score_events_idempotency_key_uidx UNIQUE (idempotency_key);

CREATE INDEX IF NOT EXISTS idx_awdp_score_events_subject
    ON public.awdp_score_events (event_id, user_id, team_id);

COMMENT ON TABLE public.awdp_score_events IS
    'AWDP append-only 计分账本（不复用 awd_score_events）；idempotency_key 全局唯一';
COMMENT ON COLUMN public.awdp_score_events.score_type IS 'break（一次性）/ fix（每轮 PATCHED）';
COMMENT ON COLUMN public.awdp_score_events.idempotency_key IS
    'awdp:break:{event}:{eg}:{subject} / awdp:fix:{event}:{round}:{instance}';

DROP TRIGGER IF EXISTS trg_awdp_score_events_family ON public.awdp_score_events;
CREATE TRIGGER trg_awdp_score_events_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_score_events
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');
