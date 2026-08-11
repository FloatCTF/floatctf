import type { EventFamily, EventPurpose, ParticipantMode } from './sea_orm_active_enums';

export type Events = {
  id: string;
  title: string;
  description?: string;
  hidden: boolean;
  start_time: string;
  rules: string;
  allow_join: boolean;
  flag_prefix?: string;
  end_time?: string;
  created_at: string;
  updated_at: string;
  family: EventFamily;
  purpose: EventPurpose;
  participant_mode: ParticipantMode;
  system_key?: string;
};
