#!/usr/bin/env bash
set -Eeuo pipefail

# Build AWD/AWDP runtime-service binaries inside Debian Bookworm and package them
# into the matching Bookworm runtime images. This prevents rolling-host GLIBC
# symbols (for example GLIBC_2.39 on Arch) from leaking into production images.
#
# 本脚本是运行时镜像的**唯一构建契约**：本地开发、RC（rc.yml）与正式发布
# （release.yml）都调用它，避免"CI 与本地各写一套"的漂移。
#
# 用法:
#   bash scripts/build-runtime-images.sh [--tag <version>] [--extra-tag <tag>]...
#   FLOATCTF_RUNTIME_IMAGE_TAG=<version> bash scripts/build-runtime-images.sh
#   # CI / 发布（需要先 docker login；本脚本默认**绝不**推送）:
#   bash scripts/build-runtime-images.sh --registry ghcr.io/floatctf --tag <V> --push
#
# 镜像引用映射（--registry 决定前缀）:
#   无 --registry（本地/历史命名，与既有开发工具一致）:
#     floatctf/awd-flagserver:<tag>
#     floatctf/awd-judgeserver:<tag>
#     floatctf/infra/awdp-judgeserver:<tag>      （历史名，保持不变）
#   --registry ghcr.io/floatctf（canonical runtime image refs）:
#     ghcr.io/floatctf/awd-flagserver:<tag>
#     ghcr.io/floatctf/awd-judgeserver:<tag>
#     ghcr.io/floatctf/awdp-judgeserver:<tag>    （awdp **扁平化**，不再带 infra/）
# 这三个 canonical ref 必须与 install.sh 写入 [awd]/[awdp] 的镜像名一致:
#   flagserver_image           = "<registry>/awd-flagserver:<V>"
#   judgeserver_image          = "<registry>/awd-judgeserver:<V>"
#   practice_judgeserver_image = "<registry>/awdp-judgeserver:<V>"
# 否则生产 AWD/AWDP 部署找不到镜像（install.sh 的 ensure_runtime_images 会在安装时
# **硬失败**，并给出 docker pull / docker load 的补救路径）。
#
# 始终**同时**打上 :latest（向后兼容既有宿主/既有配置）；--extra-tag 可追加更多 tag。
# --push 默认关闭；推送只推版本 tag，:latest 是本地兼容 tag，**绝不推送**。
#
# OCI labels：用 --label k=v（可重复）或环境变量 FLOATCTF_BUILD_{SOURCE,VERSION,
# REVISION,CREATED} 提供；同名 key 以 --label 为准。拒绝疑似密钥的 label key。

