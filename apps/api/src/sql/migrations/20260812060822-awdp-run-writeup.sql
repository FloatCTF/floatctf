-- ================================================================================
-- Migration: 20260812060822-awdp-run-writeup
-- 目标：AWDP Run 个人 Writeup（按 run 一份，练习/竞赛共用引擎）。
--   run_id PK + FK → awdp_runs(id) ON DELETE CASCADE（run 删除即随删）
--   user_id FK → users(id)（写入者；练习 run 即属主）
--   content：MD 文本
-- 属主校验在应用层（练习 run 仅 owner 可读写；竞赛 run 本期不做 writeup 面板）。
-- ================================================================================

CREATE TABLE IF NOT EXISTS public.awdp_run_writeups (
    run_id     uuid PRIMARY KEY REFERENCES public.awdp_runs(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    content    text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.awdp_run_writeups IS 'AWDP Run 个人 Writeup（练习 run 属主可读写；run 删除随删）';
COMMENT ON COLUMN public.awdp_run_writeups.run_id IS 'AWDP Run ID（一 run 一份）';
COMMENT ON COLUMN public.awdp_run_writeups.user_id IS '写入者用户（练习 run 即属主）';
COMMENT ON COLUMN public.awdp_run_writeups.content IS 'Writeup 内容（Markdown）';
