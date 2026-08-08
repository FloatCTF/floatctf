-- ================================================================================
-- Migration: 20260808183914-awd-gamebox-domain-b
-- ================================================================================
-- GameBox 领域模型重构 — Migration B：数据回填。
--
-- 安全原则（§40-43）：
--   - 每条 legacy awd_gamebox_templates → 自己的 GameBox identity + Revision 1 + EventGameBox
--     （禁止按 name/safe_name 猜测合并，宁可保守产生重复 GameBox）
--   - host_offset 优先从已存在 instance IP 反推（host 字节），
--     同 template 在不同 Team 偏移不一致 → RAISE（MIGRATION CONFLICT，禁止静默选择）
--   - 未部署的 template → 按稳定排序分配可用 offset（2..254，跳过已占用）
--   - 每个旧 gameboxes 全局行 → GameBox + Revision 1（source_toml 保存，spec 最小正规化）
--   - legacy image 无 digest → image_digest = NULL（§8.1），生产前必须 resolve/pin
--
-- 本迁移在空库上零操作，但保持对任何环境的正确性与可验证性。
-- ================================================================================

BEGIN;

-- ──────────────────────────────────────────────────────────────────────────────
-- 1. 回填 legacy awd_gamebox_templates → GameBox + Revision 1 + EventGameBox
-- ──────────────────────────────────────────────────────────────────────────────

-- 临时映射表：legacy template_id → 新 gamebox_id / event_gamebox_id（后续回填 judge/score 用）
CREATE TEMP TABLE IF NOT EXISTS _gb_mapping (
    legacy_template_id UUID PRIMARY KEY,
    gamebox_id UUID NOT NULL,
    event_gamebox_id UUID NOT NULL
) ON COMMIT DROP;

DO $$
DECLARE
    t RECORD;
    gb_id UUID;
    rev_id UUID;
    eg_id UUID;
    safe_base TEXT;
    safe_name_val TEXT;
    offset_candidates INT[];
    first_offset INT;
    used_offsets INT[];
    taken BOOLEAN;
    cand INT;
    spec_json JSONB;
    digest TEXT;
    legacy_count INT;
