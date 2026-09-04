-- ================================================================================
-- Migration: 20260815224525-awdp-event-networks
-- AWDP 每赛事独立 Docker 网络模型：
--   awdp_event_networks —— 每个 AWDP Event 一行：赛事专属 Docker 网络（子网 / 动态池 /
--   judge 固定 IP / 真实 docker network id / 生命周期状态）。练习（AWDPlusPractice
--   虚拟赛事）不落此表，沿用 config.awdp.practice_network_subnet 固定网络。
-- ================================================================================

CREATE TABLE IF NOT EXISTS public.awdp_event_networks (
    event_id UUID PRIMARY KEY REFERENCES public.events (id) ON DELETE CASCADE,
    network_name VARCHAR(64) NOT NULL UNIQUE,
    subnet_cidr CIDR NOT NULL UNIQUE,
    dynamic_pool_cidr CIDR NOT NULL,
    judge_ip INET NOT NULL,
    docker_network_id TEXT NULL,
    status TEXT NOT NULL DEFAULT 'allocated',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT awdp_event_networks_judge_inside_subnet CHECK ((judge_ip << (subnet_cidr)::inet)),
    CONSTRAINT awdp_event_networks_pool_inside_subnet CHECK (((dynamic_pool_cidr)::inet <<= (subnet_cidr)::inet)),
    CONSTRAINT awdp_event_networks_judge_not_in_pool CHECK (NOT ((judge_ip) <<= (dynamic_pool_cidr)::inet)),
    CONSTRAINT awdp_event_networks_status_valid CHECK (status IN ('allocated', 'deployed', 'released'))
);

COMMENT ON TABLE public.awdp_event_networks IS
    'AWDP 每赛事独立 Docker 网络资源：赛事专属网络（子网/动态池/judge 固定 IP）与 Docker 网络实际 id、生命周期状态';

COMMENT ON COLUMN public.awdp_event_networks.event_id IS 'AWDP 赛事 id（1:1；练习虚拟赛事 AWDPlusPractice 不落此表）';
COMMENT ON COLUMN public.awdp_event_networks.network_name IS '赛事 Docker 网络逻辑名（如 fctf-awdp-{event_id 前 12 hex}）';
COMMENT ON COLUMN public.awdp_event_networks.subnet_cidr IS '赛事子网（从 config.awdp.network_pool 池分配，赛事间互不重叠）';
COMMENT ON COLUMN public.awdp_event_networks.dynamic_pool_cidr IS '子网后半段动态 IP 池（GameBox 实例 IP 分配范围；judge 固定 IP 位于池外）';
COMMENT ON COLUMN public.awdp_event_networks.judge_ip IS '赛事 JudgeServer 固定 IP（位于 subnet 内、dynamic_pool 外）';
COMMENT ON COLUMN public.awdp_event_networks.docker_network_id IS 'Observed：docker network inspect 的真实网络 id（desired identity 在 network_name）';
COMMENT ON COLUMN public.awdp_event_networks.status IS '生命周期：allocated（已分配未部署）/ deployed（网络已创建）/ released（已清理释放）';
