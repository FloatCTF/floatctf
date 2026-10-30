-- Migration: 20261023162115-oidc-provider
-- FloatCTF 作为通用 OpenID Connect Provider（OP）所需的存储：
-- 应用注册、第一方 SSO 会话、授权事务、授权码、用户授权记录、不透明 access token。
-- 只新增表与类型，不修改 users / super_admin 及任何既有业务表。
-- 所有令牌类字段只保存 SHA-256 摘要；client secret 只保存 Argon2 PHC 串。

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'oidc_client_type') THEN
        CREATE TYPE public.oidc_client_type AS ENUM ('confidential', 'public');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'oidc_client_status') THEN
        CREATE TYPE public.oidc_client_status AS ENUM ('pending', 'active', 'disabled', 'suspended', 'rejected');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'oidc_token_auth_method') THEN
        CREATE TYPE public.oidc_token_auth_method AS ENUM ('client_secret_basic', 'client_secret_post', 'none');
    END IF;
END $$;

COMMENT ON TYPE public.oidc_client_type IS 'OIDC 客户端类型：confidential=能保管 secret 的服务端应用，public=浏览器/原生应用（无 secret，强制 PKCE）';
COMMENT ON TYPE public.oidc_client_status IS 'OIDC 客户端状态：pending=待审批，active=启用，disabled=所有者暂停，suspended=管理员封禁，rejected=审批拒绝';
COMMENT ON TYPE public.oidc_token_auth_method IS 'Token 端点客户端认证方式（RFC 6749 §2.3 / OIDC Core §9）';

