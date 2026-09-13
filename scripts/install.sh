#!/usr/bin/env bash
#
# FloatCTF 一键安装器（Phase 11）— 单文件自包含。
#
# 本脚本**不依赖仓库其他文件**：所有模板（compose.dev/prod、floatctf.toml、Caddyfile、
# systemd 单元、uninstall.sh）都内嵌在本文件里，运行时写出到 FLOATCTF_HOME。
#
# 两个入口共享同一权限模型：生产 API 运行在无 capabilities 的非 root 容器中，
# 开发 API 由 setpriv 收敛权限；宿主 Docker/网络控制统一由 floatctf-helper
# （docker group + CAP_NET_ADMIN）通过 helper-control.sock / helper-docker.sock 提供。
#
# 【生产安装】无需 clone 仓库：
#   curl -fsSL <install.sh 的 URL> -o install.sh && sudo bash install.sh
#   或显式指定 4 个 release 产物 URL：
#     sudo bash install.sh --api-url <bin> --helper-url <bin> --web-url <dist> --migrate-url <sql>
#   流程：主机初始化 → 下载 4 产物 → 本地构建 API runtime image → 部署 →
#         写 helper/Compose systemd 单元并 enable（不 start）。
#
# 【开发宿主初始化】由 `mise run setup` 内部调用；开发者无需直接运行：
#   sudo ./scripts/install.sh --develop --helper-bin <target/debug/floatctf-helper>
#   仅准备主机、系统用户/组、内核参数与 floatctf-helper，不创建 dev systemd infra。
#
# 环境变量（覆盖 4 个产物 URL / release 版本）：
#   FLOATCTF_API_URL / FLOATCTF_HELPER_URL / FLOATCTF_WEB_URL /
#   FLOATCTF_MIGRATE_URL / FLOATCTF_VERSION
# 安装根：
#   FLOATCTF_HOME=/opt/floatctf   （默认 /home/floatctf）
#
# 注意：
#   - 只做全新安装；已有数据的升级（forward-only 迁移）后续单独实现。
#   - 本脚本只写文件、创建（enable）systemd 服务，绝不自己启动服务/容器：
#     整平台由运维 systemctl start floatctf.target 启动（首次启动 postgres
#     自动用 merged.sql 初始化数据库）。
#   - AWD 服务镜像（floatctf/awd-flagserver / awd-judgeserver）暂不在本脚本构建
#     （TODO Phase 11.1：registry 拉取或本地 docker build）。
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
FLOATCTF_HOME="${FLOATCTF_HOME:-/home/floatctf}"
FCTF_USER="floatctf"
FCTF_HELPER_USER="floatctf-helper"
HELPER_INSTALL_PATH="/usr/local/libexec/floatctf-helper"

# fake 占位地址：真实 release 地址发布后替换（或经 --*-url / 环境变量覆盖）。
DEFAULT_API_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/floatctf"
DEFAULT_HELPER_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/floatctf-helper"
DEFAULT_WEB_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/web-dist.tar.gz"
DEFAULT_MIGRATE_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/merged.sql"

# ── 颜色/日志 ─────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    C_INFO=$'\033[0;34m'; C_OK=$'\033[0;32m'; C_WARN=$'\033[1;33m'; C_ERR=$'\033[0;31m'; C_END=$'\033[0m'
else
    C_INFO=''; C_OK=''; C_WARN=''; C_ERR=''; C_END=''
fi
info() { printf '%s[INFO]%s %s\n' "$C_INFO" "$C_END" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n' "$C_OK" "$C_END" "$*"; }
warn() { printf '%s[WARN]%s %s\n' "$C_WARN" "$C_END" "$*"; }
die()  { printf '%s[FAIL]%s %s\n' "$C_ERR" "$C_END" "$*" >&2; exit 1; }

# ── 参数 ──────────────────────────────────────────────────────────────────────
API_URL="$DEFAULT_API_URL"
HELPER_URL="$DEFAULT_HELPER_URL"
WEB_URL="$DEFAULT_WEB_URL"
MIGRATE_URL="$DEFAULT_MIGRATE_URL"
VERSION="${FLOATCTF_VERSION:-}"
VERSION_EXPLICIT=0
API_URL_EXPLICIT=0
HELPER_URL_EXPLICIT=0
WEB_URL_EXPLICIT=0
MIGRATE_URL_EXPLICIT=0
DEVELOP=0
HELPER_BIN=""
while [ $# -gt 0 ]; do
    case "$1" in
        --api-url) API_URL="${2:?--api-url 需要一个地址参数}"; API_URL_EXPLICIT=1; shift ;;
        --helper-url) HELPER_URL="${2:?--helper-url 需要一个地址参数}"; HELPER_URL_EXPLICIT=1; shift ;;
        --helper-bin) HELPER_BIN="${2:?--helper-bin 需要本地二进制路径}"; shift ;;
        --web-url) WEB_URL="${2:?--web-url 需要一个地址参数}"; WEB_URL_EXPLICIT=1; shift ;;
        --migrate-url) MIGRATE_URL="${2:?--migrate-url 需要一个地址参数}"; MIGRATE_URL_EXPLICIT=1; shift ;;
        --version) VERSION="${2:?--version 需要 release 版本，如 0.3.3}"; VERSION_EXPLICIT=1; shift ;;
        --develop) DEVELOP=1 ;;
        -h|--help)
            sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) die "未知参数: $1（--help 查看用法）" ;;
    esac
    shift
done
# 环境变量兜底。优先级：显式命令行参数 > FLOATCTF_* 环境变量 > 默认值/自动推导。
# （shell 惯例：显式传参优先，环境变量只作未传参时的兜底，避免静默覆盖。）
[ "$API_URL_EXPLICIT" -eq 1 ] || API_URL="${FLOATCTF_API_URL:-$API_URL}"
[ "$HELPER_URL_EXPLICIT" -eq 1 ] || HELPER_URL="${FLOATCTF_HELPER_URL:-$HELPER_URL}"
[ "$WEB_URL_EXPLICIT" -eq 1 ] || WEB_URL="${FLOATCTF_WEB_URL:-$WEB_URL}"
[ "$MIGRATE_URL_EXPLICIT" -eq 1 ] || MIGRATE_URL="${FLOATCTF_MIGRATE_URL:-$MIGRATE_URL}"
[ "$VERSION_EXPLICIT" -eq 1 ] || VERSION="${FLOATCTF_VERSION:-$VERSION}"

# 生产镜像统一使用 release 版本 tag。GitHub Release URL 可自动推导版本；
# 自定义产物 URL 需显式 --version / FLOATCTF_VERSION，避免渲染出空 tag 或回退 latest。
if [ -z "$VERSION" ] && [[ "$API_URL" =~ /releases/download/v?([^/]+)/ ]]; then
    VERSION="${BASH_REMATCH[1]}"
fi
if [ -z "$VERSION" ] && [ "$DEVELOP" -eq 1 ] && [ -f apps/api/Cargo.toml ]; then
    VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' apps/api/Cargo.toml | head -n1)"
fi
[ -n "$VERSION" ] || die "无法确定 release 版本；请传 --version <版本> 或设置 FLOATCTF_VERSION"
export VERSION

# ── trap：清理临时资源（init 阶段登记的网络资源 + 下载解压的临时目录）──────────
TMP_DOCKER_NET=""
TMP_NFT_TABLE=""
TMP_WG_IFACE=""
TMP_STAGE_DIR=""
cleanup() {
    local rc=$?
    [ -n "$TMP_WG_IFACE" ] && ip link del "$TMP_WG_IFACE" >/dev/null 2>&1 || true
    [ -n "$TMP_NFT_TABLE" ] && nft delete table inet "$TMP_NFT_TABLE" >/dev/null 2>&1 || true
    [ -n "$TMP_DOCKER_NET" ] && docker network rm "$TMP_DOCKER_NET" >/dev/null 2>&1 || true
    [ -n "$TMP_STAGE_DIR" ] && rm -rf "$TMP_STAGE_DIR" 2>/dev/null || true
    exit "$rc"
}
trap cleanup EXIT INT TERM

# ── 根权限 ────────────────────────────────────────────────────────────────────
require_root() {
    [ "$(id -u)" -eq 0 ] || die "需要 root（sudo ./install.sh）"
}

# ============================================================================
# 第一阶段：主机初始化（幂等，逐项补齐，已存在即 skip）
# ============================================================================
detect_distro() {
    if command -v pacman >/dev/null 2>&1; then echo "arch"; return; fi
    if command -v apt-get >/dev/null 2>&1; then echo "debian"; return; fi
    if command -v dnf >/dev/null 2>&1; then echo "fedora"; return; fi
    echo "unknown"
}

# postgresql 包仅取其客户端 psql（dev 的 migrate.sh status/apply 在宿主执行；
# Arch 无 client-only 包）。生产手动运维/备份同样受益。
ARCH_PKGS=(docker docker-compose nftables wireguard-tools iproute2 conntrack-tools iptables procps-ng openssl curl tar postgresql)

install_arch_pkgs() {
    local missing=() p
    for p in "${ARCH_PKGS[@]}"; do
        pacman -Q "$p" >/dev/null 2>&1 || missing+=("$p")
    done
    if [ "${#missing[@]}" -eq 0 ]; then
        ok "主机包齐全（pacman）"
        return
    fi
    info "安装缺失主机包: ${missing[*]}（pacman -S --needed）"
    pacman -S --needed --noconfirm "${missing[@]}"
    ok "主机包安装完成"
}

check_linux() {
    [ -d /proc/sys ] || die "非标准 Linux（无 /proc/sys），不支持"
    info "内核: $(uname -s) $(uname -m) $(uname -r 2>/dev/null || echo '?')"
    if command -v systemd-detect-virt >/dev/null 2>&1 \
        && [ "$(systemd-detect-virt 2>/dev/null || true)" = "docker" ]; then
        die "检测到本脚本运行在容器内；FloatCTF 主机初始化必须在真实主机执行"
    fi
    ok "Linux 环境"
}

check_commands() {
    local c
    for c in ip wg nft conntrack iptables docker sysctl modprobe; do
        command -v "$c" >/dev/null 2>&1 || die "缺少命令: $c"
    done
    ok "基础命令齐全（ip/wg/nft/conntrack/iptables/docker/sysctl/modprobe）"
}

