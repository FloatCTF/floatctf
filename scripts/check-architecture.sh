#!/usr/bin/env bash
#
# FloatCTF 前端平台架构边界守卫（docs/frontend/ARCHITECTURE.md）。
#
# 这些边界的意义：一旦破防，可插拔前端就退化成"同一个仓库里的一堆页面"——
# 外部仓库无法独立构建、非 React 前端被迫下载 Primer、bootstrap 悄悄把默认前端
# 打进产物。文字约定靠不住，所以这里做成**失败即红**的门禁。
#
# 规则（全部为"存在即违规"，无例外）：
#   1. @floatctf/sdk              不得依赖 React / 路由 / 状态库 / Primer / 应用层
#   2. @floatctf/react            不得包含 UI（Primer / Tailwind / CSS / 路由 / 页面）
#   3. @floatctf/frontend-runtime 不得依赖任何 UI 框架
#   4. frontends/default          不得 import apps/web 或逃出自身包的仓库路径
#   5. apps/web (bootstrap)       不得 import frontends/default（生产产物必须纯净）
#   6. packages/*                 不得 import 任何 app/frontend
#   7. 后端 API_CONTRACT_VERSION 必须与前端运行时的 API 契约一致
#
# 用法：scripts/check-architecture.sh
set -Eeuo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

FAIL=0

info() { printf '[check-architecture] %s\n' "$*"; }
fail() { printf '[check-architecture] FAIL: %s\n' "$*" >&2; FAIL=1; }

# 依赖存在性断言：路径漂移必须报错，绝不静默放行。
require_dir() {
    [ -d "$1" ] || fail "expected directory missing: $1（守卫会静默失效，必须修）"
}
require_file() {
    [ -f "$1" ] || fail "expected file missing: $1"
}

require_dir packages/sdk/src
require_dir packages/react/src
require_dir packages/frontend-runtime/src
require_dir apps/web/src
require_dir frontends/default/src
require_file apps/api/src/core/contract.rs

# 只检查非注释行：注释里提到某个包名不算违规。
code_only() { grep -vE '^[[:space:]]*(//|/\*|\*|\*/)'; }

# matches_imports <dir> <extended-regex>
matches_imports() {
    local dir="$1" pattern="$2"
    grep -rnE --include='*.ts' --include='*.tsx' --include='*.css' \
        "from \"${pattern}\"|import\\(\"${pattern}\"|@import[[:space:]]+\"${pattern}" \
        "$dir" 2>/dev/null | grep -v '/dist/' | grep -v '/node_modules/' | code_only || true
}

check_forbidden() { # <label> <dir> <regex> <human>
    local label="$1" dir="$2" pattern="$3" human="$4"
    local hits
    hits="$(matches_imports "$dir" "$pattern")"
    if [ -n "$hits" ]; then
        fail "$label imports $human"
        printf '%s\n' "$hits" >&2
    fi
}

# ── 1. @floatctf/sdk 必须框架无关 ─────────────────────────────────────────────
info "checking @floatctf/sdk independence"
for entry in \
    'react:React' \
    'react-dom:ReactDOM' \
    'react/jsx-runtime:React JSX runtime' \
    '@tanstack/react-query:TanStack Query' \
    '@tanstack/react-router:TanStack Router' \
    'zustand:Zustand' \
    '@primer/react:Primer' \
    '@primer/octicons-react:Primer Octicons' \
    'tailwindcss:Tailwind' \
    'styled-components:styled-components' \
    'axios-mock-adapter:axios-mock-adapter'; do
    check_forbidden "@floatctf/sdk" packages/sdk/src "${entry%%:*}" "${entry#*:}"
done

# 仓库私有路径：SDK 不得知道 apps/ 或 frontends/ 的存在
hits="$(grep -rnE --include='*.ts' --include='*.tsx' 'from "(@/|\.\./\.\./\.\./|.*apps/web|.*frontends/)' packages/sdk/src 2>/dev/null | code_only || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/sdk imports repository-private paths"
    printf '%s\n' "$hits" >&2
fi

# ── 2. @floatctf/react 只允许 headless ───────────────────────────────────────
info "checking @floatctf/react is headless"
for entry in \
    '@primer/react:Primer' \
    '@primer/octicons-react:Primer Octicons' \
    'tailwindcss:Tailwind' \
    'styled-components:styled-components' \
    '@tanstack/react-router:TanStack Router' \
    '@floatctf/frontend-default:the Default Frontend' \
    '@floatctf/frontend-runtime:the frontend runtime (bootstrap contract)'; do
    check_forbidden "@floatctf/react" packages/react/src "${entry%%:*}" "${entry#*:}"
done
hits="$(grep -rnE --include='*.ts' --include='*.tsx' 'from "(@/|.*apps/web|.*frontends/)' packages/react/src 2>/dev/null | code_only || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/react imports repository-private paths"
    printf '%s\n' "$hits" >&2
