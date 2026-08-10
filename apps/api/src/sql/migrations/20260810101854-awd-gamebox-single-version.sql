-- ================================================================================
-- Migration: 20260810101854-awd-gamebox-single-version
-- Created at: 2026-08-10 10:18:54 +0800
-- 目的：GameBox 单版本化（用户要求：像 challenges 一样单版本，去掉 Revision 历史设计）。
--   - gameboxes 增加全部运行时配置列（原 gamebox_revisions 的字段）
--   - 每个 GameBox 取其最新 Revision（revision_number 最大）回填为单版本配置
--     （注意：曾 pin 旧 Revision 的赛事随之改用最新配置——单版本化的有意语义）
--   - awd_event_gameboxes 删除 gamebox_revision_id（DROP COLUMN 自动删复合 FK）
--   - DROP gamebox_revisions 表
-- ================================================================================


-- ── 1. gameboxes 增加配置列（幂等）───────────────────────────────────────────
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS source_toml TEXT;
COMMENT ON COLUMN gameboxes.source_toml IS 'GameBox 配置源 TOML（单版本，编辑直接覆盖，同 challenges.toml_str）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS image_ref TEXT;
COMMENT ON COLUMN gameboxes.image_ref IS '镜像引用（如 registry/easy-web:v1）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS image_digest TEXT;
COMMENT ON COLUMN gameboxes.image_digest IS '镜像 digest 钉住（生产建议 pin，格式 sha256:…）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS username TEXT;
COMMENT ON COLUMN gameboxes.username IS 'GameBox 内 SSH 用户名（默认 ctf）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS default_cpu_millis BIGINT;
COMMENT ON COLUMN gameboxes.default_cpu_millis IS '默认 CPU 限制（毫核）；赛事选择时复制为 awd_event_gameboxes.cpu_millis 可再覆盖';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS default_memory_bytes BIGINT;
COMMENT ON COLUMN gameboxes.default_memory_bytes IS '默认内存限制（字节）；赛事选择时可覆盖';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS default_pids_limit BIGINT;
COMMENT ON COLUMN gameboxes.default_pids_limit IS '默认 pids 限制；赛事选择时可覆盖';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS healthcheck_json JSONB;
COMMENT ON COLUMN gameboxes.healthcheck_json IS '默认健康检查配置（JSON）；赛事选择时可覆盖';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS judge_script_name TEXT;
COMMENT ON COLUMN gameboxes.judge_script_name IS '判题脚本名（可选）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS judge_script_content TEXT;
COMMENT ON COLUMN gameboxes.judge_script_content IS '判题脚本内容（可选）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS judge_args_json JSONB;
COMMENT ON COLUMN gameboxes.judge_args_json IS '判题脚本参数（JSON，可选）';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS default_judge_timeout_secs INTEGER;
COMMENT ON COLUMN gameboxes.default_judge_timeout_secs IS '默认判题超时秒数（可选）；赛事选择时可覆盖';
ALTER TABLE gameboxes ADD COLUMN IF NOT EXISTS default_judge_retry_interval_secs INTEGER;
COMMENT ON COLUMN gameboxes.default_judge_retry_interval_secs IS '默认判题重试间隔秒数（可选）；赛事选择时可覆盖';

-- ── 2. 回填：每个 GameBox 取最新 Revision（revision_number 最大，平局取创建最新）──
UPDATE gameboxes g
SET source_toml = r.source_toml,
    image_ref = r.image_ref,
    image_digest = r.image_digest,
    username = r.username,
    default_cpu_millis = r.default_cpu_millis,
    default_memory_bytes = r.default_memory_bytes,
    default_pids_limit = r.default_pids_limit,
    healthcheck_json = r.healthcheck_json,
    judge_script_name = r.judge_script_name,
    judge_script_content = r.judge_script_content,
    judge_args_json = r.judge_args_json,
    default_judge_timeout_secs = r.default_judge_timeout_secs,
    default_judge_retry_interval_secs = r.default_judge_retry_interval_secs
FROM (
    SELECT DISTINCT ON (gamebox_id)
        gamebox_id, source_toml, image_ref, image_digest, username,
        default_cpu_millis, default_memory_bytes, default_pids_limit, healthcheck_json,
        judge_script_name, judge_script_content, judge_args_json,
        default_judge_timeout_secs, default_judge_retry_interval_secs
    FROM gamebox_revisions
    ORDER BY gamebox_id, revision_number DESC, created_at DESC
) r
WHERE r.gamebox_id = g.id;

-- ── 3. 移除 EventGameBox 的 revision pin 列（DROP COLUMN 自动删除复合 FK
--      awd_event_gameboxes_revision_fk，因该约束引用 gamebox_revision_id）────────
ALTER TABLE awd_event_gameboxes DROP COLUMN IF EXISTS gamebox_revision_id;

-- ── 4. 删除 Revision 表（gamebox_revisions_gamebox_id_fkey ON DELETE CASCADE 随表删除）──
DROP TABLE IF EXISTS gamebox_revisions;

