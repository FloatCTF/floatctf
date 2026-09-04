-- ================================================================================
-- Migration: 20260812034705-run-centered-core
-- 目标：AWDP 生命周期根 awdp_runs（Practice 与 Competition 共用引擎）。
--   practice 行   = gamebox_id × owner_user_id（个人练习，无 event，phase 创建即 break）
--   competition 行 = event_id（Event 级共享 run；participant 主体在域表 user_id/team_id）
--   awdp_events 降级为纯配置（删除 phase/started_at/break_ends_at/fix_started_at/fix_ends_at/
--     finished_at/current_round/next_action_at 列）。
--   域表（awdp_instances/awdp_breaks/awdp_fix_rounds/awdp_patch_submissions/awdp_evaluations/
--     awdp_score_events）event_id → run_id、event_gamebox_id → gamebox_id；
--   域表 family trigger 换成 assert_awdp_run()（runs 家族由 trg_awdp_runs_family 保证）。
--   冗余 event_id 列保留（仅 team 复合 FK → event_teams(event_id,id) 需要，CHECK 保证
--     event_id IS NOT NULL OR team 主体 IS NULL）。
--
-- 迁移只前进；backfill 只针对开发库既有 competition 行（无 practice 历史）。
-- ================================================================================

-- ---------------------------------------------------------------------------
-- 0. assert_awdp_run()：域表 run 守卫（run_id 必须存在；runs 家族由自身 trigger 保证）
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.assert_awdp_run()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM public.awdp_runs WHERE id = NEW.run_id) THEN
        RAISE EXCEPTION 'assert_awdp_run: run % not found', NEW.run_id;
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION public.assert_awdp_run() IS
    'AWDP 域表 run 守卫：NEW.run_id 必须存在于 awdp_runs（取代 assert_event_family 的域表用法）';

-- ---------------------------------------------------------------------------
-- 1. awdp_runs：生命周期根（快照 ×5 + timing + 进度）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.awdp_runs (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    -- scope exactly-one：practice 行 = gamebox×owner_user；competition 行 = event（其余全 NULL）
    event_id UUID NULL,
    gamebox_id UUID NULL,
    owner_user_id UUID NULL,
    owner_team_id UUID NULL,
    -- 生命周期（与旧 awdp_events 运行态同型）。
    phase public.awdp_phase NOT NULL DEFAULT 'pending',
    -- 配置快照 ×5（run 启动时从 awdp_events 配置或默认值拷贝，此后冻结）。
    break_duration_secs INTEGER NOT NULL DEFAULT 3600,
    fix_duration_secs INTEGER NOT NULL DEFAULT 3600,
    fix_round_interval_secs INTEGER NOT NULL DEFAULT 600,
    break_score BIGINT NOT NULL DEFAULT 1000,
    fix_round_score BIGINT NOT NULL DEFAULT 150,
    -- timing。
    started_at TIMESTAMPTZ NULL,
    break_ends_at TIMESTAMPTZ NULL,
    fix_started_at TIMESTAMPTZ NULL,
    fix_ends_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    -- 进度：0 = 尚未开始 Fix；Fix 期间 = 已开始轮次数；total_rounds = snapshot 推导。
    current_round INTEGER NOT NULL DEFAULT 0,
    total_rounds INTEGER NOT NULL DEFAULT 6,
    next_action_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- 约束。
    CONSTRAINT awdp_runs_exactly_one_scope_check CHECK (
        -- practice：event_id NULL + gamebox NOT NULL + owner_user NOT NULL + owner_team NULL
        (event_id IS NULL AND gamebox_id IS NOT NULL
         AND owner_user_id IS NOT NULL AND owner_team_id IS NULL)
        OR
        -- competition：event_id NOT NULL + 其余全 NULL（Event 级共享 run）
        (event_id IS NOT NULL AND gamebox_id IS NULL
         AND owner_user_id IS NULL AND owner_team_id IS NULL)
    ),
    CONSTRAINT awdp_runs_durations_positive_check
        CHECK (break_duration_secs > 0 AND fix_duration_secs > 0 AND fix_round_interval_secs > 0),
    CONSTRAINT awdp_runs_interval_divisibility_check
        CHECK (fix_duration_secs % fix_round_interval_secs = 0),
    CONSTRAINT awdp_runs_scores_positive_check
        CHECK (break_score >= 0 AND fix_round_score >= 0),
    CONSTRAINT awdp_runs_progress_check
        CHECK (current_round >= 0 AND total_rounds > 0),
    CONSTRAINT awdp_runs_event_fk
        FOREIGN KEY (event_id) REFERENCES public.events (id) ON DELETE CASCADE,
    CONSTRAINT awdp_runs_gamebox_fk
        FOREIGN KEY (gamebox_id) REFERENCES public.gameboxes (id) ON DELETE RESTRICT,
    CONSTRAINT awdp_runs_owner_user_fk
        FOREIGN KEY (owner_user_id) REFERENCES public.users (id) ON DELETE CASCADE
);

