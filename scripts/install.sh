#!/usr/bin/env bash
#
# FloatCTF 一键安装（Phase 11）— 单文件自包含安装器。
#
# 本脚本不依赖仓库其他文件：所有模板（compose.dev/prod、floatctf.toml、nginx.conf、
# systemd 单元、uninstall.sh）都内嵌在本文件里，运行时写出到 FLOATCTF_HOME。
#
# 语义（仅全新安装；升级迁移后续单独做）：
#   1. 主机初始化（幂等）：发行版检测/装缺失主机包、docker/nftables/WireGuard 能力检查、
#      IPv4 转发 + br_netfilter、floatctf 服务用户 + docker 组 + FLOATCTF_HOME 布局。
#      已存在/已做过 → 逐项跳过（不依赖 .initialized 单点标记）。
#   2. 下载 3 个 release 产物：
#        - API 二进制（bin/floatctf）
#        - 前端静态产物（web dist，tar.gz）
#        - merged.sql（单个 SQL，fresh-DB bootstrap）
#   3. 部署：渲染配置 → 装配 bin/web/compose → 起 infra(--wait) → psql 初始化 merged.sql
#      → 装 systemd 单元 → 启动 API → 生成 uninstall.sh。
#
# 用法：
#   sudo ./install.sh                          # 用默认（fake）3 个 URL，完整安装
#   sudo ./install.sh --api-url <url> --web-url <url> --migrate-url <url>
#   sudo ./install.sh --skip-download          # 跳过下载，改用本地 release/floatctf-* 产物
#   FLOATCTF_HOME=/opt/floatctf sudo ./install.sh   # 自定义安装根（默认 /home/floatctf）
#
# 环境变量（也可覆盖 URL）：
#   FLOATCTF_API_URL / FLOATCTF_WEB_URL / FLOATCTF_MIGRATE_URL
#
# 注意：
#   - 只做全新安装；已有数据的升级（forward-only 迁移）后续单独实现。
#   - AWD 服务镜像（floatctf/awd-flagserver / awd-judgeserver）暂不在本脚本构建
#     （TODO Phase 11.1：registry 拉取或本地 docker build）。
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLOATCTF_HOME="${FLOATCTF_HOME:-/home/floatctf}"
FCTF_USER="floatctf"

# fake 占位地址：真实 release 地址发布后替换（或经 --*-url / 环境变量覆盖）。
DEFAULT_API_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/floatctf"
DEFAULT_WEB_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/web-dist.tar.gz"
DEFAULT_MIGRATE_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/merged.sql"

# 本地 release 产物目录（--skip-download 时从这里读）。
LOCAL_PKG_DIR="${FLOATCTF_PKG_DIR:-$REPO_ROOT/release}"

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
WEB_URL="$DEFAULT_WEB_URL"
MIGRATE_URL="$DEFAULT_MIGRATE_URL"
SKIP_DOWNLOAD=0
while [ $# -gt 0 ]; do
    case "$1" in
        --api-url) API_URL="${2:?--api-url 需要一个地址参数}"; shift ;;
        --web-url) WEB_URL="${2:?--web-url 需要一个地址参数}"; shift ;;
        --migrate-url) MIGRATE_URL="${2:?--migrate-url 需要一个地址参数}"; shift ;;
        --skip-download) SKIP_DOWNLOAD=1 ;;
        -h|--help)
            sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) die "未知参数: $1（--help 查看用法）" ;;
    esac
    shift
done
# 环境变量覆盖（低于 --*-url 显式传参）。
API_URL="${FLOATCTF_API_URL:-$API_URL}"
WEB_URL="${FLOATCTF_WEB_URL:-$WEB_URL}"
MIGRATE_URL="${FLOATCTF_MIGRATE_URL:-$MIGRATE_URL}"

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

ARCH_PKGS=(docker docker-compose nftables wireguard-tools iproute2 procps-ng openssl curl tar)

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
    for c in ip wg nft docker sysctl modprobe; do
        command -v "$c" >/dev/null 2>&1 || die "缺少命令: $c"
    done
    ok "基础命令齐全（ip/wg/nft/docker/sysctl/modprobe）"
}

