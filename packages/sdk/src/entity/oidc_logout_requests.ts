export type OidcLogoutRequests = {
  id: string;
  browser_binding_hash: string;
  csrf_hash: string;
  oidc_client_id?: string;
  hint_sub?: string;
  hint_sid?: string;
  post_logout_redirect_uri?: string;
  state?: string;
  created_at: string;
  expires_at: string;
  completed_at?: string;
};
