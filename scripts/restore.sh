#!/usr/bin/env bash
#
# FloatCTF 生产恢复（Phase 13）— 单文件自包含，随 release 产物发布。
# 与 scripts/backup.sh 配对；归档格式与安全规则见 docs/agents/BACKUP.md。
#
# 安全模型（默认拒绝危险操作）：
#   * 必须显式表达运维意图：--yes，或在交互终端输入 RESTORE FLOATCTF
#   * 已存在安装（.env / data/postgres 非空）时默认**拒绝覆盖**，需 --force
#   * 恢复前校验归档 sha256 + MANIFEST 逐成员哈希
#   * 拒绝路径穿越（绝对路径 / ..）、符号链接、硬链接、设备/FIFO、setuid/setgid
#   * 校验 BACKUP_FORMAT 受支持、TOOL 正确
#   * PostgreSQL 主版本不一致默认拒绝（--allow-version-mismatch 才放行）
#   * 只操作本安装根的 compose project，绝不 docker prune / nft flush / 触碰无关资源
#
# 用法：sudo $FLOATCTF_HOME/restore.sh <ARCHIVE> [选项]
#   --home DIR        目标安装根（默认 $FLOATCTF_HOME，再默认 /var/lib/floatctf）
#   --compose-file F  compose 文件（默认 <home>/compose.prod.yml；不存在则用归档内那份）
#   --yes             非交互确认
#   --force           允许覆盖已存在的安装
#   --dry-run         只做校验与计划打印，不改动任何状态
#   --no-start        恢复完成后不启动平台
#   --sha256 HEX      直接指定期望的归档 sha256（否则读 <ARCHIVE>.sha256）
#   --only LIST       子集：env,config,merge,frontends,web,runtime,postgres,rustfs,redis,caddy
#   --allow-version-mismatch  PostgreSQL 主版本不一致时继续（有风险，需自担）
#   -h|--help
#
# 退出码：0 成功；1 用法/校验失败（未改动任何状态）；2 恢复过程中失败。
#
set -Eeuo pipefail

RESTORE_SUPPORTED_FORMAT_MIN=1
RESTORE_SUPPORTED_FORMAT_MAX=1
RESTORE_TOOL="floatctf-backup"

FLOATCTF_HOME="${FLOATCTF_HOME:-/var/lib/floatctf}"
ARCHIVE=""
COMPOSE_FILE=""
ONLY=""
ASSUME_YES=0
FORCE=0
DRY_RUN=0
NO_START=0
EXPECT_SHA=""
ALLOW_VERSION_MISMATCH=0

if [ -t 1 ]; then
    C_OK=$'\033[0;32m'; C_WARN=$'\033[1;33m'; C_END=$'\033[0m'
else
    C_OK=''; C_WARN=''; C_END=''
fi
info() { printf '[INFO] %s\n' "$*" >&2; }
ok()   { printf '%s[ OK ]%s %s\n' "$C_OK" "$C_END" "$*" >&2; }
warn() { printf '%s[WARN]%s %s\n' "$C_WARN" "$C_END" "$*" >&2; }
die()  { printf '[FAIL] %s\n' "$*" >&2; exit "${2:-1}"; }

WORK_DIR=""
SERVICES_STOPPED=0
cleanup() {
    local rc=$?
    [ -n "$WORK_DIR" ] && [ -d "$WORK_DIR" ] && rm -rf -- "$WORK_DIR"
    exit "$rc"
}
trap cleanup EXIT INT TERM

usage() { sed -n '2,38p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --home) FLOATCTF_HOME="${2:?--home 需要目录}"; shift ;;
        --compose-file) COMPOSE_FILE="${2:?--compose-file 需要文件路径}"; shift ;;
        --yes) ASSUME_YES=1 ;;
        --force) FORCE=1 ;;
        --dry-run) DRY_RUN=1 ;;
        --no-start) NO_START=1 ;;
        --sha256) EXPECT_SHA="${2:?--sha256 需要哈希}"; shift ;;
        --only) ONLY="${2:?--only 需要列表}"; shift ;;
        --allow-version-mismatch) ALLOW_VERSION_MISMATCH=1 ;;
        -h|--help) usage; exit 0 ;;
        -*) die "未知参数: $1（--help 查看用法）" ;;
        *)
            [ -z "$ARCHIVE" ] || die "只接受一个归档参数（多余: $1）"
            ARCHIVE="$1"
            ;;
    esac
    shift
