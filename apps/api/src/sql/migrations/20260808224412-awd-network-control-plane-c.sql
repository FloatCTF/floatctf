-- ================================================================================
-- Migration: 20260808224412-awd-network-control-plane-c
-- ================================================================================
-- AWD Network Control Plane 重构 — Migration C：网络列类型化（CIDR/INET）。
--
-- 依赖 Migration A 注册的 text→inet/cidr 隐式转换（SeaORM 以 text 绑定参数）。
-- 旧值格式本就合法（CIDR/IP 字符串），USING 显式转换；若旧数据非法格式
-- 则 ALTER 失败 = STOP，不静默改数据。
-- ================================================================================


-- Team Network：子网 → CIDR
ALTER TABLE "awd_team_networks"
    ALTER COLUMN "gamebox_subnet" TYPE CIDR USING "gamebox_subnet"::cidr;

ALTER TABLE "awd_team_networks"
    ALTER COLUMN "wireguard_subnet" TYPE CIDR USING "wireguard_subnet"::cidr;

-- WG peer：稳定 /32 → INET
ALTER TABLE "awd_wireguard_peers"
    ALTER COLUMN "assigned_ip" TYPE INET USING "assigned_ip"::inet;

-- GameBox Instance：固定 IP → INET
ALTER TABLE "awd_gamebox_instances"
    ALTER COLUMN "gamebox_ip" TYPE INET USING "gamebox_ip"::inet;

COMMENT ON COLUMN "awd_team_networks"."gamebox_subnet" IS 'Team GameBox 子网（Event Network 内的稳定持久分配，CIDR）';
COMMENT ON COLUMN "awd_team_networks"."wireguard_subnet" IS 'Team WireGuard 子网（CIDR）';
COMMENT ON COLUMN "awd_wireguard_peers"."assigned_ip" IS 'Peer 稳定 /32 地址（INET）';
COMMENT ON COLUMN "awd_gamebox_instances"."gamebox_ip" IS 'GameBox 固定 IP = Team gamebox_subnet + AwdEventGameBox.host_offset（INET）';

