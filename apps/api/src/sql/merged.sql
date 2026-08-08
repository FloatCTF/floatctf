-- ================================================================================================
-- ================================================================================================
--                                   FloatCTF Merged Migrations
-- Generated at: 2026-08-08 19:02:41 +0800
-- Migration count: 22
-- ================================================================================================
-- ================================================================================================

-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171904-up.sql
-- ================================================================================================
-- ================================================================================================

CREATE EXTENSION "uuid-ossp";
-- public tables
CREATE TYPE "setting_value_type" AS ENUM ('string', 'integer', 'boolean','float');

CREATE TABLE IF NOT EXISTS "settings" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "key" TEXT NOT NULL UNIQUE,
    "value" TEXT NOT NULL,
    "type" "setting_value_type" NOT NULL DEFAULT 'string',
    "description" TEXT NOT NULL,
    "protected" BOOLEAN NOT NULL DEFAULT TRUE,
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


CREATE TABLE IF NOT EXISTS "weapons" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "name" TEXT NOT NULL,
    "category" TEXT NOT NULL DEFAULT 'other',
    "description" TEXT,
    "has_file" BOOLEAN NOT NULL DEFAULT FALSE,
    "download_count" BIGINT NOT NULL DEFAULT 0,
    "file_url" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 1. 核心任务表
CREATE TABLE IF NOT EXISTS "scheduled_tasks" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "group_id" UUID,                         -- 比赛ID或靶机ID，用于一键销毁
    "task_name" VARCHAR(200) NOT NULL,   -- 任务名称，如 "第3轮-Flag刷新-选手A"
    "description" TEXT,
    "task_key" VARCHAR(100) NOT NULL,        -- 路由键：GAME_START, LAB_CLOSE, CHECK...
    "trigger_type" VARCHAR(50) NOT NULL,     -- 触发类型：startup, once, cron
    "status" VARCHAR(50) NOT NULL DEFAULT 'pending', -- pending, running, completed, failed, paused

    "enabled" BOOLEAN NOT NULL DEFAULT true,  -- 默认开启
    "protected" BOOLEAN NOT NULL DEFAULT true,
    "cron_expr" VARCHAR(100),                -- 例如：*/10 * * * *
    "execute_at" TIMESTAMPTZ,                -- 计划执行时间
    "expires_at" TIMESTAMPTZ,                -- 过期时间：过了这个点就不再补执行

    "payload" JSONB,                         -- 强类型的业务参数
    "error_msg" TEXT,
    "last_run_at" TIMESTAMPTZ,

    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);




-- user tables
CREATE TABLE IF NOT EXISTS "users" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "username" TEXT NOT NULL UNIQUE,
    "nickname" TEXT NOT NULL UNIQUE,
    "password" TEXT NOT NULL,
    "email" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "super_admin" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "username" TEXT NOT NULL UNIQUE,
    "password" TEXT NOT NULL,
    "email" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "announcements" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "title" TEXT NOT NULL,
    "content" TEXT,
    "publisher_id" UUID NOT NULL REFERENCES "super_admin" ("id") ON DELETE CASCADE,
    "publisher" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- challenge tables
CREATE TABLE IF NOT EXISTS "challenges" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "name" TEXT NOT NULL UNIQUE,
    -- ALTER TABLE challenges ADD COLUMN safe_name TEXT; 允许'
    "safe_name" TEXT NOT NULL UNIQUE,
    "category" TEXT NOT NULL DEFAULT 'other',
    "description" TEXT NOT NULL DEFAULT 'no description',
    "attachment" TEXT NULL,
    "hidden" BOOLEAN NOT NULL DEFAULT TRUE,
    "toml_str" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "challenge_solves" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "challenge_id" UUID NOT NULL REFERENCES "challenges" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "challenge_writeup" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "challenge_id" UUID NOT NULL REFERENCES "challenges" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "content" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "challenge_sets" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "name" TEXT NOT NULL,
    "description" TEXT,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "challenge_set_items" (
    "set_id" UUID NOT NULL REFERENCES "challenge_sets" (id) ON DELETE CASCADE,
    "challenge_id" UUID NOT NULL REFERENCES "challenges" (id) ON DELETE CASCADE,
    PRIMARY KEY ("set_id", "challenge_id")
);



-- gamebox tables
-- AWD (Only for event)
CREATE TABLE IF NOT EXISTS "gameboxes" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "name" TEXT NOT NULL UNIQUE,
    -- ALTER TABLE challenges ADD COLUMN safe_name TEXT; 允许'
    "safe_name" TEXT NOT NULL UNIQUE,
    "category" TEXT NOT NULL DEFAULT 'other',
    "description" TEXT NOT NULL DEFAULT 'no description',
    "hidden" BOOLEAN NOT NULL DEFAULT TRUE,
    "toml_str" TEXT NOT NULL,
    -- config
    "username" TEXT NOT NULL,
    "break_point" DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    "fix_point" DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    "down_point" DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    "first_bouns" DOUBLE PRECISION NOT NULL DEFAULT 0.2,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);



-- instance tables
CREATE TYPE "instance_status" AS ENUM ('pending', 'running', 'completed', 'failed');

CREATE TABLE IF NOT EXISTS "instances" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "status" "instance_status" NOT NULL DEFAULT 'pending',
    "ref" TEXT NOT NULL DEFAULT 'JeopardyPractice',
    "flag" TEXT NOT NULL,
    "content" TEXT,
    -- gamebox_id
    "challenge_id" UUID REFERENCES "challenges" (id) ON DELETE CASCADE,
    "gamebox_id" UUID REFERENCES "gameboxes" (id) ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" (id) ON DELETE CASCADE,
    "identifier" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "destroy_at" TIMESTAMPTZ NOT NULL
);



-- event tables
CREATE TYPE "event_type" AS ENUM ('jeopardy_practice','jeopardy_single', 'jeopardy_team', 'awd_team');
CREATE TYPE "event_team_member_role" AS ENUM ('captain', 'member');

CREATE TABLE IF NOT EXISTS "events" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "type" "event_type" NOT NULL DEFAULT 'jeopardy_single',
    "title" TEXT NOT NULL,
    "description" TEXT,
    "hidden" BOOLEAN NOT NULL DEFAULT TRUE,
    "start_time" TIMESTAMPTZ NOT NULL,
    "rules" TEXT NOT NULL DEFAULT 'do not cheat',
    "allow_join" BOOLEAN NOT NULL DEFAULT FALSE,
    "flag_prefix" TEXT NULL DEFAULT 'flag',
    "end_time" TIMESTAMPTZ NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "event_users" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "points" DOUBLE PRECISION NOT NULL DEFAULT 0,
    "banned" BOOLEAN NOT NULL DEFAULT false,
    "joined_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY ("event_id", "user_id")
);

CREATE TABLE IF NOT EXISTS "event_teams" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "event_id" UUID NOT NULL REFERENCES events ("id") ON DELETE CASCADE,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "points" DOUBLE PRECISION NOT NULL DEFAULT 0,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "banned" BOOLEAN NOT NULL DEFAULT false,
    UNIQUE ("event_id", "name")
);

CREATE TABLE IF NOT EXISTS "event_team_members" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "role" "event_team_member_role" NOT NULL DEFAULT 'member',
    "joined_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        "event_id",
        "team_id",
        "user_id"
    )
);

CREATE TABLE IF NOT EXISTS "event_announcements" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "title" TEXT NOT NULL,
    "content" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS "event_writeup" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "team_id" UUID NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "file_url" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT event_writeup_pkey PRIMARY KEY ("event_id", "user_id")
);

CREATE TABLE IF NOT EXISTS "event_challenges" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "challenge_id" UUID NOT NULL REFERENCES "challenges" ("id") ON DELETE CASCADE,
    "points" DOUBLE PRECISION NOT NULL DEFAULT 100,
    "hidden" BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY ("event_id", "challenge_id")
);

CREATE TABLE IF NOT EXISTS "event_challenge_solves" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "challenge_id" UUID NOT NULL REFERENCES "challenges" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "team_id" UUID NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "obtained_points" DOUBLE PRECISION NOT NULL DEFAULT 0,
    "bonus_points" DOUBLE PRECISION NOT NULL DEFAULT 0,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        "event_id",
        "challenge_id",
        "user_id"
    )
);

CREATE TABLE IF NOT EXISTS "event_gameboxes" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "gamebox_id" UUID NOT NULL REFERENCES "gameboxes" ("id") ON DELETE CASCADE,
    "hidden" BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY ("event_id", "gamebox_id")
);

-- 应该先查找instance_id 再寻找 challenge_id 可共用instance ｜ awd 可以把刷新后的 flag 填入
CREATE TABLE IF NOT EXISTS "event_instances" (
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "instance_id" UUID NOT NULL REFERENCES "instances" ("id") ON DELETE CASCADE,
    -- "challenge_id" UUID NOT NULL REFERENCES "challenges" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "team_id" UUID NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    PRIMARY KEY ("event_id", "instance_id")
);

