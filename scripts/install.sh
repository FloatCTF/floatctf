#!/usr/bin/env bash
#
# FloatCTF 一键安装器（Phase 11）— 单文件自包含。
#
# 部署模板不在本文件内联：安装器按版本从仓库 tag 拉取 deploy/compose.prod.yml、
# deploy/templates/floatctf.toml.tmpl、infra/caddy/Caddyfile.prod、deploy/systemd/*、
# deploy/uninstall.sh（FLOATCTF_TEMPLATE_BASE 可指向镜像站/私有镜像），再渲染到 FLOATCTF_HOME。
#
# 两个入口共享同一权限模型：生产 API 运行在无 capabilities 的非 root 容器中，
# 开发 API 由 setpriv 收敛权限；宿主 Docker/网络控制统一由 floatctf-helper
# （docker group + CAP_NET_ADMIN）通过 helper-control.sock / helper-docker.sock 提供。
#
# 【生产安装】无需 clone 仓库：
#   curl -fsSL <install.sh 的 URL> -o install.sh && sudo bash install.sh
#   安装后由 systemd 管理整栈：sudo systemctl start floatctf.target
#   或显式指定 6 个 release 产物 URL：
#     sudo bash install.sh --api-url <bin> --helper-url <bin> --web-url <dist> \
#                         --migrate-url <sql> --frontend-manager-url <script> \
#                         --ops-url <ops-tools.tar.gz>
#   流程：校验宿主依赖 → 下载 5 产物 + 拉取 API 镜像 → 部署 →
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
#
# 环境变量（覆盖 5 个产物 URL / release 版本 / 迁移开关 / 运行时镜像）：
#   FLOATCTF_HELPER_URL / FLOATCTF_WEB_URL /
#   FLOATCTF_MIGRATE_URL / FLOATCTF_FRONTEND_MANAGER_URL / FLOATCTF_OPS_URL /
#   FLOATCTF_VERSION / FLOATCTF_SKIP_MIGRATIONS /
#   FLOATCTF_SKIP_RUNTIME_IMAGES / FLOATCTF_RUNTIME_IMAGE_REGISTRY /
#   FLOATCTF_TEMPLATE_BASE=<raw 模板基址>
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
#     发布到 Docker Hub（release.yml 的 runtime-images job）。安装/升级时 ensure_runtime_images
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
RUNTIME_IMAGE_REGISTRY="${FLOATCTF_RUNTIME_IMAGE_REGISTRY:-docker.io/floatctf}"
[ -n "$RUNTIME_IMAGE_REGISTRY" ] || RUNTIME_IMAGE_REGISTRY="docker.io/floatctf"
RUNTIME_IMAGE_REGISTRY="${RUNTIME_IMAGE_REGISTRY%/}"
export RUNTIME_IMAGE_REGISTRY

# fake 占位地址：真实 release 地址发布后替换（或经 --*-url / 环境变量覆盖）。
# 默认取 GitHub 的「最新正式发布」：releases/latest/download/<asset> 永远指向最新 tag，
# 因此 `sudo bash install.sh` 不需要任何参数即可装/升到最新版。
RELEASES_URL_BASE="https://github.com/FloatCTF/floatctf/releases"
DEFAULT_HELPER_URL="$RELEASES_URL_BASE/latest/download/floatctf-helper"
DEFAULT_WEB_URL="$RELEASES_URL_BASE/latest/download/web-dist.tar.gz"
DEFAULT_MIGRATE_URL="$RELEASES_URL_BASE/latest/download/merged.sql"
DEFAULT_FRONTEND_MANAGER_URL="$RELEASES_URL_BASE/latest/download/frontend.sh"
DEFAULT_OPS_URL="$RELEASES_URL_BASE/latest/download/ops-tools.tar.gz"

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

