import type { InstanceStatus } from './sea_orm_active_enums';

export type ChallengeInstances = {
  id: string;
  status: InstanceStatus;
  flag: string;
  content?: string;
  challenge_id: string;
  user_id: string;
  identifier: string;
  created_at: string;
  updated_at: string;
  destroy_at: string;
  event_id: string;
  team_id?: string;
};
