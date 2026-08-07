import type { PrecheckStatus } from './sea_orm_active_enums';

export type AwdPrecheckRuns = {
  id: string;
  event_id: string;
  status: PrecheckStatus;
  trigger: string;
  revision?: string;
  config_check?: string;
  container_check?: string;
  wireguard_check?: string;
  network_check?: string;
  flag_check?: string;
  judge_check?: string;
  error_msg?: string;
  started_at: string;
  completed_at?: string;
};
