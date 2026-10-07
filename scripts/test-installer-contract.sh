#!/usr/bin/env bash
#
# FloatCTF 安装器/升级器**契约静态测试**。
#
# 无依赖：不需要网络、不需要 docker、不需要 root。只读文件 + 跑 `bash -n`。
# 断言的是「安装器目前承诺的对外契约」——CLI 表面、release 产物名、AWD/AWDP 运行时
# 镜像体检、内嵌 uninstall.sh 的活跃运行时守卫、apply_migrations 的 fresh-skip，
# 以及**内嵌 uninstall 脚本自身的语法**（除了本测试，CI 里没有别的东西会检查它）。
#
# 用法：
#   bash scripts/test-installer-contract.sh
#
# 退出码：0 = 全部通过；1 = 有失败项。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SH="$ROOT/scripts/install.sh"
BUILD_SH="$ROOT/scripts/build-runtime-images.sh"

PASS=0
FAIL=0
pass() { printf '✓ %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf '✗ %s\n' "$1"; FAIL=$((FAIL + 1)); }
# check <描述> <命令...>
check() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        pass "$desc"
    else
        fail "$desc"
    fi
}

TMP_TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-installer-contract.XXXXXX")"
cleanup() { rm -rf "$TMP_TEST_DIR"; }
trap cleanup EXIT

# 提取 shell 函数体（从 `name() {` 到列首的 `}`）。
function_body() { # file funcname
    awk -v fn="$2" '
        inb && /^\}/ { exit }
        inb { print }
        index($0, fn "() {") == 1 { inb = 1 }
    ' "$1"
}

# 提取内嵌的 uninstall.sh heredoc（顶层布局：<<'UNINSTALL_EOF' ... UNINSTALL_EOF）。
extract_uninstall() {
    local start
    start="$(grep -n "<<'UNINSTALL_EOF'" "$INSTALL_SH" 2>/dev/null | head -1 | cut -d: -f1)"
    [ -n "${start:-}" ] || return 1
    awk -v s="$start" 'NR > s { if ($0 == "UNINSTALL_EOF") exit; print }' "$INSTALL_SH"
}

# ── 1. 语法 ──────────────────────────────────────────────────────────────────
check "bash -n scripts/install.sh" bash -n "$INSTALL_SH"
check "bash -n scripts/build-runtime-images.sh" bash -n "$BUILD_SH"

# ── 2. install.sh --help ─────────────────────────────────────────────────────
INSTALL_HELP_RC=0
INSTALL_HELP="$(bash "$INSTALL_SH" --help 2>&1)" || INSTALL_HELP_RC=$?
help_has() { printf '%s\n' "$INSTALL_HELP" | grep -q -F -- "$1"; }

if [ "$INSTALL_HELP_RC" -eq 0 ]; then
    pass "scripts/install.sh --help 退出 0"
else
    fail "scripts/install.sh --help 退出 0（实际 rc=$INSTALL_HELP_RC）"
fi
check "install.sh --help 提到 --ops-url" help_has --ops-url
check "install.sh --help 提到 --skip-migrations" help_has --skip-migrations
check "install.sh --help 提到 db/ 布局（migrate.sh）" help_has 'db/migrate.sh'
check "install.sh --help 提到 FLOATCTF_OPS_URL" help_has FLOATCTF_OPS_URL

# ── 3. release 产物名 ────────────────────────────────────────────────────────
for artifact in floatctf floatctf-helper web-dist.tar.gz merged.sql frontend.sh ops-tools; do
    check "install.sh 消费 release 产物 $artifact" grep -q -F -- "$artifact" "$INSTALL_SH"
done

# ── 4. ops-tools 契约 ────────────────────────────────────────────────────────
check "install.sh 解析 --ops-url" grep -q -F -- '--ops-url)' "$INSTALL_SH"
check "install.sh 支持 FLOATCTF_OPS_URL 兜底" grep -q -F 'FLOATCTF_OPS_URL' "$INSTALL_SH"
check "install.sh 有 ops-tools 布局校验 validate_ops_tools" \
    grep -q -F 'validate_ops_tools() {' "$INSTALL_SH"
