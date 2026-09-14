-- ---------------------------------------------------------------------------
-- awdp_evaluation_proofs —— Official 评估 target-bound proof（plan §27-§30）
--
-- 目标：official evaluation claim 时生成 random 256-bit proof token，绑定
--   (evaluation_id, target instance, runtime_generation)。JudgeServer 把
--   FLOATCTF_PROOF_URL 注入 exploit 脚本环境；exploit 成功调用该 URL 即代表
--   目标 GameBox 确实执行了 proof 请求（比脚本自报 success 更强的证据）。
--
-- 语义：
--   - token 明文只出现在 job payload（JudgeServer 内存/注入脚本）；DB 只存 sha256；
--   - 一次性：consumed_at 原子置位（条件 UPDATE 防双消费）；
--   - 短生命周期：expires_at（claim + lease 时长量级）；过期不可消费；
--   - /flag（Break 确定性 flag）与 /proof/{token}（Official 一次性 proof）完全分离。
--
-- 不复用 AWD flag tables（awdp 域边界约束）。
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.awdp_evaluation_proofs (
    id UUID PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    evaluation_id UUID NOT NULL REFERENCES public.awdp_evaluations (id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    target_instance_id UUID NOT NULL REFERENCES public.event_instances (id) ON DELETE CASCADE,
    runtime_generation BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_awdp_evaluation_proofs_eval
    ON public.awdp_evaluation_proofs (evaluation_id);

COMMENT ON TABLE public.awdp_evaluation_proofs IS
    'AWDP official 评估一次性 target-bound proof token（token 明文不落库，只存 sha256）';
COMMENT ON COLUMN public.awdp_evaluation_proofs.token_hash IS 'proof token 的 sha256 哈希（明文仅存在于 job payload）';
COMMENT ON COLUMN public.awdp_evaluation_proofs.consumed_at IS '消费时间（原子置位；一次性）';