check_docker() {
    if command -v systemctl >/dev/null 2>&1 && ! systemctl -q is-active docker.service; then
        info "启动并 enable Docker daemon"
        systemctl enable --now docker.service >/dev/null \
            || die "Docker daemon 启动失败"
    fi
    docker info >/dev/null 2>&1 || die "docker daemon 不可用（docker info 失败）"
    info "Docker daemon: $(docker version --format '{{.Server.Version}}' 2>/dev/null || echo '?')"
    info "Docker storage driver: $(docker info --format '{{.Driver}}' 2>/dev/null || echo '?')"
    TMP_DOCKER_NET="fctf-init-$$-$(date +%s)"
    docker network create --driver bridge "$TMP_DOCKER_NET" >/dev/null \
        || die "docker 无法创建临时网络（权限/daemon 异常）"
    ok "docker 可用（临时网络 $TMP_DOCKER_NET 已创建，退出时清理）"
}

check_nftables() {
    nft --version >/dev/null 2>&1 || die "nft 不可用"
    info "nftables: $(nft --version | grep -o 'nf_tables' || echo 'legacy')"
    TMP_NFT_TABLE="fctf_init_$$"
    nft add table inet "$TMP_NFT_TABLE" >/dev/null \
        || die "nft 无法创建临时表（权限/内核支持异常）"
    ok "nftables 可用（临时表 $TMP_NFT_TABLE 已创建，退出时清理）"
}

check_wireguard() {
    wg --version >/dev/null 2>&1 || die "wg 不可用"
    TMP_WG_IFACE="fctf-i-$$"
    ip link add "$TMP_WG_IFACE" type wireguard >/dev/null 2>&1 \
        || die "WireGuard 内核支持不可用（ip link add type wireguard 失败）"
    ok "WireGuard 可用（临时接口 $TMP_WG_IFACE 已创建，退出时清理）"
}

SYSCTL_FILE="/etc/sysctl.d/99-floatctf.conf"
MODULES_FILE="/etc/modules-load.d/floatctf-br-netfilter.conf"

persist_sysctl() {
    local key="$1" value="$2"
    if [ ! -f "$SYSCTL_FILE" ] || ! grep -qE "^${key}\s*=\s*${value}\s*$" "$SYSCTL_FILE"; then
        mkdir -p /etc/sysctl.d
        printf '%s=%s\n' "$key" "$value" >> "$SYSCTL_FILE"
        ok "已持久化 $key=$value → $SYSCTL_FILE"
    fi
}

check_ip_forward() {
    local v
    v=$(cat /proc/sys/net/ipv4/ip_forward 2>/dev/null || echo "?")
    if [ "$v" = "1" ]; then
        ok "net.ipv4.ip_forward=1"
    else
        sysctl -w net.ipv4.ip_forward=1 >/dev/null
        ok "net.ipv4.ip_forward 已设为 1"
    fi
    persist_sysctl "net.ipv4.ip_forward" "1"
}

check_bridge_netfilter() {
    local loaded=1
    if ! modprobe br_netfilter 2>/dev/null; then
        warn "modprobe br_netfilter 失败（内核未含该模块？同桥隔离将不生效）"
        loaded=0
    fi
    if [ "$loaded" != "0" ]; then
        if [ ! -f "$MODULES_FILE" ] || ! grep -qE '^br_netfilter\s*$' "$MODULES_FILE"; then
            mkdir -p /etc/modules-load.d
            printf 'br_netfilter\n' >> "$MODULES_FILE"
            ok "已持久化模块 br_netfilter → $MODULES_FILE"
        fi
    fi
    local ok_bridge=1 k v
    for k in net.bridge.bridge-nf-call-iptables net.bridge.bridge-nf-call-ip6tables; do
        v=$(cat "/proc/sys/$k" 2>/dev/null || echo "?")
        if [ "$v" != "1" ]; then
            sysctl -w "$k=1" >/dev/null || { warn "无法写入 $k"; ok_bridge=0; }
            persist_sysctl "$k" "1"
        fi
    done
    [ "$ok_bridge" = "1" ] && ok "br_netfilter + bridge-nf-call-{ip,ip6}tables=1"
}

ensure_service_users() {
    # 显式创建共享组，避免依赖各发行版 useradd 的 USERGROUPS_ENAB 默认值。
    if ! getent group "$FCTF_USER" >/dev/null 2>&1; then
        groupadd --system "$FCTF_USER"
        ok "已创建系统组 $FCTF_USER"
    fi

    if ! id "$FCTF_USER" >/dev/null 2>&1; then
        useradd --system --gid "$FCTF_USER" --home-dir "$FLOATCTF_HOME" --shell /usr/sbin/nologin "$FCTF_USER"
        ok "已创建系统用户 $FCTF_USER"
    else
        ok "服务用户 $FCTF_USER 存在"
        if [ "$(id -gn "$FCTF_USER")" != "$FCTF_USER" ]; then
            usermod -g "$FCTF_USER" "$FCTF_USER"
            ok "已把 $FCTF_USER 主组收敛到 $FCTF_USER"
        fi
    fi

    if ! id "$FCTF_HELPER_USER" >/dev/null 2>&1; then
        useradd --system --no-create-home --gid "$FCTF_USER" --shell /usr/sbin/nologin "$FCTF_HELPER_USER"
        ok "已创建宿主控制用户 $FCTF_HELPER_USER（主组 $FCTF_USER）"
    else
        ok "宿主控制用户 $FCTF_HELPER_USER 存在"
        if [ "$(id -gn "$FCTF_HELPER_USER")" != "$FCTF_USER" ]; then
            usermod -g "$FCTF_USER" "$FCTF_HELPER_USER"
            ok "已把 $FCTF_HELPER_USER 主组收敛到 $FCTF_USER"
        fi
    fi

    if getent group docker >/dev/null 2>&1; then
        # 迁移旧安装：API 服务用户曾经可能被加入 docker 组。systemd 启动 User=floatctf
        # 时会继承 NSS 中的 supplementary groups，因此必须显式移除，才能保证 API
        # 无法绕过 helper 直连 /var/run/docker.sock。
        if id -nG "$FCTF_USER" 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
            if command -v gpasswd >/dev/null 2>&1; then
                gpasswd -d "$FCTF_USER" docker >/dev/null
            elif command -v deluser >/dev/null 2>&1; then
                deluser "$FCTF_USER" docker >/dev/null
            else
                local keep_groups
                keep_groups="$(id -nG "$FCTF_USER" | tr ' ' '\n' \
                    | grep -vx "$FCTF_USER" | grep -vx docker | paste -sd, -)"
                usermod -G "$keep_groups" "$FCTF_USER"
            fi
            ok "已确保 $FCTF_USER 不属于 docker 组（API Docker 权限仅经 helper）"
        fi

        if ! id -nG "$FCTF_HELPER_USER" 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
            usermod -aG docker "$FCTF_HELPER_USER"
            ok "已把 $FCTF_HELPER_USER 加入 docker 组（Docker 控制面仅授予 helper）"
        fi
    fi
}

check_user_layout() {
    ensure_service_users

    # floatctf UID/GID 同时作为生产 API 容器的 numeric identity；不加入 docker 组。

    local d
    for d in image/api web config/caddy data/postgres data/redis data/rustfs data/caddy data/caddy-config logs/api logs/rustfs runtime gameboxes; do
        mkdir -p "$FLOATCTF_HOME/$d"
    done
    chown root:"$FCTF_USER" "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    local run_dir
    for run_dir in data logs runtime gameboxes; do
        chown -R "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/$run_dir" >/dev/null 2>&1 || true
    done
    chown -R root:"$FCTF_USER" "$FLOATCTF_HOME/image" "$FLOATCTF_HOME/web" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/image" "$FLOATCTF_HOME/image/api" "$FLOATCTF_HOME/web" >/dev/null 2>&1 || true
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/config" >/dev/null 2>&1 || true
    ok "布局就绪: $FLOATCTF_HOME/{image/api,web,config/caddy,data/{postgres,rustfs,caddy,caddy-config},logs/{api,rustfs},runtime,gameboxes}"

    if [ ! -f "$FLOATCTF_HOME/.initialized" ]; then
        printf 'FloatCTF host initialized at %s by %s\n' "$(date -Is 2>/dev/null || date)" "${SUDO_USER:-root}" \
            > "$FLOATCTF_HOME/.initialized"
        chown "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/.initialized"
        ok "完成标记已写入 $FLOATCTF_HOME/.initialized"
    else
        ok "检测到 $FLOATCTF_HOME/.initialized（主机已初始化）"
    fi
}

run_init() {
    local mode="${1:-production}"
    info "──── 第一阶段：主机初始化（幂等，mode=$mode）────"
    require_root
    check_linux

    local DISTRO
    DISTRO=$(detect_distro)
    info "发行版: $DISTRO"
    case "$DISTRO" in
        arch)
            install_arch_pkgs
            ;;
        debian|fedora)
            die "发行版 $DISTRO 尚未实现安装路径（包名未确认）；请手动安装 docker/nftables/wireguard-tools/iproute2/procps 后重试。已支持：Arch Linux（pacman）"
            ;;
        unknown)
            die "无法识别的发行版；不支持盲装"
            ;;
    esac

    check_commands
    check_docker
    check_nftables
    check_wireguard
    check_ip_forward
    check_bridge_netfilter
    if [ "$mode" = "develop" ]; then
        ensure_service_users
        ok "开发宿主初始化完成（docker/nftables/WireGuard/转发/br_netfilter/服务用户 就绪）"
    else
        check_user_layout
        ok "生产宿主初始化完成（docker/nftables/WireGuard/转发/br_netfilter/用户/布局 就绪）"
    fi
}

# ============================================================================
# 第二阶段：获取 release 产物（3 个 URL）
# ============================================================================
download_url() { # url dest
    info "下载: $1"
    curl -fL --retry 3 --connect-timeout 30 -o "$2" "$1" \
        || die "下载失败: $1（这是 fake 占位地址，替换为真实 release 地址或经 --*-url 传入）"
}

download_release() {
    info "──── 第二阶段：下载 release 产物（4 URL）────"
    TMP_STAGE_DIR="$(mktemp -d /tmp/floatctf-install.XXXXXX)"

    # 1) API 二进制
    mkdir -p "$TMP_STAGE_DIR/bin"
    download_url "$API_URL" "$TMP_STAGE_DIR/bin/floatctf"
    chmod 0755 "$TMP_STAGE_DIR/bin/floatctf"

    # 2) 特权网络守护进程（单独 root-owned 安装，不放进 API 可写目录）
    download_url "$HELPER_URL" "$TMP_STAGE_DIR/bin/floatctf-helper"
    chmod 0755 "$TMP_STAGE_DIR/bin/floatctf-helper"

    # 3) 前端静态产物（tar.gz）
    download_url "$WEB_URL" "$TMP_STAGE_DIR/web-dist.tar.gz"
    mkdir -p "$TMP_STAGE_DIR/web"
    tar xzf "$TMP_STAGE_DIR/web-dist.tar.gz" -C "$TMP_STAGE_DIR/web" \
        || die "解压 web-dist 失败"

    # 4) merged.sql
    download_url "$MIGRATE_URL" "$TMP_STAGE_DIR/merged.sql"

    ok "release 产物就绪: $TMP_STAGE_DIR"
    PKG_DIR="$TMP_STAGE_DIR"
}

