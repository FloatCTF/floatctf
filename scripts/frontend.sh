#!/usr/bin/env bash
#
# FloatCTF 前端管理器（frontend manager）—— 生产运维与开发共用的单一入口。
#
# 用途：把"一个前端"安装到 `$FLOATCTF_HOME/frontends/`，并原子更新 `registry.json`，
# 使浏览器 bootstrap（apps/web）能够解析并加载它。
#
# 设计约束（见 docs/frontend/ARCHITECTURE.md / ARTIFACT.md）：
#   1. 本脚本必须能在**没有 FloatCTF 源码签出**的机器上独立运行：
#      只依赖宿主既有工具（bash / tar / python3 / git（仅 Git 安装时需要）/ docker（仅源码构建时需要））。
#   2. **绝不**在宿主上直接执行第三方 `pnpm/npm/yarn install` 或 `build`。
#      源码安装一律在**隔离 Docker 构建容器**内完成（cap-drop ALL、无 Docker socket、
#      不挂载宿主目录除只读源码与临时产物目录）。
#   3. 注册表更新是**原子**的（tmp + rename + flock），且绝不覆盖已安装的版本目录
#      （前端资产不可变；升级 = 装新版本 + 移动 current 指针）。
#   4. `default` 前端由平台发布并保护：常规管理不得覆盖或删除它（install.sh 用
#      `--platform` 内部开关安装它）。
#   5. 激活前端是**应用设置**（FRONTEND_ACTIVE，管理端 UI 修改），本脚本不碰它。
#
# 用法：frontend.sh <命令> [参数]；`frontend.sh help` 查看全部。
set -Eeuo pipefail

FLOATCTF_HOME="${FLOATCTF_HOME:-/var/lib/floatctf}"
FRONTENDS_ROOT="${FRONTENDS_ROOT:-$FLOATCTF_HOME/frontends}"
REGISTRY="$FRONTENDS_ROOT/registry.json"
REGISTRY_LOCK="$FRONTENDS_ROOT/.registry.lock"
DEFAULT_FRONTEND_ID="default"
# 与 packages/frontend-runtime/src/version.ts 保持一致（契约 major）。
FRONTEND_RUNTIME_CONTRACT="1"
API_CONTRACT="1"
REGISTRY_SCHEMA_VERSION="1"
# 源码构建的 Node 基线：与平台自身的开发基线（mise.toml 的 node = 26.5.1）同大版本，
# 避免"平台能构建、外部前端构建不了"这类隐性差异。可用 --node-image 覆盖。
DEFAULT_NODE_IMAGE="node:26-bookworm"
# 源码构建超时（秒）。
BUILD_TIMEOUT_SECS="${FRONTEND_BUILD_TIMEOUT_SECS:-1800}"
# pnpm 基线：与平台自身使用的版本一致（root package.json 的 packageManager）。
# 源码若声明了自己的 `packageManager`，以它为准（见 package_manager_version）。
DEFAULT_PNPM_VERSION="11.20.0"

C_INFO=""; C_OK=""; C_WARN=""; C_ERR=""; C_END=""
if [ -t 1 ]; then
    C_INFO="$(tput setaf 4 2>/dev/null || true)"
    C_OK="$(tput setaf 2 2>/dev/null || true)"
    C_WARN="$(tput setaf 3 2>/dev/null || true)"
    C_ERR="$(tput setaf 1 2>/dev/null || true)"
    C_END="$(tput sgr0 2>/dev/null || true)"
fi
info() { printf '%s[INFO]%s %s\n' "$C_INFO" "$C_END" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n' "$C_OK" "$C_END" "$*"; }
warn() { printf '%s[WARN]%s %s\n' "$C_WARN" "$C_END" "$*" >&2; }
die()  { printf '%s[FAIL]%s %s\n' "$C_ERR" "$C_END" "$*" >&2; exit 1; }

TMP_DIRS=()
cleanup() {
    local rc=$?
    local d
    for d in "${TMP_DIRS[@]:-}"; do
        [ -n "$d" ] && rm -rf -- "$d" 2>/dev/null || true
    done
    exit "$rc"
}
trap cleanup EXIT INT TERM
mktmp() {
    local d
    d="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-frontend.XXXXXX")"
    TMP_DIRS+=("$d")
    printf '%s' "$d"
}

# ── 依赖 ──────────────────────────────────────────────────────────────────────

require_python() {
    command -v python3 >/dev/null 2>&1 \
        || die "缺少 python3（frontend.sh 用它做 JSON 解析/原子注册表更新；请安装 python3 后重试）"
}
require_tar() {
    command -v tar >/dev/null 2>&1 || die "缺少 tar"
}

usage() {
    cat <<'USAGE'
FloatCTF 前端管理器

用法:
  frontend.sh <命令> [参数]

命令:
  help                              显示本帮助
  list                              列出已安装的前端与版本（* 标记当前版本）
  info <id> [version]               显示某前端/某版本的详细信息
  verify <artifact.tar.gz>          只校验预构建制品，不安装
  install <source> [选项]           安装前端
  remove <id> [version]             移除前端（不给 version 则移除该 ID 全部版本）
  set-current <id> <version>        把某 ID 的当前版本指针指向已安装版本

install 的 <source> 可以是:
  - 本地前端源码目录（含 package.json 与 floatctf.frontend.json / frontend.json）
  - Git 仓库 URL（https://... / git@... / file://...）
  - 已构建的制品归档（.tar.gz，根目录含 frontend.json）

install 选项:
  --ref <git-ref>        Git 安装时检出的分支/标签/提交（默认远端默认分支）
  --node-image <image>   源码构建使用的 Node 镜像（默认 node:26-bookworm）
  --no-build             只接受源码目录里已构建好的 dist/（不做容器构建）
  --make-current         安装成功后把该 ID 的当前版本指针指向新版本
  --platform             内部开关：允许安装受保护的 default 前端（仅 install.sh 使用）
  --reinstall            平台重部署：同版本已存在时原子替换（需配合 --platform）
  --dry-run              只打印将要执行的操作
  -h, --help             显示本帮助

环境:
  FLOATCTF_HOME          安装根（默认 /var/lib/floatctf）
  FRONTENDS_ROOT         前端存储根（默认 $FLOATCTF_HOME/frontends）

说明:
  - 本脚本**不会**修改 FRONTEND_ACTIVE；激活前端请在管理端设置页操作。
  - 源码安装一律在隔离 Docker 构建容器内执行依赖安装与构建，不会在宿主直接跑
    pnpm/npm/yarn。
USAGE
}