COMMENT ON TABLE public.awdp_runs IS
    'AWDP 生命周期根：practice（gamebox×user）与 competition（event）共用的 Break→Fix→Ended 引擎状态';
COMMENT ON COLUMN public.awdp_runs.event_id IS 'competition run 的赛事（NULL=practice）；scope exactly-one';
COMMENT ON COLUMN public.awdp_runs.gamebox_id IS 'practice run 直接引用 gameboxes（NULL=competition）';
COMMENT ON COLUMN public.awdp_runs.owner_user_id IS 'practice 主体=用户（AWDP practice 仅 individual）；competition 行为 NULL，主体在域表';
COMMENT ON COLUMN public.awdp_runs.phase IS '阶段：pending → break → fix → ended（practice 创建即 break，不走 pending）';
COMMENT ON COLUMN public.awdp_runs.break_duration_secs IS '快照：Break 时长（秒），默认 3600；run 启动后冻结';
COMMENT ON COLUMN public.awdp_runs.fix_duration_secs IS '快照：Fix 时长（秒），默认 3600';
COMMENT ON COLUMN public.awdp_runs.fix_round_interval_secs IS '快照：回合间隔（秒），默认 600；fix % interval = 0';
COMMENT ON COLUMN public.awdp_runs.break_score IS '快照：每 GameBox Break 一次性得分，默认 1000';
COMMENT ON COLUMN public.awdp_runs.fix_round_score IS '快照：每回合 PATCHED 得分，默认 150';
COMMENT ON COLUMN public.awdp_runs.current_round IS '0=未开始 Fix；Fix 期间=已开始轮次数';
COMMENT ON COLUMN public.awdp_runs.total_rounds IS '快照推导 = fix_duration / interval（默认 6）';
COMMENT ON COLUMN public.awdp_runs.next_action_at IS 'tick 下一个动作时间（phase 切换或 round cutoff）';

-- tick 扫描（FOR UPDATE SKIP LOCKED）与并发约束。
CREATE INDEX IF NOT EXISTS idx_awdp_runs_tick
    ON public.awdp_runs (next_action_at)
    WHERE next_action_at IS NOT NULL AND phase <> 'ended';
CREATE INDEX IF NOT EXISTS idx_awdp_runs_event ON public.awdp_runs (event_id);
CREATE INDEX IF NOT EXISTS idx_awdp_runs_owner_user ON public.awdp_runs (owner_user_id);

-- 幂等：同 user+gamebox 至多一个 active practice run（Start Training 重复调用返回既有）。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_runs_practice_active_uidx
    ON public.awdp_runs (gamebox_id, owner_user_id)
    WHERE phase IN ('pending', 'break', 'fix');

-- 幂等：一个 competition event 至多一个 active run（Event start 创建/复用）。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_runs_event_active_uidx
    ON public.awdp_runs (event_id)
    WHERE phase IN ('pending', 'break', 'fix');

-- 家族守卫：competition 行校验 events.family='awdp'（event_id NULL 时函数直接放行）。
DROP TRIGGER IF EXISTS trg_awdp_runs_family ON public.awdp_runs;
CREATE TRIGGER trg_awdp_runs_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awdp_runs
    FOR EACH ROW EXECUTE FUNCTION public.assert_event_family('awdp');

