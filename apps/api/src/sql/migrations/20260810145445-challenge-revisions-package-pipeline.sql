-- Migration: 20260810145445-challenge-revisions-package-pipeline
-- Challenge Package / Immutable Revision 管线：
--   challenges 降为长期身份表（删除 toml_str/attachment 运行时/版本字段）；
--   新增 challenge_revisions（不可变版本 + flag/port/attachment 元数据 + 镜像 digest）；
--   event_challenges 钉住 challenge_revision_id（复合 FK 保证 revision 属于 challenge）；
--   instances 记录创建时钉住的 challenge_revision_id（destroy/recovery 不再查 latest）。
-- 开发库无历史 Challenge 数据，采用 destructive clean schema（不迁移旧 TOML）。

-- ---------------------------------------------------------------------------
-- 1. challenge_revisions（先建表，后加 FK/索引）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.challenge_revisions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    challenge_id UUID NOT NULL,
    revision_number INTEGER NOT NULL,
    version TEXT NOT NULL,
    source_toml TEXT NOT NULL,
    spec_json JSONB NOT NULL,
    spec_digest TEXT NOT NULL,
    package_digest TEXT NOT NULL,
    flag_type TEXT NOT NULL,
    static_flag_value TEXT,
    container_port INTEGER,
    recommended_cpu_millis BIGINT NOT NULL DEFAULT 500,
    recommended_memory_bytes BIGINT NOT NULL DEFAULT 268435456,
    recommended_pids_limit BIGINT NOT NULL DEFAULT 100,
    attachment_path TEXT,
    attachment_name TEXT,
    attachment_size BIGINT,
    attachment_sha256 TEXT,
    image_ref TEXT,
    image_id TEXT,
    image_repo_digest TEXT,
    build_status TEXT NOT NULL DEFAULT 'building',
    build_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT challenge_revisions_flag_type_check
        CHECK (flag_type IN ('dynamic', 'static')),
    CONSTRAINT challenge_revisions_build_status_check
        CHECK (build_status IN ('building', 'ready', 'failed')),
    CONSTRAINT challenge_revisions_revision_number_positive
        CHECK (revision_number >= 1),
    CONSTRAINT challenge_revisions_id_challenge_key
        UNIQUE (id, challenge_id)
);

COMMENT ON TABLE public.challenge_revisions IS 'Challenge 不可变版本：package/spec digest、flag/port/attachment 元数据、镜像 identity';
COMMENT ON COLUMN public.challenge_revisions.id IS '主键';
COMMENT ON COLUMN public.challenge_revisions.challenge_id IS '所属 Challenge 身份（与 id 组成复合唯一，供 event/instance 复合 FK）';
COMMENT ON COLUMN public.challenge_revisions.revision_number IS '平台内部递增序号（同 challenge 内）';
COMMENT ON COLUMN public.challenge_revisions.version IS '作者 package SemVer（用作镜像 tag 后缀，非全局唯一）';
COMMENT ON COLUMN public.challenge_revisions.source_toml IS '原始 meta.toml 文本';
COMMENT ON COLUMN public.challenge_revisions.spec_json IS '规范化 manifest（canonical JSON，不含 static flag 明文）';
COMMENT ON COLUMN public.challenge_revisions.spec_digest IS '规范化 spec 的 SHA-256（hex）';
COMMENT ON COLUMN public.challenge_revisions.package_digest IS '逻辑 package 内容 SHA-256（hex：meta.toml+src/**+attachment/**，与 archive 外壳无关）';
COMMENT ON COLUMN public.challenge_revisions.flag_type IS 'dynamic | static';
COMMENT ON COLUMN public.challenge_revisions.static_flag_value IS 'static flag 明文（仅管理员可读；DTO/日志必须脱敏；dynamic 为 NULL）';
COMMENT ON COLUMN public.challenge_revisions.container_port IS '暴露的 TCP 端口（meta [docker].port；非 docker 题目为 NULL）';
COMMENT ON COLUMN public.challenge_revisions.recommended_cpu_millis IS '推荐 CPU（毫核）；加入赛事时可复制到 EventChallenge';
COMMENT ON COLUMN public.challenge_revisions.recommended_memory_bytes IS '推荐内存（字节）';
COMMENT ON COLUMN public.challenge_revisions.recommended_pids_limit IS '推荐 pids 限制';
COMMENT ON COLUMN public.challenge_revisions.attachment_path IS '附件相对路径（attachment/xxx，随 revision 不可变）';
COMMENT ON COLUMN public.challenge_revisions.attachment_name IS '附件文件名（展示用）';
COMMENT ON COLUMN public.challenge_revisions.attachment_size IS '附件大小（字节）';
COMMENT ON COLUMN public.challenge_revisions.attachment_sha256 IS '附件内容 SHA-256（hex，完整性校验）';
COMMENT ON COLUMN public.challenge_revisions.image_ref IS '人类可读镜像 tag：<registry-prefix>/challenges/<safe_name>:<version>';
COMMENT ON COLUMN public.challenge_revisions.image_id IS '本地 Docker image ID（sha256:…），与 RepoDigest 严格区分';
COMMENT ON COLUMN public.challenge_revisions.image_repo_digest IS 'Registry canonical RepoDigest（repo@sha256:…）；Runtime 优先使用';
COMMENT ON COLUMN public.challenge_revisions.build_status IS 'building | ready | failed';
COMMENT ON COLUMN public.challenge_revisions.build_error IS '构建失败原因（bounded/sanitized）';
COMMENT ON COLUMN public.challenge_revisions.created_at IS '创建时间（immutable row）';

