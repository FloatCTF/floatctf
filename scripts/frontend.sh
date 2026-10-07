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
# **两个独立契约**（见 docs/frontend/ARTIFACT.md）：
#   - REGISTRY_SCHEMA_VERSION: registry.json 的结构版本
#   - MANIFEST_SCHEMA_VERSION:  frontend.json 的结构版本
# 它们今天恰好都是 1，但绝不能互相复用常量（否则未来单独升版会静默耦合）。
REGISTRY_SCHEMA_VERSION="1"
MANIFEST_SCHEMA_VERSION="1"
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
  --dry-run              只打印将要执行的操作
  -h, --help             显示本帮助

环境:
  FLOATCTF_HOME          安装根（默认 /var/lib/floatctf）
  FRONTENDS_ROOT         前端存储根（默认 $FLOATCTF_HOME/frontends）

源码 manifest（floatctf.frontend.json）可选的 build 段（只允许这三个键）:
  "build": { "packageManager": "auto|pnpm|npm|yarn", "script": "build", "outputDir": "dist" }
  - script 是 package.json 里的**脚本名**（绝不是 shell 片段）
  - outputDir 是安全相对目录（默认 dist）；构建容器只复制该目录

说明:
  - **前端版本不可变**：同 ID + 同版本必须是同一份字节（资产带 immutable 长缓存）。
    内容变了就发布新版本号；没有任何 --reinstall 例外（包括 default）。
  - 本脚本**不会**修改 FRONTEND_ACTIVE；激活前端请在管理端设置页操作。
  - 源码安装一律在隔离 Docker 构建容器内执行依赖安装与构建，不会在宿主直接跑
    pnpm/npm/yarn。
USAGE
}

# ── 注册表 JSON 操作（python3；规则与 packages/frontend-runtime 对齐）──────────

# registry_helper <subcommand> [...]
registry_helper() {
    # 期望的 schema / 契约版本由 bash 侧注入：Python 侧不再复制一份常量。
    FCTF_REGISTRY_SCHEMA_VERSION="$REGISTRY_SCHEMA_VERSION" \
    FCTF_MANIFEST_SCHEMA_VERSION="$MANIFEST_SCHEMA_VERSION" \
    FCTF_FRONTEND_RUNTIME_CONTRACT="$FRONTEND_RUNTIME_CONTRACT" \
    FCTF_API_CONTRACT="$API_CONTRACT" \
    python3 - "$@" <<'PY'
import json
import os
import re
import stat
import sys
import tempfile
from pathlib import Path

# 注册表 schema 与制品 manifest schema 是**两个独立契约**（见 docs/frontend/ARTIFACT.md）。
# 期望值由 bash 侧通过环境变量注入，避免这里复制一份常量后悄悄漂移。
REGISTRY_SCHEMA_VERSION = int(os.environ.get("FCTF_REGISTRY_SCHEMA_VERSION", "1"))
MANIFEST_SCHEMA_VERSION = int(os.environ.get("FCTF_MANIFEST_SCHEMA_VERSION", "1"))
FRONTEND_RUNTIME_CONTRACT = os.environ.get("FCTF_FRONTEND_RUNTIME_CONTRACT", "1")
API_CONTRACT = os.environ.get("FCTF_API_CONTRACT", "1")

# 公开注册表的**允许字段**（$FLOATCTF_HOME/frontends 是公开静态树，见 ARCHITECTURE §5）。
ROOT_KEYS = {"schemaVersion", "updatedAt", "frontends"}
FRONTEND_KEYS = {"id", "currentVersion", "protected", "versions"}
VERSION_KEYS = {
    "version",
    "name",
    "description",
    "author",
    "compatibility",
    "entry",
    "styles",
    "installedAt",
}
COMPAT_KEYS = {"frontendRuntime", "apiContract", "sdk"}

# 制品 manifest 的允许字段。
MANIFEST_KEYS = {
    "schemaVersion",
    "id",
    "name",
    "version",
    "description",
    "author",
    "compatibility",
    "entry",
    "styles",
}
# 仅源码 manifest 额外允许的字段。
SOURCE_KEYS = {"build"}
BUILD_KEYS = {"packageManager", "script", "outputDir"}

ID_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?"
    r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
)
MAJOR_RE = re.compile(r"^(0|[1-9]\d*)$")
SEGMENT_RE = re.compile(r"^[A-Za-z0-9._@+-]+$")
SCHEME_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")
# build.script 只接受**脚本名**，绝不接受 shell 片段。
SCRIPT_NAME_RE = re.compile(r"^[A-Za-z0-9:_-]+$")


def fail(message: str, code: int = 1):
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(code)


# ── 共享校验规则（与 packages/frontend-runtime/src/paths.ts 同源）─────────────

def is_safe_id(value) -> bool:
    return isinstance(value, str) and 0 < len(value) <= 64 and bool(ID_RE.match(value))


def is_semver(value) -> bool:
    return isinstance(value, str) and bool(SEMVER_RE.match(value))


def path_error(value) -> str | None:
    if not isinstance(value, str):
        return "path must be a string"
    if value == "":
        return "path must not be empty"
    if len(value) > 512:
        return "path is too long"
    if value != value.strip():
        return "path must not have surrounding whitespace"
    if any(ord(ch) < 0x20 or ord(ch) == 0x7F for ch in value):
        return "path must not contain control characters"
    if "\\" in value:
        return "path must not contain backslashes"
    if SCHEME_RE.match(value):
        return "path must not contain a URL scheme"
    if value.startswith("/") or value.startswith("~"):
        return "path must be relative"
    for segment in value.split("/"):
        if segment == "":
            return "path must not contain empty segments"
        if segment in (".", ".."):
            return "path must not contain '.' or '..' segments"
        if not SEGMENT_RE.match(segment):
            return f"path segment contains unsupported characters: {segment}"
    return None


def is_major_compatible(constraint, current: str) -> bool:
    if not isinstance(constraint, str) or not MAJOR_RE.match(constraint.strip()):
        return False
    return int(constraint.strip()) == int(current)


# ── manifest ────────────────────────────────────────────────────────────────

