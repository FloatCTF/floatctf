export type OidcAccessTokens = {
  id: string;
  token_hash: string;
  oidc_client_id: string;
  user_id: string;
  authorization_code_id?: string;
  scopes: string;
  created_at: string;
  expires_at: string;
  revoked_at?: string;
};
