-- ================================================================================
-- Migration: 20260813000000-awdp-practice-judge
-- AWDP 练习 Judge 服务：
-- 1) awdp_practice_judge_settings —— AWDPlusPractice 虚拟赛事的 Judge 配置（单行）
-- 2) awdp_judge_results —— 练习 Judge 检查结果（exploit 检查 / flag curl 验证）
-- 3) seed AWDPlusPractice 默认配置行
-- 练习实例统一加入专用 docker 子网 fctf-awdp-practice；JudgeServer 容器
-- 部署在该子网内，按容器内网 IP 直达全部练习 GameBox。
-- ================================================================================

-- ── 1) 练习 Judge 配置（event 维度单行）─────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.awdp_practice_judge_settings (
    event_id UUID PRIMARY KEY REFERENCES public.events (id) ON DELETE CASCADE,
    -- 总开关：enabled 且 JudgeServer 容器 running 时，sweep worker 才派发检查批次。
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    -- JudgeServer 基址（含端口，如 http://10.42.2.2:8082）。
    -- 留空时平台自动推导为练习子网内固定 Judge IP（config.awdp.practice_judge_ip）。
    judge_server_url TEXT NOT NULL DEFAULT '',
    -- 例行检查间隔（秒）：sweep worker 按 last_sweep_at 距上次检查的间隔跳过。
    interval_secs INTEGER NOT NULL DEFAULT 60,
    -- flag curl 验证的端点路径（如 /flag.php；GameBox 按 FLAG env 返回 flag）。
    flag_path TEXT NOT NULL DEFAULT '/flag.php',
    -- JudgeServer 容器运行状态（stopped|running|failed）。
    container_status TEXT NOT NULL DEFAULT 'stopped',
    -- JudgeServer 容器 id（停服/健康检查用）。
    container_id TEXT NULL,
    -- 最近一次例行检查派发时间。
    last_sweep_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.awdp_practice_judge_settings IS
    'AWDP 练习 Judge 配置（AWDPlusPractice 虚拟赛事单行）：exploit 检查 + flag curl 验证的例行检查参数';

COMMENT ON COLUMN public.awdp_practice_judge_settings.event_id IS 'AWDPlusPractice 虚拟赛事 id';
COMMENT ON COLUMN public.awdp_practice_judge_settings.enabled IS '练习 Judge 总开关（enabled 且容器 running 才派发检查）';
COMMENT ON COLUMN public.awdp_practice_judge_settings.judge_server_url IS 'JudgeServer 基址（含端口）；留空自动推导为练习子网固定 IP';
COMMENT ON COLUMN public.awdp_practice_judge_settings.interval_secs IS '例行检查间隔（秒）';
COMMENT ON COLUMN public.awdp_practice_judge_settings.flag_path IS 'flag curl 验证端点路径（如 /flag.php）';
COMMENT ON COLUMN public.awdp_practice_judge_settings.container_status IS 'JudgeServer 容器运行状态（stopped|running|failed）';
COMMENT ON COLUMN public.awdp_practice_judge_settings.container_id IS 'JudgeServer 容器 id';
COMMENT ON COLUMN public.awdp_practice_judge_settings.last_sweep_at IS '最近一次例行检查派发时间';

-- ── 2) 练习 Judge 检查结果（审计/展示）──────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.awdp_judge_results (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES public.events (id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES public.awdp_runs (id) ON DELETE CASCADE,
    instance_id UUID NOT NULL REFERENCES public.event_instances (id) ON DELETE CASCADE,
    gamebox_id UUID NOT NULL REFERENCES public.gameboxes (id) ON DELETE RESTRICT,
    owner_user_id UUID NULL,
    owner_team_id UUID NULL,
    -- 检查类型：exploit（攻破验证）| flag（flag curl 验证）。
    check_kind TEXT NOT NULL,
    -- 结果：success | failure | error（含超时/脚本执行失败等）。
    status TEXT NOT NULL,
    -- 详情（脚本输出摘要 / HTTP 状态 / 期望与返回对比）。
    detail TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.awdp_judge_results IS
    'AWDP 练习 Judge 检查结果：JudgeServer 对练习 GameBox 的 exploit 检查与 flag curl 验证结果';

COMMENT ON COLUMN public.awdp_judge_results.check_kind IS '检查类型：exploit（攻破验证）| flag（flag curl 验证）';
COMMENT ON COLUMN public.awdp_judge_results.status IS '结果：success | failure | error';
COMMENT ON COLUMN public.awdp_judge_results.detail IS '详情（脚本输出摘要 / HTTP 状态 / 期望与返回对比）';

CREATE INDEX IF NOT EXISTS awdp_judge_results_event_created_idx
    ON public.awdp_judge_results (event_id, created_at DESC);
CREATE INDEX IF NOT EXISTS awdp_judge_results_instance_idx
    ON public.awdp_judge_results (instance_id, created_at DESC);

-- ── 3) seed AWDPlusPractice 默认配置行（幂等）───────────────────────────────────
INSERT INTO public.awdp_practice_judge_settings (event_id, updated_at)
SELECT '00000000-0000-0000-0000-000000000002', now()
WHERE NOT EXISTS (
    SELECT 1 FROM public.awdp_practice_judge_settings
    WHERE event_id = '00000000-0000-0000-0000-000000000002'
);
