#!/usr/bin/env bash
set -Eeuo pipefail

# Real HTTP + Docker/Judge/RustFS E2E acceptance for every supported AWDP mode:
#   1) Practice / Individual (Training Ground)
#   2) Competition / Individual
#   3) Competition / Team
#
# The harness owns an isolated PostgreSQL DB, Redis and API process. It uses the
# real FloatCTF helper Docker socket, real Docker runtime, real RustFS and a real
# AWDP JudgeServer container. The GameBox fixture is generated locally, then
# imported/built through the public admin HTTP API (no fixture DB injection).

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log()  { printf '[awdp-http-e2e] %s\n' "$*" >&2; }
pass() { printf '[awdp-http-e2e] PASS: %s\n' "$*" >&2; }
fail() { printf '[awdp-http-e2e] FAIL: %s\n' "$*" >&2; return 1; }
need() { command -v "$1" >/dev/null 2>&1 || { printf 'missing command: %s\n' "$1" >&2; exit 2; }; }
for cmd in curl psql createdb dropdb docker python3 cargo ss tar; do need "$cmd"; done

json_code_ok() {
  python3 -c 'import json,sys; raise SystemExit(0 if json.load(sys.stdin).get("code") == 0 else 1)'
}
json_get() {
  local path=$1
  python3 -c 'import json,sys
v=json.load(sys.stdin)
for key in sys.argv[1].split("."):
    if not key: continue
    if key.isdigit():
        v=v[int(key)] if isinstance(v,list) and int(key)<len(v) else None
    else:
        v=v.get(key) if isinstance(v,dict) else None
    if v is None: break
if v is None: print("")
elif isinstance(v,bool): print("true" if v else "false")
elif isinstance(v,(dict,list)): print(json.dumps(v,separators=(",",":")))
else: print(v)' "$path"
}
json_string_obj() {
  python3 -c 'import json,sys; a=sys.argv[1:]; print(json.dumps(dict(zip(a[0::2],a[1::2])),separators=(",",":")))' "$@"
}
iso_time() {
  python3 - "$1" <<'PY'
from datetime import datetime, timezone, timedelta
import sys
print((datetime.now(timezone.utc)+timedelta(seconds=int(sys.argv[1]))).isoformat(timespec='seconds'))
PY
}

SUFFIX="$(date +%s)-$$"
SAFE_SUFFIX="${SUFFIX//-/_}"
DB_NAME="floatctf_awdp_http_e2e_$SAFE_SUFFIX"
REDIS_NAME="floatctf-awdp-http-e2e-redis-$SUFFIX"
TMP="$(mktemp -d "/tmp/floatctf-awdp-http-e2e-${SUFFIX}.XXXXXX")"
CONFIG="$TMP/e2e.toml"
API_LOG="$TMP/api.log"
RESULT_LOG="$TMP/result.log"
WORK_DIR="$TMP/work"
PKG_DIR="$TMP/package/awdp-e2e-$SAFE_SUFFIX"
PKG_ZIP="$TMP/awdp-e2e-$SAFE_SUFFIX.zip"
PATCH_TGZ="$TMP/patch.tar.gz"
mkdir -p "$WORK_DIR" "$PKG_DIR/src" "$PKG_DIR/judge" "$PKG_DIR/awdp"
exec > >(tee -a "$RESULT_LOG") 2> >(tee -a "$RESULT_LOG" >&2)

API_PID=""
API_PORT=""
REDIS_PORT=""
GAMEBOX_ID=""
GAMEBOX_SAFE="awdp-e2e-$SAFE_SUFFIX"
GAMEBOX_REPO="floatctf/gameboxes/$GAMEBOX_SAFE"
PRE_GAMEBOX_IMAGES="$(docker image ls --format '{{.Repository}}:{{.Tag}}' | awk -F: -v repo="$GAMEBOX_REPO" '$1 == repo { print }')"
PRE_PRACTICE_JUDGE="$(docker ps -aq --filter name='^/fctf-awdp-practice-judge$' | head -n1)"
EVENT_IDS=()

sql() {
  PGPASSWORD=postgres psql -X -q -A -t -v ON_ERROR_STOP=1 -h 127.0.0.1 -U postgres -d "$DB_NAME" -c "$1"
}