-- ---------------------------------------------------------------------------
-- 2. backfill：开发库既有 awdp_events（competition）→ competition run（快照+timing 原样拷贝）
-- ---------------------------------------------------------------------------
INSERT INTO public.awdp_runs (
    id, event_id, gamebox_id, owner_user_id, owner_team_id, phase,
    break_duration_secs, fix_duration_secs, fix_round_interval_secs, break_score, fix_round_score,
    started_at, break_ends_at, fix_started_at, fix_ends_at, finished_at,
    current_round, total_rounds, next_action_at, created_at, updated_at
)
SELECT
    e.event_id, e.event_id, NULL, NULL, NULL, e.phase,
    e.break_duration_secs, e.fix_duration_secs, e.fix_round_interval_secs, e.break_score, e.fix_round_score,
    e.started_at, e.break_ends_at, e.fix_started_at, e.fix_ends_at, e.finished_at,
    e.current_round, e.fix_duration_secs / e.fix_round_interval_secs, e.next_action_at,
    e.created_at, e.updated_at
FROM public.awdp_events e
WHERE NOT EXISTS (SELECT 1 FROM public.awdp_runs r WHERE r.event_id = e.event_id);

COMMENT ON TABLE public.awdp_runs IS
    'AWDP 生命周期根：practice（gamebox×user）与 competition（event）共用的 Break→Fix→Ended 引擎状态（backfill 后含既有 competition run）';

-- ---------------------------------------------------------------------------
-- 3. 域表 run 化
-- ---------------------------------------------------------------------------

-- ── 3.1 awdp_instances ──
ALTER TABLE public.awdp_instances ADD COLUMN IF NOT EXISTS run_id UUID NULL;
ALTER TABLE public.awdp_instances ADD COLUMN IF NOT EXISTS gamebox_id UUID NULL;

UPDATE public.awdp_instances ext
SET run_id = r.id,
    gamebox_id = eg.gamebox_id
FROM public.awdp_event_gameboxes eg
JOIN public.awdp_runs r ON r.event_id = eg.event_id
WHERE eg.id = ext.event_gamebox_id
  AND ext.run_id IS NULL;

ALTER TABLE public.awdp_instances DROP CONSTRAINT IF EXISTS awdp_instances_event_id_fkey;
ALTER TABLE public.awdp_instances DROP CONSTRAINT IF EXISTS awdp_instances_event_gamebox_id_fkey;
ALTER TABLE public.awdp_instances ALTER COLUMN event_id DROP NOT NULL;
ALTER TABLE public.awdp_instances DROP COLUMN IF EXISTS event_gamebox_id;
DROP INDEX IF EXISTS awdp_instances_user_uidx;
DROP INDEX IF EXISTS awdp_instances_team_uidx;
DROP TRIGGER IF EXISTS trg_awdp_instances_family ON public.awdp_instances;

ALTER TABLE public.awdp_instances ALTER COLUMN run_id SET NOT NULL;
ALTER TABLE public.awdp_instances ALTER COLUMN gamebox_id SET NOT NULL;
ALTER TABLE public.awdp_instances
    ADD CONSTRAINT awdp_instances_run_fk
        FOREIGN KEY (run_id) REFERENCES public.awdp_runs (id) ON DELETE CASCADE,
    ADD CONSTRAINT awdp_instances_gamebox_fk
        FOREIGN KEY (gamebox_id) REFERENCES public.gameboxes (id) ON DELETE RESTRICT,
    ADD CONSTRAINT awdp_instances_team_event_check
        CHECK (event_id IS NOT NULL OR owner_team_id IS NULL);

-- 每 subject × run × gamebox 至多一个逻辑实例（partial unique 双主体）。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_instances_user_uidx
    ON public.awdp_instances (run_id, gamebox_id, owner_user_id)
    WHERE owner_team_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS awdp_instances_team_uidx
    ON public.awdp_instances (run_id, gamebox_id, owner_team_id)
    WHERE owner_user_id IS NULL;

COMMENT ON COLUMN public.awdp_instances.run_id IS '归属 run（practice=个人 run；competition=Event 级共享 run）';
COMMENT ON COLUMN public.awdp_instances.gamebox_id IS 'GameBox 身份（practice 直接引用；competition 对应事件所挂 gamebox）';
COMMENT ON COLUMN public.awdp_instances.event_id IS '冗余：仅 team 复合 FK → event_teams(event_id,id) 需要；practice 行为 NULL';

