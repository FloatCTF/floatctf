-- ================================================================================
-- Migration: 20261009143746-identity-account-columns-and-preflight
-- 统一账户支撑列 + super_admin → users 迁移的冲突预检
-- ================================================================================
--
-- 背景（Identity & Authorization v2, Wave 2）：
-- 统一账户模型需要两项账号级状态：
--   * status        —— 允许「禁用而不删除」（删除会把审计主体 ON DELETE SET NULL 掉，
--                      破坏审计可追踪性）；只有 active / disabled 两值，不做过度设计。
--   * token_version —— 凭据版本：改密 / 禁用 / 显式吊销时自增，使旧 Session JWT 立即失效。
--                      今天没有任何 logout 端点，不加这一列就是长期安全缺口。
--
-- 本迁移同时做**冲突预检**。后续 M3 会把 super_admin 行复制进 users，
-- 而 users 有三个唯一约束（username / nickname / email），super_admin 只有 username 唯一。
-- 预检必须**显式失败并列出冲突行**，绝不自动 rename —— 账号改名是需要人工决策的业务动作。
--
-- 预检失败的运维流程：
--   1. 该迁移整体回滚（事务），schema_migrations 不记录，前序已提交的保留；
--   2. 异常消息列出具体冲突行（username / email / id / nickname）；
--   3. 运维人工决策后用**显式 SQL** 解决（改玩家账号或改管理员账号），
--      或确认是同一人的重复账号后合并；
--   4. 重新运行 `mise run db:migration:apply`（幂等，只补失败那条及其后）。
--
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

-- ================================================================================
-- 1) 账号状态枚举
-- ================================================================================

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'account_status') THEN
    CREATE TYPE account_status AS ENUM ('active', 'disabled');
  END IF;
END $$;

COMMENT ON TYPE account_status IS '账号状态：active=可登录；disabled=已禁用（每请求校验，返回 403）';

-- ================================================================================
-- 2) users 新增列
-- ================================================================================

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS status account_status NOT NULL DEFAULT 'active';

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS token_version integer NOT NULL DEFAULT 0;

COMMENT ON COLUMN users.status IS '账号状态：active=可登录，disabled=已禁用（AccountGuard 每请求校验）';
COMMENT ON COLUMN users.token_version IS '会话版本：改密/禁用/吊销时自增，使旧 Session JWT 全部失效（与 RBAC 无关）';

-- 按状态筛选是管理端与守卫的高频路径
CREATE INDEX IF NOT EXISTS idx_users_status ON users (status);

-- ================================================================================
-- 3) 迁移冲突预检（fail closed，绝不自动 rename）
-- ================================================================================
--
-- 逐条断言 super_admin → users 复制的前置条件。任一不满足即 RAISE EXCEPTION，
-- 整个迁移回滚。消息里带**具体冲突行**，便于运维直接定位。

DO $$
DECLARE
  n integer;
  detail text;
BEGIN
  -- ── 3.1 username 冲突（users.username 唯一）──
  SELECT count(*), string_agg(format('super_admin.username=%L ↔ users.username=%L (users.id=%s)',
                                     s.username, u.username, u.id), '; ' ORDER BY s.username)
    INTO n, detail
    FROM super_admin s
    JOIN users u ON u.username = s.username;

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_PREFLIGHT[username]: % 个用户名的玩家账号与管理员账号重复，需人工决策（不会自动改名）：%',
      n, detail;
  END IF;

  -- ── 3.2 email 冲突（users.email 有唯一索引）──
  SELECT count(*), string_agg(format('super_admin.email=%L ↔ users.email=%L (users.id=%s)',
                                     s.email, u.email, u.id), '; ' ORDER BY s.email)
    INTO n, detail
    FROM super_admin s
    JOIN users u ON u.email = s.email;

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_PREFLIGHT[email]: % 个邮箱的玩家账号与管理员账号重复，需人工决策：%',
      n, detail;
  END IF;

  -- ── 3.3 super_admin 内部 email 重复（合并后同样违反 users.email 唯一）──
  SELECT count(*), string_agg(format('super_admin email=%L 出现在 id=%s 与 id=%s',
                                     a.email, a.id, b.id), '; ' ORDER BY a.email)
    INTO n, detail
    FROM super_admin a
    JOIN super_admin b ON a.email = b.email AND a.id < b.id;

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_PREFLIGHT[email-internal]: super_admin 内部有 % 对重复邮箱，合并后会违反 users.email 唯一约束：%',
      n, detail;
  END IF;

  -- ── 3.4 UUID 冲突（两个 uuid_generate_v4 空间，理论上不会，但必须断言）──
  SELECT count(*), string_agg(format('id=%s 同时存在于 super_admin 与 users', s.id), '; ')
    INTO n, detail
    FROM super_admin s
    JOIN users u ON u.id = s.id;

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_PREFLIGHT[id]: % 个主键同时存在于 super_admin 与 users，合并会产生身份歧义：%',
      n, detail;
  END IF;

  -- ── 3.5 nickname 冲突（users.nickname 唯一；super_admin 无 nickname，用 username 合成）──
  SELECT count(*), string_agg(format('super_admin.username=%L 与 users.nickname=%L 冲突 (users.id=%s)',
                                     s.username, u.nickname, u.id), '; ' ORDER BY s.username)
    INTO n, detail
    FROM super_admin s
    JOIN users u ON u.nickname = s.username;

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_PREFLIGHT[nickname]: % 个管理员用户名与既有玩家昵称重复（昵称由用户名合成），需人工决策：%',
      n, detail;
  END IF;
END $$;