acquire_package() {
    download_release
}

# ============================================================================
# 内嵌模板（写出到 FLOATCTF_HOME，占位符 ${FLOATCTF_HOME} 替换为实际值）
# ============================================================================
write_compose_prod() {
    cat > "$FLOATCTF_HOME/compose.prod.yml" <<'COMPOSE_PROD_EOF'
# FloatCTF production stack.
# 宿主只保留 floatctf-helper（systemd）；API/PostgreSQL/Redis/RustFS/Caddy 全部由 Compose 管理。
# API 不挂载 Docker socket、不授予 capabilities，只通过 /run/floatctf 的两个受控 helper socket
# 操作 Docker/宿主网络。fctf-platform-control 是 installer 创建的 external internal network，
# 仅供 API 与 FlagServer/JudgeServer 的内部回调；GameBox 不加入。

name: floatctf

services:
    postgres:
        image: postgres:17
        container_name: floatctf-postgres
        restart: unless-stopped
        ports:
            - "127.0.0.1:${POSTGRES_PORT:-5433}:5432"
        environment:
            POSTGRES_USER: ${POSTGRES_USER:-postgres}
            POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:?POSTGRES_PASSWORD 必须在 .env 设置}
            POSTGRES_DB: ${POSTGRES_DB:-floatctf_db}
        volumes:
            - ${FLOATCTF_HOME}/data/postgres:/var/lib/postgresql/data
            - ${FLOATCTF_HOME}/merged.sql:/docker-entrypoint-initdb.d/00-init.sql:ro
        healthcheck:
            test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER:-postgres} -d ${POSTGRES_DB:-floatctf_db}"]
            interval: 5s
            timeout: 3s
            retries: 10
            start_period: 10s

    redis:
        image: redis:7-alpine
        container_name: floatctf-redis
        restart: unless-stopped
        ports:
            - "127.0.0.1:${REDIS_PORT:-6380}:6379"
        command: ["redis-server", "--appendonly", "yes"]
        volumes:
            - floatctf-redis-data:/data
        healthcheck:
            test: ["CMD", "redis-cli", "ping"]
            interval: 5s
            timeout: 3s
            retries: 10
            start_period: 5s

    rustfs:
        image: rustfs/rustfs:1.0.0-beta.12
        container_name: floatctf-rustfs
        restart: unless-stopped
        user: "10001:10001"
        ports:
            - "127.0.0.1:${RUSTFS_PORT:-9000}:9000"
            - "127.0.0.1:${RUSTFS_CONSOLE_PORT:-9001}:9001"
        volumes:
            - ${FLOATCTF_HOME}/data/rustfs:/data
            - ${FLOATCTF_HOME}/logs/rustfs:/logs
        environment:
            RUSTFS_ADDRESS: ":9000"
            # API/Caddy 通过 Compose DNS + path-style S3 访问；不要启用 virtual-host domains，
            # 否则 Host: rustfs:9000 会被 RustFS 误判为 bucket 名。
            RUSTFS_ACCESS_KEY: ${RUSTFS_ACCESS_KEY:?RUSTFS_ACCESS_KEY 必须在 .env 设置}
            RUSTFS_SECRET_KEY: ${RUSTFS_SECRET_KEY:?RUSTFS_SECRET_KEY 必须在 .env 设置}
            RUSTFS_CONSOLE_ENABLE: "true"
            RUSTFS_OBS_LOG_DIRECTORY: /logs
        healthcheck:
            test: ["CMD-SHELL", "nc -z 127.0.0.1 9000 || exit 1"]
            interval: 5s
            timeout: 3s
            retries: 10
            start_period: 10s

    api:
        image: floatctf/api:${VERSION:?VERSION 必须在 .env 设置}
        container_name: floatctf-api
        restart: unless-stopped
        user: "${FLOATCTF_UID:?FLOATCTF_UID 必须在 .env 设置}:${FLOATCTF_GID:?FLOATCTF_GID 必须在 .env 设置}"
        cap_drop:
            - ALL
        security_opt:
            - no-new-privileges:true
        read_only: true
        tmpfs:
            - /tmp:rw,noexec,nosuid,nodev,size=64m
        environment:
            FLOATCTF_CONFIG: /etc/floatctf/floatctf.toml
        volumes:
            - ${FLOATCTF_HOME}/config/floatctf.toml:/etc/floatctf/floatctf.toml:ro
            - ${FLOATCTF_HOME}/runtime:/var/lib/floatctf/runtime
            - /run/floatctf:/run/floatctf:ro
        networks:
            default: {}
            platform_control:
                ipv4_address: 10.42.8.2
        depends_on:
            postgres:
                condition: service_healthy
            redis:
                condition: service_healthy
            rustfs:
                condition: service_healthy
        healthcheck:
            test: ["CMD-SHELL", "curl -sS -o /dev/null --max-time 2 http://127.0.0.1:${API_PORT:-9090}/api/users/me"]
            interval: 10s
            timeout: 3s
            retries: 12
            start_period: 20s

    caddy:
        image: caddy:2-alpine
        container_name: floatctf-caddy
        restart: unless-stopped
        ports:
            - "${HTTP_PORT:-80}:${HTTP_PORT:-80}"
            - "${HTTPS_PORT:-443}:${HTTPS_PORT:-443}/tcp"
            - "${HTTPS_PORT:-443}:${HTTPS_PORT:-443}/udp"
        environment:
            SITE_ADDRESS: ${SITE_ADDRESS:?SITE_ADDRESS 必须在 .env 设置}
            HTTP_PORT: ${HTTP_PORT:-80}
            HTTPS_PORT: ${HTTPS_PORT:-443}
            API_PORT: ${API_PORT:-9090}
        volumes:
            - ${FLOATCTF_HOME}/config/caddy:/etc/caddy:ro
            - ${FLOATCTF_HOME}/web:/srv/web:ro
            - ${FLOATCTF_HOME}/runtime/challenges:/srv/challenges:ro
            - ${FLOATCTF_HOME}/data/caddy:/data
            - ${FLOATCTF_HOME}/data/caddy-config:/config
        depends_on:
            api:
                condition: service_healthy
            rustfs:
                condition: service_healthy
        healthcheck:
            test: ["CMD-SHELL", "caddy validate --config /etc/caddy/Caddyfile >/dev/null 2>&1 || exit 1"]
            interval: 10s
            timeout: 3s
            retries: 10
            start_period: 5s

volumes:
    floatctf-redis-data:

networks:
    platform_control:
        external: true
        name: fctf-platform-control
COMPOSE_PROD_EOF
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/compose.prod.yml"
    ok "已写出 compose.prod.yml（API + PostgreSQL + Redis + RustFS + Caddy）"
}
write_config_template() {
    cat > "$FLOATCTF_HOME/.floatctf.toml.tmpl" <<'CONFIG_TMPL_EOF'
# FloatCTF production process-static configuration.
# This file is rendered by scripts/install.sh; ${...} placeholders are installer substitutions.

[application]
main_url = "https://${SITE_ADDRESS}:${HTTPS_PORT}"

[server]
work_dir = "/var/lib/floatctf/runtime"
host_address = "${HOST_ADDRESS}"
listen_ip = "0.0.0.0"
listen_port = ${API_PORT}

[logging]
timezone = "Asia/Shanghai"
filter = "info,actix_web=warn,sqlx=warn"

[docker]
socket_path = "/run/floatctf/helper-docker.sock"

[database]
url = "postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/${POSTGRES_DB}"
max_connections = 64
min_connections = 4
connect_timeout_seconds = 10
acquire_timeout_seconds = 10

[rustfs]
endpoint_url = "http://rustfs:9000"
access_key_id = "${RUSTFS_ACCESS_KEY}"
secret_access_key = "${RUSTFS_SECRET_KEY}"
region = "cn-east-1"

[auth]
jwt_secret = "${JWT_SECRET}"

[redis]
url = "redis://redis:6379/"

[realtime]
channel = "floatctf:realtime"

[awd]
network_runtime = "helper"
flagserver_image = "floatctf/awd-flagserver:${VERSION}"
judgeserver_image = "floatctf/awd-judgeserver:${VERSION}"
platform_internal_url = "http://10.42.8.2:${API_PORT}"
platform_internal_network = "fctf-platform-control"

[awdp]
practice_judgeserver_image = "floatctf/infra/awdp-judgeserver:${VERSION}"
practice_network_subnet = "10.42.2.0/23"
practice_judge_ip = "10.42.2.2"
network_pool = "10.43.0.0/16"
event_netmask = 24
platform_internal_url = "http://10.42.8.2:${API_PORT}"

[registry]
image_prefix = "floatctf"
push = false
insecure = false
build_timeout_secs = 900

[cors]
allowed_origins = []

[features]
unsafe_sql_admin = false
web_terminal = true
CONFIG_TMPL_EOF
    ok "已写出 config 模板"
}

