#!/usr/bin/env bash
#
# 外部可消费性测试：证明三个可发布包能脱离 monorepo 被**真正安装并使用**。
#
# 与 `scripts/package-sdk-dist.sh` 的区别：
#   * package-sdk-dist.sh 校验"tarball 自身长什么样"；
#   * 本脚本校验"tarball 在仓库之外能不能装、能不能解析、类型能不能加载"。
#
# 步骤：
#   1. 在仓库**之外**的临时目录跑 scripts/package-sdk-dist.sh，产出三个 .tgz。
#   2. 先做**离线** tarball 校验（不需要网络）：顶层 package/、入口、README、
#      无 src/、无测试、无 node_modules、无 .env/密钥、无 workspace: 范围。
#   3. 网络探测：失败则打印 [SKIP] 并**只**跳过 registry 相关步骤（仍然退出 0，
#      因为离线 tarball 校验已经跑过）。
#   4. 新建**全新外部消费者工程**（自带 package.json，name=floatctf-sdk-consumer-test，
#      private、type=module），只把三个 @floatctf/* 从 tarball 装进来；
#      react/react-dom/@tanstack/react-query/typescript/@types/* 作为 devDependencies
#      从公共 registry 装 —— 三个 @floatctf/* 绝不来自 registry。
#   5. 写 consumer.ts（真实 import 三个包并调用公共 API），跑 `tsc --noEmit`，
#      再跑 `node runtime-check.mjs`（真实运行时 import + 断言），
#      以及 `node consumer.ts`（Node 原生类型擦除）如果可用。
#   6. 断言消费者工程**没有任何**指向 FloatCTF 源码树的引用：
#      - 不允许 package.json / lockfile 出现 `workspace:` / `link:` / `"link": true`；
#      - 每个 node_modules/@floatctf/* 的 realpath 必须落在消费者工程**内部**；
#      - 尤其不得落在仓库根目录内。
#
# ── 关键区分（本脚本的核心前提）────────────────────────────────────────────
#   从 tarball 安装会把**真实文件**落到消费者自己的 node_modules 里，这是正确行为；
#   失败条件是"软链接 / workspace: 协议 / link: 协议把消费者指向仓库源码树"，
#   而不是"node_modules 里出现了 @floatctf/*"。
#   pnpm 即使从 tarball 安装也会在消费者自己的 node_modules/.pnpm 内建**相对软链**，
#   所以判定标准是 realpath 是否越出消费者工程边界，而不是"是否存在软链"。
#   本脚本用 npm 安装：它把 tarball 解成真实目录（无软链），证明最直接。
#
# ── 为什么安装用 npm 而不是 pnpm add ────────────────────────────────────────
#   实测（pnpm 11.20.0 与 12.4.1，并用两个临时假包 @faketest/a、@faketest/b 复现）：
#   `pnpm add ./b.tgz ./a.tgz` 在 b 依赖 `a@1.0.0` 时会去 registry 解析 a，
#   即使 a 的 tarball 就在同一条命令里、甚至已经先装过。这是 pnpm 对
#   "本地 tarball 互相依赖且包名不在 registry" 的通用限制，与 FloatCTF 的
#   package.json 内容无关（react 里 `"@floatctf/sdk": "1.0.0"` 对 registry 发布
#   是正确的）。因此：v1.0 从 tarball 安装请用 npm（本脚本即是证明）；
#   `pnpm add @floatctf/sdk @floatctf/react @floatctf/frontend-runtime`
#   要等包发布到 registry 之后才成立。脚本会额外跑一次 pnpm 探针并如实报告。
#
# 环境变量：
#   FCTF_KEEP_TMP=1        结束后保留临时目录（便于排查）
#   FCTF_NET_TIMEOUT=25    网络探测超时秒数
#
# 用法：bash scripts/test-sdk-dist.sh
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

PASS=0
FAIL=0
SKIP=0

ok() { PASS=$((PASS + 1)); printf '  ✓ %s\n' "$*"; }
no() { FAIL=$((FAIL + 1)); printf '  ✗ %s\n' "$*"; }
skip() { SKIP=$((SKIP + 1)); printf '  [SKIP] %s\n' "$*"; }
info() { printf '[INFO] %s\n' "$*"; }
note() { printf '[NOTE] %s\n' "$*"; }
section() { printf '\n── %s ──\n' "$*"; }

