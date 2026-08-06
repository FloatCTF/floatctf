import type { InstanceStatus } from './sea_orm_active_enums';

export type Instances = {
  id: string;
  status: InstanceStatus;
  ref: string;
  flag: string;
  content?: string;
  challenge_id?: string;
  user_id: string;
  identifier: string;
  created_at: string;
  updated_at: string;
  destroy_at: string;
  gamebox_id?: string;
};
