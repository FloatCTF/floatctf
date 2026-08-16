export type AwdpEventNetworks = {
  event_id: string;
  network_name: string;
  subnet_cidr: string;
  dynamic_pool_cidr: string;
  judge_ip: string;
  docker_network_id?: string;
  status: string;
  created_at: string;
  updated_at: string;
};
