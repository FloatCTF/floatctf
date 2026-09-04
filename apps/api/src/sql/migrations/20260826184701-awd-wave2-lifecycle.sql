-- ================================================================================
-- Migration: 20260826184701-awd-wave2-lifecycle
-- ================================================================================
-- Wave 2: 分离 Hardening 与 Attack 轮次生命周期
-- 1. 添加 hardening_ends_at 运行时截止时间
-- 2. 移除 RoundStatus::Grace 与 grace_ends_at
-- 3. 重建部分唯一索引（不含 grace）
-- 当前 DB 中无 grace 状态的轮次（已验证），迁移安全。

-- ── 1. Hardening 运行时截止时间 ──
ALTER TABLE awd_events ADD COLUMN IF NOT EXISTS hardening_ends_at TIMESTAMPTZ NULL;
COMMENT ON COLUMN awd_events.hardening_ends_at IS 'Hardening 阶段计划结束时间（运行时状态，非配置）';

-- ── 2. 移除 RoundStatus::Grace ──
-- 2a. 删除引用 grace 的部分唯一索引
DROP INDEX IF EXISTS idx_awd_rounds_one_active;

-- 2b. 重命名旧枚举 → 创建新枚举（不含 grace）
ALTER TYPE round_status RENAME TO round_status_old;
CREATE TYPE round_status AS ENUM ('active', 'completed', 'paused');

-- 2c. 先删除默认值，再迁移列类型，最后恢复默认值
ALTER TABLE awd_rounds ALTER COLUMN status DROP DEFAULT;
ALTER TABLE awd_rounds ALTER COLUMN status TYPE round_status USING status::text::round_status;
ALTER TABLE awd_rounds ALTER COLUMN status SET DEFAULT 'active'::round_status;

-- 2d. 删除旧枚举
DROP TYPE round_status_old;

-- 2e. 重建部分唯一索引（不含 grace）
CREATE UNIQUE INDEX idx_awd_rounds_one_active ON awd_rounds (event_id, status) WHERE status = ANY (ARRAY['active'::round_status, 'paused'::round_status]);

-- ── 3. 删除 grace_ends_at 列 ──
ALTER TABLE awd_rounds DROP COLUMN IF EXISTS grace_ends_at;