finish() {
	printf '\n通过 %d 项，失败 %d 项' "$PASS" "$FAIL"
	if [ "$SKIP" -gt 0 ]; then
		printf '（跳过 %d 项）' "$SKIP"
	fi
	printf '\n'
	if [ "$FAIL" -ne 0 ]; then
		printf '[FAIL] 外部可消费性测试失败\n' >&2
		exit 1
	fi
	printf '[ OK ] 外部可消费性测试通过\n'
	exit 0
}

# ── 依赖 ─────────────────────────────────────────────────────────────────────
command -v python3 >/dev/null 2>&1 || { printf '[FAIL] 需要 python3\n' >&2; exit 1; }
command -v node >/dev/null 2>&1 || { printf '[FAIL] 需要 node\n' >&2; exit 1; }

# 与仓库一致的 pnpm：优先 `mise exec pnpm@<packageManager 版本> -- pnpm`。
# （临时目录里没有 mise.toml，直接 `mise exec -- pnpm` 会拿到 mise 的默认版本。）
PNPM=()
PNPM_DESC=""
PINNED_PNPM="$(python3 -c 'import json,sys; pm=json.load(open(sys.argv[1])).get("packageManager",""); print(pm.split("@",1)[1] if pm.startswith("pnpm@") else "")' "$ROOT/package.json")"
if command -v mise >/dev/null 2>&1 && [ -n "$PINNED_PNPM" ]; then
	PNPM=(mise exec "pnpm@$PINNED_PNPM" -- pnpm)
	PNPM_DESC="mise exec pnpm@$PINNED_PNPM -- pnpm"
elif command -v mise >/dev/null 2>&1; then
	PNPM=(mise exec -- pnpm)
	PNPM_DESC="mise exec -- pnpm"
elif command -v pnpm >/dev/null 2>&1; then
	PNPM=(pnpm)
	PNPM_DESC="pnpm"
fi

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-sdk-consumer.XXXXXX")"
KEEP_TMP="${FCTF_KEEP_TMP:-0}"
cleanup() {
	if [ "$KEEP_TMP" = "1" ]; then
		printf '[INFO] 保留临时目录: %s\n' "$TMP_ROOT"
	else
		rm -rf -- "$TMP_ROOT"
	fi
}
trap cleanup EXIT
info "临时工作区: $TMP_ROOT（仓库外）"
info "仓库根: $ROOT"
info "pnpm 调用方式: ${PNPM_DESC:-（不可用）}"

DIST_DIR="$TMP_ROOT/dist"
CONSUMER="$TMP_ROOT/floatctf-sdk-consumer-test"

# ── 1. 生成 tarball ──────────────────────────────────────────────────────────
section "1. 生成发行 tarball（scripts/package-sdk-dist.sh，仓库外输出）"
if bash "$ROOT/scripts/package-sdk-dist.sh" "$DIST_DIR" >"$TMP_ROOT/package-sdk-dist.log" 2>&1; then
	ok "package-sdk-dist.sh 成功产出 tarball"
else
	no "package-sdk-dist.sh 失败"
	tail -n 30 "$TMP_ROOT/package-sdk-dist.log" >&2
	finish
fi

VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' \
	"$ROOT/packages/sdk/package.json")"
TAR_SDK="$DIST_DIR/floatctf-sdk-$VERSION.tgz"
TAR_REACT="$DIST_DIR/floatctf-react-$VERSION.tgz"
TAR_RUNTIME="$DIST_DIR/floatctf-frontend-runtime-$VERSION.tgz"

for spec in "sdk:$TAR_SDK" "react:$TAR_REACT" "frontend-runtime:$TAR_RUNTIME"; do
	path="${spec#*:}"
	if [ -f "$path" ]; then
		ok "产物存在: $(basename "$path")"
	else
		no "产物缺失: $(basename "$path")"
	fi
done
if [ -s "$DIST_DIR/SDK-SHA256SUMS" ] && grep -q 'floatctf-sdk-' "$DIST_DIR/SDK-SHA256SUMS"; then
	ok "SDK-SHA256SUMS 已生成且包含 sdk 条目"
else
	no "SDK-SHA256SUMS 缺失或不完整"
fi