check_docker() {
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

check_user_layout() {
    if ! id "$FCTF_USER" >/dev/null 2>&1; then
        useradd --system --home-dir "$FLOATCTF_HOME" --shell /usr/sbin/nologin "$FCTF_USER"
        ok "已创建系统用户 $FCTF_USER"
    else
        ok "服务用户 $FCTF_USER 存在"
    fi

    if getent group docker >/dev/null 2>&1; then
        if id -nG "$FCTF_USER" 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
            ok "$FCTF_USER 已在 docker 组"
        else
            usermod -aG docker "$FCTF_USER"
            ok "已把 $FCTF_USER 加入 docker 组"
        fi
    else
        warn "宿主无 docker 组（docker 未安装？后续 API 无法操作容器）"
    fi

    if id "$FCTF_USER" >/dev/null 2>&1 && getent group docker >/dev/null 2>&1 \
        && id -nG "$FCTF_USER" | tr ' ' '\n' | grep -qx docker; then
        if runuser -u "$FCTF_USER" -- docker info >/dev/null 2>&1; then
            ok "runuser 验证：$FCTF_USER 可访问 docker daemon"
        else
            warn "runuser 验证失败：$FCTF_USER 无法访问 docker daemon（daemon 未就绪或 socket 权限）"
        fi
    fi

    local d
    for d in bin web config/nginx data/postgres data/rustfs logs/api logs/nginx logs/rustfs runtime gameboxes; do
        mkdir -p "$FLOATCTF_HOME/$d"
    done
    chown root:"$FCTF_USER" "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    local run_dir
    for run_dir in bin web data logs runtime gameboxes; do
        chown -R "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/$run_dir" >/dev/null 2>&1 || true
    done
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/nginx" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/config" >/dev/null 2>&1 || true
    ok "布局就绪: $FLOATCTF_HOME/{bin,web,config/nginx,data/{postgres,rustfs},logs/{api,nginx,rustfs},runtime,gameboxes}"

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
    info "──── 第一阶段：主机初始化（幂等）────"
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
    check_user_layout
    ok "主机初始化完成（docker/nftables/WireGuard/转发/br_netfilter/用户/布局 就绪）"
}

# ============================================================================
# 第二阶段：获取 release 产物（3 个 URL，或 --skip-download 用本地产物）
# ============================================================================
download_url() { # url dest
    info "下载: $1"
    curl -fL --retry 3 --connect-timeout 30 -o "$2" "$1" \
        || die "下载失败: $1（这是 fake 占位地址，替换为真实 release 地址或经 --*-url 传入）"
}

download_release() {
    info "──── 第二阶段：下载 release 产物（3 URL）────"
    TMP_STAGE_DIR="$(mktemp -d /tmp/floatctf-install.XXXXXX)"

    # 1) API 二进制
    mkdir -p "$TMP_STAGE_DIR/bin"
    download_url "$API_URL" "$TMP_STAGE_DIR/bin/floatctf"
    chmod 0755 "$TMP_STAGE_DIR/bin/floatctf"

    # 2) 前端静态产物（tar.gz）
    download_url "$WEB_URL" "$TMP_STAGE_DIR/web-dist.tar.gz"
    mkdir -p "$TMP_STAGE_DIR/web"
    tar xzf "$TMP_STAGE_DIR/web-dist.tar.gz" -C "$TMP_STAGE_DIR/web" \
        || die "解压 web-dist 失败"

    # 3) merged.sql
    download_url "$MIGRATE_URL" "$TMP_STAGE_DIR/merged.sql"

    ok "release 产物就绪: $TMP_STAGE_DIR"
    PKG_DIR="$TMP_STAGE_DIR"
}

# --skip-download：从本地 release/floatctf-* 目录读产物。
locate_local_package() {
    local pkg_dir
    pkg_dir="$(ls -d "$LOCAL_PKG_DIR"/floatctf-* 2>/dev/null | sort -V | tail -1 || true)"
    [ -n "$pkg_dir" ] || die "本地 release 产物不存在（$LOCAL_PKG_DIR/floatctf-*）。请先准备本地产物，或去掉 --skip-download 改为下载。"
    [ -f "$pkg_dir/bin/floatctf" ] || die "本地产物缺少 bin/floatctf: $pkg_dir"
    [ -d "$pkg_dir/web" ] || die "本地产物缺少 web/: $pkg_dir"
    [ -f "$pkg_dir/merged.sql" ] || die "本地产物缺少 merged.sql: $pkg_dir"
    ok "本地 release 产物就绪: $pkg_dir"
    PKG_DIR="$pkg_dir"
}

acquire_package() {
    if [ "$SKIP_DOWNLOAD" = "1" ]; then
        info "──── 第二阶段：使用本地 release 产物（--skip-download）────"
        locate_local_package
    else
        download_release
    fi
}

# ============================================================================
# 内嵌模板（写出到 FLOATCTF_HOME，占位符 ${FLOATCTF_HOME} 替换为实际值）
# ============================================================================
write_compose_dev() {
    cat > "$FLOATCTF_HOME/compose.dev.yml" <<'COMPOSE_DEV_EOF'
volumes:
    floatctf-db-data:
      name: floatctf-dev-db-data
    floatctf-rustfs-data:
      name: floatctf-dev-rustfs-data

services:
    db:
        container_name: floatctf-dev-db
        image: postgres:17
        ports:
            - "5432:5432"
        environment:
            POSTGRES_USER: postgres
            POSTGRES_PASSWORD: postgres
            POSTGRES_DB: floatctf_db
        volumes:
            - floatctf-db-data:/var/lib/postgresql/data
            - ${PROJECT_ROOT}/apps/api/src/sql/merged.sql:/docker-entrypoint-initdb.d/00-init.sql:ro
        restart: unless-stopped

    nginx:
        depends_on:
            - db
        container_name: floatctf-dev-nginx
        image: nginx:1.26-bookworm
        ports:
            - "7780:80"
        volumes:
            - ${PROJECT_ROOT}/app/logs/nginx:/var/log/nginx
            - ${PROJECT_ROOT}/app/api/challenges:/app/api/challenges:ro
            - ${PROJECT_ROOT}/app/api/uploads:/app/api/uploads:ro
            - ${PROJECT_ROOT}/app/api/weapons:/app/api/weapons:ro
            - ${PROJECT_ROOT}/app/api/images:/app/api/images:ro
            - ${PROJECT_ROOT}/infra/nginx/nginx.dev.conf:/etc/nginx/nginx.conf:ro
        extra_hosts:
            - "host.docker.internal:host-gateway"
        restart: unless-stopped

    rustfs:
        container_name: floatctf-dev-rustfs
        image: rustfs/rustfs:latest
        volumes:
            - floatctf-rustfs-data:/data
            - ${PROJECT_ROOT}/app/logs/rustfs:/logs
        ports:
          - "127.0.0.1:9000:9000"
          - "127.0.0.1:9001:9001"
        environment:
            RUSTFS_ADDRESS: ":9000"
            RUSTFS_SERVER_DOMAINS: example.com
            RUSTFS_ACCESS_KEY: rustfsadmin
            RUSTFS_SECRET_KEY: rustfsadmin
            RUSTFS_CONSOLE_ENABLE: "true"
            RUSTFS_OBS_LOG_DIRECTORY: /logs
COMPOSE_DEV_EOF
    ok "已写出 compose.dev.yml"
}

write_compose_prod() {
    cat > "$FLOATCTF_HOME/compose.prod.yml" <<'COMPOSE_PROD_EOF'
# FloatCTF production infrastructure compose.
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
        healthcheck:
            test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER:-postgres} -d ${POSTGRES_DB:-floatctf_db}"]
            interval: 5s
            timeout: 3s
            retries: 10
            start_period: 10s
        # 禁止：公开端口、自动 initdb 迁移（迁移由 install.sh 显式执行 merged.sql）

    rustfs:
        image: rustfs/rustfs:latest
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
            RUSTFS_SERVER_DOMAINS: example.com
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

    nginx:
        image: nginx:1.26-bookworm
        container_name: floatctf-nginx
        restart: unless-stopped
        network_mode: host
        volumes:
            - ${FLOATCTF_HOME}/config/nginx/nginx.conf:/etc/nginx/nginx.conf:ro
            - ${FLOATCTF_HOME}/web:/usr/share/nginx/html:ro
            - ${FLOATCTF_HOME}/runtime/challenges:/app/api/challenges:ro
            - ${FLOATCTF_HOME}/logs/nginx:/var/log/nginx
        depends_on:
            postgres:
                condition: service_healthy
            rustfs:
                condition: service_healthy
        healthcheck:
            test: ["CMD-SHELL", "curl -fsS http://127.0.0.1:${HTTP_PORT:-80}/ >/dev/null || exit 1"]
            interval: 10s
            timeout: 3s
            retries: 10
            start_period: 5s
COMPOSE_PROD_EOF
    # 占位符替换为实际 FLOATCTF_HOME。
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/compose.prod.yml"
    ok "已写出 compose.prod.yml"
}