usage() {
    cat <<'EOF'
用法：scripts/build-runtime-images.sh [选项]

构建 AWD/AWDP 运行时服务镜像（Debian Bookworm 基线，避免宿主 GLIBC 泄漏进生产镜像）。

选项：
  --tag <tag>        主 tag（默认 latest，或环境变量 FLOATCTF_RUNTIME_IMAGE_TAG）。
                     发布/升级请用 --tag <平台版本>（与 install.sh --version 一致，如 1.0.0）。
  --extra-tag <tag>  额外 tag（可重复），同样打在前述三个镜像上（:latest 除外）。
  --registry <prefix>
                     镜像仓库前缀（env：FLOATCTF_RUNTIME_IMAGE_REGISTRY）。
                     留空（默认）= 本地历史命名 floatctf/...；
                     非空 = <prefix>/<name>，其中 awdp 扁平化为 awdp-judgeserver。
                     例：--registry ghcr.io/floatctf
  --label <k=v>      给 docker build 追加 OCI label（可重复）。
  --push             构建 + ldd 体检通过后推送**版本 tag**（绝不推 :latest）。
                     默认**不推送**；推送前需自行 docker login <registry>。
  -h, --help         显示帮助。

镜像引用（按当前解析出的 --registry / --tag）：
EOF
    print_refs "$REGISTRY" "$RUNTIME_TAG"
    cat <<'EOF'

canonical 发布引用示例（--registry ghcr.io/floatctf --tag 1.0.0）：
EOF
    print_refs "ghcr.io/floatctf" "1.0.0"
    cat <<'EOF'

仓库名映射（awdp 扁平化）：
  无 --registry       → floatctf/awd-flagserver、floatctf/awd-judgeserver、
                        floatctf/infra/awdp-judgeserver（历史名，向后兼容）
  --registry <prefix> → <prefix>/awd-flagserver、<prefix>/awd-judgeserver、
                        <prefix>/awdp-judgeserver（不再有 infra/ 段）

环境变量：
  FLOATCTF_RUNTIME_IMAGE_TAG       与 --tag 同义（命令行优先）
  FLOATCTF_RUNTIME_IMAGE_REGISTRY  与 --registry 同义（命令行优先）
  FLOATCTF_RUNTIME_RUST_IMAGE      Rust 构建镜像（默认 rust:1.97.1-slim-bookworm）
  FLOATCTF_RUNTIME_PYTHON_IMAGE    Python 构建镜像（默认 python:3.12-slim-bookworm）
  FLOATCTF_RUNTIME_BUILD_OFFLINE   非空则给 cargo build 加 --offline
  FLOATCTF_BUILD_SOURCE            OCI org.opencontainers.image.source
  FLOATCTF_BUILD_VERSION           OCI org.opencontainers.image.version
  FLOATCTF_BUILD_REVISION          OCI org.opencontainers.image.revision
  FLOATCTF_BUILD_CREATED           OCI org.opencontainers.image.created（ISO8601）
  CARGO_HOME / HOME                复用本机 cargo registry（只读挂载）

三个镜像始终额外打上 :latest（向后兼容，本地 tag，--push 不推送它）。
EOF
}

RUNTIME_TAG="${FLOATCTF_RUNTIME_IMAGE_TAG:-latest}"
REGISTRY="${FLOATCTF_RUNTIME_IMAGE_REGISTRY:-}"
PUSH=0
EXTRA_TAGS=()
CLI_LABELS=()

# 三个逻辑镜像名（同时是 docker build 的镜像 repo 名；awdp 的扁平化见 image_repo）。
IMAGE_LOGICAL=(awd-flagserver awd-judgeserver awdp-judgeserver)

# image_repo <registry-prefix> <logical-name>
#   prefix 非空 → <prefix>/<name>，其中 awdp 扁平化为 awdp-judgeserver；
#   prefix 为空 → 本地历史命名 floatctf/<name>（awdp 为 floatctf/infra/awdp-judgeserver）。
image_repo() {
    local prefix="${1%/}" name="$2"
    case "$name" in
        awdp-judgeserver)
            if [ -n "$prefix" ]; then printf '%s/awdp-judgeserver' "$prefix"
            else printf 'floatctf/infra/awdp-judgeserver'; fi
            ;;
        *)
            if [ -n "$prefix" ]; then printf '%s/%s' "$prefix" "$name"
            else printf 'floatctf/%s' "$name"; fi
            ;;
    esac
}

# image_ref <registry-prefix> <logical-name> <tag>
image_ref() { printf '%s:%s' "$(image_repo "$1" "$2")" "$3"; }

print_refs() { # <registry-prefix> <tag>
    local prefix="$1" tag="$2" name
    for name in "${IMAGE_LOGICAL[@]}"; do
        printf '  %s\n' "$(image_ref "$prefix" "$name" "$tag")"
    done
}

add_cli_label() { # <k=v>
    case "$1" in
        *=*) ;;
        *) echo "--label 需要 k=v 形式: $1" >&2; exit 2 ;;
    esac
    local key="${1%%=*}"
    [[ "$key" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "--label key 非法: $key" >&2; exit 2; }
    case "$key" in
        *[Pp]assword*|*[Ss]ecret*|*[Tt]oken*|*[Kk]ey)
            echo "拒绝把疑似密钥写入 OCI label: $key" >&2; exit 2 ;;
    esac
    CLI_LABELS+=("$1")
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --tag) RUNTIME_TAG="${2:?--tag 需要一个 tag}"; shift ;;
        --extra-tag) EXTRA_TAGS+=("${2:?--extra-tag 需要一个 tag}"); shift ;;
        --registry) REGISTRY="${2:?--registry 需要一个前缀}"; shift ;;
        --label) add_cli_label "${2:?--label 需要 k=v}"; shift ;;
        --push) PUSH=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "未知参数: $1（--help 查看用法）" >&2; exit 2 ;;
    esac
    shift