write_caddy_template() {
    cat > "$FLOATCTF_HOME/.Caddyfile.tmpl" <<'CADDY_TMPL_EOF'
# FloatCTF production Caddy configuration.
# Caddy 与 API/RustFS 同处 Compose default network；公网只发布 Caddy HTTP/HTTPS。
{
    http_port {$HTTP_PORT:80}
    https_port {$HTTPS_PORT:443}
}

{$SITE_ADDRESS} {
    encode zstd gzip
    log {
        output stdout
    }

    # API, SSE and WebSocket (including the web terminal).
    handle /api/* {
        reverse_proxy api:{$API_PORT:9090}
    }

    # RustFS public bucket: /public/a.png -> /floatctf-public/a.png
    handle_path /public/* {
        rewrite * /floatctf-public{path}
        reverse_proxy rustfs:9000
    }

    # RustFS private bucket. Presigned SigV4 URLs use the API-side internal endpoint
    # `http://rustfs:9000`, so upstream Host must remain exactly rustfs:9000.
    handle_path /private/* {
        rewrite * /floatctf-private{path}
        reverse_proxy rustfs:9000 {
            header_up Host rustfs:9000
        }
    }

    @challenge_attachment path_regexp challenge ^/static/challenges/([^/]+)/attachment/(.+)$
    handle @challenge_attachment {
        rewrite * /{re.challenge.1}/attachment/{re.challenge.2}
        root * /srv/challenges
        header X-Content-Type-Options nosniff
        header Content-Disposition attachment
        file_server
    }

    handle {
        root * /srv/web
        try_files {path} {path}/ /index.html
        file_server
    }
}
CADDY_TMPL_EOF
    ok "已写出 Caddy 模板"
}

write_api_dockerfile() {
    mkdir -p "$FLOATCTF_HOME/image/api"
    cat > "$FLOATCTF_HOME/image/api/Dockerfile" <<'API_DOCKERFILE_EOF'
FROM ubuntu:24.04

ARG FLOATCTF_VERSION=unknown

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        tzdata \
    && rm -rf /var/lib/apt/lists/*

COPY floatctf /usr/local/bin/floatctf
RUN chmod 0755 /usr/local/bin/floatctf \
    && mkdir -p /var/lib/floatctf/runtime \
    && chown 65532:65532 /var/lib/floatctf/runtime

WORKDIR /var/lib/floatctf/runtime
USER 65532:65532

LABEL io.floatctf.managed="true" \
      org.opencontainers.image.title="FloatCTF API" \
      org.opencontainers.image.version="${FLOATCTF_VERSION}"

STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/floatctf"]
API_DOCKERFILE_EOF
    chown -R root:"$FCTF_USER" "$FLOATCTF_HOME/image"
    chmod 750 "$FLOATCTF_HOME/image" "$FLOATCTF_HOME/image/api"
    ok "已写出 API runtime Dockerfile"
}
write_helper_systemd_unit() {
    mkdir -p /etc/systemd/system
    cat > /etc/systemd/system/floatctf-helper.service <<'HELPER_SVC_EOF'
[Unit]
Description=FloatCTF privileged host control plane
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=simple
User=floatctf-helper
Group=floatctf
SupplementaryGroups=docker
RuntimeDirectory=floatctf
RuntimeDirectoryMode=0750
ExecStart=/usr/local/libexec/floatctf-helper
# Type=simple only means the process was spawned; wait until both Unix sockets are
# actually bound so `systemctl start/restart` and dependent units have real readiness.
ExecStartPost=/bin/sh -c 'until [ -S /run/floatctf/helper-control.sock ] && [ -S /run/floatctf/helper-docker.sock ]; do sleep 0.05; done'
TimeoutStartSec=10s
Restart=on-failure
RestartSec=2
AmbientCapabilities=CAP_NET_ADMIN
CapabilityBoundingSet=CAP_NET_ADMIN
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
HELPER_SVC_EOF
}

write_systemd_units() {
    mkdir -p /etc/systemd/system
    write_helper_systemd_unit

    # 生产 API 已由 Compose 托管；移除旧 native API unit，避免双实例/端口竞争。
    systemctl disable floatctf-api.service >/dev/null 2>&1 || true
    systemctl stop floatctf-api.service >/dev/null 2>&1 || true
    rm -f /etc/systemd/system/floatctf-api.service

    cat > /etc/systemd/system/floatctf-infra.service <<'INFRA_SVC_EOF'
[Unit]
Description=FloatCTF production containers (API + data services + Caddy)
Requires=docker.service floatctf-helper.service
After=docker.service floatctf-helper.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=${FLOATCTF_HOME}
ExecStart=/usr/bin/docker compose -f ${FLOATCTF_HOME}/compose.prod.yml up -d --wait
ExecStop=/usr/bin/docker compose -f ${FLOATCTF_HOME}/compose.prod.yml down
TimeoutStartSec=300
TimeoutStopSec=120

[Install]
WantedBy=floatctf.target
INFRA_SVC_EOF
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" /etc/systemd/system/floatctf-infra.service

    cat > /etc/systemd/system/floatctf.target <<'TARGET_EOF'
[Unit]
Description=FloatCTF platform (host helper + production Compose stack)
Requires=floatctf-helper.service floatctf-infra.service
After=floatctf-helper.service floatctf-infra.service

[Install]
WantedBy=multi-user.target
TARGET_EOF

    ok "systemd 单元已写出（helper + Compose stack；native API unit 已清理）"
}

write_uninstall() {
    info "──── 写出卸载脚本 → $FLOATCTF_HOME/uninstall.sh ────"
    cat > "$FLOATCTF_HOME/uninstall.sh" <<'UNINSTALL_EOF'
#!/usr/bin/env bash
#
# FloatCTF uninstall (Phase 10.9) — 独立卸载脚本，可脱离源码签出运行.
#
# 本脚本安装到 /home/floatctf/uninstall.sh（由 scripts/install.sh 每次成功部署自动安装），
# 必须能在用户删除 Git 签出后独立工作：绝不依赖仓库相对路径 / scripts/install.sh /
# 源码 / mise / cargo / pnpm / git / chore / docs。仅依赖宿主既有工具：
#   systemctl, systemd, docker, docker compose, nft, iptables, ip, wg,
#   usermod/userdel, rm/install/find/cp/trap。
#
# 两个模式：
#   sudo /home/floatctf/uninstall.sh            SAFE UNINSTALL —— 移除可运行应用
#                                               （systemd、生产 Compose 容器/赛事资源、API image、
#                                               web 资产），但保留可恢复状态：
#                                               data/{postgres,rustfs,caddy,caddy-config}, config/, .env,
#                                               runtime/, logs/, .initialized, 本卸载脚本。
#                                               语义：deploy → safe uninstall → deploy 应恢复相同的
#                                               应用数据与密钥（用户/赛事/数据仍在）。
#   sudo /home/floatctf/uninstall.sh --purge    PERMANENT 删除全部 FloatCTF 自有数据
#                                               （PG/RustFS 数据、config、secrets、runtime、
#                                               日志、API image/build context、web、compose、systemd 单元、
#                                               动态赛事资源、sysctl/modules 文件、floatctf 用户、
#                                               /home/floatctf、本脚本自身）。需输入确认文本
#                                               "PURGE FLOATCTF"（除非 --yes）。
#
# 共享宿主依赖永不卸载：Docker / docker compose / nftables 包 / wireguard-tools /
# iproute2 / systemd。绝不触碰无关 Docker 对象 / WG 接口 / nftables 状态 / 路由 /
# libvirt / Incus / 其他应用。
#
set -Eeuo pipefail

FCTF_ROOT="${FLOATCTF_HOME:-${FCTF_ROOT:-/home/floatctf}}"
FCTF_USER="floatctf"
FCTF_HELPER_USER="floatctf-helper"
HELPER_INSTALL_PATH="/usr/local/libexec/floatctf-helper"

info() { printf '%s[INFO]%s %s\n'  "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n'  "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
warn() { printf '%s[WARN]%s %s\n'  "$(tput setaf 3 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n'  "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

# ── 根权限 ────────────────────────────────────────────────────────────────────
require_root() {
    [ "$(id -u)" -eq 0 ] || die "需要 root。请改用: sudo $FCTF_ROOT/uninstall.sh（或源码签出: sudo ./scripts/uninstall.sh）"
}

# ── 工具可用性（宿主既有；缺失则报错，不尝试安装）──────────────────────────────
MODE="safe"
PURGE_YES=0
SELF_TMP=""

usage() {
    cat <<'EOF'
用法：
  sudo /home/floatctf/uninstall.sh             安全卸载（保留 PG/RustFS 数据、config、secrets）
  sudo /home/floatctf/uninstall.sh --purge     永久删除全部 FloatCTF 自有数据（需确认 PURGE FLOATCTF）
  sudo /home/floatctf/uninstall.sh --purge --yes  跳过确认（仅限非交互 purge）
  sudo /home/floatctf/uninstall.sh --help
EOF
}

parse_args() {
    # 用 while 循环而非递归：递归版在参数耗尽时结尾返回非零，
    # 叠加 set -e 会导致脚本静默 exit 1、零输出（--purge 实测复发）。
    while [ "$#" -ge 1 ]; do
        case "$1" in
            --purge) MODE="purge";;
            --yes)   PURGE_YES=1;;
            -h|--help) usage; exit 0 ;;
            *) die "未知参数: $1（--help 查看用法）";;
        esac
        shift
    done
    return 0
}

# ── 自删除安全（§18）：purge 会把 /home/floatctf（含本脚本）删掉，Bash 不能继续读
#    已删除的自身文件。策略：把本脚本复制到 root 独有的 /tmp/floatctf-uninstall.<pid>.sh，
#    然后 exec 该临时副本续跑（临时副本自身设置 EXIT trap 删除自己，不留特权脚本在 /tmp）。
#    用环境变量 FCTF_UNINSTALL_CONT 标识内部续跑模式，避免无限递归。
INTERNAL_MODE_VAR="FCTF_UNINSTALL_CONT"
SELF_TMP=""

run_purge_via_temp() {
    if [ -n "${!INTERNAL_MODE_VAR:-}" ]; then
        # 已在临时副本内部续跑：让本副本负责清理自身（$0 == /tmp 副本），随后照常执行主体。
        SELF_TMP="$0"
        trap 'rm -f "$SELF_TMP"' EXIT INT TERM
        return 0
    fi
    # 原始进程：复制到 /tmp 后 exec（原进程被替换，其 trap 失效；临时副本接续清理）。
    local src="$0"
    [ -f "$src" ] || src="$FCTF_ROOT/uninstall.sh"
    SELF_TMP="/tmp/floatctf-uninstall.$$.sh"
    install -m 0700 "$src" "$SELF_TMP"
    local yesflag=()
    [ "$PURGE_YES" = "1" ] && yesflag+=(--yes)
    exec env "$INTERNAL_MODE_VAR=1" bash "$SELF_TMP" --purge "${yesflag[@]}"
}

# ============================================================================
# 动态 AWD / AWDP 资源清理（所有权严格限定：只按命名/Label 前缀匹配）
# ============================================================================
# FloatCTF 自有的命名契约（apps/api 源码与数据库确认）：
#   - 赛事 WireGuard 接口     : fawg_<8hex>
#   - 赛事 Docker 桥          : fctfawd<8hex>（docker 网络名 fctf-awd-<8hex>）
#   - 赛事 FlagServer 容器    : fctf-flagserver-<8hex>
#   - 赛事 JudgeServer 容器   : fctf-judgeserver-<8hex>
#   - 赛事 Docker 网络        : fctf-awd-<8hex>
#   - GameBox 容器            : 携带 awd.event_id / awd.resource_kind 标签
#   - 平台 control 网络       : fctf-platform-control
#   - AWDP Docker 网络        : fctf-awdp-practice / fctf-awdp-<12hex>
#     （旧版 fctf-awdp-control 仅作卸载兼容清理）
#   - AWDP JudgeServer 容器   : fctf-awdp-practice-judge / fctf-awdp-judge-<12hex>
#   - nftables 表             : inet floatctf_awd（全局）+ floatctf_awdp_*（AWDP）
#   - Docker 反欺骗放行规则   : 严格按 fawg_ 接口 / fctfawd 桥 / 赛事 CIDR 限定

AWK_NAME_OK='^[A-Za-z0-9_.-]+$'

require_tools() {
    local c
    for c in systemctl docker nft ip wg; do
        if ! command -v "$c" >/dev/null 2>&1; then
            warn "缺少命令: $c（跳过依赖它的清理步骤）"
        fi
    done
    command -v iptables >/dev/null 2>&1 || warn "缺少 iptables（跳过 Docker 反欺骗规则清理）"
}

# 名字是否为 FloatCTF 自有可安全删除的接证对象（拒绝通配/元字符）。
valid_fctf_name() { [[ "$1" =~ ${AWK_NAME_OK} ]] && [[ "$1" == fawg_* || "$1" == fctfawd* || "$1" == fctf-platform-control || "$1" == fctf-awd-* || "$1" == fctf-flagserver-* || "$1" == fctf-judgeserver-* || "$1" == fctf-awdp-* ]]; }

systemctl_stop_units() {
    info "── 停止并停用 FloatCTF systemd 单元 ──"
    # 宿主 systemd 未必在运行（容器/精简宿主）——不存在时按已停止处理。
    if ! command -v systemctl >/dev/null 2>&1; then
        warn "宿主无 systemctl，跳过 systemd 单元操作"
        return
    fi
    systemctl stop floatctf.target 2>/dev/null || true
    systemctl disable floatctf.target floatctf-infra.service floatctf-helper.service 2>/dev/null || true
    systemctl stop floatctf-helper.service floatctf-infra.service 2>/dev/null || true
    # 旧版 native API unit 只作迁移兼容清理。
    systemctl disable --now floatctf-api.service 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
    systemctl reset-failed floatctf.target floatctf-infra.service floatctf-helper.service floatctf-api.service 2>/dev/null || true
    ok "systemd 单元已停止/停用"
}

# 先移除生产 API 容器，从源头停止接受应用流量 + 停止 recover_all 重建动态资源。
stop_api_first() {
    if command -v docker >/dev/null 2>&1; then
        docker rm -f floatctf-api >/dev/null 2>&1 || true
    fi
    # 旧版 native API unit / binary 只作迁移兼容清理。
    if command -v systemctl >/dev/null 2>&1; then
        systemctl stop floatctf-api.service 2>/dev/null || true
    fi
    if [ -x "$FCTF_ROOT/bin/floatctf" ]; then
        pkill -f "^$FCTF_ROOT/bin/floatctf" 2>/dev/null || true
    fi
}

cleanup_gameboxes() {
    info "── 清理 GameBox 容器（依据 awd.* 标签，所有权限定）──"
    # 只删携带 FloatCTF awd 标签的容器；无标签者绝不触碰。
    local ids
    ids=$(docker ps -aq --filter 'label=awd.resource_kind' 2>/dev/null | tr '\n' ' ')
    [ -n "${ids// /}" ] || { ok "无 GameBox 容器"; return; }
    # 双重校验：每个 id 必须持有 awd.resource_kind 标签才删（防御竞态）。
    for id in $ids; do
        [ -n "$id" ] || continue
        [ -n "$(docker inspect -f '{{ index .Config.Labels "awd.resource_kind" }}' "$id" 2>/dev/null)" ] || { warn "容器 $id 无 awd.resource_kind 标签，跳过"; continue; }
        docker rm -f "$id" >/dev/null 2>&1 || warn "删除容器 $id 失败（忽略）"
        ok "已删除 GameBox 容器 $id"
    done
}

cleanup_awd_named_containers() {
    info "── 清理赛事 FlagServer/JudgeServer 与 AWDP JudgeServer 容器（精确名字前缀）──"
    local pat c x
    for c in fctf-flagserver- fctf-judgeserver- fctf-awdp-practice-judge fctf-awdp-judge-; do
        pat="${c}*"
        while read -r x; do
            [ -n "$x" ] || continue
            if [[ "$x" =~ ${AWK_NAME_OK} ]] && [[ "$x" == "$c"* ]]; then
                docker rm -f "$x" >/dev/null 2>&1 \
                    && ok "已删除容器 $x" || { [ -z "$(docker ps -aq --filter name="^${x}$" 2>/dev/null)" ] && ok "容器 $x 已不存在" || warn "删除容器 $x 失败（忽略）"; }
            else
                warn "容器名不匹配 FloatCTF 契约，跳过: $x"
            fi
        done < <(docker ps -a --filter "name=^$pat" --format '{{.Names}}' 2>/dev/null)
    done
    ok "赛事/AWDP 命名容器清理完成"
}

cleanup_docker_networks() {
    info "── 清理赛事 Docker 网络（名字前缀限定）──"
    local pat c x
    for c in fctf-platform-control fctf-awd- fctf-awdp-practice fctf-awdp-control fctf-awdp-; do
        pat="${c}*"
        while read -r x; do
            [ -n "$x" ] || continue
            if [[ "$x" =~ ${AWK_NAME_OK} ]] && [[ "$x" == "$c"* ]]; then
                # 网络可能仍有容器相连：先断开本平台容器再删。
                docker network rm "$x" >/dev/null 2>&1 \
                    && ok "已删除网络 $x" || warn "删除网络 $x 失败（可能仍有连接，忽略）"
            else
                warn "网络名不匹配 FloatCTF 契约，跳过: $x"
            fi
        done < <(docker network ls --format '{{.Name}}' 2>/dev/null | grep -E "^${c}[A-Za-z0-9_.-]*$")
    done
    ok "赛事 Docker 网络清理完成"
}

cleanup_wireguard() {
    info "── 清理赛事 WireGuard 接口（fawg_ 前缀限定）──"
    command -v ip >/dev/null 2>&1 || { warn "无 ip 命令，跳过 WG 接口清理"; return; }
    local iface
    for iface in $(ip -o link sh 2>/dev/null | awk -F': ' '{print $2}' | tr -d ' '); do
        [ -n "$iface" ] || continue
        [[ "$iface" =~ ^fawg_[0-9a-f]{8}$ ]] || continue
        ip link del "$iface" >/dev/null 2>&1 && ok "已删除 WG 接口 $iface" || warn "删除 WG 接口 $iface 失败（忽略）"
    done
    ok "赛事 WireGuard 接口清理完成（无关接口 wg0 等未触碰）"
}

# 删除 iptables 里 FloatCTF 自身的 Docker 反欺骗放行规则（严格按 fawg_ 接口限定）。
cleanup_iptables_docker_forward() {
    command -v iptables >/dev/null 2>&1 || { warn "无 iptables，跳过 Docker 反欺骗规则清理"; return; }
    info "── 清理 Docker 反欺骗放行规则（仅限 iifname=fawg_* 的 FloatCTF 规则）──"
    local line spec
    # raw PREROUTING 与 filter DOCKER-USER 中 -i fawg_*/fctfawd* 且 -j ACCEPT 的行
    for table in raw filter; do
        for chain in PREROUTING DOCKER-USER; do
            while read -r line; do
                [ -n "$line" ] || continue
                # 转成 -D 删除
                spec=${line#-A }
                [[ "$spec" == *" -i fawg_"* || "$spec" == *" -i fctfawd"* ]] || continue
                [[ "$spec" == *" -j ACCEPT" ]] || continue
                iptables -t "$table" -D $spec >/dev/null 2>&1 \
                    && ok "已删除规则 [$table $spec]" || warn "删除规则 [$table $spec] 失败（忽略）"
            done < <(iptables -t "$table" -S "$chain" 2>/dev/null | grep '^-A ' || true)
        done
    done
    ok "Docker 反欺骗放行规则清理完成"
}

cleanup_nftables() {
    info "── 清理 FloatCTF 自有 nftables 表（仅 floatctf_awd / floatctf_awdp_*）──"
    command -v nft >/dev/null 2>&1 || { warn "无 nft，跳过 nftables 清理"; return; }
    local fam table
    # nft list tables 输出形如 `table inet floatctf_awd`（每行一个）。
    while read -r fam table; do
        [ -n "$table" ] || continue
        case "$table" in
            floatctf_awd)
                nft delete table "$fam" "$table" >/dev/null 2>&1 && ok "已删除表 $fam $table" || warn "删除表 $fam $table 失败（忽略）"
                ;;
            floatctf_awdp_*)
                nft delete table "$fam" "$table" >/dev/null 2>&1 && ok "已删除表 $fam $table" || warn "删除表 $fam $table 失败（忽略）"
                ;;
            *) warn "非 FloatCTF 表，跳过: $fam $table" ;;
        esac
    done < <(nft list tables 2>/dev/null | sed -n 's/^table \([a-z]*\) \(.*\)$/\1 \2/p')
    ok "nftables 清理完成（未 flush ruleset，未触碰无关表）"
}

