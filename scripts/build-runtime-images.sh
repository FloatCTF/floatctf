#!/usr/bin/env bash
set -Eeuo pipefail

# Build AWD/AWDP runtime-service binaries inside Debian Bookworm and package them
# into the matching Bookworm runtime images. This prevents rolling-host GLIBC
# symbols (for example GLIBC_2.39 on Arch) from leaking into production images.
#
# 用法:
#   sudo bash scripts/build-runtime-images.sh [--tag <version>] [--extra-tag <tag>]...
#   FLOATCTF_RUNTIME_IMAGE_TAG=<version> sudo bash scripts/build-runtime-images.sh
#
# 产出镜像（三个）:
#   floatctf/awd-flagserver:<tag>
#   floatctf/awd-judgeserver:<tag>
#   floatctf/infra/awdp-judgeserver:<tag>
# 始终**同时**打上 :latest（向后兼容既有宿主）；--extra-tag 可追加更多 tag。
# 这些 tag 必须与 install.sh 写入 [awd]/[awdp] 配置的镜像名一致:
#   flagserver_image          = "floatctf/awd-flagserver:${VERSION}"
#   judgeserver_image         = "floatctf/awd-judgeserver:${VERSION}"
#   practice_judgeserver_image= "floatctf/infra/awdp-judgeserver:${VERSION}"
# 否则生产 AWD/AWDP 部署会找不到镜像（install.sh 的 check_runtime_images 会
# 在安装时提前告警，并提示执行本脚本 --tag <VERSION>）。

usage() {
    cat <<'EOF'
用法：scripts/build-runtime-images.sh [选项]

构建 AWD/AWDP 运行时服务镜像（Debian Bookworm 基线，避免宿主 GLIBC 泄漏进生产镜像）。

选项：
  --tag <tag>        主 tag（默认 latest，或环境变量 FLOATCTF_RUNTIME_IMAGE_TAG）。
                     产出 floatctf/awd-flagserver:<tag>、
                     floatctf/awd-judgeserver:<tag>、floatctf/infra/awdp-judgeserver:<tag>。
                     生产升级请用 --tag <平台版本>（与 install.sh --version 一致，如 1.0.0）。
  --extra-tag <tag>  额外 tag（可重复），同样打在前述三个镜像上。
  -h, --help         显示帮助。

环境变量：
  FLOATCTF_RUNTIME_IMAGE_TAG     与 --tag 同义（命令行优先）
  FLOATCTF_RUNTIME_RUST_IMAGE    Rust 构建镜像（默认 rust:1.97.1-slim-bookworm）
  FLOATCTF_RUNTIME_PYTHON_IMAGE  Python 构建镜像（默认 python:3.12-slim-bookworm）
  FLOATCTF_RUNTIME_BUILD_OFFLINE 非空则给 cargo build 加 --offline
  CARGO_HOME / HOME              复用本机 cargo registry（只读挂载）

三个镜像始终额外打上 :latest（向后兼容）。
EOF
}

RUNTIME_TAG="${FLOATCTF_RUNTIME_IMAGE_TAG:-latest}"
EXTRA_TAGS=()
while [ "$#" -gt 0 ]; do
    case "$1" in
        --tag) RUNTIME_TAG="${2:?--tag 需要一个 tag}"; shift ;;
        --extra-tag) EXTRA_TAGS+=("${2:?--extra-tag 需要一个 tag}"); shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "未知参数: $1（--help 查看用法）" >&2; exit 2 ;;
    esac
    shift
