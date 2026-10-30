export type OidcAuthorizationCodes = {
  id: string;
  code_hash: string;
  oidc_client_id: string;
  user_id: string;
  session_id?: string;
  redirect_uri: string;
  scopes: string;
  nonce?: string;
  code_challenge?: string;
  code_challenge_method?: string;
  auth_time: string;
  created_at: string;
  expires_at: string;
  consumed_at?: string;
};
