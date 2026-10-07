export type AwdpEventGameboxes = {
  id: string;
  event_id: string;
  gamebox_id: string;
  enabled: boolean;
  hidden: boolean;
  cpu_millis: number;
  memory_bytes: number;
  pids_limit: number;
  healthcheck_override_json?: string;
  created_at: string;
  updated_at: string;
};
