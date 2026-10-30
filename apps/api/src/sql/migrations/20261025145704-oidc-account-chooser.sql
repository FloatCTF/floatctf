-- ================================================================================
-- Migration: 20261025145704-oidc-account-chooser
-- ================================================================================
--
-- OIDC 账号选择（prompt=select_account）：把「账号选择」从「授权同意」中拆出来，
-- 作为授权事务里一个显式、服务端持有的阶段（设计见 docs/identity/ADR-oidc-account-chooser.md）。
--
-- 只新增可空列：旧事务行全部为 NULL（= 未选择、未请求切换），旧版本 API 读写本表不受影响。
-- 外键 ON DELETE SET NULL：会话被清理时选择随之失效（SET NULL 后视为未选择），绝不级联删除事务。
--
-- 幂等：ADD COLUMN IF NOT EXISTS / 约束按名存在性检查，可重复执行。

ALTER TABLE public.oidc_auth_transactions
    ADD COLUMN IF NOT EXISTS account_choice_session_id uuid,
    ADD COLUMN IF NOT EXISTS account_selected_at timestamp with time zone,
    ADD COLUMN IF NOT EXISTS account_selected_session_id uuid,
    ADD COLUMN IF NOT EXISTS account_switch_requested_at timestamp with time zone;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'oidc_auth_transactions_account_choice_session_id_fkey'
    ) THEN
        ALTER TABLE public.oidc_auth_transactions
            ADD CONSTRAINT oidc_auth_transactions_account_choice_session_id_fkey
            FOREIGN KEY (account_choice_session_id)
            REFERENCES public.oidc_browser_sessions(id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'oidc_auth_transactions_account_selected_session_id_fkey'
    ) THEN
        ALTER TABLE public.oidc_auth_transactions
            ADD CONSTRAINT oidc_auth_transactions_account_selected_session_id_fkey
            FOREIGN KEY (account_selected_session_id)
            REFERENCES public.oidc_browser_sessions(id) ON DELETE SET NULL;
    END IF;
    -- 选择时间与被选会话必须同时存在或同时为空（会话被清理导致 SET NULL 的情况除外，
    -- 因此只约束「有会话必有时间」这一方向）。
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'oidc_auth_transactions_account_selection_pair'
    ) THEN
        ALTER TABLE public.oidc_auth_transactions
            ADD CONSTRAINT oidc_auth_transactions_account_selection_pair
            CHECK (account_selected_session_id IS NULL OR account_selected_at IS NOT NULL);
    END IF;
END
$$;

COMMENT ON COLUMN public.oidc_auth_transactions.account_choice_session_id IS '账号选择页最近一次展示给用户的 SSO 会话；「继续使用此账号」必须与之相同（防止另一标签页切换账号后串号）';
COMMENT ON COLUMN public.oidc_auth_transactions.account_selected_at IS '用户在本事务内确认账号的时间（选择页「继续」或本事务内输入密码登录）';
COMMENT ON COLUMN public.oidc_auth_transactions.account_selected_session_id IS '本事务内被用户确认的 SSO 会话（只认会话 id，用户由会话决定，从不信任表单传入的用户 ID）';
COMMENT ON COLUMN public.oidc_auth_transactions.account_switch_requested_at IS '用户在本事务内选择「使用其他账号」的时间；此后旧 SSO 会话不能免密完成本事务';
