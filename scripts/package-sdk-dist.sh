#!/usr/bin/env bash
#
# 生成三个**可发布包**的发行 tarball，并做 fail-closed 校验：
#
#   @floatctf/sdk               (packages/sdk)
#   @floatctf/react             (packages/react)
#   @floatctf/frontend-runtime  (packages/frontend-runtime)
#
# 用法：
#   scripts/package-sdk-dist.sh <output-dir> [--version <V>]
#   scripts/package-sdk-dist.sh --help
#
# 产物（确定性命名，全部位于 <output-dir>）：
#   floatctf-sdk-<V>.tgz
#   floatctf-react-<V>.tgz
#   floatctf-frontend-runtime-<V>.tgz
#   SDK-SHA256SUMS     `<sha256>  <name>`（按名字排序；不含自身）
#
# 为什么需要它（与 `pnpm pack` 裸用的区别）：
#   * 三个包的 `files` 只含 `dist` + `README.md`，所以**必须先构建**
#     （`pnpm run build:packages`），否则打出来的是空壳/旧产物。
#   * 版本号必须三包一致（默认取 packages/sdk/package.json）；不一致直接失败，
#     避免产出 "react@1.0.0 依赖 sdk@0.9.0" 这类无法安装的组合。
#   * 每个 tarball 生成后逐项校验：顶层目录、入口、README/LICENSE、
#     依赖范围（`@floatctf/react` 的 `@floatctf/sdk` 必须是具体版本，
#     绝不能残留 `workspace:`）、无源码/测试/密钥/仓库绝对路径。
#   * `pnpm pack` 只写入临时暂存目录，改名后再复制到 <output-dir>；
#     仓库树（尤其 packages/*/）不会残留任何 *.tgz。
#   * 连续 pack 两次比较 sha256，报告字节级确定性结论。
#
# 注意：本脚本**只产出 tarball**，不做任何 registry 发布。
#
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

die() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
info() { printf '[INFO] %s\n' "$*"; }
ok() { printf '[ OK ] %s\n' "$*"; }
warn() { printf '[WARN] %s\n' "$*" >&2; }