done

[ -n "$ARCHIVE" ] || die "缺少归档参数（--help 查看用法）"
[ -f "$ARCHIVE" ] || die "归档不存在: $ARCHIVE"
case "$ARCHIVE" in /*) ;; *) ARCHIVE="$PWD/$ARCHIVE" ;; esac

command -v python3 >/dev/null 2>&1 || die "缺少 python3"
command -v sha256sum >/dev/null 2>&1 || die "缺少 sha256sum"
command -v tar >/dev/null 2>&1 || die "缺少 tar"
command -v docker >/dev/null 2>&1 || die "缺少 docker"

want() {
    [ -z "$ONLY" ] && return 0
    case ",$ONLY," in *",$1,"*) return 0 ;; *) return 1 ;; esac
}
if [ -n "$ONLY" ]; then
    for w in $(printf '%s' "$ONLY" | tr ',' ' '); do
        case "$w" in
            env|config|merge|frontends|web|runtime|postgres|rustfs|redis|caddy) ;;
            *) die "--only 含未知子集: $w" ;;
        esac
    done
fi

# ── 1. 完整性：先校验归档本身，再解压任何东西 ────────────────────────────────
if [ -z "$EXPECT_SHA" ] && [ -f "$ARCHIVE.sha256" ]; then
    EXPECT_SHA="$(tr -d '[:space:]' < "$ARCHIVE.sha256")"
fi
ACTUAL_SHA="$(sha256sum "$ARCHIVE" | awk '{print $1}')"
if [ -n "$EXPECT_SHA" ]; then
    [ "$EXPECT_SHA" = "$ACTUAL_SHA" ] \
        || die "归档 sha256 不匹配（期望 $EXPECT_SHA，实际 $ACTUAL_SHA）——归档已损坏或被篡改"
    ok "归档 sha256 校验通过"
else
    warn "未提供 .sha256 伴随文件；跳过归档级校验（仍会校验 MANIFEST）"
fi

# ── 2. 结构校验：拒绝路径穿越 / 链接 / 特殊文件 ──────────────────────────────
info "校验归档结构"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-restore.XXXXXX")"
chmod 700 "$WORK_DIR"

python3 - "$ARCHIVE" <<'PY'
import sys, tarfile, posixpath

archive = sys.argv[1]
MAX_MEMBERS = 200_000
ROOT = None
count = 0

with tarfile.open(archive, "r:gz") as tf:
    for m in tf:
        count += 1
        if count > MAX_MEMBERS:
            sys.exit("归档成员数异常（>%d），拒绝" % MAX_MEMBERS)
        name = m.name
        if name.startswith("/") or name.startswith("./..") or ".." in name.split("/"):
            sys.exit("归档含路径穿越条目: %r" % name)
        if not m.isdir() and not m.isfile():
            sys.exit("归档含不允许的条目类型（links/device/fifo）: %r (%s)" % (name, m.type))
        if m.mode & 0o6000:
            sys.exit("归档含 setuid/setgid 条目: %r" % name)
        parts = [p for p in name.split("/") if p not in ("", ".")]
        if not parts:
            continue
        if ROOT is None:
            ROOT = parts[0]
        elif parts[0] != ROOT:
            sys.exit("归档含多个顶层条目（%r vs %r）；不是 backup.sh 产物" % (ROOT, parts[0]))
        # 单层根目录之下必须是已知成员。
        if len(parts) > 1:
            top = parts[1]
            allowed = {"META", "MANIFEST", "env", "config", "merged.sql", "frontend.sh",
                       "uninstall.sh", "compose.prod.yml", ".initialized", "frontends",
                       "web", "runtime", "data"}
            if top not in allowed:
                sys.exit("归档含未知顶层成员: %r" % top)
print("OK")
PY
[ $? -eq 0 ] || die "归档结构校验失败"

tar -C "$WORK_DIR" -xzf "$ARCHIVE" || die "解压失败"
SQL_ROOT="$(find "$WORK_DIR" -mindepth 1 -maxdepth 1 -type d | head -1)"
[ -n "$SQL_ROOT" ] || die "归档内没有顶层目录"
[ -f "$SQL_ROOT/META" ] || die "归档缺少 META（不是 backup.sh 产物？）"

# ── 3. 元数据 ────────────────────────────────────────────────────────────────
meta_get() { sed -nE "s/^$1=(.*)$/\1/p" "$SQL_ROOT/META" | head -1; }
B_FORMAT="$(meta_get BACKUP_FORMAT)"
B_TOOL="$(meta_get TOOL)"
B_CREATED="$(meta_get CREATED_AT)"
B_HOME="$(meta_get FLOATCTF_HOME)"
B_VERSION="$(meta_get PLATFORM_VERSION)"
B_PG_MAJOR="$(meta_get POSTGRES_MAJOR)"
B_PG_SERVER="$(meta_get POSTGRES_SERVER_VERSION)"
B_MEMBERS="$(meta_get MEMBER_COUNT)"
B_SUBSET="$(meta_get SUBSET)"

[ -n "$B_FORMAT" ] || die "META 缺少 BACKUP_FORMAT"
[ "$B_TOOL" = "$RESTORE_TOOL" ] || die "META TOOL=$B_TOOL 与期望 $RESTORE_TOOL 不符"
case "$B_FORMAT" in
    ''|*[!0-9]*) die "BACKUP_FORMAT 非法: $B_FORMAT" ;;
esac
if [ "$B_FORMAT" -lt "$RESTORE_SUPPORTED_FORMAT_MIN" ] || [ "$B_FORMAT" -gt "$RESTORE_SUPPORTED_FORMAT_MAX" ]; then
    die "备份格式 v$B_FORMAT 不受本 restore.sh 支持（支持 v$RESTORE_SUPPORTED_FORMAT_MIN..v$RESTORE_SUPPORTED_FORMAT_MAX）"
fi
ok "归档元数据: format=v$B_FORMAT created=$B_CREATED platform=${B_VERSION:-?} pg=${B_PG_SERVER:-?} members=${B_MEMBERS:-?} subset=${B_SUBSET:-?}"

# ── 4. MANIFEST 逐成员校验 ───────────────────────────────────────────────────
if [ -f "$SQL_ROOT/MANIFEST" ]; then
    info "校验 MANIFEST（逐成员 sha256）"
    if ! ( cd "$SQL_ROOT" && sha256sum -c --quiet MANIFEST ); then
        die "MANIFEST 校验失败：归档内容与备份时不符"
    fi
    ok "MANIFEST 校验通过（$(wc -l < "$SQL_ROOT/MANIFEST" | tr -d ' ') 个成员）"
else
    die "归档缺少 MANIFEST"
fi

# ── 5. 覆盖安全 ──────────────────────────────────────────────────────────────
EXISTING=0
if [ -f "$FLOATCTF_HOME/.env" ]; then EXISTING=1; fi
if [ -d "$FLOATCTF_HOME/data/postgres" ] && [ -n "$(ls -A "$FLOATCTF_HOME/data/postgres" 2>/dev/null)" ]; then EXISTING=1; fi
if [ "$EXISTING" = "1" ]; then
    if [ "$FORCE" != "1" ]; then
        die "目标安装已存在数据（$FLOATCTF_HOME）。恢复会覆盖现有数据库/配置/前端。
     确认无误后加 --force 重跑；或先备份当前状态：sudo $FLOATCTF_HOME/backup.sh --out <文件>"
    fi
    warn "--force：将覆盖 $FLOATCTF_HOME 下的现有数据"
elif [ "$B_HOME" != "$FLOATCTF_HOME" ]; then
    info "归档来源安装根为 $B_HOME，本次恢复到 $FLOATCTF_HOME（迁移场景）"
fi

# ── 6. PostgreSQL 主版本兼容 ─────────────────────────────────────────────────
if want postgres; then
    [ -f "$SQL_ROOT/data/postgres.pgc" ] || die "归档不含 data/postgres.pgc，但请求恢复 postgres"
    LOCAL_PG_MAJOR="unknown"
    COMPOSE_FOR_CHECK="$COMPOSE_FILE"
    [ -n "$COMPOSE_FOR_CHECK" ] || COMPOSE_FOR_CHECK="$FLOATCTF_HOME/compose.prod.yml"
    [ -f "$COMPOSE_FOR_CHECK" ] || COMPOSE_FOR_CHECK="$SQL_ROOT/compose.prod.yml"
    if [ -f "$COMPOSE_FOR_CHECK" ]; then
        LOCAL_PG_IMAGE="$(sed -nE 's/^[[:space:]]*image:[[:space:]]*(postgres:[0-9]+).*$/\1/p' "$COMPOSE_FOR_CHECK" | head -1)"
        case "$LOCAL_PG_IMAGE" in postgres:*) LOCAL_PG_MAJOR="${LOCAL_PG_IMAGE#postgres:}" ;; esac
    fi
    if [ "$LOCAL_PG_MAJOR" != "unknown" ] && [ "$B_PG_MAJOR" != "unknown" ] \
        && [ "$LOCAL_PG_MAJOR" != "$B_PG_MAJOR" ]; then
        if [ "$ALLOW_VERSION_MISMATCH" != "1" ]; then
            die "PostgreSQL 主版本不一致：备份=$B_PG_MAJOR 目标=$LOCAL_PG_MAJOR。
     恢复到不同主版本需先升级数据目录（pg_upgrade）或改用同版本镜像。
     确需继续请加 --allow-version-mismatch（不保证可用）。"
        fi
        warn "--allow-version-mismatch：备份 pg=$B_PG_MAJOR，目标 pg=$LOCAL_PG_MAJOR"
    fi
fi

# ── 7. 显式运维意图 ──────────────────────────────────────────────────────────
if [ "$DRY_RUN" = "1" ]; then
    cat >&2 <<EOF

$(printf '%s[ OK ]%s' "$C_OK" "$C_END") --dry-run：校验全部通过，未改动任何状态
  归档        : $ARCHIVE
  目标安装根  : $FLOATCTF_HOME
  归档安装根  : $B_HOME
  平台版本    : ${B_VERSION:-?}（当前归档）
  PG 主版本   : ${B_PG_SERVER:-?}
  将恢复子集  : ${B_SUBSET:-all}
  已存在安装  : $([ "$EXISTING" = 1 ] && echo yes || echo no)
EOF
    exit 0
fi

if [ "$ASSUME_YES" != "1" ]; then
    if [ -t 0 ]; then
        echo "" >&2
        echo "即将把归档内容恢复到 $FLOATCTF_HOME，并重启平台。" >&2
        echo "此操作会覆盖该安装根下的数据库/配置/前端状态。" >&2
        printf '如要继续，请输入: RESTORE FLOATCTF\n> ' >&2
        read -r ans || die "中断：未确认，已中止（未改动任何状态）"
        [ "$ans" = "RESTORE FLOATCTF" ] || die "确认文本不匹配，已中止（未改动任何状态）"
    else
        die "非交互环境必须显式传 --yes"
    fi
fi

# ── 8. 只停止本安装根的服务 ──────────────────────────────────────────────────
# compose 需要一个可用的环境文件来解析 ${POSTGRES_PASSWORD:?} 这类必填变量。
# down 阶段目标 .env 尚未落盘，因此优先用归档内那份；随后统一改回目标 .env。
RC_ENV="$SQL_ROOT/env/.env"
if [ ! -f "$RC_ENV" ]; then
    RC_ENV="$FLOATCTF_HOME/.env"
fi
[ -f "$RC_ENV" ] || die "既无归档内 env/.env 也无目标 $FLOATCTF_HOME/.env，无法渲染 compose（--only 需包含 env）"

PROJECT="$(sed -nE 's/^name:[[:space:]]*([A-Za-z0-9_.-]+)[[:space:]]*$/\1/p' "$SQL_ROOT/compose.prod.yml" 2>/dev/null | head -1)"
[ -n "$PROJECT" ] || PROJECT="floatctf"
if [ -f "$FLOATCTF_HOME/compose.prod.yml" ]; then
    PROJECT="$(sed -nE 's/^name:[[:space:]]*([A-Za-z0-9_.-]+)[[:space:]]*$/\1/p' "$FLOATCTF_HOME/compose.prod.yml" | head -1)"
    [ -n "$PROJECT" ] || PROJECT="floatctf"
fi
info "停止 FloatCTF 服务（project=$PROJECT；不使用 down -v，不动任何卷/无关资源）"
docker compose --env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml" down --remove-orphans >/dev/null 2>&1 || true
# .env 已落盘（want env）：后续统一使用目标安装根的 .env。
[ -f "$FLOATCTF_HOME/.env" ] && RC_ENV="$FLOATCTF_HOME/.env"
SERVICES_STOPPED=1

# ── 9. 落盘文件层 ────────────────────────────────────────────────────────────
install_owned() { # src dst mode
    local src="$1" dst="$2" mode="$3"
    [ -e "$src" ] || return 0
    install -D -m "$mode" "$src" "$dst"
}
restore_tree() { # src dst
    local src="$1" dst="$2"
    [ -d "$src" ] || return 0
    mkdir -p "$dst"
    tar -C "$src" -cf - . | tar -C "$dst" -xf - --no-same-owner
}

mkdir -p "$FLOATCTF_HOME"
info "恢复文件层 → $FLOATCTF_HOME"

if want env; then
    if [ -f "$SQL_ROOT/env/.env" ]; then
        install_owned "$SQL_ROOT/env/.env" "$FLOATCTF_HOME/.env" 0600
        ok "已恢复 .env（密钥）"
    else
        warn "归档不含 env/.env（--only 子集未包含？）"
    fi
fi
if want config; then
    restore_tree "$SQL_ROOT/config" "$FLOATCTF_HOME/config"
    chmod 0750 "$FLOATCTF_HOME/config" 2>/dev/null || true
    chmod 0640 "$FLOATCTF_HOME/config/floatctf.toml" "$FLOATCTF_HOME/config/caddy/Caddyfile" 2>/dev/null || true
    ok "已恢复 config/"
fi
want merge && install_owned "$SQL_ROOT/merged.sql" "$FLOATCTF_HOME/merged.sql" 0644
install_owned "$SQL_ROOT/frontend.sh" "$FLOATCTF_HOME/frontend.sh" 0755
install_owned "$SQL_ROOT/uninstall.sh" "$FLOATCTF_HOME/uninstall.sh" 0750
install_owned "$SQL_ROOT/compose.prod.yml" "$FLOATCTF_HOME/compose.prod.yml" 0644
install_owned "$SQL_ROOT/.initialized" "$FLOATCTF_HOME/.initialized" 0644
for tmpl in .floatctf.toml.tmpl .Caddyfile.tmpl; do
    [ -f "$SQL_ROOT/config/$tmpl" ] && install_owned "$SQL_ROOT/config/$tmpl" "$FLOATCTF_HOME/$tmpl" 0640
done

if want frontends && [ -d "$SQL_ROOT/frontends" ]; then
    rm -rf -- "$FLOATCTF_HOME/frontends"
    restore_tree "$SQL_ROOT/frontends" "$FLOATCTF_HOME/frontends"
    chmod 0755 "$FLOATCTF_HOME/frontends" 2>/dev/null || true
    chmod 0644 "$FLOATCTF_HOME/frontends/registry.json" 2>/dev/null || true
    ok "已恢复 frontends/（含 registry.json）"
fi
if want web && [ -d "$SQL_ROOT/web" ]; then
    rm -rf -- "$FLOATCTF_HOME/web"
    restore_tree "$SQL_ROOT/web" "$FLOATCTF_HOME/web"
    ok "已恢复 web/"
fi
if want runtime && [ -d "$SQL_ROOT/runtime" ]; then
    mkdir -p "$FLOATCTF_HOME/runtime"
    restore_tree "$SQL_ROOT/runtime" "$FLOATCTF_HOME/runtime"
    ok "已恢复 runtime/（challenges / gameboxes）"
fi
if want caddy; then
    for d in data/caddy data/caddy-config; do
        [ -d "$SQL_ROOT/$d" ] || continue
        mkdir -p "$FLOATCTF_HOME/$d"
        restore_tree "$SQL_ROOT/$d" "$FLOATCTF_HOME/$d"
    done
    ok "已恢复 Caddy 证书/账户状态"
fi

# ── 10. 属主/权限（与 install.sh 的契约一致；非 root 时只告警）────────────────
FCTF_GID="$(getent group floatctf | cut -d: -f3 || true)"
# 容器/精简宿主可能没有 floatctf 组条目（getent 为空），此时允许显式指定，
# 否则 API 容器（数值 uid:floatctf-gid）会读不到 0640 的配置。
if [ -z "$FCTF_GID" ] && [ -n "${FLOATCTF_GROUP_GID:-}" ]; then
    FCTF_GID="$FLOATCTF_GROUP_GID"
    info "使用 FLOATCTF_GROUP_GID=$FCTF_GID 作为 floatctf 组 GID"
fi
if [ "$(id -u)" -eq 0 ] && [ -z "$FCTF_GID" ]; then
    warn "无法解析 floatctf 组 GID（容器/精简宿主？）→ 配置属主将回退 root:root，"
    warn "生产 API（数值 uid:floatctf-gid）可能读不到 0640 的 floatctf.toml。"
    warn "请设置 FLOATCTF_GROUP_GID=<gid> 后重跑 restore，或手动 chown。"
fi
if [ "$(id -u)" -eq 0 ]; then
    [ -n "$FCTF_GID" ] && chown -R root:"$FCTF_GID" "$FLOATCTF_HOME/config" 2>/dev/null || true
    [ -n "$FCTF_GID" ] && chown root:"$FCTF_GID" "$FLOATCTF_HOME/.env" 2>/dev/null || true
    chmod 0640 "$FLOATCTF_HOME/.env" 2>/dev/null || true
    chown -R 999:999 "$FLOATCTF_HOME/data/postgres" 2>/dev/null || true
    chown -R 10001:10001 "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    [ -n "$FCTF_GID" ] && chgrp -R "$FCTF_GID" "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    chmod -R g+rX "$FLOATCTF_HOME/data/rustfs" "$FLOATCTF_HOME/logs/rustfs" 2>/dev/null || true
    chown -R 65532:65532 "$FLOATCTF_HOME/runtime" 2>/dev/null || true
    chown root:root "$FLOATCTF_HOME/frontends" "$FLOATCTF_HOME/frontends/registry.json" 2>/dev/null || true
    chown root:root "$FLOATCTF_HOME/frontend.sh" 2>/dev/null || true
    chown root:"${FCTF_GID:-root}" "$FLOATCTF_HOME/uninstall.sh" 2>/dev/null || true
    ok "已按安装契约恢复属主/权限"
else
    warn "以非 root 运行：跳过属主修正（仅适用于测试用安装根）"
fi

# ── 11. RustFS / Redis 数据 ──────────────────────────────────────────────────
if want rustfs && [ -f "$SQL_ROOT/data/rustfs.tar" ]; then
    info "恢复 RustFS 数据"
    mkdir -p "$FLOATCTF_HOME/data/rustfs"
    rm -rf -- "$FLOATCTF_HOME/data/rustfs"
    mkdir -p "$FLOATCTF_HOME/data"
    tar -C "$FLOATCTF_HOME" -xf "$SQL_ROOT/data/rustfs.tar" --no-same-owner || die "RustFS 数据恢复失败" 2
    [ "$(id -u)" -eq 0 ] && { chown -R 10001:10001 "$FLOATCTF_HOME/data/rustfs" 2>/dev/null || true; }
    ok "已恢复 RustFS 数据目录"
fi
if want redis && [ -d "$SQL_ROOT/data/redis" ]; then
    info "恢复 Redis 持久化文件"
    REDIS_VOL="$(docker volume ls --format '{{.Name}}' | grep -E "(^|_)floatctf-redis-data$" | head -1 || true)"
    docker compose --env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml" up -d redis >/dev/null 2>&1 || true
    REDIS_CID="$(docker compose --env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml" ps -q redis 2>/dev/null | head -1)"
    if [ -n "$REDIS_CID" ]; then
        docker exec "$REDIS_CID" sh -c 'rm -rf /data/*' 2>/dev/null || true
        tar -C "$SQL_ROOT/data/redis" -cf - . | docker exec -i "$REDIS_CID" tar -C /data -xf - 2>/dev/null \
            && ok "已恢复 Redis 持久化文件（容器 $REDIS_CID）" \
            || warn "Redis 数据注入失败（可重建，不阻断恢复）"
    else
        warn "无法启动 redis 容器以注入数据（卷 ${REDIS_VOL:-?}）"
    fi
fi

# ── 12. PostgreSQL ───────────────────────────────────────────────────────────
if want postgres; then
    info "恢复 PostgreSQL"
    COMPOSE_ARGS=(--env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml")
    docker compose "${COMPOSE_ARGS[@]}" up -d postgres >/dev/null 2>&1 || die "启动 postgres 失败" 2
    for i in $(seq 1 60); do
        state="$(docker inspect -f '{{.State.Health.Status}}' "$(docker compose "${COMPOSE_ARGS[@]}" ps -q postgres | head -1)" 2>/dev/null || echo unknown)"
        [ "$state" = "healthy" ] && break
        sleep 2
    done
    [ "${state:-}" = "healthy" ] || die "postgres 未在 120s 内 healthy（state=$state）" 2

    PGCID="$(docker compose "${COMPOSE_ARGS[@]}" ps -q postgres | head -1)"
    # 断开既有连接 → DROP → CREATE → pg_restore（官方支持的工具链，不直连数据目录）。

    docker compose "${COMPOSE_ARGS[@]}" exec -T postgres sh -c '
        set -e
        export PGPASSWORD="$POSTGRES_PASSWORD"
        psql -U "$POSTGRES_USER" -d postgres -v ON_ERROR_STOP=1 -c \
          "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '"'"'$POSTGRES_DB'"'"' AND pid <> pg_backend_pid();" >/dev/null
        dropdb -U "$POSTGRES_USER" --if-exists "$POSTGRES_DB"
        createdb -U "$POSTGRES_USER" "$POSTGRES_DB"
    ' || die "重建数据库失败" 2

    docker exec -i "$PGCID" sh -c '
        export PGPASSWORD="$POSTGRES_PASSWORD"
        pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --no-owner --no-acl --exit-on-error -v
    ' < "$SQL_ROOT/data/postgres.pgc" 2>"$WORK_DIR/pg_restore.log" \
        || { tail -30 "$WORK_DIR/pg_restore.log" >&2 || true; die "pg_restore 失败（详见上方日志）" 2; }
    ROWS="$(docker compose "${COMPOSE_ARGS[@]}" exec -T postgres sh -c \
        'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tAc "select count(*) from public.schema_migrations"' 2>/dev/null | tr -d '\r\n ' || echo '?')"
    ok "PostgreSQL 已恢复（schema_migrations 记录数=$ROWS）"
fi

# ── 13. 启动并健康检查 ───────────────────────────────────────────────────────
if [ "$NO_START" = "1" ]; then
    ok "已完成恢复（--no-start：平台未启动）"
    exit 0
fi

info "启动平台并健康检查"
docker compose --env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml" up -d >/dev/null 2>&1 || die "启动平台失败" 2

ALL_HEALTHY=0
for i in $(seq 1 60); do
    ALL_HEALTHY=1
    for svc in postgres redis rustfs api caddy; do
        cid="$(docker compose --env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml" ps -q "$svc" 2>/dev/null | head -1)"
        [ -n "$cid" ] || { ALL_HEALTHY=0; break; }
        st="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$cid" 2>/dev/null || echo unknown)"
        case "$st" in healthy|running) ;; *) ALL_HEALTHY=0; break ;; esac
    done
    [ "$ALL_HEALTHY" = "1" ] && break
    sleep 3
done

if [ "$ALL_HEALTHY" != "1" ]; then
    docker compose --env-file "$RC_ENV" -p "$PROJECT" -f "$SQL_ROOT/compose.prod.yml" ps >&2 || true
    die "平台未在 180s 内全部 healthy；请查看 docker compose logs" 2
fi

ok "恢复完成：全部服务 healthy"
cat >&2 <<EOF

恢复摘要
  归档        : $ARCHIVE
  安装根      : $FLOATCTF_HOME
  平台版本    : ${B_VERSION:-?}
  PG 主版本   : ${B_PG_SERVER:-?}
  恢复子集    : ${B_SUBSET:-all}

建议核对:
  1) 管理员登录 + 赛事/题目数据
  2) sudo $FLOATCTF_HOME/frontend.sh list   （已安装前端与 currentVersion）
  3) 站点可访问 + API /api/admin/system/version
EOF