# ── 注册表 JSON 操作（python3；规则与 packages/frontend-runtime 对齐）──────────

# registry_helper <subcommand> [...]
registry_helper() {
    python3 - "$@" <<'PY'
import json
import os
import sys
import tempfile
from pathlib import Path

SCHEMA_VERSION = 1


def fail(message: str, code: int = 1):
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(code)


def load_registry(path: str) -> dict:
    p = Path(path)
    if not p.exists():
        return {"schemaVersion": SCHEMA_VERSION, "updatedAt": "", "frontends": {}}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"registry.json 不是合法 JSON: {path}: {exc}")
    if not isinstance(data, dict):
        fail(f"registry.json 顶层必须是对象: {path}")
    data.setdefault("schemaVersion", SCHEMA_VERSION)
    data.setdefault("frontends", {})
    if not isinstance(data["frontends"], dict):
        fail("registry.json frontends 必须是对象")
    return data


def atomic_write(path: str, data) -> None:
    """同目录 tmp + fsync + rename：任何时刻读到的都是完整文件。"""
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(target.parent), prefix=".registry.", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(data, fh, ensure_ascii=False, indent=2, sort_keys=True)
            fh.write("\n")
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, target)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def cmd_list(registry_path: str) -> int:
    data = load_registry(registry_path)
    frontends = data.get("frontends", {})
    if not frontends:
        print("# 尚未安装任何前端")
        return 0
    for fid in sorted(frontends):
        entry = frontends[fid] or {}
        current = entry.get("currentVersion", "")
        protected = "protected" if entry.get("protected") else ""
        versions = entry.get("versions", {}) or {}
        for version in sorted(versions, key=lambda v: (len(v), v)):
            marker = "*" if version == current else " "
            extra = f" [{protected}]" if protected and version == current else ""
            print(f"{marker} {fid}\t{version}{extra}")
    return 0


def cmd_info(registry_path: str, fid: str, version: str | None) -> int:
    data = load_registry(registry_path)
    entry = (data.get("frontends") or {}).get(fid)
    if entry is None:
        fail(f"未安装前端: {fid}")
    versions = entry.get("versions", {}) or {}
    wanted = version or entry.get("currentVersion")
    if wanted not in versions:
        fail(f"前端 {fid} 未安装版本: {wanted}")
    meta = versions[wanted]
    compat = meta.get("compatibility", {}) or {}
    print(f"id:            {fid}")
    print(f"version:       {wanted}")
    print(f"current:       {'yes' if wanted == entry.get('currentVersion') else 'no'}"
          f" (currentVersion={entry.get('currentVersion')})")
    print(f"protected:     {'yes' if entry.get('protected') else 'no'}")
    print(f"name:          {meta.get('name', '')}")
    if meta.get("description"):
        print(f"description:   {meta['description']}")
    if meta.get("author"):
        print(f"author:        {meta['author']}")
    print(f"entry:         {meta.get('entry', '')}")
    print(f"styles:        {', '.join(meta.get('styles') or []) or '(none)'}")
    print(f"frontendRuntime: {compat.get('frontendRuntime', '')}")
    print(f"apiContract:   {compat.get('apiContract', '')}")
    print(f"installedAt:   {meta.get('installedAt', '')}")
    print(f"installedVersions: {', '.join(sorted(versions, key=lambda v: (len(v), v)))}")
    return 0


def cmd_register(registry_path: str, fid: str, version: str, manifest_path: str,
                 protected: bool, make_current: bool, source: str) -> int:
    data = load_registry(registry_path)
    try:
        manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"frontend.json 不是合法 JSON: {manifest_path}: {exc}")
    if not isinstance(manifest, dict):
        fail("frontend.json 顶层必须是对象")

    entry = data["frontends"].setdefault(fid, {
        "id": fid,
        "currentVersion": version,
        "protected": bool(protected),
        "versions": {},
    })
    versions = entry.setdefault("versions", {})
    if version in versions:
        fail(f"前端 {fid} 版本 {version} 已安装（资产不可变，拒绝覆盖）")

    record = dict(manifest)
    record["version"] = version
    record["installedAt"] = __import__("datetime").datetime.now(
        __import__("datetime").timezone.utc
    ).strftime("%Y-%m-%dT%H:%M:%SZ")
    if source:
        record["source"] = source
    versions[version] = record
    entry["id"] = fid
    if protected:
        entry["protected"] = True
    if make_current or not entry.get("currentVersion"):
        entry["currentVersion"] = version
    data["schemaVersion"] = SCHEMA_VERSION
    data["updatedAt"] = record["installedAt"]
    atomic_write(registry_path, data)
    print(f"registered {fid} {version} (current={entry['currentVersion']})")
    return 0


