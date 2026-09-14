-- Migration: 20260811112131-event-domain-orthogonalization
-- Event Domain 正交化：EventType → family × purpose × participant_mode
-- 合并 instances/event_instances → challenge_instances
-- 合并 challenge_solves/event_challenge_solves → jeopardy_challenge_solves
-- event_challenges → jeopardy_event_challenges
-- AWD 子表 FK 收紧到 awd_events；删除 legacy event_type

-- =====================================================================
-- 1. 新枚举
-- =====================================================================

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'event_family') THEN
        CREATE TYPE public.event_family AS ENUM ('jeopardy', 'awd');
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'event_purpose') THEN
        CREATE TYPE public.event_purpose AS ENUM ('practice', 'competition');
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'participant_mode') THEN
        CREATE TYPE public.participant_mode AS ENUM ('individual', 'team');
    END IF;
END $$;

COMMENT ON TYPE public.event_family IS '赛事引擎家族：jeopardy / awd';
COMMENT ON TYPE public.event_purpose IS '赛事用途：practice 长期练习 / competition 正式比赛';
COMMENT ON TYPE public.participant_mode IS '参与主体：individual 个人 / team 团队';

-- =====================================================================
-- 2. events：新增三维字段 + system_key，end_time 可空
-- =====================================================================

ALTER TABLE public.events
    ADD COLUMN IF NOT EXISTS family public.event_family,
    ADD COLUMN IF NOT EXISTS purpose public.event_purpose,
    ADD COLUMN IF NOT EXISTS participant_mode public.participant_mode,
    ADD COLUMN IF NOT EXISTS system_key text;

-- end_time 改为可空（Practice 无结束时间）
ALTER TABLE public.events
    ALTER COLUMN end_time DROP NOT NULL;

COMMENT ON COLUMN public.events.family IS '赛事引擎家族（创建后不可变）';
COMMENT ON COLUMN public.events.purpose IS '赛事用途 practice/competition（创建后不可变）';
COMMENT ON COLUMN public.events.participant_mode IS '参与主体 individual/team（创建后不可变）';
COMMENT ON COLUMN public.events.system_key IS '系统托管赛事唯一键（如 practice:jeopardy）；NULL=普通赛事';
COMMENT ON COLUMN public.events.end_time IS '结束时间；Practice 为 NULL，Competition 必填';

-- 2a. 优先：存在 awd_events 的行强制 AWD Competition Team（修复历史 type 错标）
UPDATE public.events e
SET
    family = 'awd'::public.event_family,
    purpose = 'competition'::public.event_purpose,
    participant_mode = 'team'::public.participant_mode
FROM public.awd_events a
WHERE a.event_id = e.id
  AND (e.family IS DISTINCT FROM 'awd'::public.event_family
       OR e.purpose IS DISTINCT FROM 'competition'::public.event_purpose
       OR e.participant_mode IS DISTINCT FROM 'team'::public.participant_mode);

-- 2b. 其余按旧 type 映射
UPDATE public.events
SET
    family = CASE type::text
        WHEN 'awd_team' THEN 'awd'::public.event_family
        ELSE 'jeopardy'::public.event_family
    END,
    purpose = CASE type::text
        WHEN 'jeopardy_practice' THEN 'practice'::public.event_purpose
        ELSE 'competition'::public.event_purpose
    END,
    participant_mode = CASE type::text
        WHEN 'jeopardy_practice' THEN 'individual'::public.participant_mode
        WHEN 'jeopardy_single' THEN 'individual'::public.participant_mode
        WHEN 'jeopardy_team' THEN 'team'::public.participant_mode
        WHEN 'awd_team' THEN 'team'::public.participant_mode
    END
WHERE family IS NULL
   OR purpose IS NULL
   OR participant_mode IS NULL;

