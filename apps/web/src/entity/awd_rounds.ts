import type { AwdPhase, RoundStatus } from './sea_orm_active_enums';

export type AwdRounds = {
  id: string;
  event_id: string;
  round_number: number;
  status: RoundStatus;
  phase: AwdPhase;
  started_at: string;
  scheduled_end_at: string;
  paused_at?: string;
  remaining_secs?: string;
  completed_at?: string;
  created_at: string;
};
