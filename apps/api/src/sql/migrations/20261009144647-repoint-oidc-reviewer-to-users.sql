-- ================================================================================
-- Migration: 20261009144647-repoint-oidc-reviewer-to-users
-- 升级路径修复：在 M4 的「无外键指向 super_admin」盘点之前，先改指 OIDC 审核人外键
-- ================================================================================
--
-- 为什么需要**两个**同语义迁移（本文件与 20261026180000）：
--
-- * 老库（1.0.3 升级）：`oidc_clients` 已经存在且 `reviewed_by` 指向 `super_admin`。
--   紧随 M3（身份迁入 `users`）之后、M4（重指 6 个 actor 外键）之前必须把它改指 `users`，
--   否则 M4 的 fail-closed 盘点会因「仍有外键指向 super_admin」直接报错并回滚升级。
-- * 新库（fresh）：此刻 `oidc_clients` 还不存在（它在 20261023 才建），本文件是 no-op；
--   改指由 20261026180000 在 OIDC 建表之后完成，随后 20261027090000 才能删表。
--
-- 两个文件都是幂等的：约束已经是 `users` 时不会重复添加，表不存在时直接返回。
-- M3 逐字节保留 `super_admin.id` 作为 `users.id`，因此改指后旧数据仍然指向正确账号。
--
-- 文件内禁止 BEGIN/COMMIT/ROLLBACK（由 migrate.sh 统一包裹事务）；语句幂等。

-- 1) 改指外键：删掉指向 super_admin 的那条（如果存在），再补一条指向 users(id)。
DO $fc_repoint_reviewer$
DECLARE
    existing text;
BEGIN
    IF to_regclass('public.oidc_clients') IS NULL THEN
        RETURN;
    END IF;

    SELECT c.conname
      INTO existing
      FROM pg_constraint c
      JOIN pg_class t ON t.oid = c.confrelid
      JOIN pg_namespace n ON n.oid = t.relnamespace
     WHERE c.contype = 'f'
       AND c.conrelid = 'public.oidc_clients'::regclass
       AND n.nspname = 'public'
       AND t.relname = 'super_admin';

    IF existing IS NOT NULL THEN
        EXECUTE format('ALTER TABLE public.oidc_clients DROP CONSTRAINT %I', existing);
    END IF;

    IF NOT EXISTS (
        SELECT 1
          FROM pg_constraint
         WHERE conrelid = 'public.oidc_clients'::regclass
           AND conname = 'oidc_clients_reviewed_by_fkey'
    ) THEN
        ALTER TABLE public.oidc_clients
            ADD CONSTRAINT oidc_clients_reviewed_by_fkey
            FOREIGN KEY (reviewed_by) REFERENCES public.users(id) ON DELETE SET NULL;
    END IF;

    EXECUTE 'COMMENT ON COLUMN public.oidc_clients.reviewed_by IS ''审核该应用的管理员账号（users.id）；管理员是持有平台的账号，不再是独立身份表''';
END
$fc_repoint_reviewer$;
