#!/usr/bin/env bash
#
# FloatCTF production deploy (Phase 10.7) — 首次部署 / 重部署.
#
# 语义：
#   - 以调用用户（普通用户）身份构建/准备；仅特权操作（写 /etc/systemd/system、
#     chown 到 floatctf、systemctl）经 sudo 执行（无 sudo 则明确报错）。
#   - 分段安装：precheck → 装配产物 → 写配置（保留密钥）→ 起 infra（--wait）→
#     迁移（forward-only）→ 装 systemd 单元 → 启 API。
#   - 首次部署生成密钥并写入 /home/floatctf/config/floatctf.toml；
#     重部署保留既有密钥（JWT / RustFS），只更新非敏感值。
#   - 失败即退出（非零），绝不静默继续。
#
# 用法：
#   scripts/deploy.sh <release-dir>          # 部署指定发布目录（默认 release/floatctf-*）
#   scripts/deploy.sh --dry-run              # 只跑 precheck + 装配，不写系统
#   FCTF_ROOT=/home/floatctf scripts/deploy.sh ...   # 覆盖安装根
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FCTF_ROOT="${FCTF_ROOT:-/home/floatctf}"
FCTF_USER="floatctf"
COMPOSE_SRC="$REPO_ROOT/infra/compose/compose.prod.yml"
NGINX_TMPL="$REPO_ROOT/infra/nginx/nginx.prod.conf"
CONFIG_TMPL="$REPO_ROOT/infra/config/floatctf.prod.toml"
MIGRATE="$REPO_ROOT/apps/api/src/sql/migrate.sh"
SYSTEMD_SRC="$REPO_ROOT/infra/systemd"
RELEASE_DIR=""
DRY_RUN=0

info() { printf '%s[INFO]%s %s\n' "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n' "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
warn() { printf '%s[WARN]%s %s\n' "$(tput setaf 3 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n' "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

# ── 参数 ──────────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*)
            # 首个非选项参数为 release 目录
            if [ -z "$RELEASE_DIR" ]; then RELEASE_DIR="$1"; else die "未知参数: $1"; fi
            ;;
        *)
            if [ -z "$RELEASE_DIR" ]; then RELEASE_DIR="$1"; else die "未知参数: $1"; fi
            ;;
    esac
    shift
done

if [ -z "$RELEASE_DIR" ]; then
    RELEASE_DIR="$(ls -d "$REPO_ROOT"/release/floatctf-* 2>/dev/null | sort -V | tail -1 || true)"
    [ -n "$RELEASE_DIR" ] || die "未指定 release 目录且 release/ 下无产物；先跑 scripts/build-release.sh"
fi
RELEASE_DIR="$(realpath "$RELEASE_DIR")"
[ -f "$RELEASE_DIR/bin/floatctf" ] || die "release 目录缺少 bin/floatctf: $RELEASE_DIR"
[ -d "$RELEASE_DIR/web" ] || die "release 目录缺少 web/: $RELEASE_DIR"

VERSION="$(basename "$RELEASE_DIR" | sed 's/^floatctf-//')"
ENV_FILE="$FCTF_ROOT/.env"

# ── 工具 ──────────────────────────────────────────────────────────────────────
for c in docker realpath; do
    command -v "$c" >/dev/null 2>&1 || die "缺少命令: $c"
done

# 特权执行：已是 root 直接跑（本机 nsenter/容器 root 路径）；否则走 sudo。
run_priv() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    else
        command -v sudo >/dev/null 2>&1 || die "需要 root 或 sudo（当前非 root 且无 sudo）"
        sudo "$@"
    fi
}

sudo_needed() {
    [ "$(id -u)" -eq 0 ] && return 0
    command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null
}

# ── 1. precheck ───────────────────────────────────────────────────────────────
precheck() {
    info "── precheck ──"
    # root 用户部署直接警告（推荐普通用户 + sudo）
    [ "$(id -u)" -eq 0 ] && warn "以 root 运行 deploy.sh：构建/装配将用 root（可用普通用户 + sudo）"
    # 初始化标记（init.sh 已跑过）
    if [ ! -f "$FCTF_ROOT/.initialized" ]; then
        warn "$FCTF_ROOT/.initialized 不存在 —— 建议先跑 sudo scripts/init.sh"
    fi
    # docker daemon
    docker info >/dev/null 2>&1 || die "docker daemon 不可用"
    # 端口占用检查：读 .env（重部署时已存在）或默认值
    local api_port pg_port rustfs_port http_port
    api_port=$(env_get API_PORT 9090)
    pg_port=$(env_get POSTGRES_PORT 5433)
    rustfs_port=$(env_get RUSTFS_PORT 9000)
    http_port=$(env_get HTTP_PORT 80)
    info "端口：API=$api_port PG=$pg_port RustFS=$rustfs_port HTTP=$http_port"
    for port_spec in "$api_port" "$pg_port" "$rustfs_port" "$http_port"; do
        if ss -ltn 2>/dev/null | awk '{print $4}' | grep -qE "[:.]${port_spec}$"; then
            die "端口 $port_spec 已被占用（ss 检查）；请调整 .env 端口"
        fi
    done
    ok "precheck 通过"
}