# ============================================================================
# 生产 Compose 容器（API / postgres / redis / rustfs / caddy）
# ============================================================================
stop_infra_containers() {
    info "── 停止/移除生产 Compose 容器（保护 bind-mount 数据）──"
    if [ -f "$FCTF_ROOT/compose.prod.yml" ] && [ -d "$FCTF_ROOT" ]; then
        # 用系统 docker compose 插件；无 -> 尝试 docker-compose。
        ( cd "$FCTF_ROOT" \
            && { docker compose -f compose.prod.yml down 2>/dev/null \
                 || docker compose -f compose.prod.yml stop 2>/dev/null \
                 || docker stop floatctf-api floatctf-postgres floatctf-redis floatctf-rustfs floatctf-caddy 2>/dev/null || true; } ) \
            && ok "生产容器已停止/移除（持久数据保留）"
    else
        warn "未找到 $FCTF_ROOT/compose.prod.yml，跳过 compose down；尝试按名字精确停止"
        docker stop floatctf-api floatctf-postgres floatctf-redis floatctf-rustfs floatctf-caddy 2>/dev/null || true
    fi
    # 兜底：强制移除（精确名字，防误删无关容器）
    local c
    for c in floatctf-api floatctf-postgres floatctf-redis floatctf-rustfs floatctf-caddy; do
        if [ -n "$(docker ps -aq --filter name="^${c}$" 2>/dev/null)" ]; then
            docker rm -f "$c" >/dev/null 2>&1 && ok "已移除容器 $c" || warn "移除容器 $c 失败（忽略）"
        fi
    done
}

