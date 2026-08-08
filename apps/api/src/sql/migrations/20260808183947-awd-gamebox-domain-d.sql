-- ================================================================================
-- Migration: 20260808183947-awd-gamebox-domain-d
-- ================================================================================
-- GameBox 领域模型重构 — Migration D：删除 legacy 双轨 schema。
--
-- 依赖 A/B/C 全部落库且业务代码已切换到新模型后执行（本文件随 A/B/C 一起
-- 进入 merged.sql，但语义上属于最终清理：旧表/旧列/旧拼写不再存在于 schema）。
--
-- 删除清单（§59）：
--   - awd_gamebox_templates（被 gameboxes + gamebox_revisions + awd_event_gameboxes 取代）
--   - event_gameboxes（旧 Jeopardy-GameBox 关联，无业务调用者）
--   - instances.gamebox_id（旧 GameBox runtime 关联，仅剩 read filter）
--   - gameboxes.toml_str / username / break_point / fix_point / down_point / first_bouns
--   - awd_team_networks.next_gamebox_host（host_offset 取代）
--   - awd_gamebox_instances.template_id / docker_network_id / container_id（→ current_container_id）
--   - awd_judge_tasks.template_id / awd_score_events.gamebox_template_id（→ event_gamebox_id）
-- ================================================================================

BEGIN;

-- ──────────────────────────────────────────────────────────────────────────────
-- 1. awd_gamebox_instances：删旧列 + container_id 改名 + gamebox_ip 转 INET
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_gamebox_instances"
    DROP COLUMN IF EXISTS "template_id",
    DROP COLUMN IF EXISTS "docker_network_id",
    DROP COLUMN IF EXISTS "container_id";

-- 注：gamebox_ip 保持 VARCHAR(15)，不做 INET 转换。
-- §23 逃生口：SeaORM 1.1.20 将 INET 映射为 String，INSERT/比较时按 TEXT 绑定，
-- PostgreSQL 拒绝 text→inet（无隐式转换），实体生成产物无法携带类型覆盖。
-- 因此 INET 迁移列为独立后续项（schema 类型层面改造），本次不改。
COMMENT ON COLUMN "awd_gamebox_instances"."gamebox_ip" IS '靶机内网 IP（= team.gamebox_subnet + event_gamebox.host_offset，确定性分配）';

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. awd_team_networks：删除 next_gamebox_host（§14，host_offset 取代）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_team_networks"
    DROP COLUMN IF EXISTS "next_gamebox_host";

-- ──────────────────────────────────────────────────────────────────────────────
-- 3. gameboxes：删除 legacy runtime/计分列；name 不强制全局唯一（§53，展示名）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "gameboxes"
    DROP COLUMN IF EXISTS "toml_str",
    DROP COLUMN IF EXISTS "username",
    DROP COLUMN IF EXISTS "break_point",
    DROP COLUMN IF EXISTS "fix_point",
    DROP COLUMN IF EXISTS "down_point",
    DROP COLUMN IF EXISTS "first_bouns";

ALTER TABLE "gameboxes"
    DROP CONSTRAINT IF EXISTS "gameboxes_name_key";

-- ──────────────────────────────────────────────────────────────────────────────
-- 4. awd_judge_tasks / awd_score_events：删旧 template 引用列
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_judge_tasks"
    DROP COLUMN IF EXISTS "template_id";

ALTER TABLE "awd_score_events"
    DROP COLUMN IF EXISTS "gamebox_template_id";

-- ──────────────────────────────────────────────────────────────────────────────
-- 5. 删除 legacy 表
-- ──────────────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS "awd_gamebox_templates";
DROP TABLE IF EXISTS "event_gameboxes";

-- ──────────────────────────────────────────────────────────────────────────────
-- 6. instances.gamebox_id（Jeopardy 通用实例不再支持 GameBox，§33）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "instances"
    DROP COLUMN IF EXISTS "gamebox_id";

COMMIT;
