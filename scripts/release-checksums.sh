#!/usr/bin/env bash
#
# FloatCTF 发布制品工具（v1.0）—— release.yml / rc.yml 共用的**唯一实现**。
#
# 为什么是脚本而不是把逻辑摊进 workflow：release.yml（tag 发布）与 rc.yml（RC 非发布）
# 必须产出**同一套**制品与**同一格式**的 SHA256SUMS；两份 YAML 各写一遍必然漂移。
# 这里集中实现，workflow 只负责触发、权限、编译与上传。
#
# 模式：
#   --ops-tools <outfile>               构建确定性运维工具归档（成员白名单断言）
#   --checksums <dir|files...>          写出 SHA256SUMS（不含自身，按名字排序）
#   --pack-packages <outdir> <version>  pnpm pack 三个平台包 + 确定性改名
#   --assemble <outdir> <version>       装配 ARTIFACT CONTRACT v1.0 全量制品 + SHA256SUMS
#   -h | --help
#
# 通用选项：
#   -o | --out <file>   --checksums 的输出路径
#                       （默认：单个目录输入 → <dir>/SHA256SUMS，否则 → ./SHA256SUMS）
#
# ── ARTIFACT CONTRACT v1.0 ───────────────────────────────────────────────────
#   floatctf                           target/release/floatctf
#   floatctf-helper                    target/release/floatctf-helper
#   web-dist.tar.gz                    scripts/package-web-dist.sh
#   merged.sql                         apps/api/src/sql/migrate.sh make
#   frontend.sh                        scripts/frontend.sh
#   install.sh                         scripts/install.sh
#   ops-tools.tar.gz                   --ops-tools
#   floatctf-sdk-<V>.tgz               pnpm --filter @floatctf/sdk pack
#   floatctf-react-<V>.tgz             pnpm --filter @floatctf/react pack
#   floatctf-frontend-runtime-<V>.tgz  pnpm --filter @floatctf/frontend-runtime pack
#   SHA256SUMS                         其余每个制品的 sha256sum（**不含自身**）
#
# <V> = 平台版本 = release tag 去掉前导 v（tag v1.0.0 → 1.0.0）。版本号必须由调用方
# 显式确定；本脚本**绝不**回落到 ci / latest 之类的占位名（fail closed）。
#
# ── 确定性 / 安全约定 ────────────────────────────────────────────────────────
#   * 归档确定性：--sort=name --numeric-owner --owner=0 --group=0 --mtime=@0 + gzip -n
#   * 归档成员**白名单**断言：多一个或少一个都失败
#   * 归档内拒绝绝对路径 / .. / 符号链接 / 硬链接
#   * SHA256SUMS 以 basename 记录（必须唯一）、LC_ALL=C 按名字排序、写完立即自校验
#
# 用法示例：
#   scripts/release-checksums.sh --assemble release-artifacts 1.0.0
#   scripts/release-checksums.sh --ops-tools /tmp/ops-tools.tar.gz
#   scripts/release-checksums.sh --checksums release-artifacts
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 平台包（workspace 名 → pnpm pack 输出前缀）
PLATFORM_PACKAGES=(
    "sdk:@floatctf/sdk"
    "react:@floatctf/react"
    "frontend-runtime:@floatctf/frontend-runtime"
)

# ops-tools.tar.gz 的成员契约：顶层 backup.sh / restore.sh / db/migrate.sh + 全部迁移。
OPS_TOOLS_TOP_MEMBERS=("backup.sh" "restore.sh" "db/migrate.sh")

die() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
info() { printf '[INFO] %s\n' "$*"; }
ok() { printf '[ OK ] %s\n' "$*"; }
warn() { printf '[WARN] %s\n' "$*" >&2; }

_TMP_DIRS=()
cleanup_tmp() {
    local d
    for d in "${_TMP_DIRS[@]}"; do
        [ -n "$d" ] && rm -rf -- "$d"
    done
    return 0
}
trap cleanup_tmp EXIT

new_tmp() { # <tag> -> dir
    local d
    d="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-$1.XXXXXX")"
    _TMP_DIRS+=("$d")
    printf '%s' "$d"
}

