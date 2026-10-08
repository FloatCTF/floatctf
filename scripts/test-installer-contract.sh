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

# ── 5. ensure_runtime_images：硬要求（B1）────────────────────────────────────
check "install.sh 有 ensure_runtime_images（取代 warn-only 的 check_runtime_images）" \
    grep -q -F 'ensure_runtime_images() {' "$INSTALL_SH"
check "install.sh 已无旧 check_runtime_images" \
    bash -c '! grep -q "check_runtime_images" "$1"' _ "$INSTALL_SH"
ERI_BODY="$(function_body "$INSTALL_SH" ensure_runtime_images)"
eri_ok() {
    [ -n "$ERI_BODY" ] || return 1
    # 三个 canonical ref 由 registry 前缀 + 扁平化名拼出（awdp 已无 infra/ 段）
    printf '%s\n' "$ERI_BODY" | grep -q -F 'runtime_image_ref awd-flagserver' || return 1
    printf '%s\n' "$ERI_BODY" | grep -q -F 'runtime_image_ref awd-judgeserver' || return 1
    printf '%s\n' "$ERI_BODY" | grep -q -F 'runtime_image_ref awdp-judgeserver' || return 1
    printf '%s\n' "$ERI_BODY" | grep -q -F 'RUNTIME_IMAGE_REGISTRY' || return 1
    printf '%s\n' "$ERI_BODY" | grep -q -F 'docker image inspect' || return 1
    printf '%s\n' "$ERI_BODY" | grep -q -F 'docker pull' || return 1
    return 0
}
check "ensure_runtime_images 引用三个 canonical ref 并 inspect/pull" eri_ok
check "ensure_runtime_images 缺镜像时 die（默认硬失败）" \
    grep -q -F 'die "缺少 AWD/AWDP 运行时镜像' <<<"$ERI_BODY"
check "ensure_runtime_images 给出精确 docker pull 命令" \
    grep -q -F 'pull_cmd=' <<<"$ERI_BODY"
check "ensure_runtime_images 给出 docker load escape hatch" \
    grep -q -F 'docker load < floatctf-runtime-images.tar' <<<"$ERI_BODY"
check "ensure_runtime_images 支持 --skip-runtime-images 降级告警" \
    grep -q -F 'SKIP_RUNTIME_IMAGES' <<<"$ERI_BODY"
check "RUNTIME_IMAGE_REGISTRY 默认 canonical ghcr.io/floatctf" \
    grep -q -F 'RUNTIME_IMAGE_REGISTRY="${FLOATCTF_RUNTIME_IMAGE_REGISTRY:-ghcr.io/floatctf}"' "$INSTALL_SH"
check "install.sh 解析 --skip-runtime-images" grep -q -F -- '--skip-runtime-images)' "$INSTALL_SH"
check "run_deploy 调用 ensure_runtime_images" \
    grep -q -F '    ensure_runtime_images' "$INSTALL_SH"
# 必须在任何安装文件写入之前调用（run_deploy 里 precheck 之后、prepare_env 之前）。
run_deploy_body="$(function_body "$INSTALL_SH" run_deploy)"
erd_order_ok() {
    local pre env_line
    pre="$(printf '%s\n' "$run_deploy_body" | grep -n 'ensure_runtime_images' | head -1 | cut -d: -f1)"
    env_line="$(printf '%s\n' "$run_deploy_body" | grep -n 'prepare_env' | head -1 | cut -d: -f1)"
    [ -n "$pre" ] && [ -n "$env_line" ] && [ "$pre" -lt "$env_line" ]
}
check "ensure_runtime_images 在 prepare_env（写 .env/目录）之前调用" erd_order_ok

# ── 6. config 模板 canonical ref + 升级保护（§2.5）───────────────────────────
check "floatctf.toml 模板用 canonical registry + \$VERSION" \
    grep -q -F 'flagserver_image = "${RUNTIME_IMAGE_REGISTRY}/awd-flagserver:${VERSION}"' "$INSTALL_SH"
check "模板 judgeserver 也是 canonical" \
    grep -q -F 'judgeserver_image = "${RUNTIME_IMAGE_REGISTRY}/awd-judgeserver:${VERSION}"' "$INSTALL_SH"
check "模板 awdp 已扁平化（无 infra/ 段）" \
    grep -q -F 'practice_judgeserver_image = "${RUNTIME_IMAGE_REGISTRY}/awdp-judgeserver:${VERSION}"' "$INSTALL_SH"
