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
#   或显式指定 6 个 release 产物 URL：
#     sudo bash install.sh --api-url <bin> --helper-url <bin> --web-url <dist> \
#                         --migrate-url <sql> --frontend-manager-url <script> \
#                         --ops-url <ops-tools.tar.gz>
#   流程：主机初始化 → 下载 6 产物 → 本地构建 API runtime image → 部署 →
#         安装 bootstrap 静态页 + Default Frontend + 前端管理器 frontend.sh →
#         安装运维工具（backup.sh / restore.sh / db/migrate.sh + migrations）→
#         既有数据库则先 apply 新增迁移（fresh 库由 merged.sql bootstrap，自动跳过）→
#         写 helper/Compose systemd 单元并 enable（不 start）。
#
# 【升级既有安装】重新运行本脚本（同一入口，幂等）：
#   - release 的 db/migrate.sh 按 forward-only + 版本幂等应用**新增**迁移，
#     已应用版本由 schema_migrations 跳过；不做任何破坏性回滚。
#   - --skip-migrations（或 FLOATCTF_SKIP_MIGRATIONS=1）跳过迁移步骤；跳过时
#     平台可能因缺少新表/新列而启动失败。
#   - 管理员在 config/floatctf.toml 里自定义过的 AWD/AWDP 镜像引用会被**保留**
#     （并告警）；只有已知 stock 形态才迁移到新的 canonical GHCR 默认值。
#     强制回默认用 --reset-runtime-images；强制保留用 --keep-runtime-images。
#
# 【前置依赖（R3）】宿主必须提供 **Python 3.11+**（stdlib tomllib）：
#   release 的 db/migrate.sh 用 `python3 -c 'import tomllib'` 解析 TOML 配置，
#   升级路径的迁移全靠它。安装器用能力探测（不是版本字符串）在任何改动/下载
#   之前 fail closed。缺 tomllib 请先升级 python3（不得用 pip 安装 tomllib）。

# 安装后布局（$FLOATCTF_HOME，默认 /var/lib/floatctf）：
#   compose.prod.yml  .env  config/  data/  runtime/  logs/  web/  frontends/
#   merged.sql                 fresh-DB initdb 挂载（compose 引用）
#   backup.sh restore.sh       运维备份/恢复（来自 ops-tools 产物）
#   db/                        migrate.sh + migrations/ + merged.sql（升级用）
#   frontend.sh uninstall.sh   前端管理器 / 生命周期工具
#
# 【开发宿主初始化】由 `mise run setup` 内部调用；开发者无需直接运行：
#   sudo ./scripts/install.sh --develop --helper-bin <target/debug/floatctf-helper>
#   仅准备主机、系统用户/组、内核参数与 floatctf-helper，不创建 dev systemd infra。
#
# 环境变量（覆盖 6 个产物 URL / release 版本 / 迁移开关 / 运行时镜像）：
#   FLOATCTF_API_URL / FLOATCTF_HELPER_URL / FLOATCTF_WEB_URL /
#   FLOATCTF_MIGRATE_URL / FLOATCTF_FRONTEND_MANAGER_URL / FLOATCTF_OPS_URL /
#   FLOATCTF_VERSION / FLOATCTF_SKIP_MIGRATIONS /
#   FLOATCTF_SKIP_RUNTIME_IMAGES / FLOATCTF_RUNTIME_IMAGE_REGISTRY
# 安装根：
#   FLOATCTF_HOME=/opt/floatctf   （默认 /var/lib/floatctf）
#
# 注意：
#   - 全新安装与升级共用同一入口：升级 = 重新运行本脚本 + forward-only 迁移。
#   - 本脚本只写文件、创建（enable）systemd 服务，绝不自己启动服务/容器；
#     整平台由运维 systemctl start floatctf.target 启动（首次启动 postgres
#     自动用 merged.sql 初始化数据库）。唯一例外是 apply_migrations：升级既有
#     数据库时临时 `up -d postgres` 应用迁移（平台其余服务仍不启动）。
#   - AWD/AWDP 运行时镜像（${RUNTIME_IMAGE_REGISTRY}/awd-flagserver:V、
#     .../awd-judgeserver:V、.../awdp-judgeserver:V，V=release 版本）随 release
#     发布到 GHCR（release.yml 的 runtime-images job）。安装/升级时 ensure_runtime_images
#     会逐个 `docker image inspect`，缺则 `docker pull`；**拉不到就 die**（绝不继续
#     进入一个 AWD/AWDP 注定失败、要等比赛现场才炸的部署）。
#     离线/手工预载（escape hatch）：
#       docker save <三个 ref> -o images.tar   # 在有镜像的机器上
#       sudo docker load < images.tar          # 目标机，然后重新运行本安装器
#     只跑 Jeopardy（不使用 AWD/AWDP）的宿主可显式跳过：
#       --skip-runtime-images / FLOATCTF_SKIP_RUNTIME_IMAGES=1（降级为醒目告警）。
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
# 安装根：API 容器的 work_dir 固定为 /var/lib/floatctf/runtime（镜像 WORKDIR 同名），
# 宿主侧 ${FLOATCTF_HOME}/runtime 与它 identity 挂载，两边路径一致、无需换算。
FLOATCTF_HOME="${FLOATCTF_HOME:-/var/lib/floatctf}"
# 只需要组，不需要用户：API 在容器内以数值 "$FCTF_UID:$FCTF_GID" 运行，宿主上没有任何
# 进程以 floatctf 用户身份运行；组同时是 helper socket（0750 root:floatctf）的访问凭据。
FCTF_USER="floatctf"
FCTF_UID="65532"
FCTF_HELPER_USER="floatctf-helper"
HELPER_INSTALL_PATH="/usr/local/libexec/floatctf-helper"
# canonical AWD/AWDP 运行时镜像 registry 前缀：release.yml 的 runtime-images job 把三个
# 镜像推到这里，ensure_runtime_images 与 floatctf.toml 模板都从同一个前缀计算 ref
# （单一事实来源，避免"模板写 A、安装器查 B"）。可用 FLOATCTF_RUNTIME_IMAGE_REGISTRY
# 覆盖（例：指向自建 registry）；空值回落 canonical。
RUNTIME_IMAGE_REGISTRY="${FLOATCTF_RUNTIME_IMAGE_REGISTRY:-ghcr.io/floatctf}"
[ -n "$RUNTIME_IMAGE_REGISTRY" ] || RUNTIME_IMAGE_REGISTRY="ghcr.io/floatctf"
RUNTIME_IMAGE_REGISTRY="${RUNTIME_IMAGE_REGISTRY%/}"
export RUNTIME_IMAGE_REGISTRY

# fake 占位地址：真实 release 地址发布后替换（或经 --*-url / 环境变量覆盖）。
DEFAULT_API_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/floatctf"
DEFAULT_HELPER_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/floatctf-helper"
DEFAULT_WEB_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/web-dist.tar.gz"
DEFAULT_MIGRATE_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/merged.sql"
DEFAULT_FRONTEND_MANAGER_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/frontend.sh"
DEFAULT_OPS_URL="https://github.com/FloatCTF/floatctf/releases/download/v0.0.0-fake/ops-tools.tar.gz"

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
FRONTEND_MANAGER_URL="$DEFAULT_FRONTEND_MANAGER_URL"
OPS_URL="$DEFAULT_OPS_URL"
VERSION="${FLOATCTF_VERSION:-}"
VERSION_EXPLICIT=0
API_URL_EXPLICIT=0
HELPER_URL_EXPLICIT=0
WEB_URL_EXPLICIT=0
MIGRATE_URL_EXPLICIT=0
FRONTEND_MANAGER_URL_EXPLICIT=0
OPS_URL_EXPLICIT=0
DEVELOP=0
HELPER_BIN=""
# 升级既有安装时，迁移由 release 的 db/migrate.sh forward-only 应用；可用
# --skip-migrations 显式跳过（见 apply_migrations 的说明与风险提示）。
SKIP_MIGRATIONS="${FLOATCTF_SKIP_MIGRATIONS:-0}"
# AWD/AWDP 运行时镜像默认**硬要求**（缺则 die）；--skip-runtime-images 是 operator
# 显式退出口（只跑 Jeopardy 的宿主），降级为醒目告警。
SKIP_RUNTIME_IMAGES="${FLOATCTF_SKIP_RUNTIME_IMAGES:-0}"
# 升级时自定义镜像引用的处置：默认保留"非 stock"的自定义值；两者互斥。
RESET_RUNTIME_IMAGES=0
KEEP_RUNTIME_IMAGES=0
while [ $# -gt 0 ]; do
    case "$1" in
        --api-url) API_URL="${2:?--api-url 需要一个地址参数}"; API_URL_EXPLICIT=1; shift ;;
        --helper-url) HELPER_URL="${2:?--helper-url 需要一个地址参数}"; HELPER_URL_EXPLICIT=1; shift ;;
        --helper-bin) HELPER_BIN="${2:?--helper-bin 需要本地二进制路径}"; shift ;;
        --web-url) WEB_URL="${2:?--web-url 需要一个地址参数}"; WEB_URL_EXPLICIT=1; shift ;;
        --migrate-url) MIGRATE_URL="${2:?--migrate-url 需要一个地址参数}"; MIGRATE_URL_EXPLICIT=1; shift ;;
        --frontend-manager-url) FRONTEND_MANAGER_URL="${2:?--frontend-manager-url 需要一个地址参数}"; FRONTEND_MANAGER_URL_EXPLICIT=1; shift ;;
        --ops-url) OPS_URL="${2:?--ops-url 需要一个地址参数}"; OPS_URL_EXPLICIT=1; shift ;;
        --version) VERSION="${2:?--version 需要 release 版本，如 0.3.3}"; VERSION_EXPLICIT=1; shift ;;
        --skip-migrations) SKIP_MIGRATIONS=1 ;;
        --skip-runtime-images) SKIP_RUNTIME_IMAGES=1 ;;
        --reset-runtime-images) RESET_RUNTIME_IMAGES=1 ;;
        --keep-runtime-images) KEEP_RUNTIME_IMAGES=1 ;;
        --develop) DEVELOP=1 ;;
        -h|--help)
            # 打印文件头注释块（从第 2 行到第一条非注释行之前）；不写死行号，
            # 避免以后往头部加内容时 --help 被静默截断。
            sed -n '2,/^[^#]/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
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
[ "$FRONTEND_MANAGER_URL_EXPLICIT" -eq 1 ] || FRONTEND_MANAGER_URL="${FLOATCTF_FRONTEND_MANAGER_URL:-$FRONTEND_MANAGER_URL}"
[ "$OPS_URL_EXPLICIT" -eq 1 ] || OPS_URL="${FLOATCTF_OPS_URL:-$OPS_URL}"
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
# 升级保护的两种强制模式互斥（同时给出就无法判定意图 → fail closed，绝不猜）。
[ "$RESET_RUNTIME_IMAGES" = "1" ] && [ "$KEEP_RUNTIME_IMAGES" = "1" ] \
    && die "--reset-runtime-images 与 --keep-runtime-images 互斥，请只选一个"

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
# python3 是前端管理器（$FLOATCTF_HOME/frontend.sh）的运行时依赖：它用 python3 做
# frontend.json / registry.json 的 JSON 解析与原子注册表更新（见 docs/frontend/ARTIFACT.md）。
ARCH_PKGS=(docker docker-compose nftables wireguard-tools iproute2 conntrack-tools iptables procps-ng openssl curl tar postgresql python3)

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

