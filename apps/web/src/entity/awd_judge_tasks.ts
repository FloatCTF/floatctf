import type { JudgeTaskStatus } from './sea_orm_active_enums';

export type AwdJudgeTasks = {
  id: string;
  batch_id: string;
  event_id: string;
  round_id: string;
  gamebox_instance_id: string;
  team_id: string;
  status: JudgeTaskStatus;
  attempt_count: number;
  max_attempts: number;
  deadline_at: string;
  started_at?: string;
  finished_at?: string;
  exit_code?: string;
  stdout_limited?: string;
  stderr_limited?: string;
  duration_ms?: string;
  callback_idempotency_key?: string;
  created_at: string;
  event_gamebox_id?: string;
  worker_id?: string;
  lease_token_hash?: string;
  lease_expires_at?: string;
  heartbeat_at?: string;
  claimed_at?: string;
};
