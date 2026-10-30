-- ================================================================================
-- Migration: 20261009144648-repoint-admin-actor-fks-to-users
-- 管理动作主体外键统一指向 users(id)
-- ================================================================================
--
-- 背景（Identity & Authorization v2, Wave 3）：
-- M3 已把 super_admin 账号复制进 users（**保留原主键**），因此本迁移只需把
-- 「管理动作主体」外键的**目标表**从 super_admin 改成 users —— 数据无需任何转换。
--
-- 覆盖 5 张表 / 6 个约束：
--   announcements.publisher_id              → users(id) ON DELETE SET NULL（+ DROP NOT NULL）
--   awd_internal_token_rotations.rotated_by → users(id) ON DELETE SET NULL
--   awd_score_events.created_by             → users(id) ON DELETE SET NULL
--   awd_team_bans.banned_by                 → users(id) ON DELETE SET NULL
--   awd_team_bans.unbanned_by               → users(id) ON DELETE SET NULL
--   awd_reset_records.requested_by_admin    → users(id) ON DELETE SET NULL
--
-- （logs.superadmin_id 不在这里改：它要并入 logs.actor_user_id，由 M5 处理。）
--
-- ## 唯一的语义变更：announcements.publisher_id
--
-- **决策（Q2 固化）**：从 `NOT NULL + ON DELETE CASCADE` 改为
-- `NULL + ON DELETE SET NULL`。理由：
--   * 删除/清理账号**不应该删除平台内容**；
--   * 统一账户后 publisher 是**历史 actor**，而不是公告的生命周期 owner；
--   * M4 之后 6 个 actor 列的删除语义**统一为 SET NULL**，不再有 CASCADE 例外
--     （原先 announcements 是唯一例外，且它恰好是最容易误伤数据的一个：
--      管理端批量删账号会连带删掉平台公告）。
--
-- `announcements.publisher text NOT NULL` 是发布时写入的**展示名快照**，保持不变；
-- `publisher_id IS NULL` 时前端展示「已注销账号」，但仍优先显示 snapshot。
--
-- ## 幂等
--
-- 每个约束都是 `DROP CONSTRAINT IF EXISTS` + `ADD CONSTRAINT`：
-- 重复执行等价于重建同名约束，结果相同。`ALTER COLUMN ... DROP NOT NULL`
-- 本身幂等。
--
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

-- ================================================================================
-- 1) announcements.publisher_id：重指向 + 可空化 + SET NULL
-- ================================================================================

ALTER TABLE announcements DROP CONSTRAINT IF EXISTS announcements_publisher_id_fkey;

-- 必须**先** DROP NOT NULL：否则 SET NULL 在删除账号时会失败
ALTER TABLE announcements ALTER COLUMN publisher_id DROP NOT NULL;

ALTER TABLE announcements
    ADD CONSTRAINT announcements_publisher_id_fkey
    FOREIGN KEY (publisher_id) REFERENCES users (id) ON DELETE SET NULL;

COMMENT ON COLUMN announcements.publisher_id IS '发布者账号 ID（users.id；NULL=发布者账号已注销，展示回落到 publisher 快照）';
COMMENT ON COLUMN announcements.publisher IS '发布时的发布者展示名快照（publisher_id 为空时仍可用于展示）';
COMMENT ON TABLE announcements IS '平台公告表（由持有 announcements.manage 权限的管理账号发布）';

-- ================================================================================
-- 2) AWD 内部令牌轮换：rotated_by
-- ================================================================================

ALTER TABLE awd_internal_token_rotations
    DROP CONSTRAINT IF EXISTS awd_internal_token_rotations_rotated_by_fkey;

ALTER TABLE awd_internal_token_rotations
    ADD CONSTRAINT awd_internal_token_rotations_rotated_by_fkey
    FOREIGN KEY (rotated_by) REFERENCES users (id) ON DELETE SET NULL;