-- event_logs
-- logs JSONB
-- Set(Uuid::nil()
CREATE TABLE IF NOT EXISTS "event_logs" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "user_id" UUID REFERENCES "users" ("id") ON DELETE SET NULL,
    "team_id" UUID REFERENCES "event_teams" ("id") ON DELETE SET NULL,
    "ip_address" VARCHAR(45), -- 必须记录 IP，防撞库、防恶意操作
    -- 2. 建议增加一个简单的 category 或 action 字段 (TEXT)
    -- 虽然 details 里有，但把 'login', 'capture_flag', 'container_start' 放在外面，
    -- 这样你在 SeaORM 里做 filter 会快几个数量级。
    "type" "event_type" NOT NULL DEFAULT 'jeopardy_single',
    "level" VARCHAR(20) NOT NULL DEFAULT 'info',
    "action" VARCHAR(50) NOT NULL,

    -- ipaddress
    "details" JSONB NOT NULL DEFAULT '{}',
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


CREATE TABLE IF NOT EXISTS "logs" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- 1. 身份与位置
    "user_id" UUID REFERENCES "users" ("id") ON DELETE SET NULL,
    "superadmin_id" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL, -- 哪个超管干的
    "ip_address" VARCHAR(45), -- 必须记录 IP，防撞库、防恶意操作

    -- 2. 分类审计 (核心索引字段)
    -- category: 'AUTH', 'SYSTEM', 'SERVICE', 'ADMIN_ACTION', 'WEAPONS'
    "category" VARCHAR(30) NOT NULL,
    -- action: 动作描述，如 'delete_file', 'start_container', 'update_password'
    "action" VARCHAR(50) NOT NULL,

    -- 3. 级别与内容
    -- level: 'debug', 'info', 'warn', 'error', 'fatal'
    "level" VARCHAR(10) NOT NULL DEFAULT 'info',
    "message" TEXT NOT NULL, -- 人类可读的简述：如 "管理员 A 删除了用户 B"
    "details" JSONB NOT NULL DEFAULT '{}', -- 具体的差异化数据

    -- 4. 时间
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- 讨论帖子表
CREATE TABLE IF NOT EXISTS "discussions" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "title" TEXT NOT NULL,
    "content" TEXT NOT NULL,
    "author_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "view_count" INT NOT NULL DEFAULT 0,
    "like_count" INT NOT NULL DEFAULT 0,
    "comment_count" INT NOT NULL DEFAULT 0,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 评论表（支持回复）
CREATE TABLE IF NOT EXISTS "discussion_comments" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "discussion_id" UUID NOT NULL REFERENCES "discussions" ("id") ON DELETE CASCADE,
    "author_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "content" TEXT NOT NULL,
    "parent_id" UUID REFERENCES "discussion_comments" ("id") ON DELETE CASCADE,  -- NULL 表示顶级评论
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 点赞表（防止重复点赞）
CREATE TABLE IF NOT EXISTS "discussion_likes" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "discussion_id" UUID NOT NULL REFERENCES "discussions" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("discussion_id", "user_id")
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "settings" IS '动态设置表：管理员可编辑的键值配置（进程启动时从 TOML 播种默认值，之后可独立修改）';
COMMENT ON COLUMN "settings"."id" IS '主键';
COMMENT ON COLUMN "settings"."key" IS '设置键名（唯一）';
COMMENT ON COLUMN "settings"."value" IS '设置值（统一以字符串存储）';
COMMENT ON COLUMN "settings"."type" IS '值类型：string / integer / boolean / float';
COMMENT ON COLUMN "settings"."description" IS '设置说明';
COMMENT ON COLUMN "settings"."protected" IS '受保护标志：受保护设置不允许普通修改/删除';
COMMENT ON COLUMN "settings"."updated_at" IS '更新时间';

COMMENT ON TABLE "weapons" IS '武器库：平台提供的攻击工具/装备（可附带文件供下载）';
COMMENT ON COLUMN "weapons"."id" IS '主键';
COMMENT ON COLUMN "weapons"."name" IS '武器名称';
COMMENT ON COLUMN "weapons"."category" IS '分类（默认 other）';
COMMENT ON COLUMN "weapons"."description" IS '武器描述';
COMMENT ON COLUMN "weapons"."has_file" IS '是否附带文件';
COMMENT ON COLUMN "weapons"."download_count" IS '下载次数';
COMMENT ON COLUMN "weapons"."file_url" IS '文件下载地址';
COMMENT ON COLUMN "weapons"."created_at" IS '创建时间';
COMMENT ON COLUMN "weapons"."updated_at" IS '更新时间';

COMMENT ON TABLE "scheduled_tasks" IS '调度任务表：startup/once/cron 后台任务，由调度器引擎轮询执行';
COMMENT ON COLUMN "scheduled_tasks"."id" IS '主键';
COMMENT ON COLUMN "scheduled_tasks"."group_id" IS '业务组：比赛 ID 或靶机 ID，用于一键销毁';
COMMENT ON COLUMN "scheduled_tasks"."task_name" IS '任务名称，如"第3轮-Flag刷新-选手A"';
COMMENT ON COLUMN "scheduled_tasks"."description" IS '任务描述';
COMMENT ON COLUMN "scheduled_tasks"."task_key" IS '路由键：GAME_START / LAB_CLOSE / CHECK 等，决定执行哪个 handler';
COMMENT ON COLUMN "scheduled_tasks"."trigger_type" IS '触发类型：startup / once / cron';
COMMENT ON COLUMN "scheduled_tasks"."status" IS '状态：pending / running / completed / failed / paused';
COMMENT ON COLUMN "scheduled_tasks"."enabled" IS '是否启用';
COMMENT ON COLUMN "scheduled_tasks"."protected" IS '是否受保护（普通接口不可删除/修改）';
COMMENT ON COLUMN "scheduled_tasks"."cron_expr" IS 'cron 表达式，如 */10 * * * *';
COMMENT ON COLUMN "scheduled_tasks"."execute_at" IS '计划执行时间';
COMMENT ON COLUMN "scheduled_tasks"."expires_at" IS '过期时间：超过该时间不再补执行';
COMMENT ON COLUMN "scheduled_tasks"."payload" IS '业务参数（强类型 JSON）';
COMMENT ON COLUMN "scheduled_tasks"."error_msg" IS '最近一次执行错误信息';
COMMENT ON COLUMN "scheduled_tasks"."last_run_at" IS '最近一次执行时间';
COMMENT ON COLUMN "scheduled_tasks"."created_at" IS '创建时间';
COMMENT ON COLUMN "scheduled_tasks"."updated_at" IS '更新时间';

COMMENT ON TABLE "users" IS '用户表：参赛选手账号';
COMMENT ON COLUMN "users"."id" IS '主键';
COMMENT ON COLUMN "users"."username" IS '用户名（登录用，唯一）';
COMMENT ON COLUMN "users"."nickname" IS '昵称（展示用，唯一）';
COMMENT ON COLUMN "users"."password" IS '密码哈希（argon2id）';
COMMENT ON COLUMN "users"."email" IS '邮箱';
COMMENT ON COLUMN "users"."created_at" IS '创建时间';
COMMENT ON COLUMN "users"."updated_at" IS '更新时间';

COMMENT ON TABLE "super_admin" IS '超级管理员表：平台运营账号';
COMMENT ON COLUMN "super_admin"."id" IS '主键';
COMMENT ON COLUMN "super_admin"."username" IS '用户名（唯一）';
COMMENT ON COLUMN "super_admin"."password" IS '密码哈希（argon2id）';
COMMENT ON COLUMN "super_admin"."email" IS '邮箱';
COMMENT ON COLUMN "super_admin"."created_at" IS '创建时间';
COMMENT ON COLUMN "super_admin"."updated_at" IS '更新时间';

COMMENT ON TABLE "announcements" IS '平台公告表（由超级管理员发布）';
COMMENT ON COLUMN "announcements"."id" IS '主键';
COMMENT ON COLUMN "announcements"."title" IS '公告标题';
COMMENT ON COLUMN "announcements"."content" IS '公告内容';
COMMENT ON COLUMN "announcements"."publisher_id" IS '发布者（超级管理员）ID';
COMMENT ON COLUMN "announcements"."publisher" IS '发布者名称';
COMMENT ON COLUMN "announcements"."created_at" IS '创建时间';
COMMENT ON COLUMN "announcements"."updated_at" IS '更新时间';

COMMENT ON TABLE "challenges" IS '题目表：Jeopardy 独立题目（含题目实例的 TOML 部署配置）';
COMMENT ON COLUMN "challenges"."id" IS '主键';
COMMENT ON COLUMN "challenges"."name" IS '题目名称（唯一）';
COMMENT ON COLUMN "challenges"."safe_name" IS '安全名称（URL/路径友好，唯一）';
COMMENT ON COLUMN "challenges"."category" IS '分类（默认 other）';
COMMENT ON COLUMN "challenges"."description" IS '题目描述';
COMMENT ON COLUMN "challenges"."attachment" IS '附件（可为空）';
COMMENT ON COLUMN "challenges"."hidden" IS '是否隐藏';
COMMENT ON COLUMN "challenges"."toml_str" IS '题目实例部署配置（TOML 文本）';
COMMENT ON COLUMN "challenges"."created_at" IS '创建时间';
COMMENT ON COLUMN "challenges"."updated_at" IS '更新时间';

COMMENT ON TABLE "challenge_solves" IS '独立解题记录：练习模式的解题流水（event_id 为空）；赛事解题另有 event_challenge_solves';
COMMENT ON COLUMN "challenge_solves"."id" IS '主键';
COMMENT ON COLUMN "challenge_solves"."challenge_id" IS '题目 ID';
COMMENT ON COLUMN "challenge_solves"."user_id" IS '解题用户 ID';
COMMENT ON COLUMN "challenge_solves"."created_at" IS '解题时间';

COMMENT ON TABLE "challenge_writeup" IS '题解表：用户提交的题目 WriteUp';
COMMENT ON COLUMN "challenge_writeup"."id" IS '主键';
COMMENT ON COLUMN "challenge_writeup"."challenge_id" IS '题目 ID';
COMMENT ON COLUMN "challenge_writeup"."user_id" IS '作者用户 ID';
COMMENT ON COLUMN "challenge_writeup"."content" IS '题解内容';
COMMENT ON COLUMN "challenge_writeup"."created_at" IS '创建时间';

COMMENT ON TABLE "challenge_sets" IS '题目集合表：把若干题目组织为一个集合（如专题）';
COMMENT ON COLUMN "challenge_sets"."id" IS '主键';
COMMENT ON COLUMN "challenge_sets"."name" IS '集合名称';
COMMENT ON COLUMN "challenge_sets"."description" IS '集合描述';
COMMENT ON COLUMN "challenge_sets"."created_at" IS '创建时间';

COMMENT ON TABLE "challenge_set_items" IS '题目集合与题目的多对多关联表';
COMMENT ON COLUMN "challenge_set_items"."set_id" IS '集合 ID';
COMMENT ON COLUMN "challenge_set_items"."challenge_id" IS '题目 ID';

COMMENT ON TABLE "gameboxes" IS 'AWD 靶机模板库：赛事专用靶机定义（部署镜像与计分参数）';
COMMENT ON COLUMN "gameboxes"."id" IS '主键';
COMMENT ON COLUMN "gameboxes"."name" IS '靶机名称（唯一）';
COMMENT ON COLUMN "gameboxes"."safe_name" IS '安全名称（URL/路径友好，唯一）';
COMMENT ON COLUMN "gameboxes"."category" IS '分类（默认 other）';
COMMENT ON COLUMN "gameboxes"."description" IS '靶机描述';
COMMENT ON COLUMN "gameboxes"."hidden" IS '是否隐藏';
COMMENT ON COLUMN "gameboxes"."toml_str" IS '靶机部署配置（TOML 文本）';
COMMENT ON COLUMN "gameboxes"."username" IS '容器 SSH 用户名';
COMMENT ON COLUMN "gameboxes"."break_point" IS '被攻破时攻击方得分基数（比例）';
COMMENT ON COLUMN "gameboxes"."fix_point" IS '修复得分基数';
COMMENT ON COLUMN "gameboxes"."down_point" IS '宕机扣分基数';
COMMENT ON COLUMN "gameboxes"."first_bouns" IS '首破奖励比例（默认 0.2）';
COMMENT ON COLUMN "gameboxes"."created_at" IS '创建时间';
COMMENT ON COLUMN "gameboxes"."updated_at" IS '更新时间';

COMMENT ON TABLE "instances" IS '题目实例表：动态创建的容器实例（Jeopardy 练习/赛事共用）';
COMMENT ON COLUMN "instances"."id" IS '主键';
COMMENT ON COLUMN "instances"."status" IS '状态：pending / running / completed / failed';
COMMENT ON COLUMN "instances"."ref" IS '实例模式标识（如 JeopardyPractice）';
COMMENT ON COLUMN "instances"."flag" IS '实例内动态生成的 flag';
COMMENT ON COLUMN "instances"."content" IS '实例内容/提示（可为空）';
COMMENT ON COLUMN "instances"."challenge_id" IS '关联题目 ID（可为空）';
COMMENT ON COLUMN "instances"."gamebox_id" IS '关联靶机模板 ID（可为空）';
COMMENT ON COLUMN "instances"."user_id" IS '创建/归属用户 ID';
COMMENT ON COLUMN "instances"."identifier" IS '实例唯一标识（如容器名/ID）';
COMMENT ON COLUMN "instances"."created_at" IS '创建时间';
COMMENT ON COLUMN "instances"."updated_at" IS '更新时间';
COMMENT ON COLUMN "instances"."destroy_at" IS '自动销毁时间';

COMMENT ON TABLE "events" IS '赛事表：Jeopardy（练习/单人/团队）与 AWD 赛事';
COMMENT ON COLUMN "events"."id" IS '主键';
COMMENT ON COLUMN "events"."type" IS '赛事类型：jeopardy_practice / jeopardy_single / jeopardy_team / awd_team';
COMMENT ON COLUMN "events"."title" IS '赛事标题';
COMMENT ON COLUMN "events"."description" IS '赛事描述';
COMMENT ON COLUMN "events"."hidden" IS '是否隐藏';
COMMENT ON COLUMN "events"."start_time" IS '开始时间';
COMMENT ON COLUMN "events"."rules" IS '比赛规则说明';
COMMENT ON COLUMN "events"."allow_join" IS '是否允许加入';
COMMENT ON COLUMN "events"."flag_prefix" IS 'flag 前缀（默认 flag）';
COMMENT ON COLUMN "events"."end_time" IS '结束时间';
COMMENT ON COLUMN "events"."created_at" IS '创建时间';
COMMENT ON COLUMN "events"."updated_at" IS '更新时间';

COMMENT ON TABLE "event_users" IS '赛事参赛用户表：记录个人赛事积分与封禁状态';
COMMENT ON COLUMN "event_users"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_users"."user_id" IS '用户 ID';
COMMENT ON COLUMN "event_users"."points" IS '个人赛事积分';
COMMENT ON COLUMN "event_users"."banned" IS '是否被禁赛';
COMMENT ON COLUMN "event_users"."joined_at" IS '加入时间';

COMMENT ON TABLE "event_teams" IS '赛事队伍表：团队赛的参赛队伍';
COMMENT ON COLUMN "event_teams"."id" IS '主键';
COMMENT ON COLUMN "event_teams"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_teams"."name" IS '队伍名称（赛事内唯一）';
COMMENT ON COLUMN "event_teams"."description" IS '队伍描述';
COMMENT ON COLUMN "event_teams"."points" IS '队伍积分';
COMMENT ON COLUMN "event_teams"."created_at" IS '创建时间';
COMMENT ON COLUMN "event_teams"."updated_at" IS '更新时间';
COMMENT ON COLUMN "event_teams"."banned" IS '是否被禁赛';

COMMENT ON TABLE "event_team_members" IS '赛事队伍成员表：队员与队长关系';
COMMENT ON COLUMN "event_team_members"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_team_members"."team_id" IS '队伍 ID';
COMMENT ON COLUMN "event_team_members"."user_id" IS '用户 ID';
COMMENT ON COLUMN "event_team_members"."role" IS '角色：captain 队长 / member 队员';
COMMENT ON COLUMN "event_team_members"."joined_at" IS '加入时间';

COMMENT ON TABLE "event_announcements" IS '赛事公告表';
COMMENT ON COLUMN "event_announcements"."id" IS '主键';
COMMENT ON COLUMN "event_announcements"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_announcements"."title" IS '公告标题';
COMMENT ON COLUMN "event_announcements"."content" IS '公告内容';
COMMENT ON COLUMN "event_announcements"."created_at" IS '创建时间';

COMMENT ON TABLE "event_writeup" IS '赛事 WriteUp 提交表（文件形式，存 RustFS 对象存储）';
COMMENT ON COLUMN "event_writeup"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_writeup"."user_id" IS '提交用户 ID';
COMMENT ON COLUMN "event_writeup"."team_id" IS '所属队伍 ID（可为空）';
COMMENT ON COLUMN "event_writeup"."file_url" IS 'WriteUp 文件地址（RustFS）';
COMMENT ON COLUMN "event_writeup"."created_at" IS '提交时间';

COMMENT ON TABLE "event_challenges" IS '赛事题目表：赛事包含的题目及其分值/可见性';
COMMENT ON COLUMN "event_challenges"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_challenges"."challenge_id" IS '题目 ID';
COMMENT ON COLUMN "event_challenges"."points" IS '题目分值（默认 100）';
COMMENT ON COLUMN "event_challenges"."hidden" IS '是否隐藏';

COMMENT ON TABLE "event_challenge_solves" IS '赛事解题记录：赛事内的解题流水（含队伍归属与得分）';
COMMENT ON COLUMN "event_challenge_solves"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_challenge_solves"."challenge_id" IS '题目 ID';
COMMENT ON COLUMN "event_challenge_solves"."user_id" IS '解题用户 ID';
COMMENT ON COLUMN "event_challenge_solves"."team_id" IS '所属队伍 ID（可为空）';
COMMENT ON COLUMN "event_challenge_solves"."obtained_points" IS '实际获得分值';
COMMENT ON COLUMN "event_challenge_solves"."bonus_points" IS '额外加分（如首破奖励）';
COMMENT ON COLUMN "event_challenge_solves"."created_at" IS '解题时间';

COMMENT ON TABLE "event_gameboxes" IS '赛事靶机表：赛事包含的 AWD 靶机及其可见性';
COMMENT ON COLUMN "event_gameboxes"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_gameboxes"."gamebox_id" IS '靶机模板 ID';
COMMENT ON COLUMN "event_gameboxes"."hidden" IS '是否隐藏';

COMMENT ON TABLE "event_instances" IS '赛事实例表：赛事与实例的关联（先按 instance 查 challenge，实例可共用）';
COMMENT ON COLUMN "event_instances"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_instances"."instance_id" IS '实例 ID';
COMMENT ON COLUMN "event_instances"."user_id" IS '实例归属用户 ID';
COMMENT ON COLUMN "event_instances"."team_id" IS '所属队伍 ID（可为空）';

COMMENT ON TABLE "event_logs" IS '赛事日志表：防撞库与安全审计（登录/抓 flag/启动容器等动作）';
COMMENT ON COLUMN "event_logs"."id" IS '主键';
COMMENT ON COLUMN "event_logs"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "event_logs"."user_id" IS '操作用户 ID（可为空）';
COMMENT ON COLUMN "event_logs"."team_id" IS '操作队伍 ID（可为空）';
COMMENT ON COLUMN "event_logs"."ip_address" IS '来源 IP（必须记录，防撞库、防恶意操作）';
COMMENT ON COLUMN "event_logs"."type" IS '赛事类型';
COMMENT ON COLUMN "event_logs"."level" IS '日志级别（默认 info）';
COMMENT ON COLUMN "event_logs"."action" IS '动作类型：login / capture_flag / container_start 等（可过滤）';
COMMENT ON COLUMN "event_logs"."details" IS '详细数据（JSON）';
COMMENT ON COLUMN "event_logs"."created_at" IS '创建时间';

COMMENT ON TABLE "logs" IS '系统审计日志：管理后台操作审计（登录、删文件、起容器、改密码等）';
COMMENT ON COLUMN "logs"."id" IS '主键';
COMMENT ON COLUMN "logs"."user_id" IS '操作用户 ID（可为空）';
COMMENT ON COLUMN "logs"."superadmin_id" IS '操作超级管理员 ID（哪个超管干的）';
COMMENT ON COLUMN "logs"."ip_address" IS '来源 IP（必须记录，防撞库、防恶意操作）';
COMMENT ON COLUMN "logs"."category" IS '审计分类：AUTH / SYSTEM / SERVICE / ADMIN_ACTION / WEAPONS';
COMMENT ON COLUMN "logs"."action" IS '动作描述：delete_file / start_container / update_password 等';
COMMENT ON COLUMN "logs"."level" IS '级别：debug / info / warn / error / fatal';
COMMENT ON COLUMN "logs"."message" IS '人类可读简述，如"管理员 A 删除了用户 B"';
COMMENT ON COLUMN "logs"."details" IS '差异化数据（JSON）';
COMMENT ON COLUMN "logs"."created_at" IS '创建时间';

COMMENT ON TABLE "discussions" IS '讨论帖子表（社区）';
COMMENT ON COLUMN "discussions"."id" IS '主键';
COMMENT ON COLUMN "discussions"."title" IS '帖子标题';
COMMENT ON COLUMN "discussions"."content" IS '帖子内容';
COMMENT ON COLUMN "discussions"."author_id" IS '作者用户 ID';
COMMENT ON COLUMN "discussions"."view_count" IS '浏览量';
COMMENT ON COLUMN "discussions"."like_count" IS '点赞数';
COMMENT ON COLUMN "discussions"."comment_count" IS '评论数';
COMMENT ON COLUMN "discussions"."created_at" IS '创建时间';
COMMENT ON COLUMN "discussions"."updated_at" IS '更新时间';

COMMENT ON TABLE "discussion_comments" IS '评论表：支持回复（parent_id 指向父评论）';
COMMENT ON COLUMN "discussion_comments"."id" IS '主键';
COMMENT ON COLUMN "discussion_comments"."discussion_id" IS '所属帖子 ID';
COMMENT ON COLUMN "discussion_comments"."author_id" IS '评论作者用户 ID';
COMMENT ON COLUMN "discussion_comments"."content" IS '评论内容';
COMMENT ON COLUMN "discussion_comments"."parent_id" IS '父评论 ID（NULL=顶级评论）';
COMMENT ON COLUMN "discussion_comments"."created_at" IS '创建时间';
COMMENT ON COLUMN "discussion_comments"."updated_at" IS '更新时间';

COMMENT ON TABLE "discussion_likes" IS '点赞表：用户对帖子的点赞（唯一约束防止重复点赞）';
COMMENT ON COLUMN "discussion_likes"."id" IS '主键';
COMMENT ON COLUMN "discussion_likes"."discussion_id" IS '帖子 ID';
COMMENT ON COLUMN "discussion_likes"."user_id" IS '点赞用户 ID';
COMMENT ON COLUMN "discussion_likes"."created_at" IS '点赞时间';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171904-up.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171905-index.sql
-- ================================================================================================
-- ================================================================================================

-- users 表索引：登录/邮箱唯一性查询
CREATE UNIQUE INDEX IF NOT EXISTS "idx_users_username" ON "users" ("username");

CREATE UNIQUE INDEX IF NOT EXISTS "idx_users_email" ON "users" ("email");

-- instances 表索引：按状态/题目/用户过滤实例
CREATE INDEX IF NOT EXISTS "idx_instances_status" ON "instances" ("status");

CREATE INDEX IF NOT EXISTS "idx_instances_challenge_id" ON "instances" ("challenge_id");

CREATE INDEX IF NOT EXISTS "idx_instances_user_id" ON "instances" ("user_id");

-- events 表索引：按类型/时间范围查询赛事
CREATE INDEX IF NOT EXISTS "idx_events_type" ON "events" ("type");

CREATE INDEX IF NOT EXISTS "idx_events_start_time" ON "events" ("start_time");

CREATE INDEX IF NOT EXISTS "idx_events_end_time" ON "events" ("end_time");

-- event_users 表索引：按用户/赛事查询参赛关系
CREATE INDEX IF NOT EXISTS "idx_event_users_user_id" ON "event_users" ("user_id");

CREATE INDEX IF NOT EXISTS "idx_event_users_event_id" ON "event_users" ("event_id");

-- event_instances 表索引
CREATE INDEX IF NOT EXISTS "idx_event_instances_event_id" ON "event_instances" ("event_id");

CREATE INDEX IF NOT EXISTS "idx_event_instances_instance_id" ON "event_instances" ("instance_id");

-- event_teams 表索引
CREATE INDEX IF NOT EXISTS "idx_event_teams_event_id" ON "event_teams" ("event_id");

-- event_team_members 表索引
CREATE INDEX IF NOT EXISTS "idx_event_team_members_team_id" ON "event_team_members" ("team_id");

CREATE INDEX IF NOT EXISTS "idx_event_team_members_user_id" ON "event_team_members" ("user_id");

-- 赛事实例按用户/队伍聚合查询
CREATE INDEX "idx_event_user_challenge" ON "event_instances" (
    "event_id",
    "user_id"
);

CREATE INDEX "idx_event_team_challenge" ON "event_instances" (
    "event_id",
    "team_id"
);

-- 调度器专用的极速轮询索引（只查 pending 且到期的任务，极其重要）
CREATE INDEX idx_scheduled_tasks_poll
ON "scheduled_tasks" ("status", "execute_at")
WHERE "status" = 'pending';

-- 业务组关联索引（一键销毁整组任务）
CREATE INDEX idx_scheduled_tasks_group ON "scheduled_tasks" ("group_id");

-- 赛事日志查询索引（按赛事/动作过滤，否则比赛日志一多后台查不动）
CREATE INDEX idx_event_logs_event_id ON "event_logs" ("event_id");
CREATE INDEX idx_event_logs_action ON "event_logs" ("action");

-- 系统日志索引：后台管理页通常按时间倒序查，或按类别/用户查
CREATE INDEX idx_sys_logs_created_at ON "logs" ("created_at" DESC);
CREATE INDEX idx_sys_logs_category_action ON "logs" ("category", "action");
CREATE INDEX idx_sys_logs_user_op ON "logs" ("user_id", "superadmin_id");
-- JSONB 索引：支持搜索 details 里的具体内容
CREATE INDEX idx_sys_logs_details ON "logs" USING GIN ("details");

-- 赛事日志按来源 IP 查询（防撞库审计）
CREATE INDEX idx_event_logs_ip_address ON "event_logs" ("ip_address");

-- 社区帖子/评论/点赞查询索引
CREATE INDEX idx_discussions_author ON "discussions"("author_id");
CREATE INDEX idx_discussions_created ON "discussions"("created_at" DESC);
CREATE INDEX idx_discussion_comments_discussion ON "discussion_comments"("discussion_id");
CREATE INDEX idx_discussion_comments_author ON "discussion_comments"("author_id");
CREATE INDEX idx_discussion_likes_discussion ON "discussion_likes"("discussion_id");


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171905-index.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171906-triggers.sql
-- ================================================================================================
-- ================================================================================================

-- for updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
   NEW."updated_at" = now();
   RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 为所有含 updated_at 列的表自动挂载 BEFORE UPDATE 触发器（trg_<表名>_updated_at）
DO $$
DECLARE
    t text;
    trigger_name text;
BEGIN
  FOR t IN
      SELECT table_name
      FROM information_schema.columns
      WHERE table_schema = 'public'
        AND column_name = 'updated_at'
  LOOP
      trigger_name := format('trg_%s_updated_at', t);

      -- 如果触发器存在，则删除
      IF EXISTS (
          SELECT 1
          FROM pg_trigger
          WHERE tgname = trigger_name
            AND tgrelid = (SELECT oid FROM pg_class WHERE relname = t)
      ) THEN
          EXECUTE format('DROP TRIGGER %I ON %I;', trigger_name, t);
      END IF;

      -- 创建新的触发器
      EXECUTE format(
          'CREATE TRIGGER trg_%I_updated_at
             BEFORE UPDATE ON %I
             FOR EACH ROW
             EXECUTE FUNCTION update_updated_at_column();',
          t, t
      );
  END LOOP;
END$$;

-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171906-triggers.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171907-init.sql
-- ================================================================================================
-- ================================================================================================

-- 初始化默认超级管理员账号（sysadmin，argon2id 哈希）
INSERT INTO
    "super_admin" (
        "id",
        "username",
        "password",
        "email",
        "created_at",
        "updated_at"
    )
VALUES (
        '00000000-0000-0000-0000-000000000000',
        'sysadmin',
        '$argon2id$v=19$m=19456,t=2,p=1$3THt36/y60+8SreEtA+T5A$xp4mvnbi0niUfEux7u24ZdTnv4t5QnH8ZhA/uF+GDe8',
        'sysadmin@system.com',
        '2025-09-29 13:04:49.689893',
        '2025-09-29 13:04:49.689893'
    );


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171907-init.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171908-scheduler-retry.sql
-- ================================================================================================
-- ================================================================================================

-- Incremental update (existing DBs): scheduler reliability columns (R5-B).
-- Apply manually or via floatctf-migration m0101.
-- After applying, regenerate Entity with sea-orm-cli — do NOT hand-edit entity/.
--
--   sea-orm-cli generate entity -o src/entity --with-serde both
--   (or project-standard entity regen command)
--
-- Safe to re-run (IF NOT EXISTS).

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "attempt_count" INTEGER NOT NULL DEFAULT 0;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "max_attempts" INTEGER NOT NULL DEFAULT 3;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "timeout_secs" INTEGER;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "last_error" TEXT;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "locked_at" TIMESTAMPTZ;

ALTER TABLE "scheduled_tasks"
    ADD COLUMN IF NOT EXISTS "heartbeat_at" TIMESTAMPTZ;

COMMENT ON COLUMN "scheduled_tasks"."attempt_count" IS '已尝试执行次数';
COMMENT ON COLUMN "scheduled_tasks"."max_attempts" IS '最大重试次数，超过则判定永久失败';
COMMENT ON COLUMN "scheduled_tasks"."timeout_secs" IS '单次执行超时时间（秒）';
COMMENT ON COLUMN "scheduled_tasks"."last_error" IS '最近一次失败信息（重试诊断用）';
COMMENT ON COLUMN "scheduled_tasks"."locked_at" IS '工作进程执行锁时间';
COMMENT ON COLUMN "scheduled_tasks"."heartbeat_at" IS '工作进程心跳时间（执行期间定期更新）';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171908-scheduler-retry.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171909-practice-task-key.sql
-- ================================================================================================
-- ================================================================================================

-- Rename historical misspelled scheduler task key to the correct spelling.
-- 修正历史拼写错误的调度任务键名：CHECK_PRATICE_EVENT → CHECK_PRACTICE_EVENT
UPDATE scheduled_tasks
SET task_key = 'CHECK_PRACTICE_EVENT'
WHERE task_key = 'CHECK_PRATICE_EVENT';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171909-practice-task-key.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171910-add-user-avatar.sql
-- ================================================================================================
-- ================================================================================================

-- Historical / ad-hoc incremental updates (pre-update/ directory).
-- Prefer new files under src/sql/update/ (numbered, idempotent).
--
-- Already-applied historical patches:
ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar TEXT DEFAULT NULL;

COMMENT ON COLUMN "users"."avatar" IS '用户头像 URL（默认 NULL）';

-- After this file, apply ordered scripts in src/sql/update/ when not using
-- floatctf-migration, for example:
--   psql "$DATABASE_URL" -f src/sql/update/01-scheduler-retry.sql


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171910-add-user-avatar.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171911-awd-core.sql
-- ================================================================================================
-- ================================================================================================

-- ============================================================
-- AWD (Attack With Defense) Core Tables
-- ============================================================

-- 1. AWD Events — extends the existing events table with AWD-specific configuration
CREATE TYPE "awd_event_status" AS ENUM (
    'draft', 'configuring', 'deploying', 'deployed',
    'prechecking', 'verified', 'running', 'paused',
    'network_error', 'start_blocked', 'finished',
    'archived', 'deploy_failed', 'verification_failed'
);

CREATE TYPE "awd_phase" AS ENUM (
    'hardening', 'attack', 'pause'
);

CREATE TABLE IF NOT EXISTS "awd_events" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL UNIQUE REFERENCES "events" ("id") ON DELETE CASCADE,
    "status" "awd_event_status" NOT NULL DEFAULT 'draft',
    "phase" "awd_phase" NOT NULL DEFAULT 'hardening',

    -- Network configuration (locked after first deployment)
    "gamebox_cidr" VARCHAR(18) NOT NULL,           -- e.g. "10.0.0.0/16"
    "wireguard_cidr" VARCHAR(18) NOT NULL,          -- e.g. "10.1.0.0/16"
    "wireguard_interface_name" VARCHAR(15) NOT NULL UNIQUE,
    "wireguard_listen_port" INTEGER NOT NULL UNIQUE,
    "flagserver_ip" VARCHAR(15) NOT NULL,
    "judgeserver_ip" VARCHAR(15) NOT NULL,
    "docker_network_id" VARCHAR(64),
    "docker_network_name" VARCHAR(64),

    -- Encrypted secrets
    "event_secret_ciphertext" BYTEA NOT NULL,
    "event_secret_nonce" BYTEA NOT NULL,
    "flagserver_token_ciphertext" BYTEA,
    "flagserver_token_nonce" BYTEA,
    "judgeserver_token_ciphertext" BYTEA,
    "judgeserver_token_nonce" BYTEA,
    "wg_server_private_key_ciphertext" BYTEA,
    "wg_server_private_key_nonce" BYTEA,
    "wg_server_public_key" VARCHAR(44),
    "key_version" INTEGER NOT NULL DEFAULT 1,

    -- Scoring & reset
    "free_reset_count" INTEGER NOT NULL DEFAULT 3,
    "extra_reset_penalty" BIGINT NOT NULL DEFAULT 100,
    "reset_protection_secs" INTEGER NOT NULL DEFAULT 120,

    -- Judge configuration
    "judge_max_concurrency" INTEGER NOT NULL DEFAULT 10,
    "judge_default_timeout_secs" INTEGER NOT NULL DEFAULT 30,
    "judge_retry_interval_secs" INTEGER NOT NULL DEFAULT 5,
    "judge_grace_period_secs" INTEGER NOT NULL DEFAULT 30,
    "round_duration_secs" INTEGER NOT NULL DEFAULT 300,

    -- Archive
    "archive_retention_hours" INTEGER NOT NULL DEFAULT 168,

    -- Verification
    "verified_at" TIMESTAMPTZ,
    "verified_revision" TEXT,

    -- Timing
    "pause_remaining_secs" INTEGER,
    "started_at" TIMESTAMPTZ,
    "finished_at" TIMESTAMPTZ,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 2. Team network assignments per event
CREATE TABLE IF NOT EXISTS "awd_team_networks" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "gamebox_subnet" VARCHAR(18) NOT NULL,          -- e.g. "10.0.1.0/24"
    "wireguard_subnet" VARCHAR(18) NOT NULL,         -- e.g. "10.1.1.0/24"
    "ssh_password_ciphertext" BYTEA NOT NULL,
    "ssh_password_nonce" BYTEA NOT NULL,
    "key_version" INTEGER NOT NULL DEFAULT 1,
    "next_gamebox_host" INTEGER NOT NULL DEFAULT 2,  -- next host byte to allocate
    "next_wireguard_host" INTEGER NOT NULL DEFAULT 2,
    "status" VARCHAR(20) NOT NULL DEFAULT 'active',
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "team_id"),
    UNIQUE ("event_id", "gamebox_subnet"),
    UNIQUE ("event_id", "wireguard_subnet")
);