# ── release 版本解析 ─────────────────────────────────────────────────────────
# 解析 GitHub「最新正式发布」的版本号（不带前导 v）。
# 先跟随 /releases/latest 的 302 拿最终 tag URL（一次请求、不受 API 限流），
# 失败再退到 REST API。
resolve_latest_release_version() {
    local effective tag
    effective="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$RELEASES_URL_BASE/latest" 2>/dev/null || true)"
    if [[ "$effective" =~ /releases/tag/v?([0-9][^/]*)$ ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
        return 0
    fi
    tag="$(curl -fsSL -H 'Accept: application/vnd.github+json' \
        "https://api.github.com/repos/FloatCTF/floatctf/releases/latest" 2>/dev/null \
        | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n1 || true)"
    [[ "$tag" =~ ^[0-9][0-9A-Za-z.+-]*$ ]] || return 1
    printf '%s\n' "$tag"
}

# 显式版本时把**默认**产物地址钉到该 tag；显式传入的 --*-url 不动。
pin_default_urls_to_version() { # <version>
    local v="$1"
    pin_url() {
        printf '%s' "$1" | sed "s#^${RELEASES_URL_BASE}/latest/download/#${RELEASES_URL_BASE}/download/v${v}/#"
    }
    [ "$HELPER_URL_EXPLICIT" -eq 1 ] || HELPER_URL="$(pin_url "$HELPER_URL")"
    [ "$WEB_URL_EXPLICIT" -eq 1 ] || WEB_URL="$(pin_url "$WEB_URL")"
    [ "$MIGRATE_URL_EXPLICIT" -eq 1 ] || MIGRATE_URL="$(pin_url "$MIGRATE_URL")"
    [ "$FRONTEND_MANAGER_URL_EXPLICIT" -eq 1 ] || FRONTEND_MANAGER_URL="$(pin_url "$FRONTEND_MANAGER_URL")"
    [ "$OPS_URL_EXPLICIT" -eq 1 ] || OPS_URL="$(pin_url "$OPS_URL")"
}

# ── 参数 ──────────────────────────────────────────────────────────────────────
HELPER_URL="$DEFAULT_HELPER_URL"
WEB_URL="$DEFAULT_WEB_URL"
MIGRATE_URL="$DEFAULT_MIGRATE_URL"
FRONTEND_MANAGER_URL="$DEFAULT_FRONTEND_MANAGER_URL"
OPS_URL="$DEFAULT_OPS_URL"
VERSION="${FLOATCTF_VERSION:-}"
VERSION_EXPLICIT=0
HELPER_URL_EXPLICIT=0
WEB_URL_EXPLICIT=0
MIGRATE_URL_EXPLICIT=0
FRONTEND_MANAGER_URL_EXPLICIT=0
OPS_URL_EXPLICIT=0
# 升级既有安装时，迁移由 release 的 db/migrate.sh forward-only 应用；可用
# --skip-migrations 显式跳过（见 apply_migrations 的说明与风险提示）。
SKIP_MIGRATIONS="${FLOATCTF_SKIP_MIGRATIONS:-0}"
# AWD/AWDP 运行时镜像默认**硬要求**（缺则 die）；--skip-runtime-images 是 operator
# 显式退出口（只跑 Jeopardy 的宿主），降级为醒目告警。
SKIP_RUNTIME_IMAGES="${FLOATCTF_SKIP_RUNTIME_IMAGES:-0}"
while [ $# -gt 0 ]; do
    case "$1" in
        --helper-url) HELPER_URL="${2:?--helper-url 需要一个地址参数}"; HELPER_URL_EXPLICIT=1; shift ;;
        --web-url) WEB_URL="${2:?--web-url 需要一个地址参数}"; WEB_URL_EXPLICIT=1; shift ;;
        --migrate-url) MIGRATE_URL="${2:?--migrate-url 需要一个地址参数}"; MIGRATE_URL_EXPLICIT=1; shift ;;
        --frontend-manager-url) FRONTEND_MANAGER_URL="${2:?--frontend-manager-url 需要一个地址参数}"; FRONTEND_MANAGER_URL_EXPLICIT=1; shift ;;
        --ops-url) OPS_URL="${2:?--ops-url 需要一个地址参数}"; OPS_URL_EXPLICIT=1; shift ;;
        --version) VERSION="${2:?--version 需要 release 版本，如 0.3.3}"; VERSION_EXPLICIT=1; shift ;;
        --skip-migrations) SKIP_MIGRATIONS=1 ;;
        --skip-runtime-images) SKIP_RUNTIME_IMAGES=1 ;;
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
[ "$HELPER_URL_EXPLICIT" -eq 1 ] || HELPER_URL="${FLOATCTF_HELPER_URL:-$HELPER_URL}"
[ "$WEB_URL_EXPLICIT" -eq 1 ] || WEB_URL="${FLOATCTF_WEB_URL:-$WEB_URL}"
[ "$MIGRATE_URL_EXPLICIT" -eq 1 ] || MIGRATE_URL="${FLOATCTF_MIGRATE_URL:-$MIGRATE_URL}"
[ "$FRONTEND_MANAGER_URL_EXPLICIT" -eq 1 ] || FRONTEND_MANAGER_URL="${FLOATCTF_FRONTEND_MANAGER_URL:-$FRONTEND_MANAGER_URL}"
[ "$OPS_URL_EXPLICIT" -eq 1 ] || OPS_URL="${FLOATCTF_OPS_URL:-$OPS_URL}"
[ "$VERSION_EXPLICIT" -eq 1 ] || VERSION="${FLOATCTF_VERSION:-$VERSION}"

