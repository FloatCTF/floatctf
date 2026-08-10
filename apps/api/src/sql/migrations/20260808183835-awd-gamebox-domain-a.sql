-- ================================================================================
-- Migration: 20260808183835-awd-gamebox-domain-a
-- ================================================================================
-- GameBox 领域模型重构 — Migration A：新增领域表与实例新列。
--
-- 目标模型（四层）：
--   gameboxes（长期身份）
--     └─ gamebox_revisions（不可变部署版本）
--          └─ awd_event_gameboxes（赛事选择 + 计分/资源/可见性配置）
--               └─ awd_gamebox_instances（每队稳定逻辑靶机）
--                    └─ Docker Container（可替换运行时，仅 runtime）
--
-- 本迁移只做「新增」，不做数据回填（Migration B）与删除（Migration D）。
-- 回填依赖本迁移创建的表/列，故严格按 A → B → C → D 顺序执行。
-- ================================================================================


-- ──────────────────────────────────────────────────────────────────────────────
-- 1. gamebox_revisions：GameBox 的不可变部署版本
-- ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS "gamebox_revisions" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    "gamebox_id" UUID NOT NULL
        REFERENCES "gameboxes" ("id") ON DELETE CASCADE,

    "revision_number" INTEGER NOT NULL,

    -- 源配置（用户/题目作者的 TOML），与正规化后的 spec_json 分开保存
    "source_toml" TEXT NOT NULL,

    "spec_schema_version" INTEGER NOT NULL DEFAULT 1,
    -- FloatCTF 正规化后的实际配置（canonical JSON）
    "spec_json" JSONB NOT NULL,
    -- canonical spec_json 的 SHA-256（可稳定比较的 revision fingerprint）
    "spec_digest" VARCHAR(64) NOT NULL,

    -- 镜像与 Digest Pinning（§8）：digest 可为 NULL（legacy），
    -- Deploy/Precheck 必须在进入生产比赛前 resolve/pin。
    "image_ref" VARCHAR(500) NOT NULL,
    "image_digest" VARCHAR(200),

    "username" VARCHAR(100) NOT NULL,

    "default_cpu_millis" BIGINT NOT NULL,
    "default_memory_bytes" BIGINT NOT NULL,
    "default_pids_limit" BIGINT NOT NULL DEFAULT 100,

    "healthcheck_json" JSONB,

    -- Judge 配置属于 Revision（§9：如何判定 GameBox 是否正常是题目定义的一部分）
    "judge_script_name" VARCHAR(200),
    "judge_script_content" TEXT,
    "judge_args_json" JSONB,
    "default_judge_timeout_secs" INTEGER,
    "default_judge_retry_interval_secs" INTEGER,

    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- 约束（§5 / §11）：revision 不可变、每 GameBox 版本递增；
    -- UNIQUE(id, gamebox_id) 供 awd_event_gameboxes 复合 FK 使用。
    CONSTRAINT "gamebox_revisions_gamebox_number_key" UNIQUE ("gamebox_id", "revision_number"),
    CONSTRAINT "gamebox_revisions_gamebox_digest_key" UNIQUE ("gamebox_id", "spec_digest"),
    CONSTRAINT "gamebox_revisions_id_gamebox_key" UNIQUE ("id", "gamebox_id")
);