done
[ -n "$RUNTIME_TAG" ] || { echo '--tag 不能为空' >&2; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
RUST_IMAGE="${FLOATCTF_RUNTIME_RUST_IMAGE:-rust:1.97.1-slim-bookworm}"
PYTHON_IMAGE="${FLOATCTF_RUNTIME_PYTHON_IMAGE:-python:3.12-slim-bookworm}"
CARGO_REGISTRY="${CARGO_HOME:-$HOME/.cargo}/registry"
TMP="$(mktemp -d /tmp/floatctf-runtime-images.XXXXXX)"
TARGET="$TMP/target"
mkdir -p "$TARGET" "$TMP/flag" "$TMP/awd-judge" "$TMP/awdp-judge"
trap 'rm -rf "$TMP"' EXIT

command -v docker >/dev/null 2>&1 || { echo 'docker is required' >&2; exit 2; }
docker image inspect "$RUST_IMAGE" >/dev/null 2>&1 || docker pull "$RUST_IMAGE"
docker image inspect "$PYTHON_IMAGE" >/dev/null 2>&1 || docker pull "$PYTHON_IMAGE"

registry_args=()
if [[ -d "$CARGO_REGISTRY" ]]; then
    registry_args=(-v "$CARGO_REGISTRY:/usr/local/cargo/registry")
fi

# Run as the invoking user so a mounted Cargo registry never gets root-owned files.
docker run --rm \
    --user "$(id -u):$(id -g)" \
    -e HOME=/tmp \
    -v "$ROOT:/src:ro" \
    "${registry_args[@]}" \
    -v "$TARGET:/target" \
    -w /src \
    -e CARGO_TARGET_DIR=/target \
    "$RUST_IMAGE" \
    cargo build --locked ${FLOATCTF_RUNTIME_BUILD_OFFLINE:+--offline} --release \
      -p floatctf-awd-flagserver \
      -p floatctf-awd-judgeserver \
      -p floatctf-awdp-judgeserver

install -m 0755 "$TARGET/release/awd_flagserver" "$TMP/flag/awd_flagserver"
install -m 0755 "$TARGET/release/awd_judgeserver" "$TMP/awd-judge/awd_judgeserver"
install -m 0755 "$TARGET/release/awdp_judgeserver" "$TMP/awdp-judge/awdp_judgeserver"
cp infra/docker/awd-flagserver/Dockerfile "$TMP/flag/Dockerfile"
cp infra/docker/awd-judgeserver/Dockerfile "$TMP/awd-judge/Dockerfile"
cp infra/docker/awdp-judgeserver/Dockerfile "$TMP/awdp-judge/Dockerfile"

# 每个镜像都打上 :latest + 主 tag + 全部 --extra-tag：
# :latest 保留是为了让既有宿主/既有配置继续可用（向后兼容）。
build_image() { # image context
    local image="$1" ctx="$2"
    local args=(-t "$image:latest") t
    if [ "$RUNTIME_TAG" != "latest" ]; then
        args+=(-t "$image:$RUNTIME_TAG")
    fi
    if [ "${#EXTRA_TAGS[@]}" -gt 0 ]; then
        for t in "${EXTRA_TAGS[@]}"; do
            [ "$t" = "latest" ] && continue
            [ "$t" = "$RUNTIME_TAG" ] && continue
            args+=(-t "$image:$t")
        done
    fi
    docker build --pull=false "${args[@]}" "$ctx"
}

build_image floatctf/awd-flagserver "$TMP/flag"
build_image floatctf/awd-judgeserver "$TMP/awd-judge"
build_image floatctf/infra/awdp-judgeserver "$TMP/awdp-judge"

check_image() {
    local image=$1 binary=$2 output
    output="$(docker run --rm --entrypoint sh "$image" -ec "ldd '$binary' 2>&1")"
    printf '%s\n' "$output"
    if grep -Eq 'not found|GLIBC_[0-9.]+.*not found' <<<"$output"; then
        echo "runtime linker validation failed for $image" >&2
        return 1
    fi
    docker image inspect "$image" --format "$image {{.Id}} {{.Size}}"
}

# 对**被打上 tag 的**镜像做 ldd 校验（主 tag 与 :latest 指向同一 image id，
# 校验主 tag 即可覆盖两者）。
echo "runtime image tag: $RUNTIME_TAG（同时打 :latest）"
check_image "floatctf/awd-flagserver:$RUNTIME_TAG" /usr/local/bin/awd_flagserver
check_image "floatctf/awd-judgeserver:$RUNTIME_TAG" /usr/local/bin/awd_judgeserver
check_image "floatctf/infra/awdp-judgeserver:$RUNTIME_TAG" /usr/local/bin/awdp_judgeserver