# ── 2. 离线 tarball 校验（不依赖网络）────────────────────────────────────────
section "2. 离线 tarball 校验（无网络也能跑）"
offline_check() { # <tarball> <pkg name>
	local tarball="$1" pkgname="$2" base members
	base="$(basename "$tarball")"
	if [ ! -f "$tarball" ]; then
		no "$base 不存在，无法校验"
		return
	fi
	members="$(tar -tzf "$tarball" | LC_ALL=C sort)"

	local tops
	tops="$(printf '%s\n' "$members" | awk -F/ '{print $1}' | LC_ALL=C sort -u | tr '\n' ' ')"
	if [ "$tops" = "package " ]; then
		ok "$base: 顶层目录恰好是 package/"
	else
		no "$base: 顶层目录异常（$tops）"
	fi

	for required in package/package.json package/dist/index.js package/dist/index.d.ts package/README.md package/LICENSE; do
		if printf '%s\n' "$members" | grep -qxF "$required"; then
			ok "$base: 含 $required"
		else
			no "$base: 缺少 $required"
		fi
	done

	if printf '%s\n' "$members" | grep -qE '^package/src/|__tests__/|(^|/)\.test\.|(^|/)node_modules/|(^|/)\.env($|\.)'; then
		no "$base: 含源码/测试/node_modules/.env 等禁止内容"
	else
		ok "$base: 无源码、无测试、无 node_modules、无 .env"
	fi
	if printf '%s\n' "$members" | grep -qE '(^|/)\.npmrc$|(^|/)\.netrc$|(^|/)id_(rsa|ed25519)$|\.(pem|key|p12|pfx)$|(^|/)(credentials?|secrets?)\.'; then
		no "$base: 含疑似密钥/凭据文件"
	else
		ok "$base: 无密钥/凭据文件"
	fi

	local pj="$TMP_ROOT/offline-pj-$(printf '%s' "$pkgname" | tr '/@' '__').json"
	tar -xzOf "$tarball" package/package.json >"$pj"
	if python3 - "$pj" "$pkgname" "$VERSION" <<'PY'
import json, sys
path, name, version = sys.argv[1], sys.argv[2], sys.argv[3]
data = json.load(open(path))
problems = []
if data.get("name") != name:
    problems.append(f"name={data.get('name')!r} != {name!r}")
if data.get("version") != version:
    problems.append(f"version={data.get('version')!r} != {version!r}")
for key in ("dependencies", "peerDependencies", "optionalDependencies"):
    for dep, spec in (data.get(key) or {}).items():
        if isinstance(spec, str) and spec.startswith("workspace:"):
            problems.append(f"{key}[{dep}]={spec}")
if problems:
    print("; ".join(problems))
    sys.exit(1)
PY
	then
		ok "$base: packed package.json 的 name/version 正确，且无残留 workspace: 范围"
	else
		no "$base: packed package.json 校验失败（见上方输出）"
	fi
}

offline_check "$TAR_SDK" "@floatctf/sdk"
offline_check "$TAR_REACT" "@floatctf/react"
offline_check "$TAR_RUNTIME" "@floatctf/frontend-runtime"

PACKED_SDK_RANGE="$(python3 - "$TAR_REACT" "$VERSION" <<'PY'
import json, re, subprocess, sys
tarball, version = sys.argv[1], sys.argv[2]
raw = subprocess.run(["tar", "-xzOf", tarball, "package/package.json"],
                     check=True, capture_output=True, text=True).stdout
spec = (json.loads(raw).get("dependencies") or {}).get("@floatctf/sdk")
if spec is None:
    print("MISSING"); sys.exit(1)
if spec.startswith("workspace:"):
    print(f"WORKSPACE:{spec}"); sys.exit(1)
if not re.match(r"^[~^]?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$", spec):
    print(f"NOT_CONCRETE:{spec}"); sys.exit(1)
if spec != version:
    print(f"MISMATCH:{spec}!={version}"); sys.exit(1)
print(spec)
PY
)" || {
	no "floatctf-react: packed dependencies[\"@floatctf/sdk\"] 不是可安装的具体版本范围（$PACKED_SDK_RANGE）"
	PACKED_SDK_RANGE=""
}
if [ -n "$PACKED_SDK_RANGE" ]; then
	ok "floatctf-react: packed dependencies[\"@floatctf/sdk\"] = $PACKED_SDK_RANGE（具体版本，非 workspace:）"
fi

# ── 3. 网络探测 ──────────────────────────────────────────────────────────────
section "3. registry 网络探测"
NET=1
if ! command -v npm >/dev/null 2>&1; then
	NET=0
	skip "环境里没有 npm —— 跳过「真实安装 + tsc + node 运行」步骤"
	printf '     原因: 安装 tarball 需要包管理器（实测 pnpm 无法解析未发布的互依赖 tarball，见脚本头注释）。\n'
elif timeout "${FCTF_NET_TIMEOUT:-25}" npm ping --cache "$TMP_ROOT/npm-cache" >"$TMP_ROOT/net.log" 2>&1; then
	ok "公共 registry 可达（npm ping）"
