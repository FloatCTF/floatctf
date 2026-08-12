-- ================================================================================
-- Migration: 20260812081215-cleanup-instance-indexes
-- ================================================================================
-- 实例归一化收尾：清理重命名表后残留的旧名索引/约束 + 去重。
--
-- 背景：迁移 20260812070138 把 challenge_instances / awd_gamebox_instances
-- 重命名为 event_challenge_instance / event_gamebox_instances，但表上既有
-- 索引、约束的名字仍沿用旧表名；另有两对列完全相同的重复索引（更早 schema
-- 遗留，二者等价）。本迁移统一改为与当前表名一致，并删除重复索引。
-- 仅涉及索引/约束命名，不改表结构、不改数据，幂等由 IF EXISTS 兜底。

-- ── event_challenge_instance：约束改名 ────────────────────────────────
ALTER TABLE public.event_challenge_instance RENAME CONSTRAINT challenge_instances_pkey TO event_challenge_instance_pkey;
ALTER TABLE public.event_challenge_instance RENAME CONSTRAINT challenge_instances_event_id_fkey TO event_challenge_instance_event_id_fkey;
ALTER TABLE public.event_challenge_instance RENAME CONSTRAINT challenge_instances_event_team_fkey TO event_challenge_instance_event_team_fkey;
ALTER TABLE public.event_challenge_instance RENAME CONSTRAINT challenge_instances_instance_fk TO event_challenge_instance_instance_fk;
ALTER TABLE public.event_challenge_instance RENAME CONSTRAINT instances_challenge_id_fkey TO event_challenge_instance_challenge_id_fkey;
ALTER TABLE public.event_challenge_instance RENAME CONSTRAINT instances_user_id_fkey TO event_challenge_instance_user_id_fkey;

-- ── event_challenge_instance：重复索引去重（与 idx_challenge_instances_* 同列） ──
DROP INDEX IF EXISTS public.idx_instances_challenge_id;
DROP INDEX IF EXISTS public.idx_instances_user_id;

-- ── event_challenge_instance：索引改名 ─────────────────────────────────
ALTER INDEX public.idx_challenge_instances_challenge_id RENAME TO idx_event_challenge_instance_challenge_id;
ALTER INDEX public.idx_challenge_instances_event_id RENAME TO idx_event_challenge_instance_event_id;
ALTER INDEX public.idx_challenge_instances_event_team RENAME TO idx_event_challenge_instance_event_team;
ALTER INDEX public.idx_challenge_instances_event_user RENAME TO idx_event_challenge_instance_event_user;
ALTER INDEX public.idx_challenge_instances_user_id RENAME TO idx_event_challenge_instance_user_id;

-- ── event_gamebox_instances：约束改名 ─────────────────────────────────
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_pkey TO event_gamebox_instances_pkey;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_event_gamebox_fk TO event_gamebox_instances_event_gamebox_fk;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_event_id_awd_fkey TO event_gamebox_instances_event_id_fkey;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_instance_fk TO event_gamebox_instances_instance_fk;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_team_id_fkey TO event_gamebox_instances_team_id_fkey;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_event_gamebox_team_key TO event_gamebox_instances_event_gamebox_team_key;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_event_id_gamebox_ip_key TO event_gamebox_instances_event_id_gamebox_ip_key;
ALTER TABLE public.event_gamebox_instances RENAME CONSTRAINT awd_gamebox_instances_instance_uidx TO event_gamebox_instances_instance_uidx;

-- ── event_gamebox_instances：索引改名 ─────────────────────────────────
ALTER INDEX public.idx_awd_gamebox_instances_event RENAME TO idx_event_gamebox_instances_event;
ALTER INDEX public.idx_awd_gamebox_instances_status RENAME TO idx_event_gamebox_instances_status;
ALTER INDEX public.idx_awd_gamebox_instances_team RENAME TO idx_event_gamebox_instances_team;