remove_api_images() {
    info "── 移除 FloatCTF API runtime images ──"
    local id
    while read -r id; do
        [ -n "$id" ] || continue
        if [ "$(docker image inspect -f '{{ index .Config.Labels "io.floatctf.managed" }}' "$id" 2>/dev/null)" = "true" ]; then
            docker image rm -f "$id" >/dev/null 2>&1 || warn "删除 API image $id 失败（忽略）"
        else
            warn "floatctf/api image $id 无 managed label，跳过"
        fi
    done < <(docker image ls --filter 'reference=floatctf/api:*' -q 2>/dev/null | sort -u)
    ok "API runtime image 清理完成"
}

remove_application_artifacts() {
    info "── 移除可运行应用产物（保留 data/config/.env/runtime/logs）──"
    # image/ 是生产 API build context；bin/ 仅为旧版 native API 兼容清理。
    local p
    for p in "$FCTF_ROOT/image" "$FCTF_ROOT/bin" "$FCTF_ROOT/web" "$FCTF_ROOT/compose.dev.yml" "$FCTF_ROOT/compose.prod.yml" "$FCTF_ROOT/merged.sql" "$HELPER_INSTALL_PATH"; do
        if [ -e "$p" ] || [ -L "$p" ]; then
            rm -rf -- "$p" && ok "已移除 $p" || warn "移除 $p 失败（忽略）"
        fi
    done
}

# ============================================================================
# 主机初始化文件（purge 用）
# ============================================================================
SYSCTL_FILE="/etc/sysctl.d/99-floatctf.conf"
MODULES_FILE="/etc/modules-load.d/floatctf-br-netfilter.conf"

# ============================================================================
# SAFE UNINSTALL
# ============================================================================
safe_uninstall() {
    info "==== 安全卸载（保留可恢复状态）===="
    if [ ! -d "$FCTF_ROOT" ]; then
        info "$FCTF_ROOT 不存在 —— 已是未安装状态"
        ok "FloatCTF 已卸载（或从未安装）。"
        return
    fi

    # 1. 先停 API（停止接受流量 + 停止 recover_all 重建动态资源）
    stop_api_first
    # 2. 动态 AWD/AWDP 资源所有权清理
    require_tools
    cleanup_gameboxes
    cleanup_awd_named_containers
    cleanup_docker_networks
    cleanup_wireguard
    cleanup_iptables_docker_forward
    cleanup_nftables
    # 3. systemd（stop/disable/daemon-reload/reset-failed）
    systemctl_stop_units
    # 4. infra 容器（保数据）
    stop_infra_containers
    # 5. 移除可再生 API image 与应用产物
    remove_api_images
    remove_application_artifacts

    ok "FloatCTF 已卸载。"

    cat <<EOF

保留的数据（可恢复）:
  PostgreSQL 数据: $FCTF_ROOT/data/postgres
  RustFS 数据   : $FCTF_ROOT/data/rustfs
  Caddy 证书数据: $FCTF_ROOT/data/caddy
  配置/密钥      : $FCTF_ROOT/config 与 $FCTF_ROOT/.env
  运行时工作目录 : $FCTF_ROOT/runtime
  日志          : $FCTF_ROOT/logs

重新安装:
  运行 scripts/install.sh（会恢复相同数据与密钥，API 启动时自动重建 AWD 动态资源）。

完全删除:
  sudo $FCTF_ROOT/uninstall.sh --purge

本卸载脚本 $FCTF_ROOT/uninstall.sh 保留，作为生命周期/恢复工具（勿删）。
EOF
}

# ============================================================================
# PURGE
# ============================================================================
purge_confirm() {
    [ "$PURGE_YES" = "1" ] && return 0
    echo ""
    echo "!!!! 危险操作 !!!!"
    echo "你将永久删除全部 FloatCTF 自有数据，包括:"
    echo "  - PostgreSQL 数据        : $FCTF_ROOT/data/postgres"
    echo "  - RustFS 数据            : $FCTF_ROOT/data/rustfs"
    echo "  - Caddy 证书/账户状态    : $FCTF_ROOT/data/caddy"
    echo "  - 配置 / 密钥            : $FCTF_ROOT/config, $FCTF_ROOT/.env"
    echo "  - API image/build context / web / compose / runtime / 日志"
    echo "  - systemd 单元            floatctf-{helper,infra}.service, floatctf.target（含旧 api unit 兼容清理）"
    echo "  - 动态赛事资源            GameBox/FlagServer/JudgeServer 容器、赛事网络、"
    echo "                             WireGuard 接口、nftables 表、转发规则"
    echo "  - sysctl/modules 文件      /etc/sysctl.d/99-floatctf.conf,"
    echo "                              /etc/modules-load.d/floatctf-br-netfilter.conf"
    echo "  - floatctf / floatctf-helper 服务用户"
    echo "  - 安装根目录               $FCTF_ROOT（含本卸载脚本）"
    echo ""
    echo "此操作不可撤销。如要继续，请输入: PURGE FLOATCTF"
    read -r -p "> " ans || die "中断：未确认，已中止。"
    [ "$ans" = "PURGE FLOATCTF" ] || die "确认文本不匹配，已中止（未删除任何内容）。"
}

purge_remove_dynamic() {
    # 与 safe 阶段复用同一套所有权清理，确保浮动的赛事资源也被清除。
    require_tools
    cleanup_gameboxes
    cleanup_awd_named_containers
    cleanup_docker_networks
    cleanup_wireguard
    cleanup_iptables_docker_forward
    cleanup_nftables
}

purge_remove_systemd_units() {
    info "── 移除 FloatCTF systemd 单元 ──"
    if command -v systemctl >/dev/null 2>&1; then
        systemctl stop floatctf.target floatctf-api.service floatctf-helper.service floatctf-infra.service 2>/dev/null || true
        systemctl disable floatctf.target floatctf-api.service floatctf-helper.service floatctf-infra.service 2>/dev/null || true
        rm -f /etc/systemd/system/floatctf-api.service \
              /etc/systemd/system/floatctf-helper.service \
              /etc/systemd/system/floatctf-infra.service \
              /etc/systemd/system/floatctf.target
        systemctl daemon-reload 2>/dev/null || true
        systemctl reset-failed floatctf.target floatctf-infra.service floatctf-helper.service floatctf-api.service 2>/dev/null || true
        ok "FloatCTF systemd 单元已移除"
    else
        rm -f /etc/systemd/system/floatctf-api.service \
              /etc/systemd/system/floatctf-helper.service \
              /etc/systemd/system/floatctf-infra.service \
              /etc/systemd/system/floatctf.target
        ok "无 systemctl，直接移除单元文件"
    fi
}

purge_remove_sysctl_modules() {
    info "── 移除 FloatCTF 自有 sysctl / modules-load 文件 ──"
    # 仅删除 FloatCTF 自有文件片段；绝不整体关闭 IPv4 转发或 br_netfilter
    # （可能已被其他宿主负载依赖）。删除持久化文件后 reload 内核参数是可选的，
    # 这里不自动关闭任何内核特性，只清理持久化声明。
    local removed=0
    if [ -f "$SYSCTL_FILE" ]; then
        rm -f "$SYSCTL_FILE" && { removed=1; ok "已移除 $SYSCTL_FILE"; }
    fi
    if [ -f "$MODULES_FILE" ]; then
        rm -f "$MODULES_FILE" && { removed=1; ok "已移除 $MODULES_FILE"; }
    fi
    # 不自动 sysctl -w 关闭转发/br_netfilter：其他负载可能依赖；文档已说明此取舍。
    # 用 if 而非 `[ ... ] && ok`：removed=1 时后者返回非零，叠加 set -e 会在
    # 删除家目录/用户之前就退出（purge 实际观察到用户与 /home/floatctf 残留）。
    if [ "$removed" = "0" ]; then
        ok "无 FloatCTF sysctl/modules 文件（或已不存在）"
    fi
}

purge_remove_user() {
    info "── 移除 FloatCTF 服务用户 ──"
    if id "$FCTF_HELPER_USER" >/dev/null 2>&1; then
        userdel "$FCTF_HELPER_USER" 2>/dev/null \
            && ok "已移除用户 $FCTF_HELPER_USER" \
            || warn "userdel $FCTF_HELPER_USER 失败（可能仍有进程占用）"
    else
        ok "用户 $FCTF_HELPER_USER 不存在"
    fi

    if id "$FCTF_USER" >/dev/null 2>&1; then
        # 校验确为 FloatCTF 创建：家目录是该安装根、nologin、system 用户。
        local home shell
        home=$(getent passwd "$FCTF_USER" | cut -d: -f6)
        shell=$(getent passwd "$FCTF_USER" | cut -d: -f7)
        if [ "$home" = "$FCTF_ROOT" ] && [ "$shell" = "/usr/sbin/nologin" ]; then
            userdel -r "$FCTF_USER" 2>/dev/null && ok "已移除用户 $FCTF_USER" \
                || { warn "userdel $FCTF_USER 失败（可能仍有进程占用 /home/floatctf/runtime）"; \
                     # 回退：仅移除配置但保留记录，避免误删
                     warn "保留用户记录；请确认无 floatctf 进程后重试 userdel -r floatctf"; }
        else
            warn "账户 $FCTF_USER 不匹配预期（home=$home shell=$shell），跳过删除"
        fi
    else
        ok "用户 $FCTF_USER 不存在"
    fi

    if getent group "$FCTF_USER" >/dev/null 2>&1; then
        groupdel "$FCTF_USER" 2>/dev/null \
            && ok "已移除系统组 $FCTF_USER" \
            || warn "groupdel $FCTF_USER 失败（可能仍被账户使用）"
    fi
    # 绝不删除 docker 组或无关用户。
}