usage() {
    cat <<'EOF'
FloatCTF 发布制品工具（ARTIFACT CONTRACT v1.0）

用法：
  scripts/release-checksums.sh --ops-tools <outfile>
      构建确定性 ops-tools.tar.gz。成员（全部位于顶层）：
        backup.sh                 ← scripts/backup.sh
        restore.sh                ← scripts/restore.sh
        db/migrate.sh             ← apps/api/src/sql/migrate.sh
        db/migrations/<all>.sql   ← apps/api/src/sql/migrations/*.sql
      构建后断言成员白名单：缺少任一预期成员、或出现意外成员即失败。

  scripts/release-checksums.sh --checksums <dir|files...> [-o <outfile>]
      写出 SHA256SUMS，格式 "<hash>  <name>"，LC_ALL=C 按名字排序。
      * 输入为单个目录时：校验该目录下所有普通文件，输出 <dir>/SHA256SUMS。
      * 输入为多个文件时：输出 ./SHA256SUMS（或 -o 指定）。
      * 绝不含 SHA256SUMS 自身；输入为空（或仅剩 SHA256SUMS）即失败。

  scripts/release-checksums.sh --pack-packages <outdir> <version>
      pnpm pack @floatctf/{sdk,react,frontend-runtime}，按 <version> 确定性改名为
      floatctf-sdk-<version>.tgz / floatctf-react-<version>.tgz /
      floatctf-frontend-runtime-<version>.tgz（<version> 非法即失败）。

  scripts/release-checksums.sh --assemble <outdir> <version>
      装配 ARTIFACT CONTRACT v1.0 的全部 10 个文件 + SHA256SUMS。
      需要已构建的 target/release/{floatctf,floatctf-helper}。

选项：
  -o, --out <file>   --checksums 的输出路径
  -h, --help         显示本帮助

版本号（<version>，即 <V>）规则：x.y.z[-pre][+build]；不得为空——绝不回落 ci/latest。
EOF
}

validate_version() {
    local v="${1:-}"
    if [ -z "$v" ]; then
        die "VERSION 为空：拒绝以 ci/latest 之类的占位名命名制品（fail closed）"
    fi
    if [[ ! "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
        die "VERSION '$v' 不是合法平台版本（期望 x.y.z / x.y.z-pre / x.y.z+build）"
    fi
}

# ── --ops-tools ──────────────────────────────────────────────────────────────
ops_tools_build() {
    local out="$1"
    [ -n "$out" ] || die "--ops-tools 需要 <outfile>"

    local backup="$ROOT/scripts/backup.sh"
    local restore="$ROOT/scripts/restore.sh"
    local migrate="$ROOT/apps/api/src/sql/migrate.sh"
    local migrations_dir="$ROOT/apps/api/src/sql/migrations"

    [ -f "$backup" ] || die "缺少 ops-tools 必需成员：$backup"
    [ -f "$restore" ] || die "缺少 ops-tools 必需成员：$restore"
    [ -f "$migrate" ] || die "缺少 ops-tools 必需成员：$migrate"
    [ -d "$migrations_dir" ] || die "缺少迁移目录：$migrations_dir"

    local -a migration_srcs=()
    local f
    while IFS= read -r f; do migration_srcs+=("$f"); done \
        < <(find "$migrations_dir" -maxdepth 1 -type f -name '*.sql' | LC_ALL=C sort)
    [ "${#migration_srcs[@]}" -gt 0 ] || die "$migrations_dir 下没有任何 .sql 迁移（归档会是空的）"

    local stage
    stage="$(new_tmp ops-tools)"
    mkdir -p "$stage/db/migrations"
    chmod 0755 "$stage" "$stage/db" "$stage/db/migrations"
    install -m 0755 -- "$backup" "$stage/backup.sh"
    install -m 0755 -- "$restore" "$stage/restore.sh"
    install -m 0755 -- "$migrate" "$stage/db/migrate.sh"
    for f in "${migration_srcs[@]}"; do
        install -m 0644 -- "$f" "$stage/db/migrations/$(basename "$f")"
    done

    # 期望成员：来自**仓库源**（而非暂存目录），这样断言才有意义。
    local -a expected=("${OPS_TOOLS_TOP_MEMBERS[@]}")
    for f in "${migration_srcs[@]}"; do
        expected+=("db/migrations/$(basename "$f")")
    done

    mkdir -p "$(dirname "$out")"
    # 确定性归档：固定成员顺序、属主、时间戳；gzip -n 去掉压缩头中的名字与时间。
    tar --create --file - \
        --sort=name --numeric-owner --owner=0 --group=0 --mtime='@0' \
        --directory "$stage" \
        backup.sh restore.sh db \
        | gzip -n >"$out"

    # ── 布局/安全断言（成员白名单 + 无危险成员）──
    local listing verbose
    listing="$(tar -tzf "$out")"
    verbose="$(tar -tvzf "$out")"

    if grep -qE '^/|(^|/)\.\.(/|$)' <<<"$listing"; then
        die "ops-tools 归档含绝对路径或 .. 成员（拒绝）"
    fi
    if grep -qE '^[lh]' <<<"$verbose"; then
        die "ops-tools 归档含符号链接/硬链接成员（拒绝）"
    fi

    local actual_files expected_files
    actual_files="$(grep -v '/$' <<<"$listing" | LC_ALL=C sort)"
    expected_files="$(printf '%s\n' "${expected[@]}" | LC_ALL=C sort)"
    if [ "$actual_files" != "$expected_files" ]; then
        printf '[FAIL] ops-tools 成员与契约不符\n--- 期望 ---\n%s\n--- 实际 ---\n%s\n' \
            "$expected_files" "$actual_files" >&2
        die "ops-tools 归档布局断言失败：$out"
    fi

    local d
    while IFS= read -r d; do
        case "$d" in
            "" | "db/" | "db/migrations/") ;;
            *) die "ops-tools 归档含意外目录成员：$d" ;;
        esac
    done < <(grep '/$' <<<"$listing" || true)

    ok "ops-tools 归档已生成：$out（${#expected[@]} 个成员）"
    printf '%s\n' "${expected[@]}" | sed 's/^/  · /'
}

# ── --checksums ──────────────────────────────────────────────────────────────
generate_checksums() { # <outfile> <files...>
    local out="$1"
    shift
    [ -n "$out" ] || die "--checksums 需要输出路径"
    command -v sha256sum >/dev/null 2>&1 || die "缺少 sha256sum（coreutils）"

    local -a names=() paths=()
    local f b i
    for f in "$@"; do
        b="$(basename -- "$f")"
        if [ "$b" = "SHA256SUMS" ]; then
            info "跳过 SHA256SUMS 自身：$f"
            continue
        fi
        [ -e "$f" ] || die "checksum 输入不存在：$f"
        [ -f "$f" ] || die "checksum 输入不是普通文件：$f"
        for i in "${names[@]}"; do
            if [ "$i" = "$b" ]; then
                die "checksum 文件名冲突（按 basename 记录，必须唯一）：$b"
            fi
        done
        names+=("$b")
        paths+=("$f")
    done
    [ "${#names[@]}" -gt 0 ] || die "checksum 输入为空：至少需要一个非 SHA256SUMS 文件"

    mkdir -p "$(dirname -- "$out")"
    local out_dir
    out_dir="$(cd "$(dirname -- "$out")" && pwd)"
    local tmp="$out.tmp.$$"
    : >"$tmp"
    local h
    for i in "${!names[@]}"; do
        h="$(sha256sum -- "${paths[$i]}" | cut -d' ' -f1)"
        printf '%s  %s\n' "$h" "${names[$i]}" >>"$tmp"
    done
    # 确定性顺序：LC_ALL=C 按「名字」字段排序（字段 2）。
    LC_ALL=C sort -k2,2 -o "$tmp" "$tmp"
    mv -f -- "$tmp" "$out"

    # 自校验：清单按 basename 记录（发布目录天然是扁平的），所以只有全部输入都与
    # SHA256SUMS 同处一个目录时，才能用 `sha256sum -c` 立刻证明逐字节一致。
    local in_dir="" skip_reason="" d
    for i in "${!paths[@]}"; do
        d="$(cd "$(dirname -- "${paths[$i]}")" && pwd)"
        if [ -z "$in_dir" ]; then
            in_dir="$d"
        elif [ "$d" != "$in_dir" ]; then
            in_dir="__mixed__"
        fi
    done
    if [ "$in_dir" = "__mixed__" ]; then
        skip_reason="输入文件来自不同目录"
    elif [ "$in_dir" != "$out_dir" ]; then
        skip_reason="SHA256SUMS 与输入文件不在同一目录"
    fi
    if [ -n "$skip_reason" ]; then
        warn "跳过 SHA256SUMS 自校验（$skip_reason）；清单按 basename 记录，适用于扁平发布目录"
    else
        ( cd "$out_dir" && sha256sum --check --strict --quiet "$(basename -- "$out")" ) \
            || die "SHA256SUMS 自校验失败：$out"
    fi

    ok "SHA256SUMS 已生成：$out（${#names[@]} 个文件，不含自身）"
    cat "$out"
}

# ── --pack-packages ──────────────────────────────────────────────────────────
pack_packages() { # <outdir> <version>
    local out="$1" version="$2"
    [ -n "$out" ] || die "--pack-packages 需要 <outdir>"
    validate_version "$version"
    command -v pnpm >/dev/null 2>&1 || die "缺少 pnpm（无法执行 pnpm pack）"

    mkdir -p "$out"
    local stage
    stage="$(new_tmp pack)"

    local spec short filter produced src_name
    local -a matches=()
    local f
    for spec in "${PLATFORM_PACKAGES[@]}"; do
        short="${spec%%:*}"
        filter="${spec#*:}"
        matches=()
        while IFS= read -r f; do matches+=("$f"); done \
            < <(find "$stage" -maxdepth 1 -type f -name "floatctf-$short-*.tgz" | LC_ALL=C sort)
        [ "${#matches[@]}" -eq 0 ] || die "暂存目录残留 $short 的 tgz（上一次 pack 未清理）"

        ( cd "$ROOT" && pnpm --filter "$filter" pack --pack-destination "$stage" >/dev/null )

        matches=()
        while IFS= read -r f; do matches+=("$f"); done \
            < <(find "$stage" -maxdepth 1 -type f -name "floatctf-$short-*.tgz" | LC_ALL=C sort)
        [ "${#matches[@]}" -eq 1 ] \
            || die "pnpm pack $filter 期望恰好产出 1 个 tgz，实际 ${#matches[@]} 个"

        produced="${matches[0]}"
        src_name="$(basename -- "$produced")"
        if [ "$src_name" != "floatctf-$short-$version.tgz" ]; then
            # pnpm 用 packages/*/package.json 的 version 命名；契约要求用平台版本 <V>。
            warn "pnpm pack 产出 $src_name（包内版本），按契约改名为 floatctf-$short-$version.tgz"
        fi
        install -m 0644 -- "$produced" "$out/floatctf-$short-$version.tgz"
        rm -f -- "$produced"
        ok "已打包 $out/floatctf-$short-$version.tgz"
    done
}