-- ---------------------------------------------------------------------------
-- 2. challenges 降为身份表：删除运行时/版本列
-- ---------------------------------------------------------------------------
ALTER TABLE public.challenges DROP COLUMN IF EXISTS toml_str;
ALTER TABLE public.challenges DROP COLUMN IF EXISTS attachment;

COMMENT ON TABLE public.challenges IS 'Challenge 长期身份库（safe_name 稳定标识）；版本/运行时配置在 challenge_revisions';
COMMENT ON COLUMN public.challenges.name IS '展示名称（可随管理员更新；不作为 identity key）';
COMMENT ON COLUMN public.challenges.safe_name IS '稳定 package identity（URL/镜像路径友好，全局唯一）';

-- ---------------------------------------------------------------------------
-- 3. event_challenges 钉住 revision
-- ---------------------------------------------------------------------------
ALTER TABLE public.event_challenges
    ADD COLUMN IF NOT EXISTS challenge_revision_id UUID;

COMMENT ON COLUMN public.event_challenges.challenge_revision_id IS '钉住的 ChallengeRevision（Instance 创建/Reset/Recovery 唯一镜像与 flag 来源）；与 challenge_id 复合 FK 保证一致性';

-- 开发库无数据，直接置 NOT NULL（有历史数据时需先回填 latest ready revision）
ALTER TABLE public.event_challenges
    ALTER COLUMN challenge_revision_id SET NOT NULL;

-- ---------------------------------------------------------------------------
-- 4. instances 记录钉住的 revision（destroy/recovery 用）
-- ---------------------------------------------------------------------------
ALTER TABLE public.instances
    ADD COLUMN IF NOT EXISTS challenge_revision_id UUID;

COMMENT ON COLUMN public.instances.challenge_revision_id IS '创建该 instance 时钉住的 ChallengeRevision（可为 NULL 表示历史/未知）';

-- ---------------------------------------------------------------------------
-- 5. FK / UNIQUE / INDEX
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'challenge_revisions_challenge_id_fkey'
    ) THEN
        ALTER TABLE public.challenge_revisions
            ADD CONSTRAINT challenge_revisions_challenge_id_fkey
            FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS challenge_revisions_challenge_version_key
    ON public.challenge_revisions (challenge_id, version);

CREATE UNIQUE INDEX IF NOT EXISTS challenge_revisions_challenge_revision_number_key
    ON public.challenge_revisions (challenge_id, revision_number);

CREATE INDEX IF NOT EXISTS challenge_revisions_package_digest_idx
    ON public.challenge_revisions (package_digest);

CREATE INDEX IF NOT EXISTS challenge_revisions_build_status_idx
    ON public.challenge_revisions (build_status);

-- Ready 镜像 tag 唯一（允许 building/failed 期间 image_ref 为空或重复中间态）
CREATE UNIQUE INDEX IF NOT EXISTS challenge_revisions_ready_image_ref_key
    ON public.challenge_revisions (image_ref)
    WHERE build_status = 'ready' AND image_ref IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'event_challenges_revision_challenge_fkey'
    ) THEN
        -- 复合 FK：challenge_revision_id 必须属于同一 challenge_id
        ALTER TABLE public.event_challenges
            ADD CONSTRAINT event_challenges_revision_challenge_fkey
            FOREIGN KEY (challenge_revision_id, challenge_id)
            REFERENCES public.challenge_revisions(id, challenge_id) ON DELETE RESTRICT;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS event_challenges_event_revision_key
    ON public.event_challenges (event_id, challenge_revision_id);

CREATE INDEX IF NOT EXISTS event_challenges_revision_idx
    ON public.event_challenges (challenge_revision_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'instances_challenge_revision_id_fkey'
    ) THEN
        ALTER TABLE public.instances
            ADD CONSTRAINT instances_challenge_revision_id_fkey
            FOREIGN KEY (challenge_revision_id) REFERENCES public.challenge_revisions(id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS instances_challenge_revision_idx
    ON public.instances (challenge_revision_id);
