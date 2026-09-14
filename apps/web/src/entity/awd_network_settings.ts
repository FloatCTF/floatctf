export type AwdNetworkSettings = {
  id: string;
  gamebox_pool: string;
  gamebox_event_prefix: string;
  gamebox_team_prefix: string;
  wireguard_pool: string;
  wireguard_event_prefix: string;
  wireguard_team_prefix: string;
  wireguard_port_min: number;
  wireguard_port_max: number;
  wireguard_public_endpoint?: string;
  updated_at: string;
};