# ── .env 读写 ─────────────────────────────────────────────────────────────────
# 取值优先级：环境变量 > .env > 默认值（部署参数可临时覆盖，如 API_PORT=9290 scripts/deploy.sh）
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

# 生成/保留 .env（密钥仅首次生成；重部署保留）
prepare_env() {
    info "── 配置（.env + floatctf.toml + nginx.conf）──"
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
        # 重部署：保留密钥，只更新非敏感项（含环境覆盖的端口）
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
    if [ "$DRY_RUN" = "1" ]; then return; fi
    if ! sudo_needed; then
        die "需要 sudo（无密码 NOPASSWD 或交互不可用）才能写 $FCTF_ROOT；请以可 sudo 用户运行"
    fi
    run_priv mkdir -p "$FCTF_ROOT" 2>/dev/null || die "特权写入失败（$FCTF_ROOT）"
    run_priv chown -R "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT/data" "$FCTF_ROOT/logs" "$FCTF_ROOT/runtime" 2>/dev/null || true
}

# 用 envsubst 从模板渲染（模板占位符为 ${VAR}；nginx 变量 $host 等不可被替换）
render() { # template out
    local tmpl="$1" out="$2"
    local vars=""
    # 提取模板中全部 ${VAR} 占位符，作为 envsubst 白名单
    vars=$(grep -oE '\$\{[A-Z_]+\}' "$tmpl" | tr -d '${}' | sort -u | paste -sd, -)
    [ -n "$vars" ] || vars="_NO_VARS_"
    local varspec=""
    IFS=',' read -r -a vararr <<< "$vars"
    for v in "${vararr[@]}"; do
        varspec="$varspec\${$v} "
    done
    # envsubst 只替换白名单变量；nginx 的 $host/$http_upgrade 等保持原样
    envsubst "$varspec" < "$tmpl" > "$out.tmp" || die "envsubst 渲染失败: $tmpl"
    mv "$out.tmp" "$out"
}

prepare_configs() {
    # .env 值导入环境供 envsubst 使用
    set -a; . "$ENV_FILE"; set +a

    if [ "$DRY_RUN" = "1" ]; then
        # dry-run：渲染到临时目录仅作校验，不写 FCTF_ROOT
        local dr
        dr="$(mktemp -d /tmp/fctf-dryrun.XXXXXX)"
        render "$CONFIG_TMPL" "$dr/floatctf.toml"
        render "$NGINX_TMPL" "$dr/nginx.conf"
        ok "dry-run：配置已渲染到 $dr（floatctf.toml + nginx.conf），未写 $FCTF_ROOT"
        return
    fi

    render "$CONFIG_TMPL" "$FCTF_ROOT/config/floatctf.toml"
    render "$NGINX_TMPL" "$FCTF_ROOT/config/nginx/nginx.conf"

    # 权限：config 目录 root:floatctf 750（floatctf 组需遍历读取），
    # 配置文件 root:floatctf 640（含密钥）。
    run_priv chown root:"$FCTF_USER" "$FCTF_ROOT/config" "$FCTF_ROOT/config/nginx"
    run_priv chmod 750 "$FCTF_ROOT/config" "$FCTF_ROOT/config/nginx"
    run_priv chown root:"$FCTF_USER" "$FCTF_ROOT/config/floatctf.toml" "$FCTF_ROOT/config/nginx/nginx.conf"
    run_priv chmod 640 "$FCTF_ROOT/config/floatctf.toml" "$FCTF_ROOT/config/nginx/nginx.conf"
    run_priv mkdir -p "$FCTF_ROOT/config/nginx/keys"
    ok "配置已写入（floatctf.toml + nginx.conf，密钥保留）"
}

