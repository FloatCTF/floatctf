-- ================================================================================
-- Migration: 20261011230333-drop-super-admin-table
-- M6（Wave 10）：删除 legacy 的 `super_admin` 表 —— 双身份模型的最后一个物证。
-- ================================================================================
--
-- 背景：v1 把「选手」与「管理员」建成两张并行账号表（`users` / `super_admin`），
-- 于是同一个人可能有两行、两套口令、两个主键，权限也只能靠"在哪张表里"区分。
-- Wave 0–5 已把身份统一到 `users` + 平台 RBAC：M3 逐字节迁移了口令哈希，
-- M4/M5 把 6 个 actor 外键改指 `users` 并统一了审计主体。
-- 至此该表已无任何读取方，本迁移把它删掉。
--
-- 这是一次**不可逆**的破坏性变更，因此前置校验必须 fail closed：
-- 只要还有外键指向它、或有账号没进 `users`，就明确报错并列出具体对象，
-- 而不是让 DROP 隐式失败、或用 CASCADE 把别人的数据一起删掉。
--
-- 文件内禁止 BEGIN/COMMIT/ROLLBACK（由 migrate.sh 统一包裹事务）；语句幂等。

-- ---------------------------------------------------------------------------
-- 1) 前置校验：不得有任何外键指向 super_admin
--
-- 与 `apps/api/tests/rbac_actor_fk_contract.rs::no_fk_targets_super_admin_anymore`
-- 是同一不变式的两层保险：测试在 CI 阶段挡住，这里在**执行阶段**挡住。
-- 只靠测试是不够的 —— 生产升级时跑的只有迁移文件本身。
-- ---------------------------------------------------------------------------
DO $m6_fk_precheck$
DECLARE
    offending text;
BEGIN
    SELECT string_agg(
               format('%s.%s', c.conrelid::regclass::text, c.conname),
               ', ' ORDER BY c.conrelid::regclass::text, c.conname
           )
      INTO offending
      FROM pg_constraint c
      JOIN pg_class t ON t.oid = c.confrelid
      JOIN pg_namespace n ON n.oid = t.relnamespace
     WHERE c.contype = 'f'
       AND n.nspname = 'public'
       AND t.relname = 'super_admin';

    IF offending IS NOT NULL THEN
        RAISE EXCEPTION
            'M6 中止：仍有外键指向 public.super_admin（%）。请先用一个新迁移把这些外键改指 users，再删除本表；不要用 CASCADE 绕过 —— 那会连带删除引用方数据。',
            offending;
    END IF;
END
$m6_fk_precheck$;

-- ---------------------------------------------------------------------------
-- 2) 前置校验：表存在时，它的账号必须都已在 users 里
--
-- M3 迁移过口令哈希，但那是"当时"的事实。若之后有人手工往 super_admin 插过行
-- （例如从旧备份恢复），这里必须拦住 —— 否则 DROP 会静默丢掉一个**真实存在过**
-- 的管理员账号，而那种丢失是不可逆的。
-- ---------------------------------------------------------------------------
DO $m6_orphan_precheck$
DECLARE
    orphan_count bigint;
    sample text;
BEGIN
    IF to_regclass('public.super_admin') IS NULL THEN
        -- 表已不存在：本迁移幂等，直接返回。
        RETURN;
    END IF;

    EXECUTE $q$
        SELECT count(*), string_agg(username, ', ' ORDER BY username)
          FROM public.super_admin s
         WHERE NOT EXISTS (
                   SELECT 1 FROM public.users u WHERE u.username = s.username
               )
    $q$
    INTO orphan_count, sample;

    IF orphan_count > 0 THEN
        RAISE EXCEPTION
            'M6 中止：super_admin 中有 % 个账号在 users 里没有同名行（例如：%）。直接 DROP 会丢失这些账号；请先用一个新迁移把它们补进 users（含口令哈希）。',
            orphan_count, COALESCE(sample, '(未知)');
    END IF;
END
$m6_orphan_precheck$;

-- ---------------------------------------------------------------------------
-- 3) 删除表
--
-- 不使用 CASCADE：前置校验已保证没有外键指向它，CASCADE 只会掩盖意外情况。
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS public.super_admin;

-- ---------------------------------------------------------------------------
-- 4) 终点校验：表必须确实消失
-- ---------------------------------------------------------------------------
DO $m6_postcheck$
BEGIN
    IF to_regclass('public.super_admin') IS NOT NULL THEN
        RAISE EXCEPTION 'M6 失败：public.super_admin 仍然存在';
    END IF;
END
$m6_postcheck$;
