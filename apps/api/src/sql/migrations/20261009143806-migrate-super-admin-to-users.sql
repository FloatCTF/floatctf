-- ================================================================================
-- Migration: 20261009143806-migrate-super-admin-to-users
-- 把 super_admin 账号复制进统一 users 表，并授予内置 super-admin 角色
-- ================================================================================
--
-- 背景（Identity & Authorization v2, Wave 2）：
-- 「User 是身份，不是角色」——管理员不再是一种账号类型，而是持有平台角色的普通账号。
-- 本迁移把 super_admin 行**复制**进 users（保留主键，从而让全部 actor 外键无需改数据），
-- 并给每个旧超管授予 platform_roles 里的内置 `super-admin`。
--
-- 关键约束：
--   * **保留 password hash 的逐字节原值**，不改口令、不 rehash；
--   * 保留 created_at / updated_at 原值；
--   * super_admin 没有 nickname → 用 username 合成（M2 已断言不与既有 nickname 冲突），
--     avatar 为 NULL，status=active，token_version=0；
--   * **本迁移不删除 super_admin 行**。守卫与代码此时仍读 super_admin，
--     因此本 Wave 是纯加法、零行为变化，可独立验证与回滚。
--   * `granted_by` 为 NULL：迁移不是人类发起的授予（且授予人尚不存在）。
--
-- 幂等：全部 INSERT ... ON CONFLICT DO NOTHING；可重复执行。
--
-- M2 已做过预检，但 M2 与 M3 之间存在时间窗（dev 下 API 可能仍在运行）。
-- 因此这里在**同一事务内**复检一次，保证「校验 → 复制」之间没有写入窗口。
--
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

-- ================================================================================
-- 1) 复检（与 M2 相同的四条断言 + nickname）
-- ================================================================================

DO $$
DECLARE
  n integer;
  detail text;
BEGIN
  SELECT count(*), string_agg(format('super_admin.username=%L ↔ users.username=%L',
                                     s.username, u.username), '; ')
    INTO n, detail
    FROM super_admin s JOIN users u ON u.username = s.username;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_PREFLIGHT[username] (recheck): % 处冲突：%', n, detail;
  END IF;

  SELECT count(*), string_agg(format('super_admin.email=%L ↔ users.email=%L',
                                     s.email, u.email), '; ')
    INTO n, detail
    FROM super_admin s JOIN users u ON u.email = s.email;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_PREFLIGHT[email] (recheck): % 处冲突：%', n, detail;
  END IF;

  SELECT count(*) INTO n
    FROM super_admin a JOIN super_admin b ON a.email = b.email AND a.id < b.id;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_PREFLIGHT[email-internal] (recheck): % 对重复邮箱', n;
  END IF;

  SELECT count(*) INTO n
    FROM super_admin s JOIN users u ON u.id = s.id;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_PREFLIGHT[id] (recheck): % 个主键冲突', n;
  END IF;

  SELECT count(*), string_agg(format('super_admin.username=%L ↔ users.nickname=%L',
                                     s.username, u.nickname), '; ')
    INTO n, detail
    FROM super_admin s JOIN users u ON u.nickname = s.username;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_PREFLIGHT[nickname] (recheck): % 处冲突：%', n, detail;
  END IF;
END $$;

-- ================================================================================
-- 2) 内置 super-admin 角色（固定主键）
-- ================================================================================
--
-- 固定 UUID 与 `core::system_ids::PLATFORM_ROLE_SUPER_ADMIN` 保持一致
-- （`Uuid::from_u128(0)`）；「最后一个超管保护」的并发锁点固定在这一行上。
-- 数值与 sysadmin 的 users.id 相同，但**分属两张表**（system_ids.rs 顶部已声明该约定）。

INSERT INTO platform_roles (id, key, name, description, protected, grants_all)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'super-admin',
    'Super Admin',
    '平台全权角色：拥有全部权限（含将来新增的权限）。系统内置，不可删除。',
    true,
    true
)
ON CONFLICT (id) DO NOTHING;

-- ================================================================================
-- 3) 账号复制（保留主键 / 口令哈希 / 时间戳）
-- ================================================================================

INSERT INTO users (
    id, username, nickname, password, email,
    created_at, updated_at, avatar,
    status, token_version
)
SELECT
    s.id,
    s.username,
    btrim(s.username),          -- nickname 合成（M2 已断言不与既有 nickname 冲突）
    s.password,                 -- ★ 逐字节保留 argon2id 哈希，绝不 rehash
    s.email,                    -- ★ M2 已断言唯一
    s.created_at,               -- ★ 保留原始创建时间
    s.updated_at,               -- ★ 保留原始更新时间
    NULL,                       -- super_admin 没有 avatar
    'active'::account_status,   -- 新列默认值
    0                           -- 新列默认值
FROM super_admin s
WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.id = s.id)
ON CONFLICT (id) DO NOTHING;

-- ================================================================================
-- 4) 授予内置 super-admin 角色
-- ================================================================================

INSERT INTO user_platform_roles (user_id, role_id, granted_by, granted_at)
SELECT
    s.id,
    '00000000-0000-0000-0000-000000000000'::uuid,
    NULL,                       -- 迁移不是人类发起的授予
    now()
FROM super_admin s
ON CONFLICT (user_id, role_id) DO NOTHING;

-- ================================================================================
-- 5) 自检：复制后必须「每个旧超管都在 users 里且持有 super-admin 角色」
-- ================================================================================

DO $$
DECLARE
  n integer;
  detail text;
BEGIN
  SELECT count(*), string_agg(format('super_admin.id=%s username=%L', s.id, s.username), '; ')
    INTO n, detail
    FROM super_admin s
    LEFT JOIN users u ON u.id = s.id
   WHERE u.id IS NULL;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_SELFCHECK[copy]: % 个超管未被复制进 users：%', n, detail;
  END IF;

  SELECT count(*), string_agg(format('super_admin.id=%s username=%L', s.id, s.username), '; ')
    INTO n, detail
    FROM super_admin s
    LEFT JOIN user_platform_roles upr
           ON upr.user_id = s.id
          AND upr.role_id = '00000000-0000-0000-0000-000000000000'::uuid
   WHERE upr.user_id IS NULL;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_SELFCHECK[grant]: % 个超管未持有 super-admin 角色：%', n, detail;
  END IF;

  -- 口令哈希必须逐字节一致（防止未来有人在这条 INSERT 里"顺手 rehash"）
  SELECT count(*), string_agg(format('id=%s', s.id), '; ')
    INTO n, detail
    FROM super_admin s
    JOIN users u ON u.id = s.id
   WHERE u.password IS DISTINCT FROM s.password;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_SELFCHECK[password]: % 个账号的口令哈希在迁移中被改动：%', n, detail;
  END IF;
END $$;