-- 3. GameBox templates (per event)
CREATE TABLE IF NOT EXISTS "awd_gamebox_templates" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "challenge_id" UUID REFERENCES "challenges" ("id") ON DELETE SET NULL,
    "name" VARCHAR(200) NOT NULL,
    "image_ref" VARCHAR(500) NOT NULL,
    "username" VARCHAR(100) NOT NULL DEFAULT 'ctf',
    "meta_json" JSONB NOT NULL DEFAULT '{}',
    "cpu_millis" BIGINT NOT NULL DEFAULT 1000,
    "memory_bytes" BIGINT NOT NULL DEFAULT 536870912,    -- 512MB
    "pids_limit" BIGINT NOT NULL DEFAULT 100,
    "healthcheck_override_json" JSONB,
    "judge_script_name" VARCHAR(200),
    "judge_script_content" TEXT,
    "judge_args_json" JSONB,
    "judge_timeout_secs" INTEGER,
    "judge_retry_interval_secs" INTEGER,
    "break_points" BIGINT NOT NULL DEFAULT 100,
    "loss_points" BIGINT NOT NULL DEFAULT 100,
    "fix_points" BIGINT NOT NULL DEFAULT 100,
    "down_points" BIGINT NOT NULL DEFAULT 200,
    "first_bonus" BIGINT NOT NULL DEFAULT 20,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "name")
);

