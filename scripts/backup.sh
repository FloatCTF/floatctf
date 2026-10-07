#!/usr/bin/env bash
#
# FloatCTF 生产备份（Phase 13）— 单文件自包含，随 release 产物发布。
#
# 目标：在**不破坏运行中平台**的前提下，产出一个足以恢复完整应用状态的归档。
# 内容与取舍理由见 docs/agents/BACKUP.md。
#
# 收录（相对安装根）：
#   META / MANIFEST       元数据与逐成员 sha256
#   env/.env              全部密钥（JWT / awd_root_key / internal_token_key / DB / RustFS）
#   config/**             floatctf.toml + Caddyfile + 安装器模板
#   merged.sql            fresh-DB bootstrap（可再生产物）
#   compose.prod.yml      生成态 compose（可再生产物）
#   frontend.sh           前端管理器（可再生产物）
#   uninstall.sh          生命周期工具
#   frontends/**          已安装前端 + registry.json（含第三方前端 —— 不可再生）
#   web/**                bootstrap 引导页（可再生产物）
#   runtime/**            API work_dir：challenges / gameboxes（**不可再生**）
#   data/postgres.pgc     PostgreSQL 逻辑备份（pg_dump custom 格式）
#   data/rustfs.tar       RustFS 数据目录（**静态一致**：备份期间短暂停写）
#   data/redis/**         Redis 持久化文件（可重建）
#   data/caddy/**         ACME 证书/账户状态
#   data/caddy-config/**
#
# 刻意不收录：logs/、image/api/floatctf、Docker 镜像、构建缓存（见 docs/agents/BACKUP.md）。
#
# 用法：sudo $FLOATCTF_HOME/backup.sh [选项]
#   --home DIR        安装根（默认 $FLOATCTF_HOME，再默认 /var/lib/floatctf）
#   --out FILE        输出归档（默认 ./floatctf-backup-<UTC 时间戳>.tar.gz）
#   --compose-file F  compose 文件（默认 <home>/compose.prod.yml）
#   --no-quiesce      不停写 RustFS（**不保证** RustFS 一致；仅测试/离线用）
#   --offline         平台未运行：跳过全部数据面，仅备份文件层
#   --only LIST       子集：env,config,merge,frontends,web,runtime,postgres,rustfs,redis,caddy
#   --force           输出文件已存在时覆盖
#   --quiet           仅向 stdout 打印归档路径
#   -h|--help
#
# 退出码：0 成功；1 用法/前置条件错误；2 数据面失败（已清理半成品并恢复 RustFS）。
#
set -Eeuo pipefail

BACKUP_FORMAT_VERSION="1"
BACKUP_TOOL="floatctf-backup"

FLOATCTF_HOME="${FLOATCTF_HOME:-/var/lib/floatctf}"
OUT_FILE=""
COMPOSE_FILE=""
ONLY=""
QUIESCE=1
OFFLINE=0
FORCE=0
QUIET=0

if [ -t 1 ]; then
    C_OK=$'\033[0;32m'; C_END=$'\033[0m'
else
    C_OK=''; C_END=''
fi
info() { [ "$QUIET" = "1" ] || printf '[INFO] %s\n' "$*" >&2; }
ok()   { [ "$QUIET" = "1" ] || printf '%s[ OK ]%s %s\n' "$C_OK" "$C_END" "$*" >&2; }
warn() { printf '[WARN] %s\n' "$*" >&2; }
die()  { printf '[FAIL] %s\n' "$*" >&2; exit "${2:-1}"; }

STAGE_DIR=""
QUIESCED=0
PACKED=0

# 临时目录清理必须**永不失败**：归档已经写好后再因为清理失败而整体退非零，
# 会让运维误以为备份没成功。前端资产是 0555/0444 的不可变树，故先补写位。
drop_stage() {
    [ -n "${STAGE_DIR:-}" ] && [ -d "$STAGE_DIR" ] || return 0
    chmod -R u+w "$STAGE_DIR" 2>/dev/null || true
    rm -rf -- "$STAGE_DIR" 2>/dev/null || true
}
cleanup() {
    local rc=$?
    # 失败/中断时必须把 RustFS 还原为运行态，否则会留下一个静默停写的平台。
    if [ "$QUIESCED" = "1" ]; then
        warn "备份中断：恢复 RustFS 运行态"
        compose_cmd start rustfs >/dev/null 2>&1 || true
    fi
    drop_stage
    exit "$rc"
}
trap cleanup EXIT INT TERM