check "install.sh 有 preserve_custom_runtime_images 升级保护" \
    grep -q -F 'preserve_custom_runtime_images() {' "$INSTALL_SH"
check "prepare_configs 调用 preserve_custom_runtime_images" \
    grep -q -F 'preserve_custom_runtime_images "$prev_toml"' "$INSTALL_SH"
check "install.sh 有 is_stock_runtime_image" grep -q -F 'is_stock_runtime_image() {' "$INSTALL_SH"
STOCK_BODY="$(function_body "$INSTALL_SH" is_stock_runtime_image)"
stock_ok() {
    [ -n "$STOCK_BODY" ] || return 1
    printf '%s\n' "$STOCK_BODY" | grep -q -F 'floatctf/awd-flagserver:*' || return 1
    printf '%s\n' "$STOCK_BODY" | grep -q -F 'floatctf/awd-judgeserver:*' || return 1
    printf '%s\n' "$STOCK_BODY" | grep -q -F 'floatctf/infra/awdp-judgeserver:*' || return 1
    printf '%s\n' "$STOCK_BODY" | grep -q -F 'ghcr.io/floatctf/awd-flagserver:*' || return 1
    printf '%s\n' "$STOCK_BODY" | grep -q -F 'ghcr.io/floatctf/awd-judgeserver:*' || return 1
    printf '%s\n' "$STOCK_BODY" | grep -q -F 'ghcr.io/floatctf/awdp-judgeserver:*' || return 1
    return 0
}

# 宿主只创建 floatctf 组（不创建该用户，见 AGENTS.md），因此安装器里
# 任何 `chown "floatctf":"floatctf"` 都会在全新生效的宿主机上失败。
no_user_chown() {
    ! grep -q -F 'chown "$FCTF_USER":"$FCTF_USER"' "$INSTALL_SH"
}
check "is_stock_runtime_image 含全部 6 个 stock 形态（含旧 floatctf/infra/awdp）" stock_ok
check "install.sh 解析 --reset-runtime-images" grep -q -F -- '--reset-runtime-images)' "$INSTALL_SH"
check "install.sh 解析 --keep-runtime-images" grep -q -F -- '--keep-runtime-images)' "$INSTALL_SH"
check "--reset/--keep-runtime-images 互斥（fail closed）" \
    grep -q -F '互斥，请只选一个' "$INSTALL_SH"
check "preserve 写回自定义值并 warn" \
    grep -q -F '保留管理员自定义的运行时镜像' "$INSTALL_SH"

# runtime 目录只能按数值 UID chown：宿主只创建 floatctf 组、不创建该用户
# （见 AGENTS.md 生产说明），用用户名 chown 会在全新宿主机上以
# "chown: invalid user: 'floatctf:floatctf'" 中断整个安装（实测）。
check "install.sh 按数值 UID chown runtime" \
    grep -q -F 'chown "$FCTF_UID":"$FCTF_USER" "$FLOATCTF_HOME/runtime"' "$INSTALL_SH"
check "install.sh 没有 chown 到 floatctf 用户名（该用户不存在）" \
    no_user_chown

# ── 7. R3：Python tomllib 能力探测 ───────────────────────────────────────────
check "install.sh 有 check_python_tomllib 能力探测（import tomllib）" \
    grep -q -F "python3 -c 'import tomllib'" "$INSTALL_SH"
check "install.sh 不在版本字符串上做唯一判断（同时报出 python3 -V）" \
    grep -q -F 'python3 -V' "$INSTALL_SH"
check "install.sh 绝不做 pip install" \
    bash -c '! grep -qE "pip[[:space:]]+install" "$1"' _ "$INSTALL_SH"
check "check_commands 调用 check_python_tomllib" \
    bash -c 'awk "/^check_commands\\(\\) \\{/{f=1} f&&/^\\}/{exit} f" "$1" | grep -q check_python_tomllib' _ "$INSTALL_SH"
check "precheck 调用 check_python_tomllib（部署路径纵深防御）" \
    bash -c 'awk "/^precheck\\(\\) \\{/{f=1} f&&/^\\}/{exit} f" "$1" | grep -q check_python_tomllib' _ "$INSTALL_SH"
check "check_python_tomllib 报错信息给出 ≥3.11 要求" \
    grep -q -F 'Python ≥3.11' "$INSTALL_SH"
check "check_python_tomllib 报错信息说明迁移路径需要它" \
    grep -q -F '升级/迁移路径' "$INSTALL_SH"
