export type AwdpScoreEvents = {
  id: string;
  event_id?: string;
  user_id?: string;
  team_id?: string;
  score_type: string;
  fix_round_id?: string;
  delta: number;
  idempotency_key: string;
  created_at: string;
  run_id: string;
  gamebox_id: string;
};
