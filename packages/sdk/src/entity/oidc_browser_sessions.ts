export type OidcBrowserSessions = {
  id: string;
  session_hash: string;
  user_id: string;
  auth_time: string;
  established_via: string;
  created_at: string;
  last_seen_at: string;
  expires_at: string;
  revoked_at?: string;
  container_id?: string;
  sid: string;
  revoked_reason?: string;
};
