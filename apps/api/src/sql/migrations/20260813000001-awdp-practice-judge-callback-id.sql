-- ================================================================================
-- Migration: 20260813000001-awdp-practice-judge-callback-id
-- awdp_judge_results.callback_id：JudgeServer 回调幂等去重键。
-- 平台生成 callback_id（awdp-practice-judge:{run}:{instance}:{kind}），
-- JudgeServer 重试投递同一 callback_id，平台按唯一索引幂等跳过重复记录。
-- ================================================================================

ALTER TABLE public.awdp_judge_results
    ADD COLUMN IF NOT EXISTS callback_id TEXT NULL;

COMMENT ON COLUMN public.awdp_judge_results.callback_id IS
    '平台生成的回调幂等键（awdp-practice-judge:{run}:{instance}:{kind}），重复回调按此去重';

-- 已有行回填（可推导：kind + instance 唯一语义下 callback_id 稳定）。
UPDATE public.awdp_judge_results r
   SET callback_id = 'awdp-practice-judge:' || run_id || ':' || instance_id || ':' || check_kind
 WHERE callback_id IS NULL;

-- 唯一索引：重复回调（含并发）只会落一条。
CREATE UNIQUE INDEX IF NOT EXISTS awdp_judge_results_callback_id_uidx
    ON public.awdp_judge_results (callback_id);