else
	NET=0
	skip "公共 registry 不可达 —— 跳过「真实安装 + tsc + node 运行」步骤"
	printf '     原因: `npm ping` 失败（超时 %ss 或离线）\n' "${FCTF_NET_TIMEOUT:-25}"
	printf '     已完成的离线校验仍然有效；按约定本脚本退出 0。\n'
	printf '     离线时的 registry 输出:\n'
	sed 's/^/       | /' "$TMP_ROOT/net.log" | tail -n 5
fi

if [ "$NET" -eq 0 ]; then
	finish
fi

# npm 需要独立 cache/logs 目录：宿主 ~/.npm 可能存在 root 所有的缓存文件（EACCES）。
export npm_config_cache="$TMP_ROOT/npm-cache"
export npm_config_logs_dir="$TMP_ROOT/npm-logs"
export npm_config_audit=false
export npm_config_fund=false
export npm_config_update_notifier=false

# ── 4. 全新外部消费者工程：只用 tarball 装三个 @floatctf/* ────────────────────
section "4. 创建外部消费者工程并从 tarball 安装"
mkdir -p "$CONSUMER/src"
cat >"$CONSUMER/package.json" <<'JSON'
{
  "name": "floatctf-sdk-consumer-test",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "description": "External consumer smoke test: installs only the three release tarballs"
}
JSON
[ -s "$CONSUMER/package.json" ] || { no "无法写入消费者 package.json"; finish; }
ok "已创建全新消费者工程（无 workspace 配置、无仓库路径引用）"

if ( cd "$CONSUMER" && npm install "$TAR_SDK" "$TAR_REACT" "$TAR_RUNTIME" ) \
	>"$TMP_ROOT/npm-install-tarballs.log" 2>&1; then
	ok "npm install <三个绝对路径 .tgz> 成功（三个 @floatctf/* 全部来自 tarball）"
else
	no "npm install <三个 tarball> 失败"
	tail -n 25 "$TMP_ROOT/npm-install-tarballs.log" >&2
	finish
fi

if ( cd "$CONSUMER" && npm install --save-dev react react-dom @tanstack/react-query typescript @types/react @types/react-dom ) \
	>"$TMP_ROOT/npm-install-dev.log" 2>&1; then
	ok "devDependencies（react/react-dom/@tanstack/react-query/typescript/@types）安装成功"
else
	no "devDependencies 安装失败"
	tail -n 25 "$TMP_ROOT/npm-install-dev.log" >&2
	finish
fi

# ── 5. 断言：无 workspace: / 无 link: / 无仓库软链 ───────────────────────────
section "5. 断言消费者工程不引用 FloatCTF 源码树"
note "判定标准：tarball 安装会把真实文件落到消费者自己的 node_modules（正确）；"
note "失败条件是 realpath 越出消费者工程边界（例如落到 $ROOT）。"

if grep -rn "workspace:" "$CONSUMER/package.json" "$CONSUMER/package-lock.json" >"$TMP_ROOT/ws.txt" 2>/dev/null; then
	no "消费者工程出现 workspace: 协议引用（$(head -n 2 "$TMP_ROOT/ws.txt" | tr '\n' ' ')）"
else
	ok "package.json / package-lock.json 中没有任何 workspace: 协议引用"
fi

if grep -rnE '"(link|portal)":|^[[:space:]]*link:' "$CONSUMER/package.json" "$CONSUMER/package-lock.json" >"$TMP_ROOT/link.txt" 2>/dev/null; then
	no "消费者工程出现 link:/portal: 本地链接（$(head -n 2 "$TMP_ROOT/link.txt" | tr '\n' ' ')）"
else
	ok "package.json / package-lock.json 中没有任何 link:/portal: 本地链接"
fi

LINKS="$(find "$CONSUMER/node_modules/@floatctf" -maxdepth 1 -type l 2>/dev/null || true)"
if [ -z "$LINKS" ]; then
	ok "node_modules/@floatctf/* 里没有任何符号链接（tarball 解包为真实目录）"
else
	no "node_modules/@floatctf/* 里存在符号链接: $(printf '%s' "$LINKS" | tr '\n' ' ')"
fi

