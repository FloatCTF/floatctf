import type { SettingValueType } from './sea_orm_active_enums';

export type Settings = {
  id: string;
  key: string;
  value: string;
  /** 解析 {{VAR}} 引用后的值（value 保留原始模板） */
  resolved_value: string;
  type: SettingValueType;
  description: string;
  protected: boolean;
  updated_at: string;
};