# 版本来源优先级：--version / FLOATCTF_VERSION > 自定义产物 URL 里的 tag > 最新正式发布。
# 默认产物地址是 releases/latest/download/，所以「不传版本 = 装最新」；
# 传了版本则把默认地址钉到该 tag，避免 latest 在下载过程中漂移。
# `--version latest` 等价于不传：交给下面的 latest 解析。
[ "$VERSION" = "latest" ] && VERSION=""
if [ -z "$VERSION" ] && [[ "$WEB_URL" =~ /releases/download/v?([^/]+)/ ]]; then
    VERSION="${BASH_REMATCH[1]}"
fi
if [ -z "$VERSION" ] && [ "$DEVELOP" -eq 1 ] && [ -f apps/api/Cargo.toml ]; then
    VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' apps/api/Cargo.toml | head -n1)"
fi
if [ -z "$VERSION" ]; then
    VERSION="$(resolve_latest_release_version)" \
        || die "无法确定最新 release 版本（网络不可达或仓库还没有正式发布）；请显式传 --version <版本>"
fi
export VERSION
pin_default_urls_to_version "$VERSION"
# 升级保护的两种强制模式互斥（同时给出就无法判定意图 → fail closed，绝不猜）。

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
# 第一阶段：宿主依赖校验（安装器不装包、不写 sysctl；缺什么就报命令）
# ============================================================================

# postgresql 包仅取其客户端 psql（dev 的 migrate.sh status/apply 在宿主执行；
# Arch 无 client-only 包）。生产手动运维/备份同样受益。
# python3 是前端管理器（$FLOATCTF_HOME/frontend.sh）的运行时依赖：它用 python3 做
# frontend.json / registry.json 的 JSON 解析与原子注册表更新（见 docs/frontend/ARTIFACT.md）。
ARCH_PKGS=(docker docker-compose nftables wireguard-tools iproute2 conntrack-tools iptables procps-ng openssl curl tar postgresql python3)

# Debian/Ubuntu 的包名与 Arch 不同：procps（非 procps-ng）、conntrack（非
# conntrack-tools）、docker.io（发行版打包的 Docker）。
#   - 不装 postgresql-*：宿主运维一律经容器（docker exec），宿主 psql 非必需；
#     在 Debian 上装它会牵入 PGDG 源，反而把 apt 绑到第三方仓库。
#   - docker compose v2 插件不由 Debian 仓库提供（见 check_docker 的显式提示）。
DEBIAN_PKGS=(docker.io nftables wireguard-tools iproute2 conntrack iptables procps openssl curl tar python3)



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