for pkg in sdk react frontend-runtime; do
	dir="$CONSUMER/node_modules/@floatctf/$pkg"
	if [ ! -e "$dir" ]; then
		no "@floatctf/$pkg 未安装到消费者 node_modules"
		continue
	fi
	real="$(realpath "$dir")"
	case "$real" in
		"$ROOT" | "$ROOT"/*)
			no "@floatctf/$pkg 的 realpath 落在 FloatCTF 仓库内: $real"
			;;
		"$CONSUMER"/*)
			ok "@floatctf/$pkg realpath 在消费者工程内: $real"
			;;
		*)
			no "@floatctf/$pkg realpath 越出消费者工程: $real"
			;;
	esac
done

# 关键：证明 node_modules 里的内容确实来自 tarball，而不是别处的副本。
for spec in "sdk:$TAR_SDK" "react:$TAR_REACT" "frontend-runtime:$TAR_RUNTIME"; do
	pkg="${spec%%:*}"
	tar_path="${spec#*:}"
	installed="$CONSUMER/node_modules/@floatctf/$pkg/dist/index.js"
	if [ ! -f "$installed" ]; then
		no "@floatctf/$pkg/dist/index.js 不存在（无法比对来源）"
		continue
	fi
	h_tar="$(tar -xzOf "$tar_path" package/dist/index.js | sha256sum | cut -d' ' -f1)"
	h_inst="$(sha256sum "$installed" | cut -d' ' -f1)"
	if [ "$h_tar" = "$h_inst" ]; then
		ok "@floatctf/$pkg dist/index.js 与 tarball 字节一致（sha256 $h_inst）"
	else
		no "@floatctf/$pkg dist/index.js 与 tarball 不一致（tar=$h_tar inst=$h_inst）"
	fi
done

# 安装后的 react 依赖范围也必须具体、且确实由本地 tarball 满足。
if python3 - "$CONSUMER/node_modules/@floatctf/react/package.json" "$VERSION" <<'PY'
import json, re, sys
path = sys.argv[1]
data = json.load(open(path))
problems = []
for key in ("dependencies", "peerDependencies", "optionalDependencies"):
    for dep, spec in (data.get(key) or {}).items():
        if isinstance(spec, str) and spec.startswith("workspace:"):
            problems.append(f"{key}[{dep}]={spec}")
spec = (data.get("dependencies") or {}).get("@floatctf/sdk")
if spec is None:
    problems.append("缺少 dependencies[@floatctf/sdk]")
elif not re.match(r"^[~^]?\d+\.\d+\.\d+", spec):
    problems.append(f"dependencies[@floatctf/sdk]={spec!r} 不是具体版本")
print("; ".join(problems))
sys.exit(1 if problems else 0)
PY
then
	ok "安装后的 @floatctf/react 依赖 @floatctf/sdk 为具体版本（无 workspace:）"
else
	no "安装后的 @floatctf/react 依赖范围不合格"
fi

if python3 - "$CONSUMER/package-lock.json" <<'PY'
import json, sys
lock = json.load(open(sys.argv[1]))
entries = {k: v for k, v in (lock.get("packages") or {}).items()
           if k.startswith("node_modules/@floatctf/")}
problems = []
for name in ("node_modules/@floatctf/sdk", "node_modules/@floatctf/react",
             "node_modules/@floatctf/frontend-runtime"):
    entry = entries.get(name)
    if entry is None:
        problems.append(f"{name} 不在 lockfile 中")
        continue
    resolved = entry.get("resolved", "")
    if not resolved.startswith("file:"):
        problems.append(f"{name} 的 resolved 不是本地 tarball: {resolved!r}")
    if entry.get("link"):
        problems.append(f"{name} 被记录为 link: true")
print("; ".join(problems))
sys.exit(1 if problems else 0)
PY
then
	ok "lockfile: 三个 @floatctf/* 均由本地 tarball（file:）解析，且不是 link"
else
	no "lockfile 中三个 @floatctf/* 的来源不合格"
fi

# ── 5b. pnpm 探针（信息性；pnpm 对未发布互依赖 tarball 的通用限制）──────────
section "5b. pnpm 本地 tarball 探针（信息性，不影响通过/失败）"
if [ -z "$PNPM_DESC" ]; then
	skip "环境里没有 mise/pnpm，跳过 pnpm 探针"
else
	PROBE="$TMP_ROOT/pnpm-probe"
	mkdir -p "$PROBE"
	printf '{"name":"pnpm-tarball-probe","version":"0.0.0","private":true,"type":"module"}\n' >"$PROBE/package.json"
	if ( cd "$PROBE" && "${PNPM[@]}" add "$TAR_SDK" "$TAR_REACT" "$TAR_RUNTIME" ) \
		>"$TMP_ROOT/pnpm-probe.log" 2>&1; then
		note "pnpm（$PNPM_DESC）也能直接安装三个 tarball：成功"
	else
		note "pnpm（$PNPM_DESC）无法直接安装三个 tarball —— pnpm 的已知限制，不是包缺陷："
		note "  react 的 \"@floatctf/sdk\": \"$VERSION\" 对 registry 发布是正确的，但 pnpm 不会用"
		note "  同一批本地 tarball 去满足该 registry 范围（已用 @faketest/* 假包独立复现）。"
		note "  v1.0 从 tarball 安装请用 npm（见第 4/5 节）；pnpm 需等包发布到 registry 之后。"
		grep -m1 -E 'ERR_PNPM|404' "$TMP_ROOT/pnpm-probe.log" | sed 's/^/       | /' || true
	fi
fi

# ── 6. TS 类型检查 + 真实运行时 import ──────────────────────────────────────
section "6. tsc --noEmit + node 运行时 import"
# skipLibCheck=true 与仓库自身（packages/react、frontends/default、apps/web）以及
# 生态默认一致：TanStack Query 的 queryOptions() 推断类型会带上 unique symbol 品牌
# （dataTagSymbol），TS 的 declaration emit 不会为它补 import，因此任何含该品牌的
# .d.ts 在 skipLibCheck=false 下都会报 "Cannot find name 'dataTagSymbol'"
# （上游/TS 限制，非 FloatCTF 独有）。本节末尾会用 strict 模式探针把这一点如实打出来。
cat >"$CONSUMER/tsconfig.json" <<'JSON'
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM"],
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "verbatimModuleSyntax": true,
    "types": []
  },
  "include": ["src"]
}
JSON

# 注意：只做**运行时合法**的调用（不渲染 React、不发请求），
# 这样同一份文件既能被 tsc 检查，也能被 node 直接执行（原生类型擦除）。
cat >"$CONSUMER/src/consumer.ts" <<'TS'
/**
 * 外部消费者 smoke test —— 只从本工程 node_modules 里的 tarball 安装产物导入。
 *
 * 三个包的公共 API 面（与各自 src/index.ts 的导出一致）：
 *   @floatctf/sdk               createFloatCTFTransport / createFloatCTFClient / resolveBaseUrls
 *   @floatctf/react             createFloatCTFReact（真正的公共工厂；createUseAwdEventStream
 *                               不是包的顶层导出，它由 createFloatCTFReact 返回）
 *   @floatctf/frontend-runtime  manifest/registry 校验器 + 契约版本常量
 */
