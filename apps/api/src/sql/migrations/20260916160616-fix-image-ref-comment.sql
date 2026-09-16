-- ================================================================================
-- Migration: 20260916160616-fix-image-ref-comment
-- ================================================================================
--
-- 修正 image_ref 列注释为 floatctf-content canonical 格式。
--
-- canonical image ref（source of truth: floatctf-content/scripts/content.py::image_ref）：
--   Challenge: <registry-prefix>/<safe_name>:challenge-v<version>
--   GameBox:   <registry-prefix>/<safe_name>:gamebox-v<version>
--
-- 历史迁移（20260810140220 / 20260810145445 / 20260810235621）中的旧注释保持原样不动
-- （铁律 2：已有迁移不可修改）；其中的 challenge_revisions / gamebox_revisions 已由
-- 单版本模型迁移移除，无需也不能再更新其注释。
-- 本迁移只更新当前仍然存在、且注释仍是旧格式的两列。
--
-- COMMENT ON COLUMN 天然幂等：重复执行只会写入相同的注释。

COMMENT ON COLUMN public.challenges.image_ref IS
    '人类可读镜像 tag（canonical）：<registry-prefix>/<safe_name>:challenge-v<version>';

COMMENT ON COLUMN public.gameboxes.image_ref IS
    '人类可读镜像 tag（canonical）：<registry-prefix>/<safe_name>:gamebox-v<version>';
