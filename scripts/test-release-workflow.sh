#!/usr/bin/env bash
#
# release.yml / rc.yml 的发布契约与安全负向测试（v1.0 发布流程）。
#
# 为什么不靠"跑一遍 CI"来验证：发布工作流的失误是**不可回滚**的（误建 Release、
# 从分支 ref 发布、缺 install.sh/SHA256SUMS 的残缺 release）。这里做**静态断言**，
# 把契约钉死在文本层面，任何人在 YAML 里把守卫改坏都会立刻变红。
#
# 覆盖（对应 ARTIFACT CONTRACT v1.0）：
#   1. release.yml 只在 push tag v* / workflow_dispatch 上触发
#   2. dispacth 的第一道守卫：publish=true 必须落在 v* tag ref 上，否则 fail closed
#   3. 发布清单包含契约里**每一个**制品（含 install.sh / ops-tools.tar.gz / SHA256SUMS / 三个 tgz）
#   4. 发布动作只存在于带 tag 守卫的 publish job；build/dispatch 路径永远碰不到 action-gh-release
#   5. publish 输入默认 false；权限最小化（默认 read，只有 publish job 是 write）
#   6. rc.yml 非发布：dispatch-only、只 upload-artifact、无 action-gh-release、无 npm/pnpm publish
#   7. scripts/release-checksums.sh 可执行行为（--help / --checksums / --ops-tools）
#   8. bash -n 语法门禁
#
# 全部离线：不联网、不 docker、不跑 pnpm pack、不写仓库树（只用 mktemp 临时目录）。
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_YML="$ROOT/.github/workflows/release.yml"
RC_YML="$ROOT/.github/workflows/rc.yml"
CHECKSUMS_SH="$ROOT/scripts/release-checksums.sh"
INSTALL_SH="$ROOT/scripts/install.sh"