def parse_manifest(raw, allow_source: bool):
    """返回 (errors, warnings, artifact_manifest, build_config)。

    语义与 @floatctf/frontend-runtime 的 parseFrontendManifest **逐条对齐**；
    源码模式额外允许 `build`（且只允许它的三个已文档化字段）。
    """
    errors: list[str] = []
    warnings: list[str] = []

    if not isinstance(raw, dict):
        return ["frontend.json must be a JSON object"], warnings, None, None

    allowed = set(MANIFEST_KEYS) | (SOURCE_KEYS if allow_source else set())
    for key in raw:
        if key not in allowed:
            errors.append(f"frontend.json: unknown field `{key}`")

    if raw.get("schemaVersion") != MANIFEST_SCHEMA_VERSION:
        errors.append(
            "frontend.json: unsupported schemaVersion "
            f"{json.dumps(raw.get('schemaVersion'))} (expected {MANIFEST_SCHEMA_VERSION})"
        )
    if not is_safe_id(raw.get("id")):
        errors.append("frontend.json: id must match [a-z0-9][a-z0-9._-]* (max 64 chars)")

    name = raw.get("name")
    if not isinstance(name, str) or name.strip() == "":
        errors.append("frontend.json: name must be a non-empty string")
    elif len(name) > 128:
        errors.append("frontend.json: name must be at most 128 characters")

    if not is_semver(raw.get("version")):
        errors.append("frontend.json: version must be a valid semver string")

    if "description" in raw and (
        not isinstance(raw["description"], str) or len(raw["description"]) > 1024
    ):
        errors.append("frontend.json: description must be a string (max 1024)")
    if "author" in raw and (
        not isinstance(raw["author"], str) or len(raw["author"]) > 256
    ):
        errors.append("frontend.json: author must be a string (max 256)")

    compatibility = None
    compat = raw.get("compatibility")
    if not isinstance(compat, dict):
        errors.append("frontend.json: compatibility must be an object")
    else:
        for key in compat:
            if key not in COMPAT_KEYS:
                errors.append(f"frontend.json: compatibility has unknown field `{key}`")
        if not is_major_compatible(compat.get("frontendRuntime"), FRONTEND_RUNTIME_CONTRACT):
            errors.append(
                "frontend.json: compatibility.frontendRuntime "
                f"{json.dumps(compat.get('frontendRuntime'))} is incompatible with runtime "
                f"major {FRONTEND_RUNTIME_CONTRACT}"
            )
        if not is_major_compatible(compat.get("apiContract"), API_CONTRACT):
            errors.append(
                "frontend.json: compatibility.apiContract "
                f"{json.dumps(compat.get('apiContract'))} is incompatible with API contract "
                f"major {API_CONTRACT}"
            )
        sdk = compat.get("sdk")
        if sdk is not None and not isinstance(sdk, str):
            errors.append("frontend.json: compatibility.sdk must be a string when present")
        elif isinstance(sdk, str) and len(sdk) > 128:
            errors.append("frontend.json: compatibility.sdk must be at most 128 characters")
        elif isinstance(sdk, str):
            warnings.append(
                f"compatibility.sdk={sdk} is informational only "
                "(frontends bundle their own dependencies)"
            )
        if isinstance(compat.get("frontendRuntime"), str) and isinstance(
            compat.get("apiContract"), str
        ):
            compatibility = {
                "frontendRuntime": compat["frontendRuntime"],
                "apiContract": compat["apiContract"],
            }
            if isinstance(sdk, str):
                compatibility["sdk"] = sdk

    entry = raw.get("entry")
    err = path_error(entry)
    if err:
        errors.append(f"frontend.json: entry: {err}")

    styles = raw.get("styles")
    if styles is not None:
        if not isinstance(styles, list):
            errors.append("frontend.json: styles must be an array when present")
        else:
            if len(styles) > 16:
                errors.append("frontend.json: styles must contain at most 16 entries")
            for index, style in enumerate(styles):
                err = path_error(style)
                if err:
                    errors.append(f"frontend.json: styles[{index}]: {err}")

    build = None
    if allow_source and "build" in raw:
        raw_build = raw["build"]
        if not isinstance(raw_build, dict):
            errors.append("frontend.json: build must be an object")
        else:
            for key in raw_build:
                if key not in BUILD_KEYS:
                    errors.append(f"frontend.json: build has unknown field `{key}`")
            pm = raw_build.get("packageManager", "auto")
            if pm not in ("auto", "pnpm", "npm", "yarn"):
                errors.append(
                    "frontend.json: build.packageManager must be one of auto|pnpm|npm|yarn"
                )
            script = raw_build.get("script", "build")
            if not isinstance(script, str) or not SCRIPT_NAME_RE.match(script):
                errors.append(
                    "frontend.json: build.script must be a script NAME "
                    "([A-Za-z0-9:_-]+, never shell)"
                )
            output_dir = raw_build.get("outputDir", "dist")
            err = path_error(output_dir)
            if err:
                errors.append(f"frontend.json: build.outputDir: {err}")
            if not errors:
                build = {
                    "packageManager": pm,
                    "script": script,
                    "outputDir": output_dir,
                }

    if errors:
        return errors, warnings, None, None

    artifact = {
        "schemaVersion": MANIFEST_SCHEMA_VERSION,
        "id": raw["id"],
        "name": raw["name"],
        "version": raw["version"],
    }
    if isinstance(raw.get("description"), str):
        artifact["description"] = raw["description"]
    if isinstance(raw.get("author"), str):
        artifact["author"] = raw["author"]
    artifact["compatibility"] = compatibility
    artifact["entry"] = entry
    if isinstance(styles, list):
        artifact["styles"] = list(styles)
    return [], warnings, artifact, build


def load_manifest(path: str, allow_source: bool):
    try:
        raw = json.loads(Path(path).read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"frontend.json 不是合法 JSON: {path}: {exc}")
    errors, warnings, artifact, build = parse_manifest(raw, allow_source)
    if errors:
        for message in errors:
            print(f"error: {message}", file=sys.stderr)
        raise SystemExit(1)
    return artifact, build, warnings


# ── 注册表 ──────────────────────────────────────────────────────────────────