write_config_template() {
    cat > "$FLOATCTF_HOME/.floatctf.toml.tmpl" <<'CONFIG_TMPL_EOF'
[application]
main_url = "http://${HOST_ADDRESS}:${HTTP_PORT}"

[server]
listen_ip = "0.0.0.0"
listen_port = ${API_PORT}
work_dir = "${FLOATCTF_HOME}/runtime"

[auth]
jwt_secret = "${JWT_SECRET}"

[database]
url = "postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"

[logging]
filter = "actix_server=info,floatctf=info,fcmc=info"
timezone = "Asia/Shanghai"

[rustfs]
region = "cn-east-1"
endpoint_url = "http://127.0.0.1:${RUSTFS_PORT}"
access_key_id = "${RUSTFS_ACCESS_KEY}"
secret_access_key = "${RUSTFS_SECRET_KEY}"

[features]
web_terminal = true
unsafe_sql_admin = true

[awd]
network_runtime = "host"
flagserver_image = "floatctf/awd-flagserver:${VERSION}"
judgeserver_image = "floatctf/awd-judgeserver:${VERSION}"

[awdp]
practice_judgeserver_image = "floatctf/infra/awdp-judgeserver:latest"
practice_network_subnet = "10.42.2.0/23"
practice_judge_ip = "10.42.2.2"
network_pool = "10.43.0.0/16"
event_netmask = 24
platform_internal_url = "http://${HOST_ADDRESS}:${API_PORT}"

[registry]
image_prefix = "floatctf"
push = false
insecure = false
build_timeout_secs = 600
CONFIG_TMPL_EOF
    ok "已写出 config 模板"
}

