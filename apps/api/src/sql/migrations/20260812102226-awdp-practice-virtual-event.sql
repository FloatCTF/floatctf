-- ================================================================================
-- Migration: 20260812102226-awdp-practice-virtual-event
-- AWDPlusPractice：练习模块统一虚拟赛事（此前每 user×gamebox 一个虚拟 event）。
-- 1) 创建 AWDPlusPractice 系统虚拟赛事（system_key='awdp-practice'，固定 id，幂等）
-- 2) awdp_runs active 唯一索引重构为 (event_id, gamebox_id, owner_user_id)：
--    competition 仍是每 event 一个 active run（gamebox_id/owner_user_id 全 NULL）；
--    practice 变为同一 event 下每 (gamebox_id, owner_user_id) 一个 active run
-- 3) AWDP 引擎调度任务（awdp.tick / awdp.eval.worker）标注 Practice
-- ================================================================================

-- 1. AWDPlusPractice 虚拟赛事（练习模块单挂载点）。
--    固定 id = 00000000-0000-0000-0000-000000000002（0001 为 JeopardyPractice）。
INSERT INTO events (id, family, purpose, participant_mode, system_key, title, description,
                    hidden, allow_join, start_time, end_time, rules, flag_prefix, is_virtual,
                    created_at, updated_at)
SELECT '00000000-0000-0000-0000-000000000002',
       'awdp', 'practice', 'individual', 'awdp-practice', 'AWDPlusPractice',
       'AWDP 练习（虚拟赛事）：练习模块 gamebox 统一挂载点',
       TRUE, FALSE, now(), NULL, '', NULL, TRUE, now(), now()
WHERE NOT EXISTS (SELECT 1 FROM events WHERE system_key = 'awdp-practice');

-- 2. awdp_runs active 唯一索引重构（同名替换，语义扩展）。
--    已有数据无冲突：competition 每 event 一个 run；practice 每 run 挂各自虚拟 event。
DROP INDEX IF EXISTS awdp_runs_event_active_uidx;
CREATE UNIQUE INDEX awdp_runs_event_active_uidx
    ON public.awdp_runs (event_id, gamebox_id, owner_user_id)
    WHERE phase IN ('pending', 'break', 'fix');

-- 3. 调度任务标注 Practice（幂等文本更新；新库由 seed 直接写新名）。
UPDATE scheduled_tasks
   SET task_name = 'AWDP Practice 阶段推进/回合物化',
       description = 'recurring awdp.tick（AWDPlusPractice 引擎）'
 WHERE task_key = 'awdp.tick'
   AND task_name <> 'AWDP Practice 阶段推进/回合物化';
UPDATE scheduled_tasks
   SET task_name = 'AWDP Practice 评估 worker',
       description = 'recurring awdp.eval.worker（AWDPlusPractice 引擎）'
 WHERE task_key = 'awdp.eval.worker'
   AND task_name <> 'AWDP Practice 评估 worker';