def sanitize_registry(data: dict) -> dict:
    """只保留公开字段；丢弃任何非公开元数据（例如历史遗留的 `source`）。

    `$FLOATCTF_HOME/frontends` 由 Caddy 以只读方式公开提供，因此注册表里
    **绝不能**留下安装来源、本地路径、Git URL 等运维信息。
    """
    clean = {
        "schemaVersion": data.get("schemaVersion", REGISTRY_SCHEMA_VERSION),
        "updatedAt": data.get("updatedAt", ""),
        "frontends": {},
    }
    for fid, entry in (data.get("frontends") or {}).items():
        if not isinstance(entry, dict):
            continue
        versions = {}
        for version, meta in (entry.get("versions") or {}).items():
            if not isinstance(meta, dict):
                continue
            record = {k: v for k, v in meta.items() if k in VERSION_KEYS}
            if isinstance(record.get("compatibility"), dict):
                record["compatibility"] = {
                    k: v for k, v in record["compatibility"].items() if k in COMPAT_KEYS
                }
            versions[version] = record
        cleaned_entry = {
            "id": entry.get("id", fid),
            "currentVersion": entry.get("currentVersion", ""),
            "versions": versions,
        }
        if entry.get("protected"):
            cleaned_entry["protected"] = True
        clean["frontends"][fid] = cleaned_entry
    return clean


def load_registry(path: str) -> dict:
    p = Path(path)
    if not p.exists():
        return {"schemaVersion": REGISTRY_SCHEMA_VERSION, "updatedAt": "", "frontends": {}}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"registry.json 不是合法 JSON: {path}: {exc}")
    if not isinstance(data, dict):
        fail(f"registry.json 顶层必须是对象: {path}")
    if not isinstance(data.get("frontends", {}), dict):
        fail("registry.json frontends 必须是对象")
    return sanitize_registry(data)


def atomic_write(path: str, data) -> None:
    """同目录 tmp + fsync + rename：任何时刻读到的都是完整文件。"""
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    payload = sanitize_registry(data)
    fd, tmp = tempfile.mkstemp(dir=str(target.parent), prefix=".registry.", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, ensure_ascii=False, indent=2, sort_keys=True)
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


def now_iso() -> str:
    import datetime

    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def version_sort_key(version: str):
    """严格 semver 排序键（允许预发布），绝不混合 int/str 比较。"""
    match = SEMVER_RE.match(version)
    if not match:
        return ((1, 0), (1, 0), (1, 0), (1, version))
    major, minor, patch = (int(match.group(i)) for i in (1, 2, 3))
    prerelease = match.group(4)
    if prerelease is None:
        pre_key = (1, ())
    else:
        parts = []
        for part in prerelease.split("."):
            parts.append((0, int(part)) if part.isdigit() else (1, part))
        pre_key = (0, tuple(parts))
    return (major, minor, patch, pre_key)


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
        for version in sorted(versions, key=version_sort_key):
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
    print(f"installedVersions: {', '.join(sorted(versions, key=version_sort_key))}")
    return 0


def cmd_has_version(registry_path: str, fid: str, version: str) -> int:
    """退出码 0 = 注册表已有该版本；1 = 没有。用于显式的幂等分支判断。"""
    data = load_registry(registry_path)
    entry = (data.get("frontends") or {}).get(fid) or {}
    return 0 if version in (entry.get("versions") or {}) else 1


def _register(registry_path: str, fid: str, version: str, manifest_path: str,
              protected: bool, make_current: bool, idempotent: bool) -> int:
    data = load_registry(registry_path)
    try:
        manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        fail(f"frontend.json 不是合法 JSON: {manifest_path}: {exc}")
    errors, _warnings, artifact, _build = parse_manifest(manifest, allow_source=False)
    if errors:
        for message in errors:
            print(f"error: {message}", file=sys.stderr)
        raise SystemExit(1)

    entry = data["frontends"].setdefault(fid, {
        "id": fid,
        "currentVersion": version,
        "protected": bool(protected),
        "versions": {},
    })
    versions = entry.setdefault("versions", {})
    if version in versions and not idempotent:
        fail(f"前端 {fid} 版本 {version} 已安装（资产不可变，拒绝覆盖）")

    record = dict(artifact)
    record["version"] = version
    existing = versions.get(version) or {}
    record["installedAt"] = existing.get("installedAt") or now_iso()
    versions[version] = record
    entry["id"] = fid
    if protected:
        entry["protected"] = True
    if make_current or not entry.get("currentVersion"):
        entry["currentVersion"] = version
    data["schemaVersion"] = REGISTRY_SCHEMA_VERSION
    data["updatedAt"] = now_iso()
    atomic_write(registry_path, data)
    print(f"{'ensured' if idempotent else 'registered'} {fid} {version} "
          f"(current={entry['currentVersion']})")
    return 0


def cmd_register(registry_path: str, fid: str, version: str, manifest_path: str,
                 protected: bool, make_current: bool) -> int:
    return _register(registry_path, fid, version, manifest_path, protected, make_current, False)


def cmd_ensure(registry_path: str, fid: str, version: str, manifest_path: str,
               protected: bool, make_current: bool) -> int:
    """幂等注册：**仅**用于"资产已在位且内容一致"的场景（调用方负责判定）。"""
    return _register(registry_path, fid, version, manifest_path, protected, make_current, True)


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
        del frontends[fid]
        removed = ["(all)"]
    else:
        if version not in versions:
            fail(f"前端 {fid} 未安装版本: {version}")
        remaining = [v for v in versions if v != version]
        if not remaining:
            del frontends[fid]
        elif entry.get("currentVersion") == version:
            # currentVersion 是**显式指针**：绝不替用户猜替代版本。
            fail(
                f"版本 {version} 是 {fid} 的 currentVersion，且该前端还有其它版本。\n"
                f"       请先执行: frontend.sh set-current {fid} <其它版本>\n"
                f"       然后再删除 {version}。"
            )
        else:
            del versions[version]
        removed = [version]
    data["updatedAt"] = now_iso()
    atomic_write(registry_path, data)
    print(f"removed {fid} {','.join(removed)}")
    return 0


def cmd_set_current(registry_path: str, fid: str, version: str) -> int:
    data = load_registry(registry_path)
    entry = (data.get("frontends") or {}).get(fid)
    if entry is None:
        fail(f"未安装前端: {fid}")
    if version not in (entry.get("versions") or {}):
        fail(f"前端 {fid} 未安装版本: {version}")
    entry["currentVersion"] = version
    data["updatedAt"] = now_iso()
    atomic_write(registry_path, data)
    print(f"{fid} currentVersion = {version}")
    return 0


