-- ================================================================================
-- Migration: 20261025201938-unified-identity-sessions
-- ================================================================================
--
-- 统一身份会话（设计见 docs/identity/ADR-unified-identity.md）：
-- 1. oidc_browser_containers：浏览器账号容器。浏览器只持有一个 HttpOnly Cookie（__Host-fctf_accounts），
--    服务端只存其摘要；容器内可挂多个已认证账号的身份会话（oidc_browser_sessions 行），
--    并记录 FloatCTF 主站当前活跃的那一个（web_session_id）。
-- 2. oidc_browser_sessions 增加 container_id / sid / revoked_reason：
--    sid 是对外（ID Token、Back-Channel Logout）稳定的会话标识，与 Cookie 秘密、行主键无关。
--
-- 只新增表与列：旧版本 API 不读写新列，回滚到旧二进制不受影响；旧会话行自动获得随机 sid。
-- 幂等：IF NOT EXISTS / 约束按名存在性检查，可重复执行。

CREATE TABLE IF NOT EXISTS public.oidc_browser_containers (
    id uuid PRIMARY KEY,
    secret_hash text NOT NULL UNIQUE,
    web_session_id uuid,
    web_signed_out_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    last_seen_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone
);

CREATE INDEX IF NOT EXISTS idx_oidc_browser_containers_expires
    ON public.oidc_browser_containers (expires_at);

COMMENT ON TABLE public.oidc_browser_containers IS '浏览器账号容器：浏览器以 HttpOnly Cookie 持有随机值（只存摘要），容器内挂本浏览器已认证的多个账号会话';
COMMENT ON COLUMN public.oidc_browser_containers.id IS '主键';
COMMENT ON COLUMN public.oidc_browser_containers.secret_hash IS 'Cookie 随机值的 SHA-256 十六进制摘要（每次口令认证轮换）';
COMMENT ON COLUMN public.oidc_browser_containers.web_session_id IS 'FloatCTF 主站在本浏览器当前活跃的账号会话；为空表示主站未登录（RP 授权选择账号不会改动它）';
COMMENT ON COLUMN public.oidc_browser_containers.web_signed_out_at IS '主站最近一次明确退出的时间：早于该时间认证的会话不会被主站自动恢复';
COMMENT ON COLUMN public.oidc_browser_containers.created_at IS '创建时间';
COMMENT ON COLUMN public.oidc_browser_containers.last_seen_at IS '最近一次被使用的时间';
COMMENT ON COLUMN public.oidc_browser_containers.expires_at IS '容器过期时间（滑动续期；过期后容器及其会话都不再被识别）';
COMMENT ON COLUMN public.oidc_browser_containers.revoked_at IS '撤销时间（退出本浏览器全部账号后设置，非空即失效）';

ALTER TABLE public.oidc_browser_sessions
    ADD COLUMN IF NOT EXISTS container_id uuid,
    ADD COLUMN IF NOT EXISTS sid text,
    ADD COLUMN IF NOT EXISTS revoked_reason text;

UPDATE public.oidc_browser_sessions
   SET sid = replace(gen_random_uuid()::text, '-', '')
 WHERE sid IS NULL;

ALTER TABLE public.oidc_browser_sessions
    ALTER COLUMN sid SET DEFAULT replace(gen_random_uuid()::text, '-', ''),
    ALTER COLUMN sid SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_oidc_browser_sessions_sid
    ON public.oidc_browser_sessions (sid);
CREATE INDEX IF NOT EXISTS idx_oidc_browser_sessions_container
    ON public.oidc_browser_sessions (container_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'oidc_browser_sessions_container_id_fkey'
    ) THEN
        ALTER TABLE public.oidc_browser_sessions
            ADD CONSTRAINT oidc_browser_sessions_container_id_fkey
            FOREIGN KEY (container_id)
            REFERENCES public.oidc_browser_containers(id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'oidc_browser_containers_web_session_id_fkey'
    ) THEN
        ALTER TABLE public.oidc_browser_containers
            ADD CONSTRAINT oidc_browser_containers_web_session_id_fkey
            FOREIGN KEY (web_session_id)
            REFERENCES public.oidc_browser_sessions(id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'oidc_browser_sessions_revoked_reason_check'
    ) THEN
        ALTER TABLE public.oidc_browser_sessions
            ADD CONSTRAINT oidc_browser_sessions_revoked_reason_check
            CHECK (revoked_reason IS NULL OR revoked_reason IN (
                'logout', 'logout_browser', 'global_logout', 'replaced', 'evicted', 'legacy_logout'
            ));
    END IF;
END
$$;

COMMENT ON COLUMN public.oidc_browser_sessions.container_id IS '所属浏览器账号容器（旧版单会话 Cookie 建立的会话为空，首次访问时迁入容器）';
COMMENT ON COLUMN public.oidc_browser_sessions.sid IS '对外稳定会话标识：写入 ID Token sid 与 Back-Channel Logout Token；与 Cookie 秘密无关，不能用来登录';
COMMENT ON COLUMN public.oidc_browser_sessions.revoked_reason IS '撤销原因：logout=退出该账号，logout_browser=退出本浏览器全部账号，global_logout=退出所有关联应用，replaced=同一账号重新登录替换，evicted=超出容器账号上限，legacy_logout=旧接口退出';