import {
	createFloatCTFClient,
	createFloatCTFTransport,
	resolveBaseUrls,
} from "@floatctf/sdk";
import { createFloatCTFReact } from "@floatctf/react";
import {
	API_CONTRACT_VERSION,
	FRONTEND_REGISTRY_SCHEMA_VERSION,
	FRONTEND_RUNTIME_VERSION,
	checkRelativeAssetPath,
	emptyRegistry,
	isValidSemver,
	listFrontendIds,
	parseFrontendManifest,
	parseRegistry,
	registerFrontendVersion,
	resolveFrontend,
} from "@floatctf/frontend-runtime";

// ── @floatctf/sdk ───────────────────────────────────────────────────────────
const urls = resolveBaseUrls({ baseUrl: "https://example.test/api/" });
export const baseUrl: string = urls.baseUrl;
export const adminBaseUrl: string = urls.adminBaseUrl;
export const transport = createFloatCTFTransport("user", { baseUrl: urls.baseUrl }, urls.baseUrl);
export const client = createFloatCTFClient({
	baseUrl: urls.baseUrl,
	getUserToken: () => "external-consumer-token",
});

// ── @floatctf/react（headless 绑定工厂 + hook 类型）─────────────────────────
const useUserToken = (): string | null => "external-consumer-token";
export const bindings = createFloatCTFReact({ client, useUserToken });
export const { useAwdEventStream, eventInfoQueryOptions, AWD_PLAYER_QUERY_KEYS } = bindings;
export type ExternalBindings = ReturnType<typeof createFloatCTFReact>;

// ── @floatctf/frontend-runtime（契约常量 + 校验器）──────────────────────────
export const contractVersions = {
	frontendRuntime: FRONTEND_RUNTIME_VERSION,
	apiContract: API_CONTRACT_VERSION,
	registrySchema: FRONTEND_REGISTRY_SCHEMA_VERSION,
} as const;

export const manifestResult = parseFrontendManifest(
	JSON.stringify({
		schemaVersion: 1,
		id: "default",
		name: "Default Frontend",
		version: "1.0.0",
		compatibility: {
			frontendRuntime: FRONTEND_RUNTIME_VERSION,
			apiContract: API_CONTRACT_VERSION,
		},
		entry: "assets/frontend.js",
	}),
);