-- 2c. Practice system event 规范化
UPDATE public.events
SET
    family = 'jeopardy'::public.event_family,
    purpose = 'practice'::public.event_purpose,
    participant_mode = 'individual'::public.participant_mode,
    system_key = 'practice:jeopardy',
    title = 'Practice',
    description = COALESCE(description, 'Practice Event'),
    hidden = true,
    allow_join = false,
    end_time = NULL,
    rules = COALESCE(NULLIF(rules, ''), 'do not cheat')
WHERE type::text = 'jeopardy_practice'
   OR id = '00000000-0000-0000-0000-000000000000'::uuid
   OR title IN ('PraticeEvent', 'PracticeEvent', 'Practice');

-- 若无任何 Practice 行，确保至少有一条 system Practice（幂等）
INSERT INTO public.events (
    id, family, purpose, participant_mode, system_key,
    title, description, hidden, start_time, end_time, rules, allow_join, flag_prefix,
    type
)
SELECT
    public.uuid_generate_v4(),
    'jeopardy'::public.event_family,
    'practice'::public.event_purpose,
    'individual'::public.participant_mode,
    'practice:jeopardy',
    'Practice',
    'Practice Event',
    true,
    now(),
    NULL,
    'do not cheat',
    false,
    NULL,
    'jeopardy_practice'::public.event_type
WHERE NOT EXISTS (
    SELECT 1 FROM public.events WHERE system_key = 'practice:jeopardy'
);

-- 校验 backfill 完整
DO $$
DECLARE
    null_cnt bigint;
    invalid_cnt bigint;
BEGIN
    SELECT count(*) INTO null_cnt
    FROM public.events
    WHERE family IS NULL OR purpose IS NULL OR participant_mode IS NULL;

    IF null_cnt > 0 THEN
        RAISE EXCEPTION 'event-domain migration: % events still missing mode fields', null_cnt;
    END IF;

    SELECT count(*) INTO invalid_cnt
    FROM public.events e
    WHERE NOT (
        (e.family = 'jeopardy' AND e.purpose = 'practice' AND e.participant_mode = 'individual')
        OR (e.family = 'jeopardy' AND e.purpose = 'competition' AND e.participant_mode IN ('individual', 'team'))
        OR (e.family = 'awd' AND e.purpose = 'competition' AND e.participant_mode = 'team')
    );

    IF invalid_cnt > 0 THEN
        RAISE EXCEPTION 'event-domain migration: % events have invalid mode combination', invalid_cnt;
    END IF;
END $$;

ALTER TABLE public.events
    ALTER COLUMN family SET NOT NULL,
    ALTER COLUMN purpose SET NOT NULL,
    ALTER COLUMN participant_mode SET NOT NULL;

-- 合法组合 CHECK
ALTER TABLE public.events DROP CONSTRAINT IF EXISTS events_mode_combination_check;
ALTER TABLE public.events
    ADD CONSTRAINT events_mode_combination_check CHECK (
        (family = 'jeopardy' AND purpose = 'practice' AND participant_mode = 'individual')
        OR (family = 'jeopardy' AND purpose = 'competition' AND participant_mode IN ('individual', 'team'))
        OR (family = 'awd' AND purpose = 'competition' AND participant_mode = 'team')
    );

-- Practice end_time NULL / Competition end_time NOT NULL + 时间序
ALTER TABLE public.events DROP CONSTRAINT IF EXISTS events_end_time_by_purpose_check;
ALTER TABLE public.events
    ADD CONSTRAINT events_end_time_by_purpose_check CHECK (
        (purpose = 'practice' AND end_time IS NULL)
        OR (purpose = 'competition' AND end_time IS NOT NULL AND start_time < end_time)
    );

-- system_key 部分唯一
CREATE UNIQUE INDEX IF NOT EXISTS events_system_key_uidx
    ON public.events (system_key)
    WHERE system_key IS NOT NULL;

-- =====================================================================
-- 3. event_logs：三维快照替换 type
-- =====================================================================

ALTER TABLE public.event_logs
    ADD COLUMN IF NOT EXISTS family public.event_family,
    ADD COLUMN IF NOT EXISTS purpose public.event_purpose,
    ADD COLUMN IF NOT EXISTS participant_mode public.participant_mode;

