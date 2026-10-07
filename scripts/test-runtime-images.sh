#!/usr/bin/env bash
#
# FloatCTF 运行时镜像分发契约测试（B1 / R3 配套）。
#
# 为什么不跑 CI 验证：运行时镜像的失误与 Release 一样**不可回滚**（从分支/PR
# 误推镜像、RC 误登录 registry、默认就推送）。这里做静态 + 行为断言：
#   1. release.yml：镜像构建 + registry login + push 只存在于 tag-guarded 的
#      runtime-images job；packages: write 只在那里；三个 canonical ref 与 OCI
#      label 名齐全；pull_request / 分支 / dispatch 路径绝无 push。
#   2. rc.yml：允许本地构建/体检镜像，但绝无 docker login / docker push /
#      packages / action-gh-release。
#   3. scripts/build-runtime-images.sh：--help 列出 canonical refs；默认**不推送**；
#      用 stub docker 记录调用，证明无 --push 时不会出现任何 `push`，而带 --push
#      时会出现（负向断言必须真的可能转正，否则它证明不了任何东西）。
#   4. bash -n 语法门禁。
#
# 全部离线：不联网、不推送、不构建真实镜像（stub docker 只记录调用）。
#
# 用法：bash scripts/test-runtime-images.sh
# 退出码：0 = 全部通过；1 = 有失败项。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_YML="$ROOT/.github/workflows/release.yml"
RC_YML="$ROOT/.github/workflows/rc.yml"
BUILD_SH="$ROOT/scripts/build-runtime-images.sh"
INSTALL_SH="$ROOT/scripts/install.sh"