usage() { sed -n '2,45p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --home) FLOATCTF_HOME="${2:?--home 需要目录}"; shift ;;
        --out) OUT_FILE="${2:?--out 需要文件路径}"; shift ;;
        --compose-file) COMPOSE_FILE="${2:?--compose-file 需要文件路径}"; shift ;;
        --no-quiesce) QUIESCE=0 ;;
        --offline) OFFLINE=1 ;;
        --only) ONLY="${2:?--only 需要列表}"; shift ;;
        --force) FORCE=1 ;;
        --quiet) QUIET=1 ;;
        -h|--help) usage; exit 0 ;;
        *) die "未知参数: $1（--help 查看用法）" ;;
    esac
    shift
done

# ── 前置条件 ─────────────────────────────────────────────────────────────────
[ -d "$FLOATCTF_HOME" ] || die "安装根不存在: $FLOATCTF_HOME"
[ -f "$FLOATCTF_HOME/.env" ] || die "缺少 $FLOATCTF_HOME/.env（不是有效安装根？）"
[ -z "$COMPOSE_FILE" ] && COMPOSE_FILE="$FLOATCTF_HOME/compose.prod.yml"
[ -f "$COMPOSE_FILE" ] || die "缺少 compose 文件: $COMPOSE_FILE"
command -v python3 >/dev/null 2>&1 || die "缺少 python3"
command -v sha256sum >/dev/null 2>&1 || die "缺少 sha256sum"
# 确定性归档与成员枚举依赖 **GNU** coreutils/tar/find（--sort/--mtime/-printf）。
# 安装器会在宿主装 postgresql/tar 等；精简宿主请确保是 GNU 版本（BusyBox 不支持）。
tar --sort=name --mtime='@0' -cf /dev/null --files-from /dev/null 2>/dev/null \
    || die "需要 GNU tar（支持 --sort/--mtime）；BusyBox tar 会让归档失去确定性"
find . -maxdepth 0 -printf '' >/dev/null 2>&1 \
    || die "需要 GNU find（支持 -printf）"
command -v tar >/dev/null 2>&1 || die "缺少 tar"
command -v gzip >/dev/null 2>&1 || die "缺少 gzip"

if [ "$(id -u)" -ne 0 ]; then
    # 生产 .env 与 config/floatctf.toml 是 root:floatctf 0640；读不到就提前失败，
    # 避免产出一个缺密钥的残缺归档。
    if ! [ -r "$FLOATCTF_HOME/.env" ] || ! [ -r "$FLOATCTF_HOME/config/floatctf.toml" ]; then
        die "无法读取 .env / config/floatctf.toml；请用 sudo 运行（或加入 floatctf 组）"
    fi
fi

compose_cmd() {
    docker compose --env-file "$FLOATCTF_HOME/.env" -f "$COMPOSE_FILE" -p "$PROJECT" "$@"
}

PROJECT="$(sed -nE 's/^name:[[:space:]]*([A-Za-z0-9_.-]+)[[:space:]]*$/\1/p' "$COMPOSE_FILE" | head -1)"
[ -n "$PROJECT" ] || PROJECT="$(basename "$FLOATCTF_HOME" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9_.-')"
[ -n "$PROJECT" ] || die "无法确定 compose project name"