# R3：release 的 db/migrate.sh 用 **stdlib tomllib** 解析 TOML（Python ≥3.11），
# 升级路径的迁移全靠它。刻意做**能力探测**而不是版本字符串比较：
#   * 版本号可能被 backport / venv / wrapper 欺骗，`import tomllib` 才是真实能力；
#   * 绝不使用 pip（宿主包管理器之外的东西不碰，也不用 sudo pip）。
# 该函数被 main()（任何 mutation 之前）、check_commands 与 precheck 三处调用。
check_python_tomllib() {
    command -v python3 >/dev/null 2>&1 \
        || die "缺少命令: python3（需要 Python 3.11+ 的 stdlib tomllib；release 的 db/migrate.sh 用它解析 TOML。请用宿主包管理器安装 python3 后重试）"
    if python3 -c 'import tomllib' >/dev/null 2>&1; then
        ok "Python tomllib 可用（$(python3 -V 2>&1 | head -1)）"
        return 0
    fi
    local pyver
    pyver="$(python3 -V 2>&1 | head -1 || true)"
    [ -n "$pyver" ] || pyver="未知"
    die "Python 缺少 stdlib tomllib（需要 Python 3 且带标准库 tomllib，即 **Python ≥3.11**；检测到: ${pyver}）。release 的 db/migrate.sh 用它解析 TOML 配置，**升级/迁移路径**必须有它才能运行。请用宿主包管理器升级 python3 到 ≥3.11（tomllib 只随标准库提供：不要用 pip 往宿主装，也不要用 sudo pip）后重新运行本安装器。"
}