def cmd_manifest_fields(manifest_path: str, allow_source: str) -> int:
    """把 manifest 的标量字段以 shlex.quote 形式打印，供 bash 安全 eval。"""
    import shlex

    artifact, build, warnings = load_manifest(manifest_path, allow_source == "true")
    for message in warnings:
        print(f"warn: {message}", file=sys.stderr)
    compat = artifact["compatibility"]
    styles = artifact.get("styles", [])
    print(f"MF_SCHEMA={shlex.quote(str(artifact['schemaVersion']))}")
    print(f"MF_ID={shlex.quote(artifact['id'])}")
    print(f"MF_NAME={shlex.quote(artifact['name'])}")
    print(f"MF_VERSION={shlex.quote(artifact['version'])}")
    print(f"MF_ENTRY={shlex.quote(artifact['entry'])}")
    print(f"MF_RUNTIME={shlex.quote(compat['frontendRuntime'])}")
    print(f"MF_APICONTRACT={shlex.quote(compat['apiContract'])}")
    # 样式路径：合法路径不含空格（规则禁止），因此空格分隔是安全的。
    print(f"MF_STYLES={shlex.quote(' '.join(styles))}")
    if build is not None:
        print(f"MF_BUILD_PM={shlex.quote(build['packageManager'])}")
        print(f"MF_BUILD_SCRIPT={shlex.quote(build['script'])}")
        print(f"MF_BUILD_OUTPUTDIR={shlex.quote(build['outputDir'])}")
    else:
        print("MF_BUILD_PM=''")
        print("MF_BUILD_SCRIPT=''")
        print("MF_BUILD_OUTPUTDIR=''")
    return 0


def cmd_manifest_validate_json(manifest_path: str, allow_source: str) -> int:
    """机器可读的校验结果（parity 测试用）：{"ok":bool,"errors":[...]}。"""
    try:
        raw = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"ok": False, "errors": [f"invalid JSON: {exc}"]}))
        return 0
    errors, _warnings, _artifact, _build = parse_manifest(raw, allow_source == "true")
    print(json.dumps({"ok": not errors, "errors": errors}, ensure_ascii=False))
    # 退出码与 JSON 一致：shell 调用方可以直接用 if/&&。
    return 0 if not errors else 1


def cmd_manifest_validate(manifest_path: str, allow_source: str) -> int:
    """与 parseFrontendManifest 同语义的严格校验（成功时打印摘要）。"""
    import shlex

    artifact, build, warnings = load_manifest(manifest_path, allow_source == "true")
    for message in warnings:
        print(f"[WARN] {message}", file=sys.stderr)
    compat = artifact["compatibility"]
    extra = ""
    if build is not None:
        extra = (f"，build(packageManager={build['packageManager']}, "
                 f"script={build['script']}, outputDir={build['outputDir']})")
    print(f"{artifact['id']}@{artifact['version']} "
          f"(runtime {compat['frontendRuntime']} / api {compat['apiContract']}{extra})")
    return 0


def cmd_manifest_to_artifact(source_manifest: str, out_path: str) -> int:
    """源码 manifest → **制品** manifest：只复制运行时字段（剥掉 build）。"""
    artifact, _build, _warnings = load_manifest(source_manifest, allow_source=True)
    # 再按**制品**契约复核一次（保证生成物一定通过权威解析器）。
    errors, _w, _a, _b = parse_manifest(artifact, allow_source=False)
    if errors:
        for message in errors:
            print(f"error: generated frontend.json invalid: {message}", file=sys.stderr)
        raise SystemExit(1)
    Path(out_path).write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"generated {out_path} from source manifest (build section stripped)")
    return 0


def cmd_validate_artifact_tree(root: str, required: list[str]) -> int:
    """用 lstat 语义递归校验制品树：拒绝符号链接与任何特殊文件。"""
    base = Path(root)
    if not base.is_dir():
        fail(f"制品目录不存在: {root}")
    count = 0
    for dirpath, dirnames, filenames in os.walk(base, followlinks=False):
        # 目录本身也可能是符号链接（os.walk 不跟随，但会列在 dirnames 里）
        for name in list(dirnames) + list(filenames):
            path = Path(dirpath) / name
            try:
                st = os.lstat(path)
            except OSError as exc:  # noqa: BLE001
                fail(f"无法检查制品成员 {path}: {exc}")
            mode = st.st_mode
            if stat.S_ISLNK(mode):
                fail(f"拒绝符号链接制品成员: {path.relative_to(base)}")
            if stat.S_ISDIR(mode) or stat.S_ISREG(mode):
                count += 1
                continue
            kind = (
                "FIFO" if stat.S_ISFIFO(mode)
                else "socket" if stat.S_ISSOCK(mode)
                else "block device" if stat.S_ISBLK(mode)
                else "character device" if stat.S_ISCHR(mode)
                else f"mode {oct(mode)}"
            )
            fail(f"拒绝特殊文件类型（{kind}）: {path.relative_to(base)}")

    for rel in required:
        if not rel:
            continue
        path = base / rel
        try:
            st = os.lstat(path)
        except OSError:
            fail(f"制品缺少声明的文件: {rel}")
        if stat.S_ISLNK(st.st_mode):
            fail(f"制品声明的文件不能是符号链接: {rel}")
        if not stat.S_ISREG(st.st_mode):
            fail(f"制品声明的文件必须是普通文件: {rel}")
    print(f"artifact tree ok ({count} entries, required={len(required)})")
    return 0


def cmd_registry_ids(registry_path: str) -> int:
    data = load_registry(registry_path)
    for fid in sorted((data.get("frontends") or {})):
        print(fid)
    return 0


def cmd_registry_check_public(registry_path: str) -> int:
    """断言公开注册表里没有非公开字段（写入后自检；失败即红）。"""
    raw = json.loads(Path(registry_path).read_text(encoding="utf-8"))
    offending = []
    for key in raw:
        if key not in ROOT_KEYS:
            offending.append(f"root.{key}")
    for fid, entry in (raw.get("frontends") or {}).items():
        for key in entry:
            if key not in FRONTEND_KEYS:
                offending.append(f"frontends.{fid}.{key}")
        for version, meta in (entry.get("versions") or {}).items():
            for key in meta:
                if key not in VERSION_KEYS:
                    offending.append(f"frontends.{fid}.versions.{version}.{key}")
            for key in (meta.get("compatibility") or {}):
                if key not in COMPAT_KEYS:
                    offending.append(
                        f"frontends.{fid}.versions.{version}.compatibility.{key}"
                    )
    if offending:
        fail("公开注册表含非公开字段: " + ", ".join(sorted(offending)))
    print("registry public-schema ok")
    return 0