COMMENT ON COLUMN awd_internal_token_rotations.rotated_by IS '轮换操作者账号 ID（users.id；NULL=账号已注销或系统自动轮换）';

-- ================================================================================
-- 3) AWD 分数事件：created_by
-- ================================================================================

ALTER TABLE awd_score_events DROP CONSTRAINT IF EXISTS awd_score_events_created_by_fkey;

ALTER TABLE awd_score_events
    ADD CONSTRAINT awd_score_events_created_by_fkey
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL;

COMMENT ON COLUMN awd_score_events.created_by IS '手工调分操作者账号 ID（users.id；NULL=系统计分，无人为发起者）';

-- ================================================================================
-- 4) AWD 队伍封禁：banned_by / unbanned_by
-- ================================================================================

ALTER TABLE awd_team_bans DROP CONSTRAINT IF EXISTS awd_team_bans_banned_by_fkey;
ALTER TABLE awd_team_bans
    ADD CONSTRAINT awd_team_bans_banned_by_fkey
    FOREIGN KEY (banned_by) REFERENCES users (id) ON DELETE SET NULL;

ALTER TABLE awd_team_bans DROP CONSTRAINT IF EXISTS awd_team_bans_unbanned_by_fkey;
ALTER TABLE awd_team_bans
    ADD CONSTRAINT awd_team_bans_unbanned_by_fkey
    FOREIGN KEY (unbanned_by) REFERENCES users (id) ON DELETE SET NULL;

COMMENT ON COLUMN awd_team_bans.banned_by IS '封禁操作者账号 ID（users.id；NULL=账号已注销）';
COMMENT ON COLUMN awd_team_bans.unbanned_by IS '解封操作者账号 ID（users.id；NULL=账号已注销或尚未解封）';

-- ================================================================================
-- 5) AWD 靶机重置：requested_by_admin
-- ================================================================================
--
-- 注意：`awd_reset_records_requester_check`
--   CHECK (NOT (requested_by IS NOT NULL AND requested_by_admin IS NOT NULL))
-- **必须保留**。统一主体后它的语义从「玩家 vs 管理员」变成
-- 「赛事参与主体 vs 平台操作主体」，两者仍然互斥；且两条 FK 都是 SET NULL，
-- 主体注销后该行合法退化为「两列皆空」（见 20260914120000 的原始论证）。

ALTER TABLE awd_reset_records
    DROP CONSTRAINT IF EXISTS awd_reset_records_requested_by_admin_fkey;

ALTER TABLE awd_reset_records
    ADD CONSTRAINT awd_reset_records_requested_by_admin_fkey
    FOREIGN KEY (requested_by_admin) REFERENCES users (id) ON DELETE SET NULL;

COMMENT ON COLUMN awd_reset_records.requested_by_admin IS '发起重置的平台操作账号 ID（users.id；与 requested_by 二选一）';

-- ================================================================================
-- 6) 自检：不应再有指向 super_admin 的外键
-- ================================================================================
--
-- 只检查本迁移负责的 6 个约束之外是否还有遗漏（logs 由 M5 处理，此处豁免）。
-- 若还有别的列指向 super_admin，说明清理清单不完整 —— 显式失败。

DO $$
DECLARE
  n integer;
  detail text;
BEGIN
  SELECT count(*), string_agg(format('%s.%s', c.conrelid::regclass, a.attname), '; ')
    INTO n, detail
    FROM pg_constraint c
    JOIN pg_attribute a
      ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
   WHERE c.contype = 'f'
     AND c.confrelid = 'public.super_admin'::regclass
     AND c.conname <> 'logs_superadmin_id_fkey';

  IF n > 0 THEN
    RAISE EXCEPTION
      'IDENTITY_MIGRATION_FK_INVENTORY: 仍有 % 个外键指向 super_admin 未被本迁移处理：%',
      n, detail;
  END IF;
END $$;
