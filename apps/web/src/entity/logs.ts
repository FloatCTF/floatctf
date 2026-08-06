export type Logs = {
  id: string;
  user_id?: string;
  superadmin_id?: string;
  ip_address?: string;
  category: string;
  action: string;
  level: string;
  message: string;
  details: string;
  created_at: string;
};