purge_run() {
    info "==== 永久删除（purge）===="
    purge_confirm

    # 先移除生产 API，阻止 recovery/scheduler 继续重建资源。
    stop_api_first
    # 动态赛事资源（所有权限定）
    purge_remove_dynamic
    # systemd 单元
    purge_remove_systemd_units
    # 生产容器（保数据到最后一刻：purge 会紧接着删除数据目录）
    stop_infra_containers
    remove_api_images
    # 主机初始化文件
    purge_remove_sysctl_modules
    # 删除安装根目录（含 PG/RustFS 数据、config、secrets、image、web、runtime、日志、
    # compose、.initialized、本脚本）。用临时文件夹承载 FCTF_ROOT 以彻底删除，随后遗留
    # 的空父目录不删（可能为系统原有 /home）。
    if [ -e "$FCTF_ROOT" ]; then
        rm -rf -- "$FCTF_ROOT" && ok "已删除安装根目录 $FCTF_ROOT" \
            || warn "删除 $FCTF_ROOT 部分失败（检查权限）"
    else
        ok "安装根目录 $FCTF_ROOT 已不存在"
    fi
    # 服务用户
    purge_remove_user

    ok "FloatCTF purge 完成。"
    echo ""
    echo "遗留检查："
    echo "  - 未曾触碰共享宿主依赖（Docker / compose / nftables 包 / WG 包 / iproute2 / systemd）。"
    echo "  - 未曾触碰无关 Docker 对象 / WG 接口 / nftables 规则 / 路由 / libvirt / Incus。"
    echo "  - 如需关闭 IPv4 转发 / br_netfilter，请手动评估（可能被其他负载依赖）。"
    echo ""
    echo "重新初始化并部署（全新安装）: "
    echo "  sudo ./scripts/install.sh"
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    parse_args "$@"
    # 无论哪种模式都必须 root；purge 在复制到 /tmp 前就校验，避免无谓复制。
    require_root

    if [ "$MODE" = "purge" ]; then
        # 自删除安全：在删除 /home/floatctf（含自身）之前，把脚本复制到 /tmp 免责续跑。
        # 续跑模式里该函数只设置 EXIT trap 并返回；否则 exec 已替换当前进程（不返回）。
        run_purge_via_temp
        purge_run
    else
        safe_uninstall
    fi
}

# 兼容两种调用方式（本文件被安装到 /home/floatctf/uninstall.sh，或从源码 ./scripts/uninstall.sh）
main "$@"
UNINSTALL_EOF
    # 固化 FLOATCTF_HOME 实际路径（把占位 ${FLOATCTF_HOME} 与默认 /home/floatctf 都替换）。
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/uninstall.sh"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/uninstall.sh"
    chmod 0750 "$FLOATCTF_HOME/uninstall.sh"
    if ! bash -n "$FLOATCTF_HOME/uninstall.sh" 2>/dev/null; then
        die "生成的 uninstall.sh 语法校验失败（bash -n）"
    fi
    ok "$FLOATCTF_HOME/uninstall.sh 已写出（root:$FCTF_USER 0750）"
}

# ============================================================================
# 第三阶段：部署
# ============================================================================
ENV_FILE="$FLOATCTF_HOME/.env"

env_get() { # key default
    local key="$1" def="${2:-}"
    if [ -n "${!key:-}" ]; then
        printf '%s' "${!key}"
    elif [ -f "$ENV_FILE" ] && grep -qE "^${key}=" "$ENV_FILE"; then
        sed -nE "s/^${key}=(.*)$/\1/p" "$ENV_FILE" | head -1
    else
        printf '%s' "$def"
    fi
}
env_set() { # key value
    if [ -f "$ENV_FILE" ] && grep -qE "^${1}=" "$ENV_FILE"; then
        sed -iE "s|^${1}=.*|${1}=${2}|" "$ENV_FILE"
    else
        printf '%s=%s\n' "$1" "$2" >> "$ENV_FILE"
    fi
}

precheck() {
    info "──── 部署：precheck ────"
    docker info >/dev/null 2>&1 || die "docker daemon 不可用"
    local pg_port redis_port rustfs_port rustfs_console_port http_port https_port
    pg_port=$(env_get POSTGRES_PORT 5433)
    redis_port=$(env_get REDIS_PORT 6380)
    rustfs_port=$(env_get RUSTFS_PORT 9000)
    rustfs_console_port=$(env_get RUSTFS_CONSOLE_PORT 9001)
    http_port=$(env_get HTTP_PORT 80)
    https_port=$(env_get HTTPS_PORT 443)
    info "宿主发布端口：PG=$pg_port Redis=$redis_port RustFS=$rustfs_port/$rustfs_console_port HTTP=$http_port HTTPS=$https_port；API_PORT 仅容器内部使用"
    for port_spec in "$pg_port" "$redis_port" "$rustfs_port" "$rustfs_console_port" "$http_port" "$https_port"; do
        if ss -ltn 2>/dev/null | awk '{print $4}' | grep -qE "[:.]${port_spec}$"; then
            local owned=0
            if docker ps --format '{{.Names}} {{.Ports}}' 2>/dev/null \
                | grep -qE "floatctf-(postgres|redis|rustfs|caddy).*[:.]${port_spec}"; then
                owned=1
            fi
            if [ "$owned" = "1" ]; then
                info "端口 $port_spec 由本平台容器占用（重部署，放行）"
            else
                die "端口 $port_spec 已被无关进程占用（ss 检查）；请调整 .env 端口"
            fi
        fi
    done
    ok "precheck 通过"
}

prepare_env() {
    info "──── 部署：配置（.env + floatctf.toml + Caddyfile）────"
    mkdir -p "$FLOATCTF_HOME/config/caddy" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" "$FLOATCTF_HOME/logs/api" "$FLOATCTF_HOME/logs/rustfs"
    local site_address fctf_uid fctf_gid
    site_address=$(env_get SITE_ADDRESS "")
    [ -n "$site_address" ] || die "生产部署必须设置 SITE_ADDRESS（例如 ctf.example.com），并将该域名 DNS 指向本机"
    fctf_uid="$(id -u "$FCTF_USER")"
    fctf_gid="$(id -g "$FCTF_USER")"
    if [ ! -f "$ENV_FILE" ]; then
        : > "$ENV_FILE"
        env_set POSTGRES_USER "${POSTGRES_USER:-postgres}"
        env_set POSTGRES_DB "${POSTGRES_DB:-floatctf_db}"
        env_set POSTGRES_PASSWORD "${POSTGRES_PASSWORD:-$(openssl rand -hex 16)}"
        env_set RUSTFS_ACCESS_KEY "${RUSTFS_ACCESS_KEY:-rustfsadmin}"
        env_set RUSTFS_SECRET_KEY "${RUSTFS_SECRET_KEY:-$(openssl rand -hex 24)}"
        env_set JWT_SECRET "${JWT_SECRET:-$(openssl rand -base64 32)}"
        env_set API_PORT "${API_PORT:-9090}"
        env_set POSTGRES_PORT "${POSTGRES_PORT:-5433}"
        env_set REDIS_PORT "${REDIS_PORT:-6380}"
        env_set RUSTFS_PORT "${RUSTFS_PORT:-9000}"
        env_set RUSTFS_CONSOLE_PORT "${RUSTFS_CONSOLE_PORT:-9001}"
        env_set HTTP_PORT "${HTTP_PORT:-80}"
        env_set HTTPS_PORT "${HTTPS_PORT:-443}"
        env_set HOST_ADDRESS "${HOST_ADDRESS:-127.0.0.1}"
        env_set SITE_ADDRESS "$site_address"
        chmod 600 "$ENV_FILE"
        ok ".env 已生成（含新密钥，root 可读）"
    else
        env_set API_PORT "$(env_get API_PORT 9090)"
        env_set POSTGRES_PORT "$(env_get POSTGRES_PORT 5433)"
        env_set REDIS_PORT "$(env_get REDIS_PORT 6380)"
        env_set RUSTFS_PORT "$(env_get RUSTFS_PORT 9000)"
        env_set RUSTFS_CONSOLE_PORT "$(env_get RUSTFS_CONSOLE_PORT 9001)"
        env_set HTTP_PORT "$(env_get HTTP_PORT 80)"
        env_set HTTPS_PORT "$(env_get HTTPS_PORT 443)"
        env_set HOST_ADDRESS "$(env_get HOST_ADDRESS 127.0.0.1)"
        env_set SITE_ADDRESS "$site_address"
        ok ".env 已存在，保留密钥并更新非敏感项"
    fi
    # Compose-only 部署元数据：应用自身仍只从 TOML 读取业务配置。
    env_set VERSION "$VERSION"
    env_set FLOATCTF_HOME "$FLOATCTF_HOME"
    env_set FLOATCTF_UID "$fctf_uid"
    env_set FLOATCTF_GID "$fctf_gid"
    chown root:"$FCTF_USER" "$ENV_FILE"
    chmod 640 "$ENV_FILE"
    chown -R "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/data" "$FLOATCTF_HOME/logs" "$FLOATCTF_HOME/runtime" 2>/dev/null || true
}

render() { # template out
    local tmpl="$1" out="$2"
    local vars
    vars=$(grep -oE '\$\{[A-Z_]+\}' "$tmpl" | tr -d '${}' | sort -u | paste -sd, -)
    [ -n "$vars" ] || vars="_NO_VARS_"
    local varspec=""
    IFS=',' read -r -a vararr <<< "$vars"
    local v
    for v in "${vararr[@]}"; do
        varspec="$varspec\${$v} "
    done
    envsubst "$varspec" < "$tmpl" > "$out.tmp" || die "envsubst 渲染失败: $tmpl"
    mv "$out.tmp" "$out"
}

prepare_configs() {
    set -a
    # ENV_FILE is generated at runtime by this installer.
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
    export FLOATCTF_HOME
    # 先替换模板里的 FLOATCTF_HOME 占位符，再渲染。
    sed "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/.floatctf.toml.tmpl" > "$FLOATCTF_HOME/.floatctf.toml.tmpl.real"
    render "$FLOATCTF_HOME/.floatctf.toml.tmpl.real" "$FLOATCTF_HOME/config/floatctf.toml"
    cp "$FLOATCTF_HOME/.Caddyfile.tmpl" "$FLOATCTF_HOME/config/caddy/Caddyfile"
    rm -f "$FLOATCTF_HOME/.floatctf.toml.tmpl.real"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy"
    chmod 750 "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/caddy/Caddyfile"
    chmod 640 "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/caddy/Caddyfile"
    chown -R "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" 2>/dev/null || true
    ok "配置已写入（floatctf.toml + Caddyfile；证书状态持久化在 data/caddy）"
}

validate_caddy_config() {
    info "──── 部署：验证 Caddyfile ────"
    docker run --rm \
        -e "SITE_ADDRESS=$(env_get SITE_ADDRESS)" \
        -e "HTTP_PORT=$(env_get HTTP_PORT 80)" \
        -e "HTTPS_PORT=$(env_get HTTPS_PORT 443)" \
        -e "API_PORT=$(env_get API_PORT 9090)" \
        -v "$FLOATCTF_HOME/config/caddy:/etc/caddy:ro" \
        caddy:2-alpine \
        caddy validate --config /etc/caddy/Caddyfile \
        || die "Caddyfile 验证失败"
    ok "Caddyfile 验证通过"
}

