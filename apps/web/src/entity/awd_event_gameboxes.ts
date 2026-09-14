export type AwdEventGameboxes = {
  id: string;
  event_id: string;
  gamebox_id: string;
  host_offset: string;
  enabled: boolean;
  hidden: boolean;
  cpu_millis: number;
  memory_bytes: number;
  pids_limit: number;
  healthcheck_override_json?: string;
  judge_timeout_secs?: string;
  judge_retry_interval_secs?: string;
  judge_down_penalty: number;
  first_bonus: number;
  created_at: string;
  updated_at: string;
  attack_score: number;
};
