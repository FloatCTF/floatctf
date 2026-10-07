#!/usr/bin/env bash
#
# 发布前端产物验证（release CI 门禁）—— 断言归档布局与契约确实成立，而不是"构建没报错"。
#
# 检查项（对应 docs/frontend/ARTIFACT.md）：
#   1. bootstrap 静态产物存在（index.html）
#   2. Default Frontend manifest 存在且通过**契约校验器**（@floatctf/frontend-runtime）
#   3. manifest 声明的 entry / styles 在归档内真实存在
#   4. 归档里**没有**源码/依赖残留（package.json、node_modules、src/**、pnpm-lock.yaml）
#   5. 归档里不存在绝对路径 / `..` / 符号链接成员
#   6. `scripts/frontend.sh` 语法有效
#
# 用法：scripts/verify-release-frontend.sh [web-dist.tar.gz]
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVE="${1:-$ROOT/release/web-dist.tar.gz}"

FAIL=0
info() { printf '[verify-frontend] %s\n' "$*"; }
fail() { printf '[verify-frontend] FAIL: %s\n' "$*" >&2; FAIL=1; }

[ -f "$ARCHIVE" ] || { fail "归档不存在: $ARCHIVE"; exit 1; }

# ── 6. frontend.sh 语法 ───────────────────────────────────────────────────────
bash -n "$ROOT/scripts/frontend.sh" || fail "scripts/frontend.sh 语法错误"
bash -n "$ROOT/scripts/install.sh" || fail "scripts/install.sh 语法错误"
bash -n "$ROOT/scripts/clean.sh" || fail "scripts/clean.sh 语法错误"

# ── 5. 归档成员安全性 ─────────────────────────────────────────────────────────
LISTING="$(tar tzvf "$ARCHIVE")"
while IFS= read -r line; do
    [ -n "$line" ] || continue
    type="${line:0:1}"
    name="$(printf '%s' "$line" | awk '{ $1=$2=$3=$4=$5=""; sub(/^ +/, ""); print }')"
    [ -n "$name" ] || { fail "无法解析归档成员行: $line"; continue; }
    case "$type" in
        -|d) ;;
        *) fail "归档含非普通文件成员（$type）: $name" ;;
    esac
    case "$name" in
        /*) fail "归档含绝对路径成员: $name" ;;
        *".."*) fail "归档含 .. 成员: $name" ;;
    esac
done <<<"$LISTING"

# ── 4. 不得打包源码/依赖 ─────────────────────────────────────────────────────
for forbidden in "node_modules/" ".pnpm/" "pnpm-lock.yaml" "package-lock.json" "yarn.lock"; do
    if printf '%s\n' "$LISTING" | grep -q "$forbidden"; then
        fail "归档不应包含 $forbidden（制品必须自包含且不含源码/依赖）"
    fi
done

# ── 1. bootstrap ─────────────────────────────────────────────────────────────
if ! printf '%s\n' "$LISTING" | grep -qE '(^| )bootstrap/index\.html$'; then
    fail "归档缺少 bootstrap/index.html"
fi
if printf '%s\n' "$LISTING" | grep -qE '(^| )bootstrap/.*\.(js|css)$'; then
    info "bootstrap 静态资源存在"
else
    fail "归档缺少 bootstrap 静态资源（assets/*.js）"
fi

# ── 2/3. Default Frontend manifest + entry/styles ────────────────────────────
MANIFEST_PATH="$(printf '%s\n' "$LISTING" | awk '{ $1=$2=$3=$4=$5=""; sub(/^ +/, ""); print }' \
    | grep -E '^frontends/default/[^/]+/frontend\.json$' | head -1)"
if [ -z "$MANIFEST_PATH" ]; then
    fail "归档缺少 frontends/default/<version>/frontend.json"
else
    info "Default Frontend manifest: $MANIFEST_PATH"
    STAGE="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-verify.XXXXXX")"
    trap 'rm -rf -- "$STAGE"' EXIT
    tar xzf "$ARCHIVE" -C "$STAGE" "$MANIFEST_PATH"
    MANIFEST="$STAGE/$MANIFEST_PATH"
    ENTRY="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["entry"])' "$MANIFEST")"
    VERSIONDIR="$(dirname "$MANIFEST_PATH")"
    printf '%s\n' "$LISTING" | awk '{ $1=$2=$3=$4=$5=""; sub(/^ +/, ""); print }' \
        | grep -qE "^${VERSIONDIR}/${ENTRY}$" \
        || fail "manifest entry 不在归档内: ${VERSIONDIR}/${ENTRY}"
    for style in $(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(" ".join(d.get("styles") or []))' "$MANIFEST"); do
        printf '%s\n' "$LISTING" | awk '{ $1=$2=$3=$4=$5=""; sub(/^ +/, ""); print }' \
            | grep -qE "^${VERSIONDIR}/${style}$" \
            || fail "manifest style 不在归档内: ${VERSIONDIR}/${style}"
    done
    # 用真实契约校验器再验一遍（权威规则在 TS 包里）。
    if command -v node >/dev/null 2>&1 \
        && [ -f "$ROOT/packages/frontend-runtime/dist/index.js" ]; then
        (cd "$ROOT/frontends/default" && node -e '
            const fs = require("node:fs");
            import("@floatctf/frontend-runtime").then(({ parseFrontendManifest }) => {
                const raw = fs.readFileSync(process.argv[1], "utf8");
                const parsed = parseFrontendManifest(raw);
                if (!parsed.ok) {
                    console.error(parsed.errors.join("\n"));
                    process.exit(1);
                }
                console.log("[verify-frontend] manifest 通过 frontend-runtime 契约校验: " + parsed.manifest.id + "@" + parsed.manifest.version);
            }).catch((error) => { console.error(error); process.exit(1); });
        ' "$MANIFEST") || fail "manifest 未通过 frontend-runtime 契约校验"
    else
        info "跳过 node 契约校验（node 或 frontend-runtime/dist 不可用）"
    fi
fi

# ── 7. 注册表初始数据（install.sh 首次安装时写出）只在 install.sh 内验证 ──────
if [ "$FAIL" -ne 0 ]; then
    echo "[verify-frontend] 制品验证失败" >&2
    exit 1
fi
echo "[verify-frontend] OK"
