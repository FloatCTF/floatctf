#!/usr/bin/env bash
#
# 构建（可选推送）FloatCTF API 镜像。
#
# 为什么单独一个脚本：release.yml 里**禁止**出现裸 `docker push`（推送必须由受守卫的
# 脚本完成，见 scripts/test-runtime-images.sh）。API 镜像与三个运行时镜像的构建方式不同
# （需要 release 二进制 + deploy/api/Dockerfile），因此单独成脚本而不是塞进
# build-runtime-images.sh。
#
# 用法：
#   scripts/build-api-image.sh --binary <path> --tag <V> [--registry <prefix>] [--push]
#                              [--gid <gid>] [--repo <name>]
#
# 默认只构建，不推送；--push 需要 --registry，且 tag 不能是 latest。
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY=""
TAG=""
REGISTRY=""
PUSH=0
GID=65532
REPO_NAME="floatctf-api"

usage() {
    sed -n '3,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

die() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
info() { printf '[INFO] %s\n' "$*"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --binary) [ $# -ge 2 ] || die "--binary 需要 <path>"; BINARY="$2"; shift 2 ;;
        --tag) [ $# -ge 2 ] || die "--tag 需要 <V>"; TAG="$2"; shift 2 ;;
        --registry) [ $# -ge 2 ] || die "--registry 需要 <prefix>"; REGISTRY="${2%/}"; shift 2 ;;
        --repo) [ $# -ge 2 ] || die "--repo 需要 <name>"; REPO_NAME="$2"; shift 2 ;;
        --gid) [ $# -ge 2 ] || die "--gid 需要 <gid>"; GID="$2"; shift 2 ;;
        --push) PUSH=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "未知参数：$1（-h 查看用法）" ;;
    esac
done

[ -n "$BINARY" ] || die "缺少 --binary <release 二进制>"
[ -f "$BINARY" ] || die "二进制不存在：$BINARY"
[ -x "$BINARY" ] || die "二进制不可执行：$BINARY"
[ -n "$TAG" ] || die "缺少 --tag <V>（拒绝 latest/空 tag）"
case "$TAG" in
    latest|"") die "拒绝使用 latest 作为发布 tag" ;;
esac
[[ "$GID" =~ ^[0-9]+$ ]] || die "--gid 必须是数字"

if [ "$PUSH" -eq 1 ]; then
    [ -n "$REGISTRY" ] || die "--push 需要 --registry <prefix>（不允许推到本地命名）"
fi

image="${REPO_NAME}:${TAG}"
[ -n "$REGISTRY" ] && image="${REGISTRY}/${REPO_NAME}:${TAG}"

work="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-api-image.XXXXXX")"
trap 'rm -rf "$work"' EXIT

install -m 0755 "$BINARY" "$work/floatctf"
info "构建 $image（GID=$GID）"
docker build \
    --build-arg "FLOATCTF_VERSION=$TAG" \
    --build-arg "FLOATCTF_GID=$GID" \
    -f "$ROOT/deploy/api/Dockerfile" \
    -t "$image" \
    "$work"

actual_user="$(docker image inspect -f '{{.Config.User}}' "$image")"
[ "$actual_user" = "65532:65532" ] || die "镜像 user=$actual_user，期望 65532:65532"

if [ "$PUSH" -eq 1 ]; then
    info "推送 $image"
    docker push "$image"
else
    info "未推送（缺少 --push）；本地镜像：$image"
fi