check_commands() {
    local c
    for c in ip wg nft conntrack iptables docker sysctl modprobe; do
        command -v "$c" >/dev/null 2>&1 || die "缺少命令: $c"
    done
    # 前端管理器 frontend.sh 需要 python3（JSON 解析 + 原子注册表更新）；
    # 迁移器 db/migrate.sh 额外需要 Python ≥3.11 的 stdlib tomllib（见上）。
    command -v python3 >/dev/null 2>&1 \
        || die "缺少命令: python3（前端管理器 frontend.sh 与迁移器 db/migrate.sh 的依赖；apt/dnf/pacman 安装 python3 后重试）"
    check_python_tomllib
    ok "基础命令齐全（ip/wg/nft/conntrack/iptables/docker/sysctl/modprobe/python3+tomllib）"
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

ensure_service_group() {
    # 显式创建共享组，避免依赖各发行版 useradd 的 USERGROUPS_ENAB 默认值。
    # 不创建 floatctf 用户：容器以数值 uid/gid 运行，宿主上没有任何进程需要该身份
    #（历史上宿主 systemd 以 User=floatctf 跑 API，那个用途已随容器化消失）。
    if ! getent group "$FCTF_USER" >/dev/null 2>&1; then
        groupadd --system "$FCTF_USER"
        ok "已创建系统组 $FCTF_USER"
    fi

    # helper 必须是真实账号：systemd 单元以 User=floatctf-helper 运行，且需要 docker 组。
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
        # 历史清理：早期宿主 systemd 版本以 User=floatctf 运行 API，该账号可能被加入
        # docker 组；systemd 会继承 NSS 里的 supplementary groups，那样 API 就能绕过
        # helper 直连 /var/run/docker.sock。账号仍存在时收敛一次（已不存在则跳过）。
        if id "$FCTF_USER" >/dev/null 2>&1 \
            && id -nG "$FCTF_USER" 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
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
            ok "已确保历史账号 $FCTF_USER 不属于 docker 组（API Docker 权限仅经 helper）"
        fi

        if ! id -nG "$FCTF_HELPER_USER" 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
            usermod -aG docker "$FCTF_HELPER_USER"
            ok "已把 $FCTF_HELPER_USER 加入 docker 组（Docker 控制面仅授予 helper）"
        fi
    fi
}

check_user_layout() {
    ensure_service_group

    # 宿主目录属主 = API 容器的 numeric identity "$FCTF_UID:$FCTF_GID"。
    # FCTF_UID 与 API 镜像里的 USER 65532:65532 对齐（镜像自带 chown 65532）；
    # 组仍是 floatctf，因为 helper socket 是 0750 root:floatctf。
    local fctf_gid
    fctf_gid="$(getent group "$FCTF_USER" | cut -d: -f3)"
    [ -n "$fctf_gid" ] || die "无法解析 $FCTF_USER 组 GID"

    local d
    for d in image/api web config/caddy data/postgres data/rustfs data/caddy data/caddy-config logs/rustfs runtime; do
        mkdir -p "$FLOATCTF_HOME/$d"
    done
    chown root:"$FCTF_USER" "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    local run_dir
    for run_dir in data logs runtime; do
        chown -R "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/$run_dir" >/dev/null 2>&1 || true
    done
    chown -R root:"$FCTF_USER" "$FLOATCTF_HOME/image" "$FLOATCTF_HOME/web" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/image" "$FLOATCTF_HOME/image/api" "$FLOATCTF_HOME/web" >/dev/null 2>&1 || true
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/config" >/dev/null 2>&1 || true
    ok "布局就绪: $FLOATCTF_HOME/{image/api,web,config/caddy,data/{postgres,rustfs,caddy,caddy-config},logs/rustfs,runtime}"

    if [ ! -f "$FLOATCTF_HOME/.initialized" ]; then
        printf 'FloatCTF host initialized at %s by %s\n' "$(date -Is 2>/dev/null || date)" "${SUDO_USER:-root}" \
            > "$FLOATCTF_HOME/.initialized"
        chown "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/.initialized"
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
        ensure_service_group
        ok "开发宿主初始化完成（docker/nftables/WireGuard/转发/br_netfilter/服务组 就绪）"
    else
        check_user_layout
        ok "生产宿主初始化完成（docker/nftables/WireGuard/转发/br_netfilter/组/布局 就绪）"
    fi
}

# ============================================================================
# 第二阶段：获取 release 产物（6 个 URL）
# ============================================================================
download_url() { # url dest
    info "下载: $1"
    curl -fL --retry 3 --connect-timeout 30 -o "$2" "$1" \
        || die "下载失败: $1（这是 fake 占位地址，替换为真实 release 地址或经 --*-url 传入）"
}

# 内置的 DEFAULT_*_URL 指向 v0.0.0-fake 占位 tag（真实 release 发布后才会替换）。
# 不先拦一道的话，直接 `sudo bash install.sh` 会停在第 4 个 URL 的 curl 失败上，
# 看起来像网络/防火墙问题。这里在动手前就快速失败，并列出该传哪个参数。
ensure_real_release_urls() {
    local placeholder="releases/download/v0.0.0-fake"
    local missing=()
    [[ "$API_URL" == *"$placeholder"* ]] && missing+=("--api-url（或 FLOATCTF_API_URL）")
    [[ "$HELPER_URL" == *"$placeholder"* ]] && missing+=("--helper-url（或 FLOATCTF_HELPER_URL）")
    [[ "$WEB_URL" == *"$placeholder"* ]] && missing+=("--web-url（或 FLOATCTF_WEB_URL）")
    [[ "$MIGRATE_URL" == *"$placeholder"* ]] && missing+=("--migrate-url（或 FLOATCTF_MIGRATE_URL）")
    [[ "$FRONTEND_MANAGER_URL" == *"$placeholder"* ]] && missing+=("--frontend-manager-url（或 FLOATCTF_FRONTEND_MANAGER_URL）")
    [[ "$OPS_URL" == *"$placeholder"* ]] && missing+=("--ops-url（或 FLOATCTF_OPS_URL）")
    if [ "${#missing[@]}" -gt 0 ]; then
        die "缺少真实 release 地址：内置地址是 v0.0.0-fake 占位（仓库还没有对应发布产物）。请补：${missing[*]}"
    fi
}

download_release() {
    ensure_real_release_urls
    info "──── 第二阶段：下载 release 产物（6 URL）────"
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

    # 5) 前端管理器（安装后落到 $FLOATCTF_HOME/frontend.sh，运维无需源码签出）
    download_url "$FRONTEND_MANAGER_URL" "$TMP_STAGE_DIR/frontend.sh"
    chmod 0755 "$TMP_STAGE_DIR/frontend.sh"
    bash -n "$TMP_STAGE_DIR/frontend.sh" || die "下载的 frontend.sh 语法无效（产物损坏？）"

    # 6) 运维工具（backup.sh / restore.sh / db/migrate.sh + db/migrations/）：
    #    升级既有安装所需的迁移器与备份工具都随 release 分发，宿主无需源码签出。
    mkdir -p "$TMP_STAGE_DIR/ops"
    download_url "$OPS_URL" "$TMP_STAGE_DIR/ops-tools.tar.gz"
    tar xzf "$TMP_STAGE_DIR/ops-tools.tar.gz" -C "$TMP_STAGE_DIR/ops" \
        || die "解压 ops-tools 失败"
    validate_ops_tools "$TMP_STAGE_DIR/ops"

    ok "release 产物就绪: $TMP_STAGE_DIR"
    PKG_DIR="$TMP_STAGE_DIR"
}

# ops-tools.tar.gz 的顶层布局契约（与 release 打包脚本一致）：
#   backup.sh  restore.sh  db/migrate.sh  db/migrations/<*.sql>
# 任一项缺失都直接失败：带着残缺的运维工具继续部署，会让"升级=重新装一遍"
# 这条唯一受支持的升级路径在后续时刻静默失效。
validate_ops_tools() { # $1 = 解压根
    local root="$1" missing=()
    [ -f "$root/backup.sh" ] || missing+=("backup.sh")
    [ -f "$root/restore.sh" ] || missing+=("restore.sh")
    [ -f "$root/db/migrate.sh" ] || missing+=("db/migrate.sh")
    if [ ! -d "$root/db/migrations" ] \
        || [ -z "$(find "$root/db/migrations" -maxdepth 1 -type f -name '*.sql' -print -quit 2>/dev/null)" ]; then
        missing+=("db/migrations/*.sql")
    fi
    if [ "${#missing[@]}" -gt 0 ]; then
        die "ops-tools 产物布局不完整，缺少：${missing[*]}（期望顶层 backup.sh / restore.sh / db/migrate.sh / db/migrations/*.sql）"
    fi
    ok "运维工具就绪（backup.sh / restore.sh / db/migrate.sh / db/migrations）"
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
        # 稳定性：内存上限 + allkeys-lru（Redis 仅承载可重建数据）
        command: ["redis-server", "--appendonly", "yes", "--maxmemory", "1gb", "--maxmemory-policy", "allkeys-lru"]
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
            # 真实就绪探测：TCP 开放 ≠ S3/HTTP 层可用。此前仅 `nc -z` 导致 API 在
            # RustFS 的 S3 层就绪前初始化 bucket → "init rustfs failed: service error"
            # → panic → crash-loop（Phase 13 实测）。镜像内只有 BusyBox nc（无 curl），
            # 因此用 nc 发一个真实 HTTP 请求读 /health；stdin 需保持打开否则 nc 立刻
            # 半关连接读不到响应（`sleep 1` 即为此）。timeout 必须 > sleep+nc 超时。
            test: ["CMD-SHELL", "{ printf 'GET /health HTTP/1.1\\r\\nHost: 127.0.0.1:9000\\r\\nConnection: close\\r\\n\\r\\n'; sleep 1; } | nc -w 3 127.0.0.1 9000 | head -1 | grep -q ' 200 '"]
            interval: 10s
            timeout: 10s
            retries: 12
            start_period: 30s

    api:
        image: floatctf/api:${VERSION:?VERSION 必须在 .env 设置}
        container_name: floatctf-api
        restart: unless-stopped
        user: "${FLOATCTF_UID:-65532}:${FLOATCTF_GID:?FLOATCTF_GID 必须在 .env 设置}"
        cap_drop:
            - ALL
        security_opt:
            - no-new-privileges:true
        read_only: true
        # 稳定性：抬高文件描述符上限（此前 soft 1024）
        ulimits:
            nofile:
                soft: 65536
                hard: 65536
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
            # bootstrap 引导页（apps/web 产物）。
            - ${FLOATCTF_HOME}/web:/srv-web:ro
            # 已安装前端（版本化制品 + registry.json）。Caddy 以 /__floatctf/frontends/*
            # 同源提供；只读挂载——前端资产由 frontend.sh 维护，运行期不可变。
            # 注意：必须挂在 /srv **之外**的兄弟路径。Docker 需要先在容器内创建
            # 挂载点目录，而 /srv（runtime 挂载）是只读的 → 嵌套挂载会以
            # "Read-only file system" 失败，Caddy 整体起不来。
            - ${FLOATCTF_HOME}/frontends:/srv-frontends:ro
            # 挂载 API 的 work_dir 根（宿主 ${FLOATCTF_HOME}/runtime == 容器
            # /var/lib/floatctf/runtime）到 /srv；Caddyfile 里 root 是 /srv/challenges，
            # 于是附件根恒等于 CHALLENGES_DIR（{{WORK_DIR}}/challenges），与 dev 同构。
            - ${FLOATCTF_HOME}/runtime:/srv:ro
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
# 与 JWT 主密钥解耦的两个派生根（可选；空值 = 回落 jwt_secret，启动时会告警）：
#   awd_root_key       —— AWD/AWDP flag 与实例密钥的 HKDF 根
#   internal_token_key —— AWDP 判题容器 INTERNAL_TOKEN 的派生根（会下发进容器）
# 全新安装由 installer 生成独立随机值；**既有安装保持为空**（不改变已派生 flag），
# 需要轮换时再显式填值。
awd_root_key = "${AWD_ROOT_KEY}"
internal_token_key = "${INTERNAL_TOKEN_KEY}"

[redis]
url = "redis://redis:6379/"

[realtime]
channel = "floatctf:realtime"

[awd]
network_runtime = "helper"
# canonical 运行时镜像 ref（= release.yml 推送到 GHCR 的同一组）：
#   ${RUNTIME_IMAGE_REGISTRY}/awd-flagserver:${VERSION} 等；awdp 已扁平化（无 infra/ 段）。
# 升级时管理员的自定义值会被保留（见 preserve_custom_runtime_images）。
flagserver_image = "${RUNTIME_IMAGE_REGISTRY}/awd-flagserver:${VERSION}"
judgeserver_image = "${RUNTIME_IMAGE_REGISTRY}/awd-judgeserver:${VERSION}"
platform_internal_url = "http://10.42.8.2:${API_PORT}"
platform_internal_network = "fctf-platform-control"

[awdp]
practice_judgeserver_image = "${RUNTIME_IMAGE_REGISTRY}/awdp-judgeserver:${VERSION}"
practice_network_subnet = "10.42.2.0/23"
practice_judge_ip = "10.42.2.2"
network_pool = "10.43.0.0/16"
event_netmask = 24
platform_internal_url = "http://10.42.8.2:${API_PORT}"

[registry]
image_prefix = "floatctf"
push = false
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

    # 已安装前端的本地注册表：必须**不被永久缓存**（新装前端后要立刻可见）。
    @frontend_registry path /__floatctf/frontends/registry.json
    handle @frontend_registry {
        root * /srv-frontends
        header Cache-Control "no-store"
        header Content-Type "application/json"
        rewrite * /registry.json
        file_server
    }

    # 版本化前端资产（/__floatctf/frontends/<id>/<version>/...）：
    # 同源、路径确定、内容不可变 → 可以长缓存。不得遮蔽 /api（/api/* 先匹配）。
    handle_path /__floatctf/frontends/* {
        root * /srv-frontends
        header Cache-Control "public, max-age=31536000, immutable"
        header X-Content-Type-Options nosniff
        file_server {
            # 注册表写入锁等内部文件不外泄。
            hide .registry.lock .staging-*
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
        root * /srv-web
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
        python3 \
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
# 本脚本安装到 $FCTF_ROOT/uninstall.sh（由 scripts/install.sh 每次成功部署自动安装），
# 必须能在用户删除 Git 签出后独立工作：绝不依赖仓库相对路径 / scripts/install.sh /
# 源码 / mise / cargo / pnpm / git / chore / docs。仅依赖宿主既有工具：
#   systemctl, systemd, docker, docker compose, nft, iptables, ip, wg,
#   usermod/userdel/groupdel, rm/install/find/cp/trap。
#
# 两个模式：
#   sudo $FCTF_ROOT/uninstall.sh            SAFE UNINSTALL —— 移除可运行应用
#                                               （systemd、生产 Compose 容器/赛事资源、API image、
#                                               bootstrap web 资产），但保留可恢复状态：
#                                               data/{postgres,rustfs,caddy,caddy-config}, config/, .env,
#                                               runtime/, logs/, .initialized, 本卸载脚本，
#                                               **以及 frontends/（已安装前端 + registry.json）
#                                               与 frontend.sh（前端管理器）** ——
#                                               这样重新部署能恢复同一套前端集合。
#                                               语义：deploy → safe uninstall → deploy 应恢复相同的
#                                               应用数据与密钥（用户/赛事/数据仍在）。
#                                               安全守卫：数据库会被保留，但 AWD/AWDP 运行时会被销毁。
#                                               若存在**进行中**的 AWD 赛事或 AWDP run（数据库仍在运行
#                                               且可查询），默认拒绝卸载（避免「runtime 没了、库里
#                                               赛事还写着 running」的不一致状态）。确认要拆时用
#                                               --force 跳过守卫。
#   sudo $FCTF_ROOT/uninstall.sh --force    SAFE UNINSTALL + 跳过活跃运行时守卫
#                                               （**数据库仍保留**，进行中的赛事 runtime 会被销毁；
#                                                只应在已确认可以放弃该场次时使用）
#   sudo $FCTF_ROOT/uninstall.sh --purge    PERMANENT 删除全部 FloatCTF 自有数据
#                                               （PG/RustFS 数据、config、secrets、runtime、
#                                               日志、API image/build context、bootstrap web、
#                                               **frontends/ 已安装前端与注册表、frontend.sh**、
#                                               compose、systemd 单元、
#                                               动态赛事资源、sysctl/modules 文件、helper 用户、
#                                               安装根目录、本脚本自身）。需输入确认文本
#                                               "PURGE FLOATCTF"（除非 --yes）。
#
# 共享宿主依赖永不卸载：Docker / docker compose / nftables 包 / wireguard-tools /
# iproute2 / systemd。绝不触碰无关 Docker 对象 / WG 接口 / nftables 状态 / 路由 /
# libvirt / Incus / 其他应用。
#
set -Eeuo pipefail

# 安装根：安装时 install.sh 用 sed 把 __FLOATCTF_HOME__ 固化为实际根；
# 运行时仍可用环境变量 FLOATCTF_HOME 或 FCTF_ROOT 覆盖。
FCTF_ROOT="${FLOATCTF_HOME:-${FCTF_ROOT:-__FLOATCTF_HOME__}}"
FCTF_USER="floatctf"
FCTF_HELPER_USER="floatctf-helper"
HELPER_INSTALL_PATH="/usr/local/libexec/floatctf-helper"

info() { printf '%s[INFO]%s %s\n'  "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n'  "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
warn() { printf '%s[WARN]%s %s\n'  "$(tput setaf 3 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n'  "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

# ── 根权限 ────────────────────────────────────────────────────────────────────
require_root() {
    [ "$(id -u)" -eq 0 ] || die "需要 root。请改用: sudo $FCTF_ROOT/uninstall.sh"
}

# ── 工具可用性（宿主既有；缺失则报错，不尝试安装）──────────────────────────────
MODE="safe"
PURGE_YES=0
SAFE_FORCE=0
SELF_TMP=""

usage() {
    cat <<EOF
用法：
  sudo $FCTF_ROOT/uninstall.sh             安全卸载（保留 PG/RustFS 数据、config、secrets）
  sudo $FCTF_ROOT/uninstall.sh --force     安全卸载并跳过「活跃 AWD/AWDP 运行时」守卫
                                           （数据库仍保留；进行中的赛事 runtime 会被销毁）
  sudo $FCTF_ROOT/uninstall.sh --purge     永久删除全部 FloatCTF 自有数据（需确认 PURGE FLOATCTF）
  sudo $FCTF_ROOT/uninstall.sh --purge --yes  跳过确认（仅限非交互 purge）
  sudo $FCTF_ROOT/uninstall.sh --help

说明：
  安全卸载会保留数据库，但会销毁 AWD/AWDP 运行时（容器、赛事网络、WireGuard 接口、
  nftables 表）。若数据库仍可查询且存在进行中的 AWD 赛事 / AWDP run，本脚本默认**拒绝**
  卸载并列出受影响的 id：请先结束或归档这些赛事，或确认放弃后加 --force。
  --purge 会连数据库一起删除，因此不做该守卫，但会打印销毁内容警告。
EOF
}

parse_args() {
    # 用 while 循环而非递归：递归版在参数耗尽时结尾返回非零，
    # 叠加 set -e 会导致脚本静默 exit 1、零输出（--purge 实测复发）。
    while [ "$#" -ge 1 ]; do
        case "$1" in
            --purge) MODE="purge";;
            --yes)   PURGE_YES=1;;
            --force) SAFE_FORCE=1;;
            -h|--help) usage; exit 0 ;;
            *) die "未知参数: $1（--help 查看用法）";;
        esac
        shift
    done
    return 0
}

# ── 自删除安全（§18）：purge 会把安装根（含本脚本）删掉，Bash 不能继续读
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

# ============================================================================
# 活跃 AWD/AWDP 运行时守卫
# ============================================================================
#
# 背景：safe_uninstall **刻意保留数据库**，却会删除全局 nft 表 floatctf_awd、
# 所有 fawg_* 接口、所有 fctf-awd-*/fctf-awdp-* 网络与 FlagServer/JudgeServer
# 容器。若此时赛事正在进行，结果就是「runtime 没了、库里赛事还是 running」的
# 不一致状态：恢复后既看不到成绩也无法续赛。因此默认拒绝，只允许显式 --force。
#
# 判据（只读，绝不改库）：
#   AWD  进行中 = awd_events.status NOT IN
#                 ('draft','configuring','finished','archived','deploy_failed','verification_failed')
#   AWDP 进行中 = awdp_runs.finished_at IS NULL
# 表/列名对照 apps/api/src/entity/awd_events.rs（table_name="awd_events"，字段
# status: AwdEventStatus）与 apps/api/src/entity/awdp_runs.rs
# （table_name="awdp_runs"，字段 finished_at: Option<DateTimeWithTimeZone>）。

# 只读 SQL：以容器内 POSTGRES_USER/POSTGRES_DB 连接；SQL 通过位置参数传入，
# 避免在本脚本里嵌套引号。任何失败（容器不在、库未初始化、schema 缺失）
# 都以非零返回，由调用方决定如何降级。
pg_readonly_query() { # sql
    ( cd "$FCTF_ROOT" \
        && docker compose -f compose.prod.yml exec -T postgres \
            sh -c 'psql -X -q -A -t -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "$1"' _ "$1" 2>/dev/null )
}

# 探测活跃运行时。返回码：
#   0 = 存在进行中的 AWD/AWDP 运行时（详情已打印）
#   1 = 确无进行中的运行时
#   2 = 无法判定（已 warn；数据库不可查询时按"无活跃"处理，不阻断卸载）
probe_active_runtime() {
    if [ ! -f "$FCTF_ROOT/compose.prod.yml" ] || [ ! -f "$FCTF_ROOT/.env" ]; then
        warn "无法判定活跃赛事状态：缺少 $FCTF_ROOT/compose.prod.yml 或 $FCTF_ROOT/.env"
        return 2
    fi
    if ! command -v docker >/dev/null 2>&1; then
        warn "无法判定活跃赛事状态：无 docker 命令"
        return 2
    fi
    local cid
    cid="$( cd "$FCTF_ROOT" && docker compose -f compose.prod.yml ps -q postgres 2>/dev/null | head -1 )" || cid=""
    if [ -z "$cid" ]; then
        warn "PostgreSQL 未运行：无法判定活跃赛事状态（平台已停 -> 无运行中赛事）"
        return 2
    fi

    local awd_count awdp_count
    if ! awd_count="$(pg_readonly_query "SELECT count(*) FROM awd_events WHERE status NOT IN ('draft','configuring','finished','archived','deploy_failed','verification_failed')")" \
        || [ -z "$awd_count" ]; then
        warn "无法查询 AWD 赛事状态（数据库不可达或 schema 未初始化）"
        return 2
    fi
    if ! awdp_count="$(pg_readonly_query "SELECT count(*) FROM awdp_runs WHERE finished_at IS NULL")" \
        || [ -z "$awdp_count" ]; then
        warn "无法查询 AWDP run 状态（数据库不可达或 schema 未初始化）"
        return 2
    fi

    case "$awd_count" in ''|*[!0-9]*) warn "AWD 赛事计数异常: $awd_count"; return 2 ;; esac
    case "$awdp_count" in ''|*[!0-9]*) warn "AWDP run 计数异常: $awdp_count"; return 2 ;; esac
    if [ "$awd_count" -eq 0 ] && [ "$awdp_count" -eq 0 ]; then
        return 1
    fi

    echo ""
    echo "!!!! 检测到进行中的 AWD/AWDP 运行时 !!!!"
    if [ "$awd_count" -gt 0 ]; then
        echo "  AWD 赛事（进行中）: $awd_count 个"
        echo "    id: $(pg_readonly_query "SELECT string_agg(id::text, ', ') FROM awd_events WHERE status NOT IN ('draft','configuring','finished','archived','deploy_failed','verification_failed')" 2>/dev/null || true)"
    fi
    if [ "$awdp_count" -gt 0 ]; then
        echo "  AWDP run（未结束）: $awdp_count 个"
        echo "    id: $(pg_readonly_query "SELECT string_agg(id::text, ', ') FROM awdp_runs WHERE finished_at IS NULL" 2>/dev/null || true)"
    fi
    echo ""
    return 0
}