install_helper_binary() { # $1 = 已编译/download 的 helper 二进制
    local source="$1"
    [ -f "$source" ] || die "floatctf-helper 二进制不存在: $source"
    install -D -o root -g root -m 0755 "$source" "$HELPER_INSTALL_PATH"
    ok "floatctf-helper 已安装: $HELPER_INSTALL_PATH（root:root 0755）"
}

stage_release() {
    info "──── 部署：装配产物 → $FLOATCTF_HOME ────"
    install_helper_binary "$PKG_DIR/bin/floatctf-helper"

    write_api_dockerfile
    install -m 0755 "$PKG_DIR/bin/floatctf" "$FLOATCTF_HOME/image/api/floatctf"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/image/api/floatctf"
    docker build \
        --build-arg "FLOATCTF_VERSION=$VERSION" \
        -t "floatctf/api:$VERSION" \
        "$FLOATCTF_HOME/image/api" \
        || die "构建生产 API image 失败"
    ok "生产 API image 已构建: floatctf/api:$VERSION"

    mkdir -p "$FLOATCTF_HOME/web"
    find "$FLOATCTF_HOME/web" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + 2>/dev/null || true
    cp -a "$PKG_DIR/web/." "$FLOATCTF_HOME/web/"
    chown -R root:root "$FLOATCTF_HOME/web"
    install -m 0644 "$PKG_DIR/merged.sql" "$FLOATCTF_HOME/merged.sql"
    mkdir -p "$FLOATCTF_HOME/runtime"
    chown "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/runtime"
    fix_infra_ownership
    ok "产物装配完成（API image + helper + web + merged.sql）"
}

ensure_platform_control_network() {
    local name="fctf-platform-control"
    local legacy_name="fctf-awdp-control"

    # 旧版 AWDP control network 使用了同一 10.42.8.0/24。Docker 不允许两个 bridge
    # network 占用相同 subnet，因此升级时仅在旧网络已空闲且确属 FloatCTF 时自动迁移。
    if ! docker network inspect "$name" >/dev/null 2>&1 \
        && docker network inspect "$legacy_name" >/dev/null 2>&1; then
        local legacy_internal legacy_subnet legacy_containers legacy_managed legacy_managed_old
        legacy_internal="$(docker network inspect -f '{{.Internal}}' "$legacy_name")"
        legacy_subnet="$(docker network inspect -f '{{range .IPAM.Config}}{{.Subnet}}{{end}}' "$legacy_name")"
        legacy_containers="$(docker network inspect -f '{{len .Containers}}' "$legacy_name")"
        legacy_managed="$(docker network inspect -f '{{index .Labels "io.floatctf.managed"}}' "$legacy_name")"
        legacy_managed_old="$(docker network inspect -f '{{index .Labels "floatctf.managed"}}' "$legacy_name")"
        [ "$legacy_internal" = "true" ] \
            && [ "$legacy_subnet" = "10.42.8.0/24" ] \
            && { [ "$legacy_managed" = "true" ] || [ "$legacy_managed_old" = "true" ]; } \
            || die "$legacy_name 占用 10.42.8.0/24 但不匹配 FloatCTF 旧 control network 契约，请人工处理"
        [ "$legacy_containers" = "0" ] \
            || die "$legacy_name 仍连接 $legacy_containers 个容器；请先停止旧 AWDP Judge 后重新部署"
        docker network rm "$legacy_name" >/dev/null \
            || die "移除旧 control network 失败: $legacy_name"
        ok "已迁移旧 control network 名称: $legacy_name → $name"
    fi

    if docker network inspect "$name" >/dev/null 2>&1; then
        local internal subnet occupied
        internal="$(docker network inspect -f '{{.Internal}}' "$name")"
        subnet="$(docker network inspect -f '{{range .IPAM.Config}}{{.Subnet}}{{end}}' "$name")"
        [ "$internal" = "true" ] || die "$name 已存在但不是 internal network"
        [ "$subnet" = "10.42.8.0/24" ] || die "$name 已存在但 subnet=$subnet，期望 10.42.8.0/24"
        occupied="$(docker network inspect -f '{{range .Containers}}{{.IPv4Address}} {{end}}' "$name")"
        if printf '%s\n' "$occupied" | grep -qE '(^|[[:space:]])10\.42\.8\.2/'; then
            local api_attached
            api_attached="$(docker network inspect -f '{{range .Containers}}{{if eq .Name "floatctf-api"}}{{.IPv4Address}}{{end}}{{end}}' "$name")"
            [ "$api_attached" = "10.42.8.2/24" ] || die "$name 的 10.42.8.2 已被非 floatctf-api 容器占用"
        fi
        ok "平台 control network 已存在: $name"
        return
    fi
    docker network create \
        --driver bridge \
        --internal \
        --subnet 10.42.8.0/24 \
        --ip-range 10.42.8.128/25 \
        --label io.floatctf.managed=true \
        "$name" >/dev/null \
        || die "创建平台 control network 失败: $name"
    ok "平台 control network 已创建: $name (10.42.8.0/24, internal)"
}

validate_compose_config() {
    info "──── 部署：验证 compose.prod.yml ────"
    docker compose --env-file "$ENV_FILE" -f "$FLOATCTF_HOME/compose.prod.yml" config >/dev/null \
        || die "compose.prod.yml 验证失败"
    ok "compose.prod.yml 验证通过"
}

# 基础设施目录属主修正（生产/开发共用）：
# - rustfs（uid 10001）：另授 floatctf 组读 + 目录 setgid——rustfs 写入文件默认
#   0644/0755（other 可读），组读把「宿主用户可读」从镜像默认行为升级为显式
#   保证（未来 rustfs 收紧默认 mode 也不受影响）；容器写入 uid 不变，零迁移。
# - postgres（uid 999）：数据目录归其所有（首次启动 initdb 写入）。
# - redis（uid 999，dev bind mount）：同 postgres。
# - caddy：容器以 root 运行，属主仅归 floatctf 便于管理（同生产 prepare_configs）。
# install.sh 不启动容器，直接 chown 即可（不触碰运行中的容器）。
fix_infra_ownership() {
    chown -R 10001:10001 "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    chgrp -R "$FCTF_USER" "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    chmod -R g+rX "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    find "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" -type d -exec chmod g+s {} + 2>/dev/null || true
    chown -R 999:999 "$FLOATCTF_HOME/data/postgres" "$FLOATCTF_HOME/data/redis" 2>/dev/null || true
    chown -R "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" 2>/dev/null || true
}

install_systemd() {
    info "──── 部署：systemd 单元（helper / Compose stack / target）────"
    systemctl daemon-reload
    systemctl enable floatctf.target floatctf-infra.service floatctf-helper.service
    ok "systemd 单元已写出并 enable（未启动；用 systemctl start floatctf.target 启动）"
}

run_deploy() {
    info "──── 第三阶段：部署（写文件/镜像/网络 + 建服务，不启动容器）→ $FLOATCTF_HOME ────"
    precheck
    prepare_env
    write_compose_prod
    write_config_template
    write_caddy_template
    prepare_configs
    validate_caddy_config
    stage_release
    ensure_platform_control_network
    validate_compose_config
    write_systemd_units
    install_systemd
    write_uninstall
    ok "部署完成（服务未启动）：$FLOATCTF_HOME；生产 API 将由 Compose 以非 root/无 capabilities 容器运行"
}

# ============================================================================
# 开发宿主初始化（--develop）：仅由 `mise run setup` 内部调用。
# 开发环境自身由仓库内 mise + compose 管理；这里仅准备一次性的宿主能力与 helper。
# ============================================================================
run_develop() {
    info "════ 开发宿主初始化（mise run setup）════"

    local src_root developer
    src_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    [ -f "$src_root/Cargo.toml" ] && [ -d "$src_root/apps" ] && [ -d "$src_root/infra" ] \
        || die "未检测到 FloatCTF 源码根目录"
    [ -n "$HELPER_BIN" ] || die "开发初始化缺少 --helper-bin；请通过 `mise run setup` 执行"
    [ -x "$HELPER_BIN" ] || die "helper 二进制不可执行: $HELPER_BIN"

    run_init develop

    developer="${SUDO_USER:-}"
    [ -n "$developer" ] && id "$developer" >/dev/null 2>&1 \
        || die "开发初始化需要通过 sudo 从普通开发者会话执行"

    # 开发者保留 docker 组用于仓库 Compose；API 子进程会显式丢弃该组。
    # floatctf 组用于访问 helper 两个 Unix socket。
    for group in docker "$FCTF_USER"; do
        if getent group "$group" >/dev/null 2>&1 \
            && ! id -nG "$developer" | tr ' ' '\n' | grep -qx "$group"; then
            usermod -aG "$group" "$developer"
            ok "已把 $developer 加入 $group 组（重新登录后生效）"
        fi
    done

    install_helper_binary "$HELPER_BIN"
    write_helper_systemd_unit

    # 清理历史双轨开发单元；开发基础设施今后只由 `mise run dev` 管理。
    systemctl disable --now floatctf-dev-infra.service >/dev/null 2>&1 || true
    rm -f /etc/systemd/system/floatctf-dev-infra.service
    systemctl daemon-reload
    systemctl enable --now floatctf-helper.service

    # 等待 socket 创建，确保 setup 在控制面真正可用后才成功返回。
    local _attempt
    for _attempt in $(seq 1 50); do
        [ -S /run/floatctf/helper-control.sock ] && [ -S /run/floatctf/helper-docker.sock ] && break
        sleep 0.1
    done
    [ -S /run/floatctf/helper-control.sock ] && [ -S /run/floatctf/helper-docker.sock ] \
        || die "floatctf-helper 已启动但 sockets 未出现；请查看 journalctl -u floatctf-helper"

    cat <<EOF

开发宿主初始化完成。
- API / Vite 由当前开发者用户运行。
- floatctf-helper 由 systemd 以 $FCTF_HELPER_USER + docker 组 + CAP_NET_ADMIN 运行。
- 开发基础设施由仓库内 Docker Compose 管理。
- 首次加入 docker/floatctf 组后请重新登录，再执行：mise run dev
EOF
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    if [ "$DEVELOP" = "1" ]; then
        run_develop
        ok "开发环境初始化完成（helper 已启动；API/Web 尚未启动）"
        return
    fi
    info "FloatCTF 一键安装 → $FLOATCTF_HOME"
    run_init
    acquire_package
    run_deploy
    cat <<EOF

安装完成（未启动任何服务）。启动整平台：
  sudo systemctl start floatctf.target
（首次启动 postgres 会自动用 merged.sql 初始化数据库）
查看状态：
  systemctl status floatctf.target
  journalctl -fu floatctf-infra floatctf-api
EOF
    ok "FloatCTF 安装完成：$FLOATCTF_HOME"
}

main
