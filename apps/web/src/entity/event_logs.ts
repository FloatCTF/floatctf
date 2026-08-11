import type { EventFamily, EventPurpose, ParticipantMode } from './sea_orm_active_enums';

export type EventLogs = {
  id: string;
  event_id: string;
  user_id?: string;
  team_id?: string;
  ip_address?: string;
  level: string;
  action: string;
  details: string;
  created_at: string;
  family: EventFamily;
  purpose: EventPurpose;
  participant_mode: ParticipantMode;
};
