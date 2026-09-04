-- ================================================================================
-- Migration: 20260810121925-initial-schema
-- ================================================================================

-- FloatCTF Pre-v1 数据库 baseline：直接描述当前最终 Schema（squash 全部开发历史）。
--
-- 生成来源：REFERENCE DB（由旧 29 migrations 从零构建）的 pg_dump --schema-only，
-- 按逻辑章节组织后作为唯一 migration；旧 migrations 已由 Git 保存，不在此重复。
--
-- 章节：
--   Extensions / Types / Casts / Functions / Tables / Primary Keys /
--   Unique Constraints / Foreign Keys / Indexes / Triggers
--
-- 注：public.schema_migrations 由 migrate.sh 独占管理，不在此文件定义。

-- ================================================================================
-- Extensions（扩展）
-- ================================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;

COMMENT ON EXTENSION "uuid-ossp" IS 'generate universally unique identifiers (UUIDs)';


-- ================================================================================
-- Types（ENUM 类型）
-- ================================================================================

CREATE TYPE public.awd_event_status AS ENUM (
    'draft',
    'configuring',
    'deploying',
    'deployed',
    'prechecking',
    'verified',
    'running',
    'paused',
    'network_error',
    'start_blocked',
    'finished',
    'archived',
    'deploy_failed',
    'verification_failed'
);

CREATE TYPE public.awd_network_allocation_kind AS ENUM (
    'gamebox',
    'wireguard'
);

CREATE TYPE public.awd_network_allocation_mode AS ENUM (
    'automatic',
    'manual'
);

CREATE TYPE public.awd_phase AS ENUM (
    'hardening',
    'attack',
    'pause'
);

CREATE TYPE public.ban_status AS ENUM (
    'active',
    'pending_unban',
    'unbanned'
);

CREATE TYPE public.event_team_member_role AS ENUM (
    'captain',
    'member'
);

CREATE TYPE public.event_type AS ENUM (
    'jeopardy_practice',
    'jeopardy_single',
    'jeopardy_team',
    'awd_team'
);

CREATE TYPE public.gamebox_status AS ENUM (
    'pending',
    'creating',
    'running',
    'ready',
    'resetting',
    'missing',
    'orphan',
    'conflict',
    'start_failed',
    'reset_failed',
    'stopped'
);

CREATE TYPE public.instance_status AS ENUM (
    'pending',
    'running',
    'completed',
    'failed'
);

CREATE TYPE public.judge_task_status AS ENUM (
    'pending',
    'running',
    'up',
    'down',
    'judge_error',
    'judge_timeout',
    'skipped_resetting',
    'skipped_banned'
);

CREATE TYPE public.precheck_status AS ENUM (
    'pending',
    'running',
    'passed',
    'failed',
    'error'
);

CREATE TYPE public.round_status AS ENUM (
    'active',
    'grace',
    'completed',
    'paused'
);

CREATE TYPE public.score_event_type AS ENUM (
    'attack',
    'victim_loss',
    'judge_fix',
    'judge_down',
    'first_bonus',
    'reset_penalty',
    'adjustment'
);

CREATE TYPE public.setting_value_type AS ENUM (
    'string',
    'integer',
    'boolean',
    'float'
);

CREATE TYPE public.wg_peer_status AS ENUM (
    'active',
    'revoked',
    'rotating'
);


-- ================================================================================
-- Casts（类型转换）
-- ================================================================================

CREATE CAST (text AS cidr) WITH INOUT AS IMPLICIT;

CREATE CAST (text AS inet) WITH INOUT AS IMPLICIT;


-- ================================================================================
-- Functions（函数）
-- ================================================================================

CREATE FUNCTION public.update_updated_at_column() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
   NEW."updated_at" = now();
   RETURN NEW;
END;
$$;


-- ================================================================================
-- Tables（业务表，含 COMMENT）
-- ================================================================================

