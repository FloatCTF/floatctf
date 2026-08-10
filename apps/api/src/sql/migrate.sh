#!/usr/bin/env bash
set -Eeuo pipefail

# ================================================================================
# FloatCTF migration manager（forward-only）
#
# migrations/    → migration 唯一 source of truth（YYYYMMDDHHMMSS-name.sql）
# migrate.sh     → runner / validator / generator / merger
# schema_migrations → 数据库实际执行过的 migration 记录（由 migrate.sh 独占管理）
# merged.sql     → 从 migrations 确定性生成的 fresh database bootstrap
#
# 事务所有权归 migrate.sh：每个 migration 与它的 schema_migrations 记录
# 在同一个事务中提交（migration 文件自身不允许包含 transaction control）。
# ================================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# migrate.sh 位于 <repo>/apps/api/src/sql/，向上 4 级才是 repository root
# （sql/.. → src，sql/../.. → api，sql/../../.. → apps，sql/../../../.. → repo）。
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

# 测试可覆盖内部路径（FLOATCTF_MIGRATIONS_DIR/FLOATCTF_MERGED_FILE 仅用于隔离测试，
# 生产路径永远来自脚本自身位置）。
MIGRATIONS_DIR="${FLOATCTF_MIGRATIONS_DIR:-$SCRIPT_DIR/migrations}"
MERGED_FILE="${FLOATCTF_MERGED_FILE:-$SCRIPT_DIR/merged.sql}"
LOCK_FILE="$SCRIPT_DIR/.migrate.lock"

# FloatCTF migration runner 专用的固定 advisory lock key。
# 0x464C4154 = 'FLAT'（FloatCTF），防止任意两个 migration runner
# （CI A/B、server A/B）并发迁移同一个 PostgreSQL database。
ADVISORY_LOCK_ID=1179204948

# schema_migrations 由 migrate.sh 独占管理，普通 migration 不得创建/删除它。
SCHEMA_MIGRATIONS_DDL='CREATE TABLE IF NOT EXISTS public.schema_migrations (
    version BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    checksum CHAR(64) NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);'

# ── 本地 migration 索引（build_local_history 填充）────────────────────────────
declare -A LOCAL_VERSION_NAME=()
declare -A LOCAL_VERSION_FILE=()
declare -A LOCAL_VERSION_CHECKSUM=()
LOCAL_ORDER=()

# ── 数据库 migration 历史（build_db_history 填充）─────────────────────────────
declare -A DB_VERSION_NAME=()
declare -A DB_VERSION_CHECKSUM=()
DB_APPLIED_VERSIONS=()

DB_STATE=untracked


usage() {
    cat <<'EOF'
用法：

  ./migrate.sh new [name]
      创建新 migration 文件（YYYYMMDDHHMMSS-name.sql）。
      名称直接传参，或未传参时交互式询问。

  ./migrate.sh list
      按执行顺序列出本地 migrations（纯本地，不连接数据库）。

  ./migrate.sh validate
      校验本地 migrations（文件名/version 唯一/内容/禁止 transaction control 等），
      不连接数据库。

  ./migrate.sh status
      对比本地 migrations 与数据库 schema_migrations 状态
      （APPLIED / PENDING / MODIFIED / MISSING LOCAL / HISTORY MISMATCH）。

  ./migrate.sh verify
      验证已应用 migration 历史完整、未被修改。本地 PENDING 不算错误。

  ./migrate.sh apply
      执行 pending migrations。每迁移独立事务 + schema_migrations 原子写入，
      全程持有 PostgreSQL advisory lock，防止并发 runner 重复执行。

  ./migrate.sh make
      从全部 migrations 确定性生成 merged.sql（fresh database bootstrap）。

  ./migrate.sh help
      显示帮助。

数据库相关命令统一读取 FLOATCTF_CONFIG（[database].url）。
EOF
}


# ── 基础工具 ──────────────────────────────────────────────────────────────────

die() {
    printf '错误：%s\n' "$*" >&2
    exit 1
}


die_usage() {
    printf '错误：%s\n' "$*" >&2
    echo >&2
    usage >&2
    exit 2
}


