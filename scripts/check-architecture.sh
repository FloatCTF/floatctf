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
#   8. SDK 不得有模块级可变传输绑定（客户端必须实例隔离）
#   9. @floatctf/react 的实时 hook 不得硬编码 "/api/" 等站点路径
#  10. 前端管理器的公开注册表 schema 与制品 schema 是两个独立常量，且不写安装来源
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


# ── 9. SDK 不得有模块级可变传输绑定（客户端实例隔离）─────────────────────────
info "checking @floatctf/sdk has no module-global transport binding"
hits="$(grep -rnE --include='*.ts' \
    '\b(service_api|admin_api|bindHttpClients|resetHttpClientsForTests|httpClients)\b' \
    packages/sdk/src 2>/dev/null | grep -v '__tests__' | grep -v '/dist/' | code_only || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/sdk still exports module-global transport bindings (clients must be instance-scoped)"
    printf '%s\n' "$hits" >&2
fi
hits="$(grep -rnE --include='*.ts' \
    '^(const|let|var)[[:space:]]+(binding|currentBinding|globalTransport|sharedTransport)\b' \
    packages/sdk/src 2>/dev/null | grep -v '__tests__' | code_only || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/sdk declares a module-level mutable transport binding"
    printf '%s\n' "$hits" >&2
fi

# ── 10. @floatctf/react 实时 hook 不得硬编码站点路径 ─────────────────────────
info "checking @floatctf/react realtime hooks use the supplied client"
hits="$(grep -rnE --include='*.ts' --include='*.tsx' \
    'url:[[:space:]]*[`\"\x27](/api|/__floatctf)' packages/react/src 2>/dev/null | code_only || true)"
if [ -n "$hits" ]; then
    fail "@floatctf/react hardcodes an API/asset origin instead of using the supplied client base URL"
    printf '%s\n' "$hits" >&2
fi
hits="$(grep -rnE --include='*.ts' 'client\.sse\.(connect|connectAdmin)' packages/react/src 2>/dev/null | wc -l)"
if [ "${hits// /}" -lt 4 ]; then
    fail "@floatctf/react realtime hooks must connect through client.sse.connect/connectAdmin (found $hits)"
fi

# ── 11. 前端管理器的两个 schema 常量必须独立，且注册表不写安装来源 ────────────
info "checking frontend.sh keeps manifest/registry schemas separate and public-safe"
registry_const="$(sed -nE 's/^REGISTRY_SCHEMA_VERSION="([^"]+)".*/\1/p' scripts/frontend.sh | head -1)"
manifest_const="$(sed -nE 's/^MANIFEST_SCHEMA_VERSION="([^"]+)".*/\1/p' scripts/frontend.sh | head -1)"
if [ -z "$registry_const" ] || [ -z "$manifest_const" ]; then
    fail "scripts/frontend.sh must define REGISTRY_SCHEMA_VERSION and MANIFEST_SCHEMA_VERSION separately"
fi
if ! grep -q 'MANIFEST_SCHEMA_VERSION' scripts/frontend.sh; then
    fail "scripts/frontend.sh lost the manifest schema constant"
fi
if grep -qE '^[[:space:]]*record\["source"\]' scripts/frontend.sh; then
    fail "scripts/frontend.sh persists an installation `source` into the PUBLIC registry"
fi
# 只看真实的选项解析，不看注释/文档里对历史的说明。
if grep -qE '^[[:space:]]*--reinstall\)' scripts/frontend.sh 2>/dev/null \
    || grep -q 'INSTALL_REINSTALL' scripts/frontend.sh 2>/dev/null; then
    fail "frontend versions are immutable: the --reinstall path must not exist anymore"
fi
hits="$(grep -rn 'source_label_for' scripts/frontend.sh 2>/dev/null || true)"
if [ -n "$hits" ]; then
    fail "scripts/frontend.sh still has the removed source_label_for helper"
fi

# ── 12. Default Frontend 只能 import 公开包 API ──────────────────────────────
info "checking the Default Frontend imports only public package APIs"
hits="$(grep -rnE --include='*.ts' --include='*.tsx' \
    'from "@floatctf/[a-z-]+/[a-zA-Z0-9._/-]+"' frontends/default/src 2>/dev/null \
    | grep -vE 'from "@floatctf/sdk/entity(/[a-zA-Z0-9._-]+)?"' | code_only || true)"
if [ -n "$hits" ]; then
    fail "frontends/default imports a package subpath that is not a public entry (allowed: @floatctf/sdk/entity*)"
    printf '%s\n' "$hits" >&2
fi

if [ "$FAIL" -ne 0 ]; then
    echo "[check-architecture] architecture boundary violated" >&2
    exit 1
fi
echo "[check-architecture] OK"
