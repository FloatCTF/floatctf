import type { AccountStatus } from './sea_orm_active_enums.js';

export type Users = {
  id: string;
  username: string;
  nickname: string;
  password: string;
  email: string;
  created_at: string;
  updated_at: string;
  avatar?: string;
  status: AccountStatus;
  token_version: number;
};
