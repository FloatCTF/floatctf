-- ================================================================================
-- Migration: 20260812000820-gamebox-awdp-capability
-- 目标：GameBox 库承载 [awdp] capability 的类型化列（与 judge_script_* 风格一致）。
--
-- 语义：
--   - 无 [awdp] 的普通 GameBox：5 列全部 NULL（presence = capability）。
--   - 有 [awdp] 的 GameBox：import 成功时 5 列全部 NOT NULL（含 source.zip 产物）。
--   - DB CHECK 强制全有或全无，避免半成品状态。
--
-- source.zip 产物在 GameBox import/build 成功后一次性生成（GameBox 维度，
-- 与 Event 无关），object key 使用 gamebox/digest scope：
--   gameboxes/{gamebox_id}/awdp/{package_digest}/source.zip
-- ================================================================================

ALTER TABLE public.gameboxes
    ADD COLUMN IF NOT EXISTS awdp_source_code_dir TEXT NULL,
    ADD COLUMN IF NOT EXISTS awdp_exploit_script_name TEXT NULL,
    ADD COLUMN IF NOT EXISTS awdp_exploit_script_content TEXT NULL,
    ADD COLUMN IF NOT EXISTS awdp_source_artifact_key TEXT NULL,
    ADD COLUMN IF NOT EXISTS awdp_source_artifact_digest TEXT NULL;

COMMENT ON COLUMN public.gameboxes.awdp_source_code_dir IS
    '[awdp].source_code_dir（容器内绝对路径，source.zip 打包目录）';
COMMENT ON COLUMN public.gameboxes.awdp_exploit_script_name IS
    '[awdp].exploit_script 文件名（包内相对路径 basename）';
COMMENT ON COLUMN public.gameboxes.awdp_exploit_script_content IS
    '[awdp].exploit_script 内容（平台侧执行，绝不提供给选手）';
COMMENT ON COLUMN public.gameboxes.awdp_source_artifact_key IS
    'private RustFS 对象键 gameboxes/{id}/awdp/{package_digest}/source.zip';
COMMENT ON COLUMN public.gameboxes.awdp_source_artifact_digest IS
    'source.zip 字节的 SHA-256 十六进制摘要';

-- 全有或全无：普通 GameBox 全 NULL；[awdp] 完整 capability 全 NOT NULL。
ALTER TABLE public.gameboxes
    DROP CONSTRAINT IF EXISTS gameboxes_awdp_capability_complete_check;

ALTER TABLE public.gameboxes
    ADD CONSTRAINT gameboxes_awdp_capability_complete_check CHECK (
        (awdp_source_code_dir IS NULL
         AND awdp_exploit_script_name IS NULL
         AND awdp_exploit_script_content IS NULL
         AND awdp_source_artifact_key IS NULL
         AND awdp_source_artifact_digest IS NULL)
        OR
        (awdp_source_code_dir IS NOT NULL
         AND awdp_exploit_script_name IS NOT NULL
         AND awdp_exploit_script_content IS NOT NULL
         AND awdp_source_artifact_key IS NOT NULL
         AND awdp_source_artifact_digest IS NOT NULL)
    );
