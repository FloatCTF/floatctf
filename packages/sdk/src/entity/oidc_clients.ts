import type { OidcClientStatus, OidcClientType, OidcTokenAuthMethod } from './sea_orm_active_enums.js';

export type OidcClients = {
  id: string;
  client_id: string;
  owner_user_id: string;
  name: string;
  description?: string;
  homepage_url?: string;
  logo_url?: string;
  client_type: OidcClientType;
  token_endpoint_auth_method: OidcTokenAuthMethod;
  redirect_uris: string;
  allowed_scopes: string;
  require_pkce: boolean;
  secret_hash?: string;
  previous_secret_hash?: string;
  previous_secret_expires_at?: string;
  secret_rotated_at?: string;
  status: OidcClientStatus;
  status_reason?: string;
  reviewed_by?: string;
  reviewed_at?: string;
  created_at: string;
  updated_at: string;
  post_logout_redirect_uris: string;
  backchannel_logout_uri?: string;
};
