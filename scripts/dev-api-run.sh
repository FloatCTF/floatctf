#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ "$(id -u)" -eq 0 ]; then
    echo "dev-api-run 必须由开发者用户启动" >&2
    exit 1
fi

# 编译始终由开发者本人完成，避免 root-owned target/。
cargo build -p floatctf

FLOATCTF_GID="$(getent group floatctf | cut -d: -f3)"
[ -n "$FLOATCTF_GID" ] || {
    echo "缺少 floatctf 组；请先执行: mise run setup" >&2
    exit 1
}

CONFIG="${FLOATCTF_CONFIG:-$ROOT/apps/api/config/development.toml}"
BIN="$ROOT/target/debug/floatctf"

# API 与生产保持同一权限边界：保留开发者 UID 以继续读取源码/开发目录，
# 主组与唯一 supplementary group 都收敛为 floatctf，并显式丢弃 docker 等
# 其他附加组与全部 capabilities。这样即使开发者为了 Docker Compose 属于
# docker 组，API 进程也无法直接打开 /var/run/docker.sock。
# sudo 只用于 setgroups；mise run dev 在启动阶段已经执行 sudo -v。
cd "$ROOT/apps/api"
exec sudo -n /usr/bin/setpriv \
    --reuid="$(id -u)" \
    --regid="$FLOATCTF_GID" \
    --groups="$FLOATCTF_GID" \
    --inh-caps=-all \
    --ambient-caps=-all \
    --bounding-set=-all \
    --no-new-privs \
    env \
      HOME="$HOME" \
      PATH="$PATH" \
      FLOATCTF_CONFIG="$CONFIG" \
      "$BIN"
