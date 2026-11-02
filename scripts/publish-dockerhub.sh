#!/usr/bin/env bash
#
# 本地一键发布到 Docker Hub：release 构建 → 装配产物 → 构建 4 个镜像 → 推送。
#
# 与 CI 的关系：release.yml 在 tag 发布时做同样的事（build job + runtime-images job）。
# 本脚本用于「不想打 tag / CI 不可用 / 想先验证 Docker Hub 通道」时手工执行，
# 镜像内容与 tag 发布一致（同一个 Dockerfile、同一套产物装配）。
#
# 用法：
#   DOCKERHUB_USERNAME=floatctf DOCKERHUB_TOKEN=<PAT> \
#     scripts/publish-dockerhub.sh --version 1.0.5 --yes
#
#   --version <V>     平台版本（默认取 package.json 的 version）
#   --registry <ref>  目标 registry 前缀（默认 docker.io/floatctf）
#   --out <dir>       产物目录（默认 release-artifacts）
#   --gid <gid>       API 镜像内 floatctf 组 GID（默认 65532；发布镜像应固定）
#   --skip-build      跳过 pnpm/cargo 构建与产物装配（复用已有产物）
#   --no-login        跳过 docker login（CI/已登录环境）
#   --dry-run         只打印将执行的命令，不做任何构建/推送
#   --yes             跳过确认
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=""
REGISTRY="docker.io/floatctf"
OUT="release-artifacts"
GID=65532
SKIP_BUILD=0
DO_LOGIN=1
DRY_RUN=0
YES=0

die() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
info() { printf '\n[INFO] %s\n' "$*"; }
run() {
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '[dry-run] %s\n' "$*"
    else
        printf '+ %s\n' "$*"
        "$@"
    fi
}

usage() { sed -n '3,24p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --version) [ $# -ge 2 ] || die "--version 需要 <V>"; VERSION="$2"; shift 2 ;;
        --registry) [ $# -ge 2 ] || die "--registry 需要 <prefix>"; REGISTRY="${2%/}"; shift 2 ;;
        --out) [ $# -ge 2 ] || die "--out 需要 <dir>"; OUT="$2"; shift 2 ;;
        --gid) [ $# -ge 2 ] || die "--gid 需要 <gid>"; GID="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        --no-login) DO_LOGIN=0; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        --yes|-y) YES=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "未知参数：$1（--help 查看用法）" ;;
    esac
done

cd "$ROOT"

if [ -z "$VERSION" ]; then
    VERSION="$(/usr/bin/python3 -c 'import json;print(json.load(open("package.json"))["version"])' 2>/dev/null || true)"
fi
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] \
    || die "版本非法：'$VERSION'（期望 x.y.z）"
[[ "$GID" =~ ^[0-9]+$ ]] || die "--gid 必须是数字"

[ "$DRY_RUN" -eq 1 ] || docker info >/dev/null 2>&1 || die "docker daemon 不可用"

IMAGES=(awd-flagserver awd-judgeserver awdp-judgeserver floatctf-api)
printf '版本     : %s\n' "$VERSION"
printf 'registry : %s\n' "$REGISTRY"
printf '产物目录 : %s\n' "$OUT"
printf '镜像     : %s\n' "${IMAGES[*]}"

if [ "$SKIP_BUILD" -eq 0 ]; then
    command -v pnpm >/dev/null 2>&1 || die "pnpm 不在 PATH（先 mise install / 激活 mise 环境）"
    command -v cargo >/dev/null 2>&1 || die "cargo 不在 PATH（先 mise install / 激活 mise 环境）"
    info "1/4 release 构建（packages + web + cargo）"
    run pnpm install --frozen-lockfile
    run pnpm run build:packages
    run pnpm run build:web
    run cargo build --locked --release -p floatctf -p floatctf-helper --bins

    info "2/4 装配发布产物 → $OUT"
    run bash scripts/release-checksums.sh --assemble "$OUT" "$VERSION"
else
    info "跳过构建与装配（--skip-build）"
    [ "$DRY_RUN" -eq 1 ] || [ -x "$OUT/floatctf" ] || die "$OUT/floatctf 不存在，无法 --skip-build"
    for b in awd_flagserver awd_judgeserver awdp_judgeserver; do
        [ "$DRY_RUN" -eq 1 ] || [ -x "target/release/$b" ] || die "target/release/$b 不存在"
    done
fi

if [ "$DRY_RUN" -eq 0 ] && [ "$YES" -eq 0 ]; then
    [ -t 0 ] || die "非交互环境推送需要 --yes"
    printf '\n将构建并推送以上镜像到 %s，继续？[y/N] ' "$REGISTRY"
    read -r ans
    case "$ans" in y|Y|yes) ;; *) die "已取消" ;; esac
fi

if [ "$DO_LOGIN" -eq 1 ]; then
    info "3/4 登录 $REGISTRY"
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '[dry-run] docker login %s -u <DOCKERHUB_USERNAME> --password-stdin\n' "${REGISTRY%%/*}"
    else
        [ -n "${DOCKERHUB_USERNAME:-}" ] && [ -n "${DOCKERHUB_TOKEN:-}" ] \
            || die "需要 DOCKERHUB_USERNAME / DOCKERHUB_TOKEN（或用 --no-login）"
        printf '%s' "$DOCKERHUB_TOKEN" | docker login "${REGISTRY%%/*}" \
            --username "$DOCKERHUB_USERNAME" --password-stdin >/dev/null \
            || die "docker login 失败（PAT 需要 Read & Write）"
        trap 'docker logout "${REGISTRY%%/*}" >/dev/null 2>&1 || true' EXIT
    fi
fi

info "4/4 构建并推送镜像"
run bash scripts/build-runtime-images.sh --registry "$REGISTRY" --tag "$VERSION" --push
run bash scripts/build-api-image.sh --binary "$OUT/floatctf" \
    --registry "$REGISTRY" --tag "$VERSION" --gid "$GID" --push

if [ "$DRY_RUN" -eq 1 ]; then
    info "dry-run 结束，未构建/未推送"
    exit 0
fi

info "推送完成"
for name in "${IMAGES[@]}"; do
    ref="$REGISTRY/$name:$VERSION"
    printf '  %s  %s\n' "$ref" "$(docker image inspect -f '{{range .RepoDigests}}{{.}}{{end}}' "$ref" 2>/dev/null || echo '(no digest)')"
done
