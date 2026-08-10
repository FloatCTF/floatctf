-- ================================================================================
-- Migration: 20260808224204-awd-network-control-plane-b
-- ================================================================================
-- AWD Network Control Plane 重构 — Migration B：从 awd_events 回填 Event Network。
--
-- 规则（§102/§103）：
--   · 旧数据即事实：现有 awd_events 的 gamebox_cidr / wireguard_cidr / …
--     直接固化进 awd_event_networks，绝不重新分配。
--   · infrastructure_subnet 派生：包含 flagserver_ip 的 team-prefix 块
--     （team prefix 取平台 awd_network_settings 当前值，默认 /24）。
--   · allocation 账本：每个已固化 Event 写入 gamebox + wireguard 两条 active 记录。
--   · 冲突即 STOP：部分配置的 Event、active allocations 重叠、WG 端口重复
--     一律 RAISE EXCEPTION，不静默修改任何 CIDR。
--   · docker_network_id 属 Observed（runtime），不进新模型。
-- ================================================================================


CREATE TEMP TABLE IF NOT EXISTS "_en_backfill" (
    "event_id" UUID PRIMARY KEY,
    "event_network_id" UUID NOT NULL
);

-- ──────────────────────────────────────────────────────────────────────────────
-- 1. 回填 awd_event_networks
--    有完整网络配置（gamebox_cidr 与 wireguard_cidr 均非空）的 Event 固化网络。
-- ──────────────────────────────────────────────────────────────────────────────
WITH full_config AS (
    SELECT
        e."event_id" AS event_id,
        e."gamebox_cidr"::cidr AS gamebox_cidr,
        e."wireguard_cidr"::cidr AS wireguard_cidr,
        e."wireguard_interface_name",
        e."wireguard_listen_port",
        e."flagserver_ip"::inet AS flagserver_ip,
        e."judgeserver_ip"::inet AS judgeserver_ip,
        COALESCE(e."docker_network_name", 'fctf-awd-' || left(e."event_id"::text, 8)) AS docker_network_name,
        e."docker_network_id" IS NOT NULL AS deployed
    FROM "awd_events" e
    WHERE e."gamebox_cidr" <> ''
      AND e."wireguard_cidr" <> ''
),
inserted AS (
    INSERT INTO "awd_event_networks" (
        "event_id", "allocation_mode",
        "gamebox_cidr", "wireguard_cidr", "infrastructure_subnet",
        "flagserver_ip", "judgeserver_ip",
        "wireguard_interface_name", "wireguard_listen_port", "docker_network_name",
        "locked_at", "updated_at"
    )
    SELECT
        f.event_id,
        'manual',  -- 旧数据来路不明，保守标注 manual
        f.gamebox_cidr,
        f.wireguard_cidr,
        network(set_masklen(f.flagserver_ip, s."gamebox_team_prefix"))::cidr
            AS infrastructure_subnet,
        f.flagserver_ip,
        f.judgeserver_ip,
        f."wireguard_interface_name",
        f."wireguard_listen_port",
        f.docker_network_name,
        CASE WHEN f.deployed THEN now() ELSE NULL END AS locked_at,
        now()
    FROM full_config f
    CROSS JOIN "awd_network_settings" s
    WHERE s."id" = 1
      AND NOT EXISTS (
          SELECT 1 FROM "awd_event_networks" en
          WHERE en."event_id" = f.event_id
      )
    RETURNING "id", "event_id"
)
INSERT INTO "_en_backfill" ("event_id", "event_network_id")
SELECT "event_id", "id" FROM inserted;

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. 回填 awd_network_allocations（gamebox + wireguard 两条 active 记录）
-- ──────────────────────────────────────────────────────────────────────────────
INSERT INTO "awd_network_allocations" ("event_id", "kind", "cidr")
SELECT "event_id", 'gamebox'::awd_network_allocation_kind, "gamebox_cidr"::cidr
FROM "awd_events"
WHERE "gamebox_cidr" <> '' AND "wireguard_cidr" <> ''
  AND NOT EXISTS (
      SELECT 1 FROM "awd_network_allocations" a
      WHERE a."event_id" = "awd_events"."event_id" AND a."kind" = 'gamebox'
  );

INSERT INTO "awd_network_allocations" ("event_id", "kind", "cidr")
SELECT "event_id", 'wireguard'::awd_network_allocation_kind, "wireguard_cidr"::cidr
FROM "awd_events"
WHERE "gamebox_cidr" <> '' AND "wireguard_cidr" <> ''
  AND NOT EXISTS (
      SELECT 1 FROM "awd_network_allocations" a
      WHERE a."event_id" = "awd_events"."event_id" AND a."kind" = 'wireguard'
  );

-- ──────────────────────────────────────────────────────────────────────────────
-- 3. 校验：任何冲突立即 STOP，不静默改数据
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
DECLARE
    v_bad_partial INT;
    v_overlap INT;
    v_missing INT;
BEGIN
    -- 3.1 部分配置：只有一边网络字段的 Event 不允许进入新模型
    SELECT count(*) INTO v_bad_partial FROM "awd_events"
    WHERE ("gamebox_cidr" <> '' AND "wireguard_cidr" = '')
       OR ("gamebox_cidr" = '' AND "wireguard_cidr" <> '');

    IF v_bad_partial > 0 THEN
        RAISE EXCEPTION 'MIGRATION CONFLICT: % Event(s) have partial network config (only one of gamebox_cidr/wireguard_cidr) — STOP, fix manually', v_bad_partial;
    END IF;

    -- 3.2 完整配置的 Event 必须已固化网络
    SELECT count(*) INTO v_missing FROM "awd_events"
    WHERE "gamebox_cidr" <> '' AND "wireguard_cidr" <> ''
      AND NOT EXISTS (SELECT 1 FROM "awd_event_networks" en WHERE en."event_id" = "awd_events"."event_id");

    IF v_missing > 0 THEN
        RAISE EXCEPTION 'MIGRATION CONFLICT: % fully-configured Event(s) missing event_network — STOP', v_missing;
    END IF;

    -- 3.3 active allocations 两两重叠（含跨 kind）→ STOP
    SELECT count(*) INTO v_overlap
    FROM "awd_network_allocations" a
    JOIN "awd_network_allocations" b
      ON a."id" < b."id"
     AND a."released_at" IS NULL AND b."released_at" IS NULL
     AND a."cidr" && b."cidr";

    IF v_overlap > 0 THEN
        RAISE EXCEPTION 'MIGRATION CONFLICT: % overlapping active allocations in ledger — STOP, resolve before proceeding', v_overlap;
    END IF;

    -- 3.4 WG 端口重复由 awd_event_networks UNIQUE 约束在 INSERT 时兜底，
    --     此处补充说明性检查（若 INSERT 已成功则必然为 0）
    IF EXISTS (
        SELECT 1 FROM "awd_event_networks" a JOIN "awd_event_networks" b
        ON a."id" < b."id" AND a."wireguard_listen_port" = b."wireguard_listen_port"
    ) THEN
        RAISE EXCEPTION 'MIGRATION CONFLICT: duplicate wireguard_listen_port in event networks — STOP';
    END IF;
END $$;

DROP TABLE IF EXISTS "_en_backfill";