def cmd_touch(registry_path: str, fid: str, version: str, manifest_path: str,
              protected: bool, make_current: bool) -> int:
    """幂等注册：版本已存在时**更新记录**而不是报错（平台重部署路径）。

    与 cmd_register 的区别只在"重复版本"这一件事上：register 是首次安装的安全网
    （拒绝覆盖），touch 用于"资产已在位、只需保证注册表有条目"的场景。
    """
    data = load_registry(registry_path)
    try:
        manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"frontend.json 不是合法 JSON: {manifest_path}: {exc}")
    if not isinstance(manifest, dict):
        fail("frontend.json 顶层必须是对象")

    entry = data["frontends"].setdefault(fid, {
        "id": fid,
        "currentVersion": version,
        "protected": bool(protected),
        "versions": {},
    })
    versions = entry.setdefault("versions", {})
    existing = versions.get(version, {})
    record = dict(manifest)
    record["version"] = version
    record["installedAt"] = existing.get("installedAt") or __import__("datetime").datetime.now(
        __import__("datetime").timezone.utc
    ).strftime("%Y-%m-%dT%H:%M:%SZ")
    versions[version] = record
    entry["id"] = fid
    if protected:
        entry["protected"] = True
    if make_current or not entry.get("currentVersion"):
        entry["currentVersion"] = version
    data["schemaVersion"] = SCHEMA_VERSION
    data["updatedAt"] = record["installedAt"]
    atomic_write(registry_path, data)
    print(f"touched {fid} {version} (current={entry['currentVersion']})")
    return 0


def cmd_remove(registry_path: str, fid: str, version: str | None) -> int:
    data = load_registry(registry_path)
    frontends = data["frontends"]
    entry = frontends.get(fid)
    if entry is None:
        fail(f"未安装前端: {fid}")
    if entry.get("protected"):
        fail(f"受保护的前端不可通过前端管理移除: {fid}")
    versions = entry.get("versions", {}) or {}
    if version is None:
        removed = sorted(versions)
        del frontends[fid]
    else:
        if version not in versions:
            fail(f"前端 {fid} 未安装版本: {version}")
        del versions[version]
        removed = [version]
        if not versions:
            del frontends[fid]
        elif entry.get("currentVersion") == version:
            # 指针必须始终指向已安装版本：显式回退到剩余最新版本并打印出来。
            remaining = sorted(versions, key=lambda v: [int(p) if p.isdigit() else p
                                                        for p in v.replace("-", ".").split(".")])
            entry["currentVersion"] = remaining[-1]
            print(f"注意: currentVersion 已回退为 {entry['currentVersion']}")
    data["updatedAt"] = __import__("datetime").datetime.now(
        __import__("datetime").timezone.utc
    ).strftime("%Y-%m-%dT%H:%M:%SZ")
    atomic_write(registry_path, data)
    print(f"removed {fid} {','.join(removed) if removed else '(all)'}")
    return 0


def cmd_set_current(registry_path: str, fid: str, version: str) -> int:
    data = load_registry(registry_path)
    entry = (data.get("frontends") or {}).get(fid)
    if entry is None:
        fail(f"未安装前端: {fid}")
    if version not in (entry.get("versions") or {}):
        fail(f"前端 {fid} 未安装版本: {version}")
    entry["currentVersion"] = version
    data["updatedAt"] = __import__("datetime").datetime.now(
        __import__("datetime").timezone.utc
    ).strftime("%Y-%m-%dT%H:%M:%SZ")
    atomic_write(registry_path, data)
    print(f"{fid} currentVersion = {version}")
    return 0


def cmd_manifest_fields(manifest_path: str) -> int:
    """把 manifest 的标量字段以 shlex.quote 形式打印，供 bash 安全 eval。

    只做 JSON 结构提取；**语义规则**（安全 ID / semver / 相对路径）由 bash 侧统一检查，
    权威校验在 @floatctf/frontend-runtime 的 parseFrontendManifest / parseRegistry。
    """
    import shlex
    try:
        manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"frontend.json 不是合法 JSON: {exc}")
    if not isinstance(manifest, dict):
        fail("frontend.json 顶层必须是对象")

    def scalar(key: str, default: str = "") -> str:
        value = manifest.get(key, default)
        if value is None:
            return default
        if isinstance(value, bool):
            return "true" if value else "false"
        if isinstance(value, (int, float)):
            return str(value)
        if isinstance(value, str):
            return value
        fail(f"frontend.json: {key} 类型不受支持")

    compat = manifest.get("compatibility")
    if not isinstance(compat, dict):
        fail("frontend.json: compatibility 必须是对象")
    styles = manifest.get("styles", [])
    if styles is None:
        styles = []
    if not isinstance(styles, list):
        fail("frontend.json: styles 必须是数组")
    for style in styles:
        if not isinstance(style, str):
            fail("frontend.json: styles 元素必须是字符串")

    print(f"MF_SCHEMA={shlex.quote(scalar('schemaVersion'))}")
    print(f"MF_ID={shlex.quote(scalar('id'))}")
    print(f"MF_NAME={shlex.quote(scalar('name'))}")
    print(f"MF_VERSION={shlex.quote(scalar('version'))}")
    print(f"MF_ENTRY={shlex.quote(scalar('entry'))}")
    print(f"MF_RUNTIME={shlex.quote(str(compat.get('frontendRuntime', '')))}")
    print(f"MF_APICONTRACT={shlex.quote(str(compat.get('apiContract', '')))}")
    # 样式路径：合法路径不含空格（规则禁止），因此空格分隔是安全的。
    print(f"MF_STYLES={shlex.quote(' '.join(styles))}")
    return 0


def cmd_registry_ids(registry_path: str) -> int:
    data = load_registry(registry_path)
    for fid in sorted((data.get("frontends") or {})):
        print(fid)
    return 0


