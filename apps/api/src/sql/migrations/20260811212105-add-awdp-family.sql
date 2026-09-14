-- ================================================================================
-- Migration: 20260811212105-add-awdp-family
-- 目标：新增 EventFamily 枚举值 awdp（AWD Plus）。
-- 说明：本迁移仅添加枚举值；模式组合 CHECK 更新见随后的
--       20260811212106-awdp-mode-combination.sql（须在不同事务中，
--       因为 ALTER TYPE ADD VALUE 的新值不能在同一事务内用于约束）。
-- ================================================================================

-- event_family 枚举追加 'awdp'
-- PostgreSQL 仅允许在枚举末尾追加值。
DO $$ BEGIN
    ALTER TYPE public.event_family ADD VALUE IF NOT EXISTS 'awdp';
EXCEPTION
    WHEN duplicate_object THEN
        NULL; -- 已存在时忽略
END $$;
