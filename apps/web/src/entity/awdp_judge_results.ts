export type AwdpJudgeResults = {
  id: string;
  event_id: string;
  run_id: string;
  instance_id: string;
  gamebox_id: string;
  owner_user_id?: string;
  owner_team_id?: string;
  check_kind: string;
  status: string;
  detail?: string;
  created_at: string;
  callback_id?: string;
};
