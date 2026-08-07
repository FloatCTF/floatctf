#!/usr/bin/env bash
set -Eeuo pipefail

: "${PROJECT_ROOT:?PROJECT_ROOT 未设置，请使用 mise dev 或 source ./activate.sh}"

RUSTFS_LOG_DIR="$PROJECT_ROOT/app/logs/rustfs"
NGINX_LOG_DIR="$PROJECT_ROOT/app/logs/nginx"

# 日志目录只有首次创建（或属主/权限被改动）时才需要 sudo；
# 已就绪时跳过，避免每次启动都要求管理员权限。
NEEDS_INIT=0
for d in "$PROJECT_ROOT/app" "$PROJECT_ROOT/app/logs" "$NGINX_LOG_DIR"; do
    if [ ! -d "$d" ] || [ "$(stat -c '%u:%g:%a' "$d" 2>/dev/null || true)" != "$(id -u):$(id -g):755" ]; then
        NEEDS_INIT=1
    fi
    done
if [ ! -d "$RUSTFS_LOG_DIR" ] || [ "$(stat -c '%u:%g:%a' "$RUSTFS_LOG_DIR" 2>/dev/null || true)" != "10001:10001:750" ]; then
    NEEDS_INIT=1
fi

if [ "$NEEDS_INIT" -eq 1 ]; then
    echo "初始化日志目录（需要 sudo，仅首次或属主/权限变更时）..."
    sudo install -d -m 755 \
        -o "$USER" \
        -g "$(id -gn)" \
        "$PROJECT_ROOT/app" \
        "$PROJECT_ROOT/app/logs" \
        "$NGINX_LOG_DIR"

    sudo install -d -m 750 \
        -o 10001 \
        -g 10001 \
        "$RUSTFS_LOG_DIR"
else
    echo "日志目录已就绪，跳过 sudo 初始化"
fi

COMPOSE_FILE="$PROJECT_ROOT/infra/compose/compose.dev.yml"

exec docker compose \
  -f "$COMPOSE_FILE" \
  up -d --build "$@"
