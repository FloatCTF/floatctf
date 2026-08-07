import type { WgPeerStatus } from './sea_orm_active_enums';

export type AwdWireguardPeers = {
  id: string;
  event_id: string;
  team_id: string;
  user_id: string;
  status: WgPeerStatus;
  assigned_ip: string;
  public_key: string;
  private_key_ciphertext: string;
  private_key_nonce: string;
  key_version: number;
  created_at: string;
  rotated_at?: string;
  revoked_at?: string;
};
