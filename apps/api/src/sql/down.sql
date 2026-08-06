DROP EXTENSION IF EXISTS "uuid-ossp" CASCADE;

DROP FUNCTION IF EXISTS update_updated_at_column () CASCADE;

-- 删除表
DROP TABLE IF EXISTS "settings",
"users",
"super_admin",
"challenges",
"instances",
"events",
"event_challenges",
"event_users",
"event_teams",
"event_instances",
"event_challenge_solves",
"event_team_members",
"challenge_solves",
"challenge_writeup",
"event_announcements",
"event_writeup",
"challenge_sets",
"challenge_set_items" CASCADE;