write_nginx_template() {
    cat > "$FLOATCTF_HOME/.nginx.conf.tmpl" <<'NGINX_TMPL_EOF'
user nginx;
worker_processes auto;
worker_rlimit_nofile 100000;
pid /var/run/nginx.pid;

events {
    worker_connections 4096;
    multi_accept on;
}

http {
    include /etc/nginx/mime.types;
    default_type application/octet-stream;
    charset utf-8;

    upstream api_backend {
        server 127.0.0.1:${API_PORT};
        keepalive 64;
    }

    map $http_upgrade $connection_upgrade {
        default upgrade;
        ''      "";
    }

    log_format main
        '$remote_addr - $remote_user [$time_local] '
        '"$request" $status $body_bytes_sent '
        '"$http_referer" "$http_user_agent" '
        'upstream="$upstream_addr" '
        'request_time=$request_time '
        'upstream_time=$upstream_response_time';

    access_log /var/log/nginx/access.log main;
    error_log /var/log/nginx/error.log info;

    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    keepalive_timeout 65s;
    keepalive_requests 10000;
    reset_timedout_connection on;
    server_tokens off;

    gzip on;
    gzip_vary on;
    gzip_proxied any;
    gzip_min_length 1024;
    gzip_comp_level 6;
    gzip_buffers 16 8k;
    gzip_http_version 1.1;
    gzip_types
        text/plain
        text/css
        text/xml
        application/json
        application/javascript
        application/xml
        application/xml+rss
        application/wasm
        image/svg+xml;

    server {
        listen ${HTTP_PORT};
        listen [::]:${HTTP_PORT};
        server_name _;

        client_max_body_size 0;
        client_body_buffer_size 1m;

        proxy_connect_timeout 10s;
        proxy_send_timeout 300s;
        proxy_read_timeout 300s;
        proxy_buffering off;
        proxy_request_buffering off;

        location ^~ /api/ {
            proxy_pass http://api_backend;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection $connection_upgrade;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Host $host;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-Port $server_port;
            proxy_cache_bypass $http_upgrade;
        }

        location ^~ /public/ {
            rewrite ^/public/(.*)$ /floatctf-public/$1 break;
            proxy_pass http://127.0.0.1:${RUSTFS_PORT};
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Host $http_host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }

        location ^~ /private/ {
            rewrite ^/private/(.*)$ /floatctf-private/$1 break;
            proxy_pass http://127.0.0.1:${RUSTFS_PORT};
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Host 127.0.0.1:${RUSTFS_PORT};
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }

        location ~ ^/static/challenges/([^/]+)/attachment/(.+)$ {
            alias /app/api/challenges/$1/attachment/$2;
            try_files $uri =404;
            add_header X-Content-Type-Options nosniff always;
            add_header Content-Disposition 'attachment' always;
        }

        location / {
            root /usr/share/nginx/html;
            index index.html;
            try_files $uri $uri/ /index.html;
        }
    }
}
NGINX_TMPL_EOF
    ok "已写出 nginx 模板"
}

write_systemd_units() {
    mkdir -p /etc/systemd/system

    cat > /etc/systemd/system/floatctf-api.service <<'API_SVC_EOF'
[Unit]
Description=FloatCTF API
Requires=floatctf-infra.service
After=floatctf-infra.service network-online.target
Wants=network-online.target

[Service]
Type=simple
User=floatctf
Group=floatctf
SupplementaryGroups=docker
WorkingDirectory=${FLOATCTF_HOME}/runtime
EnvironmentFile=${FLOATCTF_HOME}/.env
Environment=FLOATCTF_CONFIG=${FLOATCTF_HOME}/config/floatctf.toml
ExecStart=${FLOATCTF_HOME}/bin/floatctf
Restart=on-failure
RestartSec=5
AmbientCapabilities=CAP_NET_ADMIN
CapabilityBoundingSet=CAP_NET_ADMIN
NoNewPrivileges=no

[Install]
WantedBy=floatctf.target
API_SVC_EOF
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" /etc/systemd/system/floatctf-api.service

    cat > /etc/systemd/system/floatctf-infra.service <<'INFRA_SVC_EOF'
[Unit]
Description=FloatCTF infrastructure containers (postgres, rustfs, nginx)
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=${FLOATCTF_HOME}
ExecStart=/usr/bin/docker compose -f ${FLOATCTF_HOME}/compose.prod.yml up -d --wait
ExecStop=/usr/bin/docker compose -f ${FLOATCTF_HOME}/compose.prod.yml down
TimeoutStartSec=300

[Install]
WantedBy=floatctf.target
INFRA_SVC_EOF
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" /etc/systemd/system/floatctf-infra.service

    cat > /etc/systemd/system/floatctf.target <<'TARGET_EOF'
[Unit]
Description=FloatCTF platform (infra + api)
Requires=floatctf-infra.service floatctf-api.service
After=floatctf-infra.service floatctf-api.service

[Install]
WantedBy=multi-user.target
TARGET_EOF

    ok "systemd 单元已写出"
}