OPS_VALIDATE_BODY="$(function_body "$INSTALL_SH" validate_ops_tools)"
ops_validate_ok() {
    [ -n "$OPS_VALIDATE_BODY" ] || return 1
    printf '%s\n' "$OPS_VALIDATE_BODY" | grep -q -F 'db/migrate.sh' || return 1
    printf '%s\n' "$OPS_VALIDATE_BODY" | grep -q -F 'db/migrations' || return 1
    printf '%s\n' "$OPS_VALIDATE_BODY" | grep -q -F 'restore.sh' || return 1
    return 0
}
check "validate_ops_tools 校验 backup/restore/db/migrate.sh/migrations" ops_validate_ok
check "install.sh 有 install_ops_tools（装配到 FLOATCTF_HOME）" \
    grep -q -F 'install_ops_tools() {' "$INSTALL_SH"
INSTALL_OPS_BODY="$(function_body "$INSTALL_SH" install_ops_tools)"
install_ops_ok() {
    [ -n "$INSTALL_OPS_BODY" ] || return 1
    printf '%s\n' "$INSTALL_OPS_BODY" | grep -q -F 'backup.sh' || return 1
    printf '%s\n' "$INSTALL_OPS_BODY" | grep -q -F 'restore.sh' || return 1
    printf '%s\n' "$INSTALL_OPS_BODY" | grep -q -F 'db/migrate.sh' || return 1
    printf '%s\n' "$INSTALL_OPS_BODY" | grep -q -F 'merged.sql' || return 1
    printf '%s\n' "$INSTALL_OPS_BODY" | grep -q -F 'bash -n' || return 1
    return 0
}
check "install_ops_tools 装 backup/restore/db/migrate.sh + db/merged.sql 并 bash -n" install_ops_ok

# ── 5. check_runtime_images 体检 ─────────────────────────────────────────────
check "install.sh 有 check_runtime_images" grep -q -F 'check_runtime_images() {' "$INSTALL_SH"
CRI_BODY="$(function_body "$INSTALL_SH" check_runtime_images)"
cri_ok() {
    [ -n "$CRI_BODY" ] || return 1
    printf '%s\n' "$CRI_BODY" | grep -q -F 'floatctf/awd-flagserver:' || return 1
    printf '%s\n' "$CRI_BODY" | grep -q -F 'floatctf/awd-judgeserver:' || return 1
    printf '%s\n' "$CRI_BODY" | grep -q -F 'floatctf/infra/awdp-judgeserver:' || return 1
    printf '%s\n' "$CRI_BODY" | grep -q -F 'docker image inspect' || return 1
    return 0
}
check "check_runtime_images 引用三个 AWD/AWDP 镜像名" cri_ok
check "check_runtime_images 是 warn 而非 die（不阻断 Jeopardy 安装）" \
    bash -c '! grep -q "die " <<<"$1"' _ "$CRI_BODY"
check "check_runtime_images 给出 build-runtime-images.sh 修复命令" \
    grep -q -F 'build-runtime-images.sh --tag' <<<"$CRI_BODY"
check "run_deploy 调用 check_runtime_images" grep -q -F '    check_runtime_images' "$INSTALL_SH"

# ── 6. apply_migrations ──────────────────────────────────────────────────────
check "install.sh 有 apply_migrations" grep -q -F 'apply_migrations() {' "$INSTALL_SH"
AM_BODY="$(function_body "$INSTALL_SH" apply_migrations)"
am_ok() {
    [ -n "$AM_BODY" ] || return 1
    printf '%s\n' "$AM_BODY" | grep -q -F 'PG_VERSION' || return 1
    printf '%s\n' "$AM_BODY" | grep -q -F 'data/postgres' || return 1
    printf '%s\n' "$AM_BODY" | grep -q -F 'fresh 数据库' || return 1
    return 0
}
check "apply_migrations 在 data/postgres 无 PG_VERSION 时跳过（fresh）" am_ok
check "apply_migrations 用 migrate.sh apply" grep -q -F 'migrate.sh" apply' <<<"$AM_BODY"
check "apply_migrations 用 FLOATCTF_CONFIG 传宿主可达 DB URL" \
    grep -q -F 'FLOATCTF_CONFIG="$tmp_cfg"' <<<"$AM_BODY"
check "apply_migrations 有 --skip-migrations 短路" \
    grep -q -F 'SKIP_MIGRATIONS' <<<"$AM_BODY"
