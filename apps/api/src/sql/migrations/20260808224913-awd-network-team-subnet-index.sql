-- ================================================================================
-- Migration: 20260808224913-awd-network-team-subnet-index
-- ================================================================================
-- AWD Network Control Plane 重构 — Migration E：Team subnet 稳定 slot 序号。
--
-- Team subnet 分配不再依赖 Team 顺序/名称（§38）：每个 Team 在 Event 内持有
-- 持久化的 subnet_index（0 = 基础设施保留块，Team 从 1 开始）。
-- 已释放的 Team slot 在同一 Event 生命周期内不复用（§39）：释放 = 行保留
-- （status='released'），allocator 的 used_indexes 读取全部行（含 released）。
--
-- 历史行 backfill：按 created_at 稳定序赋递增序号（dev 无数据为零操作）；
-- 该序号仅承担「不复用」语义，已锁定 Event 不再分配新 Team，故无需与
-- nth_subnet 完全对齐。
-- ================================================================================

BEGIN;

ALTER TABLE "awd_team_networks"
    ADD COLUMN IF NOT EXISTS "subnet_index" SMALLINT;

-- 历史行：同一 Event 内按创建顺序稳定编号（ROW_NUMBER 从 1 开始 = 首个 Team 用 index 1）
WITH ranked AS (
    SELECT
        "id",
        ROW_NUMBER() OVER (
            PARTITION BY "event_id" ORDER BY "created_at", "id"
        )::SMALLINT AS idx
    FROM "awd_team_networks"
)
UPDATE "awd_team_networks" t
SET "subnet_index" = r.idx
FROM ranked r
WHERE t."id" = r."id";

ALTER TABLE "awd_team_networks"
    ALTER COLUMN "subnet_index" SET NOT NULL;

COMMENT ON COLUMN "awd_team_networks"."subnet_index" IS 'Team 在 Event 内的稳定子网 slot 序号（0=infra 保留；已释放 slot 不复用）';

COMMIT;