usage() {
	cat <<'EOF'
用法: scripts/package-sdk-dist.sh <output-dir> [--version <V>]

为 FloatCTF 的三个可发布包生成发行 tarball：
  floatctf-sdk-<V>.tgz / floatctf-react-<V>.tgz / floatctf-frontend-runtime-<V>.tgz

参数:
  <output-dir>       输出目录（不存在则创建）。允许在仓库内，但必须由使用者显式指定。
  --version <V>      期望版本；必须与三个 packages/*/package.json 的 version 完全一致，
                     否则失败（不会用 --version 覆盖包内版本）。
  -h, --help         显示本帮助并退出。

行为:
  1. 读取并校验三个包的版本一致。
  2. 运行 `pnpm run build:packages` 刷新 dist/（files 只含 dist）。
  3. 逐包 `pnpm pack` 到临时目录，改名为确定性名称后复制到 <output-dir>。
  4. 校验每个 tarball（结构/入口/README/LICENSE/依赖范围/无源码无密钥）。
  5. 打印 `name size sha256` 表，写 <output-dir>/SDK-SHA256SUMS。
  6. 报告两次 pack 的字节级确定性结论。

不发布到任何 registry。
EOF
}

# ── 参数解析（在依赖检查之前，保证 --help 永远可用）────────────────────────
OUT_DIR=""
WANT_VERSION=""
while [ $# -gt 0 ]; do
	case "$1" in
		-h | --help)
			usage
			exit 0
			;;
		--version)
			[ $# -ge 2 ] || die "--version 需要一个参数"
			WANT_VERSION="$2"
			shift 2
			;;
		--version=*)
			WANT_VERSION="${1#*=}"
			shift
			;;
		-*)
			die "未知选项: $1（用 --help 查看用法）"
			;;
		*)
			[ -z "$OUT_DIR" ] || die "只接受一个 <output-dir> 参数（多余: $1）"
			OUT_DIR="$1"
			shift
			;;
	esac
done
[ -n "$OUT_DIR" ] || {
	usage >&2
	die "缺少 <output-dir> 参数"
}

# ── 依赖：python3（仓库其它脚本同样依赖）/ pnpm（优先 `mise exec -- pnpm`）──
command -v python3 >/dev/null 2>&1 || die "需要 python3 解析 package.json"
PNPM=()
if command -v mise >/dev/null 2>&1; then
	# 仓库约定：pnpm 必须通过 mise 运行（见 AGENTS.md）。
	PNPM=(mise exec -- pnpm)
elif command -v pnpm >/dev/null 2>&1; then
	warn "未找到 mise，直接使用 PATH 上的 pnpm"
	PNPM=(pnpm)
else
	die "既没有 mise 也没有 pnpm，无法 pack"
fi

# ── 包清单 ───────────────────────────────────────────────────────────────────
PKG_KEYS=(sdk react frontend-runtime)
PKG_NAMES=(@floatctf/sdk @floatctf/react @floatctf/frontend-runtime)
PKG_BASENAMES=(floatctf-sdk floatctf-react floatctf-frontend-runtime)

pkg_field() { # <package.json> <field>
	python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2],""))' "$1" "$2"
}

# ── 1. 版本一致性 ────────────────────────────────────────────────────────────
V_SDK="$(pkg_field "$ROOT/packages/sdk/package.json" version)"
V_REACT="$(pkg_field "$ROOT/packages/react/package.json" version)"
V_RUNTIME="$(pkg_field "$ROOT/packages/frontend-runtime/package.json" version)"
[ -n "$V_SDK" ] || die "无法从 packages/sdk/package.json 读取 version"
if [ "$V_SDK" != "$V_REACT" ] || [ "$V_SDK" != "$V_RUNTIME" ]; then
	die "三个包版本不一致：sdk=$V_SDK react=$V_REACT frontend-runtime=$V_RUNTIME（发布前必须统一）"
fi
VERSION="$V_SDK"
case "$VERSION" in
	*[!0-9A-Za-z.+-]* | *..* | .* | *.)
		die "版本号 '$VERSION' 不是安全的文件名片段"
		;;
esac
if [ -n "$WANT_VERSION" ] && [ "$WANT_VERSION" != "$VERSION" ]; then
	die "--version $WANT_VERSION 与包内版本 $VERSION 不一致（本脚本不会覆盖包内版本）"
fi
info "版本: $VERSION（三个包一致）"

mkdir -p "$OUT_DIR" || die "无法创建输出目录: $OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
info "输出目录: $OUT_DIR"

# ── 2. 构建（files 只含 dist，必须先构建）────────────────────────────────────
info "运行 pnpm run build:packages ..."
( cd "$ROOT" && "${PNPM[@]}" run build:packages ) || die "pnpm run build:packages 失败"
for key in "${PKG_KEYS[@]}"; do
	for rel in "dist/index.js" "dist/index.d.ts"; do
		[ -f "$ROOT/packages/$key/$rel" ] \
			|| die "构建后仍缺少 packages/$key/$rel（dist 不完整，tarball 会在外部不可用）"
	done
done
ok "dist 入口已就绪（每个包 dist/index.js + dist/index.d.ts）"

# ── 3. 暂存 + trap 清理 ──────────────────────────────────────────────────────
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-sdk-dist.XXXXXX")"
cleanup() { rm -rf -- "$STAGE"; }
trap cleanup EXIT
mkdir -p "$STAGE/logs"

run_pack() { # <dest-dir> <filter>
	local dest="$1" filter="$2"
	local log="$STAGE/logs/$(printf '%s' "$filter" | tr '/@' '__').log"
	mkdir -p "$dest"
	if ! ( cd "$ROOT" && "${PNPM[@]}" --filter "$filter" pack --pack-destination "$dest" ) >"$log" 2>&1; then
		tail -n 20 "$log" >&2
		die "pnpm pack 失败: $filter"
	fi
}

pack_run() { # <run-dir>  → 产出 <run-dir>/<key>/<pkg>.tgz
	local run_dir="$1"
	local i
	for i in "${!PKG_KEYS[@]}"; do
		local key="${PKG_KEYS[$i]}" filter="${PKG_NAMES[$i]}"
		local dest="$run_dir/$key"
		if [ "$run_dir" = "$STAGE/run1" ]; then
			info "pack $filter ..."
		fi
		run_pack "$dest" "$filter"
		local count
		count="$(find "$dest" -maxdepth 1 -name '*.tgz' -type f | wc -l | tr -d ' ')"
		[ "$count" = "1" ] || die "pnpm pack $filter 产出了 $count 个 tarball（期望 1）"
	done
}

# 第一轮：最终产物；第二轮：只用于确定性对比。
pack_run "$STAGE/run1"
pack_run "$STAGE/run2"

for i in "${!PKG_KEYS[@]}"; do
	key="${PKG_KEYS[$i]}"
	out_name="${PKG_BASENAMES[$i]}-$VERSION.tgz"
	src="$(find "$STAGE/run1/$key" -maxdepth 1 -name '*.tgz' -type f)"
	mv -f -- "$src" "$OUT_DIR/$out_name"
done

# ── 4. 逐 tarball 校验（fail-closed）─────────────────────────────────────────
EXTRACT_ROOT="$STAGE/extract"

json_field() { # <json-file> <field>
	python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2],""))' "$1" "$2"
}

# 校验 tarball 内的依赖范围：不允许 workspace:，react 必须有具体的 sdk 版本。
# 退出码：0=通过（stdout 为 SDK_RANGE:<spec> 或 OK）；非 0=失败（stdout 为原因）。
check_dep_ranges() { # <packed package.json> <pkg name>
	python3 - "$1" "$2" <<'PY'
import json, re, sys
path, name = sys.argv[1], sys.argv[2]
data = json.load(open(path))
bad = []
for key in ("dependencies", "peerDependencies", "optionalDependencies", "devDependencies"):
    for dep, spec in (data.get(key) or {}).items():
        if isinstance(spec, str) and spec.startswith("workspace:"):
            bad.append(f"{key}[{dep}]={spec}")
if bad:
    print("残留 workspace: 范围 -> " + "; ".join(bad))
    sys.exit(3)
if name == "@floatctf/react":
    spec = (data.get("dependencies") or {}).get("@floatctf/sdk")
    if spec is None:
        print("dependencies 缺少 @floatctf/sdk")
        sys.exit(4)
    if not re.match(r"^[~^]?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$", spec):
        print(f"@floatctf/sdk 的范围不是具体版本: {spec!r}")
        sys.exit(5)
    print(f"SDK_RANGE:{spec}")
else:
    print("OK")
PY
}

validate_tarball() { # <tarball> <pkg-name> <expected-version>
	local tarball="$1" pkgname="$2" want_version="$3"
	local base; base="$(basename "$tarball")"
	[ -f "$tarball" ] || die "$base: 不存在"

	local members
	members="$(tar -tzf "$tarball" | LC_ALL=C sort)"
	[ -n "$members" ] || die "$base: tarball 为空"

	# 4.1 顶层目录必须恰好是 package/
	local tops
	tops="$(printf '%s\n' "$members" | awk -F/ '{print $1}' | LC_ALL=C sort -u | tr '\n' ' ')"
	[ "$tops" = "package " ] \
		|| die "$base: 顶层目录不是唯一的 package/（实际: '$tops'）"

	has_member() { printf '%s\n' "$members" | grep -qxF "$1"; }

	for required in \
		package/package.json \
		package/dist/index.js \
		package/dist/index.d.ts \
		package/README.md; do
		has_member "$required" || die "$base: 缺少必需成员 $required"
	done
	# LICENSE：license 声明为 AGPL-3.0-only，分发必须带协议正文。
	# pnpm 会自动从仓库根带上 LICENSE；若某天它消失了必须立刻失败。
	has_member "package/LICENSE" \
		|| die "$base: 缺少 package/LICENSE（AGPL-3.0-only 分发必须附带协议正文）"

	# 4.2 禁止内容（模式与说明分开放：ERE 模式自身含 `|`，不能拿 `|` 当分隔符）
	local -a forbidden_patterns=(
		'^package/src/'
		'^package/[^/]+/__tests__/'
		'(^|/)\.test\.(ts|tsx|js|jsx|mjs|cjs)$'
		'(^|/)node_modules/'
		'(^|/)\.env($|\.)'
		'(^|/)\.npmrc$'
		'(^|/)\.netrc$'
		'(^|/)id_(rsa|ed25519)$'
		'\.(pem|key|p12|pfx)$'
		'(^|/)(credentials?|secrets?)\.'
		'(^|/)\.git/'
		'^/'
		'(^|/)\.\.(/|$)'
	)
	local -a forbidden_descs=(
		'源码目录 src/'
		'测试目录 __tests__/'
		'测试文件 *.test.*'
		'node_modules'
		'.env 文件'
		'.npmrc'
		'.netrc'
		'SSH 私钥'
		'密钥文件（.pem/.key/.p12/.pfx）'
		'疑似密钥文件'
		'.git 目录'
		'绝对路径成员'
		'.. 路径穿越成员'
	)
	local idx hit
	for idx in "${!forbidden_patterns[@]}"; do
		if hit="$(printf '%s\n' "$members" | grep -E "${forbidden_patterns[$idx]}" | head -n 3)"; then
			die "$base: 含禁止内容（${forbidden_descs[$idx]}）: $(printf '%s' "$hit" | tr '\n' ' ')"
		fi
	done
	# 注：`grep` 无匹配时返回 1，在 `if` 条件里不会触发 set -e。

	# 4.3 解包后检查内容（name/version/依赖范围/仓库绝对路径）
	local ext="$EXTRACT_ROOT/$base"
	rm -rf -- "$ext"
	mkdir -p "$ext"
	tar -xzf "$tarball" -C "$ext"

	local pj="$ext/package/package.json"
	[ -f "$pj" ] || die "$base: 缺少 package/package.json"
	local got_name got_version
	got_name="$(json_field "$pj" name)"
	got_version="$(json_field "$pj" version)"
	[ "$got_name" = "$pkgname" ] || die "$base: package.json name=$got_name（期望 $pkgname）"
	[ "$got_version" = "$want_version" ] \
		|| die "$base: package.json version=$got_version（期望 $want_version）"

	local dep_out
	if ! dep_out="$(check_dep_ranges "$pj" "$pkgname")"; then
		die "$base: 打包后的依赖范围不合法: $dep_out"
	fi
	case "$dep_out" in
		SDK_RANGE:*) info "$base: packed dependencies[\"@floatctf/sdk\"] = ${dep_out#SDK_RANGE:}" ;;
	esac

	# 仓库私有绝对路径不得出现在任何文本成员里。
	local leaks
	leaks="$(grep -rIl -e "$ROOT" -e '/home/[A-Za-z0-9._-]*/Projects/' "$ext/package" 2>/dev/null | head -n 3 || true)"
	[ -z "$leaks" ] || die "$base: 含仓库绝对路径: $(printf '%s' "$leaks" | tr '\n' ' ')"

	ok "$base 校验通过（$pkgname@$got_version）"
}