COMMENT ON TABLE "gamebox_revisions" IS 'GameBox 不可变部署版本：每编辑一次 GameBox 生成新 revision（revision_number +1），旧 revision 永不修改（审计/赛事 pin 保证）';
COMMENT ON COLUMN "gamebox_revisions"."source_toml" IS '源配置 TOML（用户/题目作者输入）';
COMMENT ON COLUMN "gamebox_revisions"."spec_json" IS 'FloatCTF 正规化后的实际配置（canonical JSON）';
COMMENT ON COLUMN "gamebox_revisions"."spec_digest" IS 'canonical spec_json 的 SHA-256，用于稳定比较 revision 是否变化';
COMMENT ON COLUMN "gamebox_revisions"."image_digest" IS '镜像 digest（sha256:...），可为 NULL（legacy）；生产前必须 pin';
COMMENT ON COLUMN "gamebox_revisions"."default_cpu_millis" IS '默认 CPU 限制（毫核）';
COMMENT ON COLUMN "gamebox_revisions"."default_memory_bytes" IS '默认内存限制（字节）';
COMMENT ON COLUMN "gamebox_revisions"."default_pids_limit" IS '默认进程数限制';
COMMENT ON COLUMN "gamebox_revisions"."healthcheck_json" IS '默认健康检查配置（JSON）';
COMMENT ON COLUMN "gamebox_revisions"."judge_script_name" IS '判题脚本文件名';
COMMENT ON COLUMN "gamebox_revisions"."judge_script_content" IS '判题脚本内容';
COMMENT ON COLUMN "gamebox_revisions"."judge_args_json" IS '判题脚本参数（JSON）';
COMMENT ON COLUMN "gamebox_revisions"."default_judge_timeout_secs" IS '默认判题超时（秒，赛事可覆盖）';
COMMENT ON COLUMN "gamebox_revisions"."default_judge_retry_interval_secs" IS '默认判题重试间隔（秒，赛事可覆盖）';

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. awd_event_gameboxes：某场 AWD 赛事选择的 GameBox Revision + 赛事计分配置
-- ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS "awd_event_gameboxes" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    "event_id" UUID NOT NULL
        REFERENCES "events" ("id") ON DELETE CASCADE,

    "gamebox_id" UUID NOT NULL
        REFERENCES "gameboxes" ("id") ON DELETE RESTRICT,

    -- pin 的具体 revision（§35：保存后必须 pin 为具体 UUID，禁止存 latest）
    "gamebox_revision_id" UUID NOT NULL,

    -- 确定性 IP 分配的核心：instance_ip = team.gamebox_subnet + host_offset（§13/§38）
    "host_offset" SMALLINT NOT NULL,

    "enabled" BOOLEAN NOT NULL DEFAULT TRUE,
    "hidden" BOOLEAN NOT NULL DEFAULT FALSE,

    -- 赛事资源覆盖（Revision 默认值之上的覆盖层，§49 effective config）
    "cpu_millis" BIGINT NOT NULL,
    "memory_bytes" BIGINT NOT NULL,
    "pids_limit" BIGINT NOT NULL DEFAULT 100,
    "healthcheck_override_json" JSONB,

    -- 赛事运行策略覆盖（§9：timeout/retry 更像比赛策略，属 Event 而非题目）
    "judge_timeout_secs" INTEGER,
    "judge_retry_interval_secs" INTEGER,

    -- 计分属于 Event × GameBox（§4），全部 BIGINT（§52），首破拼写修正为 first_bonus
    "break_points" BIGINT NOT NULL,
    "loss_points" BIGINT NOT NULL,
    "fix_points" BIGINT NOT NULL,
    "down_points" BIGINT NOT NULL,
    "first_bonus" BIGINT NOT NULL,

    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- §11：gamebox_id + revision_id 一致性由复合 FK 在 DB 层强制
    CONSTRAINT "awd_event_gameboxes_revision_fk"
        FOREIGN KEY ("gamebox_revision_id", "gamebox_id")
        REFERENCES "gamebox_revisions" ("id", "gamebox_id")
        ON DELETE RESTRICT,

    -- §12：Event 内一个 GameBox 只能有一个选择
    CONSTRAINT "awd_event_gameboxes_event_gamebox_key" UNIQUE ("event_id", "gamebox_id"),

    -- §13：同赛事 host_offset 唯一
    CONSTRAINT "awd_event_gameboxes_event_offset_key" UNIQUE ("event_id", "host_offset"),

    -- §13：不占用 network/broadcast/gateway/infra 保留位（.1 = 网关，.255 = 广播）
    CONSTRAINT "awd_event_gameboxes_host_offset_check" CHECK ("host_offset" BETWEEN 2 AND 254)
);