# ── 输出路径 ─────────────────────────────────────────────────────────────────
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
[ -n "$OUT_FILE" ] || OUT_FILE="$PWD/floatctf-backup-$STAMP.tar.gz"
case "$OUT_FILE" in /*) ;; *) OUT_FILE="$PWD/$OUT_FILE" ;; esac
if [ -e "$OUT_FILE" ] && [ "$FORCE" != "1" ]; then
    die "输出文件已存在: $OUT_FILE（--force 覆盖）"
fi
OUT_DIR="$(dirname "$OUT_FILE")"
[ -d "$OUT_DIR" ] || die "输出目录不存在: $OUT_DIR"

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

# ── 运行态 ───────────────────────────────────────────────────────────────────
PLATFORM_RUNNING=0
if [ "$OFFLINE" != "1" ] && command -v docker >/dev/null 2>&1; then
    if compose_cmd ps --status running -q 2>/dev/null | grep -q .; then
        PLATFORM_RUNNING=1
    fi
fi
if [ "$OFFLINE" != "1" ] && [ "$PLATFORM_RUNNING" != "1" ]; then
    warn "未检测到运行中的平台容器（project=$PROJECT）"
fi
if want postgres && [ "$PLATFORM_RUNNING" != "1" ]; then
    die "postgres 备份需要运行中的平台；请启动平台，或用 --only 排除 postgres"
fi
if { want rustfs || want redis; } && [ "$PLATFORM_RUNNING" != "1" ]; then
    warn "rustfs/redis 未运行：将跳过其数据面"
fi

# ── 元数据 ───────────────────────────────────────────────────────────────────
PLATFORM_VERSION="$(sed -nE 's/^VERSION=(.*)$/\1/p' "$FLOATCTF_HOME/.env" | head -1)"
[ -n "$PLATFORM_VERSION" ] || PLATFORM_VERSION="unknown"
DB_NAME="$(sed -nE 's/^POSTGRES_DB=(.*)$/\1/p' "$FLOATCTF_HOME/.env" | head -1)"
DB_USER="$(sed -nE 's/^POSTGRES_USER=(.*)$/\1/p' "$FLOATCTF_HOME/.env" | head -1)"
[ -n "$DB_NAME" ] || DB_NAME="floatctf_db"
[ -n "$DB_USER" ] || DB_USER="postgres"

PG_SERVER_VERSION="unknown"
PG_MAJOR="unknown"
PG_DUMP_VERSION="unknown"
if [ "$PLATFORM_RUNNING" = "1" ]; then
    PG_SERVER_VERSION="$(compose_cmd exec -T postgres sh -c \
        'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tAc "show server_version"' 2>/dev/null | tr -d '\r\n ' || true)"
    [ -n "$PG_SERVER_VERSION" ] || PG_SERVER_VERSION="unknown"
    case "$PG_SERVER_VERSION" in [0-9]*) PG_MAJOR="${PG_SERVER_VERSION%%.*}" ;; esac
    PG_DUMP_VERSION="$(compose_cmd exec -T postgres pg_dump --version 2>/dev/null | tr -d '\r\n' || true)"
    [ -n "$PG_DUMP_VERSION" ] || PG_DUMP_VERSION="unknown"
fi

STAGE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-backup.XXXXXX")"
chmod 700 "$STAGE_DIR"
mkdir -p "$STAGE_DIR/env" "$STAGE_DIR/config" "$STAGE_DIR/data"

# ── 文件层 ───────────────────────────────────────────────────────────────────
# 用 tar 管道而非 cp -a：跨 overlay/挂载边界行为一致，且可统一排除规则。
copy_tree() { # src dst [tar 额外参数...]
    local src="$1" dst="$2"
    shift 2
    mkdir -p "$dst"
    if ! tar -C "$src" -cf - "$@" 2>"$STAGE_DIR/.tar.err" | tar -C "$dst" -xf - 2>>"$STAGE_DIR/.tar.err"; then
        sed -n '1,10p' "$STAGE_DIR/.tar.err" >&2 || true
        die "复制失败: $src（上为 tar 原因；若为 Permission denied，请用 sudo 运行 backup.sh）"
    fi
    rm -f "$STAGE_DIR/.tar.err"
}

if want env; then
    info "备份密钥 .env"
    install -m 0600 "$FLOATCTF_HOME/.env" "$STAGE_DIR/env/.env"
fi

if want config; then
    info "备份 config/"
    copy_tree "$FLOATCTF_HOME/config" "$STAGE_DIR/config" .
    for tmpl in .floatctf.toml.tmpl .Caddyfile.tmpl; do
        [ -f "$FLOATCTF_HOME/$tmpl" ] && install -m 0640 "$FLOATCTF_HOME/$tmpl" "$STAGE_DIR/config/$tmpl"
    done
fi

if want merge && [ -f "$FLOATCTF_HOME/merged.sql" ]; then
    info "备份 merged.sql"
    install -m 0644 "$FLOATCTF_HOME/merged.sql" "$STAGE_DIR/merged.sql"
fi

for f in frontend.sh uninstall.sh compose.prod.yml; do
    [ -f "$FLOATCTF_HOME/$f" ] || continue
    if [ "$f" = "frontend.sh" ]; then
        install -m 0755 "$FLOATCTF_HOME/$f" "$STAGE_DIR/$f"
    else
        install -m 0644 "$FLOATCTF_HOME/$f" "$STAGE_DIR/$f"
    fi
done
[ -f "$FLOATCTF_HOME/.initialized" ] && install -m 0644 "$FLOATCTF_HOME/.initialized" "$STAGE_DIR/.initialized"

if want frontends && [ -d "$FLOATCTF_HOME/frontends" ]; then
    info "备份已安装前端 + registry.json"
    copy_tree "$FLOATCTF_HOME/frontends" "$STAGE_DIR/frontends" .
fi

if want web && [ -d "$FLOATCTF_HOME/web" ]; then
    info "备份 bootstrap 静态页"
    copy_tree "$FLOATCTF_HOME/web" "$STAGE_DIR/web" .
fi

if want runtime && [ -d "$FLOATCTF_HOME/runtime" ]; then
    info "备份 runtime/（challenges / gameboxes）"
    copy_tree "$FLOATCTF_HOME/runtime" "$STAGE_DIR/runtime" --exclude=./logs --exclude=./.bash_history .
fi

if want caddy; then
    for d in data/caddy data/caddy-config; do
        [ -d "$FLOATCTF_HOME/$d" ] || continue
        info "备份 $d（证书 / ACME 账户状态）"
        copy_tree "$FLOATCTF_HOME/$d" "$STAGE_DIR/$d" .
    done
fi

# ── PostgreSQL（逻辑备份）────────────────────────────────────────────────────
if want postgres; then
    info "备份 PostgreSQL（pg_dump custom 格式；server=$PG_SERVER_VERSION pg_dump=$PG_DUMP_VERSION）"
    if ! compose_cmd exec -T postgres sh -c \
            'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom --no-owner --no-acl' \
            > "$STAGE_DIR/data/postgres.pgc" 2>"$STAGE_DIR/postgres.err"; then
        sed -n '1,20p' "$STAGE_DIR/postgres.err" >&2 || true
        die "pg_dump 失败" 2
    fi
    rm -f "$STAGE_DIR/postgres.err"
    # 归档必须以 PostgreSQL custom-format 魔数 "PGDMP" 开头。
    if [ "$(head -c 5 "$STAGE_DIR/data/postgres.pgc")" != "PGDMP" ]; then
        die "pg_dump 输出不是合法 custom-format 归档" 2
    fi
    chmod 0600 "$STAGE_DIR/data/postgres.pgc"
fi

# ── Redis（可重建，但承载 AWD/AWDP 运行态）───────────────────────────────────
if want redis && [ "$PLATFORM_RUNNING" = "1" ]; then
    info "备份 Redis 持久化文件"
    REDIS_CID="$(compose_cmd ps -q redis 2>/dev/null | head -1)"
    if [ -n "$REDIS_CID" ]; then
        compose_cmd exec -T redis redis-cli BGSAVE >/dev/null 2>&1 || true
        mkdir -p "$STAGE_DIR/data/redis"
        if docker exec "$REDIS_CID" sh -c 'cd /data && tar -cf - .' 2>/dev/null \
                | tar -C "$STAGE_DIR/data/redis" -xf - --no-same-owner 2>/dev/null; then
            [ -n "$(ls -A "$STAGE_DIR/data/redis" 2>/dev/null)" ] || {
                rmdir "$STAGE_DIR/data/redis" 2>/dev/null || true
                warn "Redis 无持久化文件（纯内存实例？）"
            }
        else
            rmdir "$STAGE_DIR/data/redis" 2>/dev/null || true
            warn "Redis 数据复制失败（可重建，不阻断备份）"
        fi
    else
        warn "未找到 redis 容器，跳过"
    fi
fi

# ── RustFS（静态一致：短暂停写）──────────────────────────────────────────────
if want rustfs && [ "$PLATFORM_RUNNING" = "1" ] && [ -d "$FLOATCTF_HOME/data/rustfs" ]; then
    if [ "$QUIESCE" = "1" ]; then
        info "停写 RustFS 以取得一致快照（仅 FloatCTF 自身维护窗口）"
        compose_cmd stop rustfs >/dev/null 2>&1 || die "停止 rustfs 失败" 2
        QUIESCED=1
    else
        warn "--no-quiesce：RustFS 写入中复制，**不保证**一致性"
    fi
    info "打包 RustFS 数据目录"
    tar -C "$FLOATCTF_HOME" -cf "$STAGE_DIR/data/rustfs.tar" data/rustfs || die "打包 RustFS 数据失败" 2
    if [ "$QUIESCED" = "1" ]; then
        compose_cmd start rustfs >/dev/null 2>&1 || die "恢复 rustfs 运行失败（请手动 compose start rustfs）" 2
        QUIESCED=0
        ok "RustFS 已恢复运行"
    fi
fi

# ── META + MANIFEST ──────────────────────────────────────────────────────────
info "生成 META / MANIFEST"
MEMBER_COUNT="$(
    cd "$STAGE_DIR"
    find . -type f -not -name MANIFEST -not -name META -printf '%P\n' \
        | LC_ALL=C sort \
        | while IFS= read -r rel; do
            printf '%s  %s\n' "$(sha256sum "$rel" | cut -d' ' -f1)" "$rel"
        done > MANIFEST
    wc -l < MANIFEST | tr -d ' '
)"
(
    cd "$STAGE_DIR"
    {
        printf 'BACKUP_FORMAT=%s\n' "$BACKUP_FORMAT_VERSION"
        printf 'TOOL=%s\n' "$BACKUP_TOOL"
        printf 'CREATED_AT=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'HOSTNAME=%s\n' "$(hostname 2>/dev/null || echo unknown)"
        printf 'FLOATCTF_HOME=%s\n' "$FLOATCTF_HOME"
        printf 'COMPOSE_PROJECT=%s\n' "$PROJECT"
        printf 'PLATFORM_VERSION=%s\n' "$PLATFORM_VERSION"
        printf 'POSTGRES_MAJOR=%s\n' "$PG_MAJOR"
        printf 'POSTGRES_SERVER_VERSION=%s\n' "$PG_SERVER_VERSION"
        printf 'PG_DUMP_VERSION=%s\n' "$PG_DUMP_VERSION"
        printf 'DB_NAME=%s\n' "$DB_NAME"
        printf 'DB_USER=%s\n' "$DB_USER"
        printf 'MEMBER_COUNT=%s\n' "$MEMBER_COUNT"
        printf 'SUBSET=%s\n' "${ONLY:-all}"
        printf 'RUSTFS_QUIESCED=%s\n' "$([ "$QUIESCE" = "1" ] && echo yes || echo no)"
        printf 'ARCHIVE_PLAINTEXT=yes\n'
    } > META
)

# ── 打包（确定性）────────────────────────────────────────────────────────────
# 排序 + --mtime=@0 + owner/group 归零 + gzip -n：相同输入产生相同字节。
info "打包 → $OUT_FILE"
umask 077
STAGE_PARENT="$(dirname "$STAGE_DIR")"
STAGE_NAME="$(basename "$STAGE_DIR")"
if ! tar -C "$STAGE_PARENT" \
        --sort=name --numeric-owner --owner=0 --group=0 --mtime='@0' --format=gnu \
        -cf - "$STAGE_NAME" | gzip -n -9 > "$OUT_FILE.part"; then
    rm -f "$OUT_FILE.part"
    die "打包失败" 2
fi
mv "$OUT_FILE.part" "$OUT_FILE"
chmod 0600 "$OUT_FILE"
sha256sum "$OUT_FILE" | awk '{print $1}' > "$OUT_FILE.sha256"
chmod 0600 "$OUT_FILE.sha256"

PACKED=1
drop_stage
STAGE_DIR=""

SIZE="$(du -h "$OUT_FILE" | cut -f1)"
SHA="$(cat "$OUT_FILE.sha256")"
if [ "$QUIET" = "1" ]; then
    printf '%s\n' "$OUT_FILE"
else
    cat >&2 <<EOF

$(printf '%s[ OK ]%s' "$C_OK" "$C_END") 备份完成
  归档   : $OUT_FILE
  大小   : $SIZE
  sha256 : $SHA
  校验   : $OUT_FILE.sha256
  成员   : $MEMBER_COUNT 个（MANIFEST 记录逐项哈希）

恢复:
  sudo $FLOATCTF_HOME/restore.sh $OUT_FILE --yes

注意：归档是**明文**（含 .env 密钥与 Caddy 私钥），权限已设为 0600。
      请转移到受控介质，不要把内容粘进工单/日志。
EOF
fi
