-- ================================================================================
-- Migration: 20261025211040-oidc-standard-logout
-- ================================================================================
--
-- 标准退出（设计见 docs/identity/ADR-unified-identity.md §6）：
-- - OpenID Connect RP-Initiated Logout 1.0：应用登记 post_logout_redirect_uris；
--   end_session 请求在 OP 确认页上以一次性事务推进（oidc_logout_requests）。
-- - OpenID Connect Back-Channel Logout 1.0：应用登记 backchannel_logout_uri；
--   oidc_session_clients 记录「哪个身份会话（sid）向哪个应用签发过令牌」，
--   只有用户明确选择「退出所有关联应用」时，按此投递 Logout Token（outbox：oidc_backchannel_deliveries）。
--
-- 只新增表与可空/有默认值的列；旧版本 API 忽略新列，回滚安全。幂等，可重复执行。

ALTER TABLE public.oidc_clients
    ADD COLUMN IF NOT EXISTS post_logout_redirect_uris jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS backchannel_logout_uri text;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'oidc_clients_post_logout_redirect_uris_is_array'
    ) THEN
        ALTER TABLE public.oidc_clients
            ADD CONSTRAINT oidc_clients_post_logout_redirect_uris_is_array
            CHECK (jsonb_typeof(post_logout_redirect_uris) = 'array');
    END IF;
END
$$;

COMMENT ON COLUMN public.oidc_clients.post_logout_redirect_uris IS 'RP-Initiated Logout 允许的退出后回跳地址（逐字精确匹配，JSON 数组）';
COMMENT ON COLUMN public.oidc_clients.backchannel_logout_uri IS 'Back-Channel Logout 接收地址（https；只在用户明确「退出所有关联应用」时投递 Logout Token）';

CREATE TABLE IF NOT EXISTS public.oidc_session_clients (
    session_id uuid NOT NULL REFERENCES public.oidc_browser_sessions(id) ON DELETE CASCADE,
    oidc_client_id uuid NOT NULL REFERENCES public.oidc_clients(id) ON DELETE CASCADE,
    first_issued_at timestamp with time zone NOT NULL DEFAULT now(),
    last_issued_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, oidc_client_id)
);

CREATE INDEX IF NOT EXISTS idx_oidc_session_clients_client
    ON public.oidc_session_clients (oidc_client_id);

COMMENT ON TABLE public.oidc_session_clients IS '身份会话（sid）向哪些应用签发过 ID Token：全局退出时据此确定需要通知的应用';
COMMENT ON COLUMN public.oidc_session_clients.session_id IS '身份会话（oidc_browser_sessions.id）';
COMMENT ON COLUMN public.oidc_session_clients.oidc_client_id IS '应用（oidc_clients.id）';
COMMENT ON COLUMN public.oidc_session_clients.first_issued_at IS '首次向该应用签发令牌的时间';
COMMENT ON COLUMN public.oidc_session_clients.last_issued_at IS '最近一次向该应用签发令牌的时间';

CREATE TABLE IF NOT EXISTS public.oidc_logout_requests (
    id uuid PRIMARY KEY,
    browser_binding_hash text NOT NULL,
    csrf_hash text NOT NULL,
    oidc_client_id uuid REFERENCES public.oidc_clients(id) ON DELETE SET NULL,
    hint_sub text,
    hint_sid text,
    post_logout_redirect_uri text,
    state text,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone NOT NULL,
    completed_at timestamp with time zone
);

CREATE INDEX IF NOT EXISTS idx_oidc_logout_requests_expires
    ON public.oidc_logout_requests (expires_at);