PASS=0
FAIL=0
pass() { printf '✓ %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf '✗ %s\n' "$1"; FAIL=$((FAIL + 1)); }
check() { # <描述> <命令...>
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then pass "$desc"; else fail "$desc"; fi
}

TMP_TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-runtime-images.XXXXXX")"
cleanup() { rm -rf "$TMP_TEST_DIR"; }
trap cleanup EXIT

# ── helpers ──────────────────────────────────────────────────────────────────
strip_comments() { sed 's/#.*$//' "$1"; }

yaml_job_block() { # <file> <job-name>
    awk -v job="$2" '
        $0 == "  " job ":" { inb = 1; print; next }
        inb && /^  [^[:space:]]/ { exit }
        inb { print }
    ' "$1"
}

yaml_top_block() { # <file> <top-level-key>
    awk -v key="$2" '
        index($0, key ":") == 1 { inb = 1; print; next }
        inb && /^[^[:space:]#]/ { exit }
        inb { print }
    ' "$1"
}

grep_count() { grep -cE -- "$2" "$1" 2>/dev/null || true; }

for f in "$RELEASE_YML" "$RC_YML" "$BUILD_SH" "$INSTALL_SH"; do
    [ -f "$f" ] || { printf '[FAIL] 缺少必需文件：%s\n' "$f" >&2; exit 1; }
done

strip_comments "$RELEASE_YML" >"$TMP_TEST_DIR/release.nocomment.yml"
strip_comments "$RC_YML" >"$TMP_TEST_DIR/rc.nocomment.yml"
yaml_job_block "$RELEASE_YML" build >"$TMP_TEST_DIR/release-build.txt"
yaml_job_block "$RELEASE_YML" publish >"$TMP_TEST_DIR/release-publish.txt"
yaml_job_block "$RELEASE_YML" runtime-images >"$TMP_TEST_DIR/release-runtime.txt"
yaml_top_block "$RELEASE_YML" on >"$TMP_TEST_DIR/release-on.txt"
yaml_top_block "$RC_YML" on >"$TMP_TEST_DIR/rc-on.txt"

# ── 1. release.yml：镜像构建只在 tag-guarded 的 runtime-images job ───────────
echo "== 1. release.yml 运行时镜像 job =="

if [ -s "$TMP_TEST_DIR/release-runtime.txt" ]; then
    pass "release.yml 存在 runtime-images job"
else
    fail "release.yml 存在 runtime-images job"
fi

check "runtime-images 调用唯一构建契约 scripts/build-runtime-images.sh" \
    grep -q -F 'scripts/build-runtime-images.sh' "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images 使用 canonical registry 前缀 ghcr.io/floatctf" \
    grep -q -F -- '--registry ghcr.io/floatctf' "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images tag 来自 guard 输出的版本（needs.build.outputs.version）" \
    grep -q -F 'needs.build.outputs.version' "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images 显式 --push（推送是显式动作，不是默认）" \
    grep -q -E -- '^[[:space:]]+--push[[:space:]]*$' "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images needs: build" \
    grep -q -E 'needs:[[:space:]]*build' "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images job if: 要求 publish == 'true'" \
    grep -q -F "needs.build.outputs.publish == 'true'" "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images job if: 要求 github.ref_type == 'tag'" \
    grep -q -F "github.ref_type == 'tag'" "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images job if: 要求 startsWith(github.ref_name, 'v')" \
    grep -q -F "startsWith(github.ref_name, 'v')" "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images 有纵深防御的 tag 复验步骤" \
    grep -q -F 'Re-assert tag publish guard' "$TMP_TEST_DIR/release-runtime.txt"

# ── 2. registry login / push 唯一性 ──────────────────────────────────────────
echo "== 2. login / push 只可能在 tag-guarded 推送路径 =="

login_count="$(grep_count "$TMP_TEST_DIR/release.nocomment.yml" 'docker[[:space:]]+login')"
if [ "$login_count" = "1" ]; then
    pass "release.yml 全文件恰好 1 处 docker login"
else
    fail "release.yml docker login 出现 $login_count 次（期望恰好 1 次）"
fi
check "docker login 位于 runtime-images job" \
    grep -q -E 'docker[[:space:]]+login' "$TMP_TEST_DIR/release-runtime.txt"

# build（dispatch/branch/PR 都会走）与 publish job 绝不允许出现推送/登录/镜像推送参数。
for block in build publish; do
    check "$block job 不含 docker login" \
        bash -c '! grep -qE "docker[[:space:]]+login" "$1"' _ "$TMP_TEST_DIR/release-$block.txt"
    check "$block job 不含 --push" \
        bash -c '! grep -qE "(^|[[:space:]])--push([[:space:]]|$)" "$1"' _ "$TMP_TEST_DIR/release-$block.txt"
    check "$block job 不含 build-runtime-images.sh" \
        bash -c '! grep -qF "build-runtime-images.sh" "$1"' _ "$TMP_TEST_DIR/release-$block.txt"
done
# 全文件没有裸 `docker push`（推送由 build-runtime-images.sh --push 负责，
# 脚本内部受 PUSH 守门；release.yml 里出现裸 push 说明有人绕过了契约）。
check "release.yml 不含裸 docker push" \
    bash -c '! grep -qE "docker[[:space:]]+push" "$1"' _ "$TMP_TEST_DIR/release.nocomment.yml"

# ── 3. packages: write 只在推送 job ─────────────────────────────────────────
echo "== 3. packages: write 权限最小化 =="

pkg_count="$(grep_count "$TMP_TEST_DIR/release.nocomment.yml" 'packages:[[:space:]]*write')"
if [ "$pkg_count" = "1" ]; then
    pass "release.yml 全文件恰好 1 处 packages: write"
else
    fail "release.yml packages: write 出现 $pkg_count 次（期望恰好 1 次）"
fi
check "packages: write 在 runtime-images job" \
    grep -q -E 'packages:[[:space:]]*write' "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images 同时保持 contents: read" \
    grep -q -E 'contents:[[:space:]]*read' "$TMP_TEST_DIR/release-runtime.txt"
for block in build publish; do
    check "$block job 不含 packages: write" \
        bash -c '! grep -qE "packages:" "$1"' _ "$TMP_TEST_DIR/release-$block.txt"
done

# ── 4. 触发面：PR / 分支 / dispatch 不可能推送 ──────────────────────────────
echo "== 4. 触发面（PR/branch/dispatch 无推送可能）=="

check "release.yml 不监听 pull_request（无 PR 触发面）" \
    bash -c '! grep -qE "^[[:space:]]+pull_request" "$1"' _ "$TMP_TEST_DIR/release-on.txt"
check "release.yml 仅在 push tags v* 上发布（触发块声明 tags v*）" \
    grep -q -E '^[[:space:]]+tags:[[:space:]]*\[[[:space:]]*"v\*"[[:space:]]*\]' "$TMP_TEST_DIR/release-on.txt"
check "workflow_dispatch 输入 publish 默认 false" \
    bash -c 'grep -A6 -E "^[[:space:]]+publish:" "$1" | grep -qE "default:[[:space:]]*false"' _ "$TMP_TEST_DIR/release-on.txt"
# 任何一步想 push 都必须穿过 job 级 if（publish==true && tag && v*）——这是唯一的门。
check "runtime-images job if 同时要求 publish/tag/v* 三个条件" \
    bash -c 'awk "/^    if: >-/{f=1} f{print} f&&/^    runs-on:/{exit}" "$1" |
             grep -qF "needs.build.outputs.publish == '"'"'true'"'"'" &&
             awk "/^    if: >-/{f=1} f{print} f&&/^    runs-on:/{exit}" "$1" |
             grep -qF "github.ref_type == '"'"'tag'"'"'" &&
             awk "/^    if: >-/{f=1} f{print} f&&/^    runs-on:/{exit}" "$1" |
             grep -qF "startsWith(github.ref_name, '"'"'v'"'"')"' _ "$RELEASE_YML"

# ── 5. canonical 镜像名 + OCI labels ────────────────────────────────────────
echo "== 5. canonical refs 与 OCI labels =="

for name in awd-flagserver awd-judgeserver awdp-judgeserver; do
    check "build-runtime-images.sh 含 canonical 名 $name（registry 下扁平化）" \
        grep -q -F "$name" "$BUILD_SH"
done
check "build-runtime-images.sh 的 awdp 在 registry 下扁平化为 awdp-judgeserver（无 infra/ 段）" \
    grep -q -F '%s/awdp-judgeserver' "$BUILD_SH"
check "build-runtime-images.sh 无 registry 时保留历史本地名 floatctf/infra/awdp-judgeserver" \
    grep -q -F 'floatctf/infra/awdp-judgeserver' "$BUILD_SH"

for label in org.opencontainers.image.source org.opencontainers.image.version \
             org.opencontainers.image.revision org.opencontainers.image.created; do
    check "release.yml 设置 OCI label $label" \
        grep -q -F "$label" "$TMP_TEST_DIR/release-runtime.txt"
    check "build-runtime-images.sh 支持 OCI label $label" \
        grep -q -F "$label" "$BUILD_SH"
done
check "runtime-images 的 label 值全部来自 env（无硬编码版本/revision）" \
    bash -c 'grep -F -- "--label" "$1" | grep -vF "secrets." | grep -qF "FLOATCTF_BUILD_" &&
             ! grep -F -- "--label" "$1" | grep -qF "secrets."' _ "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images 的 label 不含 secrets.*" \
    bash -c '! grep -F -- "--label" "$1" | grep -qF "secrets."' _ "$TMP_TEST_DIR/release-runtime.txt"
check "runtime-images 从 workflow 上下文取 source/version/revision" \
    bash -c 'grep -qF "github.repository" "$1" && grep -qF "github.sha" "$1" &&
             grep -qF "runtime_meta.outputs.created" "$1"' _ "$TMP_TEST_DIR/release-runtime.txt"

# ── 6. rc.yml 非发布：可构建/体检，绝不登录/推送 ────────────────────────────
echo "== 6. rc.yml（RC 非发布契约）=="

check "rc.yml 调用 scripts/build-runtime-images.sh（本地构建 + ldd 体检）" \
    grep -q -F 'scripts/build-runtime-images.sh' "$RC_YML"
check "rc.yml 不带 --registry（本地命名，不指向 registry）" \
    bash -c '! grep -qE "(^|[[:space:]])--registry([[:space:]]|$)" "$1"' _ "$RC_YML"
check "rc.yml 不带 --push" \
    bash -c '! grep -qE "(^|[[:space:]])--push([[:space:]]|$)" "$1"' _ "$RC_YML"
check "rc.yml 不含 docker login" \
    bash -c '! grep -qE "docker[[:space:]]+login" "$1"' _ "$TMP_TEST_DIR/rc.nocomment.yml"
check "rc.yml 不含 docker push" \
    bash -c '! grep -qE "docker[[:space:]]+push" "$1"' _ "$TMP_TEST_DIR/rc.nocomment.yml"
check "rc.yml 不含 packages 权限（绝不申请写包权限）" \
    bash -c '! grep -qE "packages:" "$1"' _ "$TMP_TEST_DIR/rc.nocomment.yml"
check "rc.yml 不含 action-gh-release" \
    bash -c '! grep -qF "action-gh-release" "$1"' _ "$TMP_TEST_DIR/rc.nocomment.yml"
check "rc.yml 不含 contents: write" \
    bash -c '! grep -qE "contents:[[:space:]]*write" "$1"' _ "$RC_YML"
check "rc.yml 由 workflow_dispatch 触发（dispatch-only）" \
    bash -c 'grep -qE "^[[:space:]]+workflow_dispatch:" "$1" && ! grep -qE "^[[:space:]]+push:" "$1"' _ "$TMP_TEST_DIR/rc-on.txt"

# ── 7. build-runtime-images.sh --help：canonical refs ───────────────────────
echo "== 7. build-runtime-images.sh --help =="

BUILD_HELP_RC=0
BUILD_HELP="$(bash "$BUILD_SH" --help 2>&1)" || BUILD_HELP_RC=$?
if [ "$BUILD_HELP_RC" -eq 0 ]; then
    pass "build-runtime-images.sh --help 退出 0"
else
    fail "build-runtime-images.sh --help 退出 0（实际 rc=$BUILD_HELP_RC）"
fi
for ref in ghcr.io/floatctf/awd-flagserver:1.0.0 \
           ghcr.io/floatctf/awd-judgeserver:1.0.0 \
           ghcr.io/floatctf/awdp-judgeserver:1.0.0; do
    check "--help 列出 canonical ref $ref" \
        bash -c 'grep -qF -- "$2" <<<"$1"' _ "$BUILD_HELP" "$ref"
done
# 动态解析：--registry/--tag 放在 --help 之前必须反映在"当前解析"清单里。
BUILD_HELP2="$(bash "$BUILD_SH" --registry ghcr.io/floatctf --tag 1.0.0 --help 2>&1 || true)"
check "--help 动态反映 --registry/--tag（refs 与实际构建一致，不会文档漂移）" \
    bash -c 'for r in ghcr.io/floatctf/awd-flagserver:1.0.0 ghcr.io/floatctf/awd-judgeserver:1.0.0 ghcr.io/floatctf/awdp-judgeserver:1.0.0; do grep -qF -- "$r" <<<"$1" || exit 1; done' _ "$BUILD_HELP2"
check "--help 文档化 awdp 扁平化 + 无 registry 的历史名" \
    bash -c 'grep -qF "扁平化" <<<"$1" && grep -qF "floatctf/infra/awdp-judgeserver" <<<"$1"' _ "$BUILD_HELP"
check "--help 文档化 --push 默认关闭且不推 :latest" \
    bash -c 'grep -qF "默认**不推送**" <<<"$1" && grep -qF "绝不推 :latest" <<<"$1"' _ "$BUILD_HELP"
# label 安全：疑似密钥的 key 在**参数解析期**就被拒绝（放在 --help 之前也必须先失败，
# 因此这个断言不会真的触发构建）。
check "build-runtime-images.sh 拒绝疑似密钥的 --label key（parse 期 fail closed）" \
    bash -c '! bash "$1" --label org.example.token=x --help >/dev/null 2>&1' _ "$BUILD_SH"
check "build-runtime-images.sh 拒绝非 k=v 的 --label" \
    bash -c '! bash "$1" --label justakey --help >/dev/null 2>&1' _ "$BUILD_SH"

# ── 8. stub-docker 行为测试：默认绝不 push ──────────────────────────────────
echo "== 8. stub-docker：默认不推送（负向断言可转正）=="

STUB_BIN="$TMP_TEST_DIR/stubbin"
mkdir -p "$STUB_BIN"
cat > "$STUB_BIN/docker" <<'STUB'
#!/usr/bin/env bash
# 记录调用；模拟 cargo build 产出二进制；不做任何真实 docker 操作。
printf 'docker %s\n' "$*" >>"${DOCKER_STUB_LOG:?}"
target=""
for a in "$@"; do
    case "$a" in
        *:/target) target="${a%:/target}" ;;
    esac
done
if [ -n "$target" ] && [[ " $* " == *" cargo build "* ]]; then
    mkdir -p "$target/release"
    for b in awd_flagserver awd_judgeserver awdp_judgeserver; do
        printf '#!/bin/sh\nexit 0\n' >"$target/release/$b"
        chmod 0755 "$target/release/$b"
    done
fi
exit 0
STUB
chmod +x "$STUB_BIN/docker"

run_stub() { # <log-name> <script-args...>
    local log="$TMP_TEST_DIR/$1"
    shift
    : >"$log"
    DOCKER_STUB_LOG="$log" PATH="$STUB_BIN:$PATH" \
        bash "$BUILD_SH" "$@" >"$log.out" 2>&1
    return $?
}

if run_stub default.log --registry ghcr.io/floatctf --tag 1.0.0; then
    pass "stub-docker：--registry/--tag（无 --push）执行成功"
else
    fail "stub-docker：--registry/--tag（无 --push）执行失败"
    sed 's/^/      /' "$TMP_TEST_DIR/default.log.out" | tail -5
fi
check "stub-docker：默认调用里没有任何 push（--push 不是默认）" \
    bash -c '! grep -qE "docker push|push ghcr" "$1"' _ "$TMP_TEST_DIR/default.log"
check "stub-docker：默认确实构建了三个 canonical ref（证明日志捕获有效）" \
    bash -c 'for r in ghcr.io/floatctf/awd-flagserver:1.0.0 ghcr.io/floatctf/awd-judgeserver:1.0.0 ghcr.io/floatctf/awdp-judgeserver:1.0.0; do grep -qF -- "$r" "$1" || exit 1; done' _ "$TMP_TEST_DIR/default.log"
check "stub-docker：默认不执行 docker login（脚本永不 login）" \
    bash -c '! grep -qE "docker login" "$1"' _ "$TMP_TEST_DIR/default.log"

# 正向前置：同样的 stub + --push 必须真的出现 push —— 否则上面的负向断言
# 只是因为 harness 抓不到 push 而"假通过"。
if run_stub push.log --registry ghcr.io/floatctf --tag 1.0.0 --push; then
    pass "stub-docker：--push 执行成功"
else
    fail "stub-docker：--push 执行失败"
    sed 's/^/      /' "$TMP_TEST_DIR/push.log.out" | tail -5
fi
check "stub-docker：--push 时三个版本 tag 都被推送" \
    bash -c 'for r in ghcr.io/floatctf/awd-flagserver:1.0.0 ghcr.io/floatctf/awd-judgeserver:1.0.0 ghcr.io/floatctf/awdp-judgeserver:1.0.0; do grep -qF -- "docker push $r" "$1" || exit 1; done' _ "$TMP_TEST_DIR/push.log"
check "stub-docker：--push 绝不推送 :latest（本地兼容 tag）" \
    bash -c '! grep -qE "docker push .*:latest" "$1"' _ "$TMP_TEST_DIR/push.log"
# 没有 registry 时不允许推送（本地 floatctf/... 命名不推往任何 registry）。
if DOCKER_STUB_LOG="$TMP_TEST_DIR/noreg.log" PATH="$STUB_BIN:$PATH" \
    bash "$BUILD_SH" --tag 1.0.0 --push >"$TMP_TEST_DIR/noreg.log.out" 2>&1; then
    fail "stub-docker：无 --registry 的 --push 应被拒绝"
else
    pass "stub-docker：无 --registry 的 --push 被拒绝（fail closed）"
fi

# ── 9. 语法门禁 ─────────────────────────────────────────────────────────────
echo "== 9. bash -n 语法门禁 =="

for s in "$INSTALL_SH" "$BUILD_SH" "$ROOT/scripts/test-installer-contract.sh" \
         "$ROOT/scripts/test-runtime-images.sh" "$ROOT/scripts/test-release-workflow.sh" \
         "$ROOT/scripts/release-checksums.sh"; do
    check "bash -n ${s#"$ROOT"/}" bash -n "$s"
done

# ── 结果 ─────────────────────────────────────────────────────────────────────
echo ""
printf '通过 %d 项，失败 %d 项\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