-- 优先从关联 event 回填
UPDATE public.event_logs l
SET
    family = e.family,
    purpose = e.purpose,
    participant_mode = e.participant_mode
FROM public.events e
WHERE e.id = l.event_id
  AND (l.family IS NULL OR l.purpose IS NULL OR l.participant_mode IS NULL);

-- 无关联时按旧 type 映射
UPDATE public.event_logs
SET
    family = CASE type::text
        WHEN 'awd_team' THEN 'awd'::public.event_family
        ELSE 'jeopardy'::public.event_family
    END,
    purpose = CASE type::text
        WHEN 'jeopardy_practice' THEN 'practice'::public.event_purpose
        ELSE 'competition'::public.event_purpose
    END,
    participant_mode = CASE type::text
        WHEN 'jeopardy_practice' THEN 'individual'::public.participant_mode
        WHEN 'jeopardy_single' THEN 'individual'::public.participant_mode
        WHEN 'jeopardy_team' THEN 'team'::public.participant_mode
        WHEN 'awd_team' THEN 'team'::public.participant_mode
    END
WHERE family IS NULL OR purpose IS NULL OR participant_mode IS NULL;

DO $$
DECLARE cnt bigint;
BEGIN
    SELECT count(*) INTO cnt FROM public.event_logs
    WHERE family IS NULL OR purpose IS NULL OR participant_mode IS NULL;
    IF cnt > 0 THEN
        RAISE EXCEPTION 'event-domain migration: % event_logs missing mode snapshot', cnt;
    END IF;
END $$;

ALTER TABLE public.event_logs
    ALTER COLUMN family SET NOT NULL,
    ALTER COLUMN purpose SET NOT NULL,
    ALTER COLUMN participant_mode SET NOT NULL;

COMMENT ON COLUMN public.event_logs.family IS '日志创建时赛事 family 快照';
COMMENT ON COLUMN public.event_logs.purpose IS '日志创建时赛事 purpose 快照';
COMMENT ON COLUMN public.event_logs.participant_mode IS '日志创建时赛事 participant_mode 快照';

ALTER TABLE public.event_logs DROP COLUMN IF EXISTS type;

-- =====================================================================
-- 4. events 删除旧 type + DROP event_type enum
-- =====================================================================

ALTER TABLE public.events DROP COLUMN IF EXISTS type;

DROP TYPE IF EXISTS public.event_type;

-- =====================================================================
-- 5. identity 不可变 trigger
-- =====================================================================

CREATE OR REPLACE FUNCTION public.events_identity_immutable()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.family IS DISTINCT FROM NEW.family
       OR OLD.purpose IS DISTINCT FROM NEW.purpose
       OR OLD.participant_mode IS DISTINCT FROM NEW.participant_mode THEN
        RAISE EXCEPTION 'events identity fields (family/purpose/participant_mode) are immutable';
    END IF;
    IF OLD.system_key IS NOT NULL AND OLD.system_key IS DISTINCT FROM NEW.system_key THEN
        RAISE EXCEPTION 'events.system_key is immutable once set';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_events_identity_immutable ON public.events;
CREATE TRIGGER trg_events_identity_immutable
    BEFORE UPDATE ON public.events
    FOR EACH ROW
    EXECUTE FUNCTION public.events_identity_immutable();

COMMENT ON FUNCTION public.events_identity_immutable() IS '阻止修改 events 的 family/purpose/participant_mode/system_key 身份字段';

-- =====================================================================
-- 6. family guard 通用函数
-- =====================================================================

CREATE OR REPLACE FUNCTION public.assert_event_family()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    expected public.event_family := TG_ARGV[0]::public.event_family;
    actual public.event_family;
    eid uuid;