done
[ -n "$RUNTIME_TAG" ] || { echo '--tag 不能为空' >&2; exit 2; }

# OCI labels：先放环境变量派生值，再放 --label（同名 key 后者覆盖前者）。
LABEL_ARGS=()
add_label() { LABEL_ARGS+=(--label "$1"); }
[ -n "${FLOATCTF_BUILD_SOURCE:-}" ]   && add_label "org.opencontainers.image.source=${FLOATCTF_BUILD_SOURCE}"
[ -n "${FLOATCTF_BUILD_VERSION:-}" ]  && add_label "org.opencontainers.image.version=${FLOATCTF_BUILD_VERSION}"
[ -n "${FLOATCTF_BUILD_REVISION:-}" ] && add_label "org.opencontainers.image.revision=${FLOATCTF_BUILD_REVISION}"
[ -n "${FLOATCTF_BUILD_CREATED:-}" ]  && add_label "org.opencontainers.image.created=${FLOATCTF_BUILD_CREATED}"
if [ "${#CLI_LABELS[@]}" -gt 0 ]; then
    for _cli_label in "${CLI_LABELS[@]}"; do add_label "$_cli_label"; done
fi

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
    "${registry_args[@]+"${registry_args[@]}"}" \
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
# :latest 保留是为了让既有宿主/既有配置继续可用（向后兼容；--push 不推它）。
build_image() { # <logical-name> <context>
    local name="$1" ctx="$2"
    local image
    image="$(image_repo "$REGISTRY" "$name")"
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
    docker build --pull=false "${LABEL_ARGS[@]+"${LABEL_ARGS[@]}"}" "${args[@]}" "$ctx"
}

build_image awd-flagserver "$TMP/flag"
build_image awd-judgeserver "$TMP/awd-judge"
build_image awdp-judgeserver "$TMP/awdp-judge"

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

binary_of() { # <logical-name> -> 容器内二进制路径
    case "$1" in
        awd-flagserver) printf '/usr/local/bin/awd_flagserver' ;;
        awd-judgeserver) printf '/usr/local/bin/awd_judgeserver' ;;
        awdp-judgeserver) printf '/usr/local/bin/awdp_judgeserver' ;;
    esac
}

# 对**被打上 tag 的**镜像做 ldd 校验（主 tag 与 :latest 指向同一 image id，
# 校验主 tag 即可覆盖两者）。
echo "runtime image registry: ${REGISTRY:-<none: local floatctf/... names>}"
echo "runtime image tag: $RUNTIME_TAG（同时打 :latest，本地兼容 tag）"
for _name in "${IMAGE_LOGICAL[@]}"; do
    check_image "$(image_ref "$REGISTRY" "$_name" "$RUNTIME_TAG")" "$(binary_of "$_name")"
done

# --push：只推版本 tag（主 tag + --extra-tag），绝不推 :latest。
# 默认不推送：未显式 --push 时这里什么都不做（不可能误推）。
if [ "$PUSH" = "1" ]; then
    push_tags=()
    for t in "$RUNTIME_TAG" "${EXTRA_TAGS[@]+"${EXTRA_TAGS[@]}"}"; do
        [ "$t" = "latest" ] && continue
        case " ${push_tags[*]-} " in *" $t "*) continue ;; esac
        push_tags+=("$t")
    done
    [ "${#push_tags[@]}" -gt 0 ] \
        || { echo '--push 但没有可推送的版本 tag（--tag 仍是 latest 且无 --extra-tag）' >&2; exit 2; }
    [ -n "$REGISTRY" ] \
        || { echo '--push 需要 --registry <prefix>（本地 floatctf/... 命名不推送到任何 registry）' >&2; exit 2; }
    for _name in "${IMAGE_LOGICAL[@]}"; do
        for t in "${push_tags[@]}"; do
            _ref="$(image_ref "$REGISTRY" "$_name" "$t")"
            echo "docker push $_ref"
            docker push "$_ref"
        done
    done
fi
