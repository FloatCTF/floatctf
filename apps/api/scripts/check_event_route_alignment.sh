#!/usr/bin/env bash
# E9: ensure frontend AWD paths match backend route macros.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
API="$ROOT/src"
WEB="$(cd "$ROOT/../floatctf-web" && pwd)/src"

fail=0
check_backend() {
  local path="$1"
  if ! rg -q --fixed-strings "$path" "$API/modules/event" "$API/api" 2>/dev/null; then
    echo "MISSING backend path fragment: $path"
    fail=1
  else
    echo "ok backend: $path"
  fi
}
check_frontend() {
  local path="$1"
  if ! rg -q --fixed-strings "$path" "$WEB/api" "$WEB/hooks" 2>/dev/null; then
    echo "MISSING frontend path fragment: $path"
    fail=1
  else
    echo "ok frontend: $path"
  fi
}

# Player AWD
for p in \
  '/events/${eventId}/awd/gameboxes' \
  '/events/${eventId}/awd/submissions' \
  '/events/${eventId}/awd/scores' \
  '/events/${eventId}/awd/wireguard/config' \
  ; do
  check_frontend "$p"
done
check_frontend '/events/' # stream uses template
# Backend macros use {event_id}
for p in \
  '/events/{event_id}/awd/gameboxes' \
  '/events/{event_id}/awd/submissions' \
  '/events/{event_id}/awd/scores' \
  '/events/{event_id}/awd/wireguard/config' \
  '/events/{event_id}/awd/stream' \
  '/{event_id}/capabilities' \
  '/events/{event_id}/awd/deploy' \
  '/events/awd' \
  ; do
  check_backend "$p"
done

# Old paths must be absent in clients
if rg -n '/api/awd/|"/awd/events' "$WEB" --glob '*.{ts,tsx}' 2>/dev/null | rg -v 'node_modules|routeTree' ; then
  echo "FAIL: frontend still references legacy /api/awd"
  fail=1
else
  echo "ok: no legacy /api/awd in frontend src"
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "ALL route alignment checks passed"