NEEDED_ARGS = {
    "list": 2,
    "info": 4,
    "register": 8,
    "touch": 8,
    "remove": 4,
    "set-current": 5,
    "manifest-fields": 3,
    "ids": 3,
}


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        fail("registry_helper: 缺少子命令")
    cmd = argv[1]
    needed = NEEDED_ARGS.get(cmd)
    if needed is not None and len(argv) < needed:
        fail(f"registry_helper {cmd}: 参数不足（需要 {needed - 2} 个，收到 {len(argv) - 2} 个）")
    if cmd == "list":
        return cmd_list(argv[2])
    if cmd == "info":
        return cmd_info(argv[2], argv[3], argv[4] if len(argv) > 4 and argv[4] else None)
    if cmd == "register":
        return cmd_register(argv[2], argv[3], argv[4], argv[5],
                            argv[6] == "true", argv[7] == "true", argv[8])
    if cmd == "touch":
        return cmd_touch(argv[2], argv[3], argv[4], argv[5],
                         argv[6] == "true", argv[7] == "true")
    if cmd == "remove":
        return cmd_remove(argv[2], argv[3], argv[4] if len(argv) > 4 and argv[4] else None)
    if cmd == "set-current":
        return cmd_set_current(argv[2], argv[3], argv[4])
    if cmd == "manifest-fields":
        return cmd_manifest_fields(argv[2])
    if cmd == "ids":
        return cmd_registry_ids(argv[2])
    fail(f"registry_helper: 未知子命令 {cmd}")


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
PY
}

# 带锁的注册表修改（读-改-写在 flock 内完成，避免并发安装互相覆盖）。
with_registry_lock() {
    mkdir -p "$FRONTENDS_ROOT"
    if command -v flock >/dev/null 2>&1; then
        exec 9>"$REGISTRY_LOCK"
        flock -w 60 9 || die "等待注册表锁超时（$REGISTRY_LOCK）"
    else
        warn "flock 不可用，注册表更新没有并发保护（请勿并发安装前端）"
    fi
    "$@"
    if command -v flock >/dev/null 2>&1; then
        exec 9>&-
    fi
}

# ── 校验规则（与 packages/frontend-runtime 的 paths.ts / manifest.ts 同源）──────

SAFE_ID_RE='^[a-z0-9][a-z0-9._-]{0,63}$'
SEMVER_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
# 相对资产路径：不含空格/反斜杠/控制字符，不以 / ~ 开头，无 . / .. 段，无 scheme。
SAFE_PATH_RE='^[A-Za-z0-9._@+-]+(/[A-Za-z0-9._@+-]+)*$'