# 宿主依赖由运维自己准备；安装器只检查并给出精确命令。
prereq_hint() {
    if command -v pacman >/dev/null 2>&1; then
        printf '%s' 'pacman -S --needed docker docker-compose nftables wireguard-tools iproute2 conntrack-tools iptables procps-ng openssl curl tar python'
    elif command -v apt-get >/dev/null 2>&1; then
        printf '%s' 'apt-get install -y docker.io nftables wireguard-tools iproute2 conntrack iptables procps openssl curl tar python3（compose v2 插件需单装：docker-compose-plugin 或 /usr/local/lib/docker/cli-plugins/docker-compose）'
    else
        printf '%s' '用发行版包管理器安装 docker（含 compose v2）、nftables、wireguard-tools、iproute2、conntrack、iptables、procps、openssl、curl、tar、python3'
    fi
}

check_commands() {
    local c missing=()
    for c in ip wg nft conntrack iptables docker sysctl modprobe; do
        command -v "$c" >/dev/null 2>&1 || missing+=("$c")
    done
    if [ "${#missing[@]}" -gt 0 ]; then
        die "缺少命令: ${missing[*]}
  安装器不代装宿主依赖，请先准备：
    $(prereq_hint)"
    fi
    # 前端管理器 frontend.sh 需要 python3（JSON 解析 + 原子注册表更新）；
    # 迁移器 db/migrate.sh 额外需要 Python ≥3.11 的 stdlib tomllib（见上）。
    command -v python3 >/dev/null 2>&1 \
        || die "缺少命令: python3（前端管理器 frontend.sh 与迁移器 db/migrate.sh 的依赖；apt/dnf/pacman 安装 python3 后重试）"
    check_python_tomllib
    ok "基础命令齐全（ip/wg/nft/conntrack/iptables/docker/sysctl/modprobe/python3+tomllib）"
}

