export type OidcBrowserContainers = {
  id: string;
  secret_hash: string;
  web_session_id?: string;
  web_signed_out_at?: string;
  created_at: string;
  last_seen_at: string;
  expires_at: string;
  revoked_at?: string;
};
