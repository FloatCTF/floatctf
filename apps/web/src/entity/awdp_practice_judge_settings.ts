export type AwdpPracticeJudgeSettings = {
  event_id: string;
  enabled: boolean;
  judge_server_url: string;
  interval_secs: number;
  flag_path: string;
  container_status: string;
  container_id?: string;
  last_sweep_at?: string;
  created_at: string;
  updated_at: string;
};
