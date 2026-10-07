#!/usr/bin/env bash
#
# 组装生产 Web 发布产物 `web-dist.tar.gz`。
#
# 归档布局（**显式且被验证**，见 docs/frontend/ARCHITECTURE.md）：
#
#   bootstrap/                       ← apps/web/dist（引导页，无 React）
#     index.html
#     assets/...
#   frontends/
#     default/
#       <version>/                   ← frontends/default/dist（版本化不可变制品）
#         frontend.json
#         assets/frontend.js
#         assets/frontend.css
#
# 说明：
#   - **不打包源码**，也不打包 node_modules / pnpm：制品自包含。
#   - install.sh 把 `bootstrap/` 铺到 `$FLOATCTF_HOME/web`，把 `frontends/` 合并进
#     `$FLOATCTF_HOME/frontends`（保留第三方已安装前端，只更新 default 与注册表）。
#
# 用法：scripts/package-web-dist.sh [输出路径]（默认 release/web-dist.tar.gz）
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/release/web-dist.tar.gz}"

die() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
info() { printf '[INFO] %s\n' "$*"; }
ok() { printf '[ OK ] %s\n' "$*"; }

BOOTSTRAP_DIR="$ROOT/apps/web/dist"
FRONTEND_DIR="$ROOT/frontends/default/dist"
FRONTEND_PKG="$ROOT/frontends/default/package.json"

[ -f "$BOOTSTRAP_DIR/index.html" ] || die "缺少 bootstrap 构建产物: $BOOTSTRAP_DIR/index.html（先跑 pnpm run build:web）"
[ -f "$FRONTEND_DIR/frontend.json" ] || die "缺少 Default Frontend manifest: $FRONTEND_DIR/frontend.json"
[ -f "$FRONTEND_PKG" ] || die "缺少 frontends/default/package.json"

VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$FRONTEND_PKG")"
[ -n "$VERSION" ] || die "无法解析 Default Frontend 版本号"

ENTRY="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["entry"])' "$FRONTEND_DIR/frontend.json")"
[ -f "$FRONTEND_DIR/$ENTRY" ] || die "frontend.json 声明的 entry 不存在: $ENTRY"

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-web-dist.XXXXXX")"
trap 'rm -rf -- "$STAGE"' EXIT

mkdir -p "$STAGE/bootstrap" "$STAGE/frontends/default/$VERSION"
cp -a "$BOOTSTRAP_DIR/." "$STAGE/bootstrap/"
cp -a "$FRONTEND_DIR/." "$STAGE/frontends/default/$VERSION/"

mkdir -p "$(dirname "$OUT")"
# 确定性归档：固定 mtime/排序/属主，并去掉压缩时间戳，便于比对与校验。
tar --create --gzip --file "$OUT" \
    --directory "$STAGE" \
    --owner=0 --group=0 --numeric-owner \
    --mtime='@0' --sort=name \
    bootstrap frontends

ok "已生成 $OUT"
tar tzf "$OUT" | awk -F/ '{print $1"/"$2}' | sort -u | sed 's/^/  内容: /'
ok "bootstrap + frontends/default/$VERSION"