-- 4. GameBox instances (per team per template)
CREATE TYPE "gamebox_status" AS ENUM (
    'pending', 'creating', 'running', 'ready',
    'resetting', 'missing', 'orphan', 'conflict',
    'start_failed', 'reset_failed', 'stopped'
);

CREATE TABLE IF NOT EXISTS "awd_gamebox_instances" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "template_id" UUID NOT NULL REFERENCES "awd_gamebox_templates" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "status" "gamebox_status" NOT NULL DEFAULT 'pending',
    "container_id" VARCHAR(64),
    "container_name" VARCHAR(200) NOT NULL UNIQUE,
    "gamebox_ip" VARCHAR(15) NOT NULL,
    "docker_network_id" VARCHAR(64),
    "health_status" VARCHAR(20) NOT NULL DEFAULT 'unknown',
    "reset_protection_until" TIMESTAMPTZ,
    "last_health_check_at" TIMESTAMPTZ,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "deleted_at" TIMESTAMPTZ,
    UNIQUE ("event_id", "gamebox_ip"),
    UNIQUE ("event_id", "template_id", "team_id")
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_events" IS 'AWD 赛事配置：在 events 表基础上扩展的 AWD 专有配置（网络、加密密钥、计分、判题参数）';
COMMENT ON COLUMN "awd_events"."id" IS '主键';
COMMENT ON COLUMN "awd_events"."event_id" IS '关联赛事 ID（唯一）';
COMMENT ON COLUMN "awd_events"."status" IS 'AWD 赛事状态机：draft→configuring→deploying→deployed→prechecking→verified→running→…';
COMMENT ON COLUMN "awd_events"."phase" IS '当前阶段：hardening 加固 / attack 攻击 / pause 暂停';
COMMENT ON COLUMN "awd_events"."gamebox_cidr" IS '靶机网络网段，如 10.0.0.0/16（首次部署后锁定）';
COMMENT ON COLUMN "awd_events"."wireguard_cidr" IS 'WireGuard 网络网段，如 10.1.0.0/16（首次部署后锁定）';
COMMENT ON COLUMN "awd_events"."wireguard_interface_name" IS 'WireGuard 网卡名（唯一）';
COMMENT ON COLUMN "awd_events"."wireguard_listen_port" IS 'WireGuard 监听端口（唯一）';
COMMENT ON COLUMN "awd_events"."flagserver_ip" IS 'Flag 服务器 IP';
COMMENT ON COLUMN "awd_events"."judgeserver_ip" IS '判题服务器 IP';
COMMENT ON COLUMN "awd_events"."docker_network_id" IS 'Docker 网络 ID（可为空）';
COMMENT ON COLUMN "awd_events"."docker_network_name" IS 'Docker 网络名（可为空）';
COMMENT ON COLUMN "awd_events"."event_secret_ciphertext" IS '事件密钥密文（加密存储）';
COMMENT ON COLUMN "awd_events"."event_secret_nonce" IS '事件密钥加密 nonce';
COMMENT ON COLUMN "awd_events"."flagserver_token_ciphertext" IS 'Flag 服务器令牌密文';
COMMENT ON COLUMN "awd_events"."flagserver_token_nonce" IS 'Flag 服务器令牌 nonce';
COMMENT ON COLUMN "awd_events"."judgeserver_token_ciphertext" IS '判题服务器令牌密文';
COMMENT ON COLUMN "awd_events"."judgeserver_token_nonce" IS '判题服务器令牌 nonce';
COMMENT ON COLUMN "awd_events"."wg_server_private_key_ciphertext" IS 'WireGuard 服务端私钥密文';
COMMENT ON COLUMN "awd_events"."wg_server_private_key_nonce" IS 'WireGuard 服务端私钥 nonce';
COMMENT ON COLUMN "awd_events"."wg_server_public_key" IS 'WireGuard 服务端公钥';
COMMENT ON COLUMN "awd_events"."key_version" IS '密钥版本（轮换时递增）';
COMMENT ON COLUMN "awd_events"."free_reset_count" IS '每队免费重置次数（默认 3）';
COMMENT ON COLUMN "awd_events"."extra_reset_penalty" IS '超出免费次数后的额外重置惩罚分（默认 100）';
COMMENT ON COLUMN "awd_events"."reset_protection_secs" IS '重置保护期（秒）：重置后一段时间内不可再次重置（默认 120）';
COMMENT ON COLUMN "awd_events"."judge_max_concurrency" IS '判题最大并发数（默认 10）';
COMMENT ON COLUMN "awd_events"."judge_default_timeout_secs" IS '判题默认超时（秒，默认 30）';
COMMENT ON COLUMN "awd_events"."judge_retry_interval_secs" IS '判题失败重试间隔（秒，默认 5）';
COMMENT ON COLUMN "awd_events"."judge_grace_period_secs" IS '判题宽限期（秒，默认 30）：回合结束后的判题缓冲时间';
COMMENT ON COLUMN "awd_events"."round_duration_secs" IS '单回合时长（秒，默认 300）';
COMMENT ON COLUMN "awd_events"."archive_retention_hours" IS '归档保留时长（小时，默认 168）';
COMMENT ON COLUMN "awd_events"."verified_at" IS '验证通过时间';
COMMENT ON COLUMN "awd_events"."verified_revision" IS '验证通过的配置版本';
COMMENT ON COLUMN "awd_events"."pause_remaining_secs" IS '暂停时剩余的回合秒数（恢复时续走）';
COMMENT ON COLUMN "awd_events"."started_at" IS '比赛开始时间';
COMMENT ON COLUMN "awd_events"."finished_at" IS '比赛结束时间';
COMMENT ON COLUMN "awd_events"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_events"."updated_at" IS '更新时间';

COMMENT ON TABLE "awd_team_networks" IS 'AWD 队伍网络分配：每赛事每队伍的靶机/ WireGuard 子网与 SSH 凭据';
COMMENT ON COLUMN "awd_team_networks"."id" IS '主键';
COMMENT ON COLUMN "awd_team_networks"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_team_networks"."team_id" IS '队伍 ID';
COMMENT ON COLUMN "awd_team_networks"."gamebox_subnet" IS '队伍靶机子网，如 10.0.1.0/24';
COMMENT ON COLUMN "awd_team_networks"."wireguard_subnet" IS '队伍 WireGuard 子网，如 10.1.1.0/24';
COMMENT ON COLUMN "awd_team_networks"."ssh_password_ciphertext" IS '靶机 SSH 密码密文';
COMMENT ON COLUMN "awd_team_networks"."ssh_password_nonce" IS 'SSH 密码加密 nonce';
COMMENT ON COLUMN "awd_team_networks"."key_version" IS '密钥版本';
COMMENT ON COLUMN "awd_team_networks"."next_gamebox_host" IS '下一个可分配的靶机主机位（从 2 开始）';
COMMENT ON COLUMN "awd_team_networks"."next_wireguard_host" IS '下一个可分配的 WireGuard 主机位（从 2 开始）';
COMMENT ON COLUMN "awd_team_networks"."status" IS '状态（默认 active）';
COMMENT ON COLUMN "awd_team_networks"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_team_networks"."updated_at" IS '更新时间';