for i in "${!PKG_KEYS[@]}"; do
	validate_tarball "$OUT_DIR/${PKG_BASENAMES[$i]}-$VERSION.tgz" "${PKG_NAMES[$i]}" "$VERSION"
done

# ── 5. 确定性报告（成员顺序 + 字节级）───────────────────────────────────────
info "确定性检查（第二次 pack 对比第一次）..."
for i in "${!PKG_KEYS[@]}"; do
	key="${PKG_KEYS[$i]}"
	out_name="${PKG_BASENAMES[$i]}-$VERSION.tgz"
	a="$OUT_DIR/$out_name"
	b="$(find "$STAGE/run2/$key" -maxdepth 1 -name '*.tgz' -type f)"

	if diff -q <(tar -tzf "$a" | LC_ALL=C sort) <(tar -tzf "$b" | LC_ALL=C sort) >/dev/null; then
		ok "$out_name: 成员顺序确定（排序后一致）"
	else
		warn "$out_name: 成员顺序在两次 pack 之间不一致（存在非确定性打包顺序）"
	fi
	if cmp -s "$a" "$b"; then
		ok "$out_name: 字节级确定（两次 pack sha256 相同）"
	else
		warn "$out_name: 字节级不确定 —— 两次 pack sha256 不同；"
		warn "  常见原因：gzip 头里的 mtime/OS 字节、tar 成员顺序或文件系统枚举顺序。"
		warn "  成员顺序与内容仍可校验（见 SDK-SHA256SUMS），但不应把 sha256 当作跨机器可复现指纹。"
	fi
