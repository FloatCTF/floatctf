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
