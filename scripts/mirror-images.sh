#!/usr/bin/env bash
#
# 把已构建/已推送的镜像镜像到第二个 registry（同一 digest，不重新构建）。
#
# 为什么需要它：release.yml 里禁止裸 `docker push`（推送必须由受守卫的脚本完成），
# 而 GHCR → Docker Hub 的双推又必须在同一 job 内复用本地镜像（重建会浪费 3~4 分钟）。
#
# 用法：
#   scripts/mirror-images.sh --from ghcr.io/floatctf --to docker.io/floatctf --tag <V>
#                            [--images awd-flagserver,awd-judgeserver,...]
#
# 前置：源镜像已存在于本地 `docker image inspect <from>/<name>:<tag>`。
set -Eeuo pipefail

FROM_PREFIX=""
TO_PREFIX=""
TAG=""
IMAGES_CSV="awd-flagserver,awd-judgeserver,awdp-judgeserver,floatctf-api"

die() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
info() { printf '[INFO] %s\n' "$*"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --from) [ $# -ge 2 ] || die "--from 需要 <prefix>"; FROM_PREFIX="${2%/}"; shift 2 ;;
        --to) [ $# -ge 2 ] || die "--to 需要 <prefix>"; TO_PREFIX="${2%/}"; shift 2 ;;
        --tag) [ $# -ge 2 ] || die "--tag 需要 <V>"; TAG="$2"; shift 2 ;;
        --images) [ $# -ge 2 ] || die "--images 需要逗号分隔的镜像名"; IMAGES_CSV="$2"; shift 2 ;;
        -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "未知参数：$1" ;;
    esac
done

[ -n "$FROM_PREFIX" ] || die "缺少 --from <prefix>"
[ -n "$TO_PREFIX" ] || die "缺少 --to <prefix>"
[ -n "$TAG" ] || die "缺少 --tag <V>"
[ "$TAG" != "latest" ] || die "拒绝镜像 latest tag"
[ "$FROM_PREFIX" != "$TO_PREFIX" ] || die "--from 与 --to 相同，无需镜像"

# Docker Hub 需要单独凭据：release.yml 只保留一处 `docker login`（GHCR，用 GITHUB_TOKEN），
# 这里的登录由脚本内部完成，凭据只经环境变量传入，不落命令行/日志。
if [ "${TO_PREFIX%%/*}" = "docker.io" ]; then
    [ -n "${DOCKERHUB_USERNAME:-}" ] && [ -n "${DOCKERHUB_TOKEN:-}" ] \
        || die "推送到 docker.io 需要 DOCKERHUB_USERNAME / DOCKERHUB_TOKEN 环境变量"
    printf '%s' "$DOCKERHUB_TOKEN" | docker login docker.io \
        --username "$DOCKERHUB_USERNAME" --password-stdin >/dev/null \
        || die "docker login docker.io 失败（检查 PAT 是否为 Read & Write）"
    info "已登录 docker.io（$DOCKERHUB_USERNAME）"
    trap 'docker logout docker.io >/dev/null 2>&1 || true' EXIT
fi

IFS=',' read -r -a names <<<"$IMAGES_CSV"
count=0
for name in "${names[@]}"; do
    [ -n "$name" ] || continue
    src="${FROM_PREFIX}/${name}:${TAG}"
    dst="${TO_PREFIX}/${name}:${TAG}"
    docker image inspect "$src" >/dev/null 2>&1 || die "本地不存在源镜像：$src"
    info "镜像 $src → $dst"
    docker tag "$src" "$dst"
    docker push "$dst"
    count=$((count + 1))
done
[ "$count" -gt 0 ] || die "没有可镜像的镜像名"
info "完成：$count 个镜像已推送到 $TO_PREFIX"