cleanup() {
  local rc=$?
  set +e
  log "cleanup begin (rc=$rc)"

  # Preserve JudgeServer logs before teardown; they are essential for diagnosing
  # claim/data-plane failures that otherwise disappear with auto-cleanup.
  if ((rc != 0)); then
    docker logs fctf-awdp-practice-judge >"$TMP/practice-judge.log" 2>&1 || true
    for event_id in "${EVENT_IDS[@]:-}"; do
      [[ -n "$event_id" ]] || continue
      compact="${event_id//-/}"
      prefix="${compact:0:12}"
      docker logs "fctf-awdp-judge-$prefix" >"$TMP/event-judge-$prefix.log" 2>&1 || true
    done
  fi

  # Stop the isolated API first so schedulers cannot recreate judges while cleanup runs.
  if [[ -n "$API_PID" ]] && kill -0 "$API_PID" 2>/dev/null; then
    kill "$API_PID" >/dev/null 2>&1 || true
    sleep .4
    kill -9 "$API_PID" >/dev/null 2>&1 || true
  fi

  # Remove all instance containers created by runs in the isolated DB before DB removal.
  if PGPASSWORD=postgres psql -X -q -A -t -h 127.0.0.1 -U postgres -d "$DB_NAME" -c 'SELECT id FROM awdp_runs' >/dev/null 2>&1; then
    while IFS= read -r run_id; do
      [[ -n "$run_id" ]] || continue
      mapfile -t cids < <(docker ps -aq --filter "label=io.floatctf.run_id=$run_id")
      ((${#cids[@]})) && docker rm -f "${cids[@]}" >/dev/null 2>&1 || true
    done < <(sql 'SELECT id FROM awdp_runs' 2>/dev/null || true)
  fi

  for event_id in "${EVENT_IDS[@]:-}"; do
    [[ -n "$event_id" ]] || continue
    compact="${event_id//-/}"
    prefix="${compact:0:12}"
    docker rm -f "fctf-awdp-judge-$prefix" >/dev/null 2>&1 || true
    docker network rm "fctf-awdp-$prefix" >/dev/null 2>&1 || true
  done

  # Practice judge did not exist before this harness: restore the pre-test empty state.
  if [[ -z "$PRE_PRACTICE_JUDGE" ]]; then
    docker rm -f fctf-awdp-practice-judge >/dev/null 2>&1 || true
  fi

  docker rm -f "$REDIS_NAME" >/dev/null 2>&1 || true
  PGPASSWORD=postgres dropdb -h 127.0.0.1 -U postgres --if-exists --force "$DB_NAME" >/dev/null 2>&1 || true

  if [[ -z "$PRE_GAMEBOX_IMAGES" ]]; then
    mapfile -t fixture_images < <(docker image ls --format '{{.Repository}}:{{.Tag}}' | awk -F: -v repo="$GAMEBOX_REPO" '$1 == repo { print }')
    ((${#fixture_images[@]})) && docker image rm -f "${fixture_images[@]}" >/dev/null 2>&1 || true
  fi

  if ((rc == 0)); then
    rm -rf "$TMP"
    pass "isolated DB/Redis/API/AWDP runtime cleanup completed"
  else
    log "failure artifacts kept at $TMP"
    [[ -f "$API_LOG" ]] && { log "API log tail:"; tail -n 160 "$API_LOG" >&2; }
  fi
  exit "$rc"
}
trap cleanup EXIT INT TERM

api_raw() {
  local method=$1 path=$2 token=${3:-} body=${4:-}
  local out status
  out=$(mktemp "$TMP/http.XXXXXX")
  local args=(-sS --max-time 120 -X "$method" "http://127.0.0.1:$API_PORT$path" -H 'Accept: application/json')
  [[ -n "$token" ]] && args+=(-H "Authorization: Bearer $token")
  [[ -n "$body" ]] && args+=(-H 'Content-Type: application/json' --data "$body")
  status=$(curl "${args[@]}" -o "$out" -w '%{http_code}') || {
    local rc=$?
    printf 'curl failed: %s %s (rc=%s)\n' "$method" "$path" "$rc" >&2
    [[ -s "$out" ]] && cat "$out" >&2
    rm -f "$out"
    return "$rc"
  }
  printf '%s\n' "$status"
  cat "$out"
  rm -f "$out"
}
api_ok() {
  local raw status payload
  raw="$(api_raw "$@")"
  status="${raw%%$'\n'*}"
  payload="${raw#*$'\n'}"
  if [[ ! "$status" =~ ^2 ]] || ! json_code_ok <<<"$payload"; then
    printf 'API failed: %s %s -> HTTP %s\n%s\n' "$1" "$2" "$status" "$payload" >&2
    return 1
  fi
  printf '%s' "$payload"
}
api_expect_fail() {
  local raw status payload
  raw="$(api_raw "$@")"
  status="${raw%%$'\n'*}"
  payload="${raw#*$'\n'}"
  if [[ "$status" =~ ^2 ]] && json_code_ok <<<"$payload"; then
    printf 'expected failure but succeeded: %s %s\n%s\n' "$1" "$2" "$payload" >&2
    return 1
  fi
}
api_multipart_ok() {
  local method=$1 path=$2 token=$3
  shift 3
  local out status payload
  out=$(mktemp "$TMP/http.XXXXXX")
  status=$(curl -sS --max-time 240 -X "$method" "http://127.0.0.1:$API_PORT$path" \
    -H 'Accept: application/json' -H "Authorization: Bearer $token" "$@" -o "$out" -w '%{http_code}')
  payload=$(cat "$out"); rm -f "$out"
  if [[ ! "$status" =~ ^2 ]] || ! json_code_ok <<<"$payload"; then
    printf 'multipart API failed: %s %s -> HTTP %s\n%s\n' "$method" "$path" "$status" "$payload" >&2
    return 1
  fi
  printf '%s' "$payload"
}
api_multipart_expect_fail() {
  local method=$1 path=$2 token=$3
  shift 3
  local out status payload
  out=$(mktemp "$TMP/http.XXXXXX")
  status=$(curl -sS --max-time 120 -X "$method" "http://127.0.0.1:$API_PORT$path" \
    -H 'Accept: application/json' -H "Authorization: Bearer $token" "$@" -o "$out" -w '%{http_code}')
  payload=$(cat "$out"); rm -f "$out"
  if [[ "$status" =~ ^2 ]] && json_code_ok <<<"$payload"; then
    printf 'expected multipart failure but succeeded: %s %s\n%s\n' "$method" "$path" "$payload" >&2
    return 1
  fi
}
register_login() {
  local role=$1
  local username="ap_${role}_$SAFE_SUFFIX"
  local password="E2e-${role}-${SUFFIX}-Aa1!"
  api_ok POST /api/users '' "$(json_string_obj username "$username" nickname "$role" password "$password" email "$username@example.invalid")" >/dev/null
  api_ok POST /api/users/session '' "$(json_string_obj username "$username" password "$password")" | json_get data
}
make_awdp_event_json() {
  local mode=$1 title=$2 start=$3 end=$4
  python3 - "$mode" "$title" "$start" "$end" <<'PY'
import json,sys
mode,title,start,end=sys.argv[1:]
print(json.dumps({
  "family":"awdp","participant_mode":mode,"title":title,
  "description":"real AWDP HTTP E2E","hidden":False,"allow_join":True,
  "rules":"e2e rules","flag_prefix":"flag","start_time":start,"end_time":end
},separators=(",",":")))
PY
}
start_common_event_now() {
  local event_id=$1 total_secs=${2:-3600}
  api_ok PATCH "/api/admin/events/$event_id" "$ADMIN_TOKEN" "{\"start_time\":\"$(iso_time -2)\",\"end_time\":\"$(iso_time "$((total_secs - 2))")\"}" >/dev/null
}
endpoint_url_from_instance() {
  python3 -c 'import json,sys; d=(json.load(sys.stdin).get("data") or {}); eps=d.get("endpoints") or []; e=eps[0] if eps else {}; print("http://{}:{}".format(e.get("public_host","127.0.0.1"),e.get("public_port","")))'
}
fetch_flag_via_ssrf() {
  local base=$1 body="" rc=0
  # Container start means the process is running, not necessarily that the HTTP
  # listener is already accepting. Retry this real data-plane probe briefly so
  # stop→start validation tests service readiness rather than a millisecond race.
  for _ in $(seq 1 40); do
    set +e
    body=$(curl -sS --max-time 3 "${base}/?url=http%3A%2F%2Fjudge-server%2Fflag" 2>/dev/null)
    rc=$?
    set -e
    if ((rc == 0)) && [[ "$body" == flag\{* ]]; then
      printf '%s' "$body"
      return 0
    fi
    sleep .1
  done
  printf 'GameBox SSRF flag fetch failed: %s (curl_rc=%s body=%s)\n' "$base" "$rc" "$body" >&2
  return 1
}
assert_judge_dns_from_instance() {
  local container_name=$1 network_name=$2 judge_name=$3
  local aliases resolved
  aliases="$(docker inspect "$judge_name" --format "{{json (index .NetworkSettings.Networks \"$network_name\").Aliases}}" 2>/dev/null || true)"
  resolved="$(docker exec "$container_name" python3 -c 'import socket; print(socket.gethostbyname("judge-server"))' 2>&1 || true)"
  if [[ ! "$resolved" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    fail "GameBox cannot resolve judge-server (instance=$container_name network=$network_name judge_aliases=$aliases resolver=$resolved)"
  fi
  [[ "$aliases" == *'"judge-server"'* ]] || fail "JudgeServer missing judge-server Docker alias (network=$network_name aliases=$aliases)"
}
assert_public_service() {
  local base=$1 want=${2:-vulnerable}
  local body
  for _ in $(seq 1 40); do
    body="$(curl -fsS --max-time 2 "$base/" 2>/dev/null || true)"
    [[ "$body" == *"$want"* ]] && return 0
    sleep .1
  done
  fail "real GameBox public endpoint did not return marker '$want' ($base, body=$body)"
}
assert_sse_connected() {
  local path=$1 token=$2 out
  set +e
  out=$(curl -sS -N --max-time 2 -H "Authorization: Bearer $token" "http://127.0.0.1:$API_PORT$path" 2>/dev/null)
  set -e
  [[ "$out" == *": connected"* ]] || fail "SSE endpoint did not emit connected prelude: $path"
}
assert_source_download() {
  local source_json=$1 outfile=$2
  local path
  path="$(json_get data <<<"$source_json")"
  [[ "$path" == /private/* ]] || fail "source endpoint did not return /private presigned path: $path"
  curl -fsS --max-time 30 "http://127.0.0.1:7780$path" -o "$outfile"
  tar tzf "$outfile" | grep -Eq '(^|/)server.py$' || fail "downloaded source.tar.gz does not contain server.py"
}

# ────────────────────────────────────────────────────────────────────────────
# Build a deterministic, dependency-free real AWDP GameBox package.
# Vulnerable behavior: HTTP SSRF to judge-server/flag. Patch replaces server.py
# with a still-functional but non-SSRF implementation.
# ────────────────────────────────────────────────────────────────────────────
cat >"$PKG_DIR/meta.toml" <<EOF
name = "$GAMEBOX_SAFE"
safe_name = "$GAMEBOX_SAFE"
version = "1.0.0"
author = "FloatCTF E2E"
category = "web"
description = "dependency-free AWDP real E2E GameBox"

[gamebox]
username = "floatctf"

[[gamebox.healthchecks]]
type = "http"
port = 8080
path = "/"
expected_status = 200

[judge]
check_script = "judge/check.py"

[awdp]
source_code_dir = "/app"
exploit_script = "awdp/exploit.py"

[gamebox.recommended_resources]
cpu_millis = 500
memory_bytes = 134217728
pids_limit = 64
EOF
cat >"$PKG_DIR/src/Dockerfile" <<'EOF'
FROM python:3.12-slim-bookworm
WORKDIR /app
COPY server.py /app/server.py
EXPOSE 8080
CMD ["python3", "/app/server.py"]
EOF
cat >"$PKG_DIR/src/server.py" <<'PY'
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs
from urllib.request import urlopen

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        q = parse_qs(urlparse(self.path).query)
        if "url" in q:
            try:
                with urlopen(q["url"][0], timeout=3) as r:
                    body = r.read()
                self.send_response(200)
                self.end_headers()
                self.wfile.write(body)
            except Exception as e:
                self.send_response(502)
                self.end_headers()
                self.wfile.write(str(e).encode())
            return
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"vulnerable-awdp-e2e")
    def log_message(self, *_):
        pass

ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
PY
cat >"$PKG_DIR/judge/check.py" <<'PY'
import json, sys
from urllib.request import urlopen
out=[]
for ip in sys.argv[1:]:
    try:
        with urlopen(f"http://{ip}:8080/", timeout=4) as r:
            ok = r.status == 200
        out.append({"ip":ip,"success":ok,"error":None if ok else "bad status"})
    except Exception as e:
        out.append({"ip":ip,"success":False,"error":str(e)})
print(json.dumps(out))
PY
cat >"$PKG_DIR/awdp/exploit.py" <<'PY'
import json, os, sys
from urllib.parse import quote
from urllib.request import urlopen

# Official Fix evaluations receive a one-shot, target-bound proof URL. Manual
# Test Check intentionally has no proof token, so use JudgeServer /healthz as a
# harmless SSRF witness instead of /flag: competition Fix deliberately does not
# expose flags, while the product still promises manual exploit diagnostics.
proof_url = os.environ.get("FLOATCTF_PROOF_URL")
witness_url = proof_url or "http://judge-server/healthz"
out=[]
for ip in sys.argv[1:]:
    try:
        u=f"http://{ip}:8080/?url="+quote(witness_url, safe="")
        with urlopen(u, timeout=5) as r:
            body=r.read().decode(errors="replace")
        # Require the JudgeServer-controlled response body, not merely HTTP 200:
        # the patched fixture intentionally still returns 200 but never proxies.
        try:
            witness=json.loads(body)
        except json.JSONDecodeError:
            witness={}
        ok = (
            witness.get("proof") == "consumed"
            if proof_url
            else witness.get("status") == "ok"
        )
        out.append({"ip":ip,"success":ok,"error":None if ok else "SSRF witness failed"})
    except Exception as e:
        out.append({"ip":ip,"success":False,"error":str(e)})
print(json.dumps(out))
PY
python3 - "$PKG_DIR" "$PKG_ZIP" <<'PY'
import os,sys,zipfile
root,out=sys.argv[1:]
base=os.path.dirname(root)
with zipfile.ZipFile(out,"w",zipfile.ZIP_DEFLATED) as z:
    for dp,_,files in os.walk(root):
        for f in files:
            p=os.path.join(dp,f)
            z.write(p,os.path.relpath(p,base))
PY

mkdir -p "$TMP/patch/src"
cat >"$TMP/patch/patch.sh" <<'SH'
#!/bin/sh
set -eu
cp ./src/server.py "$FLOATCTF_SOURCE_DIR/server.py"
SH
cat >"$TMP/patch/src/server.py" <<'PY'
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"patched-awdp-e2e")
    def log_message(self, *_):
        pass
ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
PY
python3 - "$TMP/patch" "$PATCH_TGZ" <<'PY'
import os,sys,tarfile
root,out=sys.argv[1:]
with tarfile.open(out,"w:gz") as t:
    t.add(os.path.join(root,"patch.sh"),arcname="patch.sh")
    t.add(os.path.join(root,"src","server.py"),arcname="src/server.py")
PY

# ────────────────────────────────────────────────────────────────────────────
# Isolated infrastructure
# ────────────────────────────────────────────────────────────────────────────
log "preflight + isolated PostgreSQL/Redis/API"
[[ -S /run/floatctf/helper-docker.sock ]] || fail "missing /run/floatctf/helper-docker.sock"
[[ -z "$PRE_PRACTICE_JUDGE" ]] || fail "fctf-awdp-practice-judge already exists; refusing to replace another dev/test JudgeServer"
docker image inspect floatctf/infra/awdp-judgeserver:latest >/dev/null 2>&1 || fail "missing real AWDP judge image floatctf/infra/awdp-judgeserver:latest"
docker image inspect python:3.12-slim-bookworm >/dev/null 2>&1 || fail "missing local python:3.12-slim-bookworm base image"
if [[ "${FLOATCTF_E2E_SKIP_API_BUILD:-0}" != "1" ]]; then
  cargo build -p floatctf >/dev/null
fi
PGPASSWORD=postgres createdb -h 127.0.0.1 -U postgres "$DB_NAME"
docker run -d --name "$REDIS_NAME" -p 127.0.0.1::6379 redis:7-alpine >/dev/null
REDIS_PORT="$(docker port "$REDIS_NAME" 6379/tcp | sed -E 's/.*:([0-9]+)$/\1/' | head -n1)"
for _ in $(seq 1 50); do docker exec "$REDIS_NAME" redis-cli ping 2>/dev/null | grep -q PONG && break; sleep .1; done
docker exec "$REDIS_NAME" redis-cli ping | grep -q PONG || fail "isolated Redis did not become ready"
for p in $(seq 19300 19380); do
  if ! ss -ltnH | awk '{print $4}' | grep -Eq "(^|:)${p}$"; then API_PORT=$p; break; fi
done
[[ -n "$API_PORT" ]] || fail "no free isolated API port"
cp apps/api/config/development.toml "$CONFIG"
python3 - "$CONFIG" "$DB_NAME" "$REDIS_PORT" "$API_PORT" "$WORK_DIR" "$SUFFIX" <<'PY'
from pathlib import Path
import sys,re
p=Path(sys.argv[1]); db,rport,aport,work,suffix=sys.argv[2:]
s=p.read_text()
s=s.replace('main_url = "http://localhost:9090"',f'main_url = "http://127.0.0.1:{aport}"')
s=s.replace('listen_port = 9090',f'listen_port = {aport}',1)
s=s.replace('work_dir = "../../app"',f'work_dir = "{work}"')
s=s.replace('url = "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db"',f'url = "postgres://postgres:postgres@127.0.0.1:5432/{db}"')
s=s.replace('url = "redis://127.0.0.1:6379/"',f'url = "redis://127.0.0.1:{rport}/"')
s=s.replace('channel = "floatctf:realtime"',f'channel = "floatctf:realtime:e2e:{suffix}"')
# AWD callback is irrelevant here, but keep every callback on the isolated API.
s=s.replace('platform_internal_url = "http://127.0.0.1:9090"',f'platform_internal_url = "http://127.0.0.1:{aport}"',1)
# AWDP data-network gateway remains 10.42.2.128; only the isolated API port changes.
s=s.replace('platform_internal_url = "http://10.42.2.128:9090"',f'platform_internal_url = "http://10.42.2.128:{aport}"',1)
p.write_text(s)
PY
FLOATCTF_CONFIG="$CONFIG" apps/api/src/sql/migrate.sh apply >/dev/null
(cd apps/api && exec env FLOATCTF_CONFIG="$CONFIG" ../../target/debug/floatctf) >"$API_LOG" 2>&1 &
API_PID=$!
for _ in $(seq 1 240); do
  kill -0 "$API_PID" 2>/dev/null || { tail -n 160 "$API_LOG" >&2; fail "isolated API exited during startup"; }
  [[ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 1 "http://127.0.0.1:$API_PORT/api/users/me" 2>/dev/null || true)" == "401" ]] && break
  sleep .1
done
[[ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 2 "http://127.0.0.1:$API_PORT/api/users/me" 2>/dev/null || true)" == "401" ]] || fail "isolated API not ready"
pass "isolated API booted through PostgreSQL + Redis + RustFS + helper Docker"

ADMIN_PASSWORD="$(sed -n 's/^- 密码：`\(.*\)`/\1/p' docs/agents/TESTING.md | head -n1)"
[[ -n "$ADMIN_PASSWORD" ]] || fail "local test admin credential unavailable"
ADMIN_TOKEN="$(api_ok POST /api/admin/session '' "$(json_string_obj username sysadmin password "$ADMIN_PASSWORD")" | json_get data)"
unset ADMIN_PASSWORD
[[ -n "$ADMIN_TOKEN" ]] || fail "admin authentication failed"
pass "admin authenticated"

log "import/build/check a real dependency-free AWDP GameBox package through HTTP"
IMPORT="$(api_multipart_ok POST /api/admin/awd/gameboxes/import "$ADMIN_TOKEN" -F "package_zip=@$PKG_ZIP;type=application/zip")"
GAMEBOX_ID="$(json_get data.gamebox.id <<<"$IMPORT")"
[[ "$GAMEBOX_ID" =~ ^[0-9a-f-]{36}$ ]] || fail "GameBox import returned no UUID"
[[ "$(json_get data.gamebox.build_status <<<"$IMPORT")" == "ready" ]] || fail "GameBox import did not become ready"
[[ -n "$(sql "SELECT awdp_source_artifact_key FROM gameboxes WHERE id='$GAMEBOX_ID'")" ]] || fail "AWDP source artifact key not materialized"
CHECK="$(api_ok POST /api/admin/awd/gameboxes/check "$ADMIN_TOKEN" "{\"gamebox_id_list\":[\"$GAMEBOX_ID\"]}")"
[[ "$(json_get data.0.is_ok <<<"$CHECK")" == "true" ]] || fail "GameBox check failed"
BUILD="$(api_ok POST /api/admin/awd/gameboxes/build "$ADMIN_TOKEN" "{\"gamebox_id\":\"$GAMEBOX_ID\"}")"
[[ "$(json_get data.0.is_ok <<<"$BUILD")" == "true" ]] || fail "GameBox ensure/build failed"
api_ok GET '/api/admin/awd/gameboxes?limit=50&page=1' "$ADMIN_TOKEN" | grep -q "$GAMEBOX_ID" || fail "admin GameBox catalog missing imported fixture"
pass "real GameBox package import/build/image pin/source artifact/check + admin catalog"

TOK_A="$(register_login a)"
TOK_B="$(register_login b)"
TOK_C="$(register_login c)"
TOK_D="$(register_login d)"
B_ID="$(sql "SELECT id FROM users WHERE nickname='b'")"
pass "four real users registered and authenticated"

# ────────────────────────────────────────────────────────────────────────────
# Mode 1/3: AWDP Practice / Individual
# ────────────────────────────────────────────────────────────────────────────
log "mode 1/3: AWDP Practice / Individual"
CAT="$(api_ok GET '/api/service/gameboxes?capability=awdp&limit=50&page=1' "$TOK_A")"
[[ "$CAT" == *"$GAMEBOX_ID"* ]] || fail "AWDP training catalog missing imported GameBox"
P0="$(api_ok POST "/api/service/gameboxes/$GAMEBOX_ID/awdp/runs" "$TOK_A")"
P_RUN="$(json_get data.run_id <<<"$P0")"
[[ "$(json_get data.phase <<<"$P0")" == "pending" ]] || fail "new practice run is not pending"
P0B="$(api_ok POST "/api/service/gameboxes/$GAMEBOX_ID/awdp/runs" "$TOK_A")"
[[ "$(json_get data.run_id <<<"$P0B")" == "$P_RUN" ]] || fail "Start Training is not idempotent"
api_ok GET "/api/service/awdp/runs/$P_RUN" "$TOK_A" >/dev/null
api_expect_fail GET "/api/service/awdp/runs/$P_RUN" "$TOK_B" || fail "other user read practice run"
api_expect_fail POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/instance" "$TOK_A" || fail "pending run allowed instance start"
P1="$(api_ok POST "/api/service/awdp/runs/$P_RUN/start" "$TOK_A")"
[[ "$(json_get data.phase <<<"$P1")" == "break" ]] || fail "practice start did not enter break"
P_INSTANCE="$(json_get data.instances.0.instance_id <<<"$P1")"
[[ "$P_INSTANCE" =~ ^[0-9a-f-]{36}$ ]] || fail "practice start returned no instance"
P_INST="$(api_ok GET "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/instance" "$TOK_A")"
P_URL="$(endpoint_url_from_instance <<<"$P_INST")"
assert_public_service "$P_URL" vulnerable
P_CONTAINER="$(sql "SELECT container_name FROM event_instances WHERE id='$P_INSTANCE'")"
assert_judge_dns_from_instance "$P_CONTAINER" fctf-awdp-practice fctf-awdp-practice-judge
api_ok GET '/api/instances?limit=50&page=1' "$TOK_A" | grep -q "$P_INSTANCE" || fail "generic practice instances omitted AWDP runtime"
assert_sse_connected "/api/service/awdp/runs/$P_RUN/stream" "$TOK_A"

P_FLAG="$(fetch_flag_via_ssrf "$P_URL")"
[[ "$P_FLAG" == flag\{* ]] || fail "real JudgeServer /flag was not reachable through GameBox SSRF: $P_FLAG"
P_BAD="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/break" "$TOK_A" '{"flag":"wrong"}')"
[[ "$(json_get data.accepted <<<"$P_BAD")" == "false" ]] || fail "wrong practice break flag accepted"
P_GOOD="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/break" "$TOK_A" "{\"flag\":\"$P_FLAG\"}")"
[[ "$(json_get data.accepted <<<"$P_GOOD")" == "true" && "$(json_get data.scored <<<"$P_GOOD")" == "true" ]] || fail "correct practice break flag did not score"
P_DUP="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/break" "$TOK_A" "{\"flag\":\"$P_FLAG\"}")"
[[ "$(json_get data.already_broken <<<"$P_DUP")" == "true" && "$(json_get data.scored <<<"$P_DUP")" == "false" ]] || fail "duplicate practice break scored twice"
P_SCORE="$(api_ok GET "/api/service/awdp/runs/$P_RUN/scores" "$TOK_A")"
P_BREAK_SCORE="$(json_get data.total <<<"$P_SCORE")"
[[ "$P_BREAK_SCORE" -gt 0 ]] || fail "practice break score not recorded"

# Real stop/start preserves logical instance and endpoint; reset increments generation, practice reset_count remains 0.
api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/instance/stop" "$TOK_A" >/dev/null
[[ "$(sql "SELECT runtime_state FROM event_instances WHERE id='$P_INSTANCE'")" == "stopped" ]] || fail "practice instance stop did not persist"
P_RESTART="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/instance" "$TOK_A")"
[[ "$(json_get data.instance_id <<<"$P_RESTART")" == "$P_INSTANCE" ]] || fail "practice start after stop changed logical instance"
P_GEN0="$(json_get data.runtime_generation <<<"$P_RESTART")"
P_RESET="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/instance/reset" "$TOK_A")"
P_GEN1="$(json_get data.runtime_generation <<<"$P_RESET")"
[[ "$P_GEN1" -gt "$P_GEN0" ]] || fail "practice reset did not increment runtime generation"
[[ "$(json_get data.reset_count <<<"$P_RESET")" == "0" ]] || fail "practice reset_count should remain zero/unlimited"

P_FIX="$(api_ok POST "/api/service/awdp/runs/$P_RUN/phase" "$TOK_A" '{"phase":"fix"}')"
[[ "$(json_get data.phase <<<"$P_FIX")" == "fix" ]] || fail "practice phase switch did not enter fix"
P_ROUNDS="$(api_ok GET "/api/service/awdp/runs/$P_RUN/rounds" "$TOK_A")"
[[ -n "$(json_get data.0.id <<<"$P_ROUNDS")" ]] || fail "practice fix rounds were not materialized"
P_SOURCE="$(api_ok GET "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/source" "$TOK_A")"
assert_source_download "$P_SOURCE" "$TMP/practice-source.tar.gz"

# Before patch: health + judge pass, exploit proves it is still vulnerable; manual check is diagnostic only.
P_MANUAL_V="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/test-check" "$TOK_A")"
[[ "$(json_get data.healthcheck_ok <<<"$P_MANUAL_V")" == "true" ]] || fail "practice vulnerable manual healthcheck failed"
[[ "$(json_get data.judge_ok <<<"$P_MANUAL_V")" == "true" ]] || fail "practice vulnerable manual judge failed"
[[ "$(json_get data.exploit_ok <<<"$P_MANUAL_V")" == "true" ]] || fail "practice vulnerable exploit was not detected"
[[ "$(json_get data.total <<<"$(api_ok GET "/api/service/awdp/runs/$P_RUN/scores" "$TOK_A")")" == "$P_BREAK_SCORE" ]] || fail "manual test-check unexpectedly changed score"

P_PATCH="$(api_multipart_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/patch" "$TOK_A" -F "patch_file=@$PATCH_TGZ;type=application/gzip")"
[[ "$(json_get data.status <<<"$P_PATCH")" == "applied" ]] || fail "practice patch was not applied"
assert_public_service "$P_URL" patched
P_MANUAL_P="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/test-check" "$TOK_A")"
[[ "$(json_get data.healthcheck_ok <<<"$P_MANUAL_P")" == "true" ]] || fail "patched practice healthcheck failed"
[[ "$(json_get data.judge_ok <<<"$P_MANUAL_P")" == "true" ]] || fail "patched practice judge failed"
[[ "$(json_get data.exploit_ok <<<"$P_MANUAL_P")" == "false" ]] || fail "patched practice instance still exploitable"

P_ALL="$(api_ok POST "/api/service/awdp/runs/$P_RUN/gameboxes/$GAMEBOX_ID/all-check" "$TOK_A")"
[[ "$(json_get data.swept <<<"$P_ALL")" == "true" ]] || fail "practice ALL Check did not sweep fixed rounds"
P_ENDED="$(api_ok GET "/api/service/awdp/runs/$P_RUN" "$TOK_A")"
[[ "$(json_get data.phase <<<"$P_ENDED")" == "ended" ]] || fail "ALL Check did not end practice run"
P_FINAL_SCORE="$(json_get data.total <<<"$(api_ok GET "/api/service/awdp/runs/$P_RUN/scores" "$TOK_A")")"
[[ "$P_FINAL_SCORE" -gt "$P_BREAK_SCORE" ]] || fail "ALL Check did not award fix score"
api_ok GET "/api/service/awdp/runs/$P_RUN/evaluations" "$TOK_A" >/dev/null
P_WP="$(api_ok PUT "/api/service/awdp/runs/$P_RUN/writeup" "$TOK_A" '{"content":"awdp practice writeup v1"}')"
[[ "$(json_get data.content <<<"$P_WP")" == "awdp practice writeup v1" ]] || fail "practice writeup save failed"
api_ok PUT "/api/service/awdp/runs/$P_RUN/writeup" "$TOK_A" '{"content":"awdp practice writeup v2"}' >/dev/null
[[ "$(json_get data.content <<<"$(api_ok GET "/api/service/awdp/runs/$P_RUN/writeup" "$TOK_A")")" == "awdp practice writeup v2" ]] || fail "practice writeup upsert/read failed"

P_AGAIN="$(api_ok POST "/api/service/awdp/runs/$P_RUN/restart-training" "$TOK_A")"
P_RUN2="$(json_get data.run_id <<<"$P_AGAIN")"
[[ "$P_RUN2" != "$P_RUN" && "$(json_get data.phase <<<"$P_AGAIN")" == "pending" ]] || fail "Train Again did not create a new pending run"
[[ "$(json_get data.my_score <<<"$P_AGAIN")" == "0" ]] || fail "new practice run inherited old score"
P2_START="$(api_ok POST "/api/service/awdp/runs/$P_RUN2/start" "$TOK_A")"
[[ "$(json_get data.phase <<<"$P2_START")" == "break" ]] || fail "Train Again run did not start"
P2_END="$(api_ok POST "/api/service/awdp/runs/$P_RUN2/end" "$TOK_A")"
[[ "$(json_get data.phase <<<"$P2_END")" == "ended" ]] || fail "manual practice End failed"
pass "Practice: catalog, pending/idempotence, real Judge+GameBox+SSRF flag, break scoring, stop/start/reset, SSE, Fix rounds/source, vulnerable+patched Test Check, real patch, ALL Check scoring/end, writeup, Train Again"

# ────────────────────────────────────────────────────────────────────────────
# Mode 2/3: AWDP Competition / Individual
# ────────────────────────────────────────────────────────────────────────────
log "mode 2/3: AWDP Competition / Individual"
I_START="$(iso_time 600)"; I_END="$(iso_time 3600)"
IEV="$(api_ok POST /api/admin/events "$ADMIN_TOKEN" "$(make_awdp_event_json individual "AWDP Individual $SUFFIX" "$I_START" "$I_END")")"
I_ID="$(json_get data.id <<<"$IEV")"; EVENT_IDS+=("$I_ID")
ICFG="$(api_ok GET "/api/admin/events/$I_ID/awdp" "$ADMIN_TOKEN")"
I_UPDATED="$(json_get data.updated_at <<<"$ICFG")"
ICFG2="$(api_ok PATCH "/api/admin/events/$I_ID/awdp" "$ADMIN_TOKEN" "{\"expected_updated_at\":\"$I_UPDATED\",\"fix_duration_secs\":600,\"fix_round_interval_secs\":120,\"break_score\":70,\"fix_round_score\":11}")"
[[ "$(json_get data.break_score <<<"$ICFG2")" == "70" ]] || fail "individual AWDP config patch failed"
I_ATTACH="$(api_ok POST "/api/admin/events/$I_ID/awdp/gameboxes" "$ADMIN_TOKEN" "{\"gamebox_id\":\"$GAMEBOX_ID\",\"hidden\":false}")"
I_EG="$(json_get data.id <<<"$I_ATTACH")"
api_ok GET "/api/admin/events/$I_ID/awdp/gameboxes" "$ADMIN_TOKEN" | grep -q "$I_EG" || fail "individual attached GameBox missing"
api_ok POST "/api/events/$I_ID/join" "$TOK_B" >/dev/null
api_ok DELETE "/api/events/$I_ID/leave" "$TOK_B" >/dev/null
api_ok POST "/api/events/$I_ID/join" "$TOK_B" >/dev/null
api_ok POST "/api/admin/events/$I_ID/users/$B_ID/banned" "$ADMIN_TOKEN" >/dev/null
api_expect_fail GET "/api/events/$I_ID/awdp" "$TOK_B" || fail "banned individual AWDP participant accessed overview"
api_ok POST "/api/admin/events/$I_ID/users/$B_ID/unbanned" "$ADMIN_TOKEN" >/dev/null
start_common_event_now "$I_ID" 3000
api_expect_fail POST "/api/events/$I_ID/join" "$TOK_C" || fail "late AWDP individual join accepted"
api_expect_fail DELETE "/api/events/$I_ID/leave" "$TOK_B" || fail "AWDP individual left after event start"
api_ok POST "/api/admin/events/$I_ID/awdp/start" "$ADMIN_TOKEN" >/dev/null
I_OV="$(api_ok GET "/api/events/$I_ID/awdp" "$TOK_B")"
[[ "$(json_get data.phase <<<"$I_OV")" == "break" ]] || fail "individual AWDP did not enter break"
I_INSTANCE="$(json_get data.gameboxes.0.instance.instance_id <<<"$I_OV")"
[[ "$I_INSTANCE" =~ ^[0-9a-f-]{36}$ ]] || fail "individual AWDP auto-start did not create instance"
I_INST="$(api_ok GET "/api/events/$I_ID/awdp/gameboxes/$I_EG/instance" "$TOK_B")"
I_URL="$(endpoint_url_from_instance <<<"$I_INST")"
assert_public_service "$I_URL" vulnerable
assert_sse_connected "/api/events/$I_ID/awdp/stream" "$TOK_B"
I_FLAG="$(fetch_flag_via_ssrf "$I_URL")"
[[ "$I_FLAG" == flag\{* ]] || fail "individual AWDP JudgeServer SSRF flag unavailable"
I_BAD="$(api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/break" "$TOK_B" '{"flag":"wrong"}')"
[[ "$(json_get data.accepted <<<"$I_BAD")" == "false" ]] || fail "wrong individual AWDP break flag accepted"
I_GOOD="$(api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/break" "$TOK_B" "{\"flag\":\"$I_FLAG\"}")"
[[ "$(json_get data.scored <<<"$I_GOOD")" == "true" ]] || fail "correct individual AWDP break flag did not score"
I_DUP="$(api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/break" "$TOK_B" "{\"flag\":\"$I_FLAG\"}")"
[[ "$(json_get data.already_broken <<<"$I_DUP")" == "true" && "$(json_get data.scored <<<"$I_DUP")" == "false" ]] || fail "individual AWDP duplicate break scored"
api_ok GET "/api/events/$I_ID/awdp/scores" "$TOK_B" >/dev/null
api_ok GET "/api/events/$I_ID/awdp/trend" "$TOK_B" >/dev/null
api_ok GET "/api/events/$I_ID/awdp/scoreboard" "$TOK_B" >/dev/null
api_ok GET "/api/admin/events/$I_ID/awdp/scores" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/awdp/data" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/awdp/runs" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/awdp/instances" "$ADMIN_TOKEN" | grep -q "$I_INSTANCE" || fail "admin AWDP instances missing individual instance"

# Stop/start is allowed during Break; player Reset is intentionally Fix-only.
api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/instance/stop" "$TOK_B" >/dev/null
api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/instance" "$TOK_B" >/dev/null
api_expect_fail POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/instance/reset" "$TOK_B" || fail "competition reset was accepted during Break"

api_ok POST "/api/admin/events/$I_ID/awdp/break-to-fix" "$ADMIN_TOKEN" >/dev/null
I_FIX_OV="$(api_ok GET "/api/events/$I_ID/awdp" "$TOK_B")"
[[ "$(json_get data.phase <<<"$I_FIX_OV")" == "fix" ]] || fail "individual AWDP did not enter fix"
api_ok GET "/api/events/$I_ID/awdp/rounds" "$TOK_B" | grep -q 'sequence' || fail "individual AWDP rounds missing"
# Competition reset limit/accounting: one player reset in Fix; logical ID remains stable.
I_INST_FIX="$(api_ok GET "/api/events/$I_ID/awdp/gameboxes/$I_EG/instance" "$TOK_B")"
I_GEN0="$(json_get data.runtime_generation <<<"$I_INST_FIX")"
I_RESET="$(api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/instance/reset" "$TOK_B")"
[[ "$(json_get data.instance_id <<<"$I_RESET")" == "$I_INSTANCE" ]] || fail "individual reset changed logical instance"
[[ "$(json_get data.runtime_generation <<<"$I_RESET")" -gt "$I_GEN0" ]] || fail "individual reset did not increment generation"
[[ "$(json_get data.reset_count <<<"$I_RESET")" == "1" ]] || fail "competition reset_count did not increment"
I_SOURCE="$(api_ok GET "/api/events/$I_ID/awdp/gameboxes/$I_EG/source" "$TOK_B")"
assert_source_download "$I_SOURCE" "$TMP/individual-source.tar.gz"
I_MANUAL_V="$(api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/test-check" "$TOK_B")"
[[ "$(json_get data.healthcheck_ok <<<"$I_MANUAL_V")" == "true" && "$(json_get data.judge_ok <<<"$I_MANUAL_V")" == "true" && "$(json_get data.exploit_ok <<<"$I_MANUAL_V")" == "true" ]] || fail "individual AWDP vulnerable Test Check mismatch: $I_MANUAL_V"
I_PATCH="$(api_multipart_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/patch" "$TOK_B" -F "patch_file=@$PATCH_TGZ;type=application/gzip")"
[[ "$(json_get data.status <<<"$I_PATCH")" == "applied" ]] || fail "individual AWDP patch failed"
assert_public_service "$I_URL" patched
I_MANUAL_P="$(api_ok POST "/api/events/$I_ID/awdp/gameboxes/$I_EG/test-check" "$TOK_B")"
[[ "$(json_get data.healthcheck_ok <<<"$I_MANUAL_P")" == "true" && "$(json_get data.judge_ok <<<"$I_MANUAL_P")" == "true" && "$(json_get data.exploit_ok <<<"$I_MANUAL_P")" == "false" ]] || fail "individual AWDP patched Test Check mismatch: $I_MANUAL_P"
api_ok GET "/api/events/$I_ID/awdp/evaluations" "$TOK_B" >/dev/null

api_ok POST "/api/admin/events/$I_ID/awdp/finish" "$ADMIN_TOKEN" >/dev/null
I_END_OV="$(api_ok GET "/api/events/$I_ID/awdp" "$TOK_B")"
[[ "$(json_get data.phase <<<"$I_END_OV")" == "ended" ]] || fail "individual AWDP finish not visible as ended"
# Finish must not leave active competition containers/network/judge around.
I_PREFIX="${I_ID//-/}"; I_PREFIX="${I_PREFIX:0:12}"
[[ -z "$(docker ps -q --filter "label=io.floatctf.run_id=$(sql "SELECT id FROM awdp_runs WHERE event_id='$I_ID' ORDER BY created_at DESC LIMIT 1")")" ]] || fail "individual AWDP finish leaked running instance container"
[[ -z "$(docker ps -aq --filter "name=^/fctf-awdp-judge-$I_PREFIX$")" ]] || fail "individual AWDP finish leaked judge container"
[[ -z "$(docker network ls -q --filter "name=^fctf-awdp-$I_PREFIX$")" ]] || fail "individual AWDP finish leaked event network"
pass "Individual competition: config/mount/join/ban/lifecycle guards, auto-start, real Judge SSRF Break, scoring/scoreboards/data, reset/stop/start, SSE, Fix source/patch/Test Check/evaluations, finish cleanup"

# ────────────────────────────────────────────────────────────────────────────
# Mode 3/3: AWDP Competition / Team
# ────────────────────────────────────────────────────────────────────────────
log "mode 3/3: AWDP Competition / Team"
T_START="$(iso_time 600)"; T_END="$(iso_time 3600)"
TEV="$(api_ok POST /api/admin/events "$ADMIN_TOKEN" "$(make_awdp_event_json team "AWDP Team $SUFFIX" "$T_START" "$T_END")")"
T_ID="$(json_get data.id <<<"$TEV")"; EVENT_IDS+=("$T_ID")
TCFG="$(api_ok GET "/api/admin/events/$T_ID/awdp" "$ADMIN_TOKEN")"
T_UPDATED="$(json_get data.updated_at <<<"$TCFG")"
api_ok PATCH "/api/admin/events/$T_ID/awdp" "$ADMIN_TOKEN" "{\"expected_updated_at\":\"$T_UPDATED\",\"fix_duration_secs\":600,\"fix_round_interval_secs\":120,\"break_score\":80,\"fix_round_score\":13}" >/dev/null
T_ATTACH="$(api_ok POST "/api/admin/events/$T_ID/awdp/gameboxes" "$ADMIN_TOKEN" "{\"gamebox_id\":\"$GAMEBOX_ID\",\"hidden\":false}")"
T_EG="$(json_get data.id <<<"$T_ATTACH")"
TEAM_A="$(api_ok POST "/api/events/$T_ID/team" "$TOK_A" '{"name":"AWDP Alpha"}')"
TEAM_A_ID="$(json_get data.id <<<"$TEAM_A")"
TEAM_C="$(api_ok POST "/api/events/$T_ID/team" "$TOK_C" '{"name":"AWDP Charlie"}')"
TEAM_C_ID="$(json_get data.id <<<"$TEAM_C")"
api_ok POST "/api/events/$T_ID/team/$TEAM_A_ID/join" "$TOK_B" >/dev/null
api_ok POST "/api/events/$T_ID/team/$TEAM_C_ID/join" "$TOK_D" >/dev/null
api_ok POST "/api/events/$T_ID/team/$TEAM_A_ID/leave" "$TOK_B" >/dev/null
api_ok POST "/api/events/$T_ID/team/$TEAM_A_ID/join" "$TOK_B" >/dev/null
api_expect_fail POST "/api/events/$T_ID/team/$TEAM_C_ID/join" "$TOK_B" || fail "AWDP team user joined second team"
api_ok POST "/api/admin/events/$T_ID/teams/$TEAM_A_ID/banned" "$ADMIN_TOKEN" >/dev/null
api_expect_fail GET "/api/events/$T_ID/awdp" "$TOK_A" || fail "banned AWDP team accessed overview"
api_ok POST "/api/admin/events/$T_ID/teams/$TEAM_A_ID/unbanned" "$ADMIN_TOKEN" >/dev/null
start_common_event_now "$T_ID" 3000
api_expect_fail POST "/api/events/$T_ID/team/$TEAM_A_ID/leave" "$TOK_B" || fail "AWDP team member left after start"
api_expect_fail DELETE "/api/events/$T_ID/team/$TEAM_A_ID" "$TOK_A" || fail "AWDP captain quit team after start"
api_expect_fail POST "/api/events/$T_ID/team/$TEAM_A_ID/join" "$TOK_D" || fail "AWDP team roster changed after start"
api_ok POST "/api/admin/events/$T_ID/awdp/start" "$ADMIN_TOKEN" >/dev/null
T_OV_A="$(api_ok GET "/api/events/$T_ID/awdp" "$TOK_A")"
T_OV_B="$(api_ok GET "/api/events/$T_ID/awdp" "$TOK_B")"
T_OV_C="$(api_ok GET "/api/events/$T_ID/awdp" "$TOK_C")"
T_INST_A="$(json_get data.gameboxes.0.instance.instance_id <<<"$T_OV_A")"
T_INST_B="$(json_get data.gameboxes.0.instance.instance_id <<<"$T_OV_B")"
T_INST_C="$(json_get data.gameboxes.0.instance.instance_id <<<"$T_OV_C")"
[[ "$T_INST_A" == "$T_INST_B" ]] || fail "AWDP teammates do not share one team instance"
[[ -n "$T_INST_C" && "$T_INST_C" != "$T_INST_A" ]] || fail "different AWDP teams share the same instance"
T_A_INST="$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_B")"
T_C_INST="$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_C")"
T_URL_A="$(endpoint_url_from_instance <<<"$T_A_INST")"
T_URL_C="$(endpoint_url_from_instance <<<"$T_C_INST")"
assert_public_service "$T_URL_A" vulnerable
assert_public_service "$T_URL_C" vulnerable

# Teammate lifecycle operation affects the shared Alpha instance only, not Charlie.
api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance/stop" "$TOK_B" >/dev/null
[[ "$(sql "SELECT runtime_state FROM event_instances WHERE id='$T_INST_A'")" == "stopped" ]] || fail "AWDP teammate stop did not stop shared Alpha instance"
[[ "$(sql "SELECT runtime_state FROM event_instances WHERE id='$T_INST_C'")" == "running" ]] || fail "AWDP teammate stop affected Charlie instance"
T_A_RESTART="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_A")"
[[ "$(json_get data.instance_id <<<"$T_A_RESTART")" == "$T_INST_A" ]] || fail "AWDP teammate restart changed shared instance ID"
api_expect_fail POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance/reset" "$TOK_B" || fail "AWDP team reset was accepted during Break"

T_FLAG_A="$(fetch_flag_via_ssrf "$T_URL_A")"
T_GOOD="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/break" "$TOK_B" "{\"flag\":\"$T_FLAG_A\"}")"
[[ "$(json_get data.scored <<<"$T_GOOD")" == "true" ]] || fail "AWDP teammate break did not score for team"
T_DUP="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/break" "$TOK_A" "{\"flag\":\"$T_FLAG_A\"}")"
[[ "$(json_get data.already_broken <<<"$T_DUP")" == "true" && "$(json_get data.scored <<<"$T_DUP")" == "false" ]] || fail "AWDP team duplicate break scored twice"
T_SCORE_ROWS="$(api_ok GET "/api/events/$T_ID/awdp/scores" "$TOK_A")"
[[ "$T_SCORE_ROWS" == *"AWDP Alpha"* ]] || fail "AWDP team scoreboard missing Alpha"
api_ok GET "/api/events/$T_ID/awdp/scoreboard" "$TOK_B" >/dev/null
api_ok GET "/api/events/$T_ID/awdp/trend" "$TOK_B" >/dev/null
assert_sse_connected "/api/events/$T_ID/awdp/stream" "$TOK_B"

api_ok POST "/api/admin/events/$T_ID/awdp/break-to-fix" "$ADMIN_TOKEN" >/dev/null
T_FIX_A="$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_A")"
T_FIX_B="$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_B")"
[[ "$(json_get data.instance_id <<<"$T_FIX_A")" == "$(json_get data.instance_id <<<"$T_FIX_B")" ]] || fail "AWDP shared instance diverged after Break→Fix reset"
T_C_FIX_BEFORE="$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_C")"
T_C_GEN0="$(json_get data.runtime_generation <<<"$T_C_FIX_BEFORE")"
T_A_GEN0="$(json_get data.runtime_generation <<<"$T_FIX_B")"
T_A_RESET="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance/reset" "$TOK_B")"
[[ "$(json_get data.instance_id <<<"$T_A_RESET")" == "$T_INST_A" ]] || fail "AWDP team shared reset changed logical instance ID"
[[ "$(json_get data.runtime_generation <<<"$T_A_RESET")" -gt "$T_A_GEN0" ]] || fail "AWDP team shared reset did not increment generation"
[[ "$(json_get data.reset_count <<<"$T_A_RESET")" == "1" ]] || fail "AWDP team shared reset_count incorrect"
[[ "$(json_get data.runtime_generation <<<"$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/instance" "$TOK_C")")" == "$T_C_GEN0" ]] || fail "Alpha reset changed Charlie generation"
T_SOURCE="$(api_ok GET "/api/events/$T_ID/awdp/gameboxes/$T_EG/source" "$TOK_B")"
assert_source_download "$T_SOURCE" "$TMP/team-source.tar.gz"
T_MANUAL_V="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/test-check" "$TOK_B")"
[[ "$(json_get data.exploit_ok <<<"$T_MANUAL_V")" == "true" ]] || fail "AWDP team vulnerable Test Check mismatch: $T_MANUAL_V"
T_PATCH="$(api_multipart_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/patch" "$TOK_B" -F "patch_file=@$PATCH_TGZ;type=application/gzip")"
[[ "$(json_get data.status <<<"$T_PATCH")" == "applied" ]] || fail "AWDP teammate patch failed"
assert_public_service "$T_URL_A" patched
T_MANUAL_P="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/test-check" "$TOK_A")"
[[ "$(json_get data.exploit_ok <<<"$T_MANUAL_P")" == "false" ]] || fail "AWDP teammate patch not visible to teammate Test Check: $T_MANUAL_P"
# Charlie remains vulnerable: cross-team patch isolation.
T_MANUAL_C="$(api_ok POST "/api/events/$T_ID/awdp/gameboxes/$T_EG/test-check" "$TOK_C")"
[[ "$(json_get data.exploit_ok <<<"$T_MANUAL_C")" == "true" ]] || fail "Alpha patch leaked into Charlie team instance: $T_MANUAL_C"
api_ok GET "/api/events/$T_ID/awdp/evaluations" "$TOK_A" >/dev/null
api_ok GET "/api/events/$T_ID/awdp/evaluations" "$TOK_C" >/dev/null
api_ok GET "/api/admin/events/$T_ID/awdp/instances" "$ADMIN_TOKEN" >/dev/null

# Common competition writeup path still honors team ownership in AWDP events.
printf '%%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%%%EOF\n' > "$TMP/awdp-writeup.pdf"
api_multipart_expect_fail POST /api/submit/writeup "$TOK_B" -F "writeup_pdf=@$TMP/awdp-writeup.pdf;type=application/pdf" -F "event_id=$T_ID" -F "team_id=$TEAM_C_ID" || fail "AWDP spoofed team writeup accepted"
api_multipart_ok POST /api/submit/writeup "$TOK_B" -F "writeup_pdf=@$TMP/awdp-writeup.pdf;type=application/pdf" -F "event_id=$T_ID" -F "team_id=$TEAM_A_ID" >/dev/null
api_ok GET "/api/events/$T_ID/own_wp" "$TOK_A" >/dev/null
api_ok GET "/api/admin/events/$T_ID/writeups" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$T_ID/report" "$ADMIN_TOKEN" >/dev/null

api_ok POST "/api/admin/events/$T_ID/awdp/finish" "$ADMIN_TOKEN" >/dev/null
T_END_OV="$(api_ok GET "/api/events/$T_ID/awdp" "$TOK_A")"
[[ "$(json_get data.phase <<<"$T_END_OV")" == "ended" ]] || fail "team AWDP finish not visible as ended"
T_RUN="$(sql "SELECT id FROM awdp_runs WHERE event_id='$T_ID' ORDER BY created_at DESC LIMIT 1")"
T_PREFIX="${T_ID//-/}"; T_PREFIX="${T_PREFIX:0:12}"
[[ -z "$(docker ps -q --filter "label=io.floatctf.run_id=$T_RUN")" ]] || fail "team AWDP finish leaked running instance containers"
[[ -z "$(docker ps -aq --filter "name=^/fctf-awdp-judge-$T_PREFIX$")" ]] || fail "team AWDP finish leaked judge container"
[[ -z "$(docker network ls -q --filter "name=^fctf-awdp-$T_PREFIX$")" ]] || fail "team AWDP finish leaked event network"
pass "Team competition: team lifecycle/lock/ban, per-team auto-start and isolation, teammate shared stop/start/reset/break/patch, real Judge/SSRF, scoreboard/SSE, Fix/source/Test Check, cross-team patch isolation, writeup anti-IDOR, finish cleanup"

# Explicitly prove the invalid 8th combination is rejected at the public boundary.
BAD_START="$(iso_time 600)"; BAD_END="$(iso_time 1200)"
BAD_EVENT=$(python3 - "$BAD_START" "$BAD_END" <<'PY'
import json,sys
print(json.dumps({"family":"awdp","purpose":"practice","participant_mode":"team","title":"invalid awdp practice team","description":"invalid","hidden":False,"allow_join":True,"rules":"x","flag_prefix":"flag","start_time":sys.argv[1],"end_time":sys.argv[2]},separators=(",",":")))
PY
)
api_expect_fail POST /api/admin/events "$ADMIN_TOKEN" "$BAD_EVENT" || fail "invalid AWDP Practice/Team mode was accepted"
pass "invalid AWDP Practice/Team mode rejected"

# Global leak checks for all isolated AWDP runs.
while IFS= read -r run_id; do
  [[ -n "$run_id" ]] || continue
  [[ -z "$(docker ps -q --filter "label=io.floatctf.run_id=$run_id")" ]] || fail "running AWDP container leaked for run $run_id"
done < <(sql 'SELECT id FROM awdp_runs')

printf '\n=== AWDP REAL HTTP E2E SUMMARY ===\n'
printf 'modes: practice/individual, competition/individual, competition/team\n'
printf 'gamebox package: generated fixture imported + built through HTTP API\n'
printf 'runtime: real Docker via helper + real AWDP JudgeServer + real RustFS source artifact\n'
printf 'data plane: GameBox SSRF -> judge-server/flag verified\n'
printf 'fix plane: source download + in-container patch + restart + judge/exploit verification\n'
printf 'RESULT: PASS\n'
