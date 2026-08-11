export type AwdpPatchSubmissions = {
  id: string;
  event_id: string;
  instance_id: string;
  fix_round_id?: string;
  user_id?: string;
  team_id?: string;
  script_sha256: string;
  script_content: string;
  status: string;
  submitted_at: string;
  apply_started_at?: string;
  applied_at?: string;
  exit_code?: string;
  stdout_limited?: string;
  stderr_limited?: string;
  error_message?: string;
};
