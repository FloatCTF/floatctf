#!/usr/bin/env bash
#
# FloatCTF clean (Phase 10.8) — 清理当前源码签出（source checkout）里可再生的构建产物.
#
# 目的：
#   只清理"可再生"的源码/构建产物（Cargo target、Web dist、发布包、Python 字节码等），
#   绝不触碰已部署的生产实例（/home/floatctf）或任何系统状态。
#
#   默认（无参数）：
#     - target/                          Cargo 构建缓存（cargo build 可再生）
#     - apps/web/dist/                   Web 构建产物（vite build 可再生）
#     - release/                         本地发布包（可经 install.sh 从 GitHub Release 下载）
#     - scripts/__pycache__/             Python 生成器字节码
#
#   --all（更彻底，额外清理依赖与开发运行时数据）：
#     - node_modules/ 与 apps/web/node_modules/  pnpm 依赖（pnpm install 可再生）
#     - app/                                 开发运行时 WORK_DIR（git 忽略；含水后的
#                                             开发日志/上传/题目文件，仅开发用，可再生）
#
# 安全约束（铁律）：
#   - 所有删除路径一律锚定在仓库根（由脚本路径稳健推导），绝不越出仓库。
#   - 绝不触碰：/home/floatctf、systemd 单元、sysctl/modules 文件、Docker 生产容器/
#     网络、PostgreSQL/RustFS 数据、生产 config/secrets、nftables、WireGuard、主机路由。
#   - 删除前打印将被移除的路径；幂等，重复运行安全。
#
# 用法：
#   ./scripts/clean.sh             # 清默认再生构建产物
#   ./scripts/clean.sh --all       # 额外清依赖与开发运行时数据
#   ./scripts/clean.sh --help
#
set -Eeuo pipefail

# ── 仓库根：稳健地从脚本路径推导，绝不依赖当前工作目录 ─────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

info() { printf '%s[INFO]%s %s\n' "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n' "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n' "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

# 校验路径在仓库根内（防拼写/空变量滑出仓库）。
# 用法：require_within_root "$path_label" "$absolute_path"
require_within_root() {
    local label="$1" path="$2"
    case "$path" in
        "$REPO_ROOT"/*|"$REPO_ROOT")
            ;;
        *)
            die "拒绝删除越出仓库根的路径 [$label]: $path"
            ;;
    esac
}

# 删除单个再生路径（存在才删；先打印；绝对锚定仓库内）。
# 容器基线构建（docker run 模拟 root 写 release/stage）会产生 root/nobody 属主的
# 文件；普通用户删不掉时经 sudo 重试（路径已在 require_within_root 锁定在仓库内，
# 用 sudo 删除仓库内可再生产物是安全的）。两者都失败则告警并非零退出，绝不静默放过。
remove_path() {
    local label="$1" path="$2"
    require_within_root "$label" "$path"
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
        info "跳过（不存在）$label: $path"
        return 0
    fi
    if rm -rf -- "$path" 2>/dev/null; then
        ok "已清理 $label: $path"
        return 0
    fi
    # 属主非当前用户 → 尝试 sudo（失败则明确报错）
    info "普通 rm 失败（容器构建残留 root/nobody 属主？）$label: $path；尝试 sudo"
    if command -v sudo >/dev/null 2>&1 && sudo -n rm -rf -- "$path" 2>/dev/null; then
        ok "已经 sudo 清理 $label: $path"
        return 0
    fi
    die "无法清理 $label（$path 属主非当前用户或 sudo 不可用）；请以 root / sudo 运行并重试"
}

MODE="default"
case "${1:-}" in
    --all) MODE="all" ;;
    -h|--help)
        sed -n '2,34p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    "")
        ;;
    *)
        die "未知参数: $1（支持: 无参数 / --all / --help）"
        ;;
esac

info "FloatCTF clean（MODE=$MODE，仓库根: $REPO_ROOT）"

# ── 默认：可再生构建产物 ───────────────────────────────────────────────────────
remove_path "Cargo target"            "$REPO_ROOT/target"
remove_path "Web dist"                "$REPO_ROOT/apps/web/dist"
remove_path "发布包 release"           "$REPO_ROOT/release"
remove_path "Python 字节码 __pycache__" "$REPO_ROOT/scripts/__pycache__"

# ── --all：额外清理依赖安装与开发运行时数据（均有再生路径）─────────────────────
if [ "$MODE" = "all" ]; then
    info "── 清理依赖安装与开发运行时数据（--all）──"
    remove_path "根 node_modules"            "$REPO_ROOT/node_modules"
    remove_path "apps/web node_modules"      "$REPO_ROOT/apps/web/node_modules"
    remove_path "开发运行时工作目录 app"       "$REPO_ROOT/app"
fi

# ── 保险断言：默认模式下仓库内已无常见大体积再生产物 ───────────────────────────
# （--all 额外清掉的 node_modules/app 不在默认断言内。）

ok "clean 完成：仓库内可再生产物已清理，生产实例（/home/floatctf）与系统状态不受影响"