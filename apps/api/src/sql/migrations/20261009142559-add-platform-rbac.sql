-- ================================================================================
-- Migration: 20261009142559-add-platform-rbac
-- 平台 RBAC 数据模型：platform_roles / user_platform_roles / platform_role_permissions
-- ================================================================================
--
-- 背景（Identity & Authorization v2, Wave 1）：
-- 平台此前没有 RBAC —— 授权完全等于 JWT 的 `Role::SuperAdmin`，任何超管可以做任何事。
-- 本迁移只**新增**三张表，不改变任何既有行为：
--   * 不插入任何角色行（`super-admin` 由后续迁移在授予动作同事务内插入，保证 FK 满足）
--   * 不触碰 users / super_admin / logs / announcements
--   * 权限字符串的**权威是代码 enum**（`core::permission::Permission`），
--     DB 只存 text；写路径必须经 `Permission::from_str` 校验，
--     因此这里不建 permission catalog 表（避免第二真相源）。
--
-- 幂等：全部 CREATE ... IF NOT EXISTS / DO 块判存；可重复执行。
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

-- ================================================================================
-- platform_roles：平台角色
-- ================================================================================

CREATE TABLE IF NOT EXISTS platform_roles (
    id uuid PRIMARY KEY DEFAULT public.uuid_generate_v4(),
    key text NOT NULL,
    name text NOT NULL,
    description text NOT NULL DEFAULT '',
    protected boolean NOT NULL DEFAULT false,
    grants_all boolean NOT NULL DEFAULT false,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

COMMENT ON TABLE platform_roles IS '平台角色：账号可被授予的平台管理能力集合（RBAC）';
COMMENT ON COLUMN platform_roles.id IS '主键（内置角色使用 core::system_ids 里的固定 UUID）';
COMMENT ON COLUMN platform_roles.key IS '稳定标识：唯一、创建后不可改；权限判定与审计只用 key';
COMMENT ON COLUMN platform_roles.name IS '展示名（可改，不影响授权语义）';
COMMENT ON COLUMN platform_roles.description IS '角色说明';
COMMENT ON COLUMN platform_roles.protected IS '系统内置角色：禁止删除，且 key/grants_all/权限集不可修改';
COMMENT ON COLUMN platform_roles.grants_all IS '全权标记：解析时短路返回全部权限（含将来新增），仅供 super-admin';
COMMENT ON COLUMN platform_roles.created_at IS '创建时间';
COMMENT ON COLUMN platform_roles.updated_at IS '更新时间';

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'platform_roles_key_key'
  ) THEN
    ALTER TABLE platform_roles ADD CONSTRAINT platform_roles_key_key UNIQUE (key);
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'platform_roles_key_format'
  ) THEN
    ALTER TABLE platform_roles ADD CONSTRAINT platform_roles_key_format
      CHECK (key ~ '^[a-z][a-z0-9-]{1,63}$');
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'platform_roles_name_not_blank'
  ) THEN
    ALTER TABLE platform_roles ADD CONSTRAINT platform_roles_name_not_blank
      CHECK (length(btrim(name)) > 0);
  END IF;
END $$;

-- updated_at 自动维护（复用 baseline 既有的通用触发器函数）
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'trg_platform_roles_updated_at') THEN
    CREATE TRIGGER trg_platform_roles_updated_at
      BEFORE UPDATE ON platform_roles
      FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();
  END IF;
END $$;

-- ================================================================================
-- user_platform_roles：账号 ↔ 角色授予
-- ================================================================================

CREATE TABLE IF NOT EXISTS user_platform_roles (
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role_id uuid NOT NULL REFERENCES platform_roles (id) ON DELETE CASCADE,
    granted_by uuid NULL REFERENCES users (id) ON DELETE SET NULL,
    granted_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, role_id)
);

COMMENT ON TABLE user_platform_roles IS '账号平台角色授予：一个账号可持多个角色，权限取并集';
COMMENT ON COLUMN user_platform_roles.user_id IS '被授予账号 ID（users.id；账号删除时级联删除授予）';
COMMENT ON COLUMN user_platform_roles.role_id IS '角色 ID（角色删除时级联删除授予）';
COMMENT ON COLUMN user_platform_roles.granted_by IS '授予人账号 ID（迁移产生的内置授予为 NULL；授予人删除时置空）';
COMMENT ON COLUMN user_platform_roles.granted_at IS '授予时间';

-- 反向查询（「这个角色被授予了哪些账号」/ 角色删除时的级联）
CREATE INDEX IF NOT EXISTS idx_user_platform_roles_role
    ON user_platform_roles (role_id);

-- ================================================================================
-- platform_role_permissions：角色 ↔ 权限
-- ================================================================================
--
-- `permission` 是**稳定字符串**（形如 `events.manage`），其权威定义在代码
-- `core::permission::Permission`；本表不建 FK 到 permission catalog（不存在该表）。
-- 写路径必须 `Permission::from_str` 校验；启动时检测无法解析的 orphan 字符串并 WARN，
-- 不静默删除（删除权限是安全相关动作，必须显式）。

CREATE TABLE IF NOT EXISTS platform_role_permissions (
    role_id uuid NOT NULL REFERENCES platform_roles (id) ON DELETE CASCADE,
    permission text NOT NULL,
    PRIMARY KEY (role_id, permission)
);

COMMENT ON TABLE platform_role_permissions IS '角色权限集合：权限取并集，无 deny 语义';
COMMENT ON COLUMN platform_role_permissions.role_id IS '角色 ID（角色删除时级联删除）';
COMMENT ON COLUMN platform_role_permissions.permission IS '权限稳定字符串（代码 Permission enum 为权威；格式 <domain>.<action>）';

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'platform_role_permissions_format'
  ) THEN
    ALTER TABLE platform_role_permissions ADD CONSTRAINT platform_role_permissions_format
      CHECK (permission ~ '^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$');
  END IF;
END $$;
