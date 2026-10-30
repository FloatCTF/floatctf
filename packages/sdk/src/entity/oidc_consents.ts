export type OidcConsents = {
  id: string;
  user_id: string;
  oidc_client_id: string;
  scopes: string;
  granted_at: string;
  updated_at: string;
  last_used_at?: string;
  revoked_at?: string;
};
