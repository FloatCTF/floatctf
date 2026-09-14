-- 删除练习 Judge 管理端遗留表（plan §61 收尾 + 用户要求：AWDPlusPractice admin 不再有 judge 配置）。
--
-- 背景：
--   - awdp_practice_judge_settings：旧「管理端配置页」（enabled / judge_server_url /
--     interval_secs / flag_path / container_status）的存储。Pull + Lease 模型落地后
--     JudgeServer 已无配置项（worker 配置全部来自 env），容器部署改为「默认启用」：
--     练习 run Launch / 实例启动时自动部署，管理端配置页与 deploy/stop/results 接口
--     已整体移除（前端页面与 API 路由同删）。
--   - awdp_judge_results：旧 push 时代（/batch sweep → callback 落库）的历史结果表；
--     Phase B 起已无新写入，管理端 results Tab 随本迁移删除后无任何读取方。
--
-- 两表均为叶子表（无其它表外键引用，已核对 information_schema），直接删除安全。
-- 对应实体 awdp_judge_results / awdp_practice_judge_settings 由 db:gen 重新生成后移除。

DROP TABLE IF EXISTS public.awdp_judge_results;
DROP TABLE IF EXISTS public.awdp_practice_judge_settings;