DROP TRIGGER IF EXISTS trg_awdp_instances_run ON public.awdp_instances;
CREATE TRIGGER trg_awdp_instances_run
    BEFORE INSERT OR UPDATE OF run_id ON public.awdp_instances
    FOR EACH ROW EXECUTE FUNCTION public.assert_awdp_run();

-- ── 3.2 awdp_breaks ──
ALTER TABLE public.awdp_breaks ADD COLUMN IF NOT EXISTS run_id UUID NULL;
ALTER TABLE public.awdp_breaks ADD COLUMN IF NOT EXISTS gamebox_id UUID NULL;

UPDATE public.awdp_breaks b
SET run_id = r.id,
    gamebox_id = eg.gamebox_id
FROM public.awdp_event_gameboxes eg
JOIN public.awdp_runs r ON r.event_id = eg.event_id
WHERE eg.id = b.event_gamebox_id
  AND b.run_id IS NULL;

ALTER TABLE public.awdp_breaks DROP CONSTRAINT IF EXISTS awdp_breaks_event_id_fkey;
ALTER TABLE public.awdp_breaks DROP CONSTRAINT IF EXISTS awdp_breaks_event_gamebox_id_fkey;
ALTER TABLE public.awdp_breaks ALTER COLUMN event_id DROP NOT NULL;
ALTER TABLE public.awdp_breaks DROP COLUMN IF EXISTS event_gamebox_id;
DROP INDEX IF EXISTS awdp_breaks_user_uidx;
DROP INDEX IF EXISTS awdp_breaks_team_uidx;
DROP TRIGGER IF EXISTS trg_awdp_breaks_family ON public.awdp_breaks;

ALTER TABLE public.awdp_breaks ALTER COLUMN run_id SET NOT NULL;
ALTER TABLE public.awdp_breaks ALTER COLUMN gamebox_id SET NOT NULL;
ALTER TABLE public.awdp_breaks
    ADD CONSTRAINT awdp_breaks_run_fk
        FOREIGN KEY (run_id) REFERENCES public.awdp_runs (id) ON DELETE CASCADE,
    ADD CONSTRAINT awdp_breaks_gamebox_fk
        FOREIGN KEY (gamebox_id) REFERENCES public.gameboxes (id) ON DELETE RESTRICT,
    ADD CONSTRAINT awdp_breaks_team_event_check
        CHECK (event_id IS NOT NULL OR team_id IS NULL);

-- 每 participant × run × gamebox 至多一次（幂等兜底）。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_breaks_user_uidx
    ON public.awdp_breaks (run_id, gamebox_id, user_id)
    WHERE team_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS awdp_breaks_team_uidx
    ON public.awdp_breaks (run_id, gamebox_id, team_id)
    WHERE user_id IS NULL;

COMMENT ON COLUMN public.awdp_breaks.run_id IS '归属 run';
COMMENT ON COLUMN public.awdp_breaks.gamebox_id IS 'GameBox 身份（原 event_gamebox_id → 直引 gameboxes）';
COMMENT ON COLUMN public.awdp_breaks.event_id IS '冗余：仅 team 复合 FK 需要；practice 行为 NULL';

DROP TRIGGER IF EXISTS trg_awdp_breaks_run ON public.awdp_breaks;
CREATE TRIGGER trg_awdp_breaks_run
    BEFORE INSERT OR UPDATE OF run_id ON public.awdp_breaks
    FOR EACH ROW EXECUTE FUNCTION public.assert_awdp_run();

-- ── 3.3 awdp_fix_rounds ──
ALTER TABLE public.awdp_fix_rounds ADD COLUMN IF NOT EXISTS run_id UUID NULL;

UPDATE public.awdp_fix_rounds fr
SET run_id = r.id
FROM public.awdp_runs r
WHERE r.event_id = fr.event_id
  AND fr.run_id IS NULL;

