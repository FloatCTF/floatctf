-- ================================================================================
-- Migration: 20260811235834-awdp-instances-endpoints
-- 目标：引入通用 runtime instance 根表（instances）+ 公开端点表（instance_endpoints），
--       作为 AWDP（以及未来 family）的 generic logical runtime identity。
--
-- 背景：现有 challenge_instances（Jeopardy）与 awd_gamebox_instances（AWD）
--       各自承载 runtime identity，均深度耦合各自领域语义。AWDP 按
--       chore/plans/implement-awdp.md §11 采用独立 generic 根表：
--         - instances 只负责 runtime（容器/镜像/代际/生命周期），不承载
--           round / phase / score / patch 等赛事语义；
--         - family 专属关系走 extension 表（AWDP 用 awdp_instances，
--           见后续 migration）；
--         - 本表服务 AWDP 及未来 family；现有两套表保持原样（不迁移存量，
--           避免破坏正在工作的 Jeopardy/AWD，列为后续演进项）。
-- ================================================================================

CREATE TABLE IF NOT EXISTS public.instances (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    -- 双主体归属：exactly-one（user XOR team）。
    owner_user_id UUID NULL,
    owner_team_id UUID NULL,
    -- 运行时钉扎镜像（repo@sha256 / sha256 / tag）。
    image_ref TEXT NULL,
    -- 当前物理容器（可随 reset/recreate 更换）。
    container_id TEXT NULL,
    -- 逻辑实例的稳定容器名（reset 后以同名重建）。
    container_name TEXT NOT NULL,
    -- pending | starting | running | stopped | failed
    runtime_state TEXT NOT NULL DEFAULT 'pending',
    -- 初始 1；reset/recreate +1；同容器 restart 不变。
    runtime_generation BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ NULL,
    stopped_at TIMESTAMPTZ NULL,
    expires_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT instances_exactly_one_owner_check
        CHECK ((owner_user_id IS NULL) <> (owner_team_id IS NULL)),
    CONSTRAINT instances_runtime_state_check
        CHECK (runtime_state IN ('pending', 'starting', 'running', 'stopped', 'failed')),
    CONSTRAINT instances_runtime_generation_positive_check
        CHECK (runtime_generation >= 1),
    CONSTRAINT instances_container_name_uidx UNIQUE (container_name)
);

COMMENT ON TABLE public.instances IS
    '通用 runtime 逻辑实例根表（与赛制无关）；容器/镜像/代际/生命周期。';
COMMENT ON COLUMN public.instances.owner_user_id IS '个人归属（Individual 模式）；与 owner_team_id 恰好一个非空';
COMMENT ON COLUMN public.instances.owner_team_id IS '战队归属（Team 模式）；与 owner_user_id 恰好一个非空。event_teams 主键为复合(event_id,id)，本表不设 FK，成员资格由 family 引擎校验';
COMMENT ON COLUMN public.instances.container_name IS '逻辑实例稳定容器名；reset 后同名重建（public endpoint 不变）';
COMMENT ON COLUMN public.instances.runtime_generation IS 'runtime 代际：初始 1，reset/recreate +1，同容器 restart 不变';

-- owner_user_id FK（users 是全局用户表，单列主键可直接引用）。
ALTER TABLE public.instances
    ADD CONSTRAINT instances_owner_user_fk
    FOREIGN KEY (owner_user_id) REFERENCES public.users (id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_instances_owner_user ON public.instances (owner_user_id);
CREATE INDEX IF NOT EXISTS idx_instances_owner_team ON public.instances (owner_team_id);

-- ================================================================================
-- instance_endpoints：逻辑实例的稳定公开端点（host:port 分配）。
-- 端点由 healthcheck 推导并去重（protocol + container_port），reset 期间保持不变。
-- ================================================================================

CREATE TABLE IF NOT EXISTS public.instance_endpoints (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    instance_id UUID NOT NULL REFERENCES public.instances (id) ON DELETE CASCADE,
    -- http | tcp（前端对 http 渲染 URL、对 tcp 渲染 nc host port）。
    protocol TEXT NOT NULL,
    -- 容器内端口（healthcheck 声明）。
    container_port INTEGER NOT NULL,
    -- 宿主公开地址与端口（Docker host 端口分配）。
    public_host TEXT NOT NULL,
    public_port INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT instance_endpoints_protocol_check
        CHECK (protocol IN ('http', 'tcp')),
    CONSTRAINT instance_endpoints_port_positive_check
        CHECK (container_port > 0 AND public_port > 0),
    CONSTRAINT instance_endpoints_unique_key
        UNIQUE (instance_id, protocol, container_port)
);

COMMENT ON TABLE public.instance_endpoints IS
    '逻辑实例的稳定公开端点（protocol+container_port 唯一）；healthcheck 推导，reset 保持不变';
COMMENT ON COLUMN public.instance_endpoints.protocol IS 'http（URL 形态）或 tcp（nc host port 形态）';
