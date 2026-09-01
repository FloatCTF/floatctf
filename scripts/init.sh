#!/usr/bin/env bash
#
# FloatCTF host initialization (Phase 10.2) — HOST INITIALIZATION ONLY.
#
# 目的：准备/检查部署所需的主机前置条件，让 API 的 HostNetworkRuntime
# （nftables + WireGuard + Docker 29）具备可信的运行底座。
#
# 本脚本绝不：安装 PostgreSQL/nginx/Rust/Node、构建/部署 FloatCTF、创建数据库、
# 启动 postgres/nginx 容器、创建 systemd 单元、创建生产赛事网络。
#
# 破坏性网络检查一律使用临时唯一命名资源，并在退出（含失败）时 trap 清理。
# 失败即退出（fail-closed）；不支持的发行版路径明确报错，不盲目安装。
#
# 用法：
#   sudo scripts/init.sh            # 检查 + 准备（安装缺失的主机包、建用户、建布局）
#   sudo scripts/init.sh --check    # 只检查并报告，不写任何状态
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
FCTF_ROOT="${FCTF_ROOT:-/home/floatctf}"
FCTF_USER="floatctf"

# ── 颜色/日志（无 tty 时静默降级）─────────────────────────────────────────────
if [ -t 1 ]; then
    C_INFO=$'\033[0;34m'; C_OK=$'\033[0;32m'; C_WARN=$'\033[1;33m'; C_ERR=$'\033[0;31m'; C_END=$'\033[0m'
else
    C_INFO=''; C_OK=''; C_WARN=''; C_ERR=''; C_END=''
fi
info()  { printf '%s[INFO]%s %s\n'  "$C_INFO"  "$C_END" "$*"; }
ok()    { printf '%s[ OK ]%s %s\n'  "$C_OK"    "$C_END" "$*"; }
warn()  { printf '%s[WARN]%s %s\n'  "$C_WARN"  "$C_END" "$*"; }
die()   { printf '%s[FAIL]%s %s\n'  "$C_ERR"   "$C_END" "$*" >&2; exit 1; }

# ── trap 清理表：只登记成功创建的临时资源，避免误删既有同名资源 ────────────────
TMP_DOCKER_NET=""
TMP_NFT_TABLE=""
TMP_WG_IFACE=""
cleanup() {
    local rc=$?
    if [ -n "$TMP_WG_IFACE" ]; then
        ip link del "$TMP_WG_IFACE" >/dev/null 2>&1 || true
    fi
    if [ -n "$TMP_NFT_TABLE" ]; then
        nft delete table inet "$TMP_NFT_TABLE" >/dev/null 2>&1 || true
    fi
    if [ -n "$TMP_DOCKER_NET" ]; then
        docker network rm "$TMP_DOCKER_NET" >/dev/null 2>&1 || true
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM

# ── 根权限 ────────────────────────────────────────────────────────────────────
require_root() {
    [ "$(id -u)" -eq 0 ] || die "需要 root（sudo scripts/init.sh）"
}

# ── 发行版检测 ────────────────────────────────────────────────────────────────
detect_distro() {
    if command -v pacman >/dev/null 2>&1; then
        echo "arch"; return
    fi
    if command -v apt-get >/dev/null 2>&1; then
        echo "debian"; return
    fi
    if command -v dnf >/dev/null 2>&1; then
        echo "fedora"; return
    fi
    echo "unknown"
}

# Arch 主机包名（已确认）：docker / docker-compose（v2 插件包）/ nftables /
# wireguard-tools / iproute2 / procps-ng（提供 sysctl）/ openssl / curl / tar。
ARCH_PKGS=(docker docker-compose nftables wireguard-tools iproute2 procps-ng openssl curl tar)

install_arch_pkgs() {
    local missing=()
    local p
    for p in "${ARCH_PKGS[@]}"; do
        if ! pacman -Q "$p" >/dev/null 2>&1; then
            missing+=("$p")
        fi
    done
    if [ "${#missing[@]}" -eq 0 ]; then
        ok "主机包齐全（pacman）"
        return
    fi
    info "安装缺失主机包: ${missing[*]}（pacman -S --needed）"
    pacman -S --needed --noconfirm "${missing[@]}"
    ok "主机包安装完成"
}

# ── 检查项 ────────────────────────────────────────────────────────────────────
check_linux() {
    [ -d /proc/sys ] || die "非标准 Linux（无 /proc/sys），不支持"
    local release
    release=$(uname -r 2>/dev/null || echo "?")
    info "内核: $(uname -s) $(uname -m) $release"
    if command -v systemd-detect-virt >/dev/null 2>&1 \
        && [ "$(systemd-detect-virt 2>/dev/null || true)" = "docker" ]; then
        die "检测到本脚本运行在容器内；FloatCTF 主机初始化必须在真实主机执行"
    fi
    ok "Linux 环境"
}

check_commands() {
    local c
    for c in ip wg nft docker sysctl modprobe; do
        command -v "$c" >/dev/null 2>&1 || die "缺少命令: $c"
    done
    ok "基础命令齐全（ip/wg/nft/docker/sysctl/modprobe）"
}

# Docker：daemon 可用 + 能创建/删除临时网络（trap 清理）。
check_docker() {
    docker info >/dev/null 2>&1 || die "docker daemon 不可用（docker info 失败）"
    local ver
    ver=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo "?")
    info "Docker daemon: $ver"
    local backend
    backend=$(docker info --format '{{.Driver}}' 2>/dev/null || echo "?")
    info "Docker storage driver: $backend"
    TMP_DOCKER_NET="fctf-init-$$-$(date +%s)"
    docker network create --driver bridge "$TMP_DOCKER_NET" >/dev/null \
        || die "docker 无法创建临时网络（权限/daemon 异常）"
    ok "docker 可用（临时网络 $TMP_DOCKER_NET 已创建，退出时清理）"
}