BEGIN
    -- 逐条处理 legacy template（稳定排序：created_at → id，保证确定性）
    FOR t IN
        SELECT * FROM awd_gamebox_templates
        ORDER BY created_at ASC, id ASC
    LOOP
        -- a. 创建 GameBox identity（不与其他 gamebox 同名；安全名从 name 生成 + 冲突去重）
        safe_base := lower(regexp_replace(t.name, '[^a-zA-Z0-9_-]+', '-', 'g'));
        safe_base := regexp_replace(btrim(safe_base, '-'), '-+', '-', 'g');
        IF safe_base = '' THEN safe_base := 'gamebox'; END IF;
        safe_name_val := safe_base;
        WHILE EXISTS (SELECT 1 FROM gameboxes WHERE safe_name = safe_name_val) LOOP
            safe_name_val := safe_base || '-' || substr(md5(t.id::text), 1, 6);
        END LOOP;

        gb_id := gen_random_uuid();
        INSERT INTO gameboxes (id, name, safe_name, category, description, hidden, created_at, updated_at)
        VALUES (gb_id, t.name, safe_name_val, 'other', '从 legacy AWD template 迁移（GameBox 领域重构 Migration B）', TRUE, now(), now());

        -- b. 创建 Revision 1：spec_json = 正规化配置（image/username/资源/judge），digest = sha256(canonical)
        spec_json := jsonb_build_object(
            'image_ref', t.image_ref,
            'username', t.username,
            'cpu_millis', t.cpu_millis,
            'memory_bytes', t.memory_bytes,
            'pids_limit', t.pids_limit,
            'meta_json', COALESCE(t.meta_json, '{}'::jsonb),
            'healthcheck', t.healthcheck_override_json,
            'judge_script_name', t.judge_script_name,
            'judge_script_content', t.judge_script_content,
            'judge_args', t.judge_args_json
        );
        digest := encode(sha256(spec_json::text::bytea), 'hex');
        rev_id := gen_random_uuid();
        INSERT INTO gamebox_revisions (
            id, gamebox_id, revision_number, source_toml, spec_schema_version, spec_json, spec_digest,
            image_ref, image_digest, username, default_cpu_millis, default_memory_bytes, default_pids_limit,
            healthcheck_json, judge_script_name, judge_script_content, judge_args_json,
            default_judge_timeout_secs, default_judge_retry_interval_secs, created_at
        ) VALUES (
            rev_id, gb_id, 1, '', 1, spec_json, digest,
            t.image_ref, NULL, t.username, t.cpu_millis, t.memory_bytes, t.pids_limit,
            t.healthcheck_override_json, t.judge_script_name, t.judge_script_content, t.judge_args_json,
            t.judge_timeout_secs, t.judge_retry_interval_secs, now()
        );

        -- c. host_offset：优先从已存在 instance IP 反推（host 字节）
        offset_candidates := ARRAY(
            SELECT DISTINCT (host(split_part(inst.gamebox_ip, '/', 1)::inet))::int
            FROM awd_gamebox_instances inst
            WHERE inst.template_id = t.id
        );
        IF cardinality(offset_candidates) > 1 THEN
            RAISE EXCEPTION 'MIGRATION CONFLICT: legacy template % has inconsistent host offsets across teams: %（禁止静默选择，需人工处理）', t.id, offset_candidates;
        ELSIF cardinality(offset_candidates) = 1 THEN
            first_offset := offset_candidates[1];
            IF first_offset < 2 OR first_offset > 254 THEN
                RAISE EXCEPTION 'MIGRATION CONFLICT: legacy template % host offset % out of range 2..254', t.id, first_offset;
            END IF;
        ELSE
            -- 未部署 → 分配该事件下一个可用 offset（2..254，跳过已占用）
            used_offsets := ARRAY(
                SELECT eg.host_offset FROM awd_event_gameboxes eg
                WHERE eg.event_id = t.event_id
            );
            first_offset := NULL;
            FOR cand IN 2..254 LOOP
                taken := cand = ANY(used_offsets);
                IF NOT taken THEN
                    first_offset := cand;
                    EXIT;
                END IF;
            END LOOP;
            IF first_offset IS NULL THEN
                RAISE EXCEPTION 'MIGRATION CONFLICT: no free host_offset (2..254) for legacy template %', t.id;
            END IF;
        END IF;

        -- d. 创建 EventGameBox（计分配置从 legacy template 迁移）
        eg_id := gen_random_uuid();
        INSERT INTO awd_event_gameboxes (
            id, event_id, gamebox_id, gamebox_revision_id, host_offset,
            enabled, hidden, cpu_millis, memory_bytes, pids_limit, healthcheck_override_json,
            judge_timeout_secs, judge_retry_interval_secs,
            break_points, loss_points, fix_points, down_points, first_bonus,
            created_at, updated_at
        ) VALUES (
            eg_id, t.event_id, gb_id, rev_id, first_offset,
            TRUE, FALSE, t.cpu_millis, t.memory_bytes, t.pids_limit, t.healthcheck_override_json,
            t.judge_timeout_secs, t.judge_retry_interval_secs,
            t.break_points, t.loss_points, t.fix_points, t.down_points, t.first_bonus,
            now(), now()
        );

        -- e. 关联 legacy instance → 新 EventGameBox
        UPDATE awd_gamebox_instances
        SET event_gamebox_id = eg_id
        WHERE template_id = t.id;

        -- f. 记录映射（judge/score 回填用）
        INSERT INTO _gb_mapping (legacy_template_id, gamebox_id, event_gamebox_id)
        VALUES (t.id, gb_id, eg_id);
    END LOOP;

    SELECT count(*) INTO legacy_count FROM awd_gamebox_templates;
    RAISE NOTICE 'GameBox backfill: % legacy templates migrated (empty in dev)', legacy_count;
END $$;

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. 回填 legacy gameboxes（全局库）→ Revision 1
--    （§41：source_toml 保存原始 TOML；旧计分字段不进入 Revision，不凭空制造赛事计分配置）
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
DECLARE
    g RECORD;
    spec_json JSONB;
    digest TEXT;