ALTER TABLE public.awdp_fix_rounds DROP CONSTRAINT IF EXISTS awdp_fix_rounds_event_id_fkey;
ALTER TABLE public.awdp_fix_rounds DROP CONSTRAINT IF EXISTS awdp_fix_rounds_unique;
DROP TRIGGER IF EXISTS trg_awdp_fix_rounds_family ON public.awdp_fix_rounds;
ALTER TABLE public.awdp_fix_rounds DROP COLUMN IF EXISTS event_id;

ALTER TABLE public.awdp_fix_rounds ALTER COLUMN run_id SET NOT NULL;
ALTER TABLE public.awdp_fix_rounds
    ADD CONSTRAINT awdp_fix_rounds_run_fk
        FOREIGN KEY (run_id) REFERENCES public.awdp_runs (id) ON DELETE CASCADE;
ALTER TABLE public.awdp_fix_rounds
    ADD CONSTRAINT awdp_fix_rounds_unique UNIQUE (run_id, sequence);

COMMENT ON COLUMN public.awdp_fix_rounds.run_id IS '归属 run（UNIQUE(run_id, sequence)）';

DROP TRIGGER IF EXISTS trg_awdp_fix_rounds_run ON public.awdp_fix_rounds;
CREATE TRIGGER trg_awdp_fix_rounds_run
    BEFORE INSERT OR UPDATE OF run_id ON public.awdp_fix_rounds
    FOR EACH ROW EXECUTE FUNCTION public.assert_awdp_run();

-- ── 3.4 awdp_patch_submissions ──
ALTER TABLE public.awdp_patch_submissions ADD COLUMN IF NOT EXISTS run_id UUID NULL;

UPDATE public.awdp_patch_submissions p
SET run_id = r.id
FROM public.awdp_runs r
WHERE r.event_id = p.event_id
  AND p.run_id IS NULL;

ALTER TABLE public.awdp_patch_submissions DROP CONSTRAINT IF EXISTS awdp_patch_submissions_event_id_fkey;
ALTER TABLE public.awdp_patch_submissions ALTER COLUMN event_id DROP NOT NULL;
DROP TRIGGER IF EXISTS trg_awdp_patch_submissions_family ON public.awdp_patch_submissions;

ALTER TABLE public.awdp_patch_submissions ALTER COLUMN run_id SET NOT NULL;
ALTER TABLE public.awdp_patch_submissions
    ADD CONSTRAINT awdp_patch_submissions_run_fk
        FOREIGN KEY (run_id) REFERENCES public.awdp_runs (id) ON DELETE CASCADE,
    ADD CONSTRAINT awdp_patch_submissions_team_event_check
        CHECK (event_id IS NOT NULL OR team_id IS NULL);

COMMENT ON COLUMN public.awdp_patch_submissions.run_id IS '归属 run';
COMMENT ON COLUMN public.awdp_patch_submissions.event_id IS '冗余：仅 team 复合 FK 需要；practice 行为 NULL';

DROP TRIGGER IF EXISTS trg_awdp_patch_submissions_run ON public.awdp_patch_submissions;
CREATE TRIGGER trg_awdp_patch_submissions_run
    BEFORE INSERT OR UPDATE OF run_id ON public.awdp_patch_submissions
    FOR EACH ROW EXECUTE FUNCTION public.assert_awdp_run();

-- ── 3.5 awdp_evaluations ──
ALTER TABLE public.awdp_evaluations ADD COLUMN IF NOT EXISTS run_id UUID NULL;

UPDATE public.awdp_evaluations ev
SET run_id = r.id
FROM public.awdp_runs r
WHERE r.event_id = ev.event_id
  AND ev.run_id IS NULL;

ALTER TABLE public.awdp_evaluations DROP CONSTRAINT IF EXISTS awdp_evaluations_event_id_fkey;
DROP TRIGGER IF EXISTS trg_awdp_evaluations_family ON public.awdp_evaluations;
ALTER TABLE public.awdp_evaluations DROP COLUMN IF EXISTS event_id;

ALTER TABLE public.awdp_evaluations ALTER COLUMN run_id SET NOT NULL;
ALTER TABLE public.awdp_evaluations
    ADD CONSTRAINT awdp_evaluations_run_fk
        FOREIGN KEY (run_id) REFERENCES public.awdp_runs (id) ON DELETE CASCADE;

COMMENT ON COLUMN public.awdp_evaluations.run_id IS '归属 run（official 每 round×instance 唯一不变）';

