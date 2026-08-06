import type { SettingValueType } from './sea_orm_active_enums';

export type Settings = {
  id: string;
  key: string;
  value: string;
  type: SettingValueType;
  description: string;
  protected: boolean;
  updated_at: string;
};
