-- Migration: 20260810235621-single-version-challenge-gamebox
-- 单版本化重构（撤销 revision 管线，回到 identity 直接承载包配置）：
--   challenges / gameboxes 重新持有全部 package 运行时字段（version/spec/镜像 pin/附件/资源/判题等）；
--   event_challenges / instances / awd_event_gameboxes 删除钉住的 revision_id（事件直接引用 identity）；
--   删除 challenge_revisions / gamebox_revisions 两张版本表。
-- 数据回填：每个 identity 取 latest ready revision（无 ready 取最新），折回 identity 列；无 revision 的旧行保持 NULL。

-- ---------------------------------------------------------------------------
-- 1. challenges 恢复包管线列（曾由 20260810145445 迁出到 challenge_revisions）
-- ---------------------------------------------------------------------------
ALTER TABLE public.challenges
    ADD COLUMN IF NOT EXISTS version TEXT,
    ADD COLUMN IF NOT EXISTS source_toml TEXT,
    ADD COLUMN IF NOT EXISTS spec_json JSONB,
    ADD COLUMN IF NOT EXISTS spec_digest TEXT,
    ADD COLUMN IF NOT EXISTS package_digest TEXT,
    ADD COLUMN IF NOT EXISTS flag_type TEXT,
    ADD COLUMN IF NOT EXISTS static_flag_value TEXT,
    ADD COLUMN IF NOT EXISTS container_port INTEGER,
    ADD COLUMN IF NOT EXISTS recommended_cpu_millis BIGINT NOT NULL DEFAULT 500,
    ADD COLUMN IF NOT EXISTS recommended_memory_bytes BIGINT NOT NULL DEFAULT 268435456,
    ADD COLUMN IF NOT EXISTS recommended_pids_limit BIGINT NOT NULL DEFAULT 100,
    ADD COLUMN IF NOT EXISTS attachment_path TEXT,
    ADD COLUMN IF NOT EXISTS attachment_name TEXT,
    ADD COLUMN IF NOT EXISTS attachment_size BIGINT,
    ADD COLUMN IF NOT EXISTS attachment_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS image_ref TEXT,
    ADD COLUMN IF NOT EXISTS image_id TEXT,
    ADD COLUMN IF NOT EXISTS image_repo_digest TEXT,
    ADD COLUMN IF NOT EXISTS build_status TEXT,
    ADD COLUMN IF NOT EXISTS build_error TEXT;

COMMENT ON COLUMN public.challenges.version IS '当前 package 的 SemVer（唯一存活版本；导入要求严格递增）';
COMMENT ON COLUMN public.challenges.source_toml IS '原始 meta.toml 文本（随导入更新）';
COMMENT ON COLUMN public.challenges.spec_json IS '规范化 manifest（canonical JSON，不含 static flag 明文）';
COMMENT ON COLUMN public.challenges.spec_digest IS '规范化 spec 的 SHA-256（hex）';
COMMENT ON COLUMN public.challenges.package_digest IS '逻辑 package 内容 SHA-256（hex：meta.toml+src/**+attachment/**，与 archive 外壳无关）';
COMMENT ON COLUMN public.challenges.flag_type IS 'dynamic | static（无 package 时为 NULL）';
COMMENT ON COLUMN public.challenges.static_flag_value IS 'static flag 明文（仅管理员可读；DTO/日志必须脱敏；dynamic 为 NULL）';
COMMENT ON COLUMN public.challenges.container_port IS '暴露的 TCP 端口（meta [docker].port；非 docker 题目为 NULL）';
COMMENT ON COLUMN public.challenges.recommended_cpu_millis IS '推荐 CPU（毫核）；加入赛事时可复制到 EventChallenge';
COMMENT ON COLUMN public.challenges.recommended_memory_bytes IS '推荐内存（字节）';
COMMENT ON COLUMN public.challenges.recommended_pids_limit IS '推荐 pids 限制';
COMMENT ON COLUMN public.challenges.attachment_path IS '附件相对路径（attachment/xxx，随当前 package 不可变）';
COMMENT ON COLUMN public.challenges.attachment_name IS '附件文件名（展示用）';
COMMENT ON COLUMN public.challenges.attachment_size IS '附件大小（字节）';
COMMENT ON COLUMN public.challenges.attachment_sha256 IS '附件内容 SHA-256（hex，完整性校验）';
COMMENT ON COLUMN public.challenges.image_ref IS '人类可读镜像 tag：<registry-prefix>/challenges/<safe_name>:<version>';
COMMENT ON COLUMN public.challenges.image_id IS '本地 Docker image ID（sha256:…），与 RepoDigest 严格区分';
COMMENT ON COLUMN public.challenges.image_repo_digest IS 'Registry canonical RepoDigest（repo@sha256:…）；Runtime 优先使用';
COMMENT ON COLUMN public.challenges.build_status IS 'building | ready | failed（无 package 时为 NULL）';
COMMENT ON COLUMN public.challenges.build_error IS '构建失败原因（bounded/sanitized）';