# main() 中 tomllib 检查必须早于 run_init（run_init 才 require_root / 装包 / 建目录）。
MAIN_START="$(grep -n '^main() {' "$INSTALL_SH" | tail -1 | cut -d: -f1)"
# 去掉注释行后再比较顺序：main() 的说明注释里也提到 run_init，不能让它干扰断言。
MAIN_BODY="$(awk -v s="${MAIN_START:-1}" 'NR >= s' "$INSTALL_SH" | grep -v '^[[:space:]]*#')"
main_order_ok() {
    local py init_line
    py="$(printf '%s\n' "$MAIN_BODY" | grep -n 'check_python_tomllib' | head -1 | cut -d: -f1)"
    init_line="$(printf '%s\n' "$MAIN_BODY" | grep -n 'run_init' | head -1 | cut -d: -f1)"
    [ -n "$py" ] && [ -n "$init_line" ] && [ "$py" -lt "$init_line" ]
}
check "main() 先 check_python_tomllib 再 run_init（任何 mutation 之前）" main_order_ok
check "run_init 仍然 require_root（main 的 tomllib 检查确实早于提权/装包）" \
    bash -c 'awk "/^run_init\\(\\) \\{/{f=1} f&&/^\\}/{exit} f" "$1" | grep -q require_root' _ "$INSTALL_SH"

# ── 8. apply_migrations ──────────────────────────────────────────────────────
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

# ── 9. 内嵌 uninstall.sh：提取 + 语法 + 守卫契约 ─────────────────────────────
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

# ── 10. build-runtime-images.sh --help / registry / push 契约 ────────────────
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
check "build-runtime-images.sh --help 提到 --registry" \
    bash -c 'grep -q -F -- "--registry" <<<"$1"' _ "$BUILD_HELP"
check "build-runtime-images.sh --help 提到 --label" \
    bash -c 'grep -q -F -- "--label" <<<"$1"' _ "$BUILD_HELP"
check "build-runtime-images.sh --help 提到 --push 且默认为不推送" \
    bash -c 'grep -q -F -- "--push" <<<"$1" && grep -q -F "默认**不推送**" <<<"$1"' _ "$BUILD_HELP"
check "build-runtime-images.sh --help 列出 canonical ghcr refs" \
    bash -c 'for r in ghcr.io/floatctf/awd-flagserver:1.0.0 ghcr.io/floatctf/awd-judgeserver:1.0.0 ghcr.io/floatctf/awdp-judgeserver:1.0.0; do grep -q -F -- "$r" <<<"$1" || exit 1; done' _ "$BUILD_HELP"
check "build-runtime-images.sh --help 文档化 awdp 扁平化映射" \
    bash -c 'grep -q -F "扁平化" <<<"$1"' _ "$BUILD_HELP"
check "build-runtime-images.sh 解析 --tag" grep -q -F -- '--tag)' "$BUILD_SH"
check "build-runtime-images.sh 解析 --registry" grep -q -F -- '--registry)' "$BUILD_SH"
check "build-runtime-images.sh 解析 --label" grep -q -F -- '--label)' "$BUILD_SH"
check "build-runtime-images.sh 解析 --push" grep -q -F -- '--push) PUSH=1' "$BUILD_SH"
check "build-runtime-images.sh 支持 FLOATCTF_RUNTIME_IMAGE_TAG" \
    grep -q -F 'FLOATCTF_RUNTIME_IMAGE_TAG' "$BUILD_SH"
check "build-runtime-images.sh 支持 FLOATCTF_RUNTIME_IMAGE_REGISTRY" \
    grep -q -F 'FLOATCTF_RUNTIME_IMAGE_REGISTRY' "$BUILD_SH"
check "build-runtime-images.sh 保留 :latest（向后兼容）" \
    grep -q -F 'image:latest' "$BUILD_SH"
check "build-runtime-images.sh 按逻辑名打三个镜像（registry 前缀统一计算）" \
    bash -c 'grep -q -F "build_image awd-flagserver " "$1" &&
             grep -q -F "build_image awd-judgeserver " "$1" &&
             grep -q -F "build_image awdp-judgeserver " "$1"' _ "$BUILD_SH"
check "build-runtime-images.sh 仍对打 tag 的镜像做 ldd 校验" \
    grep -q -F 'check_image "$(image_ref "$REGISTRY"' "$BUILD_SH"
check "build-runtime-images.sh 无 --push 时不执行 docker push（PUSH 守门）" \
    bash -c 'awk "/if \\[ \"\\\$PUSH\" = \"1\" \\]; then/{f=1} f" "$1" | grep -q "docker push"' _ "$BUILD_SH"

