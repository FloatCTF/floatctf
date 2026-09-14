#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ "$(id -u)" -eq 0 ]; then
    echo "开发进程必须由当前开发者用户运行，请直接执行: mise run dev" >&2
    exit 1
fi

# Docker Compose 与“API 丢弃 docker 组”都需要一次 sudo 凭据；后续自动重启使用 -n。
sudo -v

if ! docker info >/dev/null 2>&1; then
    echo "当前用户无法访问 Docker daemon。首次 setup 后请注销并重新登录。" >&2
    exit 1
fi

HELPER_CONTROL_SOCKET=/run/floatctf/helper-control.sock
HELPER_DOCKER_SOCKET=/run/floatctf/helper-docker.sock
if ! systemctl --quiet is-active floatctf-helper.service 2>/dev/null \
    || [ ! -S "$HELPER_CONTROL_SOCKET" ] || [ ! -S "$HELPER_DOCKER_SOCKET" ]; then
    echo "floatctf-helper 未就绪，请先执行: mise run setup" >&2
    exit 1
fi
if [ ! -r "$HELPER_CONTROL_SOCKET" ] || [ ! -w "$HELPER_CONTROL_SOCKET" ] || [ ! -r "$HELPER_DOCKER_SOCKET" ] || [ ! -w "$HELPER_DOCKER_SOCKET" ]; then
    echo "当前用户没有 helper socket 权限。首次 setup 后请注销并重新登录。" >&2
    exit 1
fi

printf '%s\n' '==> 应用数据库 migrations'
mise run db:migration:apply

cat <<'EOF'
==> FloatCTF 开发环境就绪
Web:    http://0.0.0.0:7780  (LAN: http://<host-ip>:7780)
API:    0.0.0.0:9090  (local: http://127.0.0.1:9090; normally use Caddy :7780)
RustFS: http://127.0.0.1:9001

API 使用 watchexec 自动编译/重启；Web 使用 Vite HMR。
Ctrl+C 结束 API/Web；基础设施数据继续保留。
EOF

pids=()
cleanup() {
    local rc=$?
    trap - EXIT INT TERM
    for pid in "${pids[@]:-}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    exit "$rc"
}
trap cleanup EXIT INT TERM

mise run dev:api &
pids+=("$!")
mise run dev:web &
pids+=("$!")

set +e
wait -n "${pids[@]}"
status=$?
set -e
exit "$status"
