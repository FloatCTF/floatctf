import type { GameboxStatus } from './sea_orm_active_enums';

export type EventGameboxInstances = {
  id: string;
  event_id: string;
  team_id: string;
  status: GameboxStatus;
  gamebox_ip: string;
  health_status: string;
  last_health_check_at?: string;
  created_at: string;
  updated_at: string;
  deleted_at?: string;
  event_gamebox_id: string;
  instance_id: string;
};