# ── 11. R3 负向测试：tomllib 能力探测在任何 mutation 之前 fail closed ────────
#
# 这个测试**真实运行** scripts/install.sh（不是静态 grep）：
#   * PATH 最前面放一个假 python3：`import tomllib` 退出 1，`-V` 输出 Python 3.10.13；
#   * FLOATCTF_HOME 指向一个**尚不存在**的临时目录；
#   * 只传 --version 1.0.0（arg 解析只需要版本；6 个 URL 仍是 v0.0.0-fake 占位，
#     但断言的是"根本走不到下载"）。
# 断言 4 件事：
#   1) 退出码非 0；
#   2) stderr 是 tomllib 的精确报错（≥3.11 要求 + 检测到的版本 + 迁移路径说明）；
#   3) 失败发生在 require_root 之前（stderr 不含「需要 root」）——证明真的到达了
#      tomllib 检查点，而不是停在别的更早错误上；
#   4) FLOATCTF_HOME 目录不存在（安装器没有创建任何文件/目录 = 无 mutation）。
# 合起来证明：check_python_tomllib 在 main() 中先于 require_root / run_init（装包、
# 建目录）/ 下载 / docker build / 迁移执行，失败时宿主上不留任何副作用。
FAKE_BIN="$TMP_TEST_DIR/fakebin"
mkdir -p "$FAKE_BIN"
cat > "$FAKE_BIN/python3" <<'FAKE_PY'
#!/usr/bin/env bash
# 模拟 Python <3.11（无 stdlib tomllib）
if [ "${1:-}" = "-c" ] && [[ "${2:-}" == *tomllib* ]]; then exit 1; fi
if [ "${1:-}" = "-V" ] || [ "${1:-}" = "--version" ]; then echo "Python 3.10.13"; exit 0; fi
exit 0
FAKE_PY
chmod +x "$FAKE_BIN/python3"

R3_HOME="$TMP_TEST_DIR/r3-home-not-created"
R3_ERR="$TMP_TEST_DIR/r3.err"
R3_RC=0
env PATH="$FAKE_BIN:$PATH" FLOATCTF_HOME="$R3_HOME" \
    bash "$INSTALL_SH" --version 1.0.0 >"$TMP_TEST_DIR/r3.out" 2>"$R3_ERR" || R3_RC=$?

if [ "$R3_RC" -ne 0 ]; then
    pass "R3 负向：假 python3（无 tomllib）时安装器退出非 0（rc=$R3_RC）"
else
    fail "R3 负向：假 python3（无 tomllib）时安装器仍退出 0"
fi
r3_msg_ok() {
    grep -q -F 'tomllib' "$R3_ERR" || return 1
    grep -q -F 'Python ≥3.11' "$R3_ERR" || return 1
    grep -q -F '3.10.13' "$R3_ERR" || return 1
    grep -q -F '升级/迁移路径' "$R3_ERR" || return 1
    return 0
}
check "R3 负向：stderr 是精确的 tomllib 报错（要求+检测版本+迁移路径）" r3_msg_ok
check "R3 负向：失败于 require_root 之前（确实到达检查点，而非停在别处）" \
    bash -c '! grep -q "需要 root" "$1"' _ "$R3_ERR"
if [ ! -e "$R3_HOME" ]; then
    pass "R3 负向：FLOATCTF_HOME 目录未被创建（无任何 mutation）"
elif [ -z "$(ls -A "$R3_HOME" 2>/dev/null)" ]; then
    pass "R3 负向：FLOATCTF_HOME 目录为空（无任何 mutation）"
else
    fail "R3 负向：FLOATCTF_HOME 出现文件/目录（发生了 mutation）"
fi

# ── 12. 行为测试：模板渲染 + 升级保护（提取真实函数到隔离 harness）───────────
#
# 静态 grep 只能证明"代码里写了 canonical ref / 保留了自定义值"，证明不了**行为**。
# 这里把 install.sh 里的真实函数体抽出来（function_body 会丢掉 `name() {` 头，
# 故 emit_fn 补回），在临时目录里跑：
#   * write_config_template + render → 渲染结果必须是 canonical GHCR refs（用 VERSION 钉版）；
#   * preserve_custom_runtime_images → 三种模式（默认/reset/keep）语义逐条断言。
# 全部只写临时目录，不联网、不碰真实安装根。
emit_fn() { # <file> <fn>
    printf '%s() {\n' "$2"
    function_body "$1" "$2"
    printf '}\n'
}

