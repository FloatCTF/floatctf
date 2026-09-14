-- Migration: 20260810140220-gamebox-revisions-package-pipeline
-- GameBox Package / Immutable Revision 管线：
--   gameboxes 降为长期身份表；
--   新增 gamebox_revisions（不可变版本 + 镜像 digest + judge/healthcheck）；
--   awd_event_gameboxes 钉住 revision，不再从 live gameboxes 读镜像。
-- 开发库无历史 GameBox 数据，采用 destructive clean schema（不迁移旧配置列）。

-- ---------------------------------------------------------------------------
-- 1. gamebox_revisions（先建表，后改 FK）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.gamebox_revisions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    gamebox_id UUID NOT NULL,
    version TEXT NOT NULL,
    revision_number INTEGER NOT NULL,
    source_toml TEXT NOT NULL,
    spec_json JSONB NOT NULL,
    spec_digest TEXT NOT NULL,
    package_digest TEXT NOT NULL,
    image_ref TEXT,
    image_id TEXT,
    image_repo_digest TEXT,
    username TEXT NOT NULL,
    recommended_cpu_millis BIGINT NOT NULL DEFAULT 1000,
    recommended_memory_bytes BIGINT NOT NULL DEFAULT 536870912,
    recommended_pids_limit BIGINT NOT NULL DEFAULT 100,
    healthchecks_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    judge_script_name TEXT,
    judge_script_content TEXT,
    judge_args_json JSONB,
    judge_timeout_secs INTEGER,
    judge_retry_interval_secs INTEGER,
    build_status TEXT NOT NULL DEFAULT 'building',
    build_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT gamebox_revisions_build_status_check
        CHECK (build_status IN ('building', 'ready', 'failed')),
    CONSTRAINT gamebox_revisions_revision_number_positive
        CHECK (revision_number >= 1)
);

COMMENT ON TABLE public.gamebox_revisions IS 'GameBox 不可变版本：package/spec digest、镜像 identity、judge 与 readiness healthchecks';
COMMENT ON COLUMN public.gamebox_revisions.id IS '主键';
COMMENT ON COLUMN public.gamebox_revisions.gamebox_id IS '所属 GameBox 身份';
COMMENT ON COLUMN public.gamebox_revisions.version IS '作者 package SemVer（用作镜像 tag 后缀，非全局唯一）';
COMMENT ON COLUMN public.gamebox_revisions.revision_number IS '平台内部递增序号（同 gamebox 内）';
COMMENT ON COLUMN public.gamebox_revisions.source_toml IS '原始 meta.toml 文本';
COMMENT ON COLUMN public.gamebox_revisions.spec_json IS '规范化 manifest（canonical JSON）';
COMMENT ON COLUMN public.gamebox_revisions.spec_digest IS '规范化 spec 的 SHA-256（hex）';
COMMENT ON COLUMN public.gamebox_revisions.package_digest IS '逻辑 package 内容 SHA-256（hex，与 archive 外壳无关）';
COMMENT ON COLUMN public.gamebox_revisions.image_ref IS '人类可读镜像 tag：<registry-prefix>/gameboxes/<safe_name>:<version>';
COMMENT ON COLUMN public.gamebox_revisions.image_id IS '本地 Docker image ID（sha256:…），与 RepoDigest 严格区分';
COMMENT ON COLUMN public.gamebox_revisions.image_repo_digest IS 'Registry canonical RepoDigest（repo@sha256:…）；Runtime 优先使用';
COMMENT ON COLUMN public.gamebox_revisions.username IS '容器内 SSH/业务用户名';
COMMENT ON COLUMN public.gamebox_revisions.recommended_cpu_millis IS '推荐 CPU（毫核）；加入赛事时复制到 EventGameBox';
COMMENT ON COLUMN public.gamebox_revisions.recommended_memory_bytes IS '推荐内存（字节）';
COMMENT ON COLUMN public.gamebox_revisions.recommended_pids_limit IS '推荐 pids 限制';
COMMENT ON COLUMN public.gamebox_revisions.healthchecks_json IS 'Readiness probes（HTTP/TCP 列表，canonical JSON）';
COMMENT ON COLUMN public.gamebox_revisions.judge_script_name IS '判题脚本文件名（如 check.py）';
COMMENT ON COLUMN public.gamebox_revisions.judge_script_content IS '判题脚本完整内容（Revision 自包含）';
COMMENT ON COLUMN public.gamebox_revisions.judge_args_json IS '判题参数模板（JSON，可选）';
COMMENT ON COLUMN public.gamebox_revisions.judge_timeout_secs IS '默认判题超时（秒，可选）';
COMMENT ON COLUMN public.gamebox_revisions.judge_retry_interval_secs IS '默认判题重试间隔（秒，可选）';
COMMENT ON COLUMN public.gamebox_revisions.build_status IS 'building | ready | failed';
COMMENT ON COLUMN public.gamebox_revisions.build_error IS '构建失败原因（bounded/sanitized）';
COMMENT ON COLUMN public.gamebox_revisions.created_at IS '创建时间（immutable row）';