-- ── 应用注册 ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.oidc_clients (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL PRIMARY KEY,
    client_id text NOT NULL UNIQUE,
    owner_user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    name text NOT NULL,
    description text,
    homepage_url text,
    logo_url text,
    client_type public.oidc_client_type NOT NULL,
    token_endpoint_auth_method public.oidc_token_auth_method NOT NULL,
    redirect_uris jsonb DEFAULT '[]'::jsonb NOT NULL,
    allowed_scopes jsonb DEFAULT '["openid"]'::jsonb NOT NULL,
    require_pkce boolean DEFAULT true NOT NULL,
    secret_hash text,
    previous_secret_hash text,
    previous_secret_expires_at timestamp with time zone,
    secret_rotated_at timestamp with time zone,
    status public.oidc_client_status DEFAULT 'pending'::public.oidc_client_status NOT NULL,
    status_reason text,
    reviewed_by uuid REFERENCES public.super_admin(id) ON DELETE SET NULL,
    reviewed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT oidc_clients_redirect_uris_is_array CHECK (jsonb_typeof(redirect_uris) = 'array'),
    CONSTRAINT oidc_clients_allowed_scopes_is_array CHECK (jsonb_typeof(allowed_scopes) = 'array'),
    CONSTRAINT oidc_clients_type_auth_consistency CHECK (
        (client_type = 'public' AND token_endpoint_auth_method = 'none' AND secret_hash IS NULL
            AND previous_secret_hash IS NULL AND require_pkce)
        OR (client_type = 'confidential' AND token_endpoint_auth_method <> 'none' AND secret_hash IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_oidc_clients_owner ON public.oidc_clients (owner_user_id);
CREATE INDEX IF NOT EXISTS idx_oidc_clients_status ON public.oidc_clients (status);

COMMENT ON TABLE public.oidc_clients IS 'OIDC 应用（Relying Party）注册：由玩家登记、管理员审批；只有 active 状态可发起授权';
COMMENT ON COLUMN public.oidc_clients.id IS '内部主键（其他 oidc_* 表的 oidc_client_id 外键指向这里，不是公开 client_id）';
COMMENT ON COLUMN public.oidc_clients.client_id IS '公开 client_id（不可预测、全局唯一，不承担认证）';
COMMENT ON COLUMN public.oidc_clients.owner_user_id IS '应用所有者（普通玩家 users.id）；所有者只能管理自己的应用';
COMMENT ON COLUMN public.oidc_clients.name IS '应用显示名称（授权确认页展示）';
COMMENT ON COLUMN public.oidc_clients.description IS '应用简介（可空）';
COMMENT ON COLUMN public.oidc_clients.homepage_url IS '应用主页 URL（可空，授权确认页展示其域名）';
COMMENT ON COLUMN public.oidc_clients.logo_url IS '应用图标 URL（可空，https）';
COMMENT ON COLUMN public.oidc_clients.client_type IS '客户端类型：confidential / public';
COMMENT ON COLUMN public.oidc_clients.token_endpoint_auth_method IS 'Token 端点认证方式；public 固定 none';
COMMENT ON COLUMN public.oidc_clients.redirect_uris IS '已注册回调地址 JSON 数组（精确字符串匹配，无通配）';
COMMENT ON COLUMN public.oidc_clients.allowed_scopes IS '允许申请的 scope JSON 数组（openid / profile / email 子集，必含 openid）';
COMMENT ON COLUMN public.oidc_clients.require_pkce IS '是否强制 PKCE S256；public 恒为 true；confidential 仅管理员可关闭，关闭后授权请求必须带 nonce';
COMMENT ON COLUMN public.oidc_clients.secret_hash IS '当前 client secret 的 Argon2 PHC 串（明文只在创建/轮换时返回一次）';
COMMENT ON COLUMN public.oidc_clients.previous_secret_hash IS '轮换前旧 secret 的 Argon2 PHC 串（宽限期内仍可认证）';
COMMENT ON COLUMN public.oidc_clients.previous_secret_expires_at IS '旧 secret 失效时间（过后不再接受）';
COMMENT ON COLUMN public.oidc_clients.secret_rotated_at IS '最近一次轮换 secret 的时间';
COMMENT ON COLUMN public.oidc_clients.status IS '应用状态：pending / active / disabled / suspended / rejected';
COMMENT ON COLUMN public.oidc_clients.status_reason IS '管理员审批/封禁备注（可空）';
COMMENT ON COLUMN public.oidc_clients.reviewed_by IS '最近一次审批/封禁的超级管理员';
COMMENT ON COLUMN public.oidc_clients.reviewed_at IS '最近一次审批/封禁时间';
COMMENT ON COLUMN public.oidc_clients.created_at IS '创建时间';
COMMENT ON COLUMN public.oidc_clients.updated_at IS '最后修改时间';

-- ── 第一方 SSO 浏览器会话 ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.oidc_browser_sessions (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL PRIMARY KEY,
    session_hash text NOT NULL UNIQUE,
    user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    auth_time timestamp with time zone NOT NULL,
    established_via text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    CONSTRAINT oidc_browser_sessions_established_via_check
        CHECK (established_via IN ('password_login', 'oidc_login', 'jwt_bridge'))
);

CREATE INDEX IF NOT EXISTS idx_oidc_browser_sessions_user ON public.oidc_browser_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_oidc_browser_sessions_expires ON public.oidc_browser_sessions (expires_at);

COMMENT ON TABLE public.oidc_browser_sessions IS 'FloatCTF 第一方 SSO 会话：浏览器以 HttpOnly Cookie 持有随机值，服务端只存摘要，可过期/撤销';
COMMENT ON COLUMN public.oidc_browser_sessions.id IS '主键';
COMMENT ON COLUMN public.oidc_browser_sessions.session_hash IS 'Cookie 随机值的 SHA-256 十六进制摘要';
COMMENT ON COLUMN public.oidc_browser_sessions.user_id IS '会话所属玩家（只允许 users，不含 super_admin）';
COMMENT ON COLUMN public.oidc_browser_sessions.auth_time IS '用户实际完成认证（输入密码）的时间，用于 ID Token auth_time / max_age';
COMMENT ON COLUMN public.oidc_browser_sessions.established_via IS '建立方式：password_login=平台登录接口，oidc_login=OIDC 登录页，jwt_bridge=已有 JWT 桥接';
COMMENT ON COLUMN public.oidc_browser_sessions.created_at IS '创建时间';
COMMENT ON COLUMN public.oidc_browser_sessions.last_seen_at IS '最近一次被授权流程使用的时间';
COMMENT ON COLUMN public.oidc_browser_sessions.expires_at IS '过期时间（过期后不再被识别）';
COMMENT ON COLUMN public.oidc_browser_sessions.revoked_at IS '撤销时间（登出后设置，非空即失效）';

-- ── 授权事务（登录/同意跨重定向）─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.oidc_auth_transactions (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL PRIMARY KEY,
    oidc_client_id uuid NOT NULL REFERENCES public.oidc_clients(id) ON DELETE CASCADE,
    browser_binding_hash text NOT NULL,
    csrf_hash text NOT NULL,
    redirect_uri text NOT NULL,
    scopes jsonb NOT NULL,
    state text,
    nonce text,
    code_challenge text,
    code_challenge_method text,
    prompt jsonb DEFAULT '[]'::jsonb NOT NULL,
    max_age integer,
    login_hint text,
    id_token_hint_sub text,
    user_id uuid REFERENCES public.users(id) ON DELETE CASCADE,
    session_id uuid REFERENCES public.oidc_browser_sessions(id) ON DELETE SET NULL,
    authenticated_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT oidc_auth_transactions_scopes_is_array CHECK (jsonb_typeof(scopes) = 'array'),
    CONSTRAINT oidc_auth_transactions_prompt_is_array CHECK (jsonb_typeof(prompt) = 'array'),
    CONSTRAINT oidc_auth_transactions_pkce_method CHECK (
        (code_challenge IS NULL AND code_challenge_method IS NULL)
        OR (code_challenge IS NOT NULL AND code_challenge_method = 'S256')
    )
);

CREATE INDEX IF NOT EXISTS idx_oidc_auth_transactions_expires ON public.oidc_auth_transactions (expires_at);

COMMENT ON TABLE public.oidc_auth_transactions IS 'OIDC 授权事务：/oauth/authorize 校验通过后的服务端状态，登录/同意页只引用事务 id，协议参数以此为准';
COMMENT ON COLUMN public.oidc_auth_transactions.id IS '事务 id（出现在登录/同意页 URL，需配合浏览器绑定 Cookie 与 CSRF token 才能推进）';
COMMENT ON COLUMN public.oidc_auth_transactions.oidc_client_id IS '发起授权的应用（oidc_clients.id）';
COMMENT ON COLUMN public.oidc_auth_transactions.browser_binding_hash IS '浏览器绑定 Cookie 值的 SHA-256 摘要（事务只能在发起它的浏览器内推进）';
COMMENT ON COLUMN public.oidc_auth_transactions.csrf_hash IS '登录/同意表单 CSRF token 的 SHA-256 摘要';
COMMENT ON COLUMN public.oidc_auth_transactions.redirect_uri IS '已与注册值精确匹配的回调地址';
COMMENT ON COLUMN public.oidc_auth_transactions.scopes IS '本次申请且被允许的 scope JSON 数组';
COMMENT ON COLUMN public.oidc_auth_transactions.state IS 'RP 传入的 state（原样回传）';
COMMENT ON COLUMN public.oidc_auth_transactions.nonce IS 'RP 传入的 nonce（写入 ID Token）';
COMMENT ON COLUMN public.oidc_auth_transactions.code_challenge IS 'PKCE code_challenge（S256）';
COMMENT ON COLUMN public.oidc_auth_transactions.code_challenge_method IS 'PKCE 方法，仅 S256';
COMMENT ON COLUMN public.oidc_auth_transactions.prompt IS 'prompt 参数解析结果 JSON 数组（login / consent / select_account）';
COMMENT ON COLUMN public.oidc_auth_transactions.max_age IS 'max_age 参数（秒）；超过则要求重新认证';
COMMENT ON COLUMN public.oidc_auth_transactions.login_hint IS 'login_hint（仅用于预填用户名，从不作为身份依据）';
COMMENT ON COLUMN public.oidc_auth_transactions.id_token_hint_sub IS 'id_token_hint 中经验签的 sub；与会话用户不一致时要求重新登录';
COMMENT ON COLUMN public.oidc_auth_transactions.user_id IS '在本事务中完成认证/确认的用户';
COMMENT ON COLUMN public.oidc_auth_transactions.session_id IS '本事务使用的 SSO 会话';
COMMENT ON COLUMN public.oidc_auth_transactions.authenticated_at IS '本事务内重新输入密码的时间（满足 prompt=login / max_age）';
COMMENT ON COLUMN public.oidc_auth_transactions.created_at IS '创建时间';
COMMENT ON COLUMN public.oidc_auth_transactions.expires_at IS '过期时间（默认 10 分钟）';
COMMENT ON COLUMN public.oidc_auth_transactions.completed_at IS '完成时间（发码或拒绝后设置，不可再推进）';

-- ── 授权码 ──────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.oidc_authorization_codes (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL PRIMARY KEY,
    code_hash text NOT NULL UNIQUE,
    oidc_client_id uuid NOT NULL REFERENCES public.oidc_clients(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    session_id uuid REFERENCES public.oidc_browser_sessions(id) ON DELETE SET NULL,
    redirect_uri text NOT NULL,
    scopes jsonb NOT NULL,
    nonce text,
    code_challenge text,
    code_challenge_method text,
    auth_time timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    CONSTRAINT oidc_authorization_codes_scopes_is_array CHECK (jsonb_typeof(scopes) = 'array'),
    CONSTRAINT oidc_authorization_codes_pkce_method CHECK (
        (code_challenge IS NULL AND code_challenge_method IS NULL)
        OR (code_challenge IS NOT NULL AND code_challenge_method = 'S256')
    )
);

CREATE INDEX IF NOT EXISTS idx_oidc_authorization_codes_expires ON public.oidc_authorization_codes (expires_at);

COMMENT ON TABLE public.oidc_authorization_codes IS 'OIDC 授权码：随机高熵、只存摘要、短期（≤120s）、原子一次性消费';
COMMENT ON COLUMN public.oidc_authorization_codes.id IS '主键';
COMMENT ON COLUMN public.oidc_authorization_codes.code_hash IS '授权码的 SHA-256 十六进制摘要';
COMMENT ON COLUMN public.oidc_authorization_codes.oidc_client_id IS '授权码绑定的应用（oidc_clients.id）';
COMMENT ON COLUMN public.oidc_authorization_codes.user_id IS '授权用户';
COMMENT ON COLUMN public.oidc_authorization_codes.session_id IS '发码时的 SSO 会话（可空）';
COMMENT ON COLUMN public.oidc_authorization_codes.redirect_uri IS '授权请求中的 redirect_uri，兑换时必须逐字一致';
COMMENT ON COLUMN public.oidc_authorization_codes.scopes IS '已授予的 scope JSON 数组';
COMMENT ON COLUMN public.oidc_authorization_codes.nonce IS '授权请求的 nonce（写入 ID Token）';
COMMENT ON COLUMN public.oidc_authorization_codes.code_challenge IS 'PKCE code_challenge；为空时兑换请求不得携带 code_verifier';
COMMENT ON COLUMN public.oidc_authorization_codes.code_challenge_method IS 'PKCE 方法，仅 S256';
COMMENT ON COLUMN public.oidc_authorization_codes.auth_time IS '用户认证时间（ID Token auth_time）';
COMMENT ON COLUMN public.oidc_authorization_codes.created_at IS '签发时间';
COMMENT ON COLUMN public.oidc_authorization_codes.expires_at IS '过期时间';
COMMENT ON COLUMN public.oidc_authorization_codes.consumed_at IS '兑换时间（非空即已使用，再次兑换视为重放）';

-- ── 用户授权记录 ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.oidc_consents (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    oidc_client_id uuid NOT NULL REFERENCES public.oidc_clients(id) ON DELETE CASCADE,
    scopes jsonb NOT NULL,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone,
    revoked_at timestamp with time zone,
    CONSTRAINT oidc_consents_user_client_unique UNIQUE (user_id, oidc_client_id),
    CONSTRAINT oidc_consents_scopes_is_array CHECK (jsonb_typeof(scopes) = 'array')
);

CREATE INDEX IF NOT EXISTS idx_oidc_consents_client ON public.oidc_consents (oidc_client_id);

COMMENT ON TABLE public.oidc_consents IS '用户对第三方应用的授权记录：(user, client) 唯一；撤销后既有 access token 立即失效';
COMMENT ON COLUMN public.oidc_consents.id IS '主键';
COMMENT ON COLUMN public.oidc_consents.user_id IS '授权用户';
COMMENT ON COLUMN public.oidc_consents.oidc_client_id IS '被授权的应用（oidc_clients.id）';
COMMENT ON COLUMN public.oidc_consents.scopes IS '已授权 scope JSON 数组（再次授权时取并集）';
COMMENT ON COLUMN public.oidc_consents.granted_at IS '首次授权时间（撤销后重新授权会重置）';
COMMENT ON COLUMN public.oidc_consents.updated_at IS '最近一次修改时间';
COMMENT ON COLUMN public.oidc_consents.last_used_at IS '最近一次基于该授权签发令牌的时间';
COMMENT ON COLUMN public.oidc_consents.revoked_at IS '用户撤销时间（非空即无效）';

-- ── 不透明 access token ────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.oidc_access_tokens (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL PRIMARY KEY,
    token_hash text NOT NULL UNIQUE,
    oidc_client_id uuid NOT NULL REFERENCES public.oidc_clients(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    authorization_code_id uuid REFERENCES public.oidc_authorization_codes(id) ON DELETE SET NULL,
    scopes jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    CONSTRAINT oidc_access_tokens_scopes_is_array CHECK (jsonb_typeof(scopes) = 'array')
);

CREATE INDEX IF NOT EXISTS idx_oidc_access_tokens_expires ON public.oidc_access_tokens (expires_at);
CREATE INDEX IF NOT EXISTS idx_oidc_access_tokens_user_client ON public.oidc_access_tokens (user_id, oidc_client_id);
CREATE INDEX IF NOT EXISTS idx_oidc_access_tokens_code ON public.oidc_access_tokens (authorization_code_id);

COMMENT ON TABLE public.oidc_access_tokens IS 'OIDC 不透明 access token：只用于 UserInfo，不是 FloatCTF API 凭证；库内只存摘要';
COMMENT ON COLUMN public.oidc_access_tokens.id IS '主键';
COMMENT ON COLUMN public.oidc_access_tokens.token_hash IS 'access token 的 SHA-256 十六进制摘要';
COMMENT ON COLUMN public.oidc_access_tokens.oidc_client_id IS '签发给的应用（oidc_clients.id）';
COMMENT ON COLUMN public.oidc_access_tokens.user_id IS '令牌代表的用户';
COMMENT ON COLUMN public.oidc_access_tokens.authorization_code_id IS '来源授权码（授权码重放时据此撤销）';
COMMENT ON COLUMN public.oidc_access_tokens.scopes IS '令牌 scope JSON 数组（决定 UserInfo 披露范围）';
COMMENT ON COLUMN public.oidc_access_tokens.created_at IS '签发时间';
COMMENT ON COLUMN public.oidc_access_tokens.expires_at IS '过期时间';
COMMENT ON COLUMN public.oidc_access_tokens.revoked_at IS '撤销时间（授权撤销/授权码重放/应用删除时设置）';