BEGIN
    eid := NEW.event_id;
    IF eid IS NULL THEN
        RETURN NEW;
    END IF;
    SELECT e.family INTO actual FROM public.events e WHERE e.id = eid;
    IF actual IS NULL THEN
        RAISE EXCEPTION 'assert_event_family: event % not found', eid;
    END IF;
    IF actual IS DISTINCT FROM expected THEN
        RAISE EXCEPTION 'assert_event_family: event % family=% expected=%', eid, actual, expected;
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION public.assert_event_family() IS '子表 INSERT/UPDATE 时校验 parent events.family 与 TG_ARGV[0] 一致';

-- awd_events 只能挂 family=awd
DROP TRIGGER IF EXISTS trg_awd_events_family ON public.awd_events;
CREATE TRIGGER trg_awd_events_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awd_events
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_event_family('awd');

-- =====================================================================
-- 7. event_challenges → jeopardy_event_challenges
-- =====================================================================

ALTER TABLE IF EXISTS public.event_challenges RENAME TO jeopardy_event_challenges;

-- 约束/索引随表 rename 自动改名（PG 会保留约束名）；补 family guard
DROP TRIGGER IF EXISTS trg_jeopardy_event_challenges_family ON public.jeopardy_event_challenges;
CREATE TRIGGER trg_jeopardy_event_challenges_family
    BEFORE INSERT OR UPDATE OF event_id ON public.jeopardy_event_challenges
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_event_family('jeopardy');

COMMENT ON TABLE public.jeopardy_event_challenges IS 'Jeopardy 赛事题目表：赛事包含的题目及其分值/可见性';

-- =====================================================================
-- 8. instances + event_instances → challenge_instances
-- =====================================================================

ALTER TABLE IF EXISTS public.instances RENAME TO challenge_instances;

ALTER TABLE public.challenge_instances
    ADD COLUMN IF NOT EXISTS event_id uuid,
    ADD COLUMN IF NOT EXISTS team_id uuid;

-- Formal：从 event_instances 回填
UPDATE public.challenge_instances ci
SET
    event_id = ei.event_id,
    team_id = ei.team_id
FROM public.event_instances ei
WHERE ei.instance_id = ci.id
  AND ci.event_id IS NULL;

-- Practice：ref=JeopardyPractice → practice system event
UPDATE public.challenge_instances ci
SET event_id = pe.id
FROM public.events pe
WHERE pe.system_key = 'practice:jeopardy'
  AND ci.event_id IS NULL
  AND ci.ref = 'JeopardyPractice';

-- 其它仍无 event_id 的行：若 ref 能映射到唯一 event 则失败提示；否则 abort
DO $$
DECLARE orphan_cnt bigint;
BEGIN
    SELECT count(*) INTO orphan_cnt
    FROM public.challenge_instances
    WHERE event_id IS NULL;
    IF orphan_cnt > 0 THEN
        RAISE EXCEPTION 'event-domain migration: % challenge_instances cannot resolve event_id (orphan)', orphan_cnt;
    END IF;
END $$;

ALTER TABLE public.challenge_instances
    ALTER COLUMN event_id SET NOT NULL;

-- challenge_id 业务上应 NOT NULL（旧 schema 可空）；无法归属则 abort
DO $$
DECLARE null_ch bigint;
BEGIN
    SELECT count(*) INTO null_ch FROM public.challenge_instances WHERE challenge_id IS NULL;
    IF null_ch > 0 THEN
        RAISE EXCEPTION 'event-domain migration: % challenge_instances missing challenge_id', null_ch;
    END IF;
END $$;

ALTER TABLE public.challenge_instances
    ALTER COLUMN challenge_id SET NOT NULL;

-- FK
ALTER TABLE public.challenge_instances DROP CONSTRAINT IF EXISTS challenge_instances_event_id_fkey;
ALTER TABLE public.challenge_instances
    ADD CONSTRAINT challenge_instances_event_id_fkey
    FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE public.challenge_instances DROP CONSTRAINT IF EXISTS challenge_instances_team_id_fkey;
ALTER TABLE public.challenge_instances
    ADD CONSTRAINT challenge_instances_team_id_fkey
    FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

