import type { AwdpEvaluationKind, AwdpEvaluationStatus } from './sea_orm_active_enums';

export type AwdpEvaluations = {
  id: string;
  event_id: string;
  instance_id: string;
  fix_round_id?: string;
  kind: AwdpEvaluationKind;
  status: AwdpEvaluationStatus;
  healthcheck_result?: string;
  judge_result?: string;
  exploit_result?: string;
  stdout_limited?: string;
  stderr_limited?: string;
  started_at?: string;
  finished_at?: string;
  created_at: string;
  updated_at: string;
};
