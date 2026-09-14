#!/usr/bin/env bash
set -Eeuo pipefail

# Build AWD/AWDP runtime-service binaries inside Debian Bookworm and package them
# into the matching Bookworm runtime images. This prevents rolling-host GLIBC
# symbols (for example GLIBC_2.39 on Arch) from leaking into production images.

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

docker build --pull=false -t floatctf/awd-flagserver:latest "$TMP/flag"
docker build --pull=false -t floatctf/awd-judgeserver:latest "$TMP/awd-judge"
docker build --pull=false -t floatctf/infra/awdp-judgeserver:latest "$TMP/awdp-judge"

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
check_image floatctf/awd-flagserver:latest /usr/local/bin/awd_flagserver
check_image floatctf/awd-judgeserver:latest /usr/local/bin/awd_judgeserver
check_image floatctf/infra/awdp-judgeserver:latest /usr/local/bin/awdp_judgeserver