-- 删除 ref + event_instances
ALTER TABLE public.challenge_instances DROP COLUMN IF EXISTS ref;
DROP TABLE IF EXISTS public.event_instances;

-- 索引
CREATE INDEX IF NOT EXISTS idx_challenge_instances_event_id ON public.challenge_instances (event_id);
CREATE INDEX IF NOT EXISTS idx_challenge_instances_user_id ON public.challenge_instances (user_id);
CREATE INDEX IF NOT EXISTS idx_challenge_instances_challenge_id ON public.challenge_instances (challenge_id);
CREATE INDEX IF NOT EXISTS idx_challenge_instances_status ON public.challenge_instances (status);
CREATE INDEX IF NOT EXISTS idx_challenge_instances_event_user ON public.challenge_instances (event_id, user_id);
CREATE INDEX IF NOT EXISTS idx_challenge_instances_event_team ON public.challenge_instances (event_id, team_id);

DROP TRIGGER IF EXISTS trg_challenge_instances_family ON public.challenge_instances;
CREATE TRIGGER trg_challenge_instances_family
    BEFORE INSERT OR UPDATE OF event_id ON public.challenge_instances
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_event_family('jeopardy');

COMMENT ON TABLE public.challenge_instances IS 'Jeopardy 题目实例：动态容器实例，归属具体 Event（含 Practice system event）';
COMMENT ON COLUMN public.challenge_instances.event_id IS '所属赛事 ID（Practice 为 system_key=practice:jeopardy）';
COMMENT ON COLUMN public.challenge_instances.team_id IS '队伍 ID（Team 模式）；Individual/Practice 为 NULL';
COMMENT ON COLUMN public.challenge_instances.challenge_id IS '关联题目 ID';
COMMENT ON COLUMN public.challenge_instances.user_id IS '创建/归属用户 ID';
COMMENT ON COLUMN public.challenge_instances.flag IS '实例内动态生成的 flag';
COMMENT ON COLUMN public.challenge_instances.content IS '实例内容/访问信息（可为空）';
COMMENT ON COLUMN public.challenge_instances.identifier IS '运行时标识（容器名/ID）';
COMMENT ON COLUMN public.challenge_instances.destroy_at IS '自动销毁时间';

-- =====================================================================
-- 9. challenge_solves + event_challenge_solves → jeopardy_challenge_solves
-- =====================================================================

CREATE TABLE IF NOT EXISTS public.jeopardy_challenge_solves (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    challenge_id uuid NOT NULL,
    user_id uuid NOT NULL,
    team_id uuid,
    obtained_points double precision DEFAULT 0 NOT NULL,
    bonus_points double precision DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT jeopardy_challenge_solves_pkey PRIMARY KEY (id),
    CONSTRAINT jeopardy_challenge_solves_event_id_fkey
        FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE,
    CONSTRAINT jeopardy_challenge_solves_challenge_id_fkey
        FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE,
    CONSTRAINT jeopardy_challenge_solves_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE,
    CONSTRAINT jeopardy_challenge_solves_team_id_fkey
        FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE
);

COMMENT ON TABLE public.jeopardy_challenge_solves IS 'Jeopardy 解题记录（Practice + Competition 统一）';
COMMENT ON COLUMN public.jeopardy_challenge_solves.id IS '主键';
COMMENT ON COLUMN public.jeopardy_challenge_solves.event_id IS '所属赛事 ID';
COMMENT ON COLUMN public.jeopardy_challenge_solves.challenge_id IS '题目 ID';
COMMENT ON COLUMN public.jeopardy_challenge_solves.user_id IS '实际提交 Flag 的用户';
COMMENT ON COLUMN public.jeopardy_challenge_solves.team_id IS 'Solve owner 队伍（Team 模式）；Individual/Practice 为 NULL';
COMMENT ON COLUMN public.jeopardy_challenge_solves.obtained_points IS '实际获得分值（Practice 为 0）';
COMMENT ON COLUMN public.jeopardy_challenge_solves.bonus_points IS '额外加分（如首破）';
COMMENT ON COLUMN public.jeopardy_challenge_solves.created_at IS '解题时间';