# ── --assemble ───────────────────────────────────────────────────────────────
assemble() { # <outdir> <version>
    local out="$1" version="$2"
    [ -n "$out" ] || die "--assemble 需要 <outdir>"
    validate_version "$version"
    mkdir -p "$out"

    local api_bin="$ROOT/target/release/floatctf"
    local helper_bin="$ROOT/target/release/floatctf-helper"
    [ -f "$api_bin" ] || die "缺少 $api_bin（先 cargo build --locked --release -p floatctf --bins）"
    [ -f "$helper_bin" ] || die "缺少 $helper_bin（先 cargo build --locked --release -p floatctf-helper --bins）"

    install -m 0755 -- "$api_bin" "$out/floatctf"
    install -m 0755 -- "$helper_bin" "$out/floatctf-helper"

    bash "$ROOT/scripts/package-web-dist.sh" "$out/web-dist.tar.gz"

    bash "$ROOT/apps/api/src/sql/migrate.sh" make
    [ -f "$ROOT/apps/api/src/sql/merged.sql" ] || die "migrate.sh make 未产出 merged.sql"
    install -m 0644 -- "$ROOT/apps/api/src/sql/merged.sql" "$out/merged.sql"

    install -m 0755 -- "$ROOT/scripts/frontend.sh" "$out/frontend.sh"
    install -m 0755 -- "$ROOT/scripts/install.sh" "$out/install.sh"

    pack_packages "$out" "$version"
    ops_tools_build "$out/ops-tools.tar.gz"

    local -a artifacts=(
        "$out/floatctf"
        "$out/floatctf-helper"
        "$out/web-dist.tar.gz"
        "$out/merged.sql"
        "$out/frontend.sh"
        "$out/install.sh"
        "$out/ops-tools.tar.gz"
        "$out/floatctf-sdk-$version.tgz"
        "$out/floatctf-react-$version.tgz"
        "$out/floatctf-frontend-runtime-$version.tgz"
    )
    local f
    for f in "${artifacts[@]}"; do
        [ -f "$f" ] || die "装配后缺少契约制品：$f"
    done

    # SHA256SUMS 覆盖其余每个制品（不含自身）。
    generate_checksums "$out/SHA256SUMS" "${artifacts[@]}"

    ok "已装配 $(( ${#artifacts[@]} + 1 )) 个制品（含 SHA256SUMS）到 $out"
    ls -la "$out"
}