is_safe_id() { [[ "$1" =~ $SAFE_ID_RE ]]; }
is_semver() { [[ "$1" =~ $SEMVER_RE ]]; }
is_safe_rel_path() {
    local p="$1"
    [[ -n "$p" ]] || return 1
    [[ "$p" != /* && "$p" != "~"* ]] || return 1
    [[ "$p" != *"\\"* ]] || return 1
    [[ "$p" =~ $SAFE_PATH_RE ]] || return 1
    local seg
    IFS='/' read -r -a seg <<<"$p"
    local s
    for s in "${seg[@]}"; do
        [ "$s" != "." ] && [ "$s" != ".." ] && [ -n "$s" ] || return 1
    done
    return 0
}

# validate_manifest_file <frontend.json> —— 成功时导出 MF_* 变量（eval 到调用方）。
validate_manifest_file() {
    local manifest="$1"
    [ -f "$manifest" ] || die "缺少 frontend.json: $manifest"
    eval "$(registry_helper manifest-fields "$manifest")"

    [ "$MF_SCHEMA" = "$REGISTRY_SCHEMA_VERSION" ] \
        || die "frontend.json schemaVersion 必须是 $REGISTRY_SCHEMA_VERSION（实际: $MF_SCHEMA）"
    is_safe_id "$MF_ID" \
        || die "frontend.json id 非法（须匹配 [a-z0-9][a-z0-9._-]*，最长 64）: $MF_ID"
    [ -n "$MF_NAME" ] || die "frontend.json name 不能为空"
    is_semver "$MF_VERSION" || die "frontend.json version 不是合法 semver: $MF_VERSION"
    is_safe_rel_path "$MF_ENTRY" || die "frontend.json entry 必须是安全的相对路径: $MF_ENTRY"
    [ "$MF_RUNTIME" = "$FRONTEND_RUNTIME_CONTRACT" ] \
        || die "前端要求 frontendRuntime=$MF_RUNTIME，平台为 $FRONTEND_RUNTIME_CONTRACT（契约不兼容）"
    [ "$MF_APICONTRACT" = "$API_CONTRACT" ] \
        || die "前端要求 apiContract=$MF_APICONTRACT，平台为 $API_CONTRACT（契约不兼容）"

    local style
    for style in $MF_STYLES; do
        is_safe_rel_path "$style" || die "frontend.json styles 含非法相对路径: $style"
    done
    return 0
}

# ── 归档安全解包（拒绝穿越/符号链接/绝对路径）─────────────────────────────────

# validate_tar_members <archive> —— 在解包**之前**逐条校验成员。
validate_tar_members() {
    local archive="$1"
    local listing
    listing="$(tar tzvf "$archive")" || die "无法读取归档（不是合法 tar.gz?）: $archive"

    local line type name
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        type="${line:0:1}"
        # tar tzvf 输出：权限 属主/属组 大小 日期 时间 名称（名称可能含空格）
        name="$(printf '%s' "$line" | awk '{ $1=$2=$3=$4=$5=""; sub(/^ +/, ""); print }')"
        [ -n "$name" ] || die "归档含无法解析的成员行: $line"
        case "$type" in
            -|d) ;;
            l|h) die "拒绝含符号链接/硬链接的归档: $name" ;;
            *) die "拒绝含特殊文件类型的归档成员（$type）: $name" ;;
        esac
        case "$name" in
            /*) die "拒绝绝对路径归档成员: $name" ;;
            *".."*) die "拒绝含 .. 的归档成员: $name" ;;
            *"\\"*) die "拒绝含反斜杠的归档成员: $name" ;;
        esac
    done <<<"$listing"
    return 0
}

extract_archive() { # <archive> <destdir>
    local archive="$1" dest="$2"
    validate_tar_members "$archive"
    mkdir -p "$dest"
    tar xzf "$archive" -C "$dest" --no-same-owner --no-same-permissions \
        || die "解包失败: $archive"
}

# package_manager_version <srcdir> <pm> —— 优先用源码声明的 `packageManager`（如
# `pnpm@11.20.0`），否则用平台基线。**不接受任意 shell 命令**：只解析 `name@version`。
package_manager_version() {
    local srcdir="$1" pm="$2"
    local declared=""
    declared="$(python3 - "$srcdir/package.json" "$pm" <<'PYINNER'
import json
import re
import sys
from pathlib import Path

path, pm = sys.argv[1], sys.argv[2]
try:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
except Exception:
    raise SystemExit(0)
raw = data.get("packageManager")
if not isinstance(raw, str) or "@" not in raw:
    raise SystemExit(0)
name, _, version = raw.partition("@")
if name.strip() != pm:
    raise SystemExit(0)
version = version.strip()
# 只允许 semver 形状，杜绝把任意字符串塞进安装命令
if not re.fullmatch(r"[0-9]+(\.[0-9]+){0,2}(-[0-9A-Za-z.-]+)?", version):
    raise SystemExit(0)
print(version)
PYINNER
)"
    if [ -n "$declared" ]; then
        printf '%s' "$declared"
    elif [ "$pm" = "pnpm" ]; then
        printf '%s' "$DEFAULT_PNPM_VERSION"
    else
        printf '%s' "latest"
    fi
}

# ── 源码构建（隔离容器）───────────────────────────────────────────────────────

# detect_package_manager <dir> —— 按 lockfile 判定，规则见 docs/frontend/DEVELOPING.md。
detect_package_manager() {
    local dir="$1"
    if [ -f "$dir/pnpm-lock.yaml" ]; then printf 'pnpm'; return; fi
    if [ -f "$dir/yarn.lock" ]; then printf 'yarn'; return; fi
    if [ -f "$dir/package-lock.json" ]; then printf 'npm'; return; fi
    if [ -f "$dir/package.json" ]; then printf 'npm'; return; fi
    die "源码目录既没有 package.json 也没有 lockfile: $dir"
}

# build_source_frontend <srcdir> <outdir> <node_image>
#
# 在隔离 Docker 构建容器内安装依赖并执行约定的 `build` 脚本。
# 安全措施：cap-drop ALL、no-new-privileges、不加任何宿主目录的可写挂载、
# 不挂 Docker socket、不共享宿主 PID/网络命名空间。
# 注意：隔离只降低**宿主**风险，产物仍然是可信浏览器代码（见信任模型文档）。
build_source_frontend() {
    local srcdir="$1" outdir="$2" node_image="$3"
    local pm
    pm="$(detect_package_manager "$srcdir")"
    info "源码构建：包管理器=$pm，Node 镜像=$node_image（隔离容器）"

    # Node 26 镜像已不再内置 corepack，因此用镜像自带的 npm 把包管理器装到
    # **可写前缀** /tmp/npm-global（容器以调用者身份运行，/usr/local 不可写），
    # 再前置到 PATH。版本优先取源码 `packageManager`，否则用平台基线。
    local pm_version
    pm_version="$(package_manager_version "$srcdir" "$pm")"
    local install_cmd
    case "$pm" in
        pnpm)
            info "pnpm 版本: $pm_version"
            install_cmd="npm install -g --prefix /tmp/npm-global --no-fund --no-audit pnpm@${pm_version} >/dev/null && PATH=/tmp/npm-global/bin:\$PATH pnpm install --frozen-lockfile"
            ;;
        yarn)
            info "yarn 版本: $pm_version"
            install_cmd="npm install -g --prefix /tmp/npm-global --no-fund --no-audit yarn@${pm_version} >/dev/null && PATH=/tmp/npm-global/bin:\$PATH (yarn install --immutable || yarn install --frozen-lockfile || yarn install)"
            ;;
        npm)
            install_cmd='if [ -f package-lock.json ]; then npm ci; else npm install; fi'
            ;;
        *) die "不支持的包管理器: $pm" ;;
    esac

    # 只执行**约定**的 build 脚本，绝不从 JSON 里取任意命令执行。
    local run_build
    case "$pm" in
        pnpm) run_build='pnpm run build' ;;
        yarn) run_build='yarn run build' ;;
        npm) run_build='npm run build' ;;
    esac

    mkdir -p "$outdir"
    # 以**调用者身份**在容器内运行（而非 root）：
    #   - 容器内用户对 /src 有读权限（本地源码属于调用者；sudo 场景是 root，同样成立）
    #   - 产物写回 /out 后属主就是运维本人，不需要额外 chown
    #   - 与 cap-drop ALL 相容：没有 CAP_DAC_OVERRIDE 时，root 反而可能读不到
    #     调用者的 0600 文件；用调用者身份则始终可读。
    local run_uid run_gid
    run_uid="$(id -u)"
    run_gid="$(id -g)"

    timeout "$BUILD_TIMEOUT_SECS" docker run --rm \
        --network bridge \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --pids-limit 2048 \
        --user "$run_uid:$run_gid" \
        -e "HOME=/tmp" \
        -e "NPM_CONFIG_UPDATE_NOTIFIER=false" \
        -e "CI=1" \
        -v "$srcdir:/src:ro" \
        -v "$outdir:/out" \
        -w / \
        "$node_image" \
        bash -lc "
            set -Eeuo pipefail
            # 构建目录放在 /tmp（对容器内非 root 用户可写；容器根 / 不可写）。
            rm -rf /tmp/work && mkdir -p /tmp/work
            # 只复制**源码**：排除依赖与既有构建产物，保证构建从源码出发、不复用宿主产物
            # （宿主 node_modules 可能含不同平台的原生二进制）。
            cd /src
            tar -cf - \
                --exclude=./node_modules --exclude=./.pnpm-store --exclude=./.git \
                --exclude=./dist --exclude=./build --exclude=./.cache \
                --exclude=./.turbo --exclude=./.next --exclude=./.output \
                . | (cd /tmp/work && tar -xf - --no-same-owner)
            cd /tmp/work
            if [ ! -f package.json ]; then echo 'missing package.json' >&2; exit 2; fi
            node --version
            ${install_cmd}
            # npm -g --prefix 装出来的包管理器只对本次命令生效；这里把它固定进 PATH，
            # 后续约定的 build 脚本才找得到 pnpm/yarn。
            export PATH="/tmp/npm-global/bin:\$PATH"
            ${run_build}
            rm -rf /out/* 2>/dev/null || true
            if [ -d dist ]; then
                (cd dist && tar -cf - --no-same-owner .) | (cd /out && tar -xf - --no-same-owner)
            elif [ -d build ]; then
                (cd build && tar -cf - --no-same-owner .) | (cd /out && tar -xf - --no-same-owner)
            else
                echo 'build produced neither dist/ nor build/' >&2
                exit 3
            fi
        " || die "隔离容器构建失败（镜像 $node_image）。若宿主需要代理，请为 docker 配置代理后重试。"

    if [ -z "$(ls -A "$outdir" 2>/dev/null)" ]; then
        die "构建容器没有产出任何文件"
    fi
    ok "源码构建完成（隔离容器）"
}

# ── 安装 ──────────────────────────────────────────────────────────────────────

INSTALL_NODE_IMAGE="$DEFAULT_NODE_IMAGE"
INSTALL_REF=""
INSTALL_NO_BUILD=0
INSTALL_MAKE_CURRENT=0
INSTALL_PLATFORM=0
INSTALL_REINSTALL=0
INSTALL_DRY_RUN=0

cmd_install() {
    local source=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --ref) INSTALL_REF="${2:?--ref 需要参数}"; shift 2 ;;
            --node-image) INSTALL_NODE_IMAGE="${2:?--node-image 需要参数}"; shift 2 ;;
            --no-build) INSTALL_NO_BUILD=1; shift ;;
            --make-current) INSTALL_MAKE_CURRENT=1; shift ;;
            --platform) INSTALL_PLATFORM=1; shift ;;
            --reinstall) INSTALL_REINSTALL=1; shift ;;
            --dry-run) INSTALL_DRY_RUN=1; shift ;;
            -h|--help) usage; return 0 ;;
            -*) die "未知选项: $1（见 frontend.sh help）" ;;
            *) [ -z "$source" ] || die "只接受一个 <source>"; source="$1"; shift ;;
        esac
    done
    [ -n "$source" ] || die "用法: frontend.sh install <本地目录|Git URL|制品.tar.gz> [选项]"

    require_python
    require_tar

    local stage artifact_dir kind
    stage="$(mktmp)"

    if [ -f "$source" ]; then
        kind="artifact"
        artifact_dir="$stage/artifact"
        info "安装预构建制品: $source"
        extract_archive "$source" "$artifact_dir"
        # 归档可能自带一层顶层目录（frontends/<id>/<version>/ 或 <id>/<version>/）。
        artifact_dir="$(normalize_artifact_root "$artifact_dir")"
    elif [ -d "$source" ]; then
        if [ -f "$source/frontend.json" ]; then
            # 预构建制品目录（例如 release 解包后的 frontends/<id>/<version>/）：
            # 已经是可安装形态，不再构建。
            kind="directory"
            info "安装预构建制品目录: $source"
            artifact_dir="$source"
        else
            kind="directory"
            info "安装本地前端源码目录: $source"
            prepare_from_source "$source" "$stage"
            artifact_dir="$stage/built"
        fi
    else
        kind="git"
        command -v git >/dev/null 2>&1 || die "缺少 git（Git 安装需要）"
        info "克隆并构建 Git 前端: $source ${INSTALL_REF:+（ref=$INSTALL_REF）}"
        prepare_from_git "$source" "$stage"
        artifact_dir="$stage/built"
    fi

    validate_manifest_file "$artifact_dir/frontend.json"
    local fid="$MF_ID" version="$MF_VERSION"

    if [ "$fid" = "$DEFAULT_FRONTEND_ID" ] && [ "$INSTALL_PLATFORM" != "1" ]; then
        die "default 前端由平台发布并受保护，不能用 frontend.sh install 覆盖（升级走 install.sh）"
    fi

    # 入口与样式必须真实存在（否则装上一个必然加载失败的前端）。
    [ -f "$artifact_dir/$MF_ENTRY" ] \
        || die "frontend.json 声明的 entry 不存在于制品内: $MF_ENTRY"
    local style
    for style in $MF_STYLES; do
        [ -f "$artifact_dir/$style" ] || die "frontend.json 声明的 style 不存在: $style"
    done

    if [ "$INSTALL_REINSTALL" = "1" ] && [ "$INSTALL_PLATFORM" != "1" ]; then
        die "--reinstall 只允许与 --platform 一起使用（第三方前端资产不可覆盖）"
    fi

    local target="$FRONTENDS_ROOT/$fid/$version"
    local replace=0
    if [ -e "$target" ]; then
        local incoming_hash existing_hash
        incoming_hash="$(content_hash "$artifact_dir")"
        existing_hash="$(content_hash "$target")"
        if [ "$incoming_hash" = "$existing_hash" ]; then
            # 同 ID 同版本同内容 = 重部署幂等：不重写资产，只确保注册表有条目。
            info "同版本内容一致，跳过资产复制（幂等重装）: $fid@$version"
            with_registry_lock registry_ensure_entry "$fid" "$version" \
                "$artifact_dir/frontend.json" \
                "$([ "$INSTALL_PLATFORM" = 1 ] && echo true || echo false)" \
                "$([ "$INSTALL_MAKE_CURRENT" = 1 ] && echo true || echo false)"
            ok "注册表已更新: $REGISTRY"
            return 0
        fi
        if [ "$INSTALL_REINSTALL" != "1" ]; then
            die "已存在同 ID 同版本但内容不同（资产不可变，拒绝覆盖）: $target
      第三方前端请发布新版本号；平台重部署请用 --platform --reinstall"
        fi
        warn "平台重部署：同版本内容不同，将原子替换 $target"
        replace=1
    fi

    if [ "$INSTALL_DRY_RUN" = "1" ]; then
        info "[dry-run] 将安装 $fid@$version → $target"
        info "[dry-run] entry=$MF_ENTRY styles='$MF_STYLES'"
        return 0
    fi

    local staging="$FRONTENDS_ROOT/$fid/.staging-$version-$$"
    rm -rf -- "$staging"
    mkdir -p "$staging"
    # 复制全部构建产物（自包含：不依赖源码、node_modules、pnpm 或本仓库）。
    cp -a "$artifact_dir/." "$staging/"
    # 资产不可变 + 全局只读：前端是可信代码，但没有理由让它在磁盘上可写。
    chown -R root:root "$staging" 2>/dev/null || true
    chmod -R a-w "$staging" 2>/dev/null || true
    chmod -R a+rX "$staging" 2>/dev/null || true
    mkdir -p "$(dirname "$target")"
    if [ "$replace" = "1" ]; then
        chmod -R u+w -- "$target" 2>/dev/null || true
        rm -rf -- "$target"
    fi
    # 同目录 rename：任何时刻访问者看到的都是完整制品或不存在，不会是半份。
    mv -- "$staging" "$target"
    chmod u+w -- "$(dirname "$target")" 2>/dev/null || true
    ok "已安装前端资产: $target（$MF_NAME $version）"

    with_registry_lock registry_register_or_update "$fid" "$version" "$target/frontend.json" \
        "$([ "$INSTALL_PLATFORM" = 1 ] && echo true || echo false)" \
        "$([ "$INSTALL_MAKE_CURRENT" = 1 ] && echo true || echo false)" "$(source_label_for "$kind" "$source")"
    ok "注册表已更新: $REGISTRY"
    cat <<EOF

安装完成：$fid@$version
  - 激活它：管理端 → 设置 → FRONTEND_ACTIVE 选择「$fid」（或 $( [ "$fid" = default ] && echo '保持 default' || echo "设 FRONTEND_ACTIVE=$fid" )）
  - 破窗恢复：浏览器访问 ?frontend=default
EOF
}

source_label_for() { # <kind> <source>
    case "$1" in
        artifact|directory) printf 'path:%s' "$2" ;;
        git) printf 'git:%s%s' "$2" "${INSTALL_REF:+@$INSTALL_REF}" ;;
        *) printf '%s' "$2" ;;
    esac
}

# 内容指纹：用于"同版本同内容 = 幂等"判定。基于文件相对路径 + 内容 sha256。
content_hash() {
    local dir="$1"
    ( cd "$dir" && find . -type f ! -name '.registry*' -print0 2>/dev/null \
        | sort -z | xargs -0 -r sha256sum 2>/dev/null | sha256sum | cut -d' ' -f1 )
}

# 首次安装用 register（重复版本是安全网）；版本已在位则退化为 touch（更新记录）。
# 用法：registry_register_or_update <fid> <version> <manifest> <protected> <make_current> <source>
registry_register_or_update() {
    if registry_helper register "$REGISTRY" "$@"; then
        return 0
    fi
    registry_helper touch "$REGISTRY" "$1" "$2" "$3" "$4" "$5"
}

registry_ensure_entry() {
    # 幂等路径：资产已在位，只保证注册表有条目（不改动已安装版本的记录）。
    registry_helper touch "$REGISTRY" "$1" "$2" "$3" "$4" "$5"
}

# 归档可能把 dist 内容放在一层目录里；归一化到真正含 frontend.json 的目录。
normalize_artifact_root() {
    local dir="$1"
    if [ -f "$dir/frontend.json" ]; then printf '%s' "$dir"; return; fi
    local candidate
    candidate="$(find "$dir" -maxdepth 4 -name frontend.json -type f -print -quit 2>/dev/null || true)"
    if [ -n "$candidate" ]; then
        printf '%s' "$(dirname "$candidate")"
        return
    fi
    die "归档里找不到 frontend.json（制品必须自包含 manifest）: $dir"
}

# 本地源码目录 → 构建（或复用已有 dist）→ $stage/built
prepare_from_source() {
    local srcdir="$1" stage="$2"
    local manifest="$srcdir/frontend.json"
    [ -f "$manifest" ] || manifest="$srcdir/floatctf.frontend.json"
    [ -f "$manifest" ] \
        || die "源码目录缺少前端 manifest（frontend.json 或 floatctf.frontend.json）: $srcdir"

    local built="$stage/built"
    if [ "$INSTALL_NO_BUILD" = "1" ]; then
        [ -d "$srcdir/dist" ] || die "--no-build 需要源码目录已存在 dist/"
        info "复用已有 dist/（--no-build）"
        mkdir -p "$built"
        cp -a "$srcdir/dist/." "$built/"
    else
        require_docker
        build_source_frontend "$srcdir" "$built" "$INSTALL_NODE_IMAGE"
    fi

    # 构建产物必须自带 frontend.json；若源码树的 manifest 就是制品 manifest，补进去。
    if [ ! -f "$built/frontend.json" ]; then
        if [ -f "$srcdir/frontend.json" ]; then
            cp -a "$srcdir/frontend.json" "$built/frontend.json"
        elif [ -f "$srcdir/floatctf.frontend.json" ]; then
            cp -a "$srcdir/floatctf.frontend.json" "$built/frontend.json"
        else
            die "构建产物既没有 frontend.json，源码也没有可复制的 manifest"
        fi
    fi
}

prepare_from_git() {
    local url="$1" stage="$2"
    local checkout="$stage/checkout"
    if [ -n "$INSTALL_REF" ]; then
        git clone --depth 1 --branch "$INSTALL_REF" "$url" "$checkout" \
            || die "git clone 失败（ref=$INSTALL_REF）: $url"
    else
        git clone --depth 1 "$url" "$checkout" || die "git clone 失败: $url"
    fi
    rm -rf "$checkout/.git"
    prepare_from_source "$checkout" "$stage"
}

require_docker() {
    command -v docker >/dev/null 2>&1 || die "源码安装需要 docker（隔离构建容器）"
    docker info >/dev/null 2>&1 || die "docker daemon 不可用"
}

# ── 移除 / 设当前版本 / 查看 ─────────────────────────────────────────────────

cmd_remove() {
    local fid="${1:-}" version="${2:-}"
    [ -n "$fid" ] || die "用法: frontend.sh remove <id> [version]"
    require_python
    is_safe_id "$fid" || die "非法前端 ID: $fid"
    if [ "$fid" = "$DEFAULT_FRONTEND_ID" ]; then
        die "default 前端由平台发布并受保护，不能被移除"
    fi

    # 先看注册表里的路径，再删磁盘：注册表说没有就不动文件系统。
    with_registry_lock registry_helper remove "$REGISTRY" "$fid" "$version"

    local versions=()
    if [ -n "$version" ]; then
        versions=("$version")
    else
        local d
        for d in "$FRONTENDS_ROOT/$fid"/*/; do
            [ -d "$d" ] && versions+=("$(basename "$d")")
        done
    fi
    local v
    for v in "${versions[@]:-}"; do
        [ -n "$v" ] || continue
        local target="$FRONTENDS_ROOT/$fid/$v"
        case "$target" in
            "$FRONTENDS_ROOT/$fid/"*) ;;
            *) die "拒绝删除越出前端根的路径: $target" ;;
        esac
        # 安装时刻意把资产设为只读（不可变）；删除前先恢复目录可写，否则非 root
        # 运维会卡在"注册表已删、文件还在"的不一致状态。
        chmod -R u+w -- "$target" 2>/dev/null || true
        rm -rf -- "$target"
        ok "已移除前端资产: $target"
    done
    # 该 ID 已无任何版本 → 连目录一起清掉。
    if [ -d "$FRONTENDS_ROOT/$fid" ] && [ -z "$(ls -A "$FRONTENDS_ROOT/$fid" 2>/dev/null)" ]; then
        rmdir "$FRONTENDS_ROOT/$fid" 2>/dev/null || true
    fi
    ok "注册表已更新: $REGISTRY"
}

