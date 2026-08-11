import type { AwdpPhase } from './sea_orm_active_enums';

export type AwdpEvents = {
  event_id: string;
  phase: AwdpPhase;
  break_duration_secs: number;
  fix_duration_secs: number;
  fix_round_interval_secs: number;
  break_score: number;
  fix_round_score: number;
  configuration_generation: number;
  started_at?: string;
  break_ends_at?: string;
  fix_started_at?: string;
  fix_ends_at?: string;
  finished_at?: string;
  current_round: number;
  next_action_at?: string;
  created_at: string;
  updated_at: string;
};
