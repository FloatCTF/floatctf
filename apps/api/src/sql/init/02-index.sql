-- users 表索引
CREATE UNIQUE INDEX IF NOT EXISTS "idx_users_username" ON "users" ("username");

CREATE UNIQUE INDEX IF NOT EXISTS "idx_users_email" ON "users" ("email");

-- instances 表索引
CREATE INDEX IF NOT EXISTS "idx_instances_status" ON "instances" ("status");

CREATE INDEX IF NOT EXISTS "idx_instances_challenge_id" ON "instances" ("challenge_id");

CREATE INDEX IF NOT EXISTS "idx_instances_user_id" ON "instances" ("user_id");

-- events 表索引
CREATE INDEX IF NOT EXISTS "idx_events_type" ON "events" ("type");

CREATE INDEX IF NOT EXISTS "idx_events_start_time" ON "events" ("start_time");

CREATE INDEX IF NOT EXISTS "idx_events_end_time" ON "events" ("end_time");

-- event_users 表索引
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

CREATE INDEX "idx_event_user_challenge" ON "event_instances" (
    "event_id",
    "user_id"
);

CREATE INDEX "idx_event_team_challenge" ON "event_instances" (
    "event_id",
    "team_id"
);

-- 2. 调度器专用的极速轮询索引 (极其重要)
CREATE INDEX idx_scheduled_tasks_poll
ON "scheduled_tasks" ("status", "execute_at")
WHERE "status" = 'pending';

-- 3. 业务组关联索引
CREATE INDEX idx_scheduled_tasks_group ON "scheduled_tasks" ("group_id");

-- 3. 索引是关键，否则比赛日志一多，后台查不动
CREATE INDEX idx_event_logs_event_id ON "event_logs" ("event_id");
CREATE INDEX idx_event_logs_action ON "event_logs" ("action");


-- 索引：后台管理页面通常按时间倒序查，或按类别/用户查
CREATE INDEX idx_sys_logs_created_at ON "logs" ("created_at" DESC);
CREATE INDEX idx_sys_logs_category_action ON "logs" ("category", "action");
CREATE INDEX idx_sys_logs_user_op ON "logs" ("user_id", "superadmin_id");
-- JSONB 索引：支持搜索 details 里的具体内容
CREATE INDEX idx_sys_logs_details ON "logs" USING GIN ("details");

CREATE INDEX idx_event_logs_ip_address ON "event_logs" ("ip_address");


CREATE INDEX idx_discussions_author ON "discussions"("author_id");
CREATE INDEX idx_discussions_created ON "discussions"("created_at" DESC);
CREATE INDEX idx_discussion_comments_discussion ON "discussion_comments"("discussion_id");
CREATE INDEX idx_discussion_comments_author ON "discussion_comments"("author_id");
CREATE INDEX idx_discussion_likes_discussion ON "discussion_likes"("discussion_id");