BEGIN
    FOR g IN SELECT * FROM gameboxes ORDER BY created_at ASC, id ASC LOOP
        -- 已有 revision 的行跳过（幂等）
        IF EXISTS (SELECT 1 FROM gamebox_revisions WHERE gamebox_id = g.id) THEN
            CONTINUE;
        END IF;
        spec_json := jsonb_build_object(
            'legacy', TRUE,
            'source_toml', COALESCE(g.toml_str, ''),
            'username', g.username
        );
        digest := encode(sha256(spec_json::text::bytea), 'hex');
        INSERT INTO gamebox_revisions (
            id, gamebox_id, revision_number, source_toml, spec_schema_version, spec_json, spec_digest,
            image_ref, image_digest, username, default_cpu_millis, default_memory_bytes, default_pids_limit,
            healthcheck_json, judge_script_name, judge_script_content, judge_args_json,
            default_judge_timeout_secs, default_judge_retry_interval_secs, created_at
        ) VALUES (
            gen_random_uuid(), g.id, 1, COALESCE(g.toml_str, ''), 1, spec_json, digest,
            '', NULL, COALESCE(g.username, 'ctf'), 1000, 536870912, 100,
            NULL, NULL, NULL, NULL, NULL, NULL, now()
        );
    END LOOP;
END $$;

-- ──────────────────────────────────────────────────────────────────────────────
-- 3. 回填 awd_judge_tasks.event_gamebox_id（经 instance 关联）
-- ──────────────────────────────────────────────────────────────────────────────
UPDATE awd_judge_tasks jt
SET event_gamebox_id = inst.event_gamebox_id
FROM awd_gamebox_instances inst
WHERE jt.gamebox_instance_id = inst.id
  AND jt.event_gamebox_id IS NULL;

-- ──────────────────────────────────────────────────────────────────────────────
-- 4. 回填 awd_score_events.event_gamebox_id
--    instance 维度：经 awd_gamebox_instances 关联；
--    template 维度（如 first-bonus，无 instance）：经 legacy 映射表关联。
-- ──────────────────────────────────────────────────────────────────────────────
UPDATE awd_score_events se
SET event_gamebox_id = inst.event_gamebox_id
FROM awd_gamebox_instances inst
WHERE se.gamebox_instance_id = inst.id
  AND se.event_gamebox_id IS NULL;

UPDATE awd_score_events se
SET event_gamebox_id = m.event_gamebox_id
FROM _gb_mapping m
WHERE se.gamebox_template_id = m.legacy_template_id
  AND se.event_gamebox_id IS NULL;

-- ──────────────────────────────────────────────────────────────────────────────
-- 5. 可验证性断言（§61）——任何一条不满足即回滚整个迁移
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
DECLARE
    template_cnt INT; eg_cnt INT; orphan_inst INT; orphan_link INT; dup_offset INT; dup_inst INT;
BEGIN
    SELECT count(*) INTO template_cnt FROM awd_gamebox_templates;
    SELECT count(*) INTO eg_cnt FROM awd_event_gameboxes;
    IF template_cnt <> eg_cnt THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: legacy templates % <> event_gameboxes %', template_cnt, eg_cnt;
    END IF;

    -- 每个 legacy instance 都有 event_gamebox_id
    SELECT count(*) INTO orphan_inst FROM awd_gamebox_instances WHERE event_gamebox_id IS NULL;
    IF orphan_inst > 0 THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: % instances lack event_gamebox_id', orphan_inst;
    END IF;

    -- 每个 EventGameBox 都有 GameBoxRevision（FK 已保证）且无孤儿 revision link（复合 FK 已保证）

    -- 同 Event host_offset 无重复（UNIQUE 已保证，双保险）
    SELECT count(*) INTO dup_offset FROM (
        SELECT event_id, host_offset FROM awd_event_gameboxes GROUP BY 1,2 HAVING count(*) > 1
    ) d;
    IF dup_offset > 0 THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: duplicate host_offset';
    END IF;

    -- 同 Event + Team + EventGameBox 只有一个 logical Instance
    SELECT count(*) INTO dup_inst FROM (
        SELECT event_id, team_id, event_gamebox_id FROM awd_gamebox_instances
        GROUP BY 1,2,3 HAVING count(*) > 1
    ) d;
    IF dup_inst > 0 THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: duplicate logical instances';
    END IF;
END $$;

COMMIT;
