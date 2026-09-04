export type EventInstances = {
  id: string;
  event_id: string;
  owner_user_id?: string;
  owner_team_id?: string;
  image_ref?: string;
  container_id?: string;
  container_name: string;
  runtime_state: string;
  runtime_generation: number;
  created_at: string;
  started_at?: string;
  stopped_at?: string;
  expires_at?: string;
  updated_at: string;
};
