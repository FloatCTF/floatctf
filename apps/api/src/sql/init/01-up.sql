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
