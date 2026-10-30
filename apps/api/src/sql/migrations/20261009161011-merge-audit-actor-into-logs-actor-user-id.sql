-- ================================================================================
-- Migration: 20261009161011-merge-audit-actor-into-logs-actor-user-id
-- 审计主体统一：logs.user_id + logs.superadmin_id → logs.actor_user_id
-- ================================================================================
--
-- 背景（Identity & Authorization v2, Wave 6）：
-- 统一账户后「管理员」不再是独立账号类型，因此审计日志也不该再把主体**分列**成
-- 「选手 `user_id`」与「管理员 `superadmin_id`」——两者现在是**同一个身份空间**
-- （M3 已把 `super_admin` 行按原主键复制进 `users`，M4 已把 6 个 actor 外键指向 `users`；
-- 本迁移是这轮统一的最后一块）。
--
-- ## 为什么分列本身就是问题
--
-- 分列让「谁做了这件事」有两个可能的答案，于是每个读取方都要自己决定怎么合并
-- （`COALESCE`？优先取哪个？），而**写入方可以只填一个**。结果是审计查询容易出现
-- 「按 user_id 过滤漏掉管理动作」这类静默缺失 —— 审计日志漏掉本身就是安全事故。
--
-- ## 语义损失与补偿（必须明确记录）
--
-- 分列确实承载了一个信息：「这次动作是**选手行为**还是**平台管理行为**」。
-- 合并后该信息**不再由主体列表达**，改由 `category` + `action` 表达
-- （现有调用点已经在用 `AUTH` / `SUPER_ADMIN` / `ROLES` / `ANNOUNCEMENTS` 等 category）。
-- 这是有意的权衡：主体列只回答「谁」，行为域由 category 回答，两者不再互相污染。
--
-- ## 为什么可以安全回填
--
-- `super_admin` 与 `users` 在 M3 之后共享同一主键空间，且 M4 之后
-- `logs.superadmin_id` 指向的 id 一定也在 `users` 里（M3 复制了全部超管）。
-- 因此 `COALESCE(superadmin_id, user_id)` 的取值一定满足新的
-- `actor_user_id → users(id)` 外键。回填完成后本迁移会用自检断言这个不变式。
--
-- ## 幂等
--
-- 全部 `ADD COLUMN IF NOT EXISTS` / `DROP CONSTRAINT IF EXISTS` / `DROP COLUMN IF EXISTS`，
-- 且回填带 `WHERE actor_user_id IS NULL`。重复执行结果相同。
--
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

-- ================================================================================
-- 1) 新列
-- ================================================================================

ALTER TABLE logs
    ADD COLUMN IF NOT EXISTS actor_user_id uuid NULL;

COMMENT ON COLUMN logs.actor_user_id IS '审计主体账号 ID（users.id；NULL=系统动作或主体账号已注销）';

-- ================================================================================
-- 2) 回填（COALESCE：管理动作原本落在 superadmin_id，选手动作落在 user_id）
-- ================================================================================

UPDATE logs
   SET actor_user_id = COALESCE(superadmin_id, user_id)
 WHERE actor_user_id IS NULL
   AND COALESCE(superadmin_id, user_id) IS NOT NULL;

-- ================================================================================
-- 3) 回填自检：任何非空 actor_user_id 都必须在 users 里存在
-- ================================================================================
--
-- 若这里失败，说明存在「既不在 users 也不在 super_admin 的主体」这种脏数据，
-- 直接添加外键会失败且难以定位。先显式报错并列出具体行。

DO $$
DECLARE
  n integer;
  detail text;
BEGIN
  SELECT count(*), string_agg(format('logs.id=%s actor_user_id=%s', l.id, l.actor_user_id), '; ')
    INTO n, detail
    FROM logs l
   WHERE l.actor_user_id IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM users u WHERE u.id = l.actor_user_id);

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_SELFCHECK[logs-backfill]: % 行回填出的 actor_user_id 在 users 中不存在：%',
      n, detail;
  END IF;
END $$;

-- ================================================================================
-- 4) 新外键（与其它 actor 列一致：ON DELETE SET NULL）
-- ================================================================================
--
-- 用 SET NULL 而不是 CASCADE：删除账号绝不能删除审计流水。
-- 主体注销后该行保留，actor_user_id 变 NULL，`category`/`action`/`message` 仍在。

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'logs_actor_user_id_fkey') THEN
    ALTER TABLE logs
      ADD CONSTRAINT logs_actor_user_id_fkey
      FOREIGN KEY (actor_user_id) REFERENCES users (id) ON DELETE SET NULL;
  END IF;
END $$;

-- ================================================================================
-- 5) 索引：替换旧的两列组合索引
-- ================================================================================
--
-- 旧索引 `idx_sys_logs_user_op (user_id, superadmin_id)` 随两列一起删除；
-- 新索引按单一主体列建（管理端「某账号做过什么」是高频审计查询）。

CREATE INDEX IF NOT EXISTS idx_sys_logs_actor ON logs (actor_user_id);

-- ================================================================================
-- 6) 删除旧列与旧外键
-- ================================================================================
--
-- 顺序：先删外键（否则 DROP COLUMN 会因依赖报错或用 CASCADE 隐式删约束，
-- 显式写出更可读），再删索引，最后删列。

ALTER TABLE logs DROP CONSTRAINT IF EXISTS logs_superadmin_id_fkey;
ALTER TABLE logs DROP CONSTRAINT IF EXISTS logs_user_id_fkey;
DROP INDEX IF EXISTS idx_sys_logs_user_op;
ALTER TABLE logs DROP COLUMN IF EXISTS superadmin_id;
ALTER TABLE logs DROP COLUMN IF EXISTS user_id;

-- ================================================================================
-- 7) 终点自检：两列必须已消失，新列与新外键必须存在
-- ================================================================================

DO $$
DECLARE
  n integer;
BEGIN
  SELECT count(*) INTO n
    FROM information_schema.columns
   WHERE table_schema = 'public' AND table_name = 'logs'
     AND column_name IN ('user_id', 'superadmin_id');
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_SELFCHECK[logs-drop]: 旧主体列仍存在（% 列）', n;
  END IF;

  SELECT count(*) INTO n
    FROM information_schema.columns
   WHERE table_schema = 'public' AND table_name = 'logs'
     AND column_name = 'actor_user_id';
  IF n <> 1 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_SELFCHECK[logs-add]: logs.actor_user_id 不存在';
  END IF;

  SELECT count(*) INTO n
    FROM pg_constraint
   WHERE conname = 'logs_actor_user_id_fkey' AND contype = 'f'
     AND confdeltype = 'n';
  IF n <> 1 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_SELFCHECK[logs-fk]: logs_actor_user_id_fkey 缺失或不是 ON DELETE SET NULL';
  END IF;
END $$;
