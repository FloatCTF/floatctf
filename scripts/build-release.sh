#!/usr/bin/env bash
#
# FloatCTF release artifact builder (Phase 10.3) — 可移植发布产物.
#
# 产物：
#   1. API 二进制（musl 静态优先；否则容器内 glibc-2.34 基线构建）
#   2. Web dist（apps/web → vite build && tsc）
#   3. FlagServer / JudgeServer 镜像（容器基线二进制 + bookworm 运行镜像，本地构建，不推送）
#
# 可移植性：
#   - API musl 静态链接 → 任意 Linux 主机直接运行
#   - 容器基线构建 → rust:1.97.1-slim-bookworm 内编译，GLIBC 需求 ≤ 2.34
#     （bookworm glibc 2.36 下界），可跑进任何 bookworm+ 容器与旧主机
#   - AWD 服务永远走容器基线（它们本就运行在 bookworm 容器内）
#   - 审计：file / ldd / objdump -T 验证 GLIBC 需求
#
# 用法：
#   scripts/build-release.sh                # 自动：musl 可用则 musl，否则容器基线
#   scripts/build-release.sh --musl         # 强制 musl（工具链缺失时报错）
#   scripts/build-release.sh --container    # 强制容器 glibc 基线
#   scripts/build-release.sh --version x.y.z
#
set -Eeuo pipefail

# ── 常量 ──────────────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 '^version' "$REPO_ROOT/apps/api/Cargo.toml" | sed -E 's/.*= *"([^"]+)".*/\1/')"
MUSL_TARGET="x86_64-unknown-linux-musl"
BUILDER_IMAGE="rust:1.97.1-slim-bookworm"   # 与生产 toolchain 1.97.1 一致（Phase 9 实测）
OUT_ROOT="$REPO_ROOT/release"
OUT_DIR="$OUT_ROOT/floatctf-$VERSION"
STAGE="$OUT_ROOT/stage"

info() { printf '%s[INFO]%s %s\n' "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n' "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n' "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

MODE="auto"
while [ $# -gt 0 ]; do
    case "$1" in
        --musl) MODE="musl" ;;
        --container) MODE="container" ;;
        --version) VERSION="$2"; shift ;;
        -h|--help) grep -E '^#   |^# 用法' "$0" | sed 's/^#   //'; exit 0 ;;
        *) die "未知参数: $1" ;;
    esac
    shift
done

for c in cargo docker pnpm; do
    command -v "$c" >/dev/null 2>&1 || die "缺少命令: $c"
done

# ── 容器内构建（API + AWD 服务共用一次 cargo 会话）───────────────────────────
# 挂载 host ~/.cargo 为 CARGO_HOME（离线复用依赖缓存）、repo 为 /work；
# 产物经 /work 取回，CARGO_TARGET_DIR=/tmp/tgt 容器内临时，不污染 host target/。
container_build_all() {
    info "容器内 glibc 基线构建（$BUILDER_IMAGE，host cargo cache）..."
    docker run --rm \
        -e CARGO_HOME=/root/.cargo \
        -v "$HOME/.cargo:/root/.cargo" \
        -v "$REPO_ROOT:/work" \
        -w /work \
        "$BUILDER_IMAGE" \
        sh -c '
            set -e
            cd /work
            export RUSTC_WRAPPER= CARGO_TARGET_DIR=/tmp/tgt
            cargo build --release \
                -p floatctf \
                -p floatctf-awd-flagserver \
                -p floatctf-awd-judgeserver
            mkdir -p /work/release/stage/awd-flagserver /work/release/stage/awd-judgeserver
            cp /tmp/tgt/release/floatctf        /work/release/stage/api-floatctf
            cp /tmp/tgt/release/awd_flagserver  /work/release/stage/awd-flagserver/awd_flagserver
            cp /tmp/tgt/release/awd_judgeserver /work/release/stage/awd-judgeserver/awd_judgeserver
        '
    for f in api-floatctf awd-flagserver/awd_flagserver awd-judgeserver/awd_judgeserver; do
        [ -x "$STAGE/$f" ] || die "容器构建未产出: $f"
    done
    ok "容器基线二进制就绪（api + awd 服务）"
}