check_docker() {
    if command -v systemctl >/dev/null 2>&1 && ! systemctl -q is-active docker.service; then
        die "Docker daemon 未运行。请由运维启动并设为开机自启：
    sudo systemctl enable --now docker"
    fi
    docker info >/dev/null 2>&1 || die "docker daemon 不可用（docker info 失败）"
    # systemd 单元用 `docker compose`（v2 语法）管理生产栈，缺插件会在启动平台时
    # 才暴露；Debian/Ubuntu 仓库不提供 docker-compose-plugin，必须显式 fail closed。
    docker compose version >/dev/null 2>&1 \
        || die "缺少 docker compose v2 插件（生产栈由 \`docker compose\` 管理）。Debian/Ubuntu 请放置 v2 静态二进制：install -m 0755 docker-compose /usr/local/lib/docker/cli-plugins/docker-compose，然后重试"
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


check_ip_forward() {
    local v
    v=$(cat /proc/sys/net/ipv4/ip_forward 2>/dev/null || echo "?")
    [ "$v" = "1" ] || die "net.ipv4.ip_forward=$v，AWD 容器网络需要转发。请由运维设置：
    sysctl -w net.ipv4.ip_forward=1
    echo 'net.ipv4.ip_forward=1' > $SYSCTL_FILE && sysctl --system"
    ok "net.ipv4.ip_forward=1"
}

check_bridge_netfilter() {
    local k v bad=()
    grep -qE '^br_netfilter$' /proc/modules 2>/dev/null \
        || bad+=("br_netfilter 未加载")
    for k in net.bridge.bridge-nf-call-iptables net.bridge.bridge-nf-call-ip6tables; do
        v=$(cat "/proc/sys/$k" 2>/dev/null || echo "?")
        [ "$v" = "1" ] || bad+=("$k=$v")
    done
    if [ "${#bad[@]}" -gt 0 ]; then
        die "AWD 同桥隔离依赖的内核设置未就绪：${bad[*]}
  请由运维设置（会写入宿主全局配置）：
    modprobe br_netfilter
    echo br_netfilter > $MODULES_FILE
    printf 'net.bridge.bridge-nf-call-iptables=1\nnet.bridge.bridge-nf-call-ip6tables=1\n' > $SYSCTL_FILE
    sysctl --system"
    fi
    ok "br_netfilter + bridge-nf-call-{ip,ip6}tables=1"
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
    for d in web config/caddy data/postgres data/rustfs data/caddy data/caddy-config logs/rustfs runtime; do
        mkdir -p "$FLOATCTF_HOME/$d"
    done
    chown root:"$FCTF_USER" "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME" >/dev/null 2>&1 || true
    local run_dir
    for run_dir in data logs runtime; do
        chown -R "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/$run_dir" >/dev/null 2>&1 || true
    done
    chown -R root:"$FCTF_USER" "$FLOATCTF_HOME/web" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/web" >/dev/null 2>&1 || true
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy" >/dev/null 2>&1 || true
    chmod 750 "$FLOATCTF_HOME/config" >/dev/null 2>&1 || true
    ok "布局就绪: $FLOATCTF_HOME/{web,config/caddy,data/{postgres,rustfs,caddy,caddy-config},logs/rustfs,runtime}"

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
    info "──── 第一阶段：宿主依赖校验 + 目录/组 ────"
    require_root
    check_linux
    check_commands
    check_docker
    check_nftables
    check_wireguard
    check_ip_forward
    check_bridge_netfilter
    check_user_layout
    ok "宿主就绪（docker/nftables/WireGuard/转发/br_netfilter/组/布局）"
}

# ============================================================================
# 第二阶段：获取 release 产物（6 个 URL）
# ============================================================================
download_url() { # url dest
    info "下载: $1"
    curl -fL --retry 3 --connect-timeout 30 -o "$2" "$1" \
        || die "下载失败: $1"
}

download_release() {
    info "──── 第二阶段：下载 release 产物（5 URL）────"
    TMP_STAGE_DIR="$(mktemp -d /tmp/floatctf-install.XXXXXX)"

    # API 镜像不再本地构建：直接拉 Docker Hub 上已发布的 <V> 镜像（见 stage_release）。

    # 1) 特权网络守护进程（单独 root-owned 安装，不放进 API 可写目录）
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

# 部署模板（compose / TOML / Caddyfile / systemd / uninstall.sh）不内联：
# 按版本从仓库 tag 拉取，安装器只保留下载与渲染逻辑，模板只需改一处。
TEMPLATE_BASE="${FLOATCTF_TEMPLATE_BASE:-https://raw.githubusercontent.com/FloatCTF/floatctf}"

fetch_templates() {
    info "──── 获取部署模板（v$VERSION）────"
    TPL_DIR="$TMP_STAGE_DIR/templates"
    mkdir -p "$TPL_DIR"
    local rel dest
    for rel in \
        deploy/compose.prod.yml \
        deploy/templates/floatctf.toml.tmpl \
        infra/caddy/Caddyfile.prod \
        deploy/systemd/floatctf-helper.service \
        deploy/systemd/floatctf-infra.service \
        deploy/systemd/floatctf.target \
        deploy/uninstall.sh; do
        dest="$TPL_DIR/$(basename "$rel")"
        download_url "$TEMPLATE_BASE/v$VERSION/$rel" "$dest" \
            || die "获取部署模板失败: $rel
  该版本若发布时尚无外部模板，请使用与目标版本配套的 install.sh。"
    done
    ok "部署模板已获取（7 个）"
}

acquire_package() {
    download_release
    fetch_templates
}

# ============================================================================
# 部署模板（从仓库 tag 拉取，写出到 FLOATCTF_HOME 时替换占位符）
# ============================================================================
write_compose_prod() {
    install -m 0644 "$TPL_DIR/compose.prod.yml" "$FLOATCTF_HOME/compose.prod.yml"
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/compose.prod.yml"
    ok "已写出 compose.prod.yml（API + PostgreSQL + Redis + RustFS + Caddy）"
}

write_config_template() {
    install -m 0644 "$TPL_DIR/floatctf.toml.tmpl" "$FLOATCTF_HOME/.floatctf.toml.tmpl"
    ok "已写出 config 模板"
}

write_caddy_template() {
    install -m 0644 "$TPL_DIR/Caddyfile.prod" "$FLOATCTF_HOME/.Caddyfile.tmpl"
    ok "已写出 Caddy 模板"
}

write_helper_systemd_unit() {
    mkdir -p /etc/systemd/system
    install -m 0644 "$TPL_DIR/floatctf-helper.service" /etc/systemd/system/floatctf-helper.service
}

write_systemd_units() {
    mkdir -p /etc/systemd/system
    write_helper_systemd_unit

    install -m 0644 "$TPL_DIR/floatctf-infra.service" /etc/systemd/system/floatctf-infra.service
    sed -i "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" /etc/systemd/system/floatctf-infra.service
    install -m 0644 "$TPL_DIR/floatctf.target" /etc/systemd/system/floatctf.target

    ok "systemd 单元已写出（helper + Compose stack）"
}

write_uninstall() {
    info "──── 写出卸载脚本 → $FLOATCTF_HOME/uninstall.sh ────"
    install -m 0644 "$TPL_DIR/uninstall.sh" "$FLOATCTF_HOME/uninstall.sh"
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
    mkdir -p "$FLOATCTF_HOME/config/caddy" "$FLOATCTF_HOME/config/oidc" "$FLOATCTF_HOME/data/caddy" "$FLOATCTF_HOME/data/caddy-config" "$FLOATCTF_HOME/logs/rustfs"
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
        env_set API_IMAGE "docker.io/floatctf/floatctf-api:$VERSION"
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
        env_set API_IMAGE "docker.io/floatctf/floatctf-api:$VERSION"
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
    chown root:"$FCTF_USER" "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy" "$FLOATCTF_HOME/config/oidc"
    chmod 750 "$FLOATCTF_HOME/config" "$FLOATCTF_HOME/config/caddy" "$FLOATCTF_HOME/config/oidc"
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

    # API 镜像与运行时镜像同样来自 Docker Hub：默认只拉取，不做本地构建。
    # 这样安装器不再内嵌 Dockerfile，也不再需要 52 MB 的 API 二进制产物。
    local api_image="docker.io/floatctf/floatctf-api:$VERSION"
    if docker image inspect "$api_image" >/dev/null 2>&1; then
        ok "API image 已存在: $api_image"
    else
        docker pull "$api_image" || die "拉取 API image 失败: $api_image
  网络不可达时可在有网络的机器 docker save/docker load 后重跑本安装器。"
        ok "API image 已拉取: $api_image"
    fi

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
    # 只用数值 UID：宿主按自己的权限模型**只创建 floatctf 组、不创建该用户**，
    # 用用户名 chown 在全新宿主机上必然 "invalid user" 并中断安装。
    chown "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/runtime"
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
# （tag = 平台版本 $VERSION，awdp 已扁平化）。这些镜像随 release 发布到 Docker Hub
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

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    # R3（fail closed，且在**任何 mutation 之前**）：release 的 db/migrate.sh 用
    # stdlib tomllib 解析 TOML（Python ≥3.11）。这里先于 require_root / run_init
    #（会建目录）/ 下载 / 迁移 检查，缺则立刻 die，
    # 宿主上不留任何半成品。
    check_python_tomllib
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
  已在部署前逐个 docker image inspect / docker pull 校验通过。
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
