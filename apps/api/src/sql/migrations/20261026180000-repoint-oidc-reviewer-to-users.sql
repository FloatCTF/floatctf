-- ================================================================================
-- Migration: 20261026180000-repoint-oidc-reviewer-to-users
-- 合并修复：OIDC 应用审核人外键从 legacy `super_admin` 改指统一账号 `users`
-- ================================================================================
--
-- 背景：两条并行开发线合并时暴露出一个 schema **顺序**冲突。
--   * 授权 v2（本分支）把身份统一到 `users`，并删除 `super_admin`；
--   * OIDC Provider（main）在 `oidc_clients.reviewed_by` 上继续引用 `super_admin`。
-- 先删表再建 OIDC 表 → fresh database 的 `CREATE TABLE` 因目标表不存在而失败；
-- 先建 OIDC 表再删表 → 删表迁移的 fail-closed 前置校验因残留外键而拒绝执行。
-- 因此本迁移把该外键改指 `users`，删表迁移随后执行（20261027090000）。
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
