#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIGRATIONS_DIR="$SCRIPT_DIR/migrations"
MERGED_FILE="$SCRIPT_DIR/merged.sql"
LOCK_FILE="$SCRIPT_DIR/.migrate.lock"


usage() {
    cat <<'EOF'
用法：

  ./migrate.sh new [name]
      新建迁移文件。

      直接指定名称：
        ./migrate.sh new add-user-avatar
        ./migrate.sh new "add user avatar"

      不指定名称时会交互式询问：
        ./migrate.sh new

  ./migrate.sh make
      按时间戳顺序合并 migrations/ 中的迁移，
      输出为当前目录的 merged.sql。

  ./migrate.sh list
      按执行顺序列出全部迁移。

  ./migrate.sh help
      显示帮助。
EOF
}


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
        -regextype posix-extended \
        -regex '.*/[0-9]{14}-[^/]+\.sql' \
        -printf '%f\n' |
        sort
}


new_migration() {
    local raw_name="${1:-}"

    # --------------------------------------------------------------------------
    # 未传递名称时，交互式获取
    # --------------------------------------------------------------------------

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

    # --------------------------------------------------------------------------
    # 规范化名称
    # --------------------------------------------------------------------------

    local name
    name="$(normalize_name "$raw_name")"

    if [[ -z "$name" ]]; then
        echo "错误：migration 名称无效" >&2
        exit 2
    fi

    mkdir -p "$MIGRATIONS_DIR"

    # --------------------------------------------------------------------------
    # 防止并发创建 migration 时产生相同时间戳
    # --------------------------------------------------------------------------

    exec 9>"$LOCK_FILE"

    if command -v flock >/dev/null 2>&1; then
        flock 9
    fi

    local epoch
    local timestamp
    local migration_file

    epoch="$(date '+%s')"

    # --------------------------------------------------------------------------
    # 如果当前秒已经存在 migration，则自动向后顺延一秒
    # --------------------------------------------------------------------------

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

    # --------------------------------------------------------------------------
    # 创建 migration
    # --------------------------------------------------------------------------

    cat > "$migration_file" <<EOF
-- ================================================================================
-- Migration: ${timestamp}-${name}
-- Created at: $(date '+%Y-%m-%d %H:%M:%S %z')
-- ================================================================================

BEGIN;

-- 在这里编写迁移 SQL。

COMMIT;
EOF

    echo
    echo "已创建 migration："
    echo "  $migration_file"
}


make_merged() {
    local -a filenames=()
    local -a files=()

    mapfile -t filenames < <(list_migrations)

    if [[ ${#filenames[@]} -eq 0 ]]; then
        echo "错误：没有找到 migration 文件" >&2
        echo "期望格式：YYYYMMDDHHMMSS-name.sql" >&2
        exit 1
    fi

    local filename

    for filename in "${filenames[@]}"; do
        files+=("$MIGRATIONS_DIR/$filename")
    done

    local temp_file
    temp_file="$(mktemp "$SCRIPT_DIR/.merged.sql.XXXXXX")"

    cleanup() {
        rm -f "$temp_file"
    }

    trap cleanup EXIT INT TERM

    {
        printf '%s\n' '-- ================================================================================================'
        printf '%s\n' '-- ================================================================================================'
        printf '%s\n' '--                                   FloatCTF Merged Migrations'
        printf '%s\n' "-- Generated at: $(date '+%Y-%m-%d %H:%M:%S %z')"
        printf '%s\n' "-- Migration count: ${#files[@]}"
        printf '%s\n' '-- ================================================================================================'
        printf '%s\n' '-- ================================================================================================'

        local file

        for file in "${files[@]}"; do
            filename="$(basename "$file")"

            printf '\n'
            printf '%s\n' '-- ================================================================================================'
            printf '%s\n' '-- ================================================================================================'
            printf '%s\n' "-- BEGIN MIGRATION: $filename"
            printf '%s\n' '-- ================================================================================================'
            printf '%s\n' '-- ================================================================================================'
            printf '\n'

            cat "$file"

            # 防止 SQL 文件末尾没有换行，与结束标记粘连。
            printf '\n'
            printf '\n'

            printf '%s\n' '-- ================================================================================================'
            printf '%s\n' '-- ================================================================================================'
            printf '%s\n' "-- END MIGRATION: $filename"
            printf '%s\n' '-- ================================================================================================'
            printf '%s\n' '-- ================================================================================================'
            printf '\n'
        done

        printf '\n'
        printf '%s\n' '-- ================================================================================================'
        printf '%s\n' '-- ================================================================================================'
        printf '%s\n' '--                                  End of FloatCTF Migrations'
        printf '%s\n' '-- ================================================================================================'
        printf '%s\n' '-- ================================================================================================'
    } > "$temp_file"

    # mktemp 默认 0600；docker-entrypoint 以 postgres(uid 999) 读 init 脚本，
    # 必须保证 merged.sql 对容器可读，否则全新 volume 初始化会失败（库被建空）。
    chmod 644 "$temp_file"

    mv -- "$temp_file" "$MERGED_FILE"
    trap - EXIT INT TERM

    echo
    echo "已按时间顺序合并 ${#files[@]} 个 migrations："
    echo "  $MERGED_FILE"
}


main() {
    local command="${1:-help}"

    case "$command" in
        new)
            shift

            # "$*" 允许：
            #
            # ./migrate.sh new add-user-avatar
            #
            # 以及：
            #
            # ./migrate.sh new add user avatar
            #
            # 最终都会交给 normalize_name 处理。
            new_migration "$*"
            ;;

        make)
            if [[ $# -ne 1 ]]; then
                echo "错误：make 不接受额外参数" >&2
                echo
                usage
                exit 2
            fi

            make_merged
            ;;

        list)
            if [[ $# -ne 1 ]]; then
                echo "错误：list 不接受额外参数" >&2
                echo
                usage
                exit 2
            fi

            list_migrations
            ;;

        help | -h | --help)
            usage
            ;;

        *)
            echo "错误：未知命令：$command" >&2
            echo
            usage
            exit 2
            ;;
    esac
}


main "$@"
