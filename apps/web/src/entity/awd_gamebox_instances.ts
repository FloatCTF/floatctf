import type { GameboxStatus } from './sea_orm_active_enums';

export type AwdGameboxInstances = {
  id: string;
  event_id: string;
  template_id: string;
  team_id: string;
  status: GameboxStatus;
  container_id?: string;
  container_name: string;
  gamebox_ip: string;
  docker_network_id?: string;
  health_status: string;
  reset_protection_until?: string;
  last_health_check_at?: string;
  created_at: string;
  updated_at: string;
  deleted_at?: string;
};
