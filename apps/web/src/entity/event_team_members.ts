import type { EventTeamMemberRole } from './sea_orm_active_enums';

export type EventTeamMembers = {
  event_id: string;
  team_id: string;
  user_id: string;
  role: EventTeamMemberRole;
  joined_at: string;
};