check "install.sh 解析 --skip-migrations" grep -q -F -- '--skip-migrations)' "$INSTALL_SH"
check "run_deploy 调用 apply_migrations" grep -q -F '    apply_migrations' "$INSTALL_SH"

# ── 7. 内嵌 uninstall.sh：提取 + 语法 + 守卫契约 ─────────────────────────────
UNINSTALL_RAW="$TMP_TEST_DIR/uninstall.raw.sh"
UNINSTALL_SUBST="$TMP_TEST_DIR/uninstall.subst.sh"
extract_uninstall > "$UNINSTALL_RAW" 2>/dev/null || true
if [ -s "$UNINSTALL_RAW" ]; then
    pass "从 install.sh 提取内嵌 uninstall.sh（$(( $(wc -l < "$UNINSTALL_RAW") )) 行）"
else
    fail "从 install.sh 提取内嵌 uninstall.sh"
fi
sed 's|__FLOATCTF_HOME__|/var/lib/floatctf|g' "$UNINSTALL_RAW" > "$UNINSTALL_SUBST" 2>/dev/null || true

check "内嵌 uninstall.sh 语法 bash -n（替换 __FLOATCTF_HOME__ 后）" \
    bash -n "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 已无 __FLOATCTF_HOME__ 占位残留" \
    bash -c '! grep -q "__FLOATCTF_HOME__" "$1"' _ "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 含 active_runtime_guard" \
    grep -q -F 'active_runtime_guard() {' "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 含 probe_active_runtime" \
    grep -q -F 'probe_active_runtime' "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 解析 --force" grep -q -F -- '--force) SAFE_FORCE=1' "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 查询 awd_events" grep -q -F 'awd_events' "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 查询 awdp_runs" grep -q -F 'awdp_runs' "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh 在 safe_uninstall 顶部调用守卫" \
    bash -c 'awk "/^safe_uninstall\\(\\) \\{/{f=1} f&&/active_runtime_guard/{print;exit}" "$1" | grep -q active_runtime_guard' _ "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh --help 退出 0 并提到 --force" \
    bash -c 'out="$(bash "$1" --help 2>&1)" && grep -q -F -- "--force" <<<"$out"' _ "$UNINSTALL_SUBST"
check "内嵌 uninstall.sh usage 文档化活跃守卫" \
    grep -q -F '活跃' "$UNINSTALL_SUBST"

# ── 8. build-runtime-images.sh --help / tag 契约 ─────────────────────────────
BUILD_HELP_RC=0
BUILD_HELP="$(bash "$BUILD_SH" --help 2>&1)" || BUILD_HELP_RC=$?
if [ "$BUILD_HELP_RC" -eq 0 ]; then
    pass "scripts/build-runtime-images.sh --help 退出 0"
else
    fail "scripts/build-runtime-images.sh --help 退出 0（实际 rc=$BUILD_HELP_RC）"
fi
check "build-runtime-images.sh --help 提到 --tag" \
    bash -c 'grep -q -F -- "--tag" <<<"$1"' _ "$BUILD_HELP"
check "build-runtime-images.sh --help 提到 --extra-tag" \
    bash -c 'grep -q -F -- "--extra-tag" <<<"$1"' _ "$BUILD_HELP"
check "build-runtime-images.sh 解析 --tag" grep -q -F -- '--tag)' "$BUILD_SH"
check "build-runtime-images.sh 支持 FLOATCTF_RUNTIME_IMAGE_TAG" \
    grep -q -F 'FLOATCTF_RUNTIME_IMAGE_TAG' "$BUILD_SH"
check "build-runtime-images.sh 保留 :latest（向后兼容）" \
    grep -q -F 'image:latest' "$BUILD_SH"
check "build-runtime-images.sh 按主 tag 打三个镜像" \
    grep -q -F 'floatctf/awd-flagserver:$RUNTIME_TAG' "$BUILD_SH"
check "build-runtime-images.sh 仍对打 tag 的镜像做 ldd 校验" \
    grep -q -F 'check_image "floatctf/awd-flagserver:$RUNTIME_TAG"' "$BUILD_SH"

# ── 结果 ─────────────────────────────────────────────────────────────────────
echo ""
printf '通过 %d 项，失败 %d 项\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