RENDER_HARNESS="$TMP_TEST_DIR/render-harness.sh"
{
    cat <<'HDR'
set -uo pipefail
FLOATCTF_HOME="$1"; VERSION=1.0.0; RUNTIME_IMAGE_REGISTRY=ghcr.io/floatctf
export FLOATCTF_HOME VERSION RUNTIME_IMAGE_REGISTRY
SITE_ADDRESS=ctf.example.com; HTTPS_PORT=443; HOST_ADDRESS=127.0.0.1; API_PORT=9090
POSTGRES_USER=pu; POSTGRES_PASSWORD=pw; POSTGRES_DB=db; RUSTFS_ACCESS_KEY=ak; RUSTFS_SECRET_KEY=sk
JWT_SECRET=js; AWD_ROOT_KEY=ar; INTERNAL_TOKEN_KEY=it
export SITE_ADDRESS HTTPS_PORT HOST_ADDRESS API_PORT POSTGRES_USER POSTGRES_PASSWORD POSTGRES_DB
export RUSTFS_ACCESS_KEY RUSTFS_SECRET_KEY JWT_SECRET AWD_ROOT_KEY INTERNAL_TOKEN_KEY
info(){ :; }; ok(){ :; }; warn(){ printf 'WARN %s\n' "$*"; }; die(){ printf 'FAIL %s\n' "$*" >&2; exit 1; }
HDR
    emit_fn "$INSTALL_SH" write_config_template
    emit_fn "$INSTALL_SH" render
    cat <<'DRV'
mkdir -p "$FLOATCTF_HOME/config"
write_config_template
sed "s|\${FLOATCTF_HOME}|$FLOATCTF_HOME|g" "$FLOATCTF_HOME/.floatctf.toml.tmpl" > "$FLOATCTF_HOME/.real"
render "$FLOATCTF_HOME/.real" "$FLOATCTF_HOME/config/floatctf.toml"
grep -E '^(flagserver_image|judgeserver_image|practice_judgeserver_image)' "$FLOATCTF_HOME/config/floatctf.toml"
DRV
} >"$RENDER_HARNESS"

RENDER_HOME="$TMP_TEST_DIR/render-home"
RENDER_OUT="$(bash "$RENDER_HARNESS" "$RENDER_HOME" 2>&1)" || true
check "行为：模板渲染出 canonical flagserver（registry + VERSION 钉版）" \
    bash -c 'grep -qxF "flagserver_image = \"ghcr.io/floatctf/awd-flagserver:1.0.0\"" <<<"$1"' _ "$RENDER_OUT"
check "行为：模板渲染出 canonical judgeserver" \
    bash -c 'grep -qxF "judgeserver_image = \"ghcr.io/floatctf/awd-judgeserver:1.0.0\"" <<<"$1"' _ "$RENDER_OUT"
check "行为：模板渲染出扁平化 canonical awdp judgeserver" \
    bash -c 'grep -qxF "practice_judgeserver_image = \"ghcr.io/floatctf/awdp-judgeserver:1.0.0\"" <<<"$1"' _ "$RENDER_OUT"
check "行为：渲染结果不含旧的 floatctf/infra/ 本地名" \
    bash -c '! grep -qF "floatctf/infra/" <<<"$1"' _ "$RENDER_OUT"

PRESERVE_HARNESS="$TMP_TEST_DIR/preserve-harness.sh"
{
    cat <<'HDR'
set -uo pipefail
RESET_RUNTIME_IMAGES="${RESET_RUNTIME_IMAGES:-0}"; KEEP_RUNTIME_IMAGES="${KEEP_RUNTIME_IMAGES:-0}"
warn(){ printf 'WARN %s\n' "$*"; }; die(){ printf 'FAIL %s\n' "$*" >&2; exit 1; }; ok(){ :; }
HDR
    emit_fn "$INSTALL_SH" toml_get_string_key
    emit_fn "$INSTALL_SH" toml_set_string_key
    emit_fn "$INSTALL_SH" is_stock_runtime_image
    emit_fn "$INSTALL_SH" preserve_custom_runtime_images
    cat <<'DRV'
preserve_custom_runtime_images "$1" "$2"
grep -E '^(flagserver_image|judgeserver_image|practice_judgeserver_image)' "$2"
DRV
} >"$PRESERVE_HARNESS"

