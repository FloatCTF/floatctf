-- ================================================================================
-- Migration: 20260827113717-awd-wave5-reset-ban-network
-- Wave 5: Reset Ban Network Enforcement
-- ================================================================================
-- 
-- 1. Remove reset protection fields (spec §19.3: no protection window)
-- 2. Remove timed auto-unban scheduled tasks (spec §23: no automatic Ban expiration)
-- 
-- DO NOT include BEGIN/COMMIT (migrate.sh handles transaction wrapping).

-- 1. Remove reset protection from awd_events
ALTER TABLE awd_events
    DROP COLUMN IF EXISTS reset_protection_secs;

-- 2. Remove reset protection from event_gamebox_instances
ALTER TABLE event_gamebox_instances
    DROP COLUMN IF EXISTS reset_protection_until;

-- 3. Remove timed auto-unban scheduled tasks
--    These are obsolete: Ban has no duration, no automatic expiration (spec §23).
DELETE FROM scheduled_tasks
WHERE task_key = 'awd.team.unban'
   OR task_name LIKE 'AWD auto-unban%';