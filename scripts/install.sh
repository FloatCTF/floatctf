#!/usr/bin/env bash
#
# FloatCTF 一键安装（Phase 11）— 下载 release tarball + 主机初始化(幂等) + 部署。
#
# 语义：
#   1. 主机初始化（幂等）：发行版检测/装缺失主机包、docker/nftables/WireGuard 能力检查、
#      IPv4 转发 + br_netfilter、floatctf 服务用户 + docker 组 + /home/floatctf 布局。
#      已存在/已做过 → 逐项跳过（不依赖 .initialized 单点标记）。
#   2. 下载 release tarball：默认从 GitHub Release 下载自包含产物（bin + web +
#      compose.yml + config/nginx 模板 + migrate.sh + migrations + systemd 单元 +
#      uninstall.sh）。地址可经 FLOATCTF_RELEASE_URL 或 --url 覆盖（默认 fake 占位）。
#   3. 部署：渲染配置 → 装配 bin/web/compose → 起 infra(--wait) → forward-only 迁移 →
#      装 systemd 单元 → 启动 API → 安装独立 uninstall.sh。
#
# 用法：
#   sudo ./install.sh                     # 用默认（fake）release 地址，完整安装
#   sudo ./install.sh --url <tarball-url> # 显式指定 release 地址
#   sudo ./install.sh --init-only         # 只做主机初始化（跳过下载与部署），
#                                          # 开发环境用：之后自行起 docker-compose +
#                                          # 本地 vite / cargo run
#   FLOATCTF_ROOT=/home/floatctf sudo ./install.sh ...
#
# 注意：
#   - AWD 服务镜像（floatctf/awd-flagserver / awd-judgeserver）暂不在本脚本构建，
#     需另行准备（TODO Phase 11.1：registry 拉取或本地 docker build）。
#   - 本脚本替代原 init.sh / deploy.sh / build-release.sh（已删除）。
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
FCTF_ROOT="${FCTF_ROOT:-/home/floatctf}"
FCTF_USER="floatctf"

# fake 占位地址：真实 release 地址发布后替换（或经 --url / FLOATCTF_RELEASE_URL 覆盖）。
DEFAULT_RELEASE_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/floatctf-0.0.0-fake.tar.gz"

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
RELEASE_URL="$DEFAULT_RELEASE_URL"
INIT_ONLY=0
while [ $# -gt 0 ]; do
    case "$1" in
        --url) RELEASE_URL="${2:?--url 需要一个地址参数}"; shift ;;
        --init-only) INIT_ONLY=1 ;;
        -h|--help)
            sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) die "未知参数: $1（--help 查看用法）" ;;
    esac
    shift
done
# 环境变量覆盖（低于 --url 显式传参）。
RELEASE_URL="${FLOATCTF_RELEASE_URL:-$RELEASE_URL}"

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
        useradd --system --home-dir "$FCTF_ROOT" --shell /usr/sbin/nologin "$FCTF_USER"
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
        mkdir -p "$FCTF_ROOT/$d"
    done
    chown root:"$FCTF_USER" "$FCTF_ROOT" >/dev/null 2>&1 || true
    chmod 750 "$FCTF_ROOT" >/dev/null 2>&1 || true
    local run_dir
    for run_dir in bin web data logs runtime gameboxes; do
        chown -R "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT/$run_dir" >/dev/null 2>&1 || true
    done
    chown root:"$FCTF_USER" "$FCTF_ROOT/config" "$FCTF_ROOT/config/nginx" >/dev/null 2>&1 || true
    chmod 750 "$FCTF_ROOT/config" >/dev/null 2>&1 || true
    ok "布局就绪: $FCTF_ROOT/{bin,web,config/nginx,data/{postgres,rustfs},logs/{api,nginx,rustfs},runtime,gameboxes}"

    if [ ! -f "$FCTF_ROOT/.initialized" ]; then
        printf 'FloatCTF host initialized at %s by %s\n' "$(date -Is 2>/dev/null || date)" "${SUDO_USER:-root}" \
            > "$FCTF_ROOT/.initialized"
        chown "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT/.initialized"
        ok "完成标记已写入 $FCTF_ROOT/.initialized"
    else
        ok "检测到 $FCTF_ROOT/.initialized（主机已初始化）"
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
# 第二阶段：下载 release tarball
# ============================================================================
download_release() {
    info "──── 第二阶段：下载 release tarball ────"
    info "release URL: $RELEASE_URL"
    TMP_STAGE_DIR="$(mktemp -d /tmp/floatctf-install.XXXXXX)"
    local tarball="$TMP_STAGE_DIR/release.tar.gz"

    info "下载中...（curl -fL）"
    if ! curl -fL --retry 3 --connect-timeout 30 -o "$tarball" "$RELEASE_URL"; then
        die "下载失败: $RELEASE_URL（请确认地址正确；这是 fake 占位地址，替换为真实 release 地址或经 --url 传入）"
    fi

    info "解压 tarball..."
    tar xzf "$tarball" -C "$TMP_STAGE_DIR" \
        || die "解压失败: $tarball"

    # 定位解压出的顶层目录（假设 tarball 内含单一顶层目录）。
    local pkg_dir
    pkg_dir="$(find "$TMP_STAGE_DIR" -mindepth 1 -maxdepth 1 -type d ! -name 'release.tar.gz' | head -1)"
    [ -n "$pkg_dir" ] || die "tarball 结构异常：未找到顶层目录"

    # 校验关键产物存在。
    [ -f "$pkg_dir/bin/floatctf" ] || die "tarball 缺少 bin/floatctf"
    [ -d "$pkg_dir/web" ] || die "tarball 缺少 web/"
    [ -f "$pkg_dir/compose.yml" ] || die "tarball 缺少 compose.yml"
    [ -f "$pkg_dir/migrate.sh" ] || die "tarball 缺少 migrate.sh"

    VERSION="$(basename "$pkg_dir" | sed 's/^floatctf-//')"
    info "release 版本: ${VERSION:-unknown}"
    ok "release tarball 就绪: $pkg_dir"
    # 供后续阶段引用。
    PKG_DIR="$pkg_dir"
}