P_OLD="$TMP_TEST_DIR/old.toml"
P_NEW="$TMP_TEST_DIR/new.toml"
cat >"$P_OLD" <<'TOML'
[awd]
flagserver_image = "my.registry.local/awd-flagserver:9.9.9"
judgeserver_image = "floatctf/awd-judgeserver:0.3.3"

[awdp]
practice_judgeserver_image = "floatctf/infra/awdp-judgeserver:0.3.3"
TOML
cat >"$P_NEW" <<'TOML'
[awd]
flagserver_image = "ghcr.io/floatctf/awd-flagserver:1.0.0"
judgeserver_image = "ghcr.io/floatctf/awd-judgeserver:1.0.0"

[awdp]
practice_judgeserver_image = "ghcr.io/floatctf/awdp-judgeserver:1.0.0"
TOML

cp "$P_NEW" "$TMP_TEST_DIR/p-default.toml"
P_DEFAULT_OUT="$(bash "$PRESERVE_HARNESS" "$P_OLD" "$TMP_TEST_DIR/p-default.toml" 2>&1)" || true
check "行为（默认）：自定义镜像引用被保留 + warn" \
    bash -c 'grep -qF "flagserver_image = \"my.registry.local/awd-flagserver:9.9.9\"" <<<"$1" &&
             grep -qF "保留管理员自定义的运行时镜像" <<<"$1"' _ "$P_DEFAULT_OUT"
check "行为（默认）：stock 的 floatctf/awd-judgeserver 迁移到 canonical" \
    bash -c 'grep -qF "judgeserver_image = \"ghcr.io/floatctf/awd-judgeserver:1.0.0\"" <<<"$1"' _ "$P_DEFAULT_OUT"
check "行为（默认）：stock 的旧 floatctf/infra/awdp 迁移到扁平化 canonical" \
    bash -c 'grep -qF "practice_judgeserver_image = \"ghcr.io/floatctf/awdp-judgeserver:1.0.0\"" <<<"$1"' _ "$P_DEFAULT_OUT"

cp "$P_NEW" "$TMP_TEST_DIR/p-reset.toml"
P_RESET_OUT="$(RESET_RUNTIME_IMAGES=1 bash "$PRESERVE_HARNESS" "$P_OLD" "$TMP_TEST_DIR/p-reset.toml" 2>&1)" || true
check "行为（--reset-runtime-images）：自定义值被丢弃，全部回到 canonical" \
    bash -c 'grep -qxF "flagserver_image = \"ghcr.io/floatctf/awd-flagserver:1.0.0\"" <<<"$1" &&
             grep -qxF "judgeserver_image = \"ghcr.io/floatctf/awd-judgeserver:1.0.0\"" <<<"$1" &&
             grep -qF "丢弃" <<<"$1"' _ "$P_RESET_OUT"

cp "$P_NEW" "$TMP_TEST_DIR/p-keep.toml"
P_KEEP_OUT="$(KEEP_RUNTIME_IMAGES=1 bash "$PRESERVE_HARNESS" "$P_OLD" "$TMP_TEST_DIR/p-keep.toml" 2>&1)" || true
check "行为（--keep-runtime-images）：连 stock 旧值也原样保留" \
    bash -c 'grep -qF "judgeserver_image = \"floatctf/awd-judgeserver:0.3.3\"" <<<"$1" &&
             grep -qF "practice_judgeserver_image = \"floatctf/infra/awdp-judgeserver:0.3.3\"" <<<"$1"' _ "$P_KEEP_OUT"

# ── 13. 行为测试：ensure_runtime_images（stub docker，硬失败/跳过/齐备）────────
#
# B1 的核心承诺是"缺镜像就 die，且报错可操作"。这里用 stub docker 把三种状态
# 各跑一遍，断言退出码与精确文案（不碰真实 docker / registry）：
#   * 齐备      → 退出 0，打印"齐备"；
#   * 缺失/拉取失败 → 退出非 0，报错含三个 ref、精确 pull 命令、docker load escape hatch；
#   * 缺失 + --skip-runtime-images → 退出 0，降级为醒目告警（只跑 Jeopardy 的宿主）。
STUB_DIR="$TMP_TEST_DIR/stubbin-runtime"
mkdir -p "$STUB_DIR"
cat >"$STUB_DIR/docker" <<'STUB'
#!/usr/bin/env bash
# STUB_PRESENT=1 → 镜像存在/可拉取；否则 inspect 与 pull 都失败。
case "${1:-}" in
    image|pull) [ "${STUB_PRESENT:-0}" = "1" ] && exit 0 || exit 1 ;;
    *) exit 0 ;;