build_api_musl() {
    info "musl 静态构建（$MUSL_TARGET）..."
    rustup target list --installed 2>/dev/null | grep -q "$MUSL_TARGET" \
        || die "musl target 未安装：rustup target add $MUSL_TARGET"
    command -v musl-gcc >/dev/null 2>&1 || die "缺少 musl-gcc（Arch: pacman -S musl）"
    ( cd "$REPO_ROOT" && RUSTC_WRAPPER= cargo build --release --target "$MUSL_TARGET" -p floatctf )
    local bin="$REPO_ROOT/target/$MUSL_TARGET/release/floatctf"
    [ -x "$bin" ] || die "musl 构建未产出二进制"
    mkdir -p "$STAGE"
    cp "$bin" "$STAGE/api-floatctf"
    ok "musl API 二进制就绪"
}

# AWD 服务镜像：容器基线二进制 + 生产 Dockerfile（infra/docker/awd-*）
build_awd_image() {
    local name="$1" binary="$2" image="$3"
    info "构建镜像 $image..."
    docker build -t "$image:$VERSION" -t "$image:latest" \
        -f "$REPO_ROOT/infra/docker/$name/Dockerfile" \
        "$STAGE/$name"
    ok "镜像 $image 就绪（:$VERSION = :latest）"
}

audit_binary() {
    local bin="$1"
    [ -f "$bin" ] || die "审计目标不存在: $bin"
    file "$bin" | grep -q ELF || die "非 ELF: $bin"
    local kind glibc_max="静态/无需"
    if file "$bin" | grep -q "statically linked"; then
        kind="静态链接"
    else
        kind="动态链接"
        ldd "$bin" 2>&1 | grep -q "not found" && die "动态依赖缺失: $bin"
        glibc_max=$(objdump -T "$bin" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -Vu | tail -1) || true
    fi
    ok "审计 $bin: $kind（GLIBC 需求 ≤ $glibc_max）"
}

main() {
    info "FloatCTF release $VERSION（MODE=$MODE）"
    rm -rf "$OUT_DIR" "$STAGE"
    mkdir -p "$OUT_DIR"/{bin,web,images} "$STAGE"

    case "$MODE" in
        musl)
            build_api_musl
            container_build_all   # AWD 服务仍需容器基线（bookworm 运行环境）
            cp "$STAGE/api-floatctf" "$OUT_DIR/bin/floatctf"
            ;;
        container)
            container_build_all
            cp "$STAGE/api-floatctf" "$OUT_DIR/bin/floatctf"
            ;;
        auto)
            if command -v rustup >/dev/null 2>&1 && command -v musl-gcc >/dev/null 2>&1 \
                && rustup target list --installed 2>/dev/null | grep -q "$MUSL_TARGET"; then
                build_api_musl
                cp "$STAGE/api-floatctf" "$OUT_DIR/bin/floatctf"
            else
                info "musl 工具链不可用（缺 rustup/musl-gcc/target），回退容器 glibc 基线"
                container_build_all
                cp "$STAGE/api-floatctf" "$OUT_DIR/bin/floatctf"
            fi
            ;;
    esac

    audit_binary "$OUT_DIR/bin/floatctf"
    audit_binary "$STAGE/awd-flagserver/awd_flagserver"
    audit_binary "$STAGE/awd-judgeserver/awd_judgeserver"

    build_web

    build_awd_image "awd-flagserver"  "awd_flagserver"  "floatctf/awd-flagserver"
    build_awd_image "awd-judgeserver" "awd_judgeserver" "floatctf/awd-judgeserver"

    ( cd "$OUT_DIR" && find . -type f | sort > manifest.txt && sha256sum $(find . -type f | sort) > checksums.txt )
    rm -rf "$STAGE" 2>/dev/null || true

    ok "发布产物: $OUT_DIR"
    du -sh "$OUT_DIR"
    cat "$OUT_DIR/manifest.txt"
}

# Web dist 构建放 main 前面声明以保持可读；这里单独定义避免 build 流程被拆散
build_web() {
    info "构建 Web dist（pnpm --filter @floatctf/web build）..."
    ( cd "$REPO_ROOT" && pnpm --filter @floatctf/web build )
    [ -d "$REPO_ROOT/apps/web/dist" ] || die "web dist 未产出"
    cp -r "$REPO_ROOT/apps/web/dist/." "$OUT_DIR/web/"
    ok "Web dist 就绪"
}

main
