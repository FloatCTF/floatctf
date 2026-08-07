import type { BanStatus } from './sea_orm_active_enums';

export type AwdTeamBans = {
  id: string;
  event_id: string;
  team_id: string;
  status: BanStatus;
  reason?: string;
  effective_round_id?: string;
  banned_by?: string;
  banned_at: string;
  unban_requested_at?: string;
  unban_effective_round_id?: string;
  unbanned_by?: string;
  unbanned_at?: string;
};
