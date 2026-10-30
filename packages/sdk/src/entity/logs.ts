export type Logs = {
  id: string;
  ip_address?: string;
  category: string;
  action: string;
  level: string;
  message: string;
  details: string;
  created_at: string;
  actor_user_id?: string;
};