cmd_set_current() {
    local fid="${1:-}" version="${2:-}"
    [ -n "$fid" ] && [ -n "$version" ] || die "用法: frontend.sh set-current <id> <version>"
    require_python
    is_safe_id "$fid" || die "非法前端 ID: $fid"
    is_semver "$version" || die "非法版本号: $version"
    [ -d "$FRONTENDS_ROOT/$fid/$version" ] || die "该版本未安装: $fid@$version"
    with_registry_lock registry_helper set-current "$REGISTRY" "$fid" "$version"
    ok "已切换当前版本：$fid → $version（生效需刷新浏览器；FRONTEND_ACTIVE 不变）"
}

cmd_list() {
    require_python
    info "前端存储: $FRONTENDS_ROOT"
    registry_helper list "$REGISTRY"
}

cmd_info() {
    local fid="${1:-}" version="${2:-}"
    [ -n "$fid" ] || die "用法: frontend.sh info <id> [version]"
    require_python
    is_safe_id "$fid" || die "非法前端 ID: $fid"
    registry_helper info "$REGISTRY" "$fid" "${version:-}"
}

cmd_verify() {
    local archive="${1:-}"
    [ -n "$archive" ] || die "用法: frontend.sh verify <artifact.tar.gz>"
    [ -f "$archive" ] || die "文件不存在: $archive"
    require_python
    require_tar
    local stage
    stage="$(mktmp)"
    local dir="$stage/artifact"
    extract_archive "$archive" "$dir"
    dir="$(normalize_artifact_root "$dir")"
    validate_manifest_file "$dir/frontend.json"
    [ -f "$dir/$MF_ENTRY" ] || die "entry 不存在: $MF_ENTRY"
    local style
    for style in $MF_STYLES; do
        [ -f "$dir/$style" ] || die "style 不存在: $style"
    done
    ok "制品校验通过：$MF_ID@$MF_VERSION（entry=$MF_ENTRY, styles='$MF_STYLES'）"
    ok "契约：frontendRuntime=$MF_RUNTIME apiContract=$MF_APICONTRACT"
}

# ── 入口 ──────────────────────────────────────────────────────────────────────

main() {
    local cmd="${1:-help}"
    shift || true
    case "$cmd" in
        help|-h|--help) usage ;;
        list) cmd_list "$@" ;;
        info) cmd_info "$@" ;;
        verify) cmd_verify "$@" ;;
        install) cmd_install "$@" ;;
        remove) cmd_remove "$@" ;;
        set-current) cmd_set_current "$@" ;;
        *) usage >&2; die "未知命令: $cmd" ;;
    esac
}

main "$@"
