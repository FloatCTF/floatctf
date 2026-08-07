import type { ScoreEventType } from './sea_orm_active_enums';

export type AwdScoreEvents = {
  id: string;
  event_id: string;
  round_id?: string;
  team_id: string;
  event_type: ScoreEventType;
  delta: number;
  idempotency_key: string;
  related_team_id?: string;
  gamebox_instance_id?: string;
  gamebox_template_id?: string;
  reference_id?: string;
  reason?: string;
  metadata_json: string;
  created_by?: string;
  created_at: string;
};