# 守卫：存在活跃运行时时返回非零（并解释后果）。无法判定时放行（只 warn）。
active_runtime_guard() {
    local rc=0
    probe_active_runtime || rc=$?
    case "$rc" in
        0)
            echo "原因：数据库会被保留，但 AWD/AWDP 运行时（FlagServer/JudgeServer/GameBox 容器、"
            echo "      赛事 Docker 网络、fawg_* WireGuard 接口、floatctf_awd* nft 表）会被销毁 ——"
            echo "      这会留下「赛事仍在 running / run 未结束，但 runtime 已消失」的不一致状态。"
            echo ""
            echo "处理：先结束或归档这些赛事（管理端结束赛事 / 结束 AWDP run），再重新运行卸载；"
            echo "      若确认放弃该场次，用 sudo $FCTF_ROOT/uninstall.sh --force"
            return 1
            ;;
        2)  return 0 ;;
        *)  return 0 ;;
    esac
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
    info "── 移除可运行应用产物（保留 data/config/.env/runtime/logs 与已安装前端）──"
    # 刻意**不**删除：$FCTF_ROOT/frontends（已安装前端 + registry.json）与
    # $FCTF_ROOT/frontend.sh。它们是运维/第三方投入的"已安装状态"，
    # 保留才能让"卸载 → 重新部署"恢复同一套前端集合（见 docs/frontend/ARCHITECTURE.md）。
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

    # 0. 活跃运行时守卫 —— 必须排在**任何**拆除动作（含 stop_api_first）之前：
    #    数据库会被保留，但运行时会消失，进行中的赛事/run 会变成不一致状态。
    if [ "$SAFE_FORCE" = "1" ]; then
        warn "--force：跳过活跃运行时守卫（数据库保留；进行中的赛事 runtime 会被销毁）"
    elif ! active_runtime_guard; then
        die "检测到进行中的 AWD/AWDP 运行时，已拒绝安全卸载（未改动任何内容）；用 --force 强制卸载"
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
  已安装前端      : $FCTF_ROOT/frontends（含 registry.json，第三方前端与版本一并保留）
  前端管理器      : $FCTF_ROOT/frontend.sh

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
    # 删除安装根/宿主身份之前就退出（历史 purge 曾观察到目录与账号残留）。
    if [ "$removed" = "0" ]; then
        ok "无 FloatCTF sysctl/modules 文件（或已不存在）"
    fi
}

