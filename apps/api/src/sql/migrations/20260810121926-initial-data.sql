-- ================================================================================
-- Migration: 20260810121926-initial-data
-- ================================================================================

-- 程序运行必需的 bootstrap data（仅 Category B；不含任何 dev/demo/历史数据）。
--
-- 数据来源（旧开发历史中的固定 metadata 初始化，最终状态即 baseline 所需）：
--   20260806171907-init.sql                 → 内置超级管理员 sysadmin
--   20260808224040-awd-network-control-plane-a.sql → AWD 网络池默认配置（单例）
--
-- 两条均为固定身份数据（零 UUID / id=1），在 fresh DB 上只执行一次，
-- 不需要 IF NOT EXISTS 或 ON CONFLICT 兜底（保留原语义，防未来手工重放）。

-- ================================================================================
-- Bootstrap: 内置超级管理员（sysadmin，argon2id 哈希）
-- ================================================================================

INSERT INTO
    "super_admin" (
        "id",
        "username",
        "password",
        "email",
        "created_at",
        "updated_at"
    )
VALUES (
        '00000000-0000-0000-0000-000000000000',
        'sysadmin',
        '$argon2id$v=19$m=19456,t=2,p=1$3THt36/y60+8SreEtA+T5A$xp4mvnbi0niUfEux7u24ZdTnv4t5QnH8ZhA/uF+GDe8',
        'sysadmin@system.com',
        '2025-09-29 13:04:49.689893',
        '2025-09-29 13:04:49.689893'
    );


-- ================================================================================
-- Bootstrap: AWD 网络池默认配置（单例，CHECK id = 1）
-- ================================================================================

INSERT INTO "awd_network_settings" (
    id, gamebox_pool, gamebox_event_prefix, gamebox_team_prefix,
    wireguard_pool, wireguard_event_prefix, wireguard_team_prefix,
    wireguard_port_min, wireguard_port_max, wireguard_public_endpoint
) VALUES (
    1, '10.0.0.0/8', 16, 24,
    '172.16.0.0/12', 16, 24,
    30000, 40000, NULL
)
ON CONFLICT (id) DO NOTHING;