COMMENT ON TABLE "awd_gamebox_templates" IS 'AWD 靶机模板：赛事内的靶机定义（镜像、资源限制、计分与判题脚本）';
COMMENT ON COLUMN "awd_gamebox_templates"."id" IS '主键';
COMMENT ON COLUMN "awd_gamebox_templates"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_gamebox_templates"."challenge_id" IS '关联题目 ID（可为空，删除时置 NULL）';
COMMENT ON COLUMN "awd_gamebox_templates"."name" IS '模板名称（赛事内唯一）';
COMMENT ON COLUMN "awd_gamebox_templates"."image_ref" IS '容器镜像引用';
COMMENT ON COLUMN "awd_gamebox_templates"."username" IS '容器 SSH 用户名（默认 ctf）';
COMMENT ON COLUMN "awd_gamebox_templates"."meta_json" IS '模板元数据（JSON）';
COMMENT ON COLUMN "awd_gamebox_templates"."cpu_millis" IS 'CPU 限制（毫核，默认 1000）';
COMMENT ON COLUMN "awd_gamebox_templates"."memory_bytes" IS '内存限制（字节，默认 512MB）';
COMMENT ON COLUMN "awd_gamebox_templates"."pids_limit" IS '进程数上限（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."healthcheck_override_json" IS '健康检查配置覆盖（JSON）';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_script_name" IS '判题脚本文件名';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_script_content" IS '判题脚本内容';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_args_json" IS '判题脚本参数（JSON）';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_timeout_secs" IS '判题超时（秒）';
COMMENT ON COLUMN "awd_gamebox_templates"."judge_retry_interval_secs" IS '判题重试间隔（秒）';
COMMENT ON COLUMN "awd_gamebox_templates"."break_points" IS '被攻破时攻击方得分（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."loss_points" IS '被攻破时防守方失分（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."fix_points" IS '修复得分（默认 100）';
COMMENT ON COLUMN "awd_gamebox_templates"."down_points" IS '宕机扣分（默认 200）';
COMMENT ON COLUMN "awd_gamebox_templates"."first_bonus" IS '首破奖励（默认 20）';
COMMENT ON COLUMN "awd_gamebox_templates"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_gamebox_templates"."updated_at" IS '更新时间';

COMMENT ON TABLE "awd_gamebox_instances" IS 'AWD 靶机实例：每队伍每模板实际部署的容器实例';
COMMENT ON COLUMN "awd_gamebox_instances"."id" IS '主键';
COMMENT ON COLUMN "awd_gamebox_instances"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."template_id" IS '靶机模板 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."team_id" IS '队伍 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."status" IS '实例状态：pending/creating/running/ready/resetting/missing/orphan/conflict/start_failed/reset_failed/stopped';
COMMENT ON COLUMN "awd_gamebox_instances"."container_id" IS 'Docker 容器 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."container_name" IS '容器名（唯一）';
COMMENT ON COLUMN "awd_gamebox_instances"."gamebox_ip" IS '靶机内网 IP（赛事内唯一）';
COMMENT ON COLUMN "awd_gamebox_instances"."docker_network_id" IS '所在 Docker 网络 ID';
COMMENT ON COLUMN "awd_gamebox_instances"."health_status" IS '健康状态（默认 unknown）';
COMMENT ON COLUMN "awd_gamebox_instances"."reset_protection_until" IS '重置保护截止时间（此时间前不可重置）';
COMMENT ON COLUMN "awd_gamebox_instances"."last_health_check_at" IS '最近一次健康检查时间';
COMMENT ON COLUMN "awd_gamebox_instances"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_gamebox_instances"."updated_at" IS '更新时间';
COMMENT ON COLUMN "awd_gamebox_instances"."deleted_at" IS '软删除时间（NULL=未删除）';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171911-awd-core.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171912-awd-wireguard.sql
-- ================================================================================================
-- ================================================================================================

-- ============================================================
-- AWD WireGuard Tables
-- ============================================================

CREATE TYPE "wg_peer_status" AS ENUM (
    'active', 'revoked', 'rotating'
);

CREATE TABLE IF NOT EXISTS "awd_wireguard_peers" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "status" "wg_peer_status" NOT NULL DEFAULT 'active',
    "assigned_ip" VARCHAR(15) NOT NULL,             -- /32 assigned IP
    "public_key" VARCHAR(44) NOT NULL,
    "private_key_ciphertext" BYTEA NOT NULL,
    "private_key_nonce" BYTEA NOT NULL,
    "key_version" INTEGER NOT NULL DEFAULT 1,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "rotated_at" TIMESTAMPTZ,
    "revoked_at" TIMESTAMPTZ,
    UNIQUE ("event_id", "user_id"),
    UNIQUE ("event_id", "assigned_ip"),
    UNIQUE ("public_key")
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_wireguard_peers" IS 'AWD WireGuard 对等端：队伍成员接入靶机网络的 VPN 客户端（密钥加密存储，支持轮换/吊销）';
COMMENT ON COLUMN "awd_wireguard_peers"."id" IS '主键';
COMMENT ON COLUMN "awd_wireguard_peers"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_wireguard_peers"."team_id" IS '所属队伍 ID';
COMMENT ON COLUMN "awd_wireguard_peers"."user_id" IS '成员用户 ID';
COMMENT ON COLUMN "awd_wireguard_peers"."status" IS '状态：active 生效 / revoked 已吊销 / rotating 轮换中';
COMMENT ON COLUMN "awd_wireguard_peers"."assigned_ip" IS '分配的 /32 对端 IP';
COMMENT ON COLUMN "awd_wireguard_peers"."public_key" IS '对端公钥（唯一）';
COMMENT ON COLUMN "awd_wireguard_peers"."private_key_ciphertext" IS '对端私钥密文';
COMMENT ON COLUMN "awd_wireguard_peers"."private_key_nonce" IS '私钥加密 nonce';
COMMENT ON COLUMN "awd_wireguard_peers"."key_version" IS '密钥版本';
COMMENT ON COLUMN "awd_wireguard_peers"."created_at" IS '创建时间';
COMMENT ON COLUMN "awd_wireguard_peers"."rotated_at" IS '最近密钥轮换时间';
COMMENT ON COLUMN "awd_wireguard_peers"."revoked_at" IS '吊销时间';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171912-awd-wireguard.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171913-awd-rounds-flags-scores.sql
-- ================================================================================================
-- ================================================================================================

-- ============================================================
-- AWD Rounds, Flags, Submissions, and Score Events
-- ============================================================

-- 5. Rounds
CREATE TYPE "round_status" AS ENUM (
    'active', 'grace', 'completed', 'paused'
);

CREATE TABLE IF NOT EXISTS "awd_rounds" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_number" INTEGER NOT NULL,
    "status" "round_status" NOT NULL DEFAULT 'active',
    "phase" "awd_phase" NOT NULL DEFAULT 'attack',
    "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "scheduled_end_at" TIMESTAMPTZ NOT NULL,
    "grace_ends_at" TIMESTAMPTZ,
    "paused_at" TIMESTAMPTZ,
    "remaining_secs" INTEGER,
    "completed_at" TIMESTAMPTZ,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_number")
);

-- At most one active round per event
CREATE UNIQUE INDEX IF NOT EXISTS "idx_awd_rounds_one_active"
    ON "awd_rounds" ("event_id", "status")
    WHERE "status" IN ('active', 'grace', 'paused');

-- 6. Flag issues (deterministic, per GameBox per round)
CREATE TABLE IF NOT EXISTS "awd_flag_issues" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "flag_hash" VARCHAR(128) NOT NULL,              -- SHA-256 hash of the flag
    "issued_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_id", "gamebox_instance_id"),
    UNIQUE ("event_id", "round_id", "flag_hash")
);

-- 7. Flag submissions
CREATE TABLE IF NOT EXISTS "awd_flag_submissions" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "flag_issue_id" UUID NOT NULL REFERENCES "awd_flag_issues" ("id") ON DELETE CASCADE,
    "attacker_team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "victim_team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "submitted_by_user_id" UUID NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "submitted_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_id", "attacker_team_id", "gamebox_instance_id")
);

-- 8. Score events (append-only ledger)
CREATE TYPE "score_event_type" AS ENUM (
    'attack', 'victim_loss', 'judge_fix', 'judge_down',
    'first_bonus', 'reset_penalty', 'adjustment'
);

CREATE TABLE IF NOT EXISTS "awd_score_events" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "event_type" "score_event_type" NOT NULL,
    "delta" BIGINT NOT NULL,
    "idempotency_key" VARCHAR(300) NOT NULL UNIQUE,
    "related_team_id" UUID REFERENCES "event_teams" ("id") ON DELETE SET NULL,
    "gamebox_instance_id" UUID REFERENCES "awd_gamebox_instances" ("id") ON DELETE SET NULL,
    "gamebox_template_id" UUID REFERENCES "awd_gamebox_templates" ("id") ON DELETE SET NULL,
    "reference_id" UUID,
    "reason" TEXT,
    "metadata_json" JSONB NOT NULL DEFAULT '{}',
    "created_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_rounds" IS 'AWD 回合表：比赛按固定时长推进的回合（含宽限期、暂停与完成状态）';
COMMENT ON COLUMN "awd_rounds"."id" IS '主键';
COMMENT ON COLUMN "awd_rounds"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_rounds"."round_number" IS '回合序号（赛事内唯一）';
COMMENT ON COLUMN "awd_rounds"."status" IS '回合状态：active / grace 宽限 / completed / paused';
COMMENT ON COLUMN "awd_rounds"."phase" IS '回合阶段（默认 attack）';
COMMENT ON COLUMN "awd_rounds"."started_at" IS '回合开始时间';
COMMENT ON COLUMN "awd_rounds"."scheduled_end_at" IS '计划结束时间';
COMMENT ON COLUMN "awd_rounds"."grace_ends_at" IS '宽限期结束时间（可为空）';
COMMENT ON COLUMN "awd_rounds"."paused_at" IS '暂停时间（可为空）';
COMMENT ON COLUMN "awd_rounds"."remaining_secs" IS '暂停时剩余秒数（恢复时续走）';
COMMENT ON COLUMN "awd_rounds"."completed_at" IS '完成时间';
COMMENT ON COLUMN "awd_rounds"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_flag_issues" IS 'AWD Flag 发放表：每轮每靶机确定性生成 flag（只存哈希，防泄密）';
COMMENT ON COLUMN "awd_flag_issues"."id" IS '主键';
COMMENT ON COLUMN "awd_flag_issues"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_flag_issues"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_flag_issues"."gamebox_instance_id" IS '靶机实例 ID';
COMMENT ON COLUMN "awd_flag_issues"."flag_hash" IS 'flag 的 SHA-256 哈希';
COMMENT ON COLUMN "awd_flag_issues"."issued_at" IS '发放时间';

COMMENT ON TABLE "awd_flag_submissions" IS 'AWD Flag 提交表：攻击方提交对方靶机 flag 的记录';
COMMENT ON COLUMN "awd_flag_submissions"."id" IS '主键';
COMMENT ON COLUMN "awd_flag_submissions"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_flag_submissions"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_flag_submissions"."flag_issue_id" IS '对应的 flag 发放记录 ID';
COMMENT ON COLUMN "awd_flag_submissions"."attacker_team_id" IS '攻击方队伍 ID';
COMMENT ON COLUMN "awd_flag_submissions"."victim_team_id" IS '受害方队伍 ID';
COMMENT ON COLUMN "awd_flag_submissions"."gamebox_instance_id" IS '被攻击的靶机实例 ID';
COMMENT ON COLUMN "awd_flag_submissions"."submitted_by_user_id" IS '提交用户 ID';
COMMENT ON COLUMN "awd_flag_submissions"."submitted_at" IS '提交时间';

COMMENT ON TABLE "awd_score_events" IS 'AWD 积分事件账本：只追加（append-only），所有得分/扣分/调整的审计轨迹';
COMMENT ON COLUMN "awd_score_events"."id" IS '主键';
COMMENT ON COLUMN "awd_score_events"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_score_events"."round_id" IS '回合 ID（可为空）';
COMMENT ON COLUMN "awd_score_events"."team_id" IS '产生积分变化的队伍 ID';
COMMENT ON COLUMN "awd_score_events"."event_type" IS '事件类型：attack 攻击得分 / victim_loss 受害失分 / judge_fix 修复 / judge_down 宕机 / first_bonus 首破 / reset_penalty 重置惩罚 / adjustment 人工调整';
COMMENT ON COLUMN "awd_score_events"."delta" IS '积分变化量（正为得分，负为扣分）';
COMMENT ON COLUMN "awd_score_events"."idempotency_key" IS '幂等键（唯一，防止重复记账）';
COMMENT ON COLUMN "awd_score_events"."related_team_id" IS '关联队伍（如攻击/受害中的另一方，可为空）';
COMMENT ON COLUMN "awd_score_events"."gamebox_instance_id" IS '关联靶机实例（可为空）';
COMMENT ON COLUMN "awd_score_events"."gamebox_template_id" IS '关联靶机模板（可为空）';
COMMENT ON COLUMN "awd_score_events"."reference_id" IS '参考 ID（如关联的重置记录，可为空）';
COMMENT ON COLUMN "awd_score_events"."reason" IS '事件原因说明';
COMMENT ON COLUMN "awd_score_events"."metadata_json" IS '附加元数据（JSON）';
COMMENT ON COLUMN "awd_score_events"."created_by" IS '创建人（超级管理员，人工调整时有值）';
COMMENT ON COLUMN "awd_score_events"."created_at" IS '创建时间';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171913-awd-rounds-flags-scores.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171914-awd-judge-reset-ban.sql
-- ================================================================================================
-- ================================================================================================