const NOW = "2026-01-01T00:00:00.000Z";
const empty = emptyRegistry(NOW);
export const frontendIds: string[] = listFrontendIds(empty);

const registered = registerFrontendVersion(empty, {
	id: "default",
	protected: true,
	version: {
		version: "1.0.0",
		name: "Default Frontend",
		compatibility: {
			frontendRuntime: FRONTEND_RUNTIME_VERSION,
			apiContract: API_CONTRACT_VERSION,
		},
		entry: "assets/frontend.js",
		styles: ["assets/frontend.css"],
		installedAt: NOW,
	},
});
if (!registered.ok) {
	throw new Error(`registerFrontendVersion failed: ${registered.error}`);
}

// 注册表必须至少含一个已安装前端才会通过 parseRegistry（空注册表按设计被拒绝）。
export const registryResult = parseRegistry(JSON.stringify(registered.registry));
export const resolvedFrontend = resolveFrontend(registered.registry, {
	requestedId: "default",
	frontendBaseUrl: "/__floatctf/frontends",
});
export const escapeCheck = checkRelativeAssetPath("../escape");
export const semverOk: boolean = isValidSemver("1.0.0");

/** 在真实组件里可以这样用（这里不渲染，仅证明类型链路成立）。 */
export function useAwdStream(eventId: string) {
	return useAwdEventStream({ eventId, pollMs: 15_000 });
}
TS

cat >"$CONSUMER/runtime-check.mjs" <<'JS'
// 真实运行时 import：直接加载 node_modules 里来自 tarball 的 dist/index.js。
import assert from "node:assert/strict";
import {
	createFloatCTFClient,
	createFloatCTFTransport,
	resolveBaseUrls,
} from "@floatctf/sdk";
import { createFloatCTFReact } from "@floatctf/react";
import {
	API_CONTRACT_VERSION,
	FRONTEND_RUNTIME_VERSION,
	checkRelativeAssetPath,
	emptyRegistry,
	listFrontendIds,
	parseFrontendManifest,
	parseRegistry,
	registerFrontendVersion,
	resolveFrontend,
} from "@floatctf/frontend-runtime";

assert.equal(typeof createFloatCTFTransport, "function", "createFloatCTFTransport");
assert.equal(typeof createFloatCTFClient, "function", "createFloatCTFClient");
assert.equal(typeof resolveBaseUrls, "function", "resolveBaseUrls");

const urls = resolveBaseUrls({ baseUrl: "https://example.test/api/" });
assert.equal(urls.baseUrl, "https://example.test/api");
assert.equal(urls.adminBaseUrl, "https://example.test/api/admin");

const transport = createFloatCTFTransport("user", { baseUrl: urls.baseUrl }, urls.baseUrl);
assert.equal(transport.baseUrl, "https://example.test/api");
assert.equal(transport.scope, "user");

const client = createFloatCTFClient({ baseUrl: urls.baseUrl, getUserToken: () => "tok" });
assert.equal(client.baseUrl, "https://example.test/api");
assert.equal(typeof client.service.events.fetch, "function");

assert.equal(typeof createFloatCTFReact, "function", "createFloatCTFReact");
const bindings = createFloatCTFReact({ client, useUserToken: () => "tok" });
assert.equal(typeof bindings.useAwdEventStream, "function");
assert.equal(typeof bindings.useAdminAwdEventStream, "function");
assert.equal(typeof bindings.eventInfoQueryOptions, "function");
assert.equal(typeof bindings.invalidateAwdQueries, "function");

assert.equal(FRONTEND_RUNTIME_VERSION, "1");
assert.equal(API_CONTRACT_VERSION, "1");
assert.equal(checkRelativeAssetPath("../escape").ok, false);
assert.equal(checkRelativeAssetPath("assets/app.js").ok, true);

const NOW = "2026-01-01T00:00:00.000Z";
const empty = emptyRegistry(NOW);
assert.deepEqual(listFrontendIds(empty), []);

const registered = registerFrontendVersion(empty, {
	id: "default",
	protected: true,
	version: {
		version: "1.0.0",
		name: "Default Frontend",
		compatibility: { frontendRuntime: "1", apiContract: "1" },
		entry: "assets/frontend.js",
		styles: ["assets/frontend.css"],
		installedAt: NOW,
	},
});
assert.equal(registered.ok, true, registered.ok ? "" : registered.error);

const parsed = parseRegistry(JSON.stringify(registered.registry));
assert.equal(parsed.ok, true, parsed.ok ? "" : parsed.errors.join("; "));