# 数据库 URL 脱敏：只隐藏密码，便于日志展示。
mask_db_url() {
    printf '%s' "$1" | sed -E 's#(://[^:/@]*:)[^@]*@#\1***@#'
}


# SQL string literal：单引号翻倍后包裹引号（name/checksum 虽然已被严格校验，
# 仍做正确转义+引用，避免未来格式变化导致拼接注入）。
sql_literal() {
    local escaped
    escaped="$(printf '%s' "$1" | sed "s/'/''/g")"
    printf "'%s'\n" "$escaped"
}


# ── 配置加载（与 gen_entities.py 同一 source of truth：FLOATCTF_CONFIG）──────

load_db_url() {
    local config_path
    if ! config_path="$(python3 - "$PROJECT_ROOT" <<'PY'
import os, pathlib, sys
root = sys.argv[1]
value = os.getenv("FLOATCTF_CONFIG")
if not value or not value.strip():
    print("FLOATCTF_CONFIG is not set", file=sys.stderr)
    sys.exit(1)
p = pathlib.Path(value.strip()).expanduser()
if not p.is_absolute():
    p = pathlib.Path(root) / p
p = p.resolve()
if not p.is_file():
    print(f"config file not found: {p}", file=sys.stderr)
    sys.exit(1)
print(p)
PY
)"; then
        die "无法解析 FLOATCTF_CONFIG（见上方错误）"
    fi

    if ! DB_URL="$(python3 - "$config_path" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as fh:
    cfg = tomllib.load(fh)
try:
    url = cfg["database"]["url"]
except (KeyError, TypeError):
    print("缺少 [database].url", file=sys.stderr)
    sys.exit(1)
if not isinstance(url, str) or not url.strip():
    print("[database].url 无效", file=sys.stderr)
    sys.exit(1)
print(url.strip())
PY
)"; then
        die "解析 [database].url 失败（见上方错误）"
    fi

    export DB_URL
}


# ── 本地 migration 索引 ───────────────────────────────────────────────────────

normalize_name() {
    local input="$1"

    printf '%s' "$input" |
        tr '[:upper:]' '[:lower:]' |
        sed -E \
            -e 's/[[:space:]_]+/-/g' \
            -e 's/[^a-z0-9-]+//g' \
            -e 's/-+/-/g' \
            -e 's/^-+//' \
            -e 's/-+$//'
}


list_migrations() {
    if [[ ! -d "$MIGRATIONS_DIR" ]]; then
        return 0
    fi

    find "$MIGRATIONS_DIR" \
        -maxdepth 1 \
        -type f \
        -printf '%f\n' |
        grep -E '^[0-9]{14}-[a-z0-9][a-z0-9-]*\.sql$' |
        sort
}


build_local_history() {
    LOCAL_ORDER=()
    LOCAL_VERSION_NAME=()
    LOCAL_VERSION_FILE=()
    LOCAL_VERSION_CHECKSUM=()

    local -a files=()
    mapfile -t files < <(list_migrations)

    local f v n sha
    for f in "${files[@]}"; do
        v="${f:0:14}"
        n="${f#*-}"
        n="${n%.sql}"
        sha="$(sha256sum "$MIGRATIONS_DIR/$f" | awk '{print $1}')"
        LOCAL_ORDER+=("$v")
        LOCAL_VERSION_NAME["$v"]="$n"
        LOCAL_VERSION_FILE["$v"]="$f"
        LOCAL_VERSION_CHECKSUM["$v"]="$sha"
    done
}


# ── 数据库查询助手 ────────────────────────────────────────────────────────────

db_psql() {
    psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$DB_URL" "$@"
}


db_query() {
    db_psql -c "$1"
}


db_has_schema_migrations() {
    [[ "$(db_query "SELECT to_regclass('public.schema_migrations') IS NOT NULL")" == "t" ]]
}


# 非系统 schema 中已有的用户业务表数量（排除 pg_catalog/information_schema，
# 也排除 schema_migrations 本身）。
db_user_table_count() {
    db_query "SELECT count(*) FROM information_schema.tables
        WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
          AND table_name <> 'schema_migrations'"
}


db_read_history() {
    psql -X -q -A -t -F'|' -v ON_ERROR_STOP=1 -d "$DB_URL" \
        -c "SELECT version, name, checksum FROM public.schema_migrations ORDER BY version"
}


