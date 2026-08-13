export type AwdpEvaluationProofs = {
  id: string;
  evaluation_id: string;
  token_hash: string;
  target_instance_id: string;
  runtime_generation: number;
  expires_at: string;
  consumed_at?: string;
  created_at: string;
};