COMMENT ON TABLE public.oidc_logout_requests IS 'RP-Initiated Logout 事务：end_session 请求经校验后在 OP 确认页一次性推进（浏览器绑定 + CSRF）';
COMMENT ON COLUMN public.oidc_logout_requests.id IS '主键（确认页表单携带）';
COMMENT ON COLUMN public.oidc_logout_requests.browser_binding_hash IS '发起浏览器绑定 Cookie 的 SHA-256 摘要（只能在同一浏览器内确认）';
COMMENT ON COLUMN public.oidc_logout_requests.csrf_hash IS '确认页 CSRF token 的 SHA-256 摘要';
COMMENT ON COLUMN public.oidc_logout_requests.oidc_client_id IS '发起退出的应用（由 id_token_hint.aud 或 client_id 确定，可空）';
COMMENT ON COLUMN public.oidc_logout_requests.hint_sub IS 'id_token_hint 的 sub（已验签）';
COMMENT ON COLUMN public.oidc_logout_requests.hint_sid IS 'id_token_hint 的 sid（已验签）；只作用于本浏览器中与之匹配的会话';
COMMENT ON COLUMN public.oidc_logout_requests.post_logout_redirect_uri IS '已与应用登记值逐字匹配的退出后回跳地址';
COMMENT ON COLUMN public.oidc_logout_requests.state IS 'RP 传入的 state，回跳时原样带回';
COMMENT ON COLUMN public.oidc_logout_requests.created_at IS '创建时间';
COMMENT ON COLUMN public.oidc_logout_requests.expires_at IS '过期时间';
COMMENT ON COLUMN public.oidc_logout_requests.completed_at IS '完成时间（确认或取消后设置；只能完成一次）';

CREATE TABLE IF NOT EXISTS public.oidc_backchannel_deliveries (
    id uuid PRIMARY KEY,
    batch_id uuid NOT NULL,
    issuer text NOT NULL,
    oidc_client_id uuid NOT NULL REFERENCES public.oidc_clients(id) ON DELETE CASCADE,
    user_id uuid,
    sub text NOT NULL,
    sid text NOT NULL,
    reason text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    attempts integer NOT NULL DEFAULT 0,
    next_attempt_at timestamp with time zone NOT NULL DEFAULT now(),
    last_error text,
    last_http_status integer,
    last_jti text,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    delivered_at timestamp with time zone,
    CONSTRAINT oidc_backchannel_deliveries_status_check
        CHECK (status IN ('pending', 'delivered', 'failed')),
    CONSTRAINT oidc_backchannel_deliveries_reason_check
        CHECK (reason IN ('main_site', 'end_session'))
);

CREATE INDEX IF NOT EXISTS idx_oidc_backchannel_deliveries_due
    ON public.oidc_backchannel_deliveries (issuer, next_attempt_at)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_oidc_backchannel_deliveries_batch
    ON public.oidc_backchannel_deliveries (batch_id);

COMMENT ON TABLE public.oidc_backchannel_deliveries IS 'Back-Channel Logout 投递 outbox：每个 (退出操作, 应用, sid) 一行，记录尝试次数、结果与错误，供重试与审计';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.id IS '主键';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.batch_id IS '同一次「退出所有关联应用」操作的批次（用于向用户报告结果）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.issuer IS '签发 Logout Token 的 issuer（多 issuer 部署下只由同 issuer 实例投递）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.oidc_client_id IS '目标应用';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.user_id IS '被退出的用户（用户删除后保留 sub 供审计）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.sub IS 'Logout Token sub';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.sid IS 'Logout Token sid（被退出的身份会话）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.reason IS '触发来源：main_site=主站「退出所有关联应用」，end_session=应用发起并经用户确认';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.status IS 'pending=待投递/重试中，delivered=应用已确认（2xx），failed=放弃（应用拒绝或重试耗尽）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.attempts IS '已尝试次数';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.next_attempt_at IS '下次可尝试时间（指数退避；领取时用作租约）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.last_error IS '最近一次失败原因（不含令牌内容）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.last_http_status IS '最近一次 HTTP 状态码';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.last_jti IS '最近一次投递的 Logout Token jti（每次尝试重新签发）';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.created_at IS '创建时间';
COMMENT ON COLUMN public.oidc_backchannel_deliveries.delivered_at IS '应用确认时间';
