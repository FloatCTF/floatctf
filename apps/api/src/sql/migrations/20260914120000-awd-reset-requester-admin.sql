-- ================================================================================
-- Migration: 20260914120000-awd-reset-requester-admin
-- AWD reset 记录：管理员主体列
-- ================================================================================
--
-- 背景：awd_reset_records.requested_by 外键指向 users(id)（"请求用户 ID"），
-- 但管理端靶机重置（POST /api/admin/events/{id}/awd/gameboxes/{instance_id}/reset）
-- 会把 super_admin.id 写进同一列，被外键拒绝（500）。
--
-- 平台既有约定（其他 AWD 表）是"管理员主体单独一列并引用 super_admin"：
--   awd_score_events.created_by / awd_team_bans.banned_by / awd_internal_token_rotations.rotated_by
-- 本迁移按同一风格为 awd_reset_records 增加管理员列，保留 requested_by -> users(id) 不变。
--
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

ALTER TABLE awd_reset_records
    ADD COLUMN IF NOT EXISTS requested_by_admin UUID NULL
        REFERENCES super_admin(id) ON DELETE SET NULL;

COMMENT ON COLUMN awd_reset_records.requested_by_admin IS '发起重置的管理员 ID（super_admin；与 requested_by 二选一）';

-- 主体一致性：禁止"两个主体同时存在"（一次重置不可能由两方发起）。
--
-- 运行时不变式是"恰有一个主体"（玩家 requested_by / 管理员 requested_by_admin），
-- 由 reset_service::requester_columns 保证。
--
-- 但约束**不能**收紧成"恰好一个非空"：两条外键都是 ON DELETE SET NULL
-- （与 awd_score_events.created_by、awd_team_bans.banned_by 等审计列一致——
-- 主体被删除时要保留审计流水）。主体删除后相应列被置空，记录合法退化为
-- "两列皆空"。若写成恰好一个非空，删除用户/管理员时会报
--   violates check constraint "awd_reset_records_requester_check"
ALTER TABLE awd_reset_records
    ADD CONSTRAINT awd_reset_records_requester_check
    CHECK (NOT (requested_by IS NOT NULL AND requested_by_admin IS NOT NULL));
