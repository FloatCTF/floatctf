#!/usr/bin/env bash
set -Eeuo pipefail

# --------------------------------------------------
# FloatCTF GameBox Runtime Contract
# --------------------------------------------------

: "${GAMEBOX_USERNAME:?GAMEBOX_USERNAME is required}"
: "${GAMEBOX_USERPASS:?GAMEBOX_USERPASS is required}"

# username 必须是普通 Linux 用户名。
if [[ ! "$GAMEBOX_USERNAME" =~ ^[a-z_][a-z0-9_-]{0,31}$ ]]; then
    echo "[FloatCTF] invalid GAMEBOX_USERNAME" >&2
    exit 1
fi

# --------------------------------------------------
# Initialize SSH
# --------------------------------------------------

mkdir -p /run/sshd

# 首次启动容器时生成 SSH host keys。
ssh-keygen -A >/dev/null 2>&1

# 如果用户不存在则创建。
if ! id "$GAMEBOX_USERNAME" >/dev/null 2>&1; then
    useradd \
        --create-home \
        --shell /bin/bash \
        "$GAMEBOX_USERNAME"
fi

# 设置 GameBox 登录密码。
printf '%s:%s\n' \
    "$GAMEBOX_USERNAME" \
    "$GAMEBOX_USERPASS" \
    | chpasswd

# --------------------------------------------------
# Remove credentials from child process environment
# --------------------------------------------------

unset GAMEBOX_USERPASS
unset GAMEBOX_USERNAME

# --------------------------------------------------
# Start SSH
# --------------------------------------------------

/usr/sbin/sshd

# --------------------------------------------------
# Start challenge service
# --------------------------------------------------

exec "$@"