purge_remove_user() {
    info "── 移除 FloatCTF 宿主身份（helper 用户 + 历史账号 + 组）──"
    if id "$FCTF_HELPER_USER" >/dev/null 2>&1; then
        userdel "$FCTF_HELPER_USER" 2>/dev/null \
            && ok "已移除用户 $FCTF_HELPER_USER" \
            || warn "userdel $FCTF_HELPER_USER 失败（可能仍有进程占用）"
    else
        ok "用户 $FCTF_HELPER_USER 不存在"
    fi

    # v2 起安装器不再创建 floatctf 用户（容器用数值 uid，宿主不需要该身份）。
    # 若存在，只可能是历史安装或 dev setup 残留：确认是 system 账号 + nologin +
    # home 为安装根（或 v2 之前的默认根 /home/floatctf）才删除，避免误删同名账号。
    if id "$FCTF_USER" >/dev/null 2>&1; then
        local home shell
        home=$(getent passwd "$FCTF_USER" | cut -d: -f6)
        shell=$(getent passwd "$FCTF_USER" | cut -d: -f7)
        if { [ "$home" = "$FCTF_ROOT" ] || [ "$home" = "/home/floatctf" ]; } \
            && [ "$shell" = "/usr/sbin/nologin" ]; then
            userdel -r "$FCTF_USER" 2>/dev/null && ok "已移除历史用户 $FCTF_USER" \
                || { warn "userdel $FCTF_USER 失败（可能仍有进程占用 $FCTF_ROOT/runtime）"; \
                     # 回退：仅移除配置但保留记录，避免误删
                     warn "保留用户记录；请确认无 floatctf 进程后重试 userdel -r floatctf"; }
        else
            warn "账户 $FCTF_USER 不匹配预期（home=$home shell=$shell），跳过删除"
        fi
    else
        ok "用户 $FCTF_USER 不存在（v2 起不再创建）"
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

    # purge 连数据库一起删除，因此**不需要**守卫（不存在"runtime 没了、库里还在"的
    # 不一致状态）；但进行中的赛事意味着正在进行的比赛及其全部数据被销毁，
    # 必须显式、醒目地警告（不额外加交互确认，既有 purge_confirm 已足够）。
    if probe_active_runtime; then
        warn "════════════════════════════════════════════════════════════════"
        warn "注意：存在进行中的 AWD/AWDP 运行时 —— purge 将**永久销毁**以下内容："
        warn "  - 全部 PostgreSQL 数据（赛事/题目/用户/成绩，不可恢复）"
        warn "  - FlagServer / JudgeServer / GameBox 容器与 AWDP 判题容器"
        warn "  - 赛事 Docker 网络（fctf-awd-* / fctf-awdp-*）、fawg_* WireGuard 接口"
        warn "  - floatctf_awd* nftables 表与 Docker 反欺骗放行规则"
        warn "  - RustFS 数据、配置与密钥（$FCTF_ROOT/config、$FCTF_ROOT/.env）"
        warn "════════════════════════════════════════════════════════════════"
    fi

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
    echo "  - 已删除全部已安装前端与注册表（$FCTF_ROOT/frontends）以及前端管理器 frontend.sh。"
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
        # 自删除安全：在删除安装根（含自身）之前，把脚本复制到 /tmp 免责续跑。
        # 续跑模式里该函数只设置 EXIT trap 并返回；否则 exec 已替换当前进程（不返回）。
        run_purge_via_temp
        purge_run
    else
        safe_uninstall
    fi
}

# 本文件由 install.sh 写出到 $FCTF_ROOT/uninstall.sh 后就地执行。
main "$@"
UNINSTALL_EOF
    # 固化安装根：把 __FLOATCTF_HOME__ 占位替换为实际路径（环境变量仍可覆盖）。
    sed -i "s|__FLOATCTF_HOME__|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/uninstall.sh"
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
    # R3 纵深防御：即使有人绕过 run_init 直接调用部署阶段，也要在改动任何文件前
    # 确认 Python tomllib 可用（迁移器依赖它）。
    check_python_tomllib
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
    mkdir -p "$FLOATCTF_HOME/config/caddy" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" "$FLOATCTF_HOME/logs/rustfs"
    local site_address fctf_uid fctf_gid
    site_address=$(env_get SITE_ADDRESS "")
    [ -n "$site_address" ] || die "生产部署必须设置 SITE_ADDRESS（例如 ctf.example.com），并将该域名 DNS 指向本机"
    # API 容器身份：uid 是常量（与镜像 USER 65532:65532 对齐），gid 取 floatctf 组的 GID。
    fctf_uid="$FCTF_UID"
    fctf_gid="$(getent group "$FCTF_USER" | cut -d: -f3)"
    [ -n "$fctf_gid" ] || die "无法解析 $FCTF_USER 组 GID"
    if [ ! -f "$ENV_FILE" ]; then
        : > "$ENV_FILE"
        env_set POSTGRES_USER "${POSTGRES_USER:-postgres}"
        env_set POSTGRES_DB "${POSTGRES_DB:-floatctf_db}"
        env_set POSTGRES_PASSWORD "${POSTGRES_PASSWORD:-$(openssl rand -hex 16)}"
        env_set RUSTFS_ACCESS_KEY "${RUSTFS_ACCESS_KEY:-rustfsadmin}"
        env_set RUSTFS_SECRET_KEY "${RUSTFS_SECRET_KEY:-$(openssl rand -hex 24)}"
        env_set JWT_SECRET "${JWT_SECRET:-$(openssl rand -base64 32)}"
        # 全新安装：生成与 JWT 主密钥**不同**的派生根，避免"判题容器被攻陷 = 可伪造 JWT"
        # （风险清单 #7）。既有安装不补这两项 —— 补了等于强制轮换 AWD flag 与判题令牌。
        env_set AWD_ROOT_KEY "${AWD_ROOT_KEY:-$(openssl rand -base64 32)}"
        env_set INTERNAL_TOKEN_KEY "${INTERNAL_TOKEN_KEY:-$(openssl rand -base64 32)}"
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
    chown -R "$fctf_uid":"$fctf_gid" "$FLOATCTF_HOME/data" "$FLOATCTF_HOME/logs" "$FLOATCTF_HOME/runtime" 2>/dev/null || true
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