-- ---------------------------------------------------------------------------
-- 2. gameboxes 恢复包管线列（曾由 20260810140220 迁出到 gamebox_revisions）
-- ---------------------------------------------------------------------------
ALTER TABLE public.gameboxes
    ADD COLUMN IF NOT EXISTS version TEXT,
    ADD COLUMN IF NOT EXISTS source_toml TEXT,
    ADD COLUMN IF NOT EXISTS spec_json JSONB,
    ADD COLUMN IF NOT EXISTS spec_digest TEXT,
    ADD COLUMN IF NOT EXISTS package_digest TEXT,
    ADD COLUMN IF NOT EXISTS image_ref TEXT,
    ADD COLUMN IF NOT EXISTS image_id TEXT,
    ADD COLUMN IF NOT EXISTS image_repo_digest TEXT,
    ADD COLUMN IF NOT EXISTS username TEXT,
    ADD COLUMN IF NOT EXISTS recommended_cpu_millis BIGINT NOT NULL DEFAULT 1000,
    ADD COLUMN IF NOT EXISTS recommended_memory_bytes BIGINT NOT NULL DEFAULT 536870912,
    ADD COLUMN IF NOT EXISTS recommended_pids_limit BIGINT NOT NULL DEFAULT 100,
    ADD COLUMN IF NOT EXISTS healthchecks_json JSONB,
    ADD COLUMN IF NOT EXISTS judge_script_name TEXT,
    ADD COLUMN IF NOT EXISTS judge_script_content TEXT,
    ADD COLUMN IF NOT EXISTS judge_args_json JSONB,
    ADD COLUMN IF NOT EXISTS judge_timeout_secs INTEGER,
    ADD COLUMN IF NOT EXISTS judge_retry_interval_secs INTEGER,
    ADD COLUMN IF NOT EXISTS build_status TEXT,
    ADD COLUMN IF NOT EXISTS build_error TEXT;

COMMENT ON COLUMN public.gameboxes.version IS '当前 package 的 SemVer（唯一存活版本；导入要求严格递增）';
COMMENT ON COLUMN public.gameboxes.source_toml IS '原始 meta.toml 文本（随导入更新）';
COMMENT ON COLUMN public.gameboxes.spec_json IS '规范化 manifest（canonical JSON）';
COMMENT ON COLUMN public.gameboxes.spec_digest IS '规范化 spec 的 SHA-256（hex）';
COMMENT ON COLUMN public.gameboxes.package_digest IS '逻辑 package 内容 SHA-256（hex，与 archive 外壳无关）';
COMMENT ON COLUMN public.gameboxes.image_ref IS '人类可读镜像 tag：<registry-prefix>/gameboxes/<safe_name>:<version>';
COMMENT ON COLUMN public.gameboxes.image_id IS '本地 Docker image ID（sha256:…），与 RepoDigest 严格区分';
COMMENT ON COLUMN public.gameboxes.image_repo_digest IS 'Registry canonical RepoDigest（repo@sha256:…）；Runtime 优先使用';
COMMENT ON COLUMN public.gameboxes.username IS '容器内 SSH/业务用户名';
COMMENT ON COLUMN public.gameboxes.recommended_cpu_millis IS '推荐 CPU（毫核）；加入赛事时复制到 EventGameBox';
COMMENT ON COLUMN public.gameboxes.recommended_memory_bytes IS '推荐内存（字节）';
COMMENT ON COLUMN public.gameboxes.recommended_pids_limit IS '推荐 pids 限制';
COMMENT ON COLUMN public.gameboxes.healthchecks_json IS 'Readiness probes（HTTP/TCP 列表，canonical JSON）';
COMMENT ON COLUMN public.gameboxes.judge_script_name IS '判题脚本文件名（如 check.py）';
COMMENT ON COLUMN public.gameboxes.judge_script_content IS '判题脚本完整内容（package 自包含）';
COMMENT ON COLUMN public.gameboxes.judge_args_json IS '判题参数模板（JSON，可选）';
COMMENT ON COLUMN public.gameboxes.judge_timeout_secs IS '默认判题超时（秒，可选）';
COMMENT ON COLUMN public.gameboxes.judge_retry_interval_secs IS '默认判题重试间隔（秒，可选）';
COMMENT ON COLUMN public.gameboxes.build_status IS 'building | ready | failed（无 package 时为 NULL）';
COMMENT ON COLUMN public.gameboxes.build_error IS '构建失败原因（bounded/sanitized）';

-- ---------------------------------------------------------------------------
-- 3. 数据回填：每 identity 取 latest ready revision（无 ready 取最新），折回 identity
-- ---------------------------------------------------------------------------
UPDATE public.challenges AS c
SET version = r.version,
    source_toml = r.source_toml,
    spec_json = r.spec_json,
    spec_digest = r.spec_digest,
    package_digest = r.package_digest,
    flag_type = r.flag_type,
    static_flag_value = r.static_flag_value,
    container_port = r.container_port,
    recommended_cpu_millis = r.recommended_cpu_millis,
    recommended_memory_bytes = r.recommended_memory_bytes,
    recommended_pids_limit = r.recommended_pids_limit,
    attachment_path = r.attachment_path,
    attachment_name = r.attachment_name,
    attachment_size = r.attachment_size,
    attachment_sha256 = r.attachment_sha256,
    image_ref = r.image_ref,
    image_id = r.image_id,
    image_repo_digest = r.image_repo_digest,
    build_status = r.build_status,
    build_error = r.build_error
