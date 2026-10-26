export type OidcBackchannelDeliveries = {
  id: string;
  batch_id: string;
  issuer: string;
  oidc_client_id: string;
  user_id?: string;
  sub: string;
  sid: string;
  reason: string;
  status: string;
  attempts: number;
  next_attempt_at: string;
  last_error?: string;
  last_http_status?: string;
  last_jti?: string;
  created_at: string;
  delivered_at?: string;
};
