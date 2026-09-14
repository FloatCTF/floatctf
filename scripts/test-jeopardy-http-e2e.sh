#!/usr/bin/env bash
set -Eeuo pipefail

# Real HTTP E2E acceptance for every supported Jeopardy mode:
#   1) Practice / Individual
#   2) Competition / Individual
#   3) Competition / Team
# The harness owns an isolated PostgreSQL DB, Redis and API process, but uses
# the real FloatCTF helper Docker socket, real Docker runtime and real RustFS.
# The challenge fixture itself is imported and built through the public admin API.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log()  { printf '[jeopardy-http-e2e] %s\n' "$*" >&2; }
pass() { printf '[jeopardy-http-e2e] PASS: %s\n' "$*" >&2; }
fail() { printf '[jeopardy-http-e2e] FAIL: %s\n' "$*" >&2; return 1; }
need() { command -v "$1" >/dev/null 2>&1 || { printf 'missing command: %s\n' "$1" >&2; exit 2; }; }
for cmd in curl psql createdb dropdb docker python3 cargo ss; do need "$cmd"; done

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

SUFFIX="$(date +%s)-$$"
DB_NAME="floatctf_jeopardy_http_e2e_${SUFFIX//-/_}"
REDIS_NAME="floatctf-jeopardy-http-e2e-redis-$SUFFIX"
TMP="$(mktemp -d "/tmp/floatctf-jeopardy-http-e2e-${SUFFIX}.XXXXXX")"
CONFIG="$TMP/e2e.toml"
API_LOG="$TMP/api.log"
RESULT_LOG="$TMP/result.log"
WORK_DIR="$TMP/work"
mkdir -p "$WORK_DIR"
exec > >(tee -a "$RESULT_LOG") 2> >(tee -a "$RESULT_LOG" >&2)

API_PID=""
API_PORT=""
REDIS_PORT=""
PRE_TEST_C_IMAGE="$(docker image inspect floatctf/challenges/test-c:1.0.0 --format '{{.Id}}' 2>/dev/null || true)"
PRE_PRACTICE_JUDGE="$(docker ps -aq --filter name='^/fctf-awdp-practice-judge$' | head -n1)"

