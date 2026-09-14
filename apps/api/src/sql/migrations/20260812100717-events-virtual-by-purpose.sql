-- ================================================================================
-- Migration: 20260812100717-events-virtual-by-purpose
-- 练习模式（purpose='practice'）的事件即虚拟赛事（is_virtual=true）。
-- 修正存量数据 + 新增 CHECK 约束防止 is_virtual 与 purpose 漂移。
-- ================================================================================

-- 1. 存量修正：练习事件全部标记为虚拟（幂等）。
UPDATE events
SET is_virtual = TRUE
WHERE purpose = 'practice' AND is_virtual = FALSE;

-- 2. 防漂移约束：is_virtual 必须与「是否练习模式」一致。
--    练习（虚拟）赛事不出现在玩家赛事列表 / admin NormalEvents，
--    仅用于 AWDP 训练场等系统托管场景。
ALTER TABLE events
  ADD CONSTRAINT events_virtual_by_purpose_check
  CHECK (is_virtual = (purpose = 'practice'));
