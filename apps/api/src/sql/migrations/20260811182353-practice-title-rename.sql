-- ================================================================================
-- Migration: 20260811182353-practice-title-rename
-- 系统练习赛事显示名：Practice → JeopardyPractice
-- ================================================================================

UPDATE public.events
   SET title = 'JeopardyPractice'
 WHERE system_key = 'practice:jeopardy'
   AND title = 'Practice';