# ============================================================================
# 第三阶段：部署（原 deploy.sh 逻辑，模板来源改为 tarball 内）
# ============================================================================
ENV_FILE="$FCTF_ROOT/.env"

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
    [ "$(id -u)" -eq 0 ] && warn "以 root 运行：构建/装配将用 root"
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
    mkdir -p "$FCTF_ROOT/config/nginx" "$FCTF_ROOT/logs/{api,nginx,rustfs}"
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
        env_set VERSION "$VERSION"
        chmod 600 "$ENV_FILE"
        ok ".env 已生成（含新密钥，root 可读）"
    else
        env_set VERSION "$VERSION"
        env_set API_PORT "$(env_get API_PORT 9090)"
        env_set POSTGRES_PORT "$(env_get POSTGRES_PORT 5433)"
        env_set RUSTFS_PORT "$(env_get RUSTFS_PORT 9000)"
        env_set RUSTFS_CONSOLE_PORT "$(env_get RUSTFS_CONSOLE_PORT 9001)"
        env_set HTTP_PORT "$(env_get HTTP_PORT 80)"
        env_set HTTPS_PORT "$(env_get HTTPS_PORT 443)"
        env_set HOST_ADDRESS "$(env_get HOST_ADDRESS 127.0.0.1)"
        ok ".env 已存在，保留密钥并更新非敏感项"
    fi
    chown -R "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT/data" "$FCTF_ROOT/logs" "$FCTF_ROOT/runtime" 2>/dev/null || true
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
    render "$PKG_DIR/config/floatctf.prod.toml" "$FCTF_ROOT/config/floatctf.toml"
    render "$PKG_DIR/nginx/nginx.prod.conf" "$FCTF_ROOT/config/nginx/nginx.conf"
    chown root:"$FCTF_USER" "$FCTF_ROOT/config" "$FCTF_ROOT/config/nginx"
    chmod 750 "$FCTF_ROOT/config" "$FCTF_ROOT/config/nginx"
    chown root:"$FCTF_USER" "$FCTF_ROOT/config/floatctf.toml" "$FCTF_ROOT/config/nginx/nginx.conf"
    chmod 640 "$FCTF_ROOT/config/floatctf.toml" "$FCTF_ROOT/config/nginx/nginx.conf"
    mkdir -p "$FCTF_ROOT/config/nginx/keys"
    ok "配置已写入（floatctf.toml + nginx.conf，密钥保留）"
}