# ── 2. 装配产物 ───────────────────────────────────────────────────────────────
stage_release() {
    info "── 装配产物 → $FCTF_ROOT ──"
    if [ "$DRY_RUN" = "1" ]; then
        ok "dry-run：跳过写入（bin/web/compose）"
        return
    fi
    run_priv install -m 0755 "$RELEASE_DIR/bin/floatctf" "$FCTF_ROOT/bin/floatctf"
    run_priv rm -rf "$FCTF_ROOT/web"
    run_priv cp -r "$RELEASE_DIR/web" "$FCTF_ROOT/web"
    run_priv chown -R root:root "$FCTF_ROOT/web"
    run_priv install -m 0644 "$COMPOSE_SRC" "$FCTF_ROOT/compose.yml"
    run_priv mkdir -p "$FCTF_ROOT/runtime"
    run_priv chown "$FCTF_USER":"$FCTF_USER" "$FCTF_ROOT/runtime"
    # 容器专用目录属主：rustfs 容器以 uid 10001（镜像内置用户）运行，postgres 以
    # uid 999 运行；这些 bind-mount 目录必须归对应容器 uid，而非 floatctf（否则 EACCES）。
    # postgres 数据目录仅首次（空）或未初始化时设 999:999，避免破坏既有数据。
    run_priv chown -R 10001:10001 "$FCTF_ROOT/data/rustfs" "$FCTF_ROOT/logs/rustfs" 2>/dev/null || true
    if [ -z "$(ls -A "$FCTF_ROOT/data/postgres" 2>/dev/null)" ]; then
        run_priv chown -R 999:999 "$FCTF_ROOT/data/postgres" 2>/dev/null || true
    fi
    ok "产物装配完成（bin/floatctf + web/ + compose.yml + 容器目录属主）"
}

# ── 3. infra 容器 ─────────────────────────────────────────────────────────────
start_infra() {
    info "── 基础设施容器（docker compose up -d --wait）──"
    if [ "$DRY_RUN" = "1" ]; then
        ok "dry-run：跳过容器启动"
        return
    fi
    ( cd "$FCTF_ROOT" && run_priv docker compose -f compose.yml up -d --wait ) \
        || die "infra 容器启动/健康检查失败（docker compose up -d --wait）"
    ok "infra 就绪（postgres/rustfs/nginx healthcheck 通过）"
}

# ── 4. 数据库迁移（forward-only）─────────────────────────────────────────────
migrate_db() {
    info "── 数据库迁移（migrate.sh apply，forward-only）──"
    if [ "$DRY_RUN" = "1" ]; then
        ok "dry-run：跳过迁移"
        return
    fi
    run_priv env FLOATCTF_CONFIG="$FCTF_ROOT/config/floatctf.toml" \
        "$MIGRATE" apply || die "数据库迁移失败（migrate.sh apply 非零）"
    ok "迁移完成（schema_migrations 已更新）"
}

# ── 5. systemd 单元 ───────────────────────────────────────────────────────────
install_systemd() {
    info "── systemd 单元（floatctf-infra / api / target）──"
    if [ "$DRY_RUN" = "1" ]; then
        ok "dry-run：跳过单元安装"
        return
    fi
    for u in floatctf-infra.service floatctf-api.service floatctf.target; do
        run_priv install -m 0644 "$SYSTEMD_SRC/$u" "/etc/systemd/system/$u"
    done
    run_priv systemctl daemon-reload
    # 仅当单元尚未启用时 enable；target 聚合 infra+api
    run_priv systemctl enable floatctf.target floatctf-infra.service floatctf-api.service
    ok "systemd 单元已安装并 enable（floatctf.target）"
}

# ── 6. 启动 API ───────────────────────────────────────────────────────────────
start_api() {
    info "── 启动 API（floatctf-api.service）──"
    if [ "$DRY_RUN" = "1" ]; then
        ok "dry-run：跳过 API 启动"
        return
    fi
    run_priv systemctl restart floatctf-api.service || die "API 启动失败（journalctl -u floatctf-api）"
    # 等待端口就绪（API 启动较慢：AWD 恢复/连接 DB/RustFS）。
    # 无匿名 200 端点；任何 HTTP 响应（含 401/404）都证明服务已监听并路由。
    local api_port
    api_port=$(env_get API_PORT 9090)
    local tries=0
    while [ "$tries" -lt 30 ]; do
        local code
        code=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$api_port/api/announcements" 2>/dev/null || true)
        if [ -n "$code" ] && [ "$code" != "000" ]; then
            ok "API 已在 $api_port 监听（HTTP $code）"
            return
        fi
        tries=$((tries + 1)); sleep 2
    done
    die "API 60s 内未监听 $api_port（journalctl -u floatctf-api 查看日志）"
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    info "FloatCTF deploy $VERSION → $FCTF_ROOT（release: $RELEASE_DIR）"
    precheck
    prepare_env
    prepare_configs
    stage_release
    start_infra
    migrate_db
    install_systemd
    start_api
    ok "部署完成：$FCTF_ROOT"
}

main