# 生成独立卸载脚本到 FLOATCTF_HOME/uninstall.sh（内嵌全文，脱仓库可独立运行）。
write_uninstall() {
    info "──── 写出卸载脚本 → $FLOATCTF_HOME/uninstall.sh ────"
    cat > "$FLOATCTF_HOME/uninstall.sh" <<'UNINSTALL_EOF'
#!/usr/bin/env bash
#
# FloatCTF uninstall — 独立卸载脚本，可脱离源码签出运行.
#
# 两个模式：
#   sudo ${FLOATCTF_HOME}/uninstall.sh            SAFE UNINSTALL —— 移除可运行应用
#                                               （systemd、infra/赛事容器与网络、API 二进制、
#                                               web 资产），但保留可恢复状态：
#                                               data/{postgres,rustfs}, config/, .env,
#                                               runtime/, logs/, .initialized, 本卸载脚本。
#   sudo ${FLOATCTF_HOME}/uninstall.sh --purge    PERMANENT 删除全部 FloatCTF 自有数据
#                                               （PG/RustFS 数据、config、secrets、runtime、
#                                               日志、API 二进制、web、compose、systemd 单元、
#                                               动态赛事资源、sysctl/modules 文件、floatctf 用户、
#                                               ${FLOATCTF_HOME}、本脚本自身）。需输入确认文本
#                                               "PURGE FLOATCTF"（除非 --yes）。
#
# 共享宿主依赖永不卸载：Docker / docker compose / nftables 包 / wireguard-tools /
# iproute2 / systemd。绝不触碰无关 Docker 对象 / WG 接口 / nftables 状态 / 路由 /
# libvirt / Incus / 其他应用。
#
set -Eeuo pipefail

FCTF_ROOT="${FLOATCTF_HOME}"
FCTF_USER="floatctf"

info() { printf '%s[INFO]%s %s\n'  "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n'  "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
warn() { printf '%s[WARN]%s %s\n'  "$(tput setaf 3 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n'  "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

require_root() {
    [ "$(id -u)" -eq 0 ] || die "需要 root。请改用: sudo $FCTF_ROOT/uninstall.sh"
}

MODE="safe"
PURGE_YES=0
SELF_TMP=""

usage() {
    cat <<'EOF'
用法：
  sudo ${FLOATCTF_HOME}/uninstall.sh             安全卸载（保留 PG/RustFS 数据、config、secrets）
  sudo ${FLOATCTF_HOME}/uninstall.sh --purge     永久删除全部 FloatCTF 自有数据（需确认 PURGE FLOATCTF）
  sudo ${FLOATCTF_HOME}/uninstall.sh --purge --yes  跳过确认（仅限非交互 purge）
  sudo ${FLOATCTF_HOME}/uninstall.sh --help
EOF
}

parse_args() {
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

INTERNAL_MODE_VAR="FCTF_UNINSTALL_CONT"
SELF_TMP=""

run_purge_via_temp() {
    if [ -n "${!INTERNAL_MODE_VAR:-}" ]; then
        SELF_TMP="$0"
        trap 'rm -f "$SELF_TMP"' EXIT INT TERM
        return 0
    fi
    local src="$0"
    [ -f "$src" ] || src="$FCTF_ROOT/uninstall.sh"
    SELF_TMP="/tmp/floatctf-uninstall.$$.sh"
    install -m 0700 "$src" "$SELF_TMP"
    local yesflag=()
    [ "$PURGE_YES" = "1" ] && yesflag+=(--yes)
    exec env "$INTERNAL_MODE_VAR=1" bash "$SELF_TMP" --purge "${yesflag[@]}"
}

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

valid_fctf_name() { [[ "$1" =~ ${AWK_NAME_OK} ]] && [[ "$1" == fawg_* || "$1" == fctfawd* || "$1" == fctf-awd-* || "$1" == fctf-flagserver-* || "$1" == fctf-judgeserver-* || "$1" == fctf-awdp-* ]]; }

systemctl_stop_units() {
    info "── 停止并停用 FloatCTF systemd 单元 ──"
    if ! command -v systemctl >/dev/null 2>&1; then
        warn "宿主无 systemctl，跳过 systemd 单元操作"
        return
    fi
    systemctl stop floatctf.target 2>/dev/null || true
    systemctl disable floatctf.target floatctf-infra.service floatctf-api.service 2>/dev/null || true
    systemctl stop floatctf-api.service floatctf-infra.service 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
    systemctl reset-failed floatctf.target floatctf-infra.service floatctf-api.service 2>/dev/null || true
    ok "systemd 单元已停止/停用"
}

stop_api_first() {
    if command -v systemctl >/dev/null 2>&1; then
        systemctl stop floatctf-api.service 2>/dev/null || true
    fi
    if [ -x "$FCTF_ROOT/bin/floatctf" ]; then
        pkill -f "^$FCTF_ROOT/bin/floatctf" 2>/dev/null || true
    fi
}

cleanup_gameboxes() {
    info "── 清理 GameBox 容器（依据 awd.* 标签，所有权限定）──"
    local ids
    ids=$(docker ps -aq --filter 'label=awd.resource_kind' 2>/dev/null | tr '\n' ' ')
    [ -n "${ids// /}" ] || { ok "无 GameBox 容器"; return; }
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
    for c in fctf-awd- fctf-awdp-practice fctf-awdp-control fctf-awdp-; do
        pat="${c}*"
        while read -r x; do
            [ -n "$x" ] || continue
            if [[ "$x" =~ ${AWK_NAME_OK} ]] && [[ "$x" == "$c"* ]]; then
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

cleanup_iptables_docker_forward() {
    command -v iptables >/dev/null 2>&1 || { warn "无 iptables，跳过 Docker 反欺骗规则清理"; return; }
    info "── 清理 Docker 反欺骗放行规则（仅限 iifname=fawg_* 的 FloatCTF 规则）──"
    local line spec
    for table in raw filter; do
        for chain in PREROUTING DOCKER-USER; do
            while read -r line; do
                [ -n "$line" ] || continue
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

stop_infra_containers() {
    info "── 停止/移除基础设施容器（compose down，保护 bind-mount 数据）──"
    if [ -f "$FCTF_ROOT/compose.prod.yml" ] && [ -d "$FCTF_ROOT" ]; then
        ( cd "$FCTF_ROOT" \
            && { docker compose -f compose.prod.yml down 2>/dev/null \
                 || docker compose -f compose.prod.yml stop 2>/dev/null \
                 || docker stop floatctf-postgres floatctf-rustfs floatctf-nginx 2>/dev/null || true; } ) \
            && ok "infra 容器已停止/移除（数据保留在 bind-mount）"
    else
        warn "未找到 $FCTF_ROOT/compose.prod.yml，跳过 compose down；尝试按名字精确停止"
        docker stop floatctf-postgres floatctf-rustfs floatctf-nginx 2>/dev/null || true
    fi
    local c
    for c in floatctf-postgres floatctf-rustfs floatctf-nginx; do
        if [ -n "$(docker ps -aq --filter name="^${c}$" 2>/dev/null)" ]; then
            docker rm -f "$c" >/dev/null 2>&1 && ok "已移除容器 $c" || warn "移除容器 $c 失败（忽略）"
        fi
    done
}

remove_application_artifacts() {
    info "── 移除可运行应用产物（保留 data/config/.env/runtime/logs）──"
    local p
    for p in "$FCTF_ROOT/bin" "$FCTF_ROOT/web" "$FCTF_ROOT/compose.dev.yml" "$FCTF_ROOT/compose.prod.yml" "$FCTF_ROOT/merged.sql"; do
        if [ -e "$p" ] || [ -L "$p" ]; then
            rm -rf -- "$p" && ok "已移除 $p" || warn "移除 $p 失败（忽略）"
        fi
    done
}

SYSCTL_FILE="/etc/sysctl.d/99-floatctf.conf"
MODULES_FILE="/etc/modules-load.d/floatctf-br-netfilter.conf"

safe_uninstall() {
    info "==== 安全卸载（保留可恢复状态）===="
    if [ ! -d "$FCTF_ROOT" ]; then
        info "$FCTF_ROOT 不存在 —— 已是未安装状态"
        ok "FloatCTF 已卸载（或从未安装）。"
        return
    fi

    stop_api_first
    require_tools
    cleanup_gameboxes
    cleanup_awd_named_containers
    cleanup_docker_networks
    cleanup_wireguard
    cleanup_iptables_docker_forward
    cleanup_nftables
    systemctl_stop_units
    stop_infra_containers
    remove_application_artifacts

    ok "FloatCTF 已卸载。"

    cat <<EOF

保留的数据（可恢复）:
  PostgreSQL 数据: $FCTF_ROOT/data/postgres
  RustFS 数据   : $FCTF_ROOT/data/rustfs
  配置/密钥      : $FCTF_ROOT/config 与 $FCTF_ROOT/.env
  运行时工作目录 : $FCTF_ROOT/runtime
  日志          : $FCTF_ROOT/logs

重新安装:
  运行 install.sh（会恢复相同数据与密钥，API 启动时自动重建 AWD 动态资源）。

完全删除:
  sudo $FCTF_ROOT/uninstall.sh --purge

本卸载脚本 $FCTF_ROOT/uninstall.sh 保留，作为生命周期/恢复工具（勿删）。
EOF
}

purge_confirm() {
    [ "$PURGE_YES" = "1" ] && return 0
    echo ""
    echo "!!!! 危险操作 !!!!"
    echo "你将永久删除全部 FloatCTF 自有数据，包括:"
    echo "  - PostgreSQL 数据        : $FCTF_ROOT/data/postgres"
    echo "  - RustFS 数据            : $FCTF_ROOT/data/rustfs"
    echo "  - 配置 / 密钥            : $FCTF_ROOT/config, $FCTF_ROOT/.env"
    echo "  - API 二进制 / web / compose / runtime / 日志"
    echo "  - systemd 单元            floatctf-{api,infra}.service, floatctf.target"
    echo "  - 动态赛事资源            GameBox/FlagServer/JudgeServer 容器、赛事网络、"
    echo "                             WireGuard 接口、nftables 表、转发规则"
    echo "  - sysctl/modules 文件      /etc/sysctl.d/99-floatctf.conf,"
    echo "                              /etc/modules-load.d/floatctf-br-netfilter.conf"
    echo "  - floatctf 服务用户"
    echo "  - 安装根目录               $FCTF_ROOT（含本卸载脚本）"
    echo ""
    echo "此操作不可撤销。如要继续，请输入: PURGE FLOATCTF"
    read -r -p "> " ans || die "中断：未确认，已中止。"
    [ "$ans" = "PURGE FLOATCTF" ] || die "确认文本不匹配，已中止（未删除任何内容）。"
}

purge_remove_dynamic() {
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
        systemctl stop floatctf.target floatctf-api.service floatctf-infra.service 2>/dev/null || true
        systemctl disable floatctf.target floatctf-api.service floatctf-infra.service 2>/dev/null || true
        rm -f /etc/systemd/system/floatctf-api.service \
              /etc/systemd/system/floatctf-infra.service \
              /etc/systemd/system/floatctf.target
        systemctl daemon-reload 2>/dev/null || true
        systemctl reset-failed floatctf.target floatctf-infra.service floatctf-api.service 2>/dev/null || true
        ok "FloatCTF systemd 单元已移除"
    else
        rm -f /etc/systemd/system/floatctf-api.service \
              /etc/systemd/system/floatctf-infra.service \
              /etc/systemd/system/floatctf.target
        ok "无 systemctl，直接移除单元文件"
    fi
}

purge_remove_sysctl_modules() {
    info "── 移除 FloatCTF 自有 sysctl / modules-load 文件 ──"
    local removed=0
    if [ -f "$SYSCTL_FILE" ]; then
        rm -f "$SYSCTL_FILE" && { removed=1; ok "已移除 $SYSCTL_FILE"; }
    fi
    if [ -f "$MODULES_FILE" ]; then
        rm -f "$MODULES_FILE" && { removed=1; ok "已移除 $MODULES_FILE"; }
    fi
    if [ "$removed" = "0" ]; then
        ok "无 FloatCTF sysctl/modules 文件（或已不存在）"
    fi
}

purge_remove_user() {
    info "── 移除 floatctf 服务用户 ──"
    if id "$FCTF_USER" >/dev/null 2>&1; then
        local home shell
        home=$(getent passwd "$FCTF_USER" | cut -d: -f6)
        shell=$(getent passwd "$FCTF_USER" | cut -d: -f7)
        if [ "$home" = "$FCTF_ROOT" ] && [ "$shell" = "/usr/sbin/nologin" ]; then
            userdel -r "$FCTF_USER" 2>/dev/null && ok "已移除用户 $FCTF_USER" \
                || { warn "userdel $FCTF_USER 失败（可能仍有进程占用）"; \
                     warn "保留用户记录；请确认无 floatctf 进程后重试 userdel -r floatctf"; }
        else
            warn "账户 $FCTF_USER 不匹配预期（home=$home shell=$shell），跳过删除"
        fi
    else
        ok "用户 $FCTF_USER 不存在"
    fi
}

purge_run() {
    info "==== 永久删除（purge）===="
    purge_confirm
    purge_remove_dynamic
    purge_remove_systemd_units
    stop_infra_containers
    purge_remove_sysctl_modules
    if [ -e "$FCTF_ROOT" ]; then
        rm -rf -- "$FCTF_ROOT" && ok "已删除安装根目录 $FCTF_ROOT" \
            || warn "删除 $FCTF_ROOT 部分失败（检查权限）"
    else
        ok "安装根目录 $FCTF_ROOT 已不存在"
    fi
    purge_remove_user
    ok "FloatCTF purge 完成。"
    echo ""
    echo "遗留检查："
    echo "  - 未曾触碰共享宿主依赖（Docker / compose / nftables 包 / WG 包 / iproute2 / systemd）。"
    echo "  - 未曾触碰无关 Docker 对象 / WG 接口 / nftables 规则 / 路由 / libvirt / Incus。"
    echo "  - 如需关闭 IPv4 转发 / br_netfilter，请手动评估（可能被其他负载依赖）。"
    echo ""
    echo "重新初始化并部署（全新安装）: "
    echo "  sudo ./install.sh"
}

main() {
    parse_args "$@"
    require_root
    if [ "$MODE" = "purge" ]; then
        run_purge_via_temp
        purge_run
    else
        safe_uninstall
    fi
}

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
    local api_port pg_port rustfs_port http_port
    api_port=$(env_get API_PORT 9090)
    pg_port=$(env_get POSTGRES_PORT 5433)
    rustfs_port=$(env_get RUSTFS_PORT 9000)
    http_port=$(env_get HTTP_PORT 80)
    info "端口：API=$api_port PG=$pg_port RustFS=$rustfs_port HTTP=$http_port"
    for port_spec in "$api_port" "$pg_port" "$rustfs_port" "$http_port"; do
        if ss -ltn 2>/dev/null | awk '{print $4}' | grep -qE "[:.]${port_spec}$"; then
            local owned=0
            if docker ps --format '{{.Names}} {{.Ports}}' 2>/dev/null \
                | grep -qE "floatctf-(postgres|rustfs).*[:.]${port_spec}"; then
                owned=1
            elif [ "$port_spec" = "$(env_get HTTP_PORT 80)" ] || [ "$port_spec" = "$(env_get HTTPS_PORT 443)" ]; then
                [ -n "$(docker ps -q --filter name=^floatctf-nginx$ 2>/dev/null)" ] && owned=1
            elif [ "$port_spec" = "$api_port" ]; then
                systemctl -q is-active floatctf-api.service 2>/dev/null && owned=1
            fi
            if [ "$owned" = "1" ]; then
                info "端口 $port_spec 由本平台进程占用（重部署，放行）"
            else
                die "端口 $port_spec 已被无关进程占用（ss 检查）；请调整 .env 端口"
            fi
        fi
    done
    ok "precheck 通过"
}

prepare_env() {
    info "──── 部署：配置（.env + floatctf.toml + nginx.conf）────"
    mkdir -p "$FLOATCTF_HOME/config/nginx" "$FLOATCTF_HOME/logs/{api,nginx,rustfs}"
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
        env_set RUSTFS_PORT "${RUSTFS_PORT:-9000}"
        env_set RUSTFS_CONSOLE_PORT "${RUSTFS_CONSOLE_PORT:-9001}"
        env_set HTTP_PORT "${HTTP_PORT:-80}"
        env_set HTTPS_PORT "${HTTPS_PORT:-443}"
        env_set HOST_ADDRESS "${HOST_ADDRESS:-127.0.0.1}"
        chmod 600 "$ENV_FILE"
        ok ".env 已生成（含新密钥，root 可读）"
    else
        env_set API_PORT "$(env_get API_PORT 9090)"
        env_set POSTGRES_PORT "$(env_get POSTGRES_PORT 5433)"
        env_set RUSTFS_PORT "$(env_get RUSTFS_PORT 9000)"
        env_set RUSTFS_CONSOLE_PORT "$(env_get RUSTFS_CONSOLE_PORT 9001)"
        env_set HTTP_PORT "$(env_get HTTP_PORT 80)"
        env_set HTTPS_PORT "$(env_get HTTPS_PORT 443)"
        env_set HOST_ADDRESS "$(env_get HOST_ADDRESS 127.0.0.1)"
        ok ".env 已存在，保留密钥并更新非敏感项"
    fi
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
    set -a; . "$ENV_FILE"; set +a
    export FLOATCTF_HOME
    # 先替换模板里的 FLOATCTF_HOME 占位符，再渲染。
    sed "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/.floatctf.toml.tmpl" > "$FLOATCTF_HOME/.floatctf.toml.tmpl.real"
    render "$FLOATCTF_HOME/.floatctf.toml.tmpl.real" "$FLOATCTF_HOME/config/floatctf.toml"
    render "$FLOATCTF_HOME/.nginx.conf.tmpl" "$FLOATCTF_HOME/config/nginx/nginx.conf"
    rm -f "$FLOATCTF_HOME/.floatctf.toml.tmpl.real"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/nginx"
    chmod 750 "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/nginx"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/nginx/nginx.conf"
    chmod 640 "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/nginx/nginx.conf"
    mkdir -p "$FLOATCTF_HOME/config/nginx/keys"
    ok "配置已写入（floatctf.toml + nginx.conf，密钥保留）"
}

stage_release() {
    info "──── 部署：装配产物 → $FLOATCTF_HOME ────"
    install -m 0755 "$PKG_DIR/bin/floatctf" "$FLOATCTF_HOME/bin/floatctf"
    mkdir -p "$FLOATCTF_HOME/web"
    find "$FLOATCTF_HOME/web" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + 2>/dev/null || true
    cp -a "$PKG_DIR/web/." "$FLOATCTF_HOME/web/"
    chown -R root:root "$FLOATCTF_HOME/web"
    install -m 0644 "$PKG_DIR/merged.sql" "$FLOATCTF_HOME/merged.sql"
    mkdir -p "$FLOATCTF_HOME/runtime"
    chown "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/runtime"
    chown -R 10001:10001 "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    local pg_mismatch=0 pg_owner
    if [ -e "$FLOATCTF_HOME/data/postgres/PG_VERSION" ]; then
        pg_owner=$(stat -c '%u' "$FLOATCTF_HOME/data/postgres/PG_VERSION" 2>/dev/null || echo "999")
        [ "$pg_owner" = "999" ] || pg_mismatch=1
    fi
    if [ "$pg_mismatch" = "1" ]; then
        warn "postgres 数据属主非 999；停容器后修正"
        docker stop floatctf-postgres >/dev/null 2>&1 || true
        chown -R 999:999 "$FLOATCTF_HOME/data/postgres"
        docker start floatctf-postgres >/dev/null 2>&1 || true
        ok "postgres 数据属主已修正为 999"
    elif [ -z "$(ls -A "$FLOATCTF_HOME/data/postgres" 2>/dev/null)" ]; then
        chown -R 999:999 "$FLOATCTF_HOME/data/postgres" 2>/dev/null || true
    fi
    ok "产物装配完成（bin/floatctf + web/ + merged.sql + 容器目录属主）"
}

start_infra() {
    info "──── 部署：基础设施容器（docker compose -f compose.prod.yml up -d --wait）────"
    ( cd "$FLOATCTF_HOME" && docker compose -f compose.prod.yml up -d --wait ) \
        || die "infra 容器启动/健康检查失败"
    ok "infra 就绪（postgres/rustfs/nginx healthcheck 通过）"
}

init_db() {
    info "──── 部署：数据库初始化（merged.sql，fresh-DB）────"
    local pg_user pg_db
    pg_user=$(env_get POSTGRES_USER postgres)
    pg_db=$(env_get POSTGRES_DB floatctf_db)
    # 仅当库是空的才初始化（避免误灌已有数据；全新安装语义）。
    local table_count
    table_count=$(docker exec floatctf-postgres psql -U "$pg_user" -d "$pg_db" -tAc \
        "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'" 2>/dev/null || echo "?")
    if [ "$table_count" != "0" ] && [ "$table_count" != "?" ]; then
        warn "数据库 $pg_db 已有 $table_count 张表，跳过 merged.sql 初始化（全新安装语义）"
        return
    fi
    docker exec -i floatctf-postgres psql -U "$pg_user" -d "$pg_db" -v ON_ERROR_STOP=1 \
        < "$FLOATCTF_HOME/merged.sql" || die "merged.sql 初始化失败"
    ok "数据库初始化完成（merged.sql 已应用）"
}

install_systemd() {
    info "──── 部署：systemd 单元（floatctf-infra / api / target）────"
    systemctl daemon-reload
    systemctl enable floatctf.target floatctf-infra.service floatctf-api.service
    ok "systemd 单元已安装并 enable（floatctf.target）"
}

start_api() {
    info "──── 部署：启动 API（floatctf-api.service）────"
    systemctl restart floatctf-api.service || die "API 启动失败（journalctl -u floatctf-api）"
    local api_port tries=0 code
    api_port=$(env_get API_PORT 9090)
    while [ "$tries" -lt 30 ]; do
        code=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$api_port/api/announcements" 2>/dev/null || true)
        if [ -n "$code" ] && [ "$code" != "000" ]; then
            ok "API 已在 $api_port 监听（HTTP $code）"
            return
        fi
        tries=$((tries + 1)); sleep 2
    done
    die "API 60s 内未监听 $api_port（journalctl -u floatctf-api 查看日志）"
}

run_deploy() {
    info "──── 第三阶段：部署 → $FLOATCTF_HOME ────"
    precheck
    prepare_env
    write_compose_dev
    write_compose_prod
    write_config_template
    write_nginx_template
    prepare_configs
    stage_release
    start_infra
    init_db
    write_systemd_units
    install_systemd
    start_api
    write_uninstall
    ok "部署完成：$FLOATCTF_HOME"
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    info "FloatCTF 一键安装 → $FLOATCTF_HOME"
    run_init
    acquire_package
    run_deploy
    ok "FloatCTF 安装完成：$FLOATCTF_HOME"
}

main
