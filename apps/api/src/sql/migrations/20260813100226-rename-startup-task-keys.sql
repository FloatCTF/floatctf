-- 平台启动维护任务 task_key 重命名为小写点分形式（与引擎任务风格一致）。
--
-- 主键不变（固定系统 id），仅更新 task_key 字符串：
--   CHECK_PRACTICE_EVENT -> system.practice.check   （检查练习 event，id ...000）
--   CLEAN_INSTANCES      -> system.practice.clean   （实例清理，id ...001）
--   CLEAN_RUSTFS         -> platform.rustfs.clean   （RUSTFS 文件清理，id ...002）
--
-- 幂等：只命中固定主键行；重复执行无副作用。task_key 无唯一约束，
-- 但固定主键每键至多一行，rename 后仍与 TaskKey::as_str 完全一致。

UPDATE scheduled_tasks
   SET task_key = 'system.practice.check', updated_at = NOW()
 WHERE id = '00000000-0000-0000-0000-000000000000'
   AND task_key <> 'system.practice.check';

UPDATE scheduled_tasks
   SET task_key = 'system.practice.clean', updated_at = NOW()
 WHERE id = '00000000-0000-0000-0000-000000000001'
   AND task_key <> 'system.practice.clean';

UPDATE scheduled_tasks
   SET task_key = 'platform.rustfs.clean', updated_at = NOW()
 WHERE id = '00000000-0000-0000-0000-000000000002'
   AND task_key <> 'platform.rustfs.clean';