main() {
    local mode="" ops_out="" check_out="" pack_out="" pack_version="" asm_out="" asm_version=""
    local -a inputs=()

    while [ $# -gt 0 ]; do
        case "$1" in
            --ops-tools)
                [ $# -ge 2 ] || die "--ops-tools 需要 <outfile>"
                mode="ops-tools"; ops_out="$2"; shift 2 ;;
            --checksums)
                mode="checksums"; shift
                while [ $# -gt 0 ] && [[ "$1" != -* ]]; do inputs+=("$1"); shift; done ;;
            --pack-packages)
                [ $# -ge 3 ] || die "--pack-packages 需要 <outdir> <version>"
                mode="pack-packages"; pack_out="$2"; pack_version="$3"; shift 3 ;;
            --assemble)
                [ $# -ge 3 ] || die "--assemble 需要 <outdir> <version>"
                mode="assemble"; asm_out="$2"; asm_version="$3"; shift 3 ;;
            -o | --out)
                [ $# -ge 2 ] || die "$1 需要 <file>"
                check_out="$2"; shift 2 ;;
            -h | --help)
                usage; exit 0 ;;
            *)
                die "未知参数：$1（-h 查看用法）" ;;
        esac
    done

    case "$mode" in
        ops-tools)
            ops_tools_build "$ops_out" ;;
        checksums)
            [ "${#inputs[@]}" -gt 0 ] || die "--checksums 至少需要一个 <dir|files...> 输入"
            local -a files=()
            local f
            if [ "${#inputs[@]}" -eq 1 ] && [ -d "${inputs[0]}" ]; then
                local dir="${inputs[0]}"
                while IFS= read -r f; do files+=("$f"); done \
                    < <(find "$dir" -maxdepth 1 -type f | LC_ALL=C sort)
                generate_checksums "${check_out:-$dir/SHA256SUMS}" "${files[@]}"
            else
                for f in "${inputs[@]}"; do
                    [ -e "$f" ] || die "输入不存在：$f"
                    files+=("$f")
                done
                generate_checksums "${check_out:-$PWD/SHA256SUMS}" "${files[@]}"
            fi ;;
        pack-packages)
            pack_packages "$pack_out" "$pack_version" ;;
        assemble)
            assemble "$asm_out" "$asm_version" ;;
        "")
            die "必须指定模式（--ops-tools / --checksums / --pack-packages / --assemble）；-h 查看用法" ;;
        *)
            die "内部错误：未知模式 $mode" ;;
    esac
}

main "$@"
