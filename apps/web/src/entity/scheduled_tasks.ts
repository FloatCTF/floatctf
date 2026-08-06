export type ScheduledTasks = {
  id: string;
  group_id?: string;
  task_key: string;
  trigger_type: string;
  status: string;
  cron_expr?: string;
  execute_at?: string;
  expires_at?: string;
  payload?: string;
  error_msg?: string;
  last_run_at?: string;
  created_at: string;
  updated_at: string;
  task_name: string;
  description?: string;
  enabled: boolean;
  protected: boolean;
};
