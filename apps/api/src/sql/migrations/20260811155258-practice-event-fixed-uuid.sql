-- Practice event 使用与 scheduled_tasks 相同风格的固定 UUID：
--   00000000-0000-0000-0000-000000000001  (from_u128(1))
-- 语义查找仍以 system_key = practice:jeopardy 为准；本迁移把已有 Practice 行
-- 的主键（及子表 event_id）规范到该固定 id。

-- 文件内禁止 BEGIN/COMMIT（由 migrate.sh 事务托管）

DO $migration$
DECLARE
    fixed_id uuid := '00000000-0000-0000-0000-000000000001';
    old_id uuid;
BEGIN
    SELECT e.id INTO old_id
    FROM events e
    WHERE e.system_key = 'practice:jeopardy';

    -- 已是固定 id：无事可做
    IF old_id IS NOT NULL AND old_id = fixed_id THEN
        RAISE NOTICE 'practice:jeopardy already at fixed id %', fixed_id;
        RETURN;
    END IF;

    -- 固定 id 被其它非 Practice 赛事占用：硬失败，避免静默破坏
    IF EXISTS (
        SELECT 1
        FROM events e
        WHERE e.id = fixed_id
          AND (e.system_key IS DISTINCT FROM 'practice:jeopardy')
    ) THEN
        RAISE EXCEPTION
            'cannot assign practice fixed uuid %: already used by another event',
            fixed_id;
    END IF;

    -- 尚无 Practice 行：直接插入固定 id（与 ensure_practice_jeopardy_event 一致）
    IF old_id IS NULL THEN
        IF NOT EXISTS (SELECT 1 FROM events WHERE id = fixed_id) THEN
            INSERT INTO events (
                id,
                family,
                purpose,
                participant_mode,
                system_key,
                title,
                description,
                hidden,
                allow_join,
                start_time,
                end_time,
                rules,
                flag_prefix,
                created_at,
                updated_at
            ) VALUES (
                fixed_id,
                'jeopardy',
                'practice',
                'individual',
                'practice:jeopardy',
                'Practice',
                'Practice Event',
                true,
                false,
                now(),
                NULL,
                'do not cheat',
                NULL,
                now(),
                now()
            );
            RAISE NOTICE 'inserted practice:jeopardy at fixed id %', fixed_id;
        END IF;
        RETURN;
    END IF;

    -- Practice 存在但 id 不是固定值：插入克隆 → 搬迁子表 → 删除旧行
    -- identity 触发器会阻止 UPDATE system_key，故临时关闭
    ALTER TABLE events DISABLE TRIGGER trg_events_identity_immutable;

    -- 释放 system_key 唯一约束占用，便于新行带上同一 key
    UPDATE events
    SET system_key = NULL,
        updated_at = now()
    WHERE id = old_id;

    INSERT INTO events (
        id,
        family,
        purpose,
        participant_mode,
        system_key,
        title,
        description,
        hidden,
        allow_join,
        start_time,
        end_time,
        rules,
        flag_prefix,
        created_at,
        updated_at
    )
    SELECT
        fixed_id,
        family,
        purpose,
        participant_mode,
        'practice:jeopardy',
        title,
        description,
        hidden,
        allow_join,
        start_time,
        end_time,
        rules,
        flag_prefix,
        created_at,
        now()
    FROM events
    WHERE id = old_id;

    -- 子表 event_id 搬迁（FK 无 ON UPDATE CASCADE，需显式 UPDATE）
    -- 先搬 event_teams，再搬引用 (event_id, team_id) 复合外键的表
    UPDATE event_teams SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE event_team_members SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE challenge_instances SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE jeopardy_challenge_solves SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE awd_events SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE awd_network_allocations SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE event_announcements SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE event_logs SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE event_users SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE event_writeup SET event_id = fixed_id WHERE event_id = old_id;
    UPDATE jeopardy_event_challenges SET event_id = fixed_id WHERE event_id = old_id;

    DELETE FROM events WHERE id = old_id;

    ALTER TABLE events ENABLE TRIGGER trg_events_identity_immutable;

    RAISE NOTICE 'remapped practice:jeopardy % → %', old_id, fixed_id;
END
$migration$;

COMMENT ON COLUMN events.system_key IS
    '系统托管键；practice:jeopardy 对应固定 id 00000000-0000-0000-0000-000000000001';
