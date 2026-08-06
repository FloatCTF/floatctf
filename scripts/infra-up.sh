#!/usr/bin/env bash
set -Eeuo pipefail

: "${PROJECT_ROOT:?PROJECT_ROOT 未设置，请使用 mise dev 或 source ./activate.sh}"

RUSTFS_DATA_DIR="$PROJECT_ROOT/app/data"
RUSTFS_LOG_DIR="$PROJECT_ROOT/app/logs/rustfs"
NGINX_LOG_DIR="$PROJECT_ROOT/app/logs/nginx"

sudo install -d -m 755 \
  -o "$USER" \
  -g "$(id -gn)" \
  "$PROJECT_ROOT/app" \
  "$PROJECT_ROOT/app/logs" \
  "$NGINX_LOG_DIR"

sudo install -d -m 750 \
  -o 10001 \
  -g 10001 \
  "$RUSTFS_DATA_DIR" \
  "$RUSTFS_LOG_DIR"

exec docker compose \
  -f "$PROJECT_ROOT/infra/compose/compose.dev.yml" \
  up -d --build "$@"