esac
STUB
chmod +x "$STUB_DIR/docker"

ENSURE_HARNESS="$TMP_TEST_DIR/ensure-harness.sh"
{
    cat <<'HDR'
set -uo pipefail
RUNTIME_IMAGE_REGISTRY="${RUNTIME_IMAGE_REGISTRY:-ghcr.io/floatctf}"
VERSION="${VERSION:-1.0.0}"
SKIP_RUNTIME_IMAGES="${SKIP_RUNTIME_IMAGES:-0}"
RUNTIME_IMAGES_MISSING=""
info(){ printf 'INFO %s\n' "$*"; }; ok(){ printf 'OK %s\n' "$*"; }
warn(){ printf 'WARN %s\n' "$*"; }; die(){ printf 'FAIL %s\n' "$*" >&2; exit 1; }
HDR
    emit_fn "$INSTALL_SH" runtime_image_ref
    emit_fn "$INSTALL_SH" ensure_runtime_images
    printf 'ensure_runtime_images\n'
} >"$ENSURE_HARNESS"

ENSURE_OK_RC=0
ENSURE_OK_OUT="$(PATH="$STUB_DIR:$PATH" STUB_PRESENT=1 bash "$ENSURE_HARNESS" 2>&1)" || ENSURE_OK_RC=$?
if [ "$ENSURE_OK_RC" -eq 0 ] && grep -q -F '齐备' <<<"$ENSURE_OK_OUT"; then
    pass "行为：三个镜像齐备时 ensure_runtime_images 通过"
else
    fail "行为：三个镜像齐备时 ensure_runtime_images 未通过（rc=$ENSURE_OK_RC）"
fi

ENSURE_BAD_RC=0
ENSURE_BAD_OUT="$(PATH="$STUB_DIR:$PATH" STUB_PRESENT=0 bash "$ENSURE_HARNESS" 2>&1)" || ENSURE_BAD_RC=$?
if [ "$ENSURE_BAD_RC" -ne 0 ]; then
    pass "行为：缺镜像且拉取失败时 ensure_runtime_images 硬失败（rc=$ENSURE_BAD_RC）"
else
    fail "行为：缺镜像且拉取失败时 ensure_runtime_images 未失败"
fi
ensure_bad_ok() {
    local out="$1"
    grep -q -F 'ghcr.io/floatctf/awd-flagserver:1.0.0' <<<"$out" || return 1
    grep -q -F 'ghcr.io/floatctf/awd-judgeserver:1.0.0' <<<"$out" || return 1
    grep -q -F 'ghcr.io/floatctf/awdp-judgeserver:1.0.0' <<<"$out" || return 1
    grep -q -F 'docker pull ghcr.io/floatctf/awd-flagserver:1.0.0' <<<"$out" || return 1
    grep -q -F 'sudo docker load < floatctf-runtime-images.tar' <<<"$out" || return 1
    grep -q -F -- '--skip-runtime-images' <<<"$out" || return 1
    return 0
}
check "行为：硬失败信息列出三个 ref + 精确 pull 命令 + docker load 逃生口" \
    ensure_bad_ok "$ENSURE_BAD_OUT"

ENSURE_SKIP_RC=0
ENSURE_SKIP_OUT="$(PATH="$STUB_DIR:$PATH" STUB_PRESENT=0 SKIP_RUNTIME_IMAGES=1 bash "$ENSURE_HARNESS" 2>&1)" || ENSURE_SKIP_RC=$?
if [ "$ENSURE_SKIP_RC" -eq 0 ] \
    && grep -q -F '运行时镜像硬校验' <<<"$ENSURE_SKIP_OUT" \
    && grep -q -F 'FLOATCTF_SKIP_RUNTIME_IMAGES=1' <<<"$ENSURE_SKIP_OUT"; then
    pass "行为：--skip-runtime-images 降级为告警且不阻断（rc=0）"
else
    fail "行为：--skip-runtime-images 未降级为告警（rc=$ENSURE_SKIP_RC）"
fi
check "行为：跳过时仍打印缺失 ref 与 docker load 逃生口" \
    bash -c 'grep -qF "ghcr.io/floatctf/awdp-judgeserver:1.0.0" <<<"$1" &&
             grep -qF "docker load < floatctf-runtime-images.tar" <<<"$1"' _ "$ENSURE_SKIP_OUT"

