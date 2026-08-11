-- ================================================================================
-- Migration: 20260811165849-seed-testuser
-- ================================================================================

-- 补种开发/演示用测试账号 testuser（username / nickname / email 均为唯一约束，
-- 使用 ON CONFLICT (username) DO NOTHING 保证幂等，fresh DB 与既有库重放均安全）。
--
-- 密码哈希为 argon2id（与 init-data 内置 sysadmin 同一算法族），
-- 密码原文为注册时设定的值，不落明文。

INSERT INTO
    "users" (
        "id",
        "username",
        "nickname",
        "password",
        "email",
        "created_at",
        "updated_at",
        "avatar"
    )
VALUES (
        '49631e05-59d3-4f33-b310-082b697de73d',
        '1000000',
        'testuser',
        '$argon2id$v=19$m=19456,t=2,p=1$hbVn+RcwCrcB7ryi42g+2Q$5AyW8mtVR9Q8GcrXToLJym0NBYVWQ2rC3dCV8G2+LQk',
        'user@test.com',
        '2026-08-11 08:57:30.52113+00',
        '2026-08-11 08:57:30.52113+00',
        NULL
    )
ON CONFLICT (username) DO NOTHING;