CREATE TABLE public.announcements (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    title text NOT NULL,
    content text,
    publisher_id uuid NOT NULL,
    publisher text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.announcements IS '平台公告表（由超级管理员发布）';

COMMENT ON COLUMN public.announcements.id IS '主键';

COMMENT ON COLUMN public.announcements.title IS '公告标题';

COMMENT ON COLUMN public.announcements.content IS '公告内容';

COMMENT ON COLUMN public.announcements.publisher_id IS '发布者（超级管理员）ID';

COMMENT ON COLUMN public.announcements.publisher IS '发布者名称';

COMMENT ON COLUMN public.announcements.created_at IS '创建时间';

COMMENT ON COLUMN public.announcements.updated_at IS '更新时间';

CREATE TABLE public.awd_event_gameboxes (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    gamebox_id uuid NOT NULL,
    host_offset smallint NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    hidden boolean DEFAULT false NOT NULL,
    cpu_millis bigint NOT NULL,
    memory_bytes bigint NOT NULL,
    pids_limit bigint DEFAULT 100 NOT NULL,
    healthcheck_override_json jsonb,
    judge_timeout_secs integer,
    judge_retry_interval_secs integer,
    break_points bigint NOT NULL,
    loss_points bigint NOT NULL,
    fix_points bigint NOT NULL,
    down_points bigint NOT NULL,
    first_bonus bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT awd_event_gameboxes_host_offset_check CHECK (((host_offset >= 2) AND (host_offset <= 254)))
);

COMMENT ON TABLE public.awd_event_gameboxes IS 'AWD 赛事 GameBox 选择：赛事采用的 GameBox Revision + 该场自己的计分/资源/可见性配置';

COMMENT ON COLUMN public.awd_event_gameboxes.gamebox_id IS 'GameBox 长期身份（RESTRICT：被赛事引用后禁止 hard delete）';

COMMENT ON COLUMN public.awd_event_gameboxes.host_offset IS '确定性 IP 分配偏移：instance_ip = team.gamebox_subnet + host_offset（2..254，禁改部署后的偏移）';

COMMENT ON COLUMN public.awd_event_gameboxes.enabled IS '是否启用（停用后不再部署/判题）';

COMMENT ON COLUMN public.awd_event_gameboxes.hidden IS '对玩家是否隐藏';

COMMENT ON COLUMN public.awd_event_gameboxes.cpu_millis IS '赛事 CPU 限制（毫核）覆盖';

COMMENT ON COLUMN public.awd_event_gameboxes.memory_bytes IS '赛事内存限制（字节）覆盖';

COMMENT ON COLUMN public.awd_event_gameboxes.pids_limit IS '赛事进程数限制覆盖';

COMMENT ON COLUMN public.awd_event_gameboxes.healthcheck_override_json IS '健康检查覆盖（JSON）';

COMMENT ON COLUMN public.awd_event_gameboxes.judge_timeout_secs IS '判题超时（秒）覆盖';

COMMENT ON COLUMN public.awd_event_gameboxes.judge_retry_interval_secs IS '判题重试间隔（秒）覆盖';

COMMENT ON COLUMN public.awd_event_gameboxes.break_points IS '被攻破时攻击方得分';

COMMENT ON COLUMN public.awd_event_gameboxes.loss_points IS '被攻破时防守方失分';

COMMENT ON COLUMN public.awd_event_gameboxes.fix_points IS '修复得分';

COMMENT ON COLUMN public.awd_event_gameboxes.down_points IS '宕机扣分';

COMMENT ON COLUMN public.awd_event_gameboxes.first_bonus IS '首破奖励';

CREATE TABLE public.awd_event_networks (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    allocation_mode public.awd_network_allocation_mode NOT NULL,
    gamebox_cidr cidr NOT NULL,
    wireguard_cidr cidr NOT NULL,
    infrastructure_subnet cidr NOT NULL,
    flagserver_ip inet NOT NULL,
    judgeserver_ip inet NOT NULL,
    wireguard_interface_name character varying(15) NOT NULL,
    wireguard_listen_port integer NOT NULL,
    docker_network_name character varying(64) NOT NULL,
    locked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT awd_event_networks_flag_inside_infra CHECK ((flagserver_ip << (infrastructure_subnet)::inet)),
    CONSTRAINT awd_event_networks_flag_ne_judge CHECK ((flagserver_ip <> judgeserver_ip)),
    CONSTRAINT awd_event_networks_gb_wg_no_overlap CHECK ((NOT ((gamebox_cidr)::inet && (wireguard_cidr)::inet))),
    CONSTRAINT awd_event_networks_infra_inside_gb CHECK (((infrastructure_subnet)::inet <<= (gamebox_cidr)::inet)),
    CONSTRAINT awd_event_networks_judge_inside_infra CHECK ((judgeserver_ip << (infrastructure_subnet)::inet)),
    CONSTRAINT awd_event_networks_locked_at CHECK (((locked_at IS NULL) OR (locked_at >= created_at)))
);

COMMENT ON COLUMN public.awd_event_networks.allocation_mode IS '分配模式：automatic（平台池自动 reserve）/ manual（管理员指定，仍走同一套 overlap 校验）';

COMMENT ON COLUMN public.awd_event_networks.gamebox_cidr IS '赛事 GameBox 网段（CIDR）';

COMMENT ON COLUMN public.awd_event_networks.wireguard_cidr IS '赛事 WireGuard 网段（CIDR）';

COMMENT ON COLUMN public.awd_event_networks.infrastructure_subnet IS '基础设施子网（gamebox CIDR 的第一块 team-size 子网）';

COMMENT ON COLUMN public.awd_event_networks.flagserver_ip IS 'FlagServer 固定 IP（位于 infrastructure_subnet 内）';

COMMENT ON COLUMN public.awd_event_networks.judgeserver_ip IS 'JudgeServer 固定 IP（位于 infrastructure_subnet 内）';

COMMENT ON COLUMN public.awd_event_networks.wireguard_interface_name IS 'WG 接口名（deterministic，<= 15 字符，Linux 限制）';

COMMENT ON COLUMN public.awd_event_networks.wireguard_listen_port IS 'WG 监听端口（平台端口池内分配，UNIQUE 兜底并发）';

COMMENT ON COLUMN public.awd_event_networks.docker_network_name IS 'Docker 网络逻辑名（desired identity；实际 network ID 属 Observed，存 awd_runtime_resources）';

COMMENT ON COLUMN public.awd_event_networks.locked_at IS 'Deploy 后置锁时间（锁定后 addressing 禁止修改）';

CREATE TABLE public.awd_events (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    status public.awd_event_status DEFAULT 'draft'::public.awd_event_status NOT NULL,
    phase public.awd_phase DEFAULT 'hardening'::public.awd_phase NOT NULL,
    event_secret_ciphertext bytea NOT NULL,
    event_secret_nonce bytea NOT NULL,
    flagserver_token_ciphertext bytea,
    flagserver_token_nonce bytea,
    judgeserver_token_ciphertext bytea,
    judgeserver_token_nonce bytea,
    wg_server_private_key_ciphertext bytea,
    wg_server_private_key_nonce bytea,
    wg_server_public_key character varying(44),
    key_version integer DEFAULT 1 NOT NULL,
    free_reset_count integer DEFAULT 3 NOT NULL,
    extra_reset_penalty bigint DEFAULT 100 NOT NULL,
    reset_protection_secs integer DEFAULT 120 NOT NULL,
    judge_max_concurrency integer DEFAULT 10 NOT NULL,
    judge_default_timeout_secs integer DEFAULT 30 NOT NULL,
    judge_retry_interval_secs integer DEFAULT 5 NOT NULL,
    judge_grace_period_secs integer DEFAULT 30 NOT NULL,
    round_duration_secs integer DEFAULT 300 NOT NULL,
    archive_retention_hours integer DEFAULT 168 NOT NULL,
    verified_at timestamp with time zone,
    verified_revision text,
    pause_remaining_secs integer,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    paused_phase public.awd_phase,
    configuration_generation bigint DEFAULT 0 NOT NULL,
    verified_generation bigint
);

COMMENT ON TABLE public.awd_events IS 'AWD 赛事配置：在 events 表基础上扩展的 AWD 专有配置（网络、加密密钥、计分、判题参数）';

COMMENT ON COLUMN public.awd_events.id IS '主键';

COMMENT ON COLUMN public.awd_events.event_id IS '关联赛事 ID（唯一）';

COMMENT ON COLUMN public.awd_events.status IS 'AWD 赛事状态机：draft→configuring→deploying→deployed→prechecking→verified→running→…';

COMMENT ON COLUMN public.awd_events.phase IS '当前阶段：hardening 加固 / attack 攻击 / pause 暂停';

COMMENT ON COLUMN public.awd_events.event_secret_ciphertext IS '事件密钥密文（加密存储）';

COMMENT ON COLUMN public.awd_events.event_secret_nonce IS '事件密钥加密 nonce';

COMMENT ON COLUMN public.awd_events.flagserver_token_ciphertext IS 'Flag 服务器令牌密文';

COMMENT ON COLUMN public.awd_events.flagserver_token_nonce IS 'Flag 服务器令牌 nonce';

COMMENT ON COLUMN public.awd_events.judgeserver_token_ciphertext IS '判题服务器令牌密文';

COMMENT ON COLUMN public.awd_events.judgeserver_token_nonce IS '判题服务器令牌 nonce';

COMMENT ON COLUMN public.awd_events.wg_server_private_key_ciphertext IS 'WireGuard 服务端私钥密文';

COMMENT ON COLUMN public.awd_events.wg_server_private_key_nonce IS 'WireGuard 服务端私钥 nonce';

COMMENT ON COLUMN public.awd_events.wg_server_public_key IS 'WireGuard 服务端公钥';

COMMENT ON COLUMN public.awd_events.key_version IS '密钥版本（轮换时递增）';

COMMENT ON COLUMN public.awd_events.free_reset_count IS '每队免费重置次数（默认 3）';

COMMENT ON COLUMN public.awd_events.extra_reset_penalty IS '超出免费次数后的额外重置惩罚分（默认 100）';

COMMENT ON COLUMN public.awd_events.reset_protection_secs IS '重置保护期（秒）：重置后一段时间内不可再次重置（默认 120）';

COMMENT ON COLUMN public.awd_events.judge_max_concurrency IS '判题最大并发数（默认 10）';

COMMENT ON COLUMN public.awd_events.judge_default_timeout_secs IS '判题默认超时（秒，默认 30）';

COMMENT ON COLUMN public.awd_events.judge_retry_interval_secs IS '判题失败重试间隔（秒，默认 5）';

COMMENT ON COLUMN public.awd_events.judge_grace_period_secs IS '判题宽限期（秒，默认 30）：回合结束后的判题缓冲时间';

COMMENT ON COLUMN public.awd_events.round_duration_secs IS '单回合时长（秒，默认 300）';

COMMENT ON COLUMN public.awd_events.archive_retention_hours IS '归档保留时长（小时，默认 168）';

COMMENT ON COLUMN public.awd_events.verified_at IS '验证通过时间';

COMMENT ON COLUMN public.awd_events.verified_revision IS '验证通过的配置版本';

COMMENT ON COLUMN public.awd_events.pause_remaining_secs IS '暂停时剩余的回合秒数（恢复时续走）';

COMMENT ON COLUMN public.awd_events.started_at IS '比赛开始时间';

COMMENT ON COLUMN public.awd_events.finished_at IS '比赛结束时间';

COMMENT ON COLUMN public.awd_events.created_at IS '创建时间';

COMMENT ON COLUMN public.awd_events.updated_at IS '更新时间';

COMMENT ON COLUMN public.awd_events.paused_phase IS '暂停前所处的比赛阶段（resume 时恢复，Phase 0 P0-1b 引入）';

COMMENT ON COLUMN public.awd_events.configuration_generation IS '配置代数：影响 runtime 的配置每次变更 +1（Phase 2 P2-9）';

COMMENT ON COLUMN public.awd_events.verified_generation IS '已验证代数：Precheck 成功时记录当时的 configuration_generation（Phase 2 P2-9）';

CREATE TABLE public.awd_flag_issues (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    round_id uuid NOT NULL,
    gamebox_instance_id uuid NOT NULL,
    flag_hash character varying(128) NOT NULL,
    issued_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_flag_issues IS 'AWD Flag 发放表：每轮每靶机确定性生成 flag（只存哈希，防泄密）';

COMMENT ON COLUMN public.awd_flag_issues.id IS '主键';

COMMENT ON COLUMN public.awd_flag_issues.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_flag_issues.round_id IS '回合 ID';

COMMENT ON COLUMN public.awd_flag_issues.gamebox_instance_id IS '靶机实例 ID';

COMMENT ON COLUMN public.awd_flag_issues.flag_hash IS 'flag 的 SHA-256 哈希';

COMMENT ON COLUMN public.awd_flag_issues.issued_at IS '发放时间';

CREATE TABLE public.awd_flag_submissions (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    round_id uuid NOT NULL,
    flag_issue_id uuid NOT NULL,
    attacker_team_id uuid NOT NULL,
    victim_team_id uuid NOT NULL,
    gamebox_instance_id uuid NOT NULL,
    submitted_by_user_id uuid NOT NULL,
    submitted_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_flag_submissions IS 'AWD Flag 提交表：攻击方提交对方靶机 flag 的记录';

COMMENT ON COLUMN public.awd_flag_submissions.id IS '主键';

COMMENT ON COLUMN public.awd_flag_submissions.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_flag_submissions.round_id IS '回合 ID';

COMMENT ON COLUMN public.awd_flag_submissions.flag_issue_id IS '对应的 flag 发放记录 ID';

COMMENT ON COLUMN public.awd_flag_submissions.attacker_team_id IS '攻击方队伍 ID';

COMMENT ON COLUMN public.awd_flag_submissions.victim_team_id IS '受害方队伍 ID';

COMMENT ON COLUMN public.awd_flag_submissions.gamebox_instance_id IS '被攻击的靶机实例 ID';

COMMENT ON COLUMN public.awd_flag_submissions.submitted_by_user_id IS '提交用户 ID';

COMMENT ON COLUMN public.awd_flag_submissions.submitted_at IS '提交时间';

CREATE TABLE public.awd_gamebox_instances (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    team_id uuid NOT NULL,
    status public.gamebox_status DEFAULT 'pending'::public.gamebox_status NOT NULL,
    container_name character varying(200) NOT NULL,
    gamebox_ip inet NOT NULL,
    health_status character varying(20) DEFAULT 'unknown'::character varying NOT NULL,
    reset_protection_until timestamp with time zone,
    last_health_check_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    event_gamebox_id uuid NOT NULL,
    runtime_generation bigint DEFAULT 1 NOT NULL,
    current_container_id character varying(64)
);

COMMENT ON TABLE public.awd_gamebox_instances IS 'AWD 靶机实例：每队伍每模板实际部署的容器实例';

COMMENT ON COLUMN public.awd_gamebox_instances.id IS '主键';

COMMENT ON COLUMN public.awd_gamebox_instances.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_gamebox_instances.team_id IS '队伍 ID';

COMMENT ON COLUMN public.awd_gamebox_instances.status IS '实例状态：pending/creating/running/ready/resetting/missing/orphan/conflict/start_failed/reset_failed/stopped';

COMMENT ON COLUMN public.awd_gamebox_instances.container_name IS '容器名（唯一）';

COMMENT ON COLUMN public.awd_gamebox_instances.gamebox_ip IS 'GameBox 固定 IP = Team gamebox_subnet + AwdEventGameBox.host_offset（INET）';

COMMENT ON COLUMN public.awd_gamebox_instances.health_status IS '健康状态（默认 unknown）';

COMMENT ON COLUMN public.awd_gamebox_instances.reset_protection_until IS '重置保护截止时间（此时间前不可重置）';

COMMENT ON COLUMN public.awd_gamebox_instances.last_health_check_at IS '最近一次健康检查时间';

COMMENT ON COLUMN public.awd_gamebox_instances.created_at IS '创建时间';

COMMENT ON COLUMN public.awd_gamebox_instances.updated_at IS '更新时间';

COMMENT ON COLUMN public.awd_gamebox_instances.deleted_at IS '软删除时间（NULL=未删除）';

COMMENT ON COLUMN public.awd_gamebox_instances.event_gamebox_id IS '所属赛事 GameBox 选择（逻辑靶机定义；Migration B 回填后加 NOT NULL + FK）';

COMMENT ON COLUMN public.awd_gamebox_instances.runtime_generation IS '运行时代数：首次部署=1，Reset 成功替换容器 +1（容器只是当前 runtime realization）';

COMMENT ON COLUMN public.awd_gamebox_instances.current_container_id IS '当前 Docker 容器 ID（可替换的运行时资源；对应旧 container_id，改名强调非逻辑身份）';

CREATE TABLE public.awd_internal_token_rotations (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    token_type character varying(30) NOT NULL,
    rotated_by uuid,
    rotated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_internal_token_rotations IS 'AWD 内部令牌轮换审计：flagserver/judgeserver 令牌与事件密钥的轮换记录';

COMMENT ON COLUMN public.awd_internal_token_rotations.id IS '主键';

COMMENT ON COLUMN public.awd_internal_token_rotations.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_internal_token_rotations.token_type IS '令牌类型：flagserver / judgeserver / event_secret';

COMMENT ON COLUMN public.awd_internal_token_rotations.rotated_by IS '轮换操作人（超级管理员）';

COMMENT ON COLUMN public.awd_internal_token_rotations.rotated_at IS '轮换时间';

CREATE TABLE public.awd_judge_batches (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    round_id uuid NOT NULL,
    total_tasks integer DEFAULT 0 NOT NULL,
    completed_tasks integer DEFAULT 0 NOT NULL,
    failed_tasks integer DEFAULT 0 NOT NULL,
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_judge_batches IS 'AWD 判题批次：每回合发起的一批判题任务的汇总（进度与结果统计）';

COMMENT ON COLUMN public.awd_judge_batches.id IS '主键';

COMMENT ON COLUMN public.awd_judge_batches.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_judge_batches.round_id IS '回合 ID';

COMMENT ON COLUMN public.awd_judge_batches.total_tasks IS '总任务数';

COMMENT ON COLUMN public.awd_judge_batches.completed_tasks IS '已完成任务数';

COMMENT ON COLUMN public.awd_judge_batches.failed_tasks IS '失败任务数';

COMMENT ON COLUMN public.awd_judge_batches.status IS '批次状态（默认 pending）';

COMMENT ON COLUMN public.awd_judge_batches.created_at IS '创建时间';

CREATE TABLE public.awd_judge_tasks (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    batch_id uuid NOT NULL,
    event_id uuid NOT NULL,
    round_id uuid NOT NULL,
    gamebox_instance_id uuid NOT NULL,
    team_id uuid NOT NULL,
    status public.judge_task_status DEFAULT 'pending'::public.judge_task_status NOT NULL,
    attempt_count integer DEFAULT 0 NOT NULL,
    max_attempts integer DEFAULT 2 NOT NULL,
    deadline_at timestamp with time zone NOT NULL,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    exit_code integer,
    stdout_limited text,
    stderr_limited text,
    duration_ms integer,
    callback_idempotency_key character varying(300),
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    event_gamebox_id uuid
);

COMMENT ON TABLE public.awd_judge_tasks IS 'AWD 判题任务：对每个靶机实例执行健康/服务判定的单个任务（含重试与输出记录）';

COMMENT ON COLUMN public.awd_judge_tasks.id IS '主键';

COMMENT ON COLUMN public.awd_judge_tasks.batch_id IS '所属判题批次 ID';

COMMENT ON COLUMN public.awd_judge_tasks.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_judge_tasks.round_id IS '回合 ID';

COMMENT ON COLUMN public.awd_judge_tasks.gamebox_instance_id IS '被判定靶机实例 ID';

COMMENT ON COLUMN public.awd_judge_tasks.team_id IS '所属队伍 ID';

COMMENT ON COLUMN public.awd_judge_tasks.status IS '任务状态：pending/running/up/down/judge_error/judge_timeout/skipped_resetting/skipped_banned';

COMMENT ON COLUMN public.awd_judge_tasks.attempt_count IS '已尝试次数';

COMMENT ON COLUMN public.awd_judge_tasks.max_attempts IS '最大尝试次数（默认 2）';

COMMENT ON COLUMN public.awd_judge_tasks.deadline_at IS '执行截止时间';

COMMENT ON COLUMN public.awd_judge_tasks.started_at IS '开始执行时间';

COMMENT ON COLUMN public.awd_judge_tasks.finished_at IS '执行完成时间';

COMMENT ON COLUMN public.awd_judge_tasks.exit_code IS '判题脚本退出码';

COMMENT ON COLUMN public.awd_judge_tasks.stdout_limited IS '截断后的标准输出';

COMMENT ON COLUMN public.awd_judge_tasks.stderr_limited IS '截断后的标准错误输出';

COMMENT ON COLUMN public.awd_judge_tasks.duration_ms IS '执行耗时（毫秒）';

COMMENT ON COLUMN public.awd_judge_tasks.callback_idempotency_key IS '回调幂等键（防止判题回调重复处理）';

COMMENT ON COLUMN public.awd_judge_tasks.created_at IS '创建时间';

COMMENT ON COLUMN public.awd_judge_tasks.event_gamebox_id IS '判题目标 EventGameBox（SET NULL：EventGameBox 删除后保留历史行）';

CREATE TABLE public.awd_network_allocations (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    kind public.awd_network_allocation_kind NOT NULL,
    cidr cidr NOT NULL,
    allocated_at timestamp with time zone DEFAULT now() NOT NULL,
    released_at timestamp with time zone,
    CONSTRAINT awd_network_allocations_release_order CHECK (((released_at IS NULL) OR (released_at >= allocated_at)))
);

COMMENT ON COLUMN public.awd_network_allocations.kind IS '分配种类：gamebox / wireguard';

COMMENT ON COLUMN public.awd_network_allocations.cidr IS '被占用的 CIDR 块';

COMMENT ON COLUMN public.awd_network_allocations.allocated_at IS '分配时间';

COMMENT ON COLUMN public.awd_network_allocations.released_at IS '释放时间（仅 Event Archive runtime cleanup 成功后写入；NULL=仍占用）';

CREATE TABLE public.awd_network_settings (
    id smallint DEFAULT 1 NOT NULL,
    gamebox_pool cidr NOT NULL,
    gamebox_event_prefix smallint NOT NULL,
    gamebox_team_prefix smallint NOT NULL,
    wireguard_pool cidr NOT NULL,
    wireguard_event_prefix smallint NOT NULL,
    wireguard_team_prefix smallint NOT NULL,
    wireguard_port_min integer NOT NULL,
    wireguard_port_max integer NOT NULL,
    wireguard_public_endpoint text,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT awd_network_settings_gb_prefix_order CHECK (((masklen((gamebox_pool)::inet) <= gamebox_event_prefix) AND (gamebox_event_prefix <= gamebox_team_prefix))),
    CONSTRAINT awd_network_settings_pools_no_overlap CHECK ((NOT ((gamebox_pool)::inet && (wireguard_pool)::inet))),
    CONSTRAINT awd_network_settings_port_range CHECK (((wireguard_port_min >= 1) AND (wireguard_port_max <= 65535) AND (wireguard_port_min <= wireguard_port_max))),
    CONSTRAINT awd_network_settings_singleton CHECK ((id = 1)),
    CONSTRAINT awd_network_settings_wg_prefix_order CHECK (((masklen((wireguard_pool)::inet) <= wireguard_event_prefix) AND (wireguard_event_prefix <= wireguard_team_prefix)))
);

COMMENT ON COLUMN public.awd_network_settings.gamebox_pool IS 'GameBox 地址池（CIDR）';

COMMENT ON COLUMN public.awd_network_settings.gamebox_event_prefix IS '每场 Event 分配的 GameBox 前缀长度';

COMMENT ON COLUMN public.awd_network_settings.gamebox_team_prefix IS '每 Team 分配的 GameBox 前缀长度';

COMMENT ON COLUMN public.awd_network_settings.wireguard_pool IS 'WireGuard 地址池（CIDR）';

COMMENT ON COLUMN public.awd_network_settings.wireguard_event_prefix IS '每场 Event 分配的 WG 前缀长度';

COMMENT ON COLUMN public.awd_network_settings.wireguard_team_prefix IS '每 Team 分配的 WG 前缀长度';

COMMENT ON COLUMN public.awd_network_settings.wireguard_port_min IS 'WG 监听端口池下限（含）';

COMMENT ON COLUMN public.awd_network_settings.wireguard_port_max IS 'WG 监听端口池上限（含）';

COMMENT ON COLUMN public.awd_network_settings.wireguard_public_endpoint IS '平台 WG 公网入口（hostname/IP，不含端口；端口来自 Event）';

COMMENT ON COLUMN public.awd_network_settings.updated_at IS '最近一次修改时间';

CREATE TABLE public.awd_orphan_resources (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid,
    resource_type character varying(50) NOT NULL,
    resource_id character varying(200) NOT NULL,
    resource_name character varying(200),
    observed_state jsonb,
    discovered_at timestamp with time zone DEFAULT now() NOT NULL,
    resolved_at timestamp with time zone,
    resolution character varying(20) DEFAULT 'pending'::character varying
);

COMMENT ON TABLE public.awd_orphan_resources IS 'AWD 孤儿资源：数据库无记录但 Docker/WireGuard 中实际存在的资源（泄漏检测与清理）';

COMMENT ON COLUMN public.awd_orphan_resources.id IS '主键';

COMMENT ON COLUMN public.awd_orphan_resources.event_id IS '赛事 ID（可为空，删除时置 NULL）';

COMMENT ON COLUMN public.awd_orphan_resources.resource_type IS '资源类型';

COMMENT ON COLUMN public.awd_orphan_resources.resource_id IS '资源 ID';

COMMENT ON COLUMN public.awd_orphan_resources.resource_name IS '资源名称';

COMMENT ON COLUMN public.awd_orphan_resources.observed_state IS '观察到的状态（JSON）';

COMMENT ON COLUMN public.awd_orphan_resources.discovered_at IS '发现时间';

COMMENT ON COLUMN public.awd_orphan_resources.resolved_at IS '处理完成时间';

COMMENT ON COLUMN public.awd_orphan_resources.resolution IS '处理结果：pending 待处理 / adopted 已接管 / cleaned 已清理';

CREATE TABLE public.awd_precheck_runs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    status public.precheck_status DEFAULT 'pending'::public.precheck_status NOT NULL,
    trigger character varying(20) DEFAULT 'manual'::character varying NOT NULL,
    revision text,
    config_check jsonb,
    container_check jsonb,
    wireguard_check jsonb,
    network_check jsonb,
    flag_check jsonb,
    judge_check jsonb,
    error_msg text,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone
);

COMMENT ON TABLE public.awd_precheck_runs IS 'AWD 赛前检查：比赛开始前对配置/容器/WireGuard/网络/flag/判题的整体体检记录';

COMMENT ON COLUMN public.awd_precheck_runs.id IS '主键';

COMMENT ON COLUMN public.awd_precheck_runs.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_precheck_runs.status IS '检查状态：pending / running / passed / failed / error';

COMMENT ON COLUMN public.awd_precheck_runs.trigger IS '触发方式：manual 手动 / auto_t_minus_1h 开赛前 1 小时自动';

COMMENT ON COLUMN public.awd_precheck_runs.revision IS '被检查的配置版本';

COMMENT ON COLUMN public.awd_precheck_runs.config_check IS '配置检查结果（JSON）';

COMMENT ON COLUMN public.awd_precheck_runs.container_check IS '容器检查结果（JSON）';

COMMENT ON COLUMN public.awd_precheck_runs.wireguard_check IS 'WireGuard 检查结果（JSON）';

COMMENT ON COLUMN public.awd_precheck_runs.network_check IS '网络检查结果（JSON）';

COMMENT ON COLUMN public.awd_precheck_runs.flag_check IS 'flag 检查结果（JSON）';

COMMENT ON COLUMN public.awd_precheck_runs.judge_check IS '判题检查结果（JSON）';

COMMENT ON COLUMN public.awd_precheck_runs.error_msg IS '检查失败原因';

COMMENT ON COLUMN public.awd_precheck_runs.started_at IS '检查开始时间';

COMMENT ON COLUMN public.awd_precheck_runs.completed_at IS '检查完成时间';

CREATE TABLE public.awd_reset_records (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    team_id uuid NOT NULL,
    gamebox_instance_id uuid NOT NULL,
    round_id uuid,
    requested_by uuid,
    free_reset boolean DEFAULT true NOT NULL,
    penalty_score_event_id uuid,
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    error_msg text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_reset_records IS 'AWD 靶机重置记录：队伍请求重置靶机的流水（含免费/惩罚、执行状态）';

COMMENT ON COLUMN public.awd_reset_records.id IS '主键';

COMMENT ON COLUMN public.awd_reset_records.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_reset_records.team_id IS '请求队伍 ID';

COMMENT ON COLUMN public.awd_reset_records.gamebox_instance_id IS '被重置的靶机实例 ID';

COMMENT ON COLUMN public.awd_reset_records.round_id IS '请求所在回合 ID（可为空）';

COMMENT ON COLUMN public.awd_reset_records.requested_by IS '请求用户 ID（可为空）';

COMMENT ON COLUMN public.awd_reset_records.free_reset IS '是否免费重置（超出免费次数则扣分）';

COMMENT ON COLUMN public.awd_reset_records.penalty_score_event_id IS '扣除的惩罚积分事件 ID';

COMMENT ON COLUMN public.awd_reset_records.status IS '重置状态（默认 pending）';

COMMENT ON COLUMN public.awd_reset_records.started_at IS '开始执行时间';

COMMENT ON COLUMN public.awd_reset_records.completed_at IS '完成时间';

COMMENT ON COLUMN public.awd_reset_records.error_msg IS '失败原因';

COMMENT ON COLUMN public.awd_reset_records.created_at IS '创建时间';

CREATE TABLE public.awd_rounds (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    round_number integer NOT NULL,
    status public.round_status DEFAULT 'active'::public.round_status NOT NULL,
    phase public.awd_phase DEFAULT 'attack'::public.awd_phase NOT NULL,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    scheduled_end_at timestamp with time zone NOT NULL,
    grace_ends_at timestamp with time zone,
    paused_at timestamp with time zone,
    remaining_secs integer,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_rounds IS 'AWD 回合表：比赛按固定时长推进的回合（含宽限期、暂停与完成状态）';

COMMENT ON COLUMN public.awd_rounds.id IS '主键';

COMMENT ON COLUMN public.awd_rounds.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_rounds.round_number IS '回合序号（赛事内唯一）';

COMMENT ON COLUMN public.awd_rounds.status IS '回合状态：active / grace 宽限 / completed / paused';

COMMENT ON COLUMN public.awd_rounds.phase IS '回合阶段（默认 attack）';

COMMENT ON COLUMN public.awd_rounds.started_at IS '回合开始时间';

COMMENT ON COLUMN public.awd_rounds.scheduled_end_at IS '计划结束时间';

COMMENT ON COLUMN public.awd_rounds.grace_ends_at IS '宽限期结束时间（可为空）';

COMMENT ON COLUMN public.awd_rounds.paused_at IS '暂停时间（可为空）';

COMMENT ON COLUMN public.awd_rounds.remaining_secs IS '暂停时剩余秒数（恢复时续走）';

COMMENT ON COLUMN public.awd_rounds.completed_at IS '完成时间';

COMMENT ON COLUMN public.awd_rounds.created_at IS '创建时间';

CREATE TABLE public.awd_runtime_resources (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    resource_type character varying(50) NOT NULL,
    resource_id character varying(200) NOT NULL,
    resource_name character varying(200),
    observed_state jsonb,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.awd_runtime_resources IS 'AWD 运行时资源：系统实际创建的 Docker 网络/容器/WireGuard 网卡等资源（用于对账）';

COMMENT ON COLUMN public.awd_runtime_resources.id IS '主键';

COMMENT ON COLUMN public.awd_runtime_resources.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_runtime_resources.resource_type IS '资源类型：docker_network / container / wireguard_iface';

COMMENT ON COLUMN public.awd_runtime_resources.resource_id IS '资源 ID（Docker 网络 ID/容器 ID 等）';

COMMENT ON COLUMN public.awd_runtime_resources.resource_name IS '资源名称';

COMMENT ON COLUMN public.awd_runtime_resources.observed_state IS '观察到的资源状态（JSON）';

COMMENT ON COLUMN public.awd_runtime_resources.last_seen_at IS '最近一次观察到的时间';

CREATE TABLE public.awd_score_events (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    round_id uuid,
    team_id uuid NOT NULL,
    event_type public.score_event_type NOT NULL,
    delta bigint NOT NULL,
    idempotency_key character varying(300) NOT NULL,
    related_team_id uuid,
    gamebox_instance_id uuid,
    reference_id uuid,
    reason text,
    metadata_json jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    event_gamebox_id uuid
);

COMMENT ON TABLE public.awd_score_events IS 'AWD 积分事件账本：只追加（append-only），所有得分/扣分/调整的审计轨迹';

COMMENT ON COLUMN public.awd_score_events.id IS '主键';

COMMENT ON COLUMN public.awd_score_events.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_score_events.round_id IS '回合 ID（可为空）';

COMMENT ON COLUMN public.awd_score_events.team_id IS '产生积分变化的队伍 ID';

COMMENT ON COLUMN public.awd_score_events.event_type IS '事件类型：attack 攻击得分 / victim_loss 受害失分 / judge_fix 修复 / judge_down 宕机 / first_bonus 首破 / reset_penalty 重置惩罚 / adjustment 人工调整';

COMMENT ON COLUMN public.awd_score_events.delta IS '积分变化量（正为得分，负为扣分）';

COMMENT ON COLUMN public.awd_score_events.idempotency_key IS '幂等键（唯一，防止重复记账）';

COMMENT ON COLUMN public.awd_score_events.related_team_id IS '关联队伍（如攻击/受害中的另一方，可为空）';

COMMENT ON COLUMN public.awd_score_events.gamebox_instance_id IS '关联靶机实例（可为空）';

COMMENT ON COLUMN public.awd_score_events.reference_id IS '参考 ID（如关联的重置记录，可为空）';

COMMENT ON COLUMN public.awd_score_events.reason IS '事件原因说明';

COMMENT ON COLUMN public.awd_score_events.metadata_json IS '附加元数据（JSON）';

COMMENT ON COLUMN public.awd_score_events.created_by IS '创建人（超级管理员，人工调整时有值）';

COMMENT ON COLUMN public.awd_score_events.created_at IS '创建时间';

COMMENT ON COLUMN public.awd_score_events.event_gamebox_id IS '计分作用域 EventGameBox（SET NULL：EventGameBox 删除后保留历史行）';

CREATE TABLE public.awd_team_bans (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    team_id uuid NOT NULL,
    status public.ban_status DEFAULT 'active'::public.ban_status NOT NULL,
    reason text,
    effective_round_id uuid,
    banned_by uuid,
    banned_at timestamp with time zone DEFAULT now() NOT NULL,
    unban_requested_at timestamp with time zone,
    unban_effective_round_id uuid,
    unbanned_by uuid,
    unbanned_at timestamp with time zone
);

COMMENT ON TABLE public.awd_team_bans IS 'AWD 队伍封禁表：因违规被封禁的队伍（含申请解封与生效回合）';

COMMENT ON COLUMN public.awd_team_bans.id IS '主键';

COMMENT ON COLUMN public.awd_team_bans.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_team_bans.team_id IS '被封禁队伍 ID';

COMMENT ON COLUMN public.awd_team_bans.status IS '封禁状态：active / pending_unban 待解封 / unbanned';

COMMENT ON COLUMN public.awd_team_bans.reason IS '封禁原因';

COMMENT ON COLUMN public.awd_team_bans.effective_round_id IS '封禁生效回合（可为空）';

COMMENT ON COLUMN public.awd_team_bans.banned_by IS '封禁人（超级管理员）';

COMMENT ON COLUMN public.awd_team_bans.banned_at IS '封禁时间';

COMMENT ON COLUMN public.awd_team_bans.unban_requested_at IS '申请解封时间';

COMMENT ON COLUMN public.awd_team_bans.unban_effective_round_id IS '解封生效回合（可为空）';

COMMENT ON COLUMN public.awd_team_bans.unbanned_by IS '解封人（超级管理员）';

COMMENT ON COLUMN public.awd_team_bans.unbanned_at IS '解封时间';

CREATE TABLE public.awd_team_networks (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    team_id uuid NOT NULL,
    gamebox_subnet cidr NOT NULL,
    wireguard_subnet cidr NOT NULL,
    ssh_password_ciphertext bytea NOT NULL,
    ssh_password_nonce bytea NOT NULL,
    key_version integer DEFAULT 1 NOT NULL,
    next_wireguard_host integer DEFAULT 2 NOT NULL,
    status character varying(20) DEFAULT 'active'::character varying NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    subnet_index smallint NOT NULL
);

COMMENT ON TABLE public.awd_team_networks IS 'AWD 队伍网络分配：每赛事每队伍的靶机/ WireGuard 子网与 SSH 凭据';

COMMENT ON COLUMN public.awd_team_networks.id IS '主键';

COMMENT ON COLUMN public.awd_team_networks.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_team_networks.team_id IS '队伍 ID';

COMMENT ON COLUMN public.awd_team_networks.gamebox_subnet IS 'Team GameBox 子网（Event Network 内的稳定持久分配，CIDR）';

COMMENT ON COLUMN public.awd_team_networks.wireguard_subnet IS 'Team WireGuard 子网（CIDR）';

COMMENT ON COLUMN public.awd_team_networks.ssh_password_ciphertext IS '靶机 SSH 密码密文';

COMMENT ON COLUMN public.awd_team_networks.ssh_password_nonce IS 'SSH 密码加密 nonce';

COMMENT ON COLUMN public.awd_team_networks.key_version IS '密钥版本';

COMMENT ON COLUMN public.awd_team_networks.next_wireguard_host IS '下一个可分配的 WireGuard 主机位（从 2 开始）';

COMMENT ON COLUMN public.awd_team_networks.status IS '状态（默认 active）';

COMMENT ON COLUMN public.awd_team_networks.created_at IS '创建时间';

COMMENT ON COLUMN public.awd_team_networks.updated_at IS '更新时间';

COMMENT ON COLUMN public.awd_team_networks.subnet_index IS 'Team 在 Event 内的稳定子网 slot 序号（0=infra 保留；已释放 slot 不复用）';

CREATE TABLE public.awd_wireguard_peers (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    team_id uuid NOT NULL,
    user_id uuid NOT NULL,
    status public.wg_peer_status DEFAULT 'active'::public.wg_peer_status NOT NULL,
    assigned_ip inet NOT NULL,
    public_key character varying(44) NOT NULL,
    private_key_ciphertext bytea NOT NULL,
    private_key_nonce bytea NOT NULL,
    key_version integer DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    rotated_at timestamp with time zone,
    revoked_at timestamp with time zone,
    config_fetched_at timestamp with time zone
);

COMMENT ON TABLE public.awd_wireguard_peers IS 'AWD WireGuard 对等端：队伍成员接入靶机网络的 VPN 客户端（密钥加密存储，支持轮换/吊销）';

COMMENT ON COLUMN public.awd_wireguard_peers.id IS '主键';

COMMENT ON COLUMN public.awd_wireguard_peers.event_id IS '赛事 ID';

COMMENT ON COLUMN public.awd_wireguard_peers.team_id IS '所属队伍 ID';

COMMENT ON COLUMN public.awd_wireguard_peers.user_id IS '成员用户 ID';

COMMENT ON COLUMN public.awd_wireguard_peers.status IS '状态：active 生效 / revoked 已吊销 / rotating 轮换中';

COMMENT ON COLUMN public.awd_wireguard_peers.assigned_ip IS 'Peer 稳定 /32 地址（INET）';

COMMENT ON COLUMN public.awd_wireguard_peers.public_key IS '对端公钥（唯一）';

COMMENT ON COLUMN public.awd_wireguard_peers.private_key_ciphertext IS '对端私钥密文';

COMMENT ON COLUMN public.awd_wireguard_peers.private_key_nonce IS '私钥加密 nonce';

COMMENT ON COLUMN public.awd_wireguard_peers.key_version IS '密钥版本';

COMMENT ON COLUMN public.awd_wireguard_peers.created_at IS '创建时间';

COMMENT ON COLUMN public.awd_wireguard_peers.rotated_at IS '最近密钥轮换时间';

COMMENT ON COLUMN public.awd_wireguard_peers.revoked_at IS '吊销时间';

COMMENT ON COLUMN public.awd_wireguard_peers.config_fetched_at IS 'WG 配置（含私钥）首次拉取时间；NULL=尚未拉取（Phase 1 P1-15）';

CREATE TABLE public.challenge_set_items (
    set_id uuid NOT NULL,
    challenge_id uuid NOT NULL
);

COMMENT ON TABLE public.challenge_set_items IS '题目集合与题目的多对多关联表';

COMMENT ON COLUMN public.challenge_set_items.set_id IS '集合 ID';

COMMENT ON COLUMN public.challenge_set_items.challenge_id IS '题目 ID';

CREATE TABLE public.challenge_sets (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name text NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.challenge_sets IS '题目集合表：把若干题目组织为一个集合（如专题）';

COMMENT ON COLUMN public.challenge_sets.id IS '主键';

COMMENT ON COLUMN public.challenge_sets.name IS '集合名称';

COMMENT ON COLUMN public.challenge_sets.description IS '集合描述';

COMMENT ON COLUMN public.challenge_sets.created_at IS '创建时间';

CREATE TABLE public.challenge_solves (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    challenge_id uuid NOT NULL,
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    event_id uuid
);

COMMENT ON TABLE public.challenge_solves IS '独立解题记录：练习模式的解题流水（event_id 为空）；赛事解题另有 event_challenge_solves';

COMMENT ON COLUMN public.challenge_solves.id IS '主键';

COMMENT ON COLUMN public.challenge_solves.challenge_id IS '题目 ID';

COMMENT ON COLUMN public.challenge_solves.user_id IS '解题用户 ID';

COMMENT ON COLUMN public.challenge_solves.created_at IS '解题时间';

COMMENT ON COLUMN public.challenge_solves.event_id IS '所属赛事 ID（NULL=独立/练习解题）';

CREATE TABLE public.challenge_writeup (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    challenge_id uuid NOT NULL,
    user_id uuid NOT NULL,
    content text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.challenge_writeup IS '题解表：用户提交的题目 WriteUp';

COMMENT ON COLUMN public.challenge_writeup.id IS '主键';

COMMENT ON COLUMN public.challenge_writeup.challenge_id IS '题目 ID';

COMMENT ON COLUMN public.challenge_writeup.user_id IS '作者用户 ID';

COMMENT ON COLUMN public.challenge_writeup.content IS '题解内容';

COMMENT ON COLUMN public.challenge_writeup.created_at IS '创建时间';

CREATE TABLE public.challenges (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name text NOT NULL,
    safe_name text NOT NULL,
    category text DEFAULT 'other'::text NOT NULL,
    description text DEFAULT 'no description'::text NOT NULL,
    attachment text,
    hidden boolean DEFAULT true NOT NULL,
    toml_str text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.challenges IS '题目表：Jeopardy 独立题目（含题目实例的 TOML 部署配置）';

COMMENT ON COLUMN public.challenges.id IS '主键';

COMMENT ON COLUMN public.challenges.name IS '题目名称（唯一）';

COMMENT ON COLUMN public.challenges.safe_name IS '安全名称（URL/路径友好，唯一）';

COMMENT ON COLUMN public.challenges.category IS '分类（默认 other）';

COMMENT ON COLUMN public.challenges.description IS '题目描述';

COMMENT ON COLUMN public.challenges.attachment IS '附件（可为空）';

COMMENT ON COLUMN public.challenges.hidden IS '是否隐藏';

COMMENT ON COLUMN public.challenges.toml_str IS '题目实例部署配置（TOML 文本）';

COMMENT ON COLUMN public.challenges.created_at IS '创建时间';

COMMENT ON COLUMN public.challenges.updated_at IS '更新时间';

CREATE TABLE public.discussion_comments (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    discussion_id uuid NOT NULL,
    author_id uuid NOT NULL,
    content text NOT NULL,
    parent_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.discussion_comments IS '评论表：支持回复（parent_id 指向父评论）';

COMMENT ON COLUMN public.discussion_comments.id IS '主键';

COMMENT ON COLUMN public.discussion_comments.discussion_id IS '所属帖子 ID';

COMMENT ON COLUMN public.discussion_comments.author_id IS '评论作者用户 ID';

COMMENT ON COLUMN public.discussion_comments.content IS '评论内容';

COMMENT ON COLUMN public.discussion_comments.parent_id IS '父评论 ID（NULL=顶级评论）';

COMMENT ON COLUMN public.discussion_comments.created_at IS '创建时间';

COMMENT ON COLUMN public.discussion_comments.updated_at IS '更新时间';

CREATE TABLE public.discussion_likes (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    discussion_id uuid NOT NULL,
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.discussion_likes IS '点赞表：用户对帖子的点赞（唯一约束防止重复点赞）';

COMMENT ON COLUMN public.discussion_likes.id IS '主键';

COMMENT ON COLUMN public.discussion_likes.discussion_id IS '帖子 ID';

COMMENT ON COLUMN public.discussion_likes.user_id IS '点赞用户 ID';

COMMENT ON COLUMN public.discussion_likes.created_at IS '点赞时间';

CREATE TABLE public.discussions (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    title text NOT NULL,
    content text NOT NULL,
    author_id uuid NOT NULL,
    view_count integer DEFAULT 0 NOT NULL,
    like_count integer DEFAULT 0 NOT NULL,
    comment_count integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.discussions IS '讨论帖子表（社区）';

COMMENT ON COLUMN public.discussions.id IS '主键';

COMMENT ON COLUMN public.discussions.title IS '帖子标题';

COMMENT ON COLUMN public.discussions.content IS '帖子内容';

COMMENT ON COLUMN public.discussions.author_id IS '作者用户 ID';

COMMENT ON COLUMN public.discussions.view_count IS '浏览量';

COMMENT ON COLUMN public.discussions.like_count IS '点赞数';

COMMENT ON COLUMN public.discussions.comment_count IS '评论数';

COMMENT ON COLUMN public.discussions.created_at IS '创建时间';

COMMENT ON COLUMN public.discussions.updated_at IS '更新时间';

CREATE TABLE public.event_announcements (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    title text NOT NULL,
    content text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.event_announcements IS '赛事公告表';

COMMENT ON COLUMN public.event_announcements.id IS '主键';

COMMENT ON COLUMN public.event_announcements.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_announcements.title IS '公告标题';

COMMENT ON COLUMN public.event_announcements.content IS '公告内容';

COMMENT ON COLUMN public.event_announcements.created_at IS '创建时间';

CREATE TABLE public.event_challenge_solves (
    event_id uuid NOT NULL,
    challenge_id uuid NOT NULL,
    user_id uuid NOT NULL,
    team_id uuid,
    obtained_points double precision DEFAULT 0 NOT NULL,
    bonus_points double precision DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.event_challenge_solves IS '赛事解题记录：赛事内的解题流水（含队伍归属与得分）';

COMMENT ON COLUMN public.event_challenge_solves.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_challenge_solves.challenge_id IS '题目 ID';

COMMENT ON COLUMN public.event_challenge_solves.user_id IS '解题用户 ID';

COMMENT ON COLUMN public.event_challenge_solves.team_id IS '所属队伍 ID（可为空）';

COMMENT ON COLUMN public.event_challenge_solves.obtained_points IS '实际获得分值';

COMMENT ON COLUMN public.event_challenge_solves.bonus_points IS '额外加分（如首破奖励）';

COMMENT ON COLUMN public.event_challenge_solves.created_at IS '解题时间';

CREATE TABLE public.event_challenges (
    event_id uuid NOT NULL,
    challenge_id uuid NOT NULL,
    points double precision DEFAULT 100 NOT NULL,
    hidden boolean DEFAULT true NOT NULL
);

COMMENT ON TABLE public.event_challenges IS '赛事题目表：赛事包含的题目及其分值/可见性';

COMMENT ON COLUMN public.event_challenges.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_challenges.challenge_id IS '题目 ID';

COMMENT ON COLUMN public.event_challenges.points IS '题目分值（默认 100）';

COMMENT ON COLUMN public.event_challenges.hidden IS '是否隐藏';

CREATE TABLE public.event_instances (
    event_id uuid NOT NULL,
    instance_id uuid NOT NULL,
    user_id uuid NOT NULL,
    team_id uuid
);

COMMENT ON TABLE public.event_instances IS '赛事实例表：赛事与实例的关联（先按 instance 查 challenge，实例可共用）';

COMMENT ON COLUMN public.event_instances.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_instances.instance_id IS '实例 ID';

COMMENT ON COLUMN public.event_instances.user_id IS '实例归属用户 ID';

COMMENT ON COLUMN public.event_instances.team_id IS '所属队伍 ID（可为空）';

CREATE TABLE public.event_logs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    user_id uuid,
    team_id uuid,
    ip_address character varying(45),
    type public.event_type DEFAULT 'jeopardy_single'::public.event_type NOT NULL,
    level character varying(20) DEFAULT 'info'::character varying NOT NULL,
    action character varying(50) NOT NULL,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.event_logs IS '赛事日志表：防撞库与安全审计（登录/抓 flag/启动容器等动作）';

COMMENT ON COLUMN public.event_logs.id IS '主键';

COMMENT ON COLUMN public.event_logs.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_logs.user_id IS '操作用户 ID（可为空）';

COMMENT ON COLUMN public.event_logs.team_id IS '操作队伍 ID（可为空）';

COMMENT ON COLUMN public.event_logs.ip_address IS '来源 IP（必须记录，防撞库、防恶意操作）';

COMMENT ON COLUMN public.event_logs.type IS '赛事类型';

COMMENT ON COLUMN public.event_logs.level IS '日志级别（默认 info）';

COMMENT ON COLUMN public.event_logs.action IS '动作类型：login / capture_flag / container_start 等（可过滤）';

COMMENT ON COLUMN public.event_logs.details IS '详细数据（JSON）';

COMMENT ON COLUMN public.event_logs.created_at IS '创建时间';

CREATE TABLE public.event_team_members (
    event_id uuid NOT NULL,
    team_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role public.event_team_member_role DEFAULT 'member'::public.event_team_member_role NOT NULL,
    joined_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.event_team_members IS '赛事队伍成员表：队员与队长关系';

COMMENT ON COLUMN public.event_team_members.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_team_members.team_id IS '队伍 ID';

COMMENT ON COLUMN public.event_team_members.user_id IS '用户 ID';

COMMENT ON COLUMN public.event_team_members.role IS '角色：captain 队长 / member 队员';

COMMENT ON COLUMN public.event_team_members.joined_at IS '加入时间';

CREATE TABLE public.event_teams (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    event_id uuid NOT NULL,
    name text NOT NULL,
    description text,
    points double precision DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    banned boolean DEFAULT false NOT NULL
);

COMMENT ON TABLE public.event_teams IS '赛事队伍表：团队赛的参赛队伍';

COMMENT ON COLUMN public.event_teams.id IS '主键';

COMMENT ON COLUMN public.event_teams.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_teams.name IS '队伍名称（赛事内唯一）';

COMMENT ON COLUMN public.event_teams.description IS '队伍描述';

COMMENT ON COLUMN public.event_teams.points IS '队伍积分';

COMMENT ON COLUMN public.event_teams.created_at IS '创建时间';

COMMENT ON COLUMN public.event_teams.updated_at IS '更新时间';

COMMENT ON COLUMN public.event_teams.banned IS '是否被禁赛';

CREATE TABLE public.event_users (
    event_id uuid NOT NULL,
    user_id uuid NOT NULL,
    points double precision DEFAULT 0 NOT NULL,
    banned boolean DEFAULT false NOT NULL,
    joined_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.event_users IS '赛事参赛用户表：记录个人赛事积分与封禁状态';

COMMENT ON COLUMN public.event_users.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_users.user_id IS '用户 ID';

COMMENT ON COLUMN public.event_users.points IS '个人赛事积分';

COMMENT ON COLUMN public.event_users.banned IS '是否被禁赛';

COMMENT ON COLUMN public.event_users.joined_at IS '加入时间';

CREATE TABLE public.event_writeup (
    event_id uuid NOT NULL,
    user_id uuid NOT NULL,
    team_id uuid,
    file_url text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.event_writeup IS '赛事 WriteUp 提交表（文件形式，存 RustFS 对象存储）';

COMMENT ON COLUMN public.event_writeup.event_id IS '赛事 ID';

COMMENT ON COLUMN public.event_writeup.user_id IS '提交用户 ID';

COMMENT ON COLUMN public.event_writeup.team_id IS '所属队伍 ID（可为空）';

COMMENT ON COLUMN public.event_writeup.file_url IS 'WriteUp 文件地址（RustFS）';

COMMENT ON COLUMN public.event_writeup.created_at IS '提交时间';

CREATE TABLE public.events (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    type public.event_type DEFAULT 'jeopardy_single'::public.event_type NOT NULL,
    title text NOT NULL,
    description text,
    hidden boolean DEFAULT true NOT NULL,
    start_time timestamp with time zone NOT NULL,
    rules text DEFAULT 'do not cheat'::text NOT NULL,
    allow_join boolean DEFAULT false NOT NULL,
    flag_prefix text DEFAULT 'flag'::text,
    end_time timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.events IS '赛事表：Jeopardy（练习/单人/团队）与 AWD 赛事';

COMMENT ON COLUMN public.events.id IS '主键';

COMMENT ON COLUMN public.events.type IS '赛事类型：jeopardy_practice / jeopardy_single / jeopardy_team / awd_team';

COMMENT ON COLUMN public.events.title IS '赛事标题';

COMMENT ON COLUMN public.events.description IS '赛事描述';

COMMENT ON COLUMN public.events.hidden IS '是否隐藏';

COMMENT ON COLUMN public.events.start_time IS '开始时间';

COMMENT ON COLUMN public.events.rules IS '比赛规则说明';

COMMENT ON COLUMN public.events.allow_join IS '是否允许加入';

COMMENT ON COLUMN public.events.flag_prefix IS 'flag 前缀（默认 flag）';

COMMENT ON COLUMN public.events.end_time IS '结束时间';

COMMENT ON COLUMN public.events.created_at IS '创建时间';

COMMENT ON COLUMN public.events.updated_at IS '更新时间';

CREATE TABLE public.gameboxes (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name text NOT NULL,
    safe_name text NOT NULL,
    category text DEFAULT 'other'::text NOT NULL,
    description text DEFAULT 'no description'::text NOT NULL,
    hidden boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    source_toml text,
    image_ref text,
    image_digest text,
    username text,
    default_cpu_millis bigint,
    default_memory_bytes bigint,
    default_pids_limit bigint,
    healthcheck_json jsonb,
    judge_script_name text,
    judge_script_content text,
    judge_args_json jsonb,
    default_judge_timeout_secs integer,
    default_judge_retry_interval_secs integer
);

COMMENT ON TABLE public.gameboxes IS 'AWD 靶机模板库：赛事专用靶机定义（部署镜像与计分参数）';

COMMENT ON COLUMN public.gameboxes.id IS '主键';

COMMENT ON COLUMN public.gameboxes.name IS '靶机名称（唯一）';

COMMENT ON COLUMN public.gameboxes.safe_name IS '安全名称（URL/路径友好，唯一）';

COMMENT ON COLUMN public.gameboxes.category IS '分类（默认 other）';

COMMENT ON COLUMN public.gameboxes.description IS '靶机描述';

COMMENT ON COLUMN public.gameboxes.hidden IS '是否隐藏';

COMMENT ON COLUMN public.gameboxes.created_at IS '创建时间';

COMMENT ON COLUMN public.gameboxes.updated_at IS '更新时间';

COMMENT ON COLUMN public.gameboxes.source_toml IS 'GameBox 配置源 TOML（单版本，编辑直接覆盖，同 challenges.toml_str）';

COMMENT ON COLUMN public.gameboxes.image_ref IS '镜像引用（如 registry/easy-web:v1）';

COMMENT ON COLUMN public.gameboxes.image_digest IS '镜像 digest 钉住（生产建议 pin，格式 sha256:…）';

COMMENT ON COLUMN public.gameboxes.username IS 'GameBox 内 SSH 用户名（默认 ctf）';

COMMENT ON COLUMN public.gameboxes.default_cpu_millis IS '默认 CPU 限制（毫核）；赛事选择时复制为 awd_event_gameboxes.cpu_millis 可再覆盖';

COMMENT ON COLUMN public.gameboxes.default_memory_bytes IS '默认内存限制（字节）；赛事选择时可覆盖';

COMMENT ON COLUMN public.gameboxes.default_pids_limit IS '默认 pids 限制；赛事选择时可覆盖';

COMMENT ON COLUMN public.gameboxes.healthcheck_json IS '默认健康检查配置（JSON）；赛事选择时可覆盖';

COMMENT ON COLUMN public.gameboxes.judge_script_name IS '判题脚本名（可选）';

COMMENT ON COLUMN public.gameboxes.judge_script_content IS '判题脚本内容（可选）';

COMMENT ON COLUMN public.gameboxes.judge_args_json IS '判题脚本参数（JSON，可选）';

COMMENT ON COLUMN public.gameboxes.default_judge_timeout_secs IS '默认判题超时秒数（可选）；赛事选择时可覆盖';

COMMENT ON COLUMN public.gameboxes.default_judge_retry_interval_secs IS '默认判题重试间隔秒数（可选）；赛事选择时可覆盖';

CREATE TABLE public.instances (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    status public.instance_status DEFAULT 'pending'::public.instance_status NOT NULL,
    ref text DEFAULT 'JeopardyPractice'::text NOT NULL,
    flag text NOT NULL,
    content text,
    challenge_id uuid,
    user_id uuid NOT NULL,
    identifier text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    destroy_at timestamp with time zone NOT NULL
);

COMMENT ON TABLE public.instances IS '题目实例表：动态创建的容器实例（Jeopardy 练习/赛事共用）';

COMMENT ON COLUMN public.instances.id IS '主键';

COMMENT ON COLUMN public.instances.status IS '状态：pending / running / completed / failed';

COMMENT ON COLUMN public.instances.ref IS '实例模式标识（如 JeopardyPractice）';

COMMENT ON COLUMN public.instances.flag IS '实例内动态生成的 flag';

COMMENT ON COLUMN public.instances.content IS '实例内容/提示（可为空）';

COMMENT ON COLUMN public.instances.challenge_id IS '关联题目 ID（可为空）';

COMMENT ON COLUMN public.instances.user_id IS '创建/归属用户 ID';

COMMENT ON COLUMN public.instances.identifier IS '实例唯一标识（如容器名/ID）';

COMMENT ON COLUMN public.instances.created_at IS '创建时间';

COMMENT ON COLUMN public.instances.updated_at IS '更新时间';

COMMENT ON COLUMN public.instances.destroy_at IS '自动销毁时间';

CREATE TABLE public.logs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    user_id uuid,
    superadmin_id uuid,
    ip_address character varying(45),
    category character varying(30) NOT NULL,
    action character varying(50) NOT NULL,
    level character varying(10) DEFAULT 'info'::character varying NOT NULL,
    message text NOT NULL,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.logs IS '系统审计日志：管理后台操作审计（登录、删文件、起容器、改密码等）';

COMMENT ON COLUMN public.logs.id IS '主键';

COMMENT ON COLUMN public.logs.user_id IS '操作用户 ID（可为空）';

COMMENT ON COLUMN public.logs.superadmin_id IS '操作超级管理员 ID（哪个超管干的）';

COMMENT ON COLUMN public.logs.ip_address IS '来源 IP（必须记录，防撞库、防恶意操作）';

COMMENT ON COLUMN public.logs.category IS '审计分类：AUTH / SYSTEM / SERVICE / ADMIN_ACTION / WEAPONS';

COMMENT ON COLUMN public.logs.action IS '动作描述：delete_file / start_container / update_password 等';

COMMENT ON COLUMN public.logs.level IS '级别：debug / info / warn / error / fatal';

COMMENT ON COLUMN public.logs.message IS '人类可读简述，如"管理员 A 删除了用户 B"';

COMMENT ON COLUMN public.logs.details IS '差异化数据（JSON）';

COMMENT ON COLUMN public.logs.created_at IS '创建时间';

CREATE TABLE public.scheduled_tasks (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    group_id uuid,
    task_name character varying(200) NOT NULL,
    description text,
    task_key character varying(100) NOT NULL,
    trigger_type character varying(50) NOT NULL,
    status character varying(50) DEFAULT 'pending'::character varying NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    protected boolean DEFAULT true NOT NULL,
    cron_expr character varying(100),
    execute_at timestamp with time zone,
    expires_at timestamp with time zone,
    payload jsonb,
    error_msg text,
    last_run_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    attempt_count integer DEFAULT 0 NOT NULL,
    max_attempts integer DEFAULT 3 NOT NULL,
    timeout_secs integer,
    last_error text,
    locked_at timestamp with time zone,
    heartbeat_at timestamp with time zone
);

COMMENT ON TABLE public.scheduled_tasks IS '调度任务表：startup/once/cron 后台任务，由调度器引擎轮询执行';

COMMENT ON COLUMN public.scheduled_tasks.id IS '主键';

COMMENT ON COLUMN public.scheduled_tasks.group_id IS '业务组：比赛 ID 或靶机 ID，用于一键销毁';

COMMENT ON COLUMN public.scheduled_tasks.task_name IS '任务名称，如"第3轮-Flag刷新-选手A"';

COMMENT ON COLUMN public.scheduled_tasks.description IS '任务描述';

COMMENT ON COLUMN public.scheduled_tasks.task_key IS '路由键：GAME_START / LAB_CLOSE / CHECK 等，决定执行哪个 handler';

COMMENT ON COLUMN public.scheduled_tasks.trigger_type IS '触发类型：startup / once / cron';

COMMENT ON COLUMN public.scheduled_tasks.status IS '状态：pending / running / completed / failed / paused';

COMMENT ON COLUMN public.scheduled_tasks.enabled IS '是否启用';

COMMENT ON COLUMN public.scheduled_tasks.protected IS '是否受保护（普通接口不可删除/修改）';

COMMENT ON COLUMN public.scheduled_tasks.cron_expr IS 'cron 表达式，如 */10 * * * *';

COMMENT ON COLUMN public.scheduled_tasks.execute_at IS '计划执行时间';

COMMENT ON COLUMN public.scheduled_tasks.expires_at IS '过期时间：超过该时间不再补执行';

COMMENT ON COLUMN public.scheduled_tasks.payload IS '业务参数（强类型 JSON）';

COMMENT ON COLUMN public.scheduled_tasks.error_msg IS '最近一次执行错误信息';

COMMENT ON COLUMN public.scheduled_tasks.last_run_at IS '最近一次执行时间';

COMMENT ON COLUMN public.scheduled_tasks.created_at IS '创建时间';

COMMENT ON COLUMN public.scheduled_tasks.updated_at IS '更新时间';

COMMENT ON COLUMN public.scheduled_tasks.attempt_count IS '已尝试执行次数';

COMMENT ON COLUMN public.scheduled_tasks.max_attempts IS '最大重试次数，超过则判定永久失败';

COMMENT ON COLUMN public.scheduled_tasks.timeout_secs IS '单次执行超时时间（秒）';

COMMENT ON COLUMN public.scheduled_tasks.last_error IS '最近一次失败信息（重试诊断用）';

COMMENT ON COLUMN public.scheduled_tasks.locked_at IS '工作进程执行锁时间';

COMMENT ON COLUMN public.scheduled_tasks.heartbeat_at IS '工作进程心跳时间（执行期间定期更新）';

CREATE TABLE public.settings (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    type public.setting_value_type DEFAULT 'string'::public.setting_value_type NOT NULL,
    description text NOT NULL,
    protected boolean DEFAULT true NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.settings IS '动态设置表：管理员可编辑的键值配置（进程启动时从 TOML 播种默认值，之后可独立修改）';

COMMENT ON COLUMN public.settings.id IS '主键';

COMMENT ON COLUMN public.settings.key IS '设置键名（唯一）';

COMMENT ON COLUMN public.settings.value IS '设置值（统一以字符串存储）';

COMMENT ON COLUMN public.settings.type IS '值类型：string / integer / boolean / float';

COMMENT ON COLUMN public.settings.description IS '设置说明';

COMMENT ON COLUMN public.settings.protected IS '受保护标志：受保护设置不允许普通修改/删除';

COMMENT ON COLUMN public.settings.updated_at IS '更新时间';

CREATE TABLE public.super_admin (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    username text NOT NULL,
    password text NOT NULL,
    email text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.super_admin IS '超级管理员表：平台运营账号';

COMMENT ON COLUMN public.super_admin.id IS '主键';

COMMENT ON COLUMN public.super_admin.username IS '用户名（唯一）';

COMMENT ON COLUMN public.super_admin.password IS '密码哈希（argon2id）';

COMMENT ON COLUMN public.super_admin.email IS '邮箱';

COMMENT ON COLUMN public.super_admin.created_at IS '创建时间';

COMMENT ON COLUMN public.super_admin.updated_at IS '更新时间';

CREATE TABLE public.users (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    username text NOT NULL,
    nickname text NOT NULL,
    password text NOT NULL,
    email text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    avatar text
);

COMMENT ON TABLE public.users IS '用户表：参赛选手账号';

COMMENT ON COLUMN public.users.id IS '主键';

COMMENT ON COLUMN public.users.username IS '用户名（登录用，唯一）';

COMMENT ON COLUMN public.users.nickname IS '昵称（展示用，唯一）';

COMMENT ON COLUMN public.users.password IS '密码哈希（argon2id）';

COMMENT ON COLUMN public.users.email IS '邮箱';

COMMENT ON COLUMN public.users.created_at IS '创建时间';

COMMENT ON COLUMN public.users.updated_at IS '更新时间';

COMMENT ON COLUMN public.users.avatar IS '用户头像 URL（默认 NULL）';

CREATE TABLE public.weapons (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name text NOT NULL,
    category text DEFAULT 'other'::text NOT NULL,
    description text,
    has_file boolean DEFAULT false NOT NULL,
    download_count bigint DEFAULT 0 NOT NULL,
    file_url text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.weapons IS '武器库：平台提供的攻击工具/装备（可附带文件供下载）';

COMMENT ON COLUMN public.weapons.id IS '主键';

COMMENT ON COLUMN public.weapons.name IS '武器名称';

COMMENT ON COLUMN public.weapons.category IS '分类（默认 other）';

COMMENT ON COLUMN public.weapons.description IS '武器描述';

COMMENT ON COLUMN public.weapons.has_file IS '是否附带文件';

COMMENT ON COLUMN public.weapons.download_count IS '下载次数';

COMMENT ON COLUMN public.weapons.file_url IS '文件下载地址';

COMMENT ON COLUMN public.weapons.created_at IS '创建时间';

COMMENT ON COLUMN public.weapons.updated_at IS '更新时间';


-- ================================================================================
-- Primary Keys（主键约束）
-- ================================================================================

ALTER TABLE ONLY public.announcements
    ADD CONSTRAINT announcements_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_event_gameboxes
    ADD CONSTRAINT awd_event_gameboxes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_event_networks
    ADD CONSTRAINT awd_event_networks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_events
    ADD CONSTRAINT awd_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_flag_issues
    ADD CONSTRAINT awd_flag_issues_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_internal_token_rotations
    ADD CONSTRAINT awd_internal_token_rotations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_judge_batches
    ADD CONSTRAINT awd_judge_batches_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_network_allocations
    ADD CONSTRAINT awd_network_allocations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_network_settings
    ADD CONSTRAINT awd_network_settings_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_orphan_resources
    ADD CONSTRAINT awd_orphan_resources_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_precheck_runs
    ADD CONSTRAINT awd_precheck_runs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_reset_records
    ADD CONSTRAINT awd_reset_records_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_rounds
    ADD CONSTRAINT awd_rounds_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_runtime_resources
    ADD CONSTRAINT awd_runtime_resources_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_team_networks
    ADD CONSTRAINT awd_team_networks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.challenge_set_items
    ADD CONSTRAINT challenge_set_items_pkey PRIMARY KEY (set_id, challenge_id);

ALTER TABLE ONLY public.challenge_sets
    ADD CONSTRAINT challenge_sets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.challenge_solves
    ADD CONSTRAINT challenge_solves_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.challenge_writeup
    ADD CONSTRAINT challenge_writeup_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.challenges
    ADD CONSTRAINT challenges_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.discussion_comments
    ADD CONSTRAINT discussion_comments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.discussion_likes
    ADD CONSTRAINT discussion_likes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.discussions
    ADD CONSTRAINT discussions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.event_announcements
    ADD CONSTRAINT event_announcements_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.event_challenge_solves
    ADD CONSTRAINT event_challenge_solves_pkey PRIMARY KEY (event_id, challenge_id, user_id);

ALTER TABLE ONLY public.event_challenges
    ADD CONSTRAINT event_challenges_pkey PRIMARY KEY (event_id, challenge_id);

ALTER TABLE ONLY public.event_instances
    ADD CONSTRAINT event_instances_pkey PRIMARY KEY (event_id, instance_id);

ALTER TABLE ONLY public.event_logs
    ADD CONSTRAINT event_logs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.event_team_members
    ADD CONSTRAINT event_team_members_pkey PRIMARY KEY (event_id, team_id, user_id);

ALTER TABLE ONLY public.event_teams
    ADD CONSTRAINT event_teams_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.event_users
    ADD CONSTRAINT event_users_pkey PRIMARY KEY (event_id, user_id);

ALTER TABLE ONLY public.event_writeup
    ADD CONSTRAINT event_writeup_pkey PRIMARY KEY (event_id, user_id);

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.gameboxes
    ADD CONSTRAINT gameboxes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.instances
    ADD CONSTRAINT instances_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.logs
    ADD CONSTRAINT logs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.settings
    ADD CONSTRAINT settings_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.super_admin
    ADD CONSTRAINT super_admin_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.weapons
    ADD CONSTRAINT weapons_pkey PRIMARY KEY (id);


-- ================================================================================
-- Unique Constraints（唯一约束）
-- ================================================================================

ALTER TABLE ONLY public.awd_event_gameboxes
    ADD CONSTRAINT awd_event_gameboxes_event_gamebox_key UNIQUE (event_id, gamebox_id);

ALTER TABLE ONLY public.awd_event_gameboxes
    ADD CONSTRAINT awd_event_gameboxes_event_offset_key UNIQUE (event_id, host_offset);

ALTER TABLE ONLY public.awd_event_networks
    ADD CONSTRAINT awd_event_networks_docker_network_name_key UNIQUE (docker_network_name);

ALTER TABLE ONLY public.awd_event_networks
    ADD CONSTRAINT awd_event_networks_event_id_key UNIQUE (event_id);

ALTER TABLE ONLY public.awd_event_networks
    ADD CONSTRAINT awd_event_networks_wireguard_interface_name_key UNIQUE (wireguard_interface_name);

ALTER TABLE ONLY public.awd_event_networks
    ADD CONSTRAINT awd_event_networks_wireguard_listen_port_key UNIQUE (wireguard_listen_port);

ALTER TABLE ONLY public.awd_events
    ADD CONSTRAINT awd_events_event_id_key UNIQUE (event_id);

ALTER TABLE ONLY public.awd_flag_issues
    ADD CONSTRAINT awd_flag_issues_event_id_round_id_flag_hash_key UNIQUE (event_id, round_id, flag_hash);

ALTER TABLE ONLY public.awd_flag_issues
    ADD CONSTRAINT awd_flag_issues_event_id_round_id_gamebox_instance_id_key UNIQUE (event_id, round_id, gamebox_instance_id);

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_event_id_round_id_attacker_team_id_gam_key UNIQUE (event_id, round_id, attacker_team_id, gamebox_instance_id);

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_container_name_key UNIQUE (container_name);

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_event_gamebox_team_key UNIQUE (event_id, event_gamebox_id, team_id);

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_event_id_gamebox_ip_key UNIQUE (event_id, gamebox_ip);

ALTER TABLE ONLY public.awd_rounds
    ADD CONSTRAINT awd_rounds_event_id_round_number_key UNIQUE (event_id, round_number);

ALTER TABLE ONLY public.awd_runtime_resources
    ADD CONSTRAINT awd_runtime_resources_event_id_resource_type_resource_id_key UNIQUE (event_id, resource_type, resource_id);

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_idempotency_key_key UNIQUE (idempotency_key);

ALTER TABLE ONLY public.awd_team_networks
    ADD CONSTRAINT awd_team_networks_event_id_gamebox_subnet_key UNIQUE (event_id, gamebox_subnet);

ALTER TABLE ONLY public.awd_team_networks
    ADD CONSTRAINT awd_team_networks_event_id_team_id_key UNIQUE (event_id, team_id);

ALTER TABLE ONLY public.awd_team_networks
    ADD CONSTRAINT awd_team_networks_event_id_wireguard_subnet_key UNIQUE (event_id, wireguard_subnet);

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_event_id_assigned_ip_key UNIQUE (event_id, assigned_ip);

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_event_id_user_id_key UNIQUE (event_id, user_id);

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_public_key_key UNIQUE (public_key);

ALTER TABLE ONLY public.challenges
    ADD CONSTRAINT challenges_name_key UNIQUE (name);

ALTER TABLE ONLY public.challenges
    ADD CONSTRAINT challenges_safe_name_key UNIQUE (safe_name);

ALTER TABLE ONLY public.discussion_likes
    ADD CONSTRAINT discussion_likes_discussion_id_user_id_key UNIQUE (discussion_id, user_id);

ALTER TABLE ONLY public.event_teams
    ADD CONSTRAINT event_teams_event_id_name_key UNIQUE (event_id, name);

ALTER TABLE ONLY public.gameboxes
    ADD CONSTRAINT gameboxes_safe_name_key UNIQUE (safe_name);

ALTER TABLE ONLY public.settings
    ADD CONSTRAINT settings_key_key UNIQUE (key);

ALTER TABLE ONLY public.super_admin
    ADD CONSTRAINT super_admin_username_key UNIQUE (username);

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_nickname_key UNIQUE (nickname);

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


-- ================================================================================
-- Foreign Keys（外键约束）
-- ================================================================================

ALTER TABLE ONLY public.announcements
    ADD CONSTRAINT announcements_publisher_id_fkey FOREIGN KEY (publisher_id) REFERENCES public.super_admin(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_event_gameboxes
    ADD CONSTRAINT awd_event_gameboxes_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_event_gameboxes
    ADD CONSTRAINT awd_event_gameboxes_gamebox_id_fkey FOREIGN KEY (gamebox_id) REFERENCES public.gameboxes(id) ON DELETE RESTRICT;

ALTER TABLE ONLY public.awd_event_networks
    ADD CONSTRAINT awd_event_networks_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_events
    ADD CONSTRAINT awd_events_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_issues
    ADD CONSTRAINT awd_flag_issues_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_issues
    ADD CONSTRAINT awd_flag_issues_gamebox_instance_id_fkey FOREIGN KEY (gamebox_instance_id) REFERENCES public.awd_gamebox_instances(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_issues
    ADD CONSTRAINT awd_flag_issues_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.awd_rounds(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_attacker_team_id_fkey FOREIGN KEY (attacker_team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_flag_issue_id_fkey FOREIGN KEY (flag_issue_id) REFERENCES public.awd_flag_issues(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_gamebox_instance_id_fkey FOREIGN KEY (gamebox_instance_id) REFERENCES public.awd_gamebox_instances(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.awd_rounds(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_submitted_by_user_id_fkey FOREIGN KEY (submitted_by_user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_flag_submissions
    ADD CONSTRAINT awd_flag_submissions_victim_team_id_fkey FOREIGN KEY (victim_team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_event_gamebox_fk FOREIGN KEY (event_gamebox_id) REFERENCES public.awd_event_gameboxes(id) ON DELETE RESTRICT;

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_gamebox_instances
    ADD CONSTRAINT awd_gamebox_instances_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_internal_token_rotations
    ADD CONSTRAINT awd_internal_token_rotations_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_internal_token_rotations
    ADD CONSTRAINT awd_internal_token_rotations_rotated_by_fkey FOREIGN KEY (rotated_by) REFERENCES public.super_admin(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_judge_batches
    ADD CONSTRAINT awd_judge_batches_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_judge_batches
    ADD CONSTRAINT awd_judge_batches_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.awd_rounds(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_batch_id_fkey FOREIGN KEY (batch_id) REFERENCES public.awd_judge_batches(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_event_gamebox_fk FOREIGN KEY (event_gamebox_id) REFERENCES public.awd_event_gameboxes(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_gamebox_instance_id_fkey FOREIGN KEY (gamebox_instance_id) REFERENCES public.awd_gamebox_instances(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.awd_rounds(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_judge_tasks
    ADD CONSTRAINT awd_judge_tasks_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_network_allocations
    ADD CONSTRAINT awd_network_allocations_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_orphan_resources
    ADD CONSTRAINT awd_orphan_resources_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_precheck_runs
    ADD CONSTRAINT awd_precheck_runs_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_reset_records
    ADD CONSTRAINT awd_reset_records_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_reset_records
    ADD CONSTRAINT awd_reset_records_gamebox_instance_id_fkey FOREIGN KEY (gamebox_instance_id) REFERENCES public.awd_gamebox_instances(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_reset_records
    ADD CONSTRAINT awd_reset_records_requested_by_fkey FOREIGN KEY (requested_by) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_reset_records
    ADD CONSTRAINT awd_reset_records_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.awd_rounds(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_reset_records
    ADD CONSTRAINT awd_reset_records_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_rounds
    ADD CONSTRAINT awd_rounds_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_runtime_resources
    ADD CONSTRAINT awd_runtime_resources_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.super_admin(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_event_gamebox_fk FOREIGN KEY (event_gamebox_id) REFERENCES public.awd_event_gameboxes(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_gamebox_instance_id_fkey FOREIGN KEY (gamebox_instance_id) REFERENCES public.awd_gamebox_instances(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_related_team_id_fkey FOREIGN KEY (related_team_id) REFERENCES public.event_teams(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.awd_rounds(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_score_events
    ADD CONSTRAINT awd_score_events_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_banned_by_fkey FOREIGN KEY (banned_by) REFERENCES public.super_admin(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_effective_round_id_fkey FOREIGN KEY (effective_round_id) REFERENCES public.awd_rounds(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_unban_effective_round_id_fkey FOREIGN KEY (unban_effective_round_id) REFERENCES public.awd_rounds(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_team_bans
    ADD CONSTRAINT awd_team_bans_unbanned_by_fkey FOREIGN KEY (unbanned_by) REFERENCES public.super_admin(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.awd_team_networks
    ADD CONSTRAINT awd_team_networks_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_team_networks
    ADD CONSTRAINT awd_team_networks_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.awd_wireguard_peers
    ADD CONSTRAINT awd_wireguard_peers_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_set_items
    ADD CONSTRAINT challenge_set_items_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_set_items
    ADD CONSTRAINT challenge_set_items_set_id_fkey FOREIGN KEY (set_id) REFERENCES public.challenge_sets(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_solves
    ADD CONSTRAINT challenge_solves_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_solves
    ADD CONSTRAINT challenge_solves_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_solves
    ADD CONSTRAINT challenge_solves_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_writeup
    ADD CONSTRAINT challenge_writeup_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.challenge_writeup
    ADD CONSTRAINT challenge_writeup_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.discussion_comments
    ADD CONSTRAINT discussion_comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.discussion_comments
    ADD CONSTRAINT discussion_comments_discussion_id_fkey FOREIGN KEY (discussion_id) REFERENCES public.discussions(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.discussion_comments
    ADD CONSTRAINT discussion_comments_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES public.discussion_comments(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.discussion_likes
    ADD CONSTRAINT discussion_likes_discussion_id_fkey FOREIGN KEY (discussion_id) REFERENCES public.discussions(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.discussion_likes
    ADD CONSTRAINT discussion_likes_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.discussions
    ADD CONSTRAINT discussions_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_announcements
    ADD CONSTRAINT event_announcements_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_challenge_solves
    ADD CONSTRAINT event_challenge_solves_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_challenge_solves
    ADD CONSTRAINT event_challenge_solves_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_challenge_solves
    ADD CONSTRAINT event_challenge_solves_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_challenge_solves
    ADD CONSTRAINT event_challenge_solves_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_challenges
    ADD CONSTRAINT event_challenges_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_challenges
    ADD CONSTRAINT event_challenges_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_instances
    ADD CONSTRAINT event_instances_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_instances
    ADD CONSTRAINT event_instances_instance_id_fkey FOREIGN KEY (instance_id) REFERENCES public.instances(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_instances
    ADD CONSTRAINT event_instances_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_instances
    ADD CONSTRAINT event_instances_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_logs
    ADD CONSTRAINT event_logs_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_logs
    ADD CONSTRAINT event_logs_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.event_logs
    ADD CONSTRAINT event_logs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.event_team_members
    ADD CONSTRAINT event_team_members_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_team_members
    ADD CONSTRAINT event_team_members_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_team_members
    ADD CONSTRAINT event_team_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_teams
    ADD CONSTRAINT event_teams_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_users
    ADD CONSTRAINT event_users_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_users
    ADD CONSTRAINT event_users_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_writeup
    ADD CONSTRAINT event_writeup_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_writeup
    ADD CONSTRAINT event_writeup_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.event_teams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.event_writeup
    ADD CONSTRAINT event_writeup_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.instances
    ADD CONSTRAINT instances_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.challenges(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.instances
    ADD CONSTRAINT instances_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.logs
    ADD CONSTRAINT logs_superadmin_id_fkey FOREIGN KEY (superadmin_id) REFERENCES public.super_admin(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.logs
    ADD CONSTRAINT logs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;


-- ================================================================================
-- Indexes（索引）
-- ================================================================================

CREATE INDEX idx_awd_event_networks_event_id ON public.awd_event_networks USING btree (event_id);

CREATE INDEX idx_awd_events_event_id ON public.awd_events USING btree (event_id);

CREATE INDEX idx_awd_events_phase ON public.awd_events USING btree (phase);

CREATE INDEX idx_awd_events_status ON public.awd_events USING btree (status);

CREATE INDEX idx_awd_flag_issues_event_round ON public.awd_flag_issues USING btree (event_id, round_id);

CREATE INDEX idx_awd_flag_issues_hash ON public.awd_flag_issues USING btree (flag_hash);

CREATE INDEX idx_awd_flag_issues_instance ON public.awd_flag_issues USING btree (gamebox_instance_id);

CREATE INDEX idx_awd_flag_submissions_attacker ON public.awd_flag_submissions USING btree (attacker_team_id);

CREATE INDEX idx_awd_flag_submissions_event_round ON public.awd_flag_submissions USING btree (event_id, round_id);

CREATE INDEX idx_awd_flag_submissions_user ON public.awd_flag_submissions USING btree (submitted_by_user_id);

CREATE INDEX idx_awd_flag_submissions_victim ON public.awd_flag_submissions USING btree (victim_team_id);

CREATE INDEX idx_awd_gamebox_instances_event ON public.awd_gamebox_instances USING btree (event_id);

CREATE INDEX idx_awd_gamebox_instances_status ON public.awd_gamebox_instances USING btree (status);

CREATE INDEX idx_awd_gamebox_instances_team ON public.awd_gamebox_instances USING btree (team_id);

CREATE INDEX idx_awd_judge_batches_event ON public.awd_judge_batches USING btree (event_id);

CREATE INDEX idx_awd_judge_batches_round ON public.awd_judge_batches USING btree (round_id);

CREATE INDEX idx_awd_judge_tasks_batch ON public.awd_judge_tasks USING btree (batch_id);

CREATE INDEX idx_awd_judge_tasks_callback ON public.awd_judge_tasks USING btree (callback_idempotency_key);

CREATE INDEX idx_awd_judge_tasks_deadline ON public.awd_judge_tasks USING btree (deadline_at);

CREATE INDEX idx_awd_judge_tasks_event_round ON public.awd_judge_tasks USING btree (event_id, round_id);

CREATE INDEX idx_awd_judge_tasks_status ON public.awd_judge_tasks USING btree (status);

CREATE INDEX idx_awd_network_allocations_active ON public.awd_network_allocations USING btree (kind, released_at);

CREATE INDEX idx_awd_network_allocations_cidr ON public.awd_network_allocations USING btree (cidr);

CREATE INDEX idx_awd_network_allocations_event ON public.awd_network_allocations USING btree (event_id);

CREATE INDEX idx_awd_precheck_runs_event ON public.awd_precheck_runs USING btree (event_id);

CREATE INDEX idx_awd_reset_records_event ON public.awd_reset_records USING btree (event_id);

CREATE INDEX idx_awd_reset_records_instance ON public.awd_reset_records USING btree (gamebox_instance_id);

CREATE INDEX idx_awd_rounds_event ON public.awd_rounds USING btree (event_id);

CREATE UNIQUE INDEX idx_awd_rounds_one_active ON public.awd_rounds USING btree (event_id, status) WHERE (status = ANY (ARRAY['active'::public.round_status, 'grace'::public.round_status, 'paused'::public.round_status]));

CREATE INDEX idx_awd_rounds_status ON public.awd_rounds USING btree (status);

CREATE INDEX idx_awd_runtime_resources_event ON public.awd_runtime_resources USING btree (event_id);

CREATE INDEX idx_awd_score_events_event ON public.awd_score_events USING btree (event_id);

CREATE INDEX idx_awd_score_events_idempotency ON public.awd_score_events USING btree (idempotency_key);

CREATE INDEX idx_awd_score_events_team ON public.awd_score_events USING btree (team_id);

CREATE INDEX idx_awd_score_events_type ON public.awd_score_events USING btree (event_type);

CREATE INDEX idx_awd_team_bans_event ON public.awd_team_bans USING btree (event_id);

CREATE UNIQUE INDEX idx_awd_team_bans_one_active ON public.awd_team_bans USING btree (event_id, team_id) WHERE (status = 'active'::public.ban_status);

CREATE INDEX idx_awd_team_bans_team ON public.awd_team_bans USING btree (team_id);

CREATE INDEX idx_awd_team_networks_event ON public.awd_team_networks USING btree (event_id);

CREATE INDEX idx_awd_team_networks_team ON public.awd_team_networks USING btree (team_id);

CREATE INDEX idx_awd_wg_peers_event ON public.awd_wireguard_peers USING btree (event_id);

CREATE INDEX idx_awd_wg_peers_status ON public.awd_wireguard_peers USING btree (status);

CREATE INDEX idx_awd_wg_peers_team ON public.awd_wireguard_peers USING btree (team_id);

CREATE INDEX idx_awd_wg_peers_user ON public.awd_wireguard_peers USING btree (user_id);

CREATE INDEX idx_discussion_comments_author ON public.discussion_comments USING btree (author_id);

CREATE INDEX idx_discussion_comments_discussion ON public.discussion_comments USING btree (discussion_id);

CREATE INDEX idx_discussion_likes_discussion ON public.discussion_likes USING btree (discussion_id);

CREATE INDEX idx_discussions_author ON public.discussions USING btree (author_id);

CREATE INDEX idx_discussions_created ON public.discussions USING btree (created_at DESC);

CREATE INDEX idx_event_instances_event_id ON public.event_instances USING btree (event_id);

CREATE INDEX idx_event_instances_instance_id ON public.event_instances USING btree (instance_id);

CREATE INDEX idx_event_logs_action ON public.event_logs USING btree (action);

CREATE INDEX idx_event_logs_event_id ON public.event_logs USING btree (event_id);

CREATE INDEX idx_event_logs_ip_address ON public.event_logs USING btree (ip_address);

CREATE INDEX idx_event_team_challenge ON public.event_instances USING btree (event_id, team_id);

CREATE INDEX idx_event_team_members_team_id ON public.event_team_members USING btree (team_id);

CREATE INDEX idx_event_team_members_user_id ON public.event_team_members USING btree (user_id);

CREATE INDEX idx_event_teams_event_id ON public.event_teams USING btree (event_id);

CREATE INDEX idx_event_user_challenge ON public.event_instances USING btree (event_id, user_id);

CREATE INDEX idx_event_users_event_id ON public.event_users USING btree (event_id);

CREATE INDEX idx_event_users_user_id ON public.event_users USING btree (user_id);

CREATE INDEX idx_events_end_time ON public.events USING btree (end_time);

CREATE INDEX idx_events_start_time ON public.events USING btree (start_time);

CREATE INDEX idx_events_type ON public.events USING btree (type);

CREATE INDEX idx_instances_challenge_id ON public.instances USING btree (challenge_id);

CREATE INDEX idx_instances_status ON public.instances USING btree (status);

CREATE INDEX idx_instances_user_id ON public.instances USING btree (user_id);

CREATE UNIQUE INDEX idx_scheduled_tasks_awd_event_active_unique ON public.scheduled_tasks USING btree (group_id, task_key) WHERE group_id IS NOT NULL AND task_key IN ('awd.event.start', 'awd.event.auto_precheck') AND status IN ('pending', 'running');

COMMENT ON INDEX public.idx_scheduled_tasks_awd_event_active_unique IS '每个 AWD 赛事的自动预检/定时开赛任务最多存在一个 pending 或 running 实例，防止并发重复执行';

CREATE INDEX idx_scheduled_tasks_group ON public.scheduled_tasks USING btree (group_id);

CREATE INDEX idx_scheduled_tasks_poll ON public.scheduled_tasks USING btree (status, execute_at) WHERE ((status)::text = 'pending'::text);

CREATE INDEX idx_sys_logs_category_action ON public.logs USING btree (category, action);

CREATE INDEX idx_sys_logs_created_at ON public.logs USING btree (created_at DESC);

CREATE INDEX idx_sys_logs_details ON public.logs USING gin (details);

CREATE INDEX idx_sys_logs_user_op ON public.logs USING btree (user_id, superadmin_id);

CREATE UNIQUE INDEX idx_users_email ON public.users USING btree (email);

CREATE UNIQUE INDEX idx_users_username ON public.users USING btree (username);


-- ================================================================================
-- Triggers（触发器）
-- ================================================================================

CREATE TRIGGER trg_announcements_updated_at BEFORE UPDATE ON public.announcements FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_challenges_updated_at BEFORE UPDATE ON public.challenges FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_discussion_comments_updated_at BEFORE UPDATE ON public.discussion_comments FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_discussions_updated_at BEFORE UPDATE ON public.discussions FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_event_teams_updated_at BEFORE UPDATE ON public.event_teams FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_events_updated_at BEFORE UPDATE ON public.events FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_gameboxes_updated_at BEFORE UPDATE ON public.gameboxes FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_instances_updated_at BEFORE UPDATE ON public.instances FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_scheduled_tasks_updated_at BEFORE UPDATE ON public.scheduled_tasks FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_settings_updated_at BEFORE UPDATE ON public.settings FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_super_admin_updated_at BEFORE UPDATE ON public.super_admin FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_users_updated_at BEFORE UPDATE ON public.users FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();

CREATE TRIGGER trg_weapons_updated_at BEFORE UPDATE ON public.weapons FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