-- 从 event_challenge_solves 迁入
INSERT INTO public.jeopardy_challenge_solves (
    id, event_id, challenge_id, user_id, team_id, obtained_points, bonus_points, created_at
)
SELECT
    public.uuid_generate_v4(),
    ecs.event_id,
    ecs.challenge_id,
    ecs.user_id,
    ecs.team_id,
    ecs.obtained_points,
    ecs.bonus_points,
    ecs.created_at
FROM public.event_challenge_solves ecs
WHERE NOT EXISTS (
    SELECT 1 FROM public.jeopardy_challenge_solves j
    WHERE j.event_id = ecs.event_id
      AND j.challenge_id = ecs.challenge_id
      AND j.user_id = ecs.user_id
      AND j.team_id IS NOT DISTINCT FROM ecs.team_id
);

-- 从 challenge_solves 迁入（Practice；event_id NULL → practice system event）
INSERT INTO public.jeopardy_challenge_solves (
    id, event_id, challenge_id, user_id, team_id, obtained_points, bonus_points, created_at
)
SELECT
    COALESCE(cs.id, public.uuid_generate_v4()),
    COALESCE(cs.event_id, pe.id),
    cs.challenge_id,
    cs.user_id,
    NULL,
    0,
    0,
    cs.created_at
FROM public.challenge_solves cs
CROSS JOIN LATERAL (
    SELECT id FROM public.events WHERE system_key = 'practice:jeopardy' LIMIT 1
) pe
WHERE NOT EXISTS (
    SELECT 1 FROM public.jeopardy_challenge_solves j
    WHERE j.event_id = COALESCE(cs.event_id, pe.id)
      AND j.challenge_id = cs.challenge_id
      AND j.user_id = cs.user_id
      AND j.team_id IS NULL
);

-- 唯一性：Individual / Practice
CREATE UNIQUE INDEX IF NOT EXISTS jeopardy_challenge_solves_individual_uidx
    ON public.jeopardy_challenge_solves (event_id, challenge_id, user_id)
    WHERE team_id IS NULL;

-- 唯一性：Team
CREATE UNIQUE INDEX IF NOT EXISTS jeopardy_challenge_solves_team_uidx
    ON public.jeopardy_challenge_solves (event_id, challenge_id, team_id)
    WHERE team_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_jeopardy_challenge_solves_event
    ON public.jeopardy_challenge_solves (event_id);
CREATE INDEX IF NOT EXISTS idx_jeopardy_challenge_solves_challenge
    ON public.jeopardy_challenge_solves (challenge_id);
CREATE INDEX IF NOT EXISTS idx_jeopardy_challenge_solves_user
    ON public.jeopardy_challenge_solves (user_id);

DROP TRIGGER IF EXISTS trg_jeopardy_challenge_solves_family ON public.jeopardy_challenge_solves;
CREATE TRIGGER trg_jeopardy_challenge_solves_family
    BEFORE INSERT OR UPDATE OF event_id ON public.jeopardy_challenge_solves
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_event_family('jeopardy');

DROP TABLE IF EXISTS public.event_challenge_solves;
DROP TABLE IF EXISTS public.challenge_solves;

-- =====================================================================
-- 10. AWD 子表 FK → awd_events(event_id)
--     排除平台级 awd_network_allocations / awd_network_settings
-- =====================================================================

DO $$
DECLARE
    t text;
    tables text[] := ARRAY[
        'awd_event_networks',
        'awd_event_gameboxes',
        'awd_gamebox_instances',
        'awd_rounds',
        'awd_flag_issues',
        'awd_flag_submissions',
        'awd_score_events',
        'awd_judge_batches',
        'awd_judge_tasks',
        'awd_team_networks',
        'awd_wireguard_peers',
        'awd_team_bans',
        'awd_reset_records',
        'awd_runtime_resources',
        'awd_orphan_resources',
        'awd_precheck_runs',
        'awd_internal_token_rotations'
    ];
    conname text;
    orphan bigint;
