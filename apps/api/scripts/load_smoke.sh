#!/usr/bin/env bash
# Lightweight concurrent HTTP smoke against a running FloatCTF API.
#
# Skip (exit 0) when BASE_URL is unset — safe for default CI.
#
# Required tools (at least one load driver):
#   - curl  (always required for single-shot probes)
#   - hey   (preferred: go install github.com/rakyll/hey@latest)
#   - ab    (optional ApacheBench fallback)
#
# Env:
#   BASE_URL       e.g. http://127.0.0.1:8080   (required to run)
#   CONCURRENCY    default 20
#   REQUESTS       default 200
#   HEALTH_PATH    default /api/health  (fallback /health then /)
#   LOGIN_PATH     optional; if set, also hits this path (GET unless LOGIN_METHOD=POST)
#   LOGIN_METHOD   GET|POST (default GET)
#   TIMEOUT_SECS   per-request timeout default 5
set -euo pipefail

if [[ -z "${BASE_URL:-}" ]]; then
  echo "SKIP: BASE_URL unset — load smoke not run."
  echo "  example: BASE_URL=http://127.0.0.1:8080 ./scripts/load_smoke.sh"
  exit 0
fi

BASE_URL="${BASE_URL%/}"
CONCURRENCY="${CONCURRENCY:-20}"
REQUESTS="${REQUESTS:-200}"
HEALTH_PATH="${HEALTH_PATH:-/api/health}"
TIMEOUT_SECS="${TIMEOUT_SECS:-5}"

if ! command -v curl >/dev/null 2>&1; then
  echo "ERROR: curl is required"
  exit 1
fi

pick_health() {
  local path code
  for path in "${HEALTH_PATH}" /api/health /health /; do
    code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time "${TIMEOUT_SECS}" "${BASE_URL}${path}" || true)"
    if [[ "${code}" =~ ^[23][0-9][0-9]$ ]]; then
      echo "${path}"
      return 0
    fi
  done
  return 1
}

echo "== load smoke against ${BASE_URL} =="
echo "concurrency=${CONCURRENCY} requests=${REQUESTS}"

if ! health_path="$(pick_health)"; then
  echo "FAIL: no reachable health path under ${BASE_URL} (tried HEALTH_PATH, /api/health, /health, /)"
  exit 1
fi
echo "using health path: ${health_path}"

target="${BASE_URL}${health_path}"

run_with_hey() {
  hey -n "${REQUESTS}" -c "${CONCURRENCY}" -t "${TIMEOUT_SECS}" "${target}"
}

run_with_ab() {
  ab -n "${REQUESTS}" -c "${CONCURRENCY}" -s "${TIMEOUT_SECS}" "${target}/" 2>/dev/null \
    || ab -n "${REQUESTS}" -c "${CONCURRENCY}" "${target}"
}

run_with_curl_parallel() {
  # Portable fallback: background curl workers (not a full load generator).
  echo "NOTE: hey/ab missing — using curl xargs fallback (rough concurrency only)"
  seq 1 "${REQUESTS}" | xargs -P "${CONCURRENCY}" -I{} \
    curl -sS -o /dev/null -w '' --max-time "${TIMEOUT_SECS}" "${target}" || true
  # Final probe must succeed
  code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time "${TIMEOUT_SECS}" "${target}")"
  echo "final health HTTP ${code}"
  [[ "${code}" =~ ^[23][0-9][0-9]$ ]]
}

if command -v hey >/dev/null 2>&1; then
  echo "driver: hey"
  run_with_hey
elif command -v ab >/dev/null 2>&1; then
  echo "driver: ab"
  run_with_ab
else
  echo "driver: curl+xargs"
  run_with_curl_parallel
fi

if [[ -n "${LOGIN_PATH:-}" ]]; then
  login_url="${BASE_URL}${LOGIN_PATH}"
  method="${LOGIN_METHOD:-GET}"
  echo "== optional login probe ${method} ${login_url} =="
  if [[ "${method}" == "POST" ]]; then
    curl -sS -o /dev/null -w "login HTTP %{http_code}\n" --max-time "${TIMEOUT_SECS}" \
      -X POST "${login_url}" || true
  else
    curl -sS -o /dev/null -w "login HTTP %{http_code}\n" --max-time "${TIMEOUT_SECS}" \
      "${login_url}" || true
  fi
fi

echo "== load smoke finished =="