DROP TRIGGER IF EXISTS trg_awdp_evaluations_run ON public.awdp_evaluations;
CREATE TRIGGER trg_awdp_evaluations_run
    BEFORE INSERT OR UPDATE OF run_id ON public.awdp_evaluations
    FOR EACH ROW EXECUTE FUNCTION public.assert_awdp_run();

-- ── 3.6 awdp_score_events ──
ALTER TABLE public.awdp_score_events ADD COLUMN IF NOT EXISTS run_id UUID NULL;
ALTER TABLE public.awdp_score_events ADD COLUMN IF NOT EXISTS gamebox_id UUID NULL;

UPDATE public.awdp_score_events s
SET run_id = r.id,
    gamebox_id = eg.gamebox_id
FROM public.awdp_event_gameboxes eg
JOIN public.awdp_runs r ON r.event_id = eg.event_id
WHERE eg.id = s.event_gamebox_id
  AND s.run_id IS NULL;

ALTER TABLE public.awdp_score_events DROP CONSTRAINT IF EXISTS awdp_score_events_event_id_fkey;
ALTER TABLE public.awdp_score_events DROP CONSTRAINT IF EXISTS awdp_score_events_event_gamebox_id_fkey;
ALTER TABLE public.awdp_score_events ALTER COLUMN event_id DROP NOT NULL;
ALTER TABLE public.awdp_score_events DROP COLUMN IF EXISTS event_gamebox_id;
DROP INDEX IF EXISTS idx_awdp_score_events_subject;
DROP TRIGGER IF EXISTS trg_awdp_score_events_family ON public.awdp_score_events;

ALTER TABLE public.awdp_score_events ALTER COLUMN run_id SET NOT NULL;
ALTER TABLE public.awdp_score_events ALTER COLUMN gamebox_id SET NOT NULL;
ALTER TABLE public.awdp_score_events
    ADD CONSTRAINT awdp_score_events_run_fk
        FOREIGN KEY (run_id) REFERENCES public.awdp_runs (id) ON DELETE CASCADE,
    ADD CONSTRAINT awdp_score_events_gamebox_fk
        FOREIGN KEY (gamebox_id) REFERENCES public.gameboxes (id) ON DELETE RESTRICT,
    ADD CONSTRAINT awdp_score_events_team_event_check
        CHECK (event_id IS NOT NULL OR team_id IS NULL);

CREATE INDEX IF NOT EXISTS idx_awdp_score_events_subject
    ON public.awdp_score_events (run_id, user_id, team_id);

COMMENT ON COLUMN public.awdp_score_events.run_id IS '归属 run';
COMMENT ON COLUMN public.awdp_score_events.gamebox_id IS 'GameBox 身份（原 event_gamebox_id → 直引 gameboxes）';
COMMENT ON COLUMN public.awdp_score_events.event_id IS '冗余：仅 team 复合 FK 需要；practice 行为 NULL';
COMMENT ON COLUMN public.awdp_score_events.idempotency_key IS
    'awdp:break:{run}:{gamebox}:{subject} / awdp:fix:{run}:{round}:{instance}';

DROP TRIGGER IF EXISTS trg_awdp_score_events_run ON public.awdp_score_events;
CREATE TRIGGER trg_awdp_score_events_run
    BEFORE INSERT OR UPDATE OF run_id ON public.awdp_score_events
    FOR EACH ROW EXECUTE FUNCTION public.assert_awdp_run();

-- ---------------------------------------------------------------------------
-- 4. awdp_events 降级为纯配置：删除运行态列（已迁移至 awdp_runs）
-- ---------------------------------------------------------------------------
ALTER TABLE public.awdp_events DROP CONSTRAINT IF EXISTS awdp_events_current_round_nonneg_check;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS phase;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS started_at;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS break_ends_at;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS fix_started_at;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS fix_ends_at;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS finished_at;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS current_round;
ALTER TABLE public.awdp_events DROP COLUMN IF EXISTS next_action_at;

COMMENT ON TABLE public.awdp_events IS
    'AWDP 赛事纯配置（1:1 events）：duration/score/configuration_generation；运行态全部在 awdp_runs';