-- ============================================================
-- AWD Judge, Reset, and Ban Tables
-- ============================================================

-- 9. Judge batches
CREATE TABLE IF NOT EXISTS "awd_judge_batches" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "total_tasks" INTEGER NOT NULL DEFAULT 0,
    "completed_tasks" INTEGER NOT NULL DEFAULT 0,
    "failed_tasks" INTEGER NOT NULL DEFAULT 0,
    "status" VARCHAR(20) NOT NULL DEFAULT 'pending',
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 10. Judge tasks
CREATE TYPE "judge_task_status" AS ENUM (
    'pending', 'running', 'up', 'down',
    'judge_error', 'judge_timeout',
    'skipped_resetting', 'skipped_banned'
);

CREATE TABLE IF NOT EXISTS "awd_judge_tasks" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "batch_id" UUID NOT NULL REFERENCES "awd_judge_batches" ("id") ON DELETE CASCADE,
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "round_id" UUID NOT NULL REFERENCES "awd_rounds" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "template_id" UUID NOT NULL REFERENCES "awd_gamebox_templates" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "status" "judge_task_status" NOT NULL DEFAULT 'pending',
    "attempt_count" INTEGER NOT NULL DEFAULT 0,
    "max_attempts" INTEGER NOT NULL DEFAULT 2,
    "deadline_at" TIMESTAMPTZ NOT NULL,
    "started_at" TIMESTAMPTZ,
    "finished_at" TIMESTAMPTZ,
    "exit_code" INTEGER,
    "stdout_limited" TEXT,
    "stderr_limited" TEXT,
    "duration_ms" INTEGER,
    "callback_idempotency_key" VARCHAR(300),
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "round_id", "gamebox_instance_id", "template_id")
);

-- 11. Reset records
CREATE TABLE IF NOT EXISTS "awd_reset_records" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "gamebox_instance_id" UUID NOT NULL REFERENCES "awd_gamebox_instances" ("id") ON DELETE CASCADE,
    "round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "requested_by" UUID REFERENCES "users" ("id") ON DELETE SET NULL,
    "free_reset" BOOLEAN NOT NULL DEFAULT TRUE,
    "penalty_score_event_id" UUID,
    "status" VARCHAR(20) NOT NULL DEFAULT 'pending',
    "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "completed_at" TIMESTAMPTZ,
    "error_msg" TEXT,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 12. Team bans
CREATE TYPE "ban_status" AS ENUM (
    'active', 'pending_unban', 'unbanned'
);

CREATE TABLE IF NOT EXISTS "awd_team_bans" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "team_id" UUID NOT NULL REFERENCES "event_teams" ("id") ON DELETE CASCADE,
    "status" "ban_status" NOT NULL DEFAULT 'active',
    "reason" TEXT,
    "effective_round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "banned_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "banned_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "unban_requested_at" TIMESTAMPTZ,
    "unban_effective_round_id" UUID REFERENCES "awd_rounds" ("id") ON DELETE SET NULL,
    "unbanned_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "unbanned_at" TIMESTAMPTZ
);

-- At most one active ban per team per event
CREATE UNIQUE INDEX IF NOT EXISTS "idx_awd_team_bans_one_active"
    ON "awd_team_bans" ("event_id", "team_id")
    WHERE "status" = 'active';


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_judge_batches" IS 'AWD 判题批次：每回合发起的一批判题任务的汇总（进度与结果统计）';
COMMENT ON COLUMN "awd_judge_batches"."id" IS '主键';
COMMENT ON COLUMN "awd_judge_batches"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_judge_batches"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_judge_batches"."total_tasks" IS '总任务数';
COMMENT ON COLUMN "awd_judge_batches"."completed_tasks" IS '已完成任务数';
COMMENT ON COLUMN "awd_judge_batches"."failed_tasks" IS '失败任务数';
COMMENT ON COLUMN "awd_judge_batches"."status" IS '批次状态（默认 pending）';
COMMENT ON COLUMN "awd_judge_batches"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_judge_tasks" IS 'AWD 判题任务：对每个靶机实例执行健康/服务判定的单个任务（含重试与输出记录）';
COMMENT ON COLUMN "awd_judge_tasks"."id" IS '主键';
COMMENT ON COLUMN "awd_judge_tasks"."batch_id" IS '所属判题批次 ID';
COMMENT ON COLUMN "awd_judge_tasks"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_judge_tasks"."round_id" IS '回合 ID';
COMMENT ON COLUMN "awd_judge_tasks"."gamebox_instance_id" IS '被判定靶机实例 ID';
COMMENT ON COLUMN "awd_judge_tasks"."template_id" IS '靶机模板 ID';
COMMENT ON COLUMN "awd_judge_tasks"."team_id" IS '所属队伍 ID';
COMMENT ON COLUMN "awd_judge_tasks"."status" IS '任务状态：pending/running/up/down/judge_error/judge_timeout/skipped_resetting/skipped_banned';
COMMENT ON COLUMN "awd_judge_tasks"."attempt_count" IS '已尝试次数';
COMMENT ON COLUMN "awd_judge_tasks"."max_attempts" IS '最大尝试次数（默认 2）';
COMMENT ON COLUMN "awd_judge_tasks"."deadline_at" IS '执行截止时间';
COMMENT ON COLUMN "awd_judge_tasks"."started_at" IS '开始执行时间';
COMMENT ON COLUMN "awd_judge_tasks"."finished_at" IS '执行完成时间';
COMMENT ON COLUMN "awd_judge_tasks"."exit_code" IS '判题脚本退出码';
COMMENT ON COLUMN "awd_judge_tasks"."stdout_limited" IS '截断后的标准输出';
COMMENT ON COLUMN "awd_judge_tasks"."stderr_limited" IS '截断后的标准错误输出';
COMMENT ON COLUMN "awd_judge_tasks"."duration_ms" IS '执行耗时（毫秒）';
COMMENT ON COLUMN "awd_judge_tasks"."callback_idempotency_key" IS '回调幂等键（防止判题回调重复处理）';
COMMENT ON COLUMN "awd_judge_tasks"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_reset_records" IS 'AWD 靶机重置记录：队伍请求重置靶机的流水（含免费/惩罚、执行状态）';
COMMENT ON COLUMN "awd_reset_records"."id" IS '主键';
COMMENT ON COLUMN "awd_reset_records"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_reset_records"."team_id" IS '请求队伍 ID';
COMMENT ON COLUMN "awd_reset_records"."gamebox_instance_id" IS '被重置的靶机实例 ID';
COMMENT ON COLUMN "awd_reset_records"."round_id" IS '请求所在回合 ID（可为空）';
COMMENT ON COLUMN "awd_reset_records"."requested_by" IS '请求用户 ID（可为空）';
COMMENT ON COLUMN "awd_reset_records"."free_reset" IS '是否免费重置（超出免费次数则扣分）';
COMMENT ON COLUMN "awd_reset_records"."penalty_score_event_id" IS '扣除的惩罚积分事件 ID';
COMMENT ON COLUMN "awd_reset_records"."status" IS '重置状态（默认 pending）';
COMMENT ON COLUMN "awd_reset_records"."started_at" IS '开始执行时间';
COMMENT ON COLUMN "awd_reset_records"."completed_at" IS '完成时间';
COMMENT ON COLUMN "awd_reset_records"."error_msg" IS '失败原因';
COMMENT ON COLUMN "awd_reset_records"."created_at" IS '创建时间';

COMMENT ON TABLE "awd_team_bans" IS 'AWD 队伍封禁表：因违规被封禁的队伍（含申请解封与生效回合）';
COMMENT ON COLUMN "awd_team_bans"."id" IS '主键';
COMMENT ON COLUMN "awd_team_bans"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_team_bans"."team_id" IS '被封禁队伍 ID';
COMMENT ON COLUMN "awd_team_bans"."status" IS '封禁状态：active / pending_unban 待解封 / unbanned';
COMMENT ON COLUMN "awd_team_bans"."reason" IS '封禁原因';
COMMENT ON COLUMN "awd_team_bans"."effective_round_id" IS '封禁生效回合（可为空）';
COMMENT ON COLUMN "awd_team_bans"."banned_by" IS '封禁人（超级管理员）';
COMMENT ON COLUMN "awd_team_bans"."banned_at" IS '封禁时间';
COMMENT ON COLUMN "awd_team_bans"."unban_requested_at" IS '申请解封时间';
COMMENT ON COLUMN "awd_team_bans"."unban_effective_round_id" IS '解封生效回合（可为空）';
COMMENT ON COLUMN "awd_team_bans"."unbanned_by" IS '解封人（超级管理员）';
COMMENT ON COLUMN "awd_team_bans"."unbanned_at" IS '解封时间';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171914-awd-judge-reset-ban.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171915-awd-precheck-runtime.sql
-- ================================================================================================
-- ================================================================================================

-- ============================================================
-- AWD Precheck and Runtime Tables
-- ============================================================

-- 13. Precheck runs
CREATE TYPE "precheck_status" AS ENUM (
    'pending', 'running', 'passed', 'failed', 'error'
);

CREATE TABLE IF NOT EXISTS "awd_precheck_runs" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "status" "precheck_status" NOT NULL DEFAULT 'pending',
    "trigger" VARCHAR(20) NOT NULL DEFAULT 'manual',  -- manual, auto_t_minus_1h
    "revision" TEXT,
    "config_check" JSONB,
    "container_check" JSONB,
    "wireguard_check" JSONB,
    "network_check" JSONB,
    "flag_check" JSONB,
    "judge_check" JSONB,
    "error_msg" TEXT,
    "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "completed_at" TIMESTAMPTZ
);

-- 14. Runtime resources (for reconciliation)
CREATE TABLE IF NOT EXISTS "awd_runtime_resources" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "resource_type" VARCHAR(50) NOT NULL,           -- docker_network, container, wireguard_iface
    "resource_id" VARCHAR(200) NOT NULL,
    "resource_name" VARCHAR(200),
    "observed_state" JSONB,
    "last_seen_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE ("event_id", "resource_type", "resource_id")
);

-- 15. Orphan resources (DB has no record, Docker/WG has resource)
CREATE TABLE IF NOT EXISTS "awd_orphan_resources" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID REFERENCES "events" ("id") ON DELETE SET NULL,
    "resource_type" VARCHAR(50) NOT NULL,
    "resource_id" VARCHAR(200) NOT NULL,
    "resource_name" VARCHAR(200),
    "observed_state" JSONB,
    "discovered_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "resolved_at" TIMESTAMPTZ,
    "resolution" VARCHAR(20) DEFAULT 'pending'     -- pending, adopted, cleaned
);

