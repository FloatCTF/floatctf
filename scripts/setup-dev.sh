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

# ── 宿主依赖（安装器不再代装，这里同样只检查）────────────────────────────────
missing=()
for c in docker nft wg ip python3; do
    command -v "$c" >/dev/null 2>&1 || missing+=("$c")
done
if [ "${#missing[@]}" -gt 0 ]; then
    cat >&2 <<EOF
缺少命令: ${missing[*]}
请先按 INSTALL.md §0 准备宿主依赖（Docker、nftables、wireguard-tools、iproute2、python3），
例如 Debian/Ubuntu:
  sudo apt-get install -y docker.io nftables wireguard-tools iproute2 conntrack iptables procps python3
EOF
    exit 1
fi
docker info >/dev/null 2>&1 || {
    echo "Docker daemon 未运行：sudo systemctl enable --now docker" >&2
    exit 1
}
python3 -c 'import tomllib' >/dev/null 2>&1 || {
    echo "python3 缺少 stdlib tomllib（需要 Python ≥3.11）" >&2
    exit 1
}
[ "$(cat /proc/sys/net/ipv4/ip_forward 2>/dev/null || echo 0)" = "1" ] || {
    echo "net.ipv4.ip_forward!=1；开发 AWD 需要转发：sudo sysctl -w net.ipv4.ip_forward=1" >&2
    exit 1
}

printf '%s\n' '==> 安装固定版本工具链与项目依赖'
mise install
pnpm install
cargo fetch

printf '%s\n' '==> 构建 floatctf-helper'
cargo build -p floatctf-helper

printf '%s\n' '==> 创建组/用户并安装宿主控制 helper'
sudo groupadd -f floatctf
if ! id -u floatctf-helper >/dev/null 2>&1; then
    sudo useradd --system --no-create-home --gid floatctf --shell /usr/sbin/nologin floatctf-helper
fi
if getent group docker >/dev/null 2>&1; then
    sudo usermod -aG docker floatctf-helper
fi

sudo install -D -o root -g root -m 0755 \
    "$PROJECT_ROOT/target/debug/floatctf-helper" /usr/local/libexec/floatctf-helper
sudo install -m 0644 \
    "$PROJECT_ROOT/deploy/systemd/floatctf-helper.service" /etc/systemd/system/floatctf-helper.service
sudo systemctl daemon-reload
sudo systemctl enable --now floatctf-helper.service

# 开发者账号需要 docker（仓库 Compose）与 floatctf（helper socket）两组
for group in docker floatctf; do
    getent group "$group" >/dev/null 2>&1 || continue
    if ! id -nG "$USER" | tr ' ' '\n' | grep -qx "$group"; then
        sudo usermod -aG "$group" "$USER"
        printf '已把 %s 加入 %s 组（重新登录后生效）\n' "$USER" "$group"
    fi
done

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