stage_release() {
    info "──── 部署：装配产物 → $FCTF_ROOT ────"
    install -m 0755 "$PKG_DIR/bin/floatctf" "$FCTF_ROOT/bin/floatctf"
    mkdir -p "$FCTF_ROOT/web"
    find "$FCTF_ROOT/web" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + 2>/dev/null || true
    cp -a "$PKG_DIR/web/." "$FCTF_ROOT/web/"
    chown -R root:root "$FCTF_ROOT/web"
    install -m 0644 "$PKG_DIR/compose.yml" "$FCTF_ROOT/compose.yml"
    mkdir -p "$FCTF_ROOT/runtime"
    chown "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT/runtime"
    chown -R 10001:10001 "$FCTF_ROOT/data/rustfs" "$FCTF_ROOT/logs/rustfs" 2>/dev/null || true
    local pg_mismatch=0 pg_owner
    if [ -e "$FCTF_ROOT/data/postgres/PG_VERSION" ]; then
        pg_owner=$(stat -c '%u' "$FCTF_ROOT/data/postgres/PG_VERSION" 2>/dev/null || echo "999")
        [ "$pg_owner" = "999" ] || pg_mismatch=1
    fi
    if [ "$pg_mismatch" = "1" ]; then
        warn "postgres 数据属主非 999；停容器后修正"
        docker stop floatctf-postgres >/dev/null 2>&1 || true
        chown -R 999:999 "$FCTF_ROOT/data/postgres"
        docker start floatctf-postgres >/dev/null 2>&1 || true
        ok "postgres 数据属主已修正为 999"
    elif [ -z "$(ls -A "$FCTF_ROOT/data/postgres" 2>/dev/null)" ]; then
        chown -R 999:999 "$FCTF_ROOT/data/postgres" 2>/dev/null || true
    fi
    ok "产物装配完成（bin/floatctf + web/ + compose.yml + 容器目录属主）"
}

start_infra() {
    info "──── 部署：基础设施容器（docker compose up -d --wait）────"
    ( cd "$FCTF_ROOT" && docker compose -f compose.yml up -d --wait ) \
        || die "infra 容器启动/健康检查失败"
    ok "infra 就绪（postgres/rustfs/nginx healthcheck 通过）"
}

migrate_db() {
    info "──── 部署：数据库迁移（forward-only）────"
    env FLOATCTF_CONFIG="$FCTF_ROOT/config/floatctf.toml" \
        "$PKG_DIR/migrate.sh" apply || die "数据库迁移失败（migrate.sh apply 非零）"
    ok "迁移完成（schema_migrations 已更新）"
}

install_systemd() {
    info "──── 部署：systemd 单元（floatctf-infra / api / target）────"
    local u
    for u in floatctf-infra.service floatctf-api.service floatctf.target; do
        install -m 0644 "$PKG_DIR/systemd/$u" "/etc/systemd/system/$u"
    done
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

install_uninstall() {
    info "──── 部署：安装独立卸载脚本 → $FCTF_ROOT/uninstall.sh ────"
    local src="$PKG_DIR/uninstall.sh"
    [ -f "$src" ] || die "缺少卸载脚本: $src"
    local tmp="$FCTF_ROOT/.uninstall.tmp.$$"
    install -m 0750 "$src" "$tmp" || die "安装 uninstall.sh 临时副本失败"
    if ! bash -n "$tmp" 2>/dev/null; then
        rm -f "$tmp" 2>/dev/null || true
        die "uninstall.sh 语法校验失败（bash -n），未安装"
    fi
    install -m 0750 "$tmp" "$FCTF_ROOT/uninstall.sh" || { rm -f "$tmp" 2>/dev/null || true; die "安装 uninstall.sh 失败"; }
    rm -f "$tmp" 2>/dev/null || true
    chown root:"$FCTF_USER" "$FCTF_ROOT/uninstall.sh"
    chmod 0750 "$FCTF_ROOT/uninstall.sh"
    ok "$FCTF_ROOT/uninstall.sh 已安装（root:$FCTF_USER 0750）"
}

run_deploy() {
    info "──── 第三阶段：部署 → $FCTF_ROOT ────"
    precheck
    prepare_env
    prepare_configs
    stage_release
    start_infra
    migrate_db
    install_systemd
    start_api
    install_uninstall
    ok "部署完成：$FCTF_ROOT"
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    info "FloatCTF 一键安装 → $FCTF_ROOT"
    run_init
    if [ "$INIT_ONLY" = "1" ]; then
        ok "仅初始化完成（--init-only，跳过下载与部署）。"
        ok "开发环境：自行启动 docker compose + 本地 vite/cargo run。"
        return
    fi
    download_release
    run_deploy
    ok "FloatCTF 安装完成：$FCTF_ROOT"
}

main