cleanup() {
  local rc=$?
  set +e
  log "cleanup begin (rc=$rc)"
  mapfile -t ids < <(docker ps -a --format '{{.Names}}' | grep -E '^(JP|JS|JT)-' || true)
  if ((${#ids[@]})); then docker rm -f "${ids[@]}" >/dev/null 2>&1 || true; fi
  if [[ -n "$API_PID" ]] && kill -0 "$API_PID" 2>/dev/null; then
    kill "$API_PID" >/dev/null 2>&1 || true
    sleep 0.4
    kill -9 "$API_PID" >/dev/null 2>&1 || true
  fi
  # API startup runs the global AWDP practice-environment ensure task even though
  # this harness only tests Jeopardy. If no practice judge existed before the
  # harness, remove the one this isolated API created so later suites do not
  # inherit a worker whose callback points at a dead test API.
  if [[ -z "$PRE_PRACTICE_JUDGE" ]]; then
    docker rm -f fctf-awdp-practice-judge >/dev/null 2>&1 || true
  fi
  docker rm -f "$REDIS_NAME" >/dev/null 2>&1 || true
  PGPASSWORD=postgres dropdb -h 127.0.0.1 -U postgres --if-exists --force "$DB_NAME" >/dev/null 2>&1 || true
  if [[ -z "$PRE_TEST_C_IMAGE" ]]; then
    docker image rm -f floatctf/challenges/test-c:1.0.0 >/dev/null 2>&1 || true
  fi
  if ((rc == 0)); then
    rm -rf "$TMP"
    pass "isolated DB/Redis/API/runtime cleanup completed"
  else
    log "failure artifacts kept at $TMP"
  fi
  exit "$rc"
}
trap cleanup EXIT INT TERM

sql() {
  PGPASSWORD=postgres psql -X -q -A -t -v ON_ERROR_STOP=1 -h 127.0.0.1 -U postgres -d "$DB_NAME" -c "$1"
}
api_raw() {
  local method=$1 path=$2 token=${3:-} body=${4:-}
  local out status
  out=$(mktemp "$TMP/http.XXXXXX")
  local args=(-sS --max-time 90 -X "$method" "http://127.0.0.1:$API_PORT$path" -H 'Accept: application/json')
  [[ -n "$token" ]] && args+=(-H "Authorization: Bearer $token")
  [[ -n "$body" ]] && args+=(-H 'Content-Type: application/json' --data "$body")
  status=$(curl "${args[@]}" -o "$out" -w '%{http_code}')
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
  status=$(curl -sS --max-time 180 -X "$method" "http://127.0.0.1:$API_PORT$path" \
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
  status=$(curl -sS --max-time 90 -X "$method" "http://127.0.0.1:$API_PORT$path" \
    -H 'Accept: application/json' -H "Authorization: Bearer $token" "$@" -o "$out" -w '%{http_code}')
  payload=$(cat "$out"); rm -f "$out"
  if [[ "$status" =~ ^2 ]] && json_code_ok <<<"$payload"; then
    printf 'expected multipart failure but succeeded: %s %s\n%s\n' "$method" "$path" "$payload" >&2
    return 1
  fi
}
iso_time() {
  python3 - "$1" <<'PY'
from datetime import datetime, timezone, timedelta
import sys
print((datetime.now(timezone.utc)+timedelta(seconds=int(sys.argv[1]))).isoformat(timespec='seconds'))
PY
}
register_login() {
  local role=$1
  local username="je_${role}_${SUFFIX//-/_}"
  local password="E2e-${role}-${SUFFIX}-Aa1!"
  api_ok POST /api/users '' "$(json_string_obj username "$username" nickname "$role" password "$password" email "$username@example.invalid")" >/dev/null
  api_ok POST /api/users/session '' "$(json_string_obj username "$username" password "$password")" | json_get data
}
make_event_json() {
  local mode=$1 title=$2 start=$3 end=$4
  python3 - "$mode" "$title" "$start" "$end" <<'PY'
import json,sys
mode,title,start,end=sys.argv[1:]
print(json.dumps({
  "family":"jeopardy","participant_mode":mode,"title":title,
  "description":"real HTTP E2E","hidden":False,"allow_join":True,
  "rules":"e2e rules","flag_prefix":"flag","start_time":start,"end_time":end
},separators=(",",":")))
PY
}
launch() {
  local token=$1 event_id=${2:-}
  if [[ -n "$event_id" ]]; then
    api_ok POST /api/instances/launch "$token" "{\"event_id\":\"$event_id\",\"challenge_id\":\"$CHALLENGE_ID\"}"
  else
    api_ok POST /api/instances/launch "$token" "{\"challenge_id\":\"$CHALLENGE_ID\"}"
  fi
}
instance_flag() { sql "SELECT flag FROM event_challenge_instance WHERE id='$1'"; }
instance_url() {
  python3 -c 'import json,sys,re; o=json.load(sys.stdin); s=(o.get("data") or {}).get("content") or ""; m=re.search(r"https?://[^\"< ]+",s); print(m.group(0) if m else "")'
}
assert_runtime_http() {
  local url=$1 expected=$2
  [[ -n "$url" ]] || fail "instance returned no runtime URL"
  local body=""
  # Container create/start returning is not an application-readiness guarantee.
  # Apache/PHP can briefly reset/refuse the first connection while its entrypoint
  # is still coming up, so poll the actual data plane before declaring the E2E bad.
  for _ in $(seq 1 40); do
    body=$(curl -fsS --max-time 2 "$url" 2>/dev/null || true)
    if [[ "$body" == *"$expected"* ]]; then
      return 0
    fi
    sleep .25
  done
  printf 'runtime did not become ready: %s\nlast body: %s\n' "$url" "$body" >&2
  fail "real challenge HTTP did not expose its injected dynamic flag"
}

log "preflight + isolated infrastructure"
[[ -S /run/floatctf/helper-docker.sock ]] || fail "missing /run/floatctf/helper-docker.sock"
if [[ "${FLOATCTF_E2E_SKIP_API_BUILD:-0}" != "1" ]]; then
  cargo build -p floatctf >/dev/null
fi
PGPASSWORD=postgres createdb -h 127.0.0.1 -U postgres "$DB_NAME"
docker run -d --name "$REDIS_NAME" -p 127.0.0.1::6379 redis:7-alpine >/dev/null
REDIS_PORT="$(docker port "$REDIS_NAME" 6379/tcp | sed -E 's/.*:([0-9]+)$/\1/' | head -n1)"
for _ in $(seq 1 50); do docker exec "$REDIS_NAME" redis-cli ping 2>/dev/null | grep -q PONG && break; sleep .1; done
docker exec "$REDIS_NAME" redis-cli ping | grep -q PONG || fail "isolated Redis did not become ready"
for p in $(seq 19200 19250); do
  if ! ss -ltnH | awk '{print $4}' | grep -Eq "(^|:)${p}$"; then API_PORT=$p; break; fi
done
[[ -n "$API_PORT" ]] || fail "no free isolated API port"
cp apps/api/config/development.toml "$CONFIG"
python3 - "$CONFIG" "$DB_NAME" "$REDIS_PORT" "$API_PORT" "$WORK_DIR" "$SUFFIX" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1]); db,rport,aport,work,suffix=sys.argv[2:]
s=p.read_text()
s=s.replace('main_url = "http://localhost:9090"',f'main_url = "http://127.0.0.1:{aport}"')
s=s.replace('listen_port = 9090',f'listen_port = {aport}',1)
s=s.replace('work_dir = "../../app"',f'work_dir = "{work}"')
s=s.replace('url = "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db"',f'url = "postgres://postgres:postgres@127.0.0.1:5432/{db}"')
s=s.replace('url = "redis://127.0.0.1:6379/"',f'url = "redis://127.0.0.1:{rport}/"')
s=s.replace('channel = "floatctf:realtime"',f'channel = "floatctf:realtime:e2e:{suffix}"')
s=s.replace('platform_internal_url = "http://127.0.0.1:9090"',f'platform_internal_url = "http://127.0.0.1:{aport}"',1)
p.write_text(s)
PY
FLOATCTF_CONFIG="$CONFIG" apps/api/src/sql/migrate.sh apply >/dev/null
[[ "$(sql 'SELECT count(*) FROM schema_migrations')" == "45" ]] || fail "expected 45 migrations"
(cd apps/api && exec env FLOATCTF_CONFIG="$CONFIG" ../../target/debug/floatctf) >"$API_LOG" 2>&1 &
API_PID=$!
for _ in $(seq 1 180); do
  kill -0 "$API_PID" 2>/dev/null || { tail -n 120 "$API_LOG" >&2; fail "isolated API exited during startup"; }
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

log "import/build/check a real challenge package through HTTP"
IMPORT="$(api_multipart_ok POST /api/admin/challenges/import "$ADMIN_TOKEN" -F "package_zip=@$ROOT/examples/test-c.zip;type=application/zip")"
CHALLENGE_ID="$(json_get data.challenge.id <<<"$IMPORT")"
[[ "$CHALLENGE_ID" =~ ^[0-9a-f-]{36}$ ]] || fail "challenge import returned no UUID"
[[ "$(json_get data.challenge.build_status <<<"$IMPORT")" == "ready" ]] || fail "challenge import did not become ready"
CHECK="$(api_ok POST /api/admin/challenges/check "$ADMIN_TOKEN" "{\"challenge_id_list\":[\"$CHALLENGE_ID\"]}")"
[[ "$(json_get data.0.is_ok <<<"$CHECK")" == "true" ]] || fail "challenge check failed"
BUILD="$(api_ok POST /api/admin/challenges/build "$ADMIN_TOKEN" "{\"challenge_id\":\"$CHALLENGE_ID\"}")"
[[ "$(json_get data.0.is_ok <<<"$BUILD")" == "true" ]] || fail "challenge build ensure failed"
api_ok GET "/api/admin/challenges/$CHALLENGE_ID" "$ADMIN_TOKEN" >/dev/null
api_ok GET '/api/admin/challenges?limit=10&page=1' "$ADMIN_TOKEN" >/dev/null
pass "real challenge package import/build/pin/check + admin catalog"

TOK_A="$(register_login a)"
TOK_B="$(register_login b)"
TOK_C="$(register_login c)"
TOK_D="$(register_login d)"
pass "four real users registered and authenticated"

# ────────────────────────────────────────────────────────────────────────────
# Jeopardy Practice / Individual
# ────────────────────────────────────────────────────────────────────────────
log "mode 1/3: Jeopardy Practice / Individual"
CAT="$(api_ok GET '/api/challenges?limit=50&page=1' "$TOK_A")"
[[ "$CAT" == *"$CHALLENGE_ID"* ]] || fail "practice catalog missing imported challenge"
api_ok GET "/api/challenges/$CHALLENGE_ID" "$TOK_A" >/dev/null
P1="$(launch "$TOK_A")"
P1_ID="$(json_get data.id <<<"$P1")"
P1_FLAG="$(instance_flag "$P1_ID")"
P1_URL="$(instance_url <<<"$P1")"
[[ -n "$P1_FLAG" ]] || fail "practice runtime has no stored flag"
assert_runtime_http "$P1_URL" "$P1_FLAG"
api_ok GET "/api/instances/$P1_ID" "$TOK_A" >/dev/null
api_ok GET '/api/instances?limit=20&page=1' "$TOK_A" | grep -q "$P1_ID" || fail "practice instance list missing runtime"
api_ok GET "/api/challenges/$CHALLENGE_ID/instance" "$TOK_A" >/dev/null
api_expect_fail POST /api/submit/flag "$TOK_A" "{\"instance_id\":\"$P1_ID\",\"flag\":\"wrong\"}" || fail "wrong practice flag accepted"
api_ok POST /api/submit/flag "$TOK_A" "{\"instance_id\":\"$P1_ID\",\"flag\":\"$P1_FLAG\"}" >/dev/null
[[ "$(sql "SELECT runtime_state FROM event_instances WHERE id='$P1_ID'")" == "completed" ]] || fail "practice solve did not complete runtime"
A_ID="$(sql "SELECT id FROM users WHERE nickname='a'")"
[[ "$(sql "SELECT count(*) FROM jeopardy_challenge_solves WHERE challenge_id='$CHALLENGE_ID' AND user_id='$A_ID' AND obtained_points=0")" == "1" ]] || fail "practice zero-point solved marker missing"
P2="$(launch "$TOK_A")"
P2_ID="$(json_get data.id <<<"$P2")"
P2_FLAG="$(instance_flag "$P2_ID")"
[[ "$P2_ID" != "$P1_ID" ]] || fail "retraining reused completed row"
[[ "$P2_FLAG" != "$P1_FLAG" ]] || fail "retraining did not rotate dynamic flag"
api_ok DELETE "/api/instances/$P2_ID" "$TOK_A" >/dev/null
P3="$(launch "$TOK_A")"
P3_ID="$(json_get data.id <<<"$P3")"
api_ok DELETE /api/instances "$TOK_A" "{\"id_list\":[\"$P3_ID\"]}" >/dev/null
api_ok POST "/api/challenges/$CHALLENGE_ID/my_writeup" "$TOK_A" '{"content":"practice writeup v1"}' >/dev/null
WP="$(api_ok POST "/api/challenges/$CHALLENGE_ID/my_writeup" "$TOK_A" '{"content":"practice writeup v2"}')"
WP_ID="$(json_get data.id <<<"$WP")"
MY_WP="$(api_ok GET "/api/challenges/$CHALLENGE_ID/my_writeup" "$TOK_A")"
[[ "$(json_get data.content <<<"$MY_WP")" == "practice writeup v2" ]] || fail "challenge writeup upsert failed"
api_ok GET "/api/challenges/$CHALLENGE_ID/writeups" "$TOK_B" | grep -q "$WP_ID" || fail "public challenge writeup list missing row"
api_ok GET "/api/writeups/$WP_ID" "$TOK_B" >/dev/null
api_ok GET /api/solves "$TOK_A" >/dev/null
api_ok GET /api/solves/top15users "$TOK_A" >/dev/null
pass "Practice: catalog/detail, real container HTTP, launch/get/list, wrong+correct flag, solve marker, retrain rotation, destroy/bulk destroy, writeup/solves"

mount_challenge() {
  api_ok POST "/api/admin/events/$1/challenges" "$ADMIN_TOKEN" "{\"challenge_id\":\"$CHALLENGE_ID\",\"points\":100}" >/dev/null
}
create_announcement() {
  api_ok POST "/api/admin/events/$1/announcements" "$ADMIN_TOKEN" "{\"title\":\"$2\",\"content\":\"body\"}"
}
start_event_now() {
  api_ok PATCH "/api/admin/events/$1" "$ADMIN_TOKEN" "{\"start_time\":\"$(iso_time -2)\",\"end_time\":\"$(iso_time 1800)\"}" >/dev/null
}

# ────────────────────────────────────────────────────────────────────────────
# Jeopardy Competition / Individual
# ────────────────────────────────────────────────────────────────────────────
log "mode 2/3: Jeopardy Competition / Individual"
I_START="$(iso_time 600)"; I_END="$(iso_time 3600)"
IEV="$(api_ok POST /api/admin/events "$ADMIN_TOKEN" "$(make_event_json individual "Jeopardy Individual $SUFFIX" "$I_START" "$I_END")")"
I_ID="$(json_get data.id <<<"$IEV")"
mount_challenge "$I_ID"
api_ok PATCH "/api/admin/events/$I_ID/challenges" "$ADMIN_TOKEN" "{\"challenge_id\":\"$CHALLENGE_ID\",\"points\":120}" >/dev/null
api_ok POST "/api/admin/events/$I_ID/challenges/hidden" "$ADMIN_TOKEN" "{\"challenge_id\":\"$CHALLENGE_ID\"}" >/dev/null
api_expect_fail GET "/api/events/$I_ID/challenges" "$TOK_B" || fail "player listed competition challenges before event start"
ANN="$(create_announcement "$I_ID" ind-ann)"
ANN_ID="$(json_get data.id <<<"$ANN")"
api_ok PATCH "/api/admin/events/$I_ID/announcements/$ANN_ID" "$ADMIN_TOKEN" '{"title":"ind-ann-updated","content":"updated"}' >/dev/null
api_ok GET "/api/admin/events/$I_ID/announcements/$ANN_ID" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/announcements" "$ADMIN_TOKEN" >/dev/null
api_ok POST "/api/events/$I_ID/join" "$TOK_B" >/dev/null
api_ok DELETE "/api/events/$I_ID/leave" "$TOK_B" >/dev/null
api_ok POST "/api/events/$I_ID/join" "$TOK_B" >/dev/null
B_ID="$(sql "SELECT id FROM users WHERE nickname='b'")"
api_ok GET "/api/admin/events/$I_ID/users" "$ADMIN_TOKEN" >/dev/null
api_ok POST "/api/admin/events/$I_ID/users/$B_ID/banned" "$ADMIN_TOKEN" >/dev/null
api_expect_fail POST /api/instances/launch "$TOK_B" "{\"event_id\":\"$I_ID\",\"challenge_id\":\"$CHALLENGE_ID\"}" || fail "banned individual user launched instance"
api_ok POST "/api/admin/events/$I_ID/users/$B_ID/unbanned" "$ADMIN_TOKEN" >/dev/null
start_event_now "$I_ID"
api_expect_fail DELETE "/api/events/$I_ID/leave" "$TOK_B" || fail "individual competitor left after start"
api_expect_fail POST "/api/events/$I_ID/join" "$TOK_C" || fail "late individual join accepted"
api_ok GET /api/events "$TOK_B" | grep -q "$I_ID" || fail "event list missing individual competition"
api_ok GET "/api/events/$I_ID" "$TOK_B" >/dev/null
api_ok GET "/api/events/$I_ID/capabilities" "$TOK_B" >/dev/null
HIDDEN="$(api_ok GET "/api/events/$I_ID/challenges" "$TOK_B")"
[[ "$HIDDEN" != *"$CHALLENGE_ID"* ]] || fail "hidden event challenge visible after event start"
api_ok POST "/api/admin/events/$I_ID/challenges/open" "$ADMIN_TOKEN" "{\"challenge_id\":\"$CHALLENGE_ID\"}" >/dev/null
api_ok GET "/api/events/$I_ID/challenges" "$TOK_B" | grep -q "$CHALLENGE_ID" || fail "opened challenge not visible"
api_ok GET "/api/events/$I_ID/announcements" "$TOK_B" | grep -q 'ind-ann-updated' || fail "player announcement list failed"
I1="$(launch "$TOK_B" "$I_ID")"
I1_ID="$(json_get data.id <<<"$I1")"
I_FLAG="$(instance_flag "$I1_ID")"
assert_runtime_http "$(instance_url <<<"$I1")" "$I_FLAG"
I2="$(launch "$TOK_B" "$I_ID")"
[[ "$(json_get data.id <<<"$I2")" == "$I1_ID" ]] || fail "individual running launch is not idempotent"
api_ok GET "/api/events/$I_ID/instances" "$TOK_B" | grep -q "$I1_ID" || fail "event instance list missing individual runtime"
api_ok GET "/api/events/$I_ID/challenges/$CHALLENGE_ID/instance" "$TOK_B" | grep -q "$I1_ID" || fail "event challenge instance lookup failed"
api_expect_fail POST /api/submit/flag "$TOK_B" "{\"event_id\":\"$I_ID\",\"instance_id\":\"$I1_ID\",\"flag\":\"wrong\"}" || fail "wrong competition flag accepted"
api_ok POST /api/submit/flag "$TOK_B" "{\"event_id\":\"$I_ID\",\"instance_id\":\"$I1_ID\",\"flag\":\"$I_FLAG\"}" >/dev/null
POINTS="$(sql "SELECT points FROM event_users WHERE event_id='$I_ID' AND user_id='$B_ID'")"
python3 - "$POINTS" <<'PY' || fail "individual score expected 120, got $POINTS"
import sys
raise SystemExit(0 if abs(float(sys.argv[1])-120.0)<1e-9 else 1)
PY
api_ok GET "/api/events/$I_ID/scoreboard" "$TOK_B" >/dev/null
api_ok GET "/api/events/$I_ID/trend" "$TOK_B" >/dev/null
printf '%%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%%%EOF\n' > "$TMP/writeup.pdf"
api_multipart_ok POST /api/submit/writeup "$TOK_B" -F "writeup_pdf=@$TMP/writeup.pdf;type=application/pdf" -F "event_id=$I_ID" >/dev/null
api_ok GET "/api/events/$I_ID/own_wp" "$TOK_B" >/dev/null
api_ok GET "/api/admin/events/$I_ID/writeups" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/data" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/logs" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$I_ID/report" "$ADMIN_TOKEN" >/dev/null
api_ok DELETE "/api/admin/events/$I_ID/announcements" "$ADMIN_TOKEN" "{\"id_list\":[\"$ANN_ID\"]}" >/dev/null
pass "Individual: event/challenge/announcement CRUD, registration/leave, ban guards, lifecycle lock, capabilities, real instance, scoring, scoreboard/trend, writeup/report/data/logs"

# ────────────────────────────────────────────────────────────────────────────
# Jeopardy Competition / Team
# ────────────────────────────────────────────────────────────────────────────
log "mode 3/3: Jeopardy Competition / Team"
T_START="$(iso_time 600)"; T_END="$(iso_time 3600)"
TEV="$(api_ok POST /api/admin/events "$ADMIN_TOKEN" "$(make_event_json team "Jeopardy Team $SUFFIX" "$T_START" "$T_END")")"
T_ID="$(json_get data.id <<<"$TEV")"
mount_challenge "$T_ID"
TEAM_A="$(api_ok POST "/api/events/$T_ID/team" "$TOK_A" '{"name":"Alpha"}')"
TEAM_A_ID="$(json_get data.id <<<"$TEAM_A")"
TEAM_C="$(api_ok POST "/api/events/$T_ID/team" "$TOK_C" '{"name":"Charlie"}')"
TEAM_C_ID="$(json_get data.id <<<"$TEAM_C")"
api_ok POST "/api/events/$T_ID/team/$TEAM_A_ID/join" "$TOK_B" >/dev/null
api_ok POST "/api/events/$T_ID/team/$TEAM_C_ID/join" "$TOK_D" >/dev/null
api_ok POST "/api/events/$T_ID/team/$TEAM_A_ID/leave" "$TOK_B" >/dev/null
api_ok POST "/api/events/$T_ID/team/$TEAM_A_ID/join" "$TOK_B" >/dev/null
api_expect_fail POST "/api/events/$T_ID/team/$TEAM_C_ID/join" "$TOK_B" || fail "user joined a second team"
api_ok GET "/api/admin/events/$T_ID/teams" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$T_ID/teams/$TEAM_A_ID" "$ADMIN_TOKEN" >/dev/null
api_ok POST "/api/admin/events/$T_ID/teams/$TEAM_A_ID/banned" "$ADMIN_TOKEN" >/dev/null
api_expect_fail POST /api/instances/launch "$TOK_A" "{\"event_id\":\"$T_ID\",\"challenge_id\":\"$CHALLENGE_ID\"}" || fail "banned team launched instance"
api_ok POST "/api/admin/events/$T_ID/teams/$TEAM_A_ID/unbanned" "$ADMIN_TOKEN" >/dev/null
start_event_now "$T_ID"
api_expect_fail POST "/api/events/$T_ID/team/$TEAM_A_ID/leave" "$TOK_B" || fail "team member left after start"
api_expect_fail DELETE "/api/events/$T_ID/team/$TEAM_A_ID" "$TOK_A" || fail "captain quit after start"
api_expect_fail POST "/api/events/$T_ID/team/$TEAM_A_ID/join" "$TOK_D" || fail "team roster changed after start"
T1="$(launch "$TOK_A" "$T_ID")"
T1_ID="$(json_get data.id <<<"$T1")"
api_ok GET "/api/events/$T_ID/challenges/$CHALLENGE_ID/instance" "$TOK_B" | grep -q "$T1_ID" || fail "teammate cannot see shared instance"
api_ok GET "/api/instances/$T1_ID" "$TOK_B" | grep -q "$T1_ID" || fail "generic instance endpoint rejected teammate shared-instance read"
T_REUSE="$(launch "$TOK_B" "$T_ID")"
[[ "$(json_get data.id <<<"$T_REUSE")" == "$T1_ID" ]] || fail "teammate did not reuse shared instance"
api_expect_fail DELETE "/api/instances/$T1_ID" "$TOK_C" || fail "other team destroyed Alpha shared instance"
[[ "$(sql "SELECT runtime_state FROM event_instances WHERE id='$T1_ID'")" == "running" ]] || fail "unauthorized destroy changed shared runtime"
api_ok DELETE "/api/instances/$T1_ID" "$TOK_B" >/dev/null
T2="$(launch "$TOK_A" "$T_ID")"
T2_ID="$(json_get data.id <<<"$T2")"
[[ "$T2_ID" != "$T1_ID" ]] || fail "shared instance did not relaunch after teammate destroy"
T2_FLAG="$(instance_flag "$T2_ID")"
api_ok POST /api/submit/flag "$TOK_B" "{\"event_id\":\"$T_ID\",\"instance_id\":\"$T2_ID\",\"flag\":\"$T2_FLAG\"}" >/dev/null
TEAM_POINTS="$(sql "SELECT points FROM event_teams WHERE id='$TEAM_A_ID'")"
python3 - "$TEAM_POINTS" <<'PY' || fail "team score expected 100, got $TEAM_POINTS"
import sys
raise SystemExit(0 if abs(float(sys.argv[1])-100.0)<1e-9 else 1)
PY
api_expect_fail POST /api/submit/flag "$TOK_A" "{\"event_id\":\"$T_ID\",\"flag\":\"$T2_FLAG\"}" || fail "duplicate team flag accepted"
api_ok GET "/api/events/$T_ID/scoreboard" "$TOK_A" >/dev/null
api_ok GET "/api/events/$T_ID/trend" "$TOK_A" >/dev/null
api_multipart_expect_fail POST /api/submit/writeup "$TOK_B" -F "writeup_pdf=@$TMP/writeup.pdf;type=application/pdf" -F "event_id=$T_ID" -F "team_id=$TEAM_C_ID" || fail "spoofed team writeup accepted"
api_multipart_ok POST /api/submit/writeup "$TOK_B" -F "writeup_pdf=@$TMP/writeup.pdf;type=application/pdf" -F "event_id=$T_ID" -F "team_id=$TEAM_A_ID" >/dev/null
api_ok GET "/api/events/$T_ID/own_wp" "$TOK_A" >/dev/null
api_ok GET "/api/admin/events/$T_ID/writeups" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$T_ID/data" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$T_ID/logs" "$ADMIN_TOKEN" >/dev/null
api_ok GET "/api/admin/events/$T_ID/report" "$ADMIN_TOKEN" >/dev/null
pass "Team: create/join/leave/rejoin, duplicate-team guard, admin view/ban, roster lock, shared instance teammate view/reuse/destroy/solve, scoring, writeup anti-IDOR/report"

# Admin event delete endpoint on a disposable event.
D_START="$(iso_time 600)"; D_END="$(iso_time 1200)"
DEV="$(api_ok POST /api/admin/events "$ADMIN_TOKEN" "$(make_event_json individual "Jeopardy Delete $SUFFIX" "$D_START" "$D_END")")"
D_ID="$(json_get data.id <<<"$DEV")"
api_ok DELETE /api/admin/events "$ADMIN_TOKEN" "{\"id_list\":[\"$D_ID\"]}" >/dev/null
[[ "$(sql "SELECT count(*) FROM events WHERE id='$D_ID'")" == "0" ]] || fail "admin event delete failed"
pass "event delete endpoint"

[[ "$(docker ps -a --format '{{.Names}}' | grep -Ec '^(JP|JS|JT)-' || true)" == "0" ]] || fail "Jeopardy containers leaked"
[[ "$(sql "SELECT count(*) FROM event_instances WHERE runtime_state='running'")" == "0" ]] || fail "running instance rows leaked"

printf '\n=== JEOPARDY REAL HTTP E2E SUMMARY ===\n'
printf 'modes: practice/individual, competition/individual, competition/team\n'
printf 'challenge package: imported + built through HTTP API\n'
printf 'runtime: real Docker via helper; real challenge HTTP verified\n'
printf 'RESULT: PASS\n'
