-- Team membership invariants.
--
-- 1) 同一用户在同一赛事最多属于一个队伍；
-- 2) membership 的 team 必须属于同一 event；
-- 3) membership 必须对应 event_users 报名行；
-- 4) 每支队伍最多一个 captain。
--
-- 旧数据若违反任一规则则直接终止迁移，不做静默删除/合并，避免误改赛事历史。

DO $$
DECLARE
    n bigint;
BEGIN
    SELECT count(*) INTO n
    FROM (
        SELECT event_id, user_id
        FROM public.event_team_members
        GROUP BY event_id, user_id
        HAVING count(*) > 1
    ) AS duplicated_members;
    IF n > 0 THEN
        RAISE EXCEPTION
            'team-membership migration aborted: % users belong to multiple teams in the same event',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.event_team_members etm
    JOIN public.event_teams et ON et.id = etm.team_id
    WHERE et.event_id IS DISTINCT FROM etm.event_id;
    IF n > 0 THEN
        RAISE EXCEPTION
            'team-membership migration aborted: % memberships reference a team from another event',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.event_team_members etm
    LEFT JOIN public.event_users eu
      ON eu.event_id = etm.event_id
     AND eu.user_id = etm.user_id
    WHERE eu.user_id IS NULL;
    IF n > 0 THEN
        RAISE EXCEPTION
            'team-membership migration aborted: % memberships have no matching event_users row',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM (
        SELECT event_id, team_id
        FROM public.event_team_members
        WHERE role = 'captain'::public.event_team_member_role
        GROUP BY event_id, team_id
        HAVING count(*) > 1
    ) AS duplicated_captains;
    IF n > 0 THEN
        RAISE EXCEPTION
            'team-membership migration aborted: % teams have multiple captains',
            n;
    END IF;
END $$;

-- 一个用户在一个赛事中只能属于一个队伍。数据库约束兜底并发 join/create。
ALTER TABLE public.event_team_members
    DROP CONSTRAINT IF EXISTS event_team_members_event_user_key;

ALTER TABLE public.event_team_members
    ADD CONSTRAINT event_team_members_event_user_key UNIQUE (event_id, user_id);

COMMENT ON CONSTRAINT event_team_members_event_user_key ON public.event_team_members IS
    '同一用户在同一赛事最多属于一个队伍';

-- membership 的 team 必须属于相同 event；替换旧的 team_id-only FK。
ALTER TABLE public.event_team_members
    DROP CONSTRAINT IF EXISTS event_team_members_team_id_fkey;

ALTER TABLE public.event_team_members
    DROP CONSTRAINT IF EXISTS event_team_members_event_team_fkey;

ALTER TABLE public.event_team_members
    ADD CONSTRAINT event_team_members_event_team_fkey
    FOREIGN KEY (event_id, team_id)
    REFERENCES public.event_teams (event_id, id)
    ON DELETE CASCADE;

COMMENT ON CONSTRAINT event_team_members_event_team_fkey ON public.event_team_members IS
    '队伍成员关系只能引用同一赛事下的队伍';

-- Team-mode 报名状态与成员关系保持一致：删除 event_users 时自动移除 membership。
ALTER TABLE public.event_team_members
    DROP CONSTRAINT IF EXISTS event_team_members_event_user_fkey;

ALTER TABLE public.event_team_members
    ADD CONSTRAINT event_team_members_event_user_fkey
    FOREIGN KEY (event_id, user_id)
    REFERENCES public.event_users (event_id, user_id)
    ON DELETE CASCADE;

COMMENT ON CONSTRAINT event_team_members_event_user_fkey ON public.event_team_members IS
    '队伍成员必须同时存在对应 event_users 报名行';

-- 一队只允许一个 captain；member 数量不限。
DROP INDEX IF EXISTS public.event_team_members_one_captain_uidx;

CREATE UNIQUE INDEX event_team_members_one_captain_uidx
    ON public.event_team_members (event_id, team_id)
    WHERE role = 'captain'::public.event_team_member_role;

COMMENT ON INDEX public.event_team_members_one_captain_uidx IS
    '每支赛事队伍最多一个 captain';
