import type { AwdpPhase } from './sea_orm_active_enums';

export type AwdpRuns = {
  id: string;
  event_id: string;
  gamebox_id?: string;
  owner_user_id?: string;
  owner_team_id?: string;
  phase: AwdpPhase;
  break_duration_secs: number;
  fix_duration_secs: number;
  fix_round_interval_secs: number;
  break_score: number;
  fix_round_score: number;
  started_at?: string;
  break_ends_at?: string;
  fix_started_at?: string;
  fix_ends_at?: string;
  finished_at?: string;
  current_round: number;
  total_rounds: number;
  next_action_at?: string;
  created_at: string;
  updated_at: string;
  early_patched_seq?: string;
};