build_db_history() {
    DB_VERSION_NAME=()
    DB_VERSION_CHECKSUM=()
    DB_APPLIED_VERSIONS=()

    # 完全空的数据库（无 schema_migrations）没有可读取的 history，
    # 此时所有本地 migration 都视为 PENDING（read-only status 必须支持 fresh DB）。
    if ! db_has_schema_migrations; then
        return 0
    fi

    local -a rows=()
    local row v n sha
    mapfile -t rows < <(db_read_history)
    for row in "${rows[@]}"; do
        IFS='|' read -r v n sha <<< "$row"
        DB_VERSION_NAME["$v"]="$n"
        DB_VERSION_CHECKSUM["$v"]="$sha"
        DB_APPLIED_VERSIONS+=("$v")
    done
}


# 数据库状态：tracked（有 schema_migrations）/ empty（无任何业务表）/
# untracked（有业务表但无 migration history）。
check_db_state() {
    if db_has_schema_migrations; then
        DB_STATE=tracked
        return 0
    fi

    local n
    n="$(db_user_table_count)"
    if [[ "$n" -gt 0 ]]; then
        DB_STATE=untracked
    else
        DB_STATE=empty
    fi
}


# 比对已应用历史与本地：发现异常输出并返回 1。
# 检查项：MISSING LOCAL / HISTORY MISMATCH（同名不同名）/ MODIFIED（checksum 不一致）。
verify_applied_history() {
    # EMPTY DB（或未初始化 schema_migrations）：无历史可校验
    if ! db_has_schema_migrations; then
        return 0
    fi

    local -a rows=()
    local row v n sha
    local -a problems=()

    mapfile -t rows < <(db_read_history)
    if [[ ${#rows[@]} -eq 0 ]]; then
        return 0
    fi

    for row in "${rows[@]}"; do
        IFS='|' read -r v n sha <<< "$row"

        if [[ -z "${LOCAL_VERSION_NAME[$v]+x}" ]]; then
            problems+=("MISSING LOCAL: version $v（name=$n）在本地不存在")
            continue
        fi

        if [[ "${LOCAL_VERSION_NAME[$v]}" != "$n" ]]; then
            problems+=("HISTORY MISMATCH: version $v 本地 name=${LOCAL_VERSION_NAME[$v]}，数据库 name=$n")
            continue
        fi

        if [[ "${LOCAL_VERSION_CHECKSUM[$v]}" != "$sha" ]]; then
            problems+=(
                "MODIFIED: version $v（$n）已被修改"
                "  数据库 checksum: $sha"
                "  本地 checksum: ${LOCAL_VERSION_CHECKSUM[$v]}"
            )
        fi
    done

    if [[ ${#problems[@]} -gt 0 ]]; then
        for p in "${problems[@]}"; do
            echo "  $p"
        done
        return 1
    fi

    return 0
}


# ── validate：纯本地校验，不连接数据库 ─────────────────────────────────────────

# 检测 migration 中的违规语句（剥离 SQL 注释后按实际语句匹配，避免注释中提及造成误报）：
#   ownership —— 操作 migrate.sh 独占管理的 schema_migrations 元数据表
#   nontrans  —— PostgreSQL 普通事务块内无法执行的语句（contract 要求所有 migration transaction-safe）
# 输出：<类别>:<行号>:<片段>
detect_banned_statements() {
    perl -0777 -e '
        my $content = <>;                 # 整个文件
        $content =~ s{--[^\n]*}{}g;      # 去掉 -- 行注释
        $content =~ s{/\*.*?\*/}{}gs;   # 去掉 /* */ 块注释

        # 语句起点：文件开头 / 行首(/m) / 前一条语句后的分号
        my $target = qr/\b(?:public\s*\.\s*)?schema_migrations\b/i;

        my @rules = (
            ["ownership", qr/(?:^|;)\s*CREATE\s+(?:TEMP(?:ORARY)?|UNLOGGED\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?$target/im],
            ["ownership", qr/(?:^|;)\s*DROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?$target/im],
            ["ownership", qr/(?:^|;)\s*ALTER\s+TABLE\s+(?:ONLY\s+)?(?:IF\s+EXISTS\s+)?$target/im],
            ["ownership", qr/(?:^|;)\s*TRUNCATE\s+(?:TABLE\s+)?$target/im],
            ["ownership", qr/(?:^|;)\s*INSERT\s+INTO\s+$target/im],
            ["ownership", qr/(?:^|;)\s*UPDATE\s+$target/im],
            ["ownership", qr/(?:^|;)\s*DELETE\s+FROM\s+$target/im],
            ["nontrans", qr/(?:^|;)\s*(?:CREATE|DROP)\s+(?:UNIQUE\s+)?INDEX\s+CONCURRENTLY\b/im],
            ["nontrans", qr/(?:^|;)\s*REINDEX\s+CONCURRENTLY\b/im],
            ["nontrans", qr/(?:^|;)\s*REFRESH\s+MATERIALIZED\s+VIEW\s+CONCURRENTLY\b/im],
            ["nontrans", qr/(?:^|;)\s*VACUUM\b/im],
            ["nontrans", qr/(?:^|;)\s*(?:CREATE|DROP)\s+DATABASE\b/im],
            ["nontrans", qr/(?:^|;)\s*(?:CREATE|DROP)\s+TABLESPACE\b/im],
            ["nontrans", qr/(?:^|;)\s*ALTER\s+SYSTEM\b/im],
            ["nontrans", qr/(?:^|;)\s*(?:CREATE|DROP)\s+SUBSCRIPTION\b/im],
        );

        for my $r (@rules) {
            my ($cat, $p) = @$r;
            while ($content =~ /$p/g) {
                my $line = 1 + (() = substr($content, 0, $-[0]) =~ /\n/g);
                my $frag = substr($content, $-[0], 70);
                $frag =~ s/\s+/ /g;
                print "$cat:$line:$frag\n";
            }
        }
    ' "$1"
}


run_validate() {
    local -i errors=0

    if [[ ! -d "$MIGRATIONS_DIR" ]]; then
        echo "错误：migrations 目录不存在：$MIGRATIONS_DIR" >&2
        exit 1
    fi

    local -a files=()
    mapfile -t files < <(find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' -printf '%f\n' | sort)

    if [[ ${#files[@]} -eq 0 ]]; then
        echo "错误：migrations 目录为空，没有任何 *.sql 文件" >&2
        exit 1
    fi

    local f v content tc
    local -A seen_version=()

    for f in "${files[@]}"; do
        if [[ ! "$f" =~ ^[0-9]{14}-[a-z0-9][a-z0-9-]*\.sql$ ]]; then
            echo "错误：文件名格式非法：$f" >&2
            echo "  期望：^[0-9]{14}-[a-z0-9][a-z0-9-]*\\.sql\$（例如 20260810101854-awd-gamebox-single-version.sql）" >&2
            errors=$((errors + 1))
            continue
        fi

        v="${f:0:14}"
        if [[ -n "${seen_version[$v]+x}" ]]; then
            echo "错误：version 重复：$v（$f 与 ${seen_version[$v]}）" >&2
            errors=$((errors + 1))
        else
            seen_version["$v"]="$f"
        fi

        # 内容不能为空：剔除注释行与空行后必须还有内容（合法占位迁移可带 warning，
        # 例如 20260806171917-scheduler-retry.sql 有意不包含 DDL）
        content="$(sed -E '/^[[:space:]]*--/d; /^[[:space:]]*$/d' "$MIGRATIONS_DIR/$f")"
        if [[ -z "$content" ]]; then
            echo "警告：migration 内容为空（仅注释/空白）：$f" >&2
            echo "  若为有意占位请保留注释说明；否则补充实际 SQL。" >&2
        fi

        # 禁止 standalone transaction control
        tc="$(grep -nE '^[[:space:]]*(BEGIN|START[[:space:]]+TRANSACTION|COMMIT|ROLLBACK)[[:space:]]*;' "$MIGRATIONS_DIR/$f" || true)"
        if [[ -n "$tc" ]]; then
            echo "错误：migration 不能包含 transaction control：$f" >&2
            echo "  Migration transactions are managed by migrate.sh." >&2
            echo "$tc" >&2
            errors=$((errors + 1))
        fi

        # 禁止管理 schema_migrations 元数据表 + 禁止非事务安全语句
        # （剥离注释后按实际 SQL 语句检测，注释中提及 schema_migrations 不构成违规）。
        local -a banned=()
        mapfile -t banned < <(detect_banned_statements "$MIGRATIONS_DIR/$f")

        local b bcat bline bfrag
        for b in "${banned[@]}"; do
            IFS=':' read -r bcat bline bfrag <<< "$b"
            if [[ "$bcat" == ownership ]]; then
                echo "错误：migration 不允许操作 schema_migrations（由 migrate.sh 独占管理）：$f" >&2
                echo "  schema_migrations is managed exclusively by migrate.sh" >&2
                echo "  $f:$bline: $bfrag" >&2
                errors=$((errors + 1))
            elif [[ "$bcat" == nontrans ]]; then
                echo "错误：migration 包含非事务安全语句（不支持）：$f" >&2
                echo "  Non-transactional migration statement is not supported:" >&2
                echo "  $f:$bline" >&2
                echo "  $bfrag" >&2
                errors=$((errors + 1))
            fi
        done
    done

    if [[ $errors -gt 0 ]]; then
        echo "校验失败：$errors 个问题" >&2
        exit 1
    fi

    echo "OK：${#files[@]} 个 migrations 校验通过（$MIGRATIONS_DIR）"
}


# ── new：创建 migration（保留 flock + 秒级 collision + 名称规范化）─────────────

new_migration() {
    local raw_name="${1:-}"

    if [[ -z "$raw_name" ]]; then
        if [[ ! -t 0 ]]; then
            echo "错误：未提供 migration 名称，且当前不是交互式终端" >&2
            echo "示例：./migrate.sh new add-user-avatar" >&2
            exit 2
        fi

        read -r -p "请输入 migration 名称: " raw_name

        if [[ -z "${raw_name//[[:space:]]/}" ]]; then
            echo "错误：migration 名称不能为空" >&2
            exit 2
        fi
    fi

    local name
    name="$(normalize_name "$raw_name")"

    if [[ -z "$name" ]]; then
        echo "错误：migration 名称无效" >&2
        exit 2
    fi

    mkdir -p "$MIGRATIONS_DIR"

    # 本地 flock：只保护 migration 文件创建（数据库并发由 advisory lock 负责）
    exec 9>"$LOCK_FILE"
    if command -v flock >/dev/null 2>&1; then
        flock 9
    fi

    local epoch timestamp migration_file
    epoch="$(date '+%s')"

    while true; do
        timestamp="$(date -d "@$epoch" '+%Y%m%d%H%M%S')"
        migration_file="$MIGRATIONS_DIR/${timestamp}-${name}.sql"

        if ! find "$MIGRATIONS_DIR" \
            -maxdepth 1 \
            -type f \
            -name "${timestamp}-*.sql" \
            -print -quit |
            grep -q .; then
            break
        fi

        epoch=$((epoch + 1))
    done

    # 新 contract：migration 文件只描述 schema/data changes，
    # 不包含 BEGIN;/COMMIT;（事务所有权归 migrate.sh）。
    cat > "$migration_file" <<EOF
-- ================================================================================
-- Migration: ${timestamp}-${name}
-- ================================================================================

-- 在这里编写迁移 SQL。
EOF

    echo
    echo "已创建 migration："
    echo "  $migration_file"
}


# ── list：纯本地，按执行顺序输出完整文件名 ────────────────────────────────────

run_list() {
    list_migrations
}


# ── status：read-only，对比本地与数据库 ───────────────────────────────────────

run_status() {
    run_validate
    load_db_url

    echo "FloatCTF Database Migrations"
    echo "Database: $(mask_db_url "$DB_URL")"
    echo

    check_db_state

    if [[ "$DB_STATE" == untracked ]]; then
        echo "Database contains existing schema but has no migration history."
        echo "Migration state: UNTRACKED"
        echo
        echo "提示：该库需要 baseline/stamp 后才能由 migrate.sh 接管；本次不自动修复。" >&2
        exit 1
    fi

    build_local_history
    build_db_history

    local -i maxw=4
    local v n
    for v in "${LOCAL_ORDER[@]}"; do
        n="${LOCAL_VERSION_NAME[$v]}"
        if [[ ${#n} -gt $maxw ]]; then
            maxw=${#n}
        fi
    done

    printf '%-14s  %-*s  %s\n' "VERSION" "$maxw" "NAME" "STATUS"
    printf '%s\n' '----------------------------------------------------------------'

    local -i has_problems=0
    local -i applied=0
    local -i pending=0
    local status

    for v in "${LOCAL_ORDER[@]}"; do
        n="${LOCAL_VERSION_NAME[$v]}"

        if [[ -n "${DB_VERSION_NAME[$v]+x}" ]]; then
            if [[ "${DB_VERSION_NAME[$v]}" != "$n" ]]; then
                status="HISTORY MISMATCH"
                has_problems=1
            elif [[ "${DB_VERSION_CHECKSUM[$v]}" != "${LOCAL_VERSION_CHECKSUM[$v]}" ]]; then
                status="MODIFIED"
                has_problems=1
            else
                status="APPLIED"
                applied=$((applied + 1))
            fi
        else
            status="PENDING"
            pending=$((pending + 1))
        fi

        printf '%-14s  %-*s  %s\n' "$v" "$maxw" "$n" "$status"
    done

    for v in "${DB_APPLIED_VERSIONS[@]}"; do
        if [[ -z "${LOCAL_VERSION_NAME[$v]+x}" ]]; then
            printf '%-14s  %-*s  %s\n' "$v" "$maxw" "${DB_VERSION_NAME[$v]}" "MISSING LOCAL"
            has_problems=1
        fi
    done

    echo
    echo "Applied: $applied"
    echo "Pending: $pending"

    if [[ $has_problems -eq 1 ]]; then
        echo "存在异常状态（MISSING LOCAL / MODIFIED / HISTORY MISMATCH）" >&2
        exit 1
    fi

    exit 0
}


# ── verify：只验证已应用历史 ───────────────────────────────────────────────────

run_verify() {
    run_validate
    load_db_url

    echo "FloatCTF Database Migrations"
    echo "Database: $(mask_db_url "$DB_URL")"
    echo

    check_db_state

    if [[ "$DB_STATE" == untracked ]]; then
        echo "错误：数据库已有 schema 但没有任何 migration history（UNTRACKED）" >&2
        echo "verify 拒绝继续；不自动修复。" >&2
        exit 1
    fi

    build_local_history

    echo "Verifying applied migration history..."
    if ! verify_applied_history; then
        echo >&2
        echo "FAIL" >&2
        exit 1
    fi

    echo "OK"
}


# ── apply ─────────────────────────────────────────────────────────────────────

# 生成 apply session SQL：
#   1. pg_advisory_lock（同一连接全程持有，连接关闭自动释放）
#   2. 确保 schema_migrations 存在
#   3. 每个 migration：锁后基于数据库当前状态判断 pending（NOT EXISTS），
#      独立事务 = migration SQL + schema_migrations INSERT 原子提交
#   4. pg_advisory_unlock
generate_apply_script() {
    local out="$1"
    local v n sha

    {
        printf '%s\n' '-- FloatCTF migration apply session（由 migrate.sh 生成）'
        printf '%s\n' "-- advisory lock: pg_advisory_lock($ADVISORY_LOCK_ID)  (= 0x464C4154 'FLAT')"
        printf '%s\n' "SELECT pg_advisory_lock($ADVISORY_LOCK_ID);"
        printf '%s\n' "$SCHEMA_MIGRATIONS_DDL"

        for v in "${LOCAL_ORDER[@]}"; do
            n="${LOCAL_VERSION_NAME[$v]}"
            sha="${LOCAL_VERSION_CHECKSUM[$v]}"

            printf '%s\n' "SELECT NOT EXISTS (SELECT 1 FROM public.schema_migrations WHERE version = $v) AS pending_v;"
            printf '%s\n' '\gset'
            printf '%s\n' '\if :pending_v'
            printf '%s\n' 'BEGIN;'
            printf '%s\n' "\\ir '$MIGRATIONS_DIR/${LOCAL_VERSION_FILE[$v]}'"
            printf 'INSERT INTO public.schema_migrations (version, name, checksum) VALUES (%s, %s, %s);\n' \
                "$v" "$(sql_literal "$n")" "$(sql_literal "$sha")"
            printf '%s\n' 'COMMIT;'
            printf '%s\n' "\\echo APPLIED_MIGRATION:${LOCAL_VERSION_FILE[$v]}"
            printf '%s\n' '\endif'
        done

        printf '%s\n' "SELECT pg_advisory_unlock($ADVISORY_LOCK_ID);"
    } > "$out"
}


# 找出第一个未出现在数据库历史的本地 migration（用于失败定位）。
find_failing_migration() {
    local v
    build_db_history
    for v in "${LOCAL_ORDER[@]}"; do
        if [[ -z "${DB_VERSION_NAME[$v]+x}" ]]; then
            echo "${LOCAL_VERSION_FILE[$v]}"
            return 0
        fi
    done
    echo "<unknown migration>"
}


run_apply() {
    run_validate
    load_db_url

    echo "FloatCTF Database Migrations"
    echo "Database: $(mask_db_url "$DB_URL")"
    echo

    build_local_history
    check_db_state

    case "$DB_STATE" in
        untracked)
            echo "错误：数据库已有 schema 但没有任何 migration history（UNTRACKED）" >&2
            echo "apply 拒绝在该数据库上从 migration #1 开始执行" >&2
            echo "（否则会 CREATE TABLE already exists / ALTER twice，破坏已有 schema）。" >&2
            echo "baseline/stamp 以后再处理，本次不实现。" >&2
            exit 1
            ;;
        empty)
            echo "Database state: EMPTY（无业务表；schema_migrations 将自动创建）"
            ;;
        tracked)
            :
            ;;
    esac

    echo "Verifying migration history..."
    if ! verify_applied_history; then
        echo "已应用历史存在不一致（见上），apply 拒绝继续。" >&2
        exit 1
    fi
    echo "OK"

    # 具体应用了哪些以 psql 的 \echo APPLIED_MIGRATION: 标记为准——该标记只在锁内
    # 实际执行后才输出，并发败者 runner 不会误报。

    local apply_script
    apply_script="$(mktemp "$SCRIPT_DIR/.apply.XXXXXX.sql")"
    trap 'rm -f -- "$apply_script"' EXIT INT TERM

    generate_apply_script "$apply_script"

    local out
    echo
    echo "Applying migrations:"

    if ! out="$(psql -X -v ON_ERROR_STOP=1 -d "$DB_URL" -f "$apply_script" 2>&1)"; then
        echo
        echo "Migration failed:" >&2
        echo "  $(find_failing_migration)" >&2
        echo >&2
        echo "$out" >&2
        exit 1
    fi

    # 本次实际应用的 migrations（来自锁内执行的 \echo 标记，顺序即执行顺序）
    local -a applied_files=()
    local line
    while IFS= read -r line; do
        case "$line" in
            APPLIED_MIGRATION:*)
                applied_files+=("${line#APPLIED_MIGRATION:}")
                ;;
        esac
    done <<< "$out"

    local -i applied_this="${#applied_files[@]}"
    local -i total="${#LOCAL_ORDER[@]}"
    local -i after_count
    after_count="$(db_read_history | wc -l | tr -d ' ')"

    local f
    for f in "${applied_files[@]}"; do
        printf '  → %s\n    applied\n' "$f"
    done

    if [[ $applied_this -eq 0 ]]; then
        echo "  (没有待应用的 migrations)"
    fi

    echo
    echo "Done."
    echo
    echo "Applied: $applied_this"
    echo "Skipped: $((after_count - applied_this))"
    echo "Pending: $((total - after_count))"

    rm -f -- "$apply_script"
    trap - EXIT INT TERM
}


# ── make：确定性 merged.sql ───────────────────────────────────────────────────

run_make() {
    run_validate
    build_local_history

    local v n sha
    local -i count="${#LOCAL_ORDER[@]}"

    local temp_file
    temp_file="$(mktemp "$SCRIPT_DIR/.merged.sql.XXXXXX")"
    trap 'rm -f -- "$temp_file"' EXIT INT TERM

    {
        printf '%s\n' '-- ================================================================================'
        printf '%s\n' '-- ================================================================================'
        printf '%s\n' '--                              FloatCTF Merged Migrations'
        printf '%s\n' '--'
        printf '%s\n' '-- AUTO-GENERATED FILE. DO NOT EDIT DIRECTLY.'
        printf '%s\n' '--'
        printf '%s\n' '-- Source: apps/api/src/sql/migrations'
        printf '%s\n' '-- Generate with: mise run db:migration:merge'
        printf '%s\n' '--'
        printf '%s\n' "-- Migration count: $count"
        printf '%s\n' '--'
        printf '%s\n' '-- 每个 migration 与其 schema_migrations 记录在同一事务提交。'
        printf '%s\n' '-- 内容由 migrations/ 输入确定性决定（无时间戳/hostname/路径）。'
        printf '%s\n' '-- ================================================================================'
        printf '%s\n' '-- ================================================================================'
        printf '\n'
        printf '%s\n' "$SCHEMA_MIGRATIONS_DDL"
        printf '\n'

        for v in "${LOCAL_ORDER[@]}"; do
            n="${LOCAL_VERSION_NAME[$v]}"
            sha="${LOCAL_VERSION_CHECKSUM[$v]}"

            printf '%s\n' '-- ================================================================================'
            printf '%s\n' '-- ================================================================================'
            printf '%s\n' "-- BEGIN MIGRATION: ${LOCAL_VERSION_FILE[$v]}"
            printf '%s\n' "-- SHA256: $sha"
            printf '%s\n' '-- ================================================================================'
            printf '%s\n' '-- ================================================================================'
            printf '\n'
            printf '%s\n' 'BEGIN;'
            printf '\n'
            cat "$MIGRATIONS_DIR/${LOCAL_VERSION_FILE[$v]}"
            printf '\n'
            printf 'INSERT INTO public.schema_migrations (version, name, checksum) VALUES (%s, %s, %s);\n' \
                "$v" "$(sql_literal "$n")" "$(sql_literal "$sha")"
            printf '\n'
            printf '%s\n' 'COMMIT;'
            printf '\n'
            printf '%s\n' '-- ================================================================================'
            printf '%s\n' '-- ================================================================================'
            printf '%s\n' "-- END MIGRATION: ${LOCAL_VERSION_FILE[$v]}"
            printf '%s\n' '-- ================================================================================'
            printf '%s\n' '-- ================================================================================'
            printf '\n'
        done

        printf '%s\n' '-- ================================================================================'
        printf '%s\n' '-- ================================================================================'
        printf '%s\n' '--                              End of FloatCTF Migrations'
        printf '%s\n' '-- ================================================================================'
        printf '%s\n' '-- ================================================================================'
    } > "$temp_file"

    # docker-entrypoint 以 postgres(uid 999) 读 init 脚本，不依赖 umask。
    chmod 644 "$temp_file"

    mv -- "$temp_file" "$MERGED_FILE"
    trap - EXIT INT TERM

    echo
    echo "已生成 deterministic merged.sql（$count 个 migrations）："
    echo "  $MERGED_FILE"
}


main() {
    local command="${1:-help}"

    case "$command" in
        new)
            shift
            # "$*" 允许 new add-user-avatar 以及 new add user avatar，
            # 最终都交给 normalize_name 处理。
            new_migration "$*"
            ;;
        list)
            [[ $# -eq 1 ]] || die_usage "list 不接受额外参数"
            run_list
            ;;
        validate)
            [[ $# -eq 1 ]] || die_usage "validate 不接受额外参数"
            run_validate
            ;;
        status)
            [[ $# -eq 1 ]] || die_usage "status 不接受额外参数"
            run_status
            ;;
        verify)
            [[ $# -eq 1 ]] || die_usage "verify 不接受额外参数"
            run_verify
            ;;
        apply)
            [[ $# -eq 1 ]] || die_usage "apply 不接受额外参数"
            run_apply
            ;;
        make)
            [[ $# -eq 1 ]] || die_usage "make 不接受额外参数"
            run_make
            ;;
        help | -h | --help)
            usage
            ;;
        *)
            die_usage "未知命令：$command"
            ;;
    esac
}


main "$@"