-- 16. Internal token rotations (audit trail)
CREATE TABLE IF NOT EXISTS "awd_internal_token_rotations" (
    "id" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    "event_id" UUID NOT NULL REFERENCES "events" ("id") ON DELETE CASCADE,
    "token_type" VARCHAR(30) NOT NULL,              -- flagserver, judgeserver, event_secret
    "rotated_by" UUID REFERENCES "super_admin" ("id") ON DELETE SET NULL,
    "rotated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- =====================================================================
-- 表与列中文注释（可重复执行）
-- =====================================================================

COMMENT ON TABLE "awd_precheck_runs" IS 'AWD 赛前检查：比赛开始前对配置/容器/WireGuard/网络/flag/判题的整体体检记录';
COMMENT ON COLUMN "awd_precheck_runs"."id" IS '主键';
COMMENT ON COLUMN "awd_precheck_runs"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_precheck_runs"."status" IS '检查状态：pending / running / passed / failed / error';
COMMENT ON COLUMN "awd_precheck_runs"."trigger" IS '触发方式：manual 手动 / auto_t_minus_1h 开赛前 1 小时自动';
COMMENT ON COLUMN "awd_precheck_runs"."revision" IS '被检查的配置版本';
COMMENT ON COLUMN "awd_precheck_runs"."config_check" IS '配置检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."container_check" IS '容器检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."wireguard_check" IS 'WireGuard 检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."network_check" IS '网络检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."flag_check" IS 'flag 检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."judge_check" IS '判题检查结果（JSON）';
COMMENT ON COLUMN "awd_precheck_runs"."error_msg" IS '检查失败原因';
COMMENT ON COLUMN "awd_precheck_runs"."started_at" IS '检查开始时间';
COMMENT ON COLUMN "awd_precheck_runs"."completed_at" IS '检查完成时间';

COMMENT ON TABLE "awd_runtime_resources" IS 'AWD 运行时资源：系统实际创建的 Docker 网络/容器/WireGuard 网卡等资源（用于对账）';
COMMENT ON COLUMN "awd_runtime_resources"."id" IS '主键';
COMMENT ON COLUMN "awd_runtime_resources"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_runtime_resources"."resource_type" IS '资源类型：docker_network / container / wireguard_iface';
COMMENT ON COLUMN "awd_runtime_resources"."resource_id" IS '资源 ID（Docker 网络 ID/容器 ID 等）';
COMMENT ON COLUMN "awd_runtime_resources"."resource_name" IS '资源名称';
COMMENT ON COLUMN "awd_runtime_resources"."observed_state" IS '观察到的资源状态（JSON）';
COMMENT ON COLUMN "awd_runtime_resources"."last_seen_at" IS '最近一次观察到的时间';

COMMENT ON TABLE "awd_orphan_resources" IS 'AWD 孤儿资源：数据库无记录但 Docker/WireGuard 中实际存在的资源（泄漏检测与清理）';
COMMENT ON COLUMN "awd_orphan_resources"."id" IS '主键';
COMMENT ON COLUMN "awd_orphan_resources"."event_id" IS '赛事 ID（可为空，删除时置 NULL）';
COMMENT ON COLUMN "awd_orphan_resources"."resource_type" IS '资源类型';
COMMENT ON COLUMN "awd_orphan_resources"."resource_id" IS '资源 ID';
COMMENT ON COLUMN "awd_orphan_resources"."resource_name" IS '资源名称';
COMMENT ON COLUMN "awd_orphan_resources"."observed_state" IS '观察到的状态（JSON）';
COMMENT ON COLUMN "awd_orphan_resources"."discovered_at" IS '发现时间';
COMMENT ON COLUMN "awd_orphan_resources"."resolved_at" IS '处理完成时间';
COMMENT ON COLUMN "awd_orphan_resources"."resolution" IS '处理结果：pending 待处理 / adopted 已接管 / cleaned 已清理';

COMMENT ON TABLE "awd_internal_token_rotations" IS 'AWD 内部令牌轮换审计：flagserver/judgeserver 令牌与事件密钥的轮换记录';
COMMENT ON COLUMN "awd_internal_token_rotations"."id" IS '主键';
COMMENT ON COLUMN "awd_internal_token_rotations"."event_id" IS '赛事 ID';
COMMENT ON COLUMN "awd_internal_token_rotations"."token_type" IS '令牌类型：flagserver / judgeserver / event_secret';
COMMENT ON COLUMN "awd_internal_token_rotations"."rotated_by" IS '轮换操作人（超级管理员）';
COMMENT ON COLUMN "awd_internal_token_rotations"."rotated_at" IS '轮换时间';


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171915-awd-precheck-runtime.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171916-awd-indexes.sql
-- ================================================================================================
-- ================================================================================================

-- ============================================================
-- AWD Indexes
-- ============================================================

-- awd_events 查询索引：按赛事/状态/阶段过滤
CREATE INDEX IF NOT EXISTS "idx_awd_events_event_id" ON "awd_events" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_events_status" ON "awd_events" ("status");
CREATE INDEX IF NOT EXISTS "idx_awd_events_phase" ON "awd_events" ("phase");

-- awd_team_networks 查询索引：按赛事/队伍查网络分配
CREATE INDEX IF NOT EXISTS "idx_awd_team_networks_event" ON "awd_team_networks" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_team_networks_team" ON "awd_team_networks" ("team_id");

-- awd_gamebox_templates 查询索引：按赛事查模板
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_templates_event" ON "awd_gamebox_templates" ("event_id");

-- awd_gamebox_instances 查询索引：按赛事/队伍/模板/状态/容器查实例
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_event" ON "awd_gamebox_instances" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_team" ON "awd_gamebox_instances" ("team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_template" ON "awd_gamebox_instances" ("template_id");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_status" ON "awd_gamebox_instances" ("status");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_container" ON "awd_gamebox_instances" ("container_id");

-- awd_wireguard_peers 查询索引：按赛事/队伍/用户/状态查对等端
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_event" ON "awd_wireguard_peers" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_team" ON "awd_wireguard_peers" ("team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_user" ON "awd_wireguard_peers" ("user_id");
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_status" ON "awd_wireguard_peers" ("status");

-- awd_rounds 查询索引：按赛事/状态查回合
CREATE INDEX IF NOT EXISTS "idx_awd_rounds_event" ON "awd_rounds" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_rounds_status" ON "awd_rounds" ("status");

-- awd_flag_issues 查询索引：按赛事回合/靶机/哈希查 flag 发放
CREATE INDEX IF NOT EXISTS "idx_awd_flag_issues_event_round" ON "awd_flag_issues" ("event_id", "round_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_issues_instance" ON "awd_flag_issues" ("gamebox_instance_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_issues_hash" ON "awd_flag_issues" ("flag_hash");

-- awd_flag_submissions 查询索引：按赛事回合/攻防双方/提交人查提交
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_event_round" ON "awd_flag_submissions" ("event_id", "round_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_attacker" ON "awd_flag_submissions" ("attacker_team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_victim" ON "awd_flag_submissions" ("victim_team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_user" ON "awd_flag_submissions" ("submitted_by_user_id");

-- awd_score_events 查询索引：按赛事/队伍/类型/幂等键查积分账本
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_event" ON "awd_score_events" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_team" ON "awd_score_events" ("team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_type" ON "awd_score_events" ("event_type");
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_idempotency" ON "awd_score_events" ("idempotency_key");

-- awd_judge_batches 查询索引：按赛事/回合查判题批次
CREATE INDEX IF NOT EXISTS "idx_awd_judge_batches_event" ON "awd_judge_batches" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_batches_round" ON "awd_judge_batches" ("round_id");

-- awd_judge_tasks 查询索引：按批次/赛事回合/状态/截止时间/回调幂等键查判题任务
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_batch" ON "awd_judge_tasks" ("batch_id");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_event_round" ON "awd_judge_tasks" ("event_id", "round_id");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_status" ON "awd_judge_tasks" ("status");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_deadline" ON "awd_judge_tasks" ("deadline_at");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_callback" ON "awd_judge_tasks" ("callback_idempotency_key");

-- awd_reset_records 查询索引：按赛事/靶机查重置记录
CREATE INDEX IF NOT EXISTS "idx_awd_reset_records_event" ON "awd_reset_records" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_reset_records_instance" ON "awd_reset_records" ("gamebox_instance_id");

-- awd_team_bans 查询索引：按赛事/队伍查封禁
CREATE INDEX IF NOT EXISTS "idx_awd_team_bans_event" ON "awd_team_bans" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_team_bans_team" ON "awd_team_bans" ("team_id");

-- awd_precheck_runs 查询索引：按赛事查检查记录
CREATE INDEX IF NOT EXISTS "idx_awd_precheck_runs_event" ON "awd_precheck_runs" ("event_id");

-- awd_runtime_resources 查询索引：按赛事查运行时资源
CREATE INDEX IF NOT EXISTS "idx_awd_runtime_resources_event" ON "awd_runtime_resources" ("event_id");


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171916-awd-indexes.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260806171917-scheduler-retry.sql
-- ================================================================================================
-- ================================================================================================

-- 本文件有意不包含 DDL：调度重试列属于增量迁移，已由 20260806171908 提供，
-- 避免 awd/*.sql 批量执行时重复应用。
-- Moved: scheduler reliability columns are incremental, not AWD schema.
-- Canonical: src/sql/update/01-scheduler-retry.sql
-- Applied by migration m0101 (include_str of that path).
-- This file intentionally has no DDL (avoids double-apply if awd/*.sql is bulk-run).


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260806171917-scheduler-retry.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260807105735-add-challenge-solves-event-id.sql
-- ================================================================================================
-- ================================================================================================

-- ================================================================================
-- Migration: 20260807105735-add-challenge-solves-event-id
-- Created at: 2026-08-07 10:57:35 +0800
--
-- Restore `challenge_solves.event_id`, consumed by:
--   - GET /api/challenge_solves  (filter by event_id)
--   - GET /api/challenge_solves/top15users  (EventId.is_null() -> practice only)
--   - event submission service (record solve against an event)
-- The column was present in the entity/code but never existed in the schema.
-- Safe to re-run (IF NOT EXISTS).
-- ================================================================================

BEGIN;

ALTER TABLE "challenge_solves"
    ADD COLUMN IF NOT EXISTS "event_id" UUID REFERENCES "events" ("id") ON DELETE CASCADE;

COMMENT ON COLUMN "challenge_solves"."event_id" IS
    '所属赛事 ID（NULL=独立/练习解题）';

COMMENT ON TABLE "challenge_solves" IS
    '独立解题记录：练习模式的解题流水（event_id 为空）；赛事解题另有 event_challenge_solves';

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260807105735-add-challenge-solves-event-id.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808093622-awd-paused-phase.sql
-- ================================================================================================
-- ================================================================================================

-- ================================================================================
-- Migration: 20260808093622-awd-paused-phase
-- Created at: 2026-08-08
-- ================================================================================
-- Phase 0 P0-1b：resume 需要恢复暂停前的比赛阶段（原实现硬编码 Attack 是已知缺陷，
-- Phase 4 P4-8 依赖此列）。暂停时经 transition_event 原子写入 paused_phase。
-- ================================================================================

BEGIN;

ALTER TABLE "awd_events"
    ADD COLUMN IF NOT EXISTS "paused_phase" "awd_phase";

COMMENT ON COLUMN "awd_events"."paused_phase" IS '暂停前所处的比赛阶段（resume 时恢复，Phase 0 P0-1b 引入）';

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808093622-awd-paused-phase.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808120928-awd-wg-config-fetched.sql
-- ================================================================================================
-- ================================================================================================

-- ================================================================================
-- Migration: 20260808120928-awd-wg-config-fetched
-- ================================================================================
-- Phase 1 P1-15：WireGuard 私钥一次性返回。
-- player 首次拉取 WG 配置时返回私钥并记录 config_fetched_at；
-- 之后再次请求不再返回私钥（防 token 泄漏后私钥被反复拉取）。
-- ================================================================================

BEGIN;

ALTER TABLE "awd_wireguard_peers"
    ADD COLUMN IF NOT EXISTS "config_fetched_at" TIMESTAMPTZ;

COMMENT ON COLUMN "awd_wireguard_peers"."config_fetched_at" IS 'WG 配置（含私钥）首次拉取时间；NULL=尚未拉取（Phase 1 P1-15）';

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808120928-awd-wg-config-fetched.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808124905-awd-configuration-generation.sql
-- ================================================================================================
-- ================================================================================================

-- ================================================================================
-- Migration: 20260808124905-awd-configuration-generation
-- ================================================================================
-- Phase 2 P2-9：configuration_generation / verified_generation 机制。
-- 所有影响 runtime 的配置写入口调用 touch_configuration → configuration_generation += 1；
-- Precheck 成功：verified_generation = configuration_generation；
-- Start 校验两者相等，不匹配 → StartBlocked（AWD_CONFIG_CHANGED）。
-- ================================================================================

BEGIN;

ALTER TABLE "awd_events"
    ADD COLUMN IF NOT EXISTS "configuration_generation" BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS "verified_generation" BIGINT NULL;

COMMENT ON COLUMN "awd_events"."configuration_generation" IS '配置代数：影响 runtime 的配置每次变更 +1（Phase 2 P2-9）';
COMMENT ON COLUMN "awd_events"."verified_generation" IS '已验证代数：Precheck 成功时记录当时的 configuration_generation（Phase 2 P2-9）';

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808124905-awd-configuration-generation.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808183835-awd-gamebox-domain-a.sql
-- ================================================================================================
-- ================================================================================================

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

BEGIN;

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

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808183835-awd-gamebox-domain-a.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808183914-awd-gamebox-domain-b.sql
-- ================================================================================================
-- ================================================================================================

-- ================================================================================
-- Migration: 20260808183914-awd-gamebox-domain-b
-- ================================================================================
-- GameBox 领域模型重构 — Migration B：数据回填。
--
-- 安全原则（§40-43）：
--   - 每条 legacy awd_gamebox_templates → 自己的 GameBox identity + Revision 1 + EventGameBox
--     （禁止按 name/safe_name 猜测合并，宁可保守产生重复 GameBox）
--   - host_offset 优先从已存在 instance IP 反推（host 字节），
--     同 template 在不同 Team 偏移不一致 → RAISE（MIGRATION CONFLICT，禁止静默选择）
--   - 未部署的 template → 按稳定排序分配可用 offset（2..254，跳过已占用）
--   - 每个旧 gameboxes 全局行 → GameBox + Revision 1（source_toml 保存，spec 最小正规化）
--   - legacy image 无 digest → image_digest = NULL（§8.1），生产前必须 resolve/pin
--
-- 本迁移在空库上零操作，但保持对任何环境的正确性与可验证性。
-- ================================================================================

BEGIN;

-- ──────────────────────────────────────────────────────────────────────────────
-- 1. 回填 legacy awd_gamebox_templates → GameBox + Revision 1 + EventGameBox
-- ──────────────────────────────────────────────────────────────────────────────

-- 临时映射表：legacy template_id → 新 gamebox_id / event_gamebox_id（后续回填 judge/score 用）
CREATE TEMP TABLE IF NOT EXISTS _gb_mapping (
    legacy_template_id UUID PRIMARY KEY,
    gamebox_id UUID NOT NULL,
    event_gamebox_id UUID NOT NULL
) ON COMMIT DROP;

DO $$
DECLARE
    t RECORD;
    gb_id UUID;
    rev_id UUID;
    eg_id UUID;
    safe_base TEXT;
    safe_name_val TEXT;
    offset_candidates INT[];
    first_offset INT;
    used_offsets INT[];
    taken BOOLEAN;
    cand INT;
    spec_json JSONB;
    digest TEXT;
    legacy_count INT;
BEGIN
    -- 逐条处理 legacy template（稳定排序：created_at → id，保证确定性）
    FOR t IN
        SELECT * FROM awd_gamebox_templates
        ORDER BY created_at ASC, id ASC
    LOOP
        -- a. 创建 GameBox identity（不与其他 gamebox 同名；安全名从 name 生成 + 冲突去重）
        safe_base := lower(regexp_replace(t.name, '[^a-zA-Z0-9_-]+', '-', 'g'));
        safe_base := regexp_replace(btrim(safe_base, '-'), '-+', '-', 'g');
        IF safe_base = '' THEN safe_base := 'gamebox'; END IF;
        safe_name_val := safe_base;
        WHILE EXISTS (SELECT 1 FROM gameboxes WHERE safe_name = safe_name_val) LOOP
            safe_name_val := safe_base || '-' || substr(md5(t.id::text), 1, 6);
        END LOOP;

        gb_id := gen_random_uuid();
        INSERT INTO gameboxes (id, name, safe_name, category, description, hidden, created_at, updated_at)
        VALUES (gb_id, t.name, safe_name_val, 'other', '从 legacy AWD template 迁移（GameBox 领域重构 Migration B）', TRUE, now(), now());

        -- b. 创建 Revision 1：spec_json = 正规化配置（image/username/资源/judge），digest = sha256(canonical)
        spec_json := jsonb_build_object(
            'image_ref', t.image_ref,
            'username', t.username,
            'cpu_millis', t.cpu_millis,
            'memory_bytes', t.memory_bytes,
            'pids_limit', t.pids_limit,
            'meta_json', COALESCE(t.meta_json, '{}'::jsonb),
            'healthcheck', t.healthcheck_override_json,
            'judge_script_name', t.judge_script_name,
            'judge_script_content', t.judge_script_content,
            'judge_args', t.judge_args_json
        );
        digest := encode(sha256(spec_json::text::bytea), 'hex');
        rev_id := gen_random_uuid();
        INSERT INTO gamebox_revisions (
            id, gamebox_id, revision_number, source_toml, spec_schema_version, spec_json, spec_digest,
            image_ref, image_digest, username, default_cpu_millis, default_memory_bytes, default_pids_limit,
            healthcheck_json, judge_script_name, judge_script_content, judge_args_json,
            default_judge_timeout_secs, default_judge_retry_interval_secs, created_at
        ) VALUES (
            rev_id, gb_id, 1, '', 1, spec_json, digest,
            t.image_ref, NULL, t.username, t.cpu_millis, t.memory_bytes, t.pids_limit,
            t.healthcheck_override_json, t.judge_script_name, t.judge_script_content, t.judge_args_json,
            t.judge_timeout_secs, t.judge_retry_interval_secs, now()
        );

        -- c. host_offset：优先从已存在 instance IP 反推（host 字节）
        offset_candidates := ARRAY(
            SELECT DISTINCT (host(split_part(inst.gamebox_ip, '/', 1)::inet))::int
            FROM awd_gamebox_instances inst
            WHERE inst.template_id = t.id
        );
        IF cardinality(offset_candidates) > 1 THEN
            RAISE EXCEPTION 'MIGRATION CONFLICT: legacy template % has inconsistent host offsets across teams: %（禁止静默选择，需人工处理）', t.id, offset_candidates;
        ELSIF cardinality(offset_candidates) = 1 THEN
            first_offset := offset_candidates[1];
            IF first_offset < 2 OR first_offset > 254 THEN
                RAISE EXCEPTION 'MIGRATION CONFLICT: legacy template % host offset % out of range 2..254', t.id, first_offset;
            END IF;
        ELSE
            -- 未部署 → 分配该事件下一个可用 offset（2..254，跳过已占用）
            used_offsets := ARRAY(
                SELECT eg.host_offset FROM awd_event_gameboxes eg
                WHERE eg.event_id = t.event_id
            );
            first_offset := NULL;
            FOR cand IN 2..254 LOOP
                taken := cand = ANY(used_offsets);
                IF NOT taken THEN
                    first_offset := cand;
                    EXIT;
                END IF;
            END LOOP;
            IF first_offset IS NULL THEN
                RAISE EXCEPTION 'MIGRATION CONFLICT: no free host_offset (2..254) for legacy template %', t.id;
            END IF;
        END IF;

        -- d. 创建 EventGameBox（计分配置从 legacy template 迁移）
        eg_id := gen_random_uuid();
        INSERT INTO awd_event_gameboxes (
            id, event_id, gamebox_id, gamebox_revision_id, host_offset,
            enabled, hidden, cpu_millis, memory_bytes, pids_limit, healthcheck_override_json,
            judge_timeout_secs, judge_retry_interval_secs,
            break_points, loss_points, fix_points, down_points, first_bonus,
            created_at, updated_at
        ) VALUES (
            eg_id, t.event_id, gb_id, rev_id, first_offset,
            TRUE, FALSE, t.cpu_millis, t.memory_bytes, t.pids_limit, t.healthcheck_override_json,
            t.judge_timeout_secs, t.judge_retry_interval_secs,
            t.break_points, t.loss_points, t.fix_points, t.down_points, t.first_bonus,
            now(), now()
        );

        -- e. 关联 legacy instance → 新 EventGameBox
        UPDATE awd_gamebox_instances
        SET event_gamebox_id = eg_id
        WHERE template_id = t.id;

        -- f. 记录映射（judge/score 回填用）
        INSERT INTO _gb_mapping (legacy_template_id, gamebox_id, event_gamebox_id)
        VALUES (t.id, gb_id, eg_id);
    END LOOP;

    SELECT count(*) INTO legacy_count FROM awd_gamebox_templates;
    RAISE NOTICE 'GameBox backfill: % legacy templates migrated (empty in dev)', legacy_count;
END $$;

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. 回填 legacy gameboxes（全局库）→ Revision 1
--    （§41：source_toml 保存原始 TOML；旧计分字段不进入 Revision，不凭空制造赛事计分配置）
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
DECLARE
    g RECORD;
    spec_json JSONB;
    digest TEXT;
BEGIN
    FOR g IN SELECT * FROM gameboxes ORDER BY created_at ASC, id ASC LOOP
        -- 已有 revision 的行跳过（幂等）
        IF EXISTS (SELECT 1 FROM gamebox_revisions WHERE gamebox_id = g.id) THEN
            CONTINUE;
        END IF;
        spec_json := jsonb_build_object(
            'legacy', TRUE,
            'source_toml', COALESCE(g.toml_str, ''),
            'username', g.username
        );
        digest := encode(sha256(spec_json::text::bytea), 'hex');
        INSERT INTO gamebox_revisions (
            id, gamebox_id, revision_number, source_toml, spec_schema_version, spec_json, spec_digest,
            image_ref, image_digest, username, default_cpu_millis, default_memory_bytes, default_pids_limit,
            healthcheck_json, judge_script_name, judge_script_content, judge_args_json,
            default_judge_timeout_secs, default_judge_retry_interval_secs, created_at
        ) VALUES (
            gen_random_uuid(), g.id, 1, COALESCE(g.toml_str, ''), 1, spec_json, digest,
            '', NULL, COALESCE(g.username, 'ctf'), 1000, 536870912, 100,
            NULL, NULL, NULL, NULL, NULL, NULL, now()
        );
    END LOOP;
END $$;

-- ──────────────────────────────────────────────────────────────────────────────
-- 3. 回填 awd_judge_tasks.event_gamebox_id（经 instance 关联）
-- ──────────────────────────────────────────────────────────────────────────────
UPDATE awd_judge_tasks jt
SET event_gamebox_id = inst.event_gamebox_id
FROM awd_gamebox_instances inst
WHERE jt.gamebox_instance_id = inst.id
  AND jt.event_gamebox_id IS NULL;

-- ──────────────────────────────────────────────────────────────────────────────
-- 4. 回填 awd_score_events.event_gamebox_id
--    instance 维度：经 awd_gamebox_instances 关联；
--    template 维度（如 first-bonus，无 instance）：经 legacy 映射表关联。
-- ──────────────────────────────────────────────────────────────────────────────
UPDATE awd_score_events se
SET event_gamebox_id = inst.event_gamebox_id
FROM awd_gamebox_instances inst
WHERE se.gamebox_instance_id = inst.id
  AND se.event_gamebox_id IS NULL;

UPDATE awd_score_events se
SET event_gamebox_id = m.event_gamebox_id
FROM _gb_mapping m
WHERE se.gamebox_template_id = m.legacy_template_id
  AND se.event_gamebox_id IS NULL;

-- ──────────────────────────────────────────────────────────────────────────────
-- 5. 可验证性断言（§61）——任何一条不满足即回滚整个迁移
-- ──────────────────────────────────────────────────────────────────────────────
DO $$
DECLARE
    template_cnt INT; eg_cnt INT; orphan_inst INT; orphan_link INT; dup_offset INT; dup_inst INT;
BEGIN
    SELECT count(*) INTO template_cnt FROM awd_gamebox_templates;
    SELECT count(*) INTO eg_cnt FROM awd_event_gameboxes;
    IF template_cnt <> eg_cnt THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: legacy templates % <> event_gameboxes %', template_cnt, eg_cnt;
    END IF;

    -- 每个 legacy instance 都有 event_gamebox_id
    SELECT count(*) INTO orphan_inst FROM awd_gamebox_instances WHERE event_gamebox_id IS NULL;
    IF orphan_inst > 0 THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: % instances lack event_gamebox_id', orphan_inst;
    END IF;

    -- 每个 EventGameBox 都有 GameBoxRevision（FK 已保证）且无孤儿 revision link（复合 FK 已保证）

    -- 同 Event host_offset 无重复（UNIQUE 已保证，双保险）
    SELECT count(*) INTO dup_offset FROM (
        SELECT event_id, host_offset FROM awd_event_gameboxes GROUP BY 1,2 HAVING count(*) > 1
    ) d;
    IF dup_offset > 0 THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: duplicate host_offset';
    END IF;

    -- 同 Event + Team + EventGameBox 只有一个 logical Instance
    SELECT count(*) INTO dup_inst FROM (
        SELECT event_id, team_id, event_gamebox_id FROM awd_gamebox_instances
        GROUP BY 1,2,3 HAVING count(*) > 1
    ) d;
    IF dup_inst > 0 THEN
        RAISE EXCEPTION 'MIGRATION VERIFY FAIL: duplicate logical instances';
    END IF;
END $$;

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808183914-awd-gamebox-domain-b.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808183946-awd-gamebox-domain-c.sql
-- ================================================================================================
-- ================================================================================================

-- ================================================================================
-- Migration: 20260808183946-awd-gamebox-domain-c
-- ================================================================================
-- GameBox 领域模型重构 — Migration C：回填后的约束收紧。
--
-- 仅在 Migration B 回填完成后执行（依赖所有旧行已获得 event_gamebox_id）。
-- ================================================================================

BEGIN;

-- ──────────────────────────────────────────────────────────────────────────────
-- 1. awd_gamebox_instances：event_gamebox_id 强制 + FK + 新 UNIQUE
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_gamebox_instances"
    ALTER COLUMN "event_gamebox_id" SET NOT NULL;

ALTER TABLE "awd_gamebox_instances"
    ADD CONSTRAINT "awd_gamebox_instances_event_gamebox_fk"
    FOREIGN KEY ("event_gamebox_id")
    REFERENCES "awd_event_gameboxes" ("id")
    ON DELETE RESTRICT;

-- 旧 UNIQUE(event_id, template_id, team_id) 被新 UNIQUE(event_id, event_gamebox_id, team_id) 取代
ALTER TABLE "awd_gamebox_instances"
    DROP CONSTRAINT IF EXISTS "awd_gamebox_instances_event_id_template_id_team_id_key";

ALTER TABLE "awd_gamebox_instances"
    ADD CONSTRAINT "awd_gamebox_instances_event_gamebox_team_key"
    UNIQUE ("event_id", "event_gamebox_id", "team_id");

-- ──────────────────────────────────────────────────────────────────────────────
-- 2. awd_judge_tasks / awd_score_events：FK（SET NULL 保历史，§57）
-- ──────────────────────────────────────────────────────────────────────────────
ALTER TABLE "awd_judge_tasks"
    ADD CONSTRAINT "awd_judge_tasks_event_gamebox_fk"
    FOREIGN KEY ("event_gamebox_id")
    REFERENCES "awd_event_gameboxes" ("id")
    ON DELETE SET NULL;

ALTER TABLE "awd_score_events"
    ADD CONSTRAINT "awd_score_events_event_gamebox_fk"
    FOREIGN KEY ("event_gamebox_id")
    REFERENCES "awd_event_gameboxes" ("id")
    ON DELETE SET NULL;

COMMENT ON COLUMN "awd_judge_tasks"."event_gamebox_id" IS '判题目标 EventGameBox（SET NULL：EventGameBox 删除后保留历史行）';
COMMENT ON COLUMN "awd_score_events"."event_gamebox_id" IS '计分作用域 EventGameBox（SET NULL：EventGameBox 删除后保留历史行）';

COMMIT;


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808183946-awd-gamebox-domain-c.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
-- BEGIN MIGRATION: 20260808183947-awd-gamebox-domain-d.sql
-- ================================================================================================
-- ================================================================================================

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


-- ================================================================================================
-- ================================================================================================
-- END MIGRATION: 20260808183947-awd-gamebox-domain-d.sql
-- ================================================================================================
-- ================================================================================================


-- ================================================================================================
-- ================================================================================================
--                                  End of FloatCTF Migrations
-- ================================================================================================
-- ================================================================================================