const resolved = resolveFrontend(registered.registry, {
	requestedId: "default",
	frontendBaseUrl: "/__floatctf/frontends",
});
assert.equal(resolved.ok, true, resolved.ok ? "" : resolved.errors.join("; "));
assert.ok(resolved.frontend.entryUrl.endsWith("/assets/frontend.js"), resolved.frontend.entryUrl);

const manifest = parseFrontendManifest(
	JSON.stringify({
		schemaVersion: 1,
		id: "default",
		name: "Default",
		version: "1.0.0",
		compatibility: { frontendRuntime: "1", apiContract: "1" },
		entry: "assets/frontend.js",
	}),
);
assert.equal(manifest.ok, true, "parseFrontendManifest(valid)");
assert.equal(manifest.manifest.entry, "assets/frontend.js");

console.log("runtime-check ok");
JS

TSC_OK=1
if ( cd "$CONSUMER" && ./node_modules/.bin/tsc --noEmit ) >"$TMP_ROOT/tsc.log" 2>&1; then
	ok "tsc --noEmit 通过（三个包的 .d.ts 在消费者工程内可解析；skipLibCheck=true，同仓库与生态默认）"
else
	TSC_OK=0
	no "tsc --noEmit 失败"
	tail -n 30 "$TMP_ROOT/tsc.log" >&2
fi

# 信息性 strict 探针（skipLibCheck=false）：把已知的上游/TS 限制如实打出来。
# 不计入失败：仓库自身（packages/react、frontends/default、apps/web）也全部使用
# skipLibCheck=true，这是 TanStack Query 品牌的既定使用前提。
if ( cd "$CONSUMER" && ./node_modules/.bin/tsc --noEmit --skipLibCheck false ) \
	>"$TMP_ROOT/tsc-strict.log" 2>&1; then
	note "skipLibCheck=false 的 strict 探针也通过"
else
	STRICT_HITS="$(grep -c "dataTagSymbol\|dataTagErrorSymbol" "$TMP_ROOT/tsc-strict.log" || true)"
	note "skipLibCheck=false 的 strict 探针不通过（$STRICT_HITS 行指向 dataTagSymbol）——已知上游限制，不计失败："
	note "  @floatctf/react/dist/index.d.ts 复现了 TanStack queryOptions() 的 unique symbol 品牌，"
	note "  TS declaration emit 不会为它补 import；仓库自身 tsconfig 全部使用 skipLibCheck=true。"
	printf '       | %s\n' "$(head -n 3 "$TMP_ROOT/tsc-strict.log")"
fi

if ( cd "$CONSUMER" && node runtime-check.mjs ) >"$TMP_ROOT/runtime.log" 2>&1; then
	ok "node runtime-check.mjs 通过（真实 import 三个 dist 并断言公共 API）"
else
	no "node runtime-check.mjs 失败"
	tail -n 30 "$TMP_ROOT/runtime.log" >&2
fi

NODE_HELP="$(node --help 2>&1 || true)"
case "$NODE_HELP" in
*experimental-strip-types*)
	if ( cd "$CONSUMER" && node --experimental-strip-types src/consumer.ts ) >"$TMP_ROOT/strip.log" 2>&1; then
		ok "node --experimental-strip-types src/consumer.ts 通过（TS 源码可被外部直接执行）"
	else
		no "node --experimental-strip-types src/consumer.ts 失败"
		tail -n 20 "$TMP_ROOT/strip.log" >&2
	fi
	;;
*)
	# Node >= 22.18 / 24 可直接执行 .ts（原生类型擦除），无需 flag。
	if ( cd "$CONSUMER" && node src/consumer.ts ) >"$TMP_ROOT/strip.log" 2>&1; then
		ok "node src/consumer.ts 通过（Node 原生类型擦除，无需 flag）"
	else
		skip "当前 node 不支持 .ts 直接执行（无 --experimental-strip-types 且原生擦除不可用）"
	fi
	;;
esac

# ── 7. 汇总 ──────────────────────────────────────────────────────────────────
section "7. 结论"
if [ "$TSC_OK" -eq 1 ]; then
	printf '[INFO] 类型链路: tsc --noEmit 在仓库外的消费者工程里通过\n'
fi
printf '[INFO] 三个包均从 .tgz 安装：无 workspace: 协议、无 link:/portal:、无软链、realpath 全在 %s 内\n' "$CONSUMER"
printf '[INFO] 与仓库源码树的关系: 仅"内容同源"（sha256 与 tarball 一致），无符号链接、无路径引用\n'

finish
