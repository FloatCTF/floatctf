export type AwdTeamNetworks = {
  id: string;
  event_id: string;
  team_id: string;
  gamebox_subnet: string;
  wireguard_subnet: string;
  ssh_password_ciphertext: string;
  ssh_password_nonce: string;
  key_version: number;
  next_wireguard_host: number;
  status: string;
  created_at: string;
  updated_at: string;
};