BEGIN
    FOREACH t IN ARRAY tables LOOP
        -- 校验无孤儿 event_id（不在 awd_events）
        EXECUTE format(
            'SELECT count(*) FROM public.%I c
             WHERE c.event_id IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM public.awd_events a WHERE a.event_id = c.event_id)',
            t
        ) INTO orphan;
        IF orphan > 0 THEN
            RAISE EXCEPTION 'event-domain migration: %.event_id has % rows not in awd_events', t, orphan;
        END IF;

        -- 删除指向 events 的 FK
        FOR conname IN
            SELECT ci.conname
            FROM pg_constraint ci
            JOIN pg_class rel ON rel.oid = ci.conrelid
            JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
            JOIN pg_class ref ON ref.oid = ci.confrelid
            WHERE nsp.nspname = 'public'
              AND rel.relname = t
              AND ci.contype = 'f'
              AND ref.relname = 'events'
              AND pg_get_constraintdef(ci.oid) ILIKE '%(event_id)%'
        LOOP
            EXECUTE format('ALTER TABLE public.%I DROP CONSTRAINT %I', t, conname);
        END LOOP;

        -- 挂到 awd_events(event_id)；orphan 表 event_id 可空用 SET NULL
        IF t = 'awd_orphan_resources' THEN
            EXECUTE format(
                'ALTER TABLE public.%I
                 ADD CONSTRAINT %I_event_id_awd_fkey
                 FOREIGN KEY (event_id) REFERENCES public.awd_events(event_id) ON DELETE SET NULL',
                t, t
            );
        ELSE
            EXECUTE format(
                'ALTER TABLE public.%I
                 ADD CONSTRAINT %I_event_id_awd_fkey
                 FOREIGN KEY (event_id) REFERENCES public.awd_events(event_id) ON DELETE CASCADE',
                t, t
            );
        END IF;
    END LOOP;
END $$;

-- =====================================================================
-- 11. 最终校验
-- =====================================================================

DO $$
DECLARE
    c bigint;
BEGIN
    SELECT count(*) INTO c FROM public.events WHERE family IS NULL;
    IF c > 0 THEN RAISE EXCEPTION 'final check: events.family null'; END IF;

    SELECT count(*) INTO c FROM pg_type WHERE typname = 'event_type';
    IF c > 0 THEN RAISE EXCEPTION 'final check: event_type enum still exists'; END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='event_instances') THEN
        RAISE EXCEPTION 'final check: event_instances still exists';
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='challenge_solves') THEN
        RAISE EXCEPTION 'final check: challenge_solves still exists';
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='event_challenge_solves') THEN
        RAISE EXCEPTION 'final check: event_challenge_solves still exists';
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='event_challenges') THEN
        RAISE EXCEPTION 'final check: event_challenges still exists';
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='instances') THEN
        RAISE EXCEPTION 'final check: instances still exists';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='challenge_instances') THEN
        RAISE EXCEPTION 'final check: challenge_instances missing';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='jeopardy_challenge_solves') THEN
        RAISE EXCEPTION 'final check: jeopardy_challenge_solves missing';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='jeopardy_event_challenges') THEN
        RAISE EXCEPTION 'final check: jeopardy_event_challenges missing';
    END IF;
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema='public' AND table_name='challenge_instances' AND column_name='ref'
    ) THEN
        RAISE EXCEPTION 'final check: challenge_instances.ref still exists';
    END IF;
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema='public' AND table_name='events' AND column_name='type'
    ) THEN
        RAISE EXCEPTION 'final check: events.type still exists';
    END IF;

    SELECT count(*) INTO c FROM public.events WHERE system_key = 'practice:jeopardy';
    IF c <> 1 THEN
        RAISE EXCEPTION 'final check: expected exactly 1 practice:jeopardy event, got %', c;
    END IF;
END $$;