done

# ── 6. 汇总表 + SDK-SHA256SUMS ───────────────────────────────────────────────
SUMS_FILE="$OUT_DIR/SDK-SHA256SUMS"
: >"$SUMS_FILE"
printf '%-42s %12s  %s\n' 'NAME' 'SIZE(bytes)' 'SHA256'
for i in "${!PKG_KEYS[@]}"; do
	out_name="${PKG_BASENAMES[$i]}-$VERSION.tgz"
	f="$OUT_DIR/$out_name"
	size="$(wc -c <"$f" | tr -d ' ')"
	hash="$(sha256sum "$f" | cut -d' ' -f1)"
	printf '%-42s %12s  %s\n' "$out_name" "$size" "$hash"
done
# SDK-SHA256SUMS：`<hash>  <name>`（相对文件名），按名字排序，不含自身。
# 在输出目录内执行，保证记录的是文件名而不是绝对路径。
(
	cd "$OUT_DIR"
	for i in "${!PKG_KEYS[@]}"; do
		sha256sum "${PKG_BASENAMES[$i]}-$VERSION.tgz"
	done | LC_ALL=C sort -k2
) >"$SUMS_FILE"
ok "已写入 $SUMS_FILE"

# ── 7. 仓库树绝不允许残留 tarball ────────────────────────────────────────────
stray_pkgs="$(find "$ROOT/packages" -name '*.tgz' -type f 2>/dev/null || true)"
[ -z "$stray_pkgs" ] || die "packages/*/ 内残留 tarball: $(printf '%s' "$stray_pkgs" | tr '\n' ' ')"
stray_root="$(find "$ROOT" -maxdepth 1 -name '*.tgz' -type f 2>/dev/null || true)"
[ -z "$stray_root" ] || die "仓库根目录残留 tarball: $(printf '%s' "$stray_root" | tr '\n' ' ')"
ok "仓库树内无残留 *.tgz"

info "完成。产物位于 $OUT_DIR（未发布到任何 registry）。"