-- ---------------------------------------------------------------------------
-- 2. 清理旧 EventGameBox 数据依赖（空表环境安全）并加 revision 钉住
-- ---------------------------------------------------------------------------
-- 无历史数据；若有残留 instance 先清空以保证 FK 可重建
DELETE FROM public.awd_gamebox_instances;
DELETE FROM public.awd_event_gameboxes;

ALTER TABLE public.awd_event_gameboxes
    ADD COLUMN IF NOT EXISTS gamebox_revision_id UUID;

COMMENT ON COLUMN public.awd_event_gameboxes.gamebox_revision_id IS '钉住的 GameBoxRevision（Deploy/Reset/Recovery 唯一镜像与 judge 来源）';

-- healthcheck_override 语义保留：赛事可覆盖 readiness probes；默认 NULL 表示用 revision
COMMENT ON COLUMN public.awd_event_gameboxes.healthcheck_override_json IS 'Readiness probes 覆盖（JSON 数组）；NULL 表示使用 Revision.healthchecks_json';

-- ---------------------------------------------------------------------------
-- 3. gameboxes 降为身份表：删除 runtime 配置列
-- ---------------------------------------------------------------------------
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS source_toml;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS image_ref;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS image_digest;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS username;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS default_cpu_millis;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS default_memory_bytes;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS default_pids_limit;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS healthcheck_json;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS judge_script_name;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS judge_script_content;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS judge_args_json;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS default_judge_timeout_secs;
ALTER TABLE public.gameboxes DROP COLUMN IF EXISTS default_judge_retry_interval_secs;

COMMENT ON TABLE public.gameboxes IS 'AWD GameBox 长期身份库（safe_name 稳定标识）；运行时配置在 gamebox_revisions';
COMMENT ON COLUMN public.gameboxes.name IS '展示名称（可随管理员更新；不作为 identity key）';
COMMENT ON COLUMN public.gameboxes.safe_name IS '稳定 package identity（URL/镜像路径友好，全局唯一）';

-- ---------------------------------------------------------------------------
-- 4. FK / UNIQUE / INDEX
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'gamebox_revisions_gamebox_id_fkey'
    ) THEN
        ALTER TABLE public.gamebox_revisions
            ADD CONSTRAINT gamebox_revisions_gamebox_id_fkey
            FOREIGN KEY (gamebox_id) REFERENCES public.gameboxes(id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS gamebox_revisions_gamebox_version_key
    ON public.gamebox_revisions (gamebox_id, version);

CREATE UNIQUE INDEX IF NOT EXISTS gamebox_revisions_gamebox_revision_number_key
    ON public.gamebox_revisions (gamebox_id, revision_number);

CREATE INDEX IF NOT EXISTS gamebox_revisions_package_digest_idx
    ON public.gamebox_revisions (package_digest);

CREATE INDEX IF NOT EXISTS gamebox_revisions_build_status_idx
    ON public.gamebox_revisions (build_status);

-- Ready 镜像 tag 唯一（允许 building/failed 期间 image_ref 为空或重复中间态）
CREATE UNIQUE INDEX IF NOT EXISTS gamebox_revisions_ready_image_ref_key
    ON public.gamebox_revisions (image_ref)
    WHERE build_status = 'ready' AND image_ref IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'awd_event_gameboxes_revision_fkey'
    ) THEN
        ALTER TABLE public.awd_event_gameboxes
            ADD CONSTRAINT awd_event_gameboxes_revision_fkey
            FOREIGN KEY (gamebox_revision_id) REFERENCES public.gamebox_revisions(id) ON DELETE RESTRICT;
    END IF;
END $$;

-- 钉住 revision 后不允许空
ALTER TABLE public.awd_event_gameboxes
    ALTER COLUMN gamebox_revision_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS awd_event_gameboxes_revision_idx
    ON public.awd_event_gameboxes (gamebox_revision_id);

-- 同一赛事同一 revision 只选一次（保留 event+gamebox 唯一）
CREATE UNIQUE INDEX IF NOT EXISTS awd_event_gameboxes_event_revision_key
    ON public.awd_event_gameboxes (event_id, gamebox_revision_id);