fi
# headless 包不得含 CSS
hits="$(find packages/react/src -name '*.css' -o -name '*.scss' 2>/dev/null || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/react contains stylesheets"
    printf '%s\n' "$hits" >&2
fi
# headless 包不得含 JSX：非测试目录里出现 .tsx 就说明有人开始写组件了。
hits="$(find packages/react/src -name '*.tsx' ! -path '*__tests__*' 2>/dev/null || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/react contains .tsx outside tests (must stay headless)"
    printf '%s\n' "$hits" >&2
fi

# ── 3. @floatctf/frontend-runtime 必须框架无关 ───────────────────────────────
info "checking @floatctf/frontend-runtime independence"
for entry in \
    'react:React' \
    'react-dom:ReactDOM' \
    '@tanstack/react-query:TanStack Query' \
    'zustand:Zustand' \
    '@primer/react:Primer' \
    'tailwindcss:Tailwind' \
    '@floatctf/sdk:the SDK' \
    '@floatctf/react:the React bindings'; do
    check_forbidden "@floatctf/frontend-runtime" packages/frontend-runtime/src "${entry%%:*}" "${entry#*:}"
done

# ── 4. frontends/default 不得逃出自身包 ──────────────────────────────────────
info "checking frontends/default stays inside its package"
hits="$(grep -rnE --include='*.ts' --include='*.tsx' --include='*.css' \
    'from "(\.\./)+apps/|from "apps/|from "@floatctf/web|@floatctf/sdk/src|@floatctf/react/src|@floatctf/frontend-runtime/src' \
    frontends/default/src 2>/dev/null | grep -v '/dist/' | code_only || true)"
if [ -n "$hits" ]; then
    fail "frontends/default escapes its package (apps/web or another package's src)"
    printf '%s\n' "$hits" >&2
fi

# ── 5. bootstrap 不得把默认前端打进产物 ──────────────────────────────────────
info "checking apps/web bootstrap stays a bootstrap"
hits="$(grep -rnE --include='*.ts' --include='*.tsx' --include='*.html' \
    'from "@floatctf/frontend-default|from "@floatctf/react|from "react' \
    apps/web/src apps/web/index.html 2>/dev/null | code_only || true)"
if [ -n "$hits" ]; then
    fail "apps/web bootstrap imports the Default Frontend or React (must stay boring)"
    printf '%s\n' "$hits" >&2
fi
if [ -d apps/web/dist ]; then
    hits="$(grep -rl 'frontend-default' apps/web/dist 2>/dev/null || true)"
    if [ -n "$hits" ]; then
        fail "bootstrap production bundle references @floatctf/frontend-default"
        printf '%s\n' "$hits" >&2
    fi
fi

# ── 6. packages/* 不得反向依赖 app / frontend ────────────────────────────────
info "checking packages/* never import apps or frontends"
hits="$(grep -rnE --include='*.ts' --include='*.tsx' \
    'from "(@/|[.][.]/[.][.]/[.][.]/(apps|frontends)|.*apps/web|.*frontends/default)' \
    packages 2>/dev/null | grep -v '/dist/' | grep -v '/node_modules/' | code_only || true)"
if [ -n "$hits" ]; then
    fail "a package imports an app/frontend (dependency direction inverted)"
    printf '%s\n' "$hits" >&2
fi

# ── 7. API 契约版本必须前后端一致 ────────────────────────────────────────────
info "checking API contract version agreement"
runtime_version="$(sed -nE 's/^export const API_CONTRACT_VERSION = "([^"]+)";.*/\1/p' packages/frontend-runtime/src/version.ts)"
api_version="$(sed -nE 's/^pub const API_CONTRACT_VERSION: &str = "([^"]+)";.*/\1/p' apps/api/src/core/contract.rs)"
if [ -z "$runtime_version" ]; then
    fail "could not read API_CONTRACT_VERSION from packages/frontend-runtime/src/version.ts"
fi
if [ -z "$api_version" ]; then
    fail "could not read API_CONTRACT_VERSION from apps/api/src/core/contract.rs"
fi
if [ -n "$runtime_version" ] && [ "$runtime_version" != "$api_version" ]; then
    fail "API contract version drift: runtime=$runtime_version api=$api_version"
fi

# ── 8. 前端运行时契约版本与会话契约一致 ──────────────────────────────────────
runtime_contract="$(sed -nE 's/^export const FRONTEND_RUNTIME_VERSION = "([^"]+)";.*/\1/p' packages/frontend-runtime/src/version.ts)"
api_runtime_contract="$(sed -nE 's/^pub const FRONTEND_RUNTIME_CONTRACT_VERSION: &str = "([^"]+)";.*/\1/p' apps/api/src/core/contract.rs)"
if [ -n "$runtime_contract" ] && [ "$runtime_contract" != "$api_runtime_contract" ]; then
    fail "frontend runtime contract drift: runtime=$runtime_contract api=$api_runtime_contract"
fi

if [ "$FAIL" -ne 0 ]; then
    echo "[check-architecture] architecture boundary violated" >&2
    exit 1
fi
echo "[check-architecture] OK"