NEEDED_ARGS = {
    "list": 2,
    "info": 4,
    "has-version": 5,
    "register": 7,
    "ensure": 7,
    "remove": 4,
    "set-current": 5,
    "manifest-fields": 4,
    "manifest-validate": 4,
    "manifest-validate-json": 4,
    "manifest-to-artifact": 4,
    "validate-artifact-tree": 3,
    "ids": 3,
    "check-public": 3,
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
    if cmd == "has-version":
        return cmd_has_version(argv[2], argv[3], argv[4])
    if cmd == "register":
        return cmd_register(argv[2], argv[3], argv[4], argv[5],
                            argv[6] == "true", argv[7] == "true")
    if cmd == "ensure":
        return cmd_ensure(argv[2], argv[3], argv[4], argv[5],
                          argv[6] == "true", argv[7] == "true")
    if cmd == "remove":
        return cmd_remove(argv[2], argv[3], argv[4] if len(argv) > 4 and argv[4] else None)
    if cmd == "set-current":
        return cmd_set_current(argv[2], argv[3], argv[4])
    if cmd == "manifest-fields":
        return cmd_manifest_fields(argv[2], argv[3])
    if cmd == "manifest-validate":
        return cmd_manifest_validate(argv[2], argv[3])
    if cmd == "manifest-validate-json":
        return cmd_manifest_validate_json(argv[2], argv[3])
    if cmd == "manifest-to-artifact":
        return cmd_manifest_to_artifact(argv[2], argv[3])
    if cmd == "validate-artifact-tree":
        return cmd_validate_artifact_tree(argv[2], argv[3:])
    if cmd == "ids":
        return cmd_registry_ids(argv[2])
    if cmd == "check-public":
        return cmd_registry_check_public(argv[2])
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
    # 必须**显式**传递内层退出码：否则 `exec 9>&-` 会让函数永远返回 0，
    # 把"注册表拒绝写入"这类失败吞掉（cmd_remove 曾在被拒绝后仍然删掉资产）。
    local rc=0
    "$@" || rc=$?
    if command -v flock >/dev/null 2>&1; then
        exec 9>&- || true
    fi
    return $rc
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

# validate_manifest_file <frontend.json> —— 严格校验（语义与 @floatctf/frontend-runtime
# 的 parseFrontendManifest **逐条一致**），成功时导出 MF_* 变量。
#
# 生产环境不能假设宿主有 Node / @floatctf/frontend-runtime，所以这里保留 Python 实现；
# 但**规则必须等价**：允许字段集合、长度上限、ID/semver/路径规则、compatibility 规则
# 全部在 registry_helper 的 parse_manifest 里实现，并有与真实解析器的同源测试。
validate_manifest_file() {
    local manifest="$1" source_mode="${2:-false}"
    [ -f "$manifest" ] || die "缺少 frontend.json: $manifest"
    local summary
    summary="$(registry_helper manifest-validate "$manifest" "$source_mode")" \
        || die "frontend.json 未通过严格校验: $manifest"
    eval "$(registry_helper manifest-fields "$manifest" "$source_mode")" \
        || die "frontend.json 字段提取失败: $manifest"
    info "manifest 校验通过：$summary"
    return 0
}

# validate_source_manifest <dir> —— 源码 manifest（floatctf.frontend.json 优先）。
# 允许且只允许额外的 `build` 段；成功时导出 MF_* 与 MF_BUILD_*。
validate_source_manifest() {
    local dir="$1"
    local manifest="$dir/floatctf.frontend.json"
    [ -f "$manifest" ] || manifest="$dir/frontend.json"
    [ -f "$manifest" ] \
        || die "源码目录缺少前端 manifest（floatctf.frontend.json 或 frontend.json）: $dir"
    validate_manifest_file "$manifest" true
    SOURCE_MANIFEST="$manifest"
    return 0
}

# validate_artifact_tree <dir> [required-relative-file ...]
#
# 对**任何**来源（归档解包 / 源码构建 / 预构建目录 / 安装暂存树）统一执行的安全边界：
# 用 lstat 语义拒绝符号链接、硬链接（表现为特殊类型）、FIFO、socket、设备文件。
# `[ -f x ]` 会跟随符号链接，因此必须在它之前跑这一步。
validate_artifact_tree() {
    local dir="$1"
    shift || true
    [ -d "$dir" ] || die "制品目录不存在: $dir"
    registry_helper validate-artifact-tree "$dir" "$@" >/dev/null \
        || die "制品树未通过安全校验（含符号链接/特殊文件或缺少声明文件）: $dir"
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

# resolve_build_identity —— 解析**构建容器**里使用的非特权身份。
#
# 生产文档里的安装命令是 `sudo frontend.sh install ...`，此时 `id -u` 是 0；
# 如果直接用它，构建容器就会以 UID 0 运行（与"隔离/非 root"的说法矛盾）。
# 规则（见 docs/frontend/DEVELOPING.md）：
#   1. sudo 调用 → 用 SUDO_UID/SUDO_GID（已校验为非 0）
#   2. 普通用户直接调用 → 用该用户
#   3. root 直接调用且没有非 root 调用者 → 使用专用非特权身份（默认 65534:65534）
# 任何情况下都不返回 UID 0。
resolve_build_identity() {
    local sudo_uid="${SUDO_UID:-}" sudo_gid="${SUDO_GID:-}"
    local uid="" gid=""

    if [ -n "$sudo_uid" ] && [ "$sudo_uid" != "0" ] && [ -n "$sudo_gid" ]; then
        uid="$sudo_uid"; gid="$sudo_gid"
    elif [ "$(id -u)" != "0" ]; then
        uid="$(id -u)"; gid="$(id -g)"
    else
        uid="${FCTF_BUILD_UID:-65534}"; gid="${FCTF_BUILD_GID:-65534}"
    fi

    [ -n "$uid" ] && [ -n "$gid" ] || die "无法解析构建身份（uid/gid 为空）"
    [ "$uid" != "0" ] || die "拒绝以 UID 0 运行构建容器（见 frontend.sh 的构建身份规则）"
    printf '%s:%s' "$uid" "$gid"
}

# _resolve-build-identity <euid> <sudo_uid> <sudo_gid> —— 供测试使用的纯函数形式。
_resolve_build_identity() {
    local euid="$1" sudo_uid="${2:-}" sudo_gid="${3:-}"
    if [ -n "$sudo_uid" ] && [ "$sudo_uid" != "0" ] && [ -n "$sudo_gid" ]; then
        printf '%s:%s' "$sudo_uid" "$sudo_gid"; return 0
    fi
    if [ "$euid" != "0" ]; then
        printf '%s:%s' "$euid" "${4:-$euid}"; return 0
    fi
    printf '%s:%s' "${FCTF_BUILD_UID:-65534}" "${FCTF_BUILD_GID:-65534}"
}

# build_source_frontend <srcdir> <outdir> <node_image> <pm> <script> <output_dir>
#
# 在隔离 Docker 构建容器内安装依赖并执行**约定的**构建脚本。
# 安全措施：cap-drop ALL、no-new-privileges、**非 root 构建身份**、只读源码挂载、
# 不挂 Docker socket、不共享宿主 PID/网络命名空间、pids-limit。
# 注意：隔离只降低**宿主**风险，产物仍然是可信浏览器代码（见信任模型文档）。
build_source_frontend() {
    local srcdir="$1" outdir="$2" node_image="$3" pm="$4" build_script="$5" output_dir="$6"

    # 构建身份：**绝不**是 0。sudo 场景用 SUDO_UID/SUDO_GID；纯 root 场景用专用身份。
    local identity build_uid build_gid
    identity="$(resolve_build_identity)"
    build_uid="${identity%%:*}"
    build_gid="${identity##*:}"
    [ "$build_uid" != "0" ] || die "内部错误：构建身份解析为 UID 0"

    # 暂存源码与输出目录，并交给构建身份所有：这样即使调用者是 root（或源码属主
    # 与构建身份不同），容器内也一定可读/可写，**不需要** chmod 用户的原仓库。
    local build_stage="$7" 
    [ -n "$build_stage" ] || die "内部错误：缺少构建暂存目录"
    local stage_src="$build_stage/src" stage_out="$build_stage/out"
    rm -rf -- "$build_stage"
    mkdir -p "$stage_src" "$stage_out"

    info "源码构建：包管理器=$pm，脚本=$build_script，输出目录=$output_dir，Node 镜像=$node_image"
    info "构建身份（容器内非 root）：$identity"

    # 只复制**源码**：排除依赖与既有构建产物，保证构建从源码出发、不复用宿主产物
    # （宿主 node_modules 可能含不同平台的原生二进制）。
    tar -cf - \
        --exclude=./node_modules --exclude=./.pnpm-store --exclude=./.git \
        --exclude=./dist --exclude=./build --exclude=./.cache \
        --exclude=./.turbo --exclude=./.next --exclude=./.output \
        -C "$srcdir" . | tar -xf - -C "$stage_src" --no-same-owner
    chown -R "$build_uid:$build_gid" "$build_stage" 2>/dev/null || true

    # Node 26 镜像已不再内置 corepack，因此用镜像自带的 npm 把包管理器装到
    # **可写前缀** /tmp/npm-global（容器以非 root 运行，/usr/local 不可写）。
    local pm_version
    pm_version="$(package_manager_version "$srcdir" "$pm")"
    local install_cmd
    case "$pm" in
        pnpm)
            info "pnpm 版本: $pm_version"
            install_cmd='npm install -g --prefix /tmp/npm-global --no-fund --no-audit "pnpm@${FCTF_PM_VERSION}" >/dev/null && PATH="/tmp/npm-global/bin:$PATH" pnpm install --frozen-lockfile'
            ;;
        yarn)
            info "yarn 版本: $pm_version"
            install_cmd='npm install -g --prefix /tmp/npm-global --no-fund --no-audit "yarn@${FCTF_PM_VERSION}" >/dev/null && PATH="/tmp/npm-global/bin:$PATH" (yarn install --immutable || yarn install --frozen-lockfile || yarn install)'
            ;;
        npm)
            install_cmd='if [ -f package-lock.json ]; then npm ci; else npm install; fi'
            ;;
        *) die "不支持的包管理器: $pm" ;;
    esac

    # 静态命令 + 环境变量传参：脚本名/输出目录**绝不**插值进 shell 程序。
    case "$pm" in
        pnpm) run_build='pnpm run "$FCTF_BUILD_SCRIPT"' ;;
        yarn) run_build='yarn run "$FCTF_BUILD_SCRIPT"' ;;
        npm) run_build='npm run "$FCTF_BUILD_SCRIPT"' ;;
    esac

    timeout "$BUILD_TIMEOUT_SECS" docker run --rm \
        --network bridge \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --pids-limit 2048 \
        --user "$build_uid:$build_gid" \
        -e "HOME=/tmp" \
        -e "NPM_CONFIG_UPDATE_NOTIFIER=false" \
        -e "CI=1" \
        -e "FCTF_PM_VERSION=$pm_version" \
        -e "FCTF_BUILD_SCRIPT=$build_script" \
        -e "FCTF_OUTPUT_DIR=$output_dir" \
        -v "$stage_src:/src:ro" \
        -v "$stage_out:/out" \
        -w / \
        "$node_image" \
        bash -lc "
            set -Eeuo pipefail
            # 构建目录放在 /tmp（对容器内非 root 用户可写；容器根 / 不可写）。
            rm -rf /tmp/work && mkdir -p /tmp/work
            cd /src
            tar -cf - . | (cd /tmp/work && tar -xf - --no-same-owner)
            cd /tmp/work
            if [ ! -f package.json ]; then echo 'missing package.json' >&2; exit 2; fi
            node --version
            ${install_cmd}
            # npm -g --prefix 装出来的包管理器只对本次命令生效；固定进 PATH。
            export PATH="/tmp/npm-global/bin:\$PATH"
            ${run_build}
            rm -rf /out/* 2>/dev/null || true
            if [ ! -d \"\$FCTF_OUTPUT_DIR\" ]; then
                echo \"build did not produce the declared outputDir: \$FCTF_OUTPUT_DIR\" >&2
                exit 3
            fi
            (cd \"\$FCTF_OUTPUT_DIR\" && tar -cf - --no-same-owner .) | (cd /out && tar -xf - --no-same-owner)
        " || die "隔离容器构建失败（镜像 $node_image）。若宿主需要代理，请为 docker 配置代理后重试。"

    if [ -z "$(ls -A "$stage_out" 2>/dev/null)" ]; then
        die "构建容器没有产出任何文件"
    fi
    # 构建产物落到调用方给的 outdir（保持调用方原有契约）。
    mkdir -p "$outdir"
    rm -rf -- "$outdir"/*
    tar -cf - -C "$stage_out" . | tar -xf - -C "$outdir" --no-same-owner
    chmod -R u+w "$outdir" 2>/dev/null || true
    ok "源码构建完成（隔离容器，构建身份 $identity）"
}

# ── 安装 ──────────────────────────────────────────────────────────────────────

INSTALL_NODE_IMAGE="$DEFAULT_NODE_IMAGE"
INSTALL_REF=""
INSTALL_NO_BUILD=0
INSTALL_MAKE_CURRENT=0
INSTALL_PLATFORM=0
INSTALL_DRY_RUN=0

# 只用于**打印**：绝不写进公开注册表（$FLOATCTF_HOME/frontends 是公开静态树）。
# Git URL 里的 userinfo 可能含凭据，一律打码。
redact_source() {
    printf '%s' "$1" | sed -E 's#^([a-zA-Z][a-zA-Z0-9+.-]*://)[^/@]*@#\1***@#'
}

cmd_install() {
    local source=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --ref) INSTALL_REF="${2:?--ref 需要参数}"; shift 2 ;;
            --node-image) INSTALL_NODE_IMAGE="${2:?--node-image 需要参数}"; shift 2 ;;
            --no-build) INSTALL_NO_BUILD=1; shift ;;
            --make-current) INSTALL_MAKE_CURRENT=1; shift ;;
            --platform) INSTALL_PLATFORM=1; shift ;;
            --dry-run) INSTALL_DRY_RUN=1; shift ;;
            -h|--help) usage; return 0 ;;
            -*) die "未知选项: $1（见 frontend.sh help）" ;;
            *) [ -z "$source" ] || die "只接受一个 <source>"; source="$1"; shift ;;
        esac
    done
    [ -n "$source" ] || die "用法: frontend.sh install <本地目录|Git URL|制品.tar.gz> [选项]"

    require_python
    require_tar

    local stage artifact_dir kind display_source
    stage="$(mktmp)"
    display_source="$(redact_source "$source")"

    if [ -f "$source" ]; then
        kind="artifact"
        artifact_dir="$stage/artifact"
        info "安装预构建制品: $display_source"
        extract_archive "$source" "$artifact_dir"
        # 归档可能自带一层顶层目录（frontends/<id>/<version>/ 或 <id>/<version>/）。
        artifact_dir="$(normalize_artifact_root "$artifact_dir")"
        validate_artifact_tree "$artifact_dir"
    elif [ -d "$source" ]; then
        if [ -f "$source/frontend.json" ]; then
            # 预构建制品目录（例如 release 解包后的 frontends/<id>/<version>/）：
            # 已经是可安装形态，不再构建。
            kind="directory"
            info "安装预构建制品目录: $display_source"
            artifact_dir="$source"
            validate_artifact_tree "$artifact_dir"
        else
            kind="directory"
            info "安装本地前端源码目录: $display_source"
            prepare_from_source "$source" "$stage"
            artifact_dir="$stage/built"
            validate_artifact_tree "$artifact_dir"
        fi
    else
        kind="git"
        command -v git >/dev/null 2>&1 || die "缺少 git（Git 安装需要）"
        info "克隆并构建 Git 前端: $display_source ${INSTALL_REF:+（ref=$INSTALL_REF）}"
        prepare_from_git "$source" "$stage"
        artifact_dir="$stage/built"
        validate_artifact_tree "$artifact_dir"
    fi

    validate_manifest_file "$artifact_dir/frontend.json"
    local fid="$MF_ID" version="$MF_VERSION"

    if [ "$fid" = "$DEFAULT_FRONTEND_ID" ] && [ "$INSTALL_PLATFORM" != "1" ]; then
        die "default 前端由平台发布并受保护，不能用 frontend.sh install 覆盖（升级走 install.sh）"
    fi

    # 入口与样式必须是制品内的**普通文件**（符号链接/特殊文件已在树校验里被拒）。
    validate_artifact_tree "$artifact_dir" "frontend.json" "$MF_ENTRY" $MF_STYLES

    local target="$FRONTENDS_ROOT/$fid/$version"
    if [ -e "$target" ]; then
        local incoming_hash existing_hash
        incoming_hash="$(content_hash "$artifact_dir")"
        existing_hash="$(content_hash "$target")"
        if [ "$incoming_hash" = "$existing_hash" ]; then
            # 同 ID 同版本同内容 = 重部署幂等：资产不可变，只确保注册表有条目。
            info "同版本内容一致，跳过资产复制（幂等重装）: $fid@$version"
            with_registry_lock registry_helper ensure "$REGISTRY" "$fid" "$version" \
                "$artifact_dir/frontend.json" \
                "$([ "$INSTALL_PLATFORM" = 1 ] && echo true || echo false)" \
                "$([ "$INSTALL_MAKE_CURRENT" = 1 ] && echo true || echo false)"
            with_registry_lock registry_helper check-public "$REGISTRY" >/dev/null
            ok "注册表已更新: $REGISTRY"
            return 0
        fi
        # 资产 URL 带 `immutable` 长缓存：同 ID + 同版本必须是**同一份字节**。
        # 因此这里没有任何例外（包括 default / --platform）。
        die "已存在同 ID 同版本但内容不同（资产不可变，拒绝覆盖）: $target
      → 请发布新的前端版本号（平台版本与前端版本可以独立演进）"
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
    # -a 会保留符号链接，因此复制**前后**都跑一次树校验（防止其间被替换）。
    cp -a "$artifact_dir/." "$staging/"
    validate_artifact_tree "$staging" "frontend.json" "$MF_ENTRY" $MF_STYLES
    # 资产不可变 + 全局只读：前端是可信代码，但没有理由让它在磁盘上可写。
    chown -R root:root "$staging" 2>/dev/null || true
    chmod -R a-w "$staging" 2>/dev/null || true
    chmod -R a+rX "$staging" 2>/dev/null || true
    mkdir -p "$(dirname "$target")"
    # 同目录 rename：任何时刻访问者看到的都是完整制品或不存在，不会是半份。
    mv -- "$staging" "$target"
    chmod u+w -- "$(dirname "$target")" 2>/dev/null || true
    ok "已安装前端资产: $target（$MF_NAME $version）"

    with_registry_lock registry_register_or_update "$fid" "$version" "$artifact_dir/frontend.json" \
        "$([ "$INSTALL_PLATFORM" = 1 ] && echo true || echo false)" \
        "$([ "$INSTALL_MAKE_CURRENT" = 1 ] && echo true || echo false)"
    with_registry_lock registry_helper check-public "$REGISTRY" >/dev/null
    ok "注册表已更新: $REGISTRY"
    cat <<EOF

安装完成：$fid@$version
  - 激活它：管理端 → 设置 → FRONTEND_ACTIVE 选择「$fid」（或 $( [ "$fid" = default ] && echo '保持 default' || echo "设 FRONTEND_ACTIVE=$fid" )）
  - 破窗恢复：浏览器访问 ?frontend=default
EOF
}

# 内容指纹：用于"同版本同内容 = 幂等"判定。基于文件相对路径 + 内容 sha256。
content_hash() {
    local dir="$1"
    ( cd "$dir" && find . -type f ! -name '.registry*' -print0 2>/dev/null \
        | sort -z | xargs -0 -r sha256sum 2>/dev/null | sha256sum | cut -d' ' -f1 )
}

# 注册表写入：**显式**区分"首次注册"与"幂等确保"，不用"任意失败就 touch"做控制流。
# 调用前调用方必须已验证：磁盘上的该版本资产与待装内容**字节一致**（否则上面已 die）。
registry_register_or_update() {
    local fid="$1" version="$2" manifest="$3" protected="$4" make_current="$5"
    if registry_helper has-version "$REGISTRY" "$fid" "$version"; then
        # 幂等路径：注册表已有该版本，且磁盘内容已确认一致 → 只补齐条目。
        registry_helper ensure "$REGISTRY" "$fid" "$version" "$manifest" "$protected" "$make_current" \
            || die "更新注册表失败（已有版本，ensure 路径）: $fid@$version"
    else
        # 首次注册：任何失败都是致命的，绝不退化成 touch。
        registry_helper register "$REGISTRY" "$fid" "$version" "$manifest" "$protected" "$make_current" \
            || die "写入注册表失败（首次注册路径）: $fid@$version"
    fi
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

# 本地源码目录 → 容器构建（或复用已有产物）→ 生成严格 frontend.json → $stage/built
prepare_from_source() {
    local srcdir="$1" stage="$2"
    # 严格解析源码 manifest：只允许运行时字段 + build{packageManager,script,outputDir}
    validate_source_manifest "$srcdir"

    local built="$stage/built"
    local build_stage="$stage/build-work"
    mkdir -p "$built"

    if [ "$INSTALL_NO_BUILD" = "1" ]; then
        local reuse="${MF_BUILD_OUTPUTDIR:-dist}"
        [ -d "$srcdir/$reuse" ] || die "--no-build 需要源码目录已存在 $reuse/"
        info "复用已有产物目录（--no-build）: $reuse/"
        tar -cf - -C "$srcdir/$reuse" . | tar -xf - -C "$built" --no-same-owner
    else
        require_docker
        local pm="$MF_BUILD_PM"
        if [ "$pm" = "auto" ] || [ -z "$pm" ]; then
            pm="$(detect_package_manager "$srcdir")"
        fi
        local build_script="${MF_BUILD_SCRIPT:-build}"
        local output_dir="${MF_BUILD_OUTPUTDIR:-dist}"
        build_source_frontend "$srcdir" "$built" "$INSTALL_NODE_IMAGE" \
            "$pm" "$build_script" "$output_dir" "$build_stage"
    fi

    # 产物必须自带**制品** manifest。若没有，就从源码 manifest 生成一个：
    # **只复制运行时字段**（剥掉 build），然后按制品契约复核 —— 绝不 `cp` 源码 manifest。
    if [ ! -f "$built/frontend.json" ]; then
        info "构建未产出 frontend.json，按制品契约从源码 manifest 生成"
        registry_helper manifest-to-artifact "$SOURCE_MANIFEST" "$built/frontend.json" \
            || die "无法从源码 manifest 生成 frontend.json"
    fi
    validate_manifest_file "$built/frontend.json"
}

prepare_from_git() {
    local url="$1" stage="$2"
    local checkout="$stage/checkout"
    if [ -n "$INSTALL_REF" ]; then
        git clone --depth 1 --branch "$INSTALL_REF" "$url" "$checkout" \
            || die "git clone 失败（ref=$INSTALL_REF）"
    else
        git clone --depth 1 "$url" "$checkout" || die "git clone 失败"
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

    # 先改注册表、再删磁盘：注册表拒绝（例如"该版本是 currentVersion"）时
    # **绝不**留下"文件已删、注册表还在"的不一致状态。
    local remove_out
    if ! remove_out="$(with_registry_lock registry_helper remove "$REGISTRY" "$fid" "$version" 2>&1)"; then
        die "$remove_out"
    fi

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
    validate_artifact_tree "$dir"
    validate_manifest_file "$dir/frontend.json"
    validate_artifact_tree "$dir" "frontend.json" "$MF_ENTRY" $MF_STYLES
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
        # 内部命令（不在 help 中宣传）：供测试断言构建身份解析规则，见 §7.2。
        build-identity) resolve_build_identity; printf '\n' ;;
        _manifest-validate-json) registry_helper manifest-validate-json "$@";;
        _manifest-to-artifact) registry_helper manifest-to-artifact "$@";;
        _resolve-build-identity) _resolve_build_identity "$@"; printf '\n' ;;
        *) usage >&2; die "未知命令: $cmd" ;;
    esac
}

main "$@"