# ── 升级保护：管理员自定义的 AWD/AWDP 镜像引用（§2.5）────────────────────────
# prepare_configs 每次都用模板重渲染 config/floatctf.toml；没有保护的话，管理员
# 指向自建 registry / 私有镜像的配置会在升级时被静默冲掉（"升级后 AWD 忽然拉
# 官方镜像"）。这里只处理 TOML 里那三个 key（数据库里没有任何镜像设置）。
toml_get_string_key() { # <file> <section> <key>：打印该 key 的字符串值（去引号）
    local file="$1" section="$2" key="$3"
    [ -f "$file" ] || return 1
    # 用字符串比较而不是正则拼 section：`[awd]` 里的方括号在 ERE 里是字符类。
    awk -v want="[$section]" -v key="$key" '
        /^[[:space:]]*\[/ {
            line = $0
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
            inseg = (line == want)
            next
        }
        inseg && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
            line = $0
            sub(/^[^=]*=[[:space:]]*/, "", line)
            sub(/[[:space:]]*(#.*)?$/, "", line)
            gsub(/^"|"$/, "", line)
            print line
            exit
        }
    ' "$file"
}

toml_set_string_key() { # <file> <section> <key> <value>
    local file="$1" section="$2" key="$3" value="$4" tmp="$1.tmp.$$"
    awk -v want="[$section]" -v key="$key" -v val="$value" '
        /^[[:space:]]*\[/ {
            line = $0
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
            inseg = (line == want)
        }
        !done && inseg && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
            print key " = \"" val "\""
            done = 1
            next
        }
        { print }
    ' "$file" > "$tmp" || return 1
    mv "$tmp" "$file"
}

# 已知 stock 形态（安装器历史默认 + 现在的 canonical 默认）= 可安全迁移到 canonical；
# 其它任何值都视为管理员自定义 → 保留 + 告警。
is_stock_runtime_image() { # <image-ref>
    case "$1" in
        floatctf/awd-flagserver:*|floatctf/awd-judgeserver:*|floatctf/infra/awdp-judgeserver:*|\
        ghcr.io/floatctf/awd-flagserver:*|ghcr.io/floatctf/awd-judgeserver:*|ghcr.io/floatctf/awdp-judgeserver:*)
            return 0
            ;;
        *) return 1 ;;
    esac
}

# 把既有 floatctf.toml 里**自定义**的三个镜像引用写回新渲染的文件。
#   --reset-runtime-images → 忽略既有值（强制 canonical），被丢弃的自定义值告警
#   --keep-runtime-images  → 无条件保留既有值（连 stock 也保留）
#   默认                   → stock 迁移到 canonical；非 stock 保留 + warn
preserve_custom_runtime_images() { # <旧 toml 快照> <新渲染的 toml>
    local old="$1" new="$2" spec section key existing
    for spec in "awd flagserver_image" "awd judgeserver_image" "awdp practice_judgeserver_image"; do
        section="${spec%% *}"
        key="${spec##* }"
        existing="$(toml_get_string_key "$old" "$section" "$key" 2>/dev/null || true)"
        [ -n "$existing" ] || continue
        if [ "$RESET_RUNTIME_IMAGES" = "1" ]; then
            is_stock_runtime_image "$existing" || \
                warn "--reset-runtime-images：丢弃 [$section] $key 的自定义值 $existing（改用 canonical 默认）"
            continue
        fi
        if [ "$KEEP_RUNTIME_IMAGES" = "1" ] || ! is_stock_runtime_image "$existing"; then
            toml_set_string_key "$new" "$section" "$key" "$existing" \
                || die "写回自定义运行时镜像引用失败: [$section] $key"
            warn "升级保护：保留管理员自定义的运行时镜像 [$section] $key = $existing（改回 canonical 默认请加 --reset-runtime-images）"
        fi
    done
}

prepare_configs() {
    set -a
    # ENV_FILE is generated at runtime by this installer.
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
    export FLOATCTF_HOME
    # 升级保护：渲染前先快照既有 floatctf.toml（渲染会覆盖它）。快照含 JWT_SECRET
    # 等敏感值 → 必须是 0600（umask 077 建文件 + 显式 chmod 双重保险），绝不能
    # 因为 cp 默认 0644 把密钥泄露给本机其他用户。
    local prev_toml=""
    if [ -f "$FLOATCTF_HOME/config/floatctf.toml" ]; then
        prev_toml="$FLOATCTF_HOME/.floatctf.toml.prev"
        ( umask 077; cp "$FLOATCTF_HOME/config/floatctf.toml" "$prev_toml" )
        chmod 0600 "$prev_toml"
    fi
    # 先替换模板里的 FLOATCTF_HOME 占位符，再渲染。
    sed "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/.floatctf.toml.tmpl" > "$FLOATCTF_HOME/.floatctf.toml.tmpl.real"
    render "$FLOATCTF_HOME/.floatctf.toml.tmpl.real" "$FLOATCTF_HOME/config/floatctf.toml"
    if [ -n "$prev_toml" ]; then
        preserve_custom_runtime_images "$prev_toml" "$FLOATCTF_HOME/config/floatctf.toml"
        rm -f "$prev_toml"
    fi
    cp "$FLOATCTF_HOME/.Caddyfile.tmpl" "$FLOATCTF_HOME/config/caddy/Caddyfile"
    rm -f "$FLOATCTF_HOME/.floatctf.toml.tmpl.real"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy"
    chmod 750 "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy"
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/caddy/Caddyfile"
    chmod 640 "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/caddy/Caddyfile"
    chown -R "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" 2>/dev/null || true
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

    # ── 前端管理器：先安装它，后面的前端安装复用它的注册表语义与安全校验 ──
    install -m 0755 "$PKG_DIR/frontend.sh" "$FLOATCTF_HOME/frontend.sh"
    chown root:root "$FLOATCTF_HOME/frontend.sh"
    bash -n "$FLOATCTF_HOME/frontend.sh" || die "frontend.sh 语法无效（产物损坏？）"
    ok "前端管理器已安装: $FLOATCTF_HOME/frontend.sh"

    # ── bootstrap 引导页（apps/web 产物；**不含**任何官方 UI 实现）──
    [ -d "$PKG_DIR/web/bootstrap" ]         || die "web-dist 归档缺少 bootstrap/ 目录（旧格式产物？请使用与新安装器配套的 release）"
    mkdir -p "$FLOATCTF_HOME/web"
    find "$FLOATCTF_HOME/web" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + 2>/dev/null || true
    cp -a "$PKG_DIR/web/bootstrap/." "$FLOATCTF_HOME/web/"
    chown -R root:root "$FLOATCTF_HOME/web"
    ok "bootstrap 静态页已安装: $FLOATCTF_HOME/web"

    install_platform_frontends
    install -m 0644 "$PKG_DIR/merged.sql" "$FLOATCTF_HOME/merged.sql"
    install_ops_tools
    mkdir -p "$FLOATCTF_HOME/runtime"
    chown "$FCTF_USER":"$FCTF_USER" "$FLOATCTF_HOME/runtime"
    fix_infra_ownership
    ok "产物装配完成（API image + helper + bootstrap + 前端管理器 + 已安装前端 + merged.sql + 运维工具）"
}

# 安装 ops-tools（backup.sh / restore.sh / db/migrate.sh + db/migrations/）。
#
# 布局契约（$FLOATCTF_HOME）：
#   backup.sh  restore.sh           运维入口（root:root 0755）
#   db/migrate.sh                   forward-only 迁移器（migrate.sh 以自身
#                                   SCRIPT_DIR 为锚点找 migrations/ 与 merged.sql，
#                                   因此这两者必须与它同级放在 db/ 下）
#   db/migrations/*.sql             release 的全部迁移
#   db/merged.sql                   已装配 merged.sql 的副本（fresh bootstrap 语义）
# 所有 shell 脚本安装后都过一遍 `bash -n`：语法坏掉的迁移器在升级时才会暴露，
# 而那时运维已经不在安装窗口里了。
install_ops_tools() {
    info "── 装配运维工具（backup.sh / restore.sh / db/migrate.sh）──"
    install -m 0755 "$PKG_DIR/ops/backup.sh" "$FLOATCTF_HOME/backup.sh"
    install -m 0755 "$PKG_DIR/ops/restore.sh" "$FLOATCTF_HOME/restore.sh"
    chown root:root "$FLOATCTF_HOME/backup.sh" "$FLOATCTF_HOME/restore.sh"

    mkdir -p "$FLOATCTF_HOME/db"
    install -m 0755 "$PKG_DIR/ops/db/migrate.sh" "$FLOATCTF_HOME/db/migrate.sh"
    # migrations/ 先清空再铺：release 是迁移的完整集合，残留的旧文件会让
    # migrate.sh 看到 release 之外的迁移（历史由 schema_migrations 校验，不应出现）。
    find "$FLOATCTF_HOME/db/migrations" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + 2>/dev/null || true
    mkdir -p "$FLOATCTF_HOME/db/migrations"
    local f
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        install -m 0644 "$f" "$FLOATCTF_HOME/db/migrations/$(basename "$f")"
    done < <(find "$PKG_DIR/ops/db/migrations" -maxdepth 1 -type f -name '*.sql' | sort)
    install -m 0644 "$PKG_DIR/merged.sql" "$FLOATCTF_HOME/db/merged.sql"
    chown root:root "$FLOATCTF_HOME/db" "$FLOATCTF_HOME/db/migrate.sh" "$FLOATCTF_HOME/db/merged.sql"
    chown -R root:root "$FLOATCTF_HOME/db/migrations"

    local script
    for script in "$FLOATCTF_HOME/backup.sh" "$FLOATCTF_HOME/restore.sh" "$FLOATCTF_HOME/db/migrate.sh"; do
        bash -n "$script" || die "安装的运维脚本语法无效: $script（ops-tools 产物损坏？）"
    done
    ok "运维工具已安装: backup.sh / restore.sh / db/migrate.sh（db/migrations/ $(find "$FLOATCTF_HOME/db/migrations" -maxdepth 1 -type f -name '*.sql' | wc -l | tr -d ' ') 个迁移）"
}

