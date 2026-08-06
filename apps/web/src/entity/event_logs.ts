import type { EventType } from './sea_orm_active_enums';

export type EventLogs = {
  id: string;
  event_id: string;
  user_id?: string;
  team_id?: string;
  type: EventType;
  level: string;
  action: string;
  details: string;
  created_at: string;
  ip_address?: string;
};
