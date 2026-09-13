#!/usr/bin/env bash
set -Eeuo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

if [ "$(id -u)" -eq 0 ]; then
    echo "请以开发者用户运行: mise run setup（脚本会在主机初始化阶段自行调用 sudo）" >&2
    exit 1
fi

command -v mise >/dev/null 2>&1 || {
    echo "缺少 mise，请先安装 mise 并激活 shell。" >&2
    exit 1
}

printf '%s\n' '==> 安装固定版本工具链与项目依赖'
mise install
pnpm install
cargo fetch

printf '%s\n' '==> 构建 floatctf-helper'
cargo build -p floatctf-helper

printf '%s\n' '==> 初始化宿主网络能力并安装宿主控制 helper'
sudo "$PROJECT_ROOT/scripts/install.sh" \
    --develop \
    --helper-bin "$PROJECT_ROOT/target/debug/floatctf-helper"

printf '%s\n' '==> 验证 helper'
systemctl --no-pager --quiet is-active floatctf-helper.service || {
    echo "floatctf-helper 未运行，请检查: journalctl -u floatctf-helper" >&2
    exit 1
}
[ -S /run/floatctf/helper-control.sock ] && [ -S /run/floatctf/helper-docker.sock ] || {
    echo "缺少 helper socket（/run/floatctf/helper-control.sock 或 helper-docker.sock）" >&2
    exit 1
}

if id -nG "$USER" | tr ' ' '\n' | grep -qx floatctf \
    && id -nG "$USER" | tr ' ' '\n' | grep -qx docker; then
    echo "开发宿主已就绪。下一步: mise run dev"
else
    cat <<'EOF'
开发宿主已就绪；当前 shell 尚未获得新组权限。
请注销并重新登录一次，然后执行: mise run dev
EOF
fi
