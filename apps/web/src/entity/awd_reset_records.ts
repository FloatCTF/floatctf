export type AwdResetRecords = {
  id: string;
  event_id: string;
  team_id: string;
  gamebox_instance_id: string;
  round_id?: string;
  requested_by?: string;
  free_reset: boolean;
  penalty_score_event_id?: string;
  status: string;
  started_at: string;
  completed_at?: string;
  error_msg?: string;
  created_at: string;
};