# ── R2：rustfs 就绪探测契约（CI 静态门禁；真实 Docker E2E 见 test-rustfs-readiness.sh）──
# 背景：TCP 开放 != S3/HTTP 层可用，旧的 `nc -z` 探测会让 API 在 RustFS 就绪前
# 初始化 bucket -> panic -> crash-loop（Phase 13 实测）。这里断言安装器模板用的是
# 真实 HTTP /health 探测，且参数足以容纳探针自身耗时（timeout 必须 > sleep+nc 超时）。
check "install.sh 的 rustfs healthcheck 是真实 HTTP /health 探测（非 TCP-only）" \
    bash -c 'grep -qF "GET /health HTTP/1.1" "$1" && ! grep -qF "nc -z 127.0.0.1 9000 || exit 1" "$1"' _ "$ROOT/scripts/install.sh"
check "install.sh 的 rustfs healthcheck 参数容纳探针耗时（interval 10s/timeout 10s/retries 12/start_period 30s）" \
    bash -c 'f="$1"; for p in "interval: 10s" "timeout: 10s" "retries: 12" "start_period: 30s"; do
        grep -qF "$p" <<<"$(awk "/healthcheck:/{n++} n>=2 && n<=3" "$f")" || { echo "missing: $p"; exit 1; }
      done' _ "$ROOT/scripts/install.sh"

# ── 参考文件与安装器模板的一致性（防止静默漂移，see Phase 13.1 §10.5/§10.2）──
# 生产 Caddyfile 的权威来源是 install.sh 内嵌的 CADDY_TMPL_EOF；infra/caddy/Caddyfile.prod
# 是给运维阅读的镜像副本（仅多一个“勿单独编辑”头部）。这里断言正文逐字节相同，
# 否则一旦有人只改一处，运维照抄参考文件就会部署出与安装器不同的站点配置。
check "参考 Caddyfile 与 install.sh 内嵌模板正文逐字节一致（无静默漂移）" \
    python3 - "$ROOT" <<'PYEOF'
import pathlib, re, sys
root = pathlib.Path(sys.argv[1])
ins = (root / "scripts/install.sh").read_text()
m = re.search(r"<<'CADDY_TMPL_EOF'\n(.*?)\nCADDY_TMPL_EOF", ins, re.S)
if not m:
    print("install.sh 中找不到 CADDY_TMPL_EOF 正文"); raise SystemExit(1)
body = m.group(1)
ref = (root / "infra/caddy/Caddyfile.prod").read_text()
# 参考文件正文从模板首行（注释锚点）开始，忽略其上的镜像说明头。
anchor_line = "# FloatCTF production Caddy configuration."
idx = ref.find(anchor_line)
if idx < 0:
    print("参考 Caddyfile 缺少模板首行锚点:", anchor_line); raise SystemExit(1)
ref_body = ref[idx:].rstrip("\n")
if ref_body != body.rstrip("\n"):
    import difflib
    d = "\n".join(difflib.unified_diff(body.splitlines(), ref_body.splitlines(),
                                       "install.sh:Caddyfile", "infra/caddy/Caddyfile.prod", lineterm=""))
    print(d[:2000]); raise SystemExit(1)
PYEOF

# 参考生产 TOML 必须覆盖安装器模板里的每一个键/值（允许参考文件多出注释，不允许少键或值不同）。
check "参考 floatctf.prod.toml 覆盖安装器模板的全部键值（无静默漂移）" \
    python3 - "$ROOT" <<'PYEOF'
import pathlib, re, sys
root = pathlib.Path(sys.argv[1])
ins = (root / "scripts/install.sh").read_text()
m = re.search(r"<<'CONFIG_TMPL_EOF'\n(.*?)\nCONFIG_TMPL_EOF", ins, re.S)
if not m:
    print("install.sh 中找不到 CONFIG_TMPL_EOF 正文"); raise SystemExit(1)
want = []
for line in m.group(1).splitlines():
    s = line.strip()
    if not s or s.startswith("#") or s.startswith("["):
        continue
    if "=" in s:
        want.append(re.sub(r"\s+", " ", s))
ref = (root / "infra/config/floatctf.prod.toml").read_text()
have = {re.sub(r"\s+", " ", l.strip()) for l in ref.splitlines() if l.strip() and not l.strip().startswith("#")}
missing = [w for w in want if w not in have]
if missing:
    print("参考 TOML 缺少/值不同：")
    for w in missing:
        print("  -", w)
    raise SystemExit(1)
PYEOF

# ── 结果 ─────────────────────────────────────────────────────────────────────
echo ""
printf '通过 %d 项，失败 %d 项\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