PASS=0
FAIL=0
note() { printf '  %s\n' "$*"; }
pass() { PASS=$((PASS + 1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
fail() { FAIL=$((FAIL + 1)); printf '  \033[31m✗\033[0m %s\n' "$1"; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/fcft-release-test.XXXXXX")"
cleanup() { rm -rf -- "$WORK"; }
trap cleanup EXIT

# ── 极简 YAML 结构切片（只认缩进，不引入 YAML 依赖）─────────────────────────

# 顶层块：从 `key:` 起，到下一个顶层键为止。
yaml_top_block() { # <file> <top-level-key>
    awk -v key="$2" '
        index($0, key ":") == 1 { inb = 1; print; next }
        inb && /^[^[:space:]#]/ { exit }
        inb { print }
    ' "$1"
}

# job 块：从 `  <job>:` 起，到下一个两空格缩进的键为止。
yaml_job_block() { # <file> <job-name>
    awk -v job="$2" '
        $0 == "  " job ":" { inb = 1; print; next }
        inb && /^  [^[:space:]]/ { exit }
        inb { print }
    ' "$1"
}

# 缩进子块：匹配 <parent-regex> 的那一行之后、缩进更深的所有行。
yaml_sub_block() { # <file> <parent-ERE>
    awk -v pat="$2" '
        !inb && $0 ~ pat { inb = 1; base = match($0, /[^ ]/) - 1; next }
        inb {
            if ($0 ~ /^[[:space:]]*$/) { print ""; next }
            cur = match($0, /[^ ]/) - 1
            if (cur <= base) exit
            print
        }
    ' "$1"
}

# 某个 step（按 id）的 `run: |` 脚本正文。
yaml_step_run() { # <file> <step-id>
    awk -v id="$2" '
        $0 ~ ("^[[:space:]]*id:[[:space:]]*" id "[[:space:]]*$") { seen = 1 }
        seen && /^[[:space:]]*run:[[:space:]]*\|/ {
            inrun = 1; base = match($0, /[^ ]/) - 1; next
        }
        inrun {
            if ($0 ~ /^[[:space:]]*$/) { print ""; next }
            cur = match($0, /[^ ]/) - 1
            if (cur <= base) exit
            print
        }
    ' "$1"
}

# YAML 注释剥离（只用于"禁止出现某命令"的负向断言，避免被注释里的词误伤）。
strip_comments() { sed 's/#.*$//' "$1"; }

grep_in() { # <label> <file> <ERE>
    local label="$1" file="$2" pat="$3"
    if [ ! -f "$file" ]; then
        fail "$label（文件不存在：$file）"
        return 0
    fi
    if grep -qE -- "$pat" "$file"; then pass "$label"; else fail "$label（未匹配 /$pat/）"; fi
}

grep_not_in() { # <label> <file> <ERE>
    local label="$1" file="$2" pat="$3"
    if [ ! -f "$file" ]; then
        fail "$label（文件不存在：$file）"
        return 0
    fi
    if grep -qE -- "$pat" "$file"; then
        fail "$label（不应出现 /$pat/）"
    else
        pass "$label"
    fi
}

# ── 前置：文件必须存在 ───────────────────────────────────────────────────────
for f in "$RELEASE_YML" "$RC_YML" "$CHECKSUMS_SH" "$INSTALL_SH"; do
    [ -f "$f" ] || { printf '[FAIL] 缺少必需文件：%s\n' "$f" >&2; exit 1; }
done

yaml_top_block "$RELEASE_YML" on >"$WORK/release-on.txt"
yaml_job_block "$RELEASE_YML" build >"$WORK/release-build.txt"
yaml_job_block "$RELEASE_YML" publish >"$WORK/release-publish.txt"
yaml_top_block "$RELEASE_YML" permissions >"$WORK/release-permissions.txt"
yaml_sub_block "$WORK/release-publish.txt" '^[[:space:]]*files:[[:space:]]*[|]' >"$WORK/release-files.txt"
yaml_step_run "$RELEASE_YML" guard >"$WORK/release-guard-run.txt"
yaml_top_block "$RC_YML" on >"$WORK/rc-on.txt"
yaml_top_block "$RC_YML" permissions >"$WORK/rc-permissions.txt"
strip_comments "$RELEASE_YML" >"$WORK/release.nocomment.yml"
strip_comments "$RC_YML" >"$WORK/rc.nocomment.yml"

echo "== 1. release.yml 触发语义与守卫 =="

grep_in "release.yml 触发 push tags v*" "$WORK/release-on.txt" '^[[:space:]]+tags:[[:space:]]*\[[[:space:]]*"v\*"[[:space:]]*\]'
grep_in "release.yml 声明 push 事件" "$WORK/release-on.txt" '^[[:space:]]+push:'
grep_in "release.yml 支持 workflow_dispatch" "$WORK/release-on.txt" '^[[:space:]]+workflow_dispatch:'

if [ ! -s "$WORK/release-guard-run.txt" ]; then
    fail "guard 步骤（id: guard）存在且含 run 脚本"
else
    pass "guard 步骤（id: guard）存在且含 run 脚本"
fi

grep_in "guard 计算 is_tag" "$WORK/release-guard-run.txt" 'is_tag='
grep_in "guard 计算 publish/version 输出" "$WORK/release-guard-run.txt" 'GITHUB_OUTPUT'
grep_in "guard 对 publish=true 非 tag ref fail closed（exit 1）" "$WORK/release-guard-run.txt" 'exit 1'
grep_in "guard 硬性要求 v* tag（is_tag != true 即拒绝）" "$WORK/release-guard-run.txt" '"\$is_tag" != "true"'
grep_in "guard 无法确定 VERSION 时 fail closed" "$WORK/release-guard-run.txt" '无法确定平台版本 VERSION'
grep_in "guard 拒绝 ci/latest 占位名" "$WORK/release-guard-run.txt" '拒绝以 ci/latest'

guard_line="$(grep -n 'id: guard' "$RELEASE_YML" | head -1 | cut -d: -f1)"
checkout_line="$(grep -n 'uses: actions/checkout@v4' "$RELEASE_YML" | head -1 | cut -d: -f1)"
if [ -n "$guard_line" ] && [ -n "$checkout_line" ] && [ "$guard_line" -lt "$checkout_line" ]; then
    pass "guard 是 build job 的第一步（先于 checkout，fail fast）"
else
    fail "guard 不是 build job 的第一步（guard@${guard_line:-?} checkout@${checkout_line:-?}）"
fi

grep_in "build job 导出 publish/version outputs" "$WORK/release-build.txt" 'publish: \$\{\{ steps\.guard\.outputs\.publish \}\}'
grep_in "build job 导出 version output" "$WORK/release-build.txt" 'version: \$\{\{ steps\.guard\.outputs\.version \}\}'

echo "== 2. publish 输入默认 false + 权限最小化 =="

# 只在确实声明了 publish 输入时断言其默认值（本仓实现是有的）。
if grep -qE '^[[:space:]]+publish:' "$WORK/release-on.txt"; then
    publish_block="$(yaml_sub_block "$WORK/release-on.txt" '^[[:space:]]+publish:')"
    if grep -qE '^[[:space:]]*default:[[:space:]]*false[[:space:]]*$' <<<"$publish_block"; then
        pass "publish 输入存在且 default: false"
    else
        fail "publish 输入存在但 default 不是 false"
    fi
else
    fail "release.yml 未声明 publish 输入（本仓实现要求显式输入，默认 false）"
fi

grep_in "workflow 默认权限 contents: read" "$WORK/release-permissions.txt" 'contents:[[:space:]]*read'
grep_not_in "workflow 默认权限不含 contents: write" "$WORK/release-permissions.txt" 'contents:[[:space:]]*write'
grep_in "build job 显式 contents: read" "$WORK/release-build.txt" 'contents:[[:space:]]*read'
grep_not_in "build job 不含 contents: write" "$WORK/release-build.txt" 'contents:[[:space:]]*write'
grep_in "publish job 才有 contents: write" "$WORK/release-publish.txt" 'contents:[[:space:]]*write'
grep_in "publish job needs: build" "$WORK/release-publish.txt" 'needs:[[:space:]]*build'
grep_in "publish job 带 needs.build.outputs.publish 条件" "$WORK/release-publish.txt" "needs\.build\.outputs\.publish == 'true'"

echo "== 3. 发布动作只在带 tag 守卫的 publish job 内 =="

if [ ! -s "$WORK/release-publish.txt" ]; then
    fail "release.yml 存在 publish job（独立 job + tag 条件）"
else
    pass "release.yml 存在独立 publish job"
fi
grep_in "publish job 使用 softprops/action-gh-release" "$WORK/release-publish.txt" 'uses:[[:space:]]*softprops/action-gh-release@v2'
grep_not_in "build/dispatch 路径不含 action-gh-release" "$WORK/release-build.txt" 'softprops/action-gh-release'
grep_in "release 步骤自身 if: 要求 ref_type == tag" "$WORK/release-publish.txt" "github\.ref_type == 'tag'"
grep_in "release 步骤自身 if: 要求 v* tag ref_name" "$WORK/release-publish.txt" "startsWith\(github\.ref_name, 'v'\)"
grep_in "release 步骤自身 if: 要求 publish == true" "$WORK/release-publish.txt" "needs\.build\.outputs\.publish == 'true'"
grep_in "publish job 有纵深防御的 tag 复验步骤" "$WORK/release-publish.txt" 'Re-assert tag guard'
grep_in "release 列表 fail_on_unmatched_files: true" "$WORK/release-publish.txt" 'fail_on_unmatched_files:[[:space:]]*true'

gh_release_count="$(grep -c 'softprops/action-gh-release' "$WORK/release.nocomment.yml" || true)"
if [ "$gh_release_count" = "1" ]; then
    pass "全文件只出现 1 处 action-gh-release（无第二个发布出口）"
else
    fail "action-gh-release 出现 $gh_release_count 次（期望恰好 1 次）"
fi

echo "== 4. ARTIFACT CONTRACT v1.0：发布清单逐项覆盖 =="

if [ ! -s "$WORK/release-files.txt" ]; then
    fail "release.yml 的软发布 files: 清单非空"
else
    pass "release.yml 的发布 files: 清单非空"
fi

check_artifact() { # <label> <ERE>
    local label="$1" pat="$2"
    if grep -qE -- "$pat" "$WORK/release-files.txt"; then
        pass "制品在发布清单中：$label"
    else
        fail "制品缺失于发布清单：$label（未匹配 /$pat/）"
    fi
}

check_artifact_fixed() { # <label> <literal fragment>
    local label="$1" needle="$2"
    if grep -qF -- "$needle" "$WORK/release-files.txt"; then
        pass "制品在发布清单中：$label"
    else
        fail "制品缺失于发布清单：$label（未找到字面量 $needle）"
    fi
}

check_artifact "floatctf" "(^|/)floatctf[[:space:]]*$"
check_artifact "floatctf-helper" "(^|/)floatctf-helper[[:space:]]*$"
check_artifact "web-dist.tar.gz" "(^|/)web-dist\.tar\.gz[[:space:]]*$"
check_artifact "merged.sql" "(^|/)merged\.sql[[:space:]]*$"
check_artifact "frontend.sh" "(^|/)frontend\.sh[[:space:]]*$"
check_artifact "install.sh" "(^|/)install\.sh[[:space:]]*$"
check_artifact "ops-tools.tar.gz" "(^|/)ops-tools\.tar\.gz[[:space:]]*$"
# 三个 tgz 的名字里含 `${{ needs.build.outputs.version }}`（内部有空格），用字面量匹配，
# 顺带断言版本号确实来自 guard 解析出的平台版本，而不是硬编码。
check_artifact_fixed "floatctf-sdk-<V>.tgz" 'floatctf-sdk-${{ needs.build.outputs.version }}.tgz'
check_artifact_fixed "floatctf-react-<V>.tgz" 'floatctf-react-${{ needs.build.outputs.version }}.tgz'
check_artifact_fixed "floatctf-frontend-runtime-<V>.tgz" 'floatctf-frontend-runtime-${{ needs.build.outputs.version }}.tgz'
check_artifact "SHA256SUMS" "(^|/)SHA256SUMS[[:space:]]*$"

echo "== 5. SHA256SUMS 生成 + 版本 fail-closed 语义 =="

grep_in "release.yml 调用 scripts/release-checksums.sh" "$WORK/release-build.txt" 'release-checksums\.sh'
grep_in "release.yml 使用 --assemble（内部生成 SHA256SUMS）" "$WORK/release-build.txt" '--assemble'
grep_in "release-checksums.sh 使用 sha256sum" "$CHECKSUMS_SH" 'sha256sum'
grep_in "release-checksums.sh 明确排除 SHA256SUMS 自身" "$CHECKSUMS_SH" 'SHA256SUMS 自身'
grep_in "release-checksums.sh 版本为空时 fail closed" "$CHECKSUMS_SH" '拒绝以 ci/latest'
grep_in "release-checksums.sh 用 LC_ALL=C sort 保证确定性顺序" "$CHECKSUMS_SH" 'LC_ALL=C sort'

echo "== 6. rc.yml 非发布契约 =="

grep_in "rc.yml 由 workflow_dispatch 触发" "$WORK/rc-on.txt" '^[[:space:]]+workflow_dispatch:'
grep_not_in "rc.yml 不监听 push（绝不自动产出发布物）" "$WORK/rc-on.txt" '^[[:space:]]+push:'
grep_not_in "rc.yml 不监听 tag" "$WORK/rc-on.txt" '^[[:space:]]+tags:'
grep_in "rc.yml 使用 actions/upload-artifact" "$RC_YML" 'uses:[[:space:]]*actions/upload-artifact@v4'
grep_in "rc.yml 权限 contents: read" "$WORK/rc-permissions.txt" 'contents:[[:space:]]*read'
grep_not_in "rc.yml 不含 contents: write" "$RC_YML" 'contents:[[:space:]]*write'
grep_not_in "rc.yml 不含 action-gh-release" "$WORK/rc.nocomment.yml" 'action-gh-release'
grep_not_in "rc.yml 不含 npm publish" "$WORK/rc.nocomment.yml" 'npm[[:space:]]+publish'
grep_not_in "rc.yml 不含 pnpm publish" "$WORK/rc.nocomment.yml" 'pnpm[[:space:]]+publish'
grep_in "rc.yml 运行 verify-release-frontend.sh" "$RC_YML" 'scripts/verify-release-frontend\.sh'
grep_in "rc.yml 运行 test-frontend-manager.sh" "$RC_YML" 'scripts/test-frontend-manager\.sh'
grep_in "rc.yml 运行 check-architecture.sh" "$RC_YML" 'scripts/check-architecture\.sh'
grep_in "rc.yml 复用 release-checksums.sh --assemble" "$RC_YML" 'release-checksums\.sh --assemble'
grep_in "rc.yml 版本无法确定时 fail closed" "$RC_YML" '拒绝使用 ci/latest'

echo "== 7. scripts/release-checksums.sh 行为 =="

if bash "$CHECKSUMS_SH" --help >"$WORK/help.txt" 2>&1; then
    pass "release-checksums.sh --help 退出 0"
else
    fail "release-checksums.sh --help 未退出 0"
fi
grep_in "--help 文档覆盖 --ops-tools" "$WORK/help.txt" '--ops-tools'
grep_in "--help 文档覆盖 --checksums" "$WORK/help.txt" '--checksums'

if bash "$CHECKSUMS_SH" >"$WORK/noargs.txt" 2>&1; then
    fail "无模式调用应当失败（fail closed）"
else
    pass "无模式调用失败（fail closed）"
fi

# --checksums：3 个文件 + SHA256SUMS 自身，按名字排序，可被 sha256sum -c 校验。
CK="$WORK/checksums"
mkdir -p "$CK"
printf 'alpha\n' >"$CK/b.bin"
printf 'beta\n' >"$CK/a.bin"
printf 'gamma\n' >"$CK/c.bin"
if bash "$CHECKSUMS_SH" --checksums "$CK" >"$WORK/ck.log" 2>&1; then
    pass "--checksums <dir> 退出 0"
else
    fail "--checksums <dir> 未退出 0"
    sed 's/^/      /' "$WORK/ck.log" | head -5
fi
if [ -f "$CK/SHA256SUMS" ]; then
    pass "--checksums <dir> 在目录内写出 SHA256SUMS"
    line_count="$(grep -c . "$CK/SHA256SUMS" || true)"
    if [ "$line_count" = "3" ]; then
        pass "SHA256SUMS 覆盖 3 个文件（不含自身）"
    else
        fail "SHA256SUMS 行数异常：$line_count（期望 3）"
    fi
    if grep -q 'SHA256SUMS' "$CK/SHA256SUMS"; then
        fail "SHA256SUMS 不应包含自身"
    else
        pass "SHA256SUMS 不含自身"
    fi
    if [ "$(cut -d' ' -f3- "$CK/SHA256SUMS" | tr '\n' ' ')" = "a.bin b.bin c.bin " ]; then
        pass "SHA256SUMS 按名字排序（a.bin b.bin c.bin）"
    else
        fail "SHA256SUMS 顺序异常：$(cut -d' ' -f3- "$CK/SHA256SUMS" | tr '\n' ' ')"
    fi
    if grep -qE '^[0-9a-f]{64}  [^ ]+$' "$CK/SHA256SUMS"; then
        pass "SHA256SUMS 格式为 <hash>  <name>"
    else
        fail "SHA256SUMS 格式不符（期望 <hash> 两个空格 <name>）"
    fi
    if ( cd "$CK" && sha256sum --check --strict --quiet SHA256SUMS ) >/dev/null 2>&1; then
        pass "SHA256SUMS 可被 sha256sum -c 校验"
    else
        fail "SHA256SUMS 无法通过 sha256sum -c"
    fi
else
    fail "--checksums <dir> 未写出 SHA256SUMS"
fi

mkdir -p "$WORK/empty"
if bash "$CHECKSUMS_SH" --checksums "$WORK/empty" >/dev/null 2>&1; then
    fail "空目录 --checksums 应当失败（fail closed）"
else
    pass "空目录 --checksums 失败（fail closed）"
fi

# --ops-tools：成员白名单断言 + 全部迁移都在。
OPS="$WORK/ops/ops-tools.tar.gz"
mkdir -p "$WORK/ops"
if bash "$CHECKSUMS_SH" --ops-tools "$OPS" >"$WORK/ops.log" 2>&1; then
    pass "--ops-tools 退出 0"
    members="$(tar -tzf "$OPS")"
    for m in backup.sh restore.sh db/migrate.sh; do
        if grep -qx -- "$m" <<<"$members"; then
            pass "ops-tools 含顶层成员 $m"
        else
            fail "ops-tools 缺少成员 $m"
        fi
    done
    expected_migrations="$(find "$ROOT/apps/api/src/sql/migrations" -maxdepth 1 -type f -name '*.sql' | wc -l)"
    actual_migrations="$(grep -c '^db/migrations/.*\.sql$' <<<"$members" || true)"
    if [ "$expected_migrations" = "$actual_migrations" ]; then
        pass "ops-tools 含全部 $actual_migrations 个迁移"
    else
        fail "ops-tools 迁移数不符：$actual_migrations/$expected_migrations"
    fi
    if grep -qE '^/|(^|/)\.\.(/|$)' <<<"$members"; then
        fail "ops-tools 含绝对路径/.. 成员"
    else
        pass "ops-tools 无绝对路径/.. 成员"
    fi
    # 确定性：同输入两次构建必须逐字节一致。
    if bash "$CHECKSUMS_SH" --ops-tools "$WORK/ops/ops-tools-2.tar.gz" >/dev/null 2>&1 \
        && [ "$(sha256sum <"$OPS" | cut -d' ' -f1)" = "$(sha256sum <"$WORK/ops/ops-tools-2.tar.gz" | cut -d' ' -f1)" ]; then
        pass "ops-tools 构建确定性（两次 sha256 相同）"
    else
        fail "ops-tools 构建不确定（两次 sha256 不同）"
    fi
else
    fail "--ops-tools 未退出 0"
    sed 's/^/      /' "$WORK/ops.log" | head -5
fi

echo "== 8. 语法门禁（bash -n）=="

for s in "$CHECKSUMS_SH" "$INSTALL_SH" "$ROOT/scripts/test-release-workflow.sh"; do
    if bash -n "$s" 2>"$WORK/bashn.log"; then
        pass "bash -n 通过：${s#"$ROOT"/}"
    else
        fail "bash -n 失败：${s#"$ROOT"/}"
        sed 's/^/      /' "$WORK/bashn.log" | head -5
    fi
done

# 可选：有 PyYAML 时再做一次真正的 YAML 解析（无则明确跳过，不算失败）。
if python3 -c 'import yaml' >/dev/null 2>&1; then
    if python3 - "$RELEASE_YML" "$RC_YML" <<'PY'
import sys, yaml
for p in sys.argv[1:]:
    with open(p, encoding="utf-8") as fh:
        yaml.safe_load(fh)
PY
    then
        pass "PyYAML 解析 release.yml / rc.yml 通过"
    else
        fail "PyYAML 解析失败（YAML 语法错误）"
    fi
else
    note "（跳过 YAML 语法解析：本机无 PyYAML）"
fi

echo
printf '通过 %d 项，失败 %d 项\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