# 安装随 release 发布的前端（bootstrap/ 之外的 frontends/<id>/<version>/）。
#
# 关键语义（见 docs/frontend/ARCHITECTURE.md）：
#   - **只动 release 里带的前端**（当前仅 default）；第三方已安装前端与其版本目录、
#     以及注册表里第三方条目的 currentVersion 指针，一律保持不变。
#   - `default` 升级：装新版本 + 把 currentVersion 指到新版本（旧版本保留 → 可回滚）。
#   - **前端版本不可变**：同版本同内容 = 幂等重装（不重写资产）；同版本不同内容会被
#     前端管理器拒绝（资产带 immutable 长缓存）。Default UI 变了就必须升它的**前端版本号**
#     —— 平台版本与前端版本独立演进。
#   - `FRONTEND_ACTIVE`（数据库设置）与本流程无关，升级不会改动它。
install_platform_frontends() {
    local release_frontends="$PKG_DIR/web/frontends"
    if [ ! -d "$release_frontends" ]; then
        warn "web-dist 归档不含 frontends/ —— 平台不会安装任何前端（请检查 release 产物）"
        return 0
    fi

    local version_dir id version installed=0
    while IFS= read -r version_dir; do
        [ -n "$version_dir" ] || continue
        id="$(basename "$(dirname "$version_dir")")"
        version="$(basename "$version_dir")"
        [ -f "$version_dir/frontend.json" ] \
            || die "release 内前端制品缺少 frontend.json: $version_dir"

        info "安装 release 前端: $id@$version"
        if [ "$id" = "default" ]; then
            # default 始终随平台发布：受保护、可放在 release 之外被移除，并移动 current 指针。
            "$FLOATCTF_HOME/frontend.sh" install "$version_dir" \
                --platform --make-current \
                || die "安装平台前端失败: $id@$version"
        else
            # 其他由平台发布的前端：安装但不擅自改变其 current 指针。
            "$FLOATCTF_HOME/frontend.sh" install "$version_dir" \
                --platform \
                || die "安装平台前端失败: $id@$version"
        fi
        installed=$((installed + 1))
    done < <(find "$release_frontends" -mindepth 2 -maxdepth 2 -type d | sort)

    [ "$installed" -gt 0 ] || die "release 的 frontends/ 目录里没有任何前端制品"
    [ -f "$FLOATCTF_HOME/frontends/registry.json" ] \
        || die "前端注册表未生成: $FLOATCTF_HOME/frontends/registry.json"

    # 前端目录与注册表归 root（前端资产由 frontend.sh 设为全局只读，API 用户不可写）。
    # 以 root 运行时 chown 必然成功；非 root 场景（测试/自建根目录）只告警不中断。
    chown root:root "$FLOATCTF_HOME/frontends" "$FLOATCTF_HOME/frontends/registry.json" 2>/dev/null \
        || warn "无法把前端目录 chown 为 root:root（非 root 安装？前端功能不受影响）"
    chmod 0755 "$FLOATCTF_HOME/frontends" 2>/dev/null || true
    chmod 0644 "$FLOATCTF_HOME/frontends/registry.json" 2>/dev/null || true
    ok "已安装 $installed 个 release 前端（注册表: $FLOATCTF_HOME/frontends/registry.json）"
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
# - caddy：容器以 root 运行，属主仅归 API 身份便于管理（同生产 prepare_configs）。
# install.sh 不启动容器，直接 chown 即可（不触碰运行中的容器）。
fix_infra_ownership() {
    chown -R 10001:10001 "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    chgrp -R "$FCTF_USER" "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    chmod -R g+rX "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    find "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" -type d -exec chmod g+s {} + 2>/dev/null || true
    chown -R 999:999 "$FLOATCTF_HOME/data/postgres" 2>/dev/null || true
    chown -R "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" 2>/dev/null || true
}

install_systemd() {
    info "──── 部署：systemd 单元（helper / Compose stack / target）────"
    systemctl daemon-reload
    systemctl enable floatctf.target floatctf-infra.service floatctf-helper.service
    ok "systemd 单元已写出并 enable（未启动；用 systemctl start floatctf.target 启动）"
}

# ── 升级：既有数据库的新增迁移 ────────────────────────────────────────────────
#
# 为什么需要它：release 只发布 merged.sql，而 PostgreSQL 仅在**首次**初始化
# （空数据目录）时执行 /docker-entrypoint-initdb.d。既有安装因此无法靠 merged.sql
# 拿到新迁移 —— 唯一的受支持路径就是本函数：release 自带 db/migrate.sh +
# db/migrations/，以 forward-only、按版本幂等的方式补齐新迁移。
#
# 时机与前置：本函数在 stage_release 之后、ensure_platform_control_network 之前
# 调用；此时 compose.prod.yml 与 .env 都已写好，而平台尚未启动。fresh 安装
# （无 PG_VERSION）直接跳过：那种情况下 merged.sql 会在首次启动 postgres 时
# 建好**全部**表与 schema_migrations 记录。
apply_migrations() {
    info "──── 部署：数据库迁移（forward-only）────"
    if [ "${SKIP_MIGRATIONS:-0}" = "1" ]; then
        warn "--skip-migrations / FLOATCTF_SKIP_MIGRATIONS=1：跳过 migrate apply（平台可能因缺少新表/新列而启动失败）"
        return 0
    fi

    # 既有 cluster 判据：data/postgres 存在、非空、且含 PG_VERSION。
    # 三者任一不满足 = fresh 安装（initdb 尚未跑过）。
    # 安装器必须以 root 运行；data/postgres 归容器内 postgres(999) 且多为 0700，
    # 非 root 读不了 —— 此时**宁可失败也不猜**（把"读不到"误判成 fresh 会静默跳过迁移）。
    local pg_data="$FLOATCTF_HOME/data/postgres"
    local pg_entries=""
    if [ -d "$pg_data" ]; then
        pg_entries="$(ls -A "$pg_data" 2>/dev/null)" \
            || die "无法读取 $pg_data（权限不足）；无法判定 fresh/既有安装。请以 root 运行安装器"
    fi
    if [ -z "$pg_entries" ] || [ ! -f "$pg_data/PG_VERSION" ]; then
        info "fresh 数据库：由 merged.sql 完成 bootstrap，跳过 migrate apply"
        return 0
    fi

    [ -f "$FLOATCTF_HOME/db/migrate.sh" ] \
        || die "缺少 $FLOATCTF_HOME/db/migrate.sh（ops-tools 未装配？）；无法升级既有数据库"

    info "检测到既有 PostgreSQL cluster：仅启动 postgres 服务以应用新增迁移"
    docker compose --env-file "$ENV_FILE" -f "$FLOATCTF_HOME/compose.prod.yml" up -d postgres \
        || die "为应用迁移启动 postgres 失败（docker compose up -d postgres）"

    # 有界等待 healthcheck（90 × 2s = 180s）；失败即放弃部署，绝不在数据库
    # 半就绪时继续。
    local cid status attempt
    cid="$(docker compose --env-file "$ENV_FILE" -f "$FLOATCTF_HOME/compose.prod.yml" ps -q postgres 2>/dev/null | head -1)"
    [ -n "$cid" ] || die "无法解析 postgres 容器 id（docker compose ps -q postgres）"
    status=""
    for attempt in $(seq 1 90); do
        status="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$cid" 2>/dev/null || true)"
        [ "$status" = "healthy" ] && break
        sleep 2
    done
    [ "$status" = "healthy" ] \
        || die "postgres 健康检查未在 180s 内变为 healthy（当前: ${status:-unknown}）；平台**未启动**，请 journalctl 查看后重新运行本安装器"

    # migrate.sh 只从 FLOATCTF_CONFIG 的 [database].url 读取目标库，因此需要一份
    # 宿主可达的临时配置：容器内主机名 postgres 在宿主不可解析，改用回环 + 发布的
    # PG 端口（compose 把 ${POSTGRES_PORT} 绑在 127.0.0.1）。
    # 假设：.env 里的用户/密码/库名不含 URL 特殊字符（新装密码是 hex；若历史密码含
    # '@' ':' '/' '#' 等，需要改成百分号编码后手动执行 migrate.sh）。此处不做额外
    # 编码，保持可读、可复核。
    local db_user db_pass db_name pg_port tmp_cfg
    db_user="$(env_get POSTGRES_USER postgres)"
    db_pass="$(env_get POSTGRES_PASSWORD)"
    db_name="$(env_get POSTGRES_DB floatctf_db)"
    pg_port="$(env_get POSTGRES_PORT 5433)"
    [ -n "$db_pass" ] || die "无法从 $ENV_FILE 读取 POSTGRES_PASSWORD；无法连接数据库应用迁移"
    tmp_cfg="$(mktemp "${TMP_STAGE_DIR:?内部错误: TMP_STAGE_DIR 未设置}/migrate-config.XXXXXX.toml")"
    # umask 077 + 显式 chmod：临时配置含明文密码，只在本次迁移期间存在。
    ( umask 077
      printf '[database]\nurl = "postgres://%s:%s@127.0.0.1:%s/%s"\n' \
          "$db_user" "$db_pass" "$pg_port" "$db_name" > "$tmp_cfg" )
    chmod 0600 "$tmp_cfg"

    local migrate_rc=0
    info "应用迁移: FLOATCTF_CONFIG=<临时配置> bash $FLOATCTF_HOME/db/migrate.sh apply"
    FLOATCTF_CONFIG="$tmp_cfg" bash "$FLOATCTF_HOME/db/migrate.sh" apply || migrate_rc=$?
    # 无论成功失败都立即删除：密码不落盘、不残留。
    rm -f "$tmp_cfg"
    if [ "$migrate_rc" -ne 0 ]; then
        die "迁移应用失败（exit $migrate_rc）：平台**未启动**，数据库保持迁移前的状态（已成功的迁移已提交，可安全重试）。请修复上方错误后**重新运行本安装器**（幂等，已应用版本会自动跳过），或手动执行：FLOATCTF_CONFIG=<宿主可达的 TOML> bash $FLOATCTF_HOME/db/migrate.sh status"
    fi
    ok "数据库迁移已应用（postgres 容器保持运行；平台其余服务仍未启动）"
}

