export type AwdOrphanResources = {
  id: string;
  event_id?: string;
  resource_type: string;
  resource_id: string;
  resource_name?: string;
  observed_state?: string;
  discovered_at: string;
  resolved_at?: string;
  resolution?: string;
};
