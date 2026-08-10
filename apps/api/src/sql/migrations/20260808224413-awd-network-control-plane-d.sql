-- ================================================================================
-- Migration: 20260808224413-awd-network-control-plane-d
-- ================================================================================
-- AWD Network Control Plane 重构 — Migration D：删除 awd_events 重复网络字段。
--
-- 逻辑网络配置已迁至 awd_event_networks（Migration B 回填）；
-- docker_network_id 属 Observed（runtime），由 awd_runtime_resources 管理。
-- next_gamebox_host 已在 GameBox 领域重构（Migration D of that series）删除。
-- ================================================================================


ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "gamebox_cidr";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "wireguard_cidr";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "wireguard_interface_name";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "wireguard_listen_port";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "flagserver_ip";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "judgeserver_ip";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "docker_network_id";
ALTER TABLE "awd_events" DROP COLUMN IF EXISTS "docker_network_name";