# ── 部署前硬校验：AWD/AWDP 运行时镜像（B1）────────────────────────────────────
#
# write_config_template 把 [awd]/[awdp] 指向 $RUNTIME_IMAGE_REGISTRY 下的三个镜像
# （tag = 平台版本 $VERSION，awdp 已扁平化）。这些镜像随 release 发布到 GHCR
# （release.yml 的 runtime-images job）——但安装环境可能离线、registry 不可达或
# 尚未登录。缺镜像时 AWD/AWDP 部署会在**比赛现场**才失败，且错误信息很难懂。
# 因此这里逐个 inspect，缺则 pull；仍然缺就 **die**（绝不继续进入一个注定失败的
# 部署）。只跑 Jeopardy 的宿主可用 --skip-runtime-images 显式降级为告警。
runtime_image_ref() { # <flattened-name> → <registry>/<name>:<VERSION>
    printf '%s/%s:%s' "$RUNTIME_IMAGE_REGISTRY" "$1" "$VERSION"
}

RUNTIME_IMAGES_MISSING=""
ensure_runtime_images() {
    info "──── 部署：AWD/AWDP 运行时镜像（$RUNTIME_IMAGE_REGISTRY，tag=$VERSION）────"
    local images=(
        "$(runtime_image_ref awd-flagserver)"
        "$(runtime_image_ref awd-judgeserver)"
        "$(runtime_image_ref awdp-judgeserver)"
    )
    local img missing=() pulled=()
    for img in "${images[@]}"; do
        if docker image inspect "$img" >/dev/null 2>&1; then
            ok "镜像已就绪: $img"
            continue
        fi
        info "本地无此镜像，尝试: docker pull $img"
        if docker pull "$img" >/dev/null 2>&1 && docker image inspect "$img" >/dev/null 2>&1; then
            pulled+=("$img")
            continue
        fi
        missing+=("$img")
    done
    [ "${#pulled[@]}" -gt 0 ] && info "已拉取: ${pulled[*]}"

    if [ "${#missing[@]}" -eq 0 ]; then
        RUNTIME_IMAGES_MISSING=""
        ok "AWD/AWDP 运行时镜像齐备（$VERSION）"
        return 0
    fi
    RUNTIME_IMAGES_MISSING="${missing[*]}"

    local missing_list pull_cmd save_args
    missing_list="$(printf '%s, ' "${missing[@]}")"; missing_list="${missing_list%, }"
    pull_cmd="$(printf 'docker pull %s; ' "${missing[@]}")"; pull_cmd="${pull_cmd%; }"
    save_args="$(printf '%s ' "${missing[@]}")"

    if [ "$SKIP_RUNTIME_IMAGES" = "1" ]; then
        warn "════════════════════════════════════════════════════════════════"
        warn "--skip-runtime-images / FLOATCTF_SKIP_RUNTIME_IMAGES=1：**跳过**运行时镜像硬校验"
        warn "缺少 AWD/AWDP 运行时镜像：AWD/AWDP 赛事在补齐前**无法部署**（Jeopardy 不受影响）"
        local m
        for m in "${missing[@]}"; do warn "  缺失: $m"; done
        warn "补齐（需网络可达；私有 registry 还需 docker login）:"
        warn "  $pull_cmd"
        warn "离线/手工预载（escape hatch）——在有镜像的机器上:"
        warn "  docker save ${save_args}-o floatctf-runtime-images.tar"
        warn "目标机: sudo docker load < floatctf-runtime-images.tar 然后重新运行本安装器"
        warn "════════════════════════════════════════════════════════════════"
        return 0
    fi

    die "缺少 AWD/AWDP 运行时镜像（tag=$VERSION）：${missing_list}
  这些镜像由 release.yml 的 runtime-images job 发布到 $RUNTIME_IMAGE_REGISTRY；
  目标机拉不到 = AWD/AWDP 赛事部署必然失败（Jeopardy 不受影响）。
  精确拉取命令（需网络可达；私有 registry 还需 docker login）:
    ${pull_cmd}
  离线/手工预载（escape hatch）:
    # 在有镜像的机器上
    docker save ${save_args}-o floatctf-runtime-images.tar
    # 目标机
    sudo docker load < floatctf-runtime-images.tar
    然后重新运行本安装器（幂等）。
  只部署 Jeopardy（不使用 AWD/AWDP）时可显式跳过本检查:
    --skip-runtime-images（或 FLOATCTF_SKIP_RUNTIME_IMAGES=1）"
}

run_deploy() {
    info "──── 第三阶段：部署（写文件/镜像/网络 + 建服务，不启动容器）→ $FLOATCTF_HOME ────"
    precheck
    # B1：运行时镜像必须在**动任何安装文件之前**就绪（缺则 die）。放最前面意味着
    # 失败时系统里没有任何"半启动/半写入"的部署状态，重跑即可。
    ensure_runtime_images
    prepare_env
    write_compose_prod
    write_config_template
    write_caddy_template
    prepare_configs
    validate_caddy_config
    stage_release
    # 既有数据库先补新迁移（fresh 库自动跳过；平台此时尚未启动）。
    apply_migrations
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
    # R3（fail closed，且在**任何 mutation 之前**）：release 的 db/migrate.sh 用
    # stdlib tomllib 解析 TOML（Python ≥3.11）。这里先于 require_root / run_init
    #（会装包、建目录）/ 下载 / docker build / 迁移 检查，缺则立刻 die，
    # 宿主上不留任何半成品。
    check_python_tomllib
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
  journalctl -fu floatctf.target floatctf-infra
（生产 API 由 Compose 托管，没有 floatctf-api.service —— 不要照抄成 floatctf-api）
备份 / 恢复（运维工具已随 release 装好）：
  sudo $FLOATCTF_HOME/backup.sh                 # 产出 $PWD/floatctf-backup-<UTC>.tar.gz
  sudo $FLOATCTF_HOME/restore.sh <归档> --yes
升级既有安装：重新运行本安装器（release 的 db/migrate.sh 会 forward-only 补齐新迁移）；
  已装好的迁移器也可手动使用：sudo $FLOATCTF_HOME/db/migrate.sh status
AWD/AWDP 运行时镜像（$RUNTIME_IMAGE_REGISTRY，tag=$VERSION）：
  已在部署前逐个 docker image inspect / docker pull 校验通过；
  升级时管理员自定义的镜像引用会被保留（--reset-runtime-images 可强制回默认）。
EOF
    if [ -n "$RUNTIME_IMAGES_MISSING" ]; then
        cat <<EOF

!! 注意：已按 --skip-runtime-images / FLOATCTF_SKIP_RUNTIME_IMAGES=1 跳过镜像校验 !!
  缺失镜像: $RUNTIME_IMAGES_MISSING
  AWD/AWDP 赛事在补齐前**无法部署**（Jeopardy 不受影响）。
  联网补齐：docker pull <上面的 ref>（每个）；私有 registry 需先 docker login。
  离线预载：在有镜像的机器 docker save <refs> -o images.tar，目标机 sudo docker load < images.tar，
            然后重新运行本安装器。
EOF
    fi
    ok "FloatCTF 安装完成：$FLOATCTF_HOME"
}

main
