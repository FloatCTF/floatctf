import type { AwdNetworkAllocationMode } from './sea_orm_active_enums';

export type AwdEventNetworks = {
  id: string;
  event_id: string;
  allocation_mode: AwdNetworkAllocationMode;
  gamebox_cidr: string;
  wireguard_cidr: string;
  infrastructure_subnet: string;
  flagserver_ip: string;
  judgeserver_ip: string;
  wireguard_interface_name: string;
  wireguard_listen_port: number;
  docker_network_name: string;
  locked_at?: string;
  created_at: string;
  updated_at: string;
};