# nftables：工具可用 + 能创建/删除临时表（trap 清理）。
check_nftables() {
    nft --version >/dev/null 2>&1 || die "nft 不可用"
    local fam
    fam=$(nft --version | grep -o 'nf_tables' || echo "legacy")
    info "nftables: $fam"
    TMP_NFT_TABLE="fctf_init_$$"
    nft add table inet "$TMP_NFT_TABLE" >/dev/null \
        || die "nft 无法创建临时表（权限/内核支持异常）"
    ok "nftables 可用（临时表 $TMP_NFT_TABLE 已创建，退出时清理）"
}

# WireGuard：工具可用 + 内核支持创建临时接口（trap 清理）。
check_wireguard() {
    wg --version >/dev/null 2>&1 || die "wg 不可用"
    # 接口名必须 ≤ 15 字符（IFNAMSIZ）：超长名在较新内核上直接报
    # "Attribute failed policy validation"（真实主机实测）。
    TMP_WG_IFACE="fctf-i-$$"
    ip link add "$TMP_WG_IFACE" type wireguard >/dev/null 2>&1 \
        || die "WireGuard 内核支持不可用（ip link add type wireguard 失败）"
    ok "WireGuard 可用（临时接口 $TMP_WG_IFACE 已创建，退出时清理）"
}

# IPv4 转发：读取当前值；--check 只报告，默认模式写 1（幂等）。
check_ip_forward() {
    local v
    v=$(cat /proc/sys/net/ipv4/ip_forward 2>/dev/null || echo "?")
    if [ "$v" = "1" ]; then
        ok "net.ipv4.ip_forward=1"
        return
    fi
    if [ "$CHECK_ONLY" = "1" ]; then
        warn "net.ipv4.ip_forward=$v（应为 1；--check 不写状态）"
        return
    fi
    sysctl -w net.ipv4.ip_forward=1 >/dev/null
    ok "net.ipv4.ip_forward 已设为 1"
}

# br_netfilter：同桥容器流量必须经过 FORWARD 链，FloatCTF 隔离才生效
# （Phase 9 真实主机实测；nftables.rs reconcile 时 best-effort，生产由 init 预检）。
check_bridge_netfilter() {
    if ! modprobe br_netfilter 2>/dev/null; then
        warn "modprobe br_netfilter 失败（内核未含该模块？同桥隔离将不生效）"
        return
    fi
    local ok_bridge=1
    local k
    for k in net.bridge.bridge-nf-call-iptables net.bridge.bridge-nf-call-ip6tables; do
        local v
        v=$(cat "/proc/sys/$k" 2>/dev/null || echo "?")
        if [ "$v" != "1" ]; then
            if [ "$CHECK_ONLY" = "1" ]; then
                warn "$k=$v（应为 1；--check 不写状态）"
                ok_bridge=0
            else
                sysctl -w "$k=1" >/dev/null || { warn "无法写入 $k"; ok_bridge=0; }
            fi
        fi
    done
    [ "$ok_bridge" = "1" ] && ok "br_netfilter + bridge-nf-call-{ip,ip6}tables=1"
}

# floatctf 服务用户 + /home/floatctf 布局。
check_user_layout() {
    if ! id "$FCTF_USER" >/dev/null 2>&1; then
        if [ "$CHECK_ONLY" = "1" ]; then
            warn "服务用户 $FCTF_USER 不存在（--check 不创建）"
        else
            useradd --system --home-dir "$FCTF_ROOT" --shell /usr/sbin/nologin "$FCTF_USER"
            ok "已创建系统用户 $FCTF_USER"
        fi
    else
        ok "服务用户 $FCTF_USER 存在"
    fi
    local d
    for d in bin web config/nginx data/postgres logs/api logs/nginx runtime gameboxes; do
        mkdir -p "$FCTF_ROOT/$d"
    done
    # 归属：floatctf 用户可读写运行数据；目录本身保持 755。
    if ! chown -R "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT" >/dev/null 2>&1; then
        warn "chown $FCTF_ROOT 需要 root（请以 sudo 运行）"
    fi
    ok "布局就绪: $FCTF_ROOT/{bin,web,config/nginx,data/postgres,logs/{api,nginx},runtime,gameboxes}"
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
CHECK_ONLY=0
case "${1:-}" in
    --check) CHECK_ONLY=1 ;;
    -h|--help)
        sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    "" ) ;;
    *) die "未知参数: $1（支持: 无参数 / --check / --help）" ;;
esac

require_root
check_linux

DISTRO=$(detect_distro)
info "发行版: $DISTRO"
case "$DISTRO" in
    arch)
        if [ "$CHECK_ONLY" != "1" ]; then
            install_arch_pkgs
        else
            info "--check 模式：不安装任何包（缺失的包将按 check_commands 报告）"
        fi
        ;;
    debian|fedora)
        die "发行版 $DISTRO 尚未实现安装路径（包名未确认）；请手动安装 docker/nftables/wireguard-tools/iproute2/procps 后重试。已支持：Arch Linux（pacman）"
        ;;
    unknown)
        die "无法识别的发行版；不支持盲装。请参考 chore/phase10-deployment-design.md §7"
        ;;
esac

check_commands
check_docker
check_nftables
check_wireguard
check_ip_forward
check_bridge_netfilter
check_user_layout

if [ "$CHECK_ONLY" = "1" ]; then
    ok "主机预检完成（--check 只读，未写任何状态）"
else
    ok "主机初始化完成：docker/nftables/WireGuard/转发/br_netfilter/用户/布局 就绪"
fi