COMMENT ON TABLE "awd_event_gameboxes" IS 'AWD 赛事 GameBox 选择：赛事采用的 GameBox Revision + 该场自己的计分/资源/可见性配置';
COMMENT ON COLUMN "awd_event_gameboxes"."gamebox_id" IS 'GameBox 长期身份（RESTRICT：被赛事引用后禁止 hard delete）';
COMMENT ON COLUMN "awd_event_gameboxes"."gamebox_revision_id" IS '赛事 pin 的不可变 Revision（保存后为具体 UUID，不存 latest）';
COMMENT ON COLUMN "awd_event_gameboxes"."host_offset" IS '确定性 IP 分配偏移：instance_ip = team.gamebox_subnet + host_offset（2..254，禁改部署后的偏移）';
COMMENT ON COLUMN "awd_event_gameboxes"."enabled" IS '是否启用（停用后不再部署/判题）';
COMMENT ON COLUMN "awd_event_gameboxes"."hidden" IS '对玩家是否隐藏';
COMMENT ON COLUMN "awd_event_gameboxes"."cpu_millis" IS '赛事 CPU 限制（毫核）覆盖';
COMMENT ON COLUMN "awd_event_gameboxes"."memory_bytes" IS '赛事内存限制（字节）覆盖';
COMMENT ON COLUMN "awd_event_gameboxes"."pids_limit" IS '赛事进程数限制覆盖';
COMMENT ON COLUMN "awd_event_gameboxes"."healthcheck_override_json" IS '健康检查覆盖（JSON）';
COMMENT ON COLUMN "awd_event_gameboxes"."judge_timeout_secs" IS '判题超时（秒）覆盖';
COMMENT ON COLUMN "awd_event_gameboxes"."judge_retry_interval_secs" IS '判题重试间隔（秒）覆盖';
COMMENT ON COLUMN "awd_event_gameboxes"."break_points" IS '被攻破时攻击方得分';
COMMENT ON COLUMN "awd_event_gameboxes"."loss_points" IS '被攻破时防守方失分';
COMMENT ON COLUMN "awd_event_gameboxes"."fix_points" IS '修复得分';
COMMENT ON COLUMN "awd_event_gameboxes"."down_points" IS '宕机扣分';
COMMENT ON COLUMN "awd_event_gameboxes"."first_bonus" IS '首破奖励';

-- ──────────────────────────────────────────────────────────────────────────────
-- 3. awd_gamebox_instances：新增领域列（回填在 Migration B）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_gamebox_instances"
    ADD COLUMN IF NOT EXISTS "event_gamebox_id" UUID,
    ADD COLUMN IF NOT EXISTS "runtime_generation" BIGINT NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS "current_container_id" VARCHAR(64);

COMMENT ON COLUMN "awd_gamebox_instances"."event_gamebox_id" IS '所属赛事 GameBox 选择（逻辑靶机定义；Migration B 回填后加 NOT NULL + FK）';
COMMENT ON COLUMN "awd_gamebox_instances"."runtime_generation" IS '运行时代数：首次部署=1，Reset 成功替换容器 +1（容器只是当前 runtime realization）';
COMMENT ON COLUMN "awd_gamebox_instances"."current_container_id" IS '当前 Docker 容器 ID（可替换的运行时资源；对应旧 container_id，改名强调非逻辑身份）';

-- ──────────────────────────────────────────────────────────────────────────────
-- 4. awd_judge_tasks / awd_score_events：新增 event_gamebox_id 列（回填在 B）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_judge_tasks"
    ADD COLUMN IF NOT EXISTS "event_gamebox_id" UUID;

ALTER TABLE "awd_score_events"
    ADD COLUMN IF NOT EXISTS "event_gamebox_id" UUID;

COMMENT ON COLUMN "awd_judge_tasks"."event_gamebox_id" IS '判题目标（EventGameBox 维度；Judge 配置从 EventGameBox → Revision 解析）';
COMMENT ON COLUMN "awd_score_events"."event_gamebox_id" IS '计分作用域（EventGameBox 维度；first-blood 等按 EventGameBox 独立）';

