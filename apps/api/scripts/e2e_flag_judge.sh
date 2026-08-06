#!/usr/bin/env bash
# FlagServer / JudgeServer smoke harness.
#
# Default (no RUN_DOCKER_TESTS): cargo-check the bins and exit 0 (skip Docker).
# With RUN_DOCKER_TESTS=1: build Dockerfiles, run labeled containers, smoke health,
# trap cleanup of only those test resources.
#
# Never touches unlabeled / production containers.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CARGO_TERM_COLOR=always
CARGO=(cargo --config 'build.rustc-wrapper=""')

label_uuid="${FLOATCTF_E2E_UUID:-$(uuidgen 2>/dev/null || cat /proc/sys/kernel/random/uuid 2>/dev/null || date +%s)}"
LABEL="floatctf-e2e=${label_uuid}"
FLAG_IMAGE="floatctf-e2e-flagserver:${label_uuid}"
JUDGE_IMAGE="floatctf-e2e-judgeserver:${label_uuid}"
FLAG_NAME="floatctf-e2e-flag-${label_uuid}"
JUDGE_NAME="floatctf-e2e-judge-${label_uuid}"
NET_NAME="floatctf-e2e-net-${label_uuid}"

cleanup() {
  if [[ "${RUN_DOCKER_TESTS:-}" != "1" ]]; then
    return 0
  fi
  if ! command -v docker >/dev/null 2>&1; then
    return 0
  fi
  echo "== cleanup (label ${LABEL}) =="
  # Only remove resources we created for this run.
  docker rm -f "${FLAG_NAME}" >/dev/null 2>&1 || true
  docker rm -f "${JUDGE_NAME}" >/dev/null 2>&1 || true
  docker network rm "${NET_NAME}" >/dev/null 2>&1 || true
  docker rmi -f "${FLAG_IMAGE}" >/dev/null 2>&1 || true
  docker rmi -f "${JUDGE_IMAGE}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "== Flag/Judge E2E harness =="
echo "label=${LABEL}"

if [[ ! -f Dockerfile.awd-flagserver || ! -f Dockerfile.awd-judgeserver ]]; then
  echo "ERROR: Dockerfiles missing (Dockerfile.awd-flagserver / Dockerfile.awd-judgeserver)"
  exit 1
fi

if [[ "${RUN_DOCKER_TESTS:-}" != "1" ]]; then
  echo "RUN_DOCKER_TESTS unset — skipping Docker build/run; cargo-checking bins only."
  "${CARGO[@]}" check --bin awd_flagserver
  "${CARGO[@]}" check --bin awd_judgeserver
  echo "SKIP: Docker E2E (set RUN_DOCKER_TESTS=1 to enable)."
  exit 0
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "RUN_DOCKER_TESTS=1 but docker not available — cargo-checking bins and exiting 0."
  "${CARGO[@]}" check --bin awd_flagserver
  "${CARGO[@]}" check --bin awd_judgeserver
  echo "SKIP: docker CLI missing."
  exit 0
fi

if ! docker info >/dev/null 2>&1; then
  echo "RUN_DOCKER_TESTS=1 but docker daemon unreachable — cargo-checking bins and exiting 0."
  "${CARGO[@]}" check --bin awd_flagserver
  "${CARGO[@]}" check --bin awd_judgeserver
  echo "SKIP: docker daemon not reachable."
  exit 0
fi

echo "== docker build (flagserver) =="
docker build \
  -f Dockerfile.awd-flagserver \
  -t "${FLAG_IMAGE}" \
  --label "${LABEL}" \
  .

echo "== docker build (judgeserver) =="
docker build \
  -f Dockerfile.awd-judgeserver \
  -t "${JUDGE_IMAGE}" \
  --label "${LABEL}" \
  .

echo "== network =="
docker network create --label "${LABEL}" "${NET_NAME}"

# Dummy platform config so processes can bind without a real platform.
# FlagServer needs these env vars at startup; /flag will 5xx without platform — that is OK for smoke.
COMMON_ENV=(
  -e "EVENT_ID=00000000-0000-4000-8000-000000000001"
  -e "INTERNAL_TOKEN=e2e-test-token"
  -e "PLATFORM_INTERNAL_URL=http://127.0.0.1:9"
  -e "LISTEN_ADDR=0.0.0.0:8080"
)

echo "== run flagserver =="
docker run -d \
  --name "${FLAG_NAME}" \
  --label "${LABEL}" \
  --network "${NET_NAME}" \
  -p "0:8080" \
  "${COMMON_ENV[@]}" \
  "${FLAG_IMAGE}"

echo "== run judgeserver =="
docker run -d \
  --name "${JUDGE_NAME}" \
  --label "${LABEL}" \
  --network "${NET_NAME}" \
  -p "0:8080" \
  "${COMMON_ENV[@]}" \
  "${JUDGE_IMAGE}"

# Wait for processes to bind
sleep 2

flag_running="$(docker inspect -f '{{.State.Running}}' "${FLAG_NAME}" 2>/dev/null || echo false)"
judge_running="$(docker inspect -f '{{.State.Running}}' "${JUDGE_NAME}" 2>/dev/null || echo false)"

if [[ "${flag_running}" != "true" ]]; then
  echo "FAIL: flagserver container not running"
  docker logs "${FLAG_NAME}" 2>&1 | tail -50 || true
  exit 1
fi
if [[ "${judge_running}" != "true" ]]; then
  echo "FAIL: judgeserver container not running"
  docker logs "${JUDGE_NAME}" 2>&1 | tail -50 || true
  exit 1
fi

judge_host_port="$(docker inspect -f '{{(index (index .NetworkSettings.Ports "8080/tcp") 0).HostPort}}' "${JUDGE_NAME}")"
flag_host_port="$(docker inspect -f '{{(index (index .NetworkSettings.Ports "8080/tcp") 0).HostPort}}' "${FLAG_NAME}")"

echo "== smoke HTTP =="
# Judge exposes GET /health
if ! curl -fsS --max-time 5 "http://127.0.0.1:${judge_host_port}/health" | grep -q ok; then
  echo "FAIL: judgeserver /health"
  docker logs "${JUDGE_NAME}" 2>&1 | tail -50 || true
  exit 1
fi
echo "judgeserver /health OK (host port ${judge_host_port})"

# FlagServer has no /health; only verify TCP accept / non-empty response path exists.
# GET /flag without platform should return an error status but prove the server is listening.
flag_code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 "http://127.0.0.1:${flag_host_port}/flag" || true)"
if [[ -z "${flag_code}" || "${flag_code}" == "000" ]]; then
  echo "FAIL: flagserver not accepting HTTP (code=${flag_code})"
  docker logs "${FLAG_NAME}" 2>&1 | tail -50 || true
  exit 1
fi
echo "flagserver listening (HTTP ${flag_code} on host port ${flag_host_port})"

echo "== Flag/Judge Docker E2E smoke passed =="
