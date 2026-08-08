import type { GameboxStatus } from './sea_orm_active_enums';

export type AwdGameboxInstances = {
  id: string;
  event_id: string;
  team_id: string;
  status: GameboxStatus;
  container_name: string;
  gamebox_ip: string;
  health_status: string;
  reset_protection_until?: string;
  last_health_check_at?: string;
  created_at: string;
  updated_at: string;
  deleted_at?: string;
  event_gamebox_id: string;
  runtime_generation: number;
  current_container_id?: string;
};