FROM (
    SELECT DISTINCT ON (challenge_id) *
    FROM public.challenge_revisions
    ORDER BY challenge_id,
        CASE WHEN build_status = 'ready' THEN 0 ELSE 1 END,
        created_at DESC
) AS r
WHERE r.challenge_id = c.id;

UPDATE public.gameboxes AS g
SET version = r.version,
    source_toml = r.source_toml,
    spec_json = r.spec_json,
    spec_digest = r.spec_digest,
    package_digest = r.package_digest,
    image_ref = r.image_ref,
    image_id = r.image_id,
    image_repo_digest = r.image_repo_digest,
    username = r.username,
    recommended_cpu_millis = r.recommended_cpu_millis,
    recommended_memory_bytes = r.recommended_memory_bytes,
    recommended_pids_limit = r.recommended_pids_limit,
    healthchecks_json = r.healthchecks_json,
    judge_script_name = r.judge_script_name,
    judge_script_content = r.judge_script_content,
    judge_args_json = r.judge_args_json,
    judge_timeout_secs = r.judge_timeout_secs,
    judge_retry_interval_secs = r.judge_retry_interval_secs,
    build_status = r.build_status,
    build_error = r.build_error
FROM (
    SELECT DISTINCT ON (gamebox_id) *
    FROM public.gamebox_revisions
    ORDER BY gamebox_id,
        CASE WHEN build_status = 'ready' THEN 0 ELSE 1 END,
        created_at DESC
) AS r
WHERE r.gamebox_id = g.id;

-- ---------------------------------------------------------------------------
-- 4. 删除事件/实例对 revision 的钉住
-- ---------------------------------------------------------------------------
DROP INDEX IF EXISTS event_challenges_event_revision_key;
DROP INDEX IF EXISTS event_challenges_revision_idx;
ALTER TABLE public.event_challenges
    DROP CONSTRAINT IF EXISTS event_challenges_revision_challenge_fkey;
ALTER TABLE public.event_challenges
    DROP COLUMN IF EXISTS challenge_revision_id;

DROP INDEX IF EXISTS instances_challenge_revision_idx;
ALTER TABLE public.instances
    DROP CONSTRAINT IF EXISTS instances_challenge_revision_id_fkey;
ALTER TABLE public.instances
    DROP COLUMN IF EXISTS challenge_revision_id;

DROP INDEX IF EXISTS awd_event_gameboxes_event_revision_key;
DROP INDEX IF EXISTS awd_event_gameboxes_revision_idx;
ALTER TABLE public.awd_event_gameboxes
    DROP CONSTRAINT IF EXISTS awd_event_gameboxes_revision_fkey;
ALTER TABLE public.awd_event_gameboxes
    DROP COLUMN IF EXISTS gamebox_revision_id;

-- ---------------------------------------------------------------------------
-- 5. 删除 revision 表
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS public.challenge_revisions;
DROP TABLE IF EXISTS public.gamebox_revisions;

-- ---------------------------------------------------------------------------
-- 6. identity 上的取值约束（与旧 revision 表一致；NULL 不参与检查）
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'challenges_flag_type_check'
    ) THEN
        ALTER TABLE public.challenges
            ADD CONSTRAINT challenges_flag_type_check
            CHECK (flag_type IN ('dynamic', 'static'));
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'challenges_build_status_check'
    ) THEN
        ALTER TABLE public.challenges
            ADD CONSTRAINT challenges_build_status_check
            CHECK (build_status IN ('building', 'ready', 'failed'));
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'gameboxes_build_status_check'
    ) THEN
        ALTER TABLE public.gameboxes
            ADD CONSTRAINT gameboxes_build_status_check
            CHECK (build_status IN ('building', 'ready', 'failed'));
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- 7. 表级注释更新
-- ---------------------------------------------------------------------------
COMMENT ON TABLE public.challenges IS 'Challenge 库：身份 + 唯一存活版本（单版本设计；导入要求 version 严格递增）';
COMMENT ON TABLE public.gameboxes IS 'AWD GameBox 库：身份 + 唯一存活版本（单版本设计；导入要求 version 严格递增）';
COMMENT ON TABLE public.event_challenges IS '赛事题目表：赛事包含的题目及其分值/可见性（直接引用 Challenge 当前版本）';
COMMENT ON TABLE public.awd_event_gameboxes IS 'AWD 赛事 GameBox 选择：赛事采用的 GameBox（直接引用 GameBox 当前版本）+ 该场自己的计分/资源/可见性配置';
