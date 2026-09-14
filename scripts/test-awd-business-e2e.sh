#!/usr/bin/env bash
set -Eeuo pipefail

# FloatCTF AWD full business acceptance E2E.
# Explicitly run on a prepared Linux AWD development host. It uses an isolated
# PostgreSQL DB, Redis container and API port, but the real system helper,
# Docker Engine, WireGuard and nftables.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() { printf '[awd-e2e] %s\n' "$*" >&2; }
pass() { printf '[awd-e2e] PASS: %s\n' "$*" >&2; }
fail() { printf '[awd-e2e] FAIL: %s\n' "$*" >&2; return 1; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing command: $1" >&2; exit 2; }; }
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
    if key.isdigit(): v=v[int(key)]
    else: v=v.get(key) if isinstance(v,dict) else None
    if v is None: break
if v is None: print("")
elif isinstance(v,bool): print("true" if v else "false")
else: print(v)' "$path"
}

json_team_field() {
    local team_id=$1 field=$2
    python3 -c 'import json,sys
obj=json.load(sys.stdin); team=sys.argv[1]; field=sys.argv[2]
for row in obj.get("data") or []:
    if str(row.get("team_id")) == team:
        v=row.get(field)
        print("" if v is None else v)
        raise SystemExit(0)
raise SystemExit(1)' "$team_id" "$field"
}

json_string_obj() {
    python3 -c 'import json,sys; a=sys.argv[1:]; print(json.dumps(dict(zip(a[0::2],a[1::2])), separators=(",",":")))' "$@"
}


helper_remove_wireguard() {
    local interface=$1
    python3 - "$interface" <<'PYHELPER'
import json, socket, sys
iface=sys.argv[1]
s=socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(5)
s.connect('/run/floatctf/helper-control.sock')
s.sendall((json.dumps({'op':'remove_wireguard','interface':iface},separators=(',',':'))+'\n').encode())
s.shutdown(socket.SHUT_WR)
line=s.makefile('r').readline()
if not line:
    raise SystemExit('empty helper response')
resp=json.loads(line)
if not resp.get('ok'):
    raise SystemExit(resp.get('error','helper remove_wireguard failed'))
PYHELPER
}


helper_nft_table_is_absent() {
    local table=$1
    python3 - "$table" <<'PYHELPER'
import json, socket, sys
table=sys.argv[1]
s=socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(5)
s.connect('/run/floatctf/helper-control.sock')
s.sendall((json.dumps({'op':'list_nft_table','table':table},separators=(',',':'))+'\n').encode())
s.shutdown(socket.SHUT_WR)
line=s.makefile('r').readline()
if not line:
    raise SystemExit('empty helper response')
resp=json.loads(line)
if not resp.get('ok'):
    raise SystemExit(resp.get('error','helper list_nft_table failed'))
data=resp.get('data')
if isinstance(data, str) and not data.strip():
    raise SystemExit(0)
raise SystemExit(1)
PYHELPER
}

SUFFIX="$(date +%s)-$$"
DB_NAME="floatctf_awd_business_e2e_${SUFFIX//-/_}"
REDIS_NAME="floatctf-awd-business-e2e-redis-$SUFFIX"
TMP="$(mktemp -d "/tmp/floatctf-awd-business-e2e-${SUFFIX}.XXXXXX")"
CONFIG="$TMP/e2e.toml"
API_LOG="$TMP/api.log"
RESULT_LOG="$TMP/result.log"
WORK_DIR="$TMP/work"
mkdir -p "$WORK_DIR"
exec > >(tee -a "$RESULT_LOG") 2> >(tee -a "$RESULT_LOG" >&2)

API_PID="" API_PORT="" REDIS_PORT="" EVENT_ID="" NETWORK_NAME="" WG_INTERFACE=""
PRE_PRACTICE_JUDGE="$(docker ps -aq --filter name='^/fctf-awdp-practice-judge$' | head -n1)"

cleanup() {
    local rc=$?
    set +e
    log "cleanup begin (rc=$rc)"
    if [[ -n "$EVENT_ID" ]]; then
        mapfile -t ids < <(docker ps -aq --filter "label=awd.event_id=$EVENT_ID" 2>/dev/null)
        if ((${#ids[@]})); then docker rm -f "${ids[@]}" >/dev/null 2>&1 || true; fi
    fi
    [[ -n "$NETWORK_NAME" ]] && docker network rm "$NETWORK_NAME" >/dev/null 2>&1 || true
    if [[ -n "$WG_INTERFACE" ]] && ip link show "$WG_INTERFACE" >/dev/null 2>&1; then
        helper_remove_wireguard "$WG_INTERFACE" >/dev/null 2>&1 || true
    fi
    if [[ -n "$API_PID" ]] && kill -0 "$API_PID" 2>/dev/null; then
        kill "$API_PID" 2>/dev/null || true
        for _ in $(seq 1 50); do kill -0 "$API_PID" 2>/dev/null || break; sleep 0.1; done
        kill -9 "$API_PID" 2>/dev/null || true
    fi
    # The isolated API also runs the global AWDP practice-environment ensure
    # startup task. Restore the pre-test absence of that shared worker.
    if [[ -z "$PRE_PRACTICE_JUDGE" ]]; then
        docker rm -f fctf-awdp-practice-judge >/dev/null 2>&1 || true
    fi
    docker rm -f "$REDIS_NAME" >/dev/null 2>&1 || true
    PGPASSWORD=postgres dropdb -h 127.0.0.1 -U postgres --if-exists --force "$DB_NAME" >/dev/null 2>&1 || true
    log "artifacts kept at: $TMP"
    (( rc == 0 )) && pass "isolated DB/Redis/API and event runtime cleanup completed"
    exit "$rc"
}
trap cleanup EXIT INT TERM

api_raw() {
    local method=$1 path=$2 token=${3:-} body=${4:-}
    local out status
    out=$(mktemp "$TMP/http.XXXXXX")
    local args=(-sS --max-time 60 -X "$method" "http://127.0.0.1:${API_PORT}${path}" -H 'Accept: application/json')
    [[ -n "$token" ]] && args+=(-H "Authorization: Bearer $token")
    [[ -n "$body" ]] && args+=(-H 'Content-Type: application/json' --data "$body")
    status=$(curl "${args[@]}" -o "$out" -w '%{http_code}') || { cat "$out" >&2 || true; rm -f "$out"; return 90; }
    printf '%s\n' "$status"
    cat "$out"
    rm -f "$out"
}

api_ok() {
    local raw status payload
    raw="$(api_raw "$@")"; status="${raw%%$'\n'*}"; payload="${raw#*$'\n'}"
    if [[ ! "$status" =~ ^2 ]] || ! json_code_ok <<<"$payload"; then
        printf 'API failed: %s %s -> HTTP %s\n%s\n' "$1" "$2" "$status" "$payload" >&2
        return 1
    fi
    printf '%s' "$payload"
}

api_expect_fail() {
    local raw status payload
    raw="$(api_raw "$@")"; status="${raw%%$'\n'*}"; payload="${raw#*$'\n'}"
    if [[ "$status" =~ ^2 ]] && json_code_ok <<<"$payload"; then
        printf 'expected failure but succeeded: %s %s\n%s\n' "$1" "$2" "$payload" >&2
        return 1
    fi
}

sql() {
    PGPASSWORD=postgres psql -X -q -A -t -v ON_ERROR_STOP=1 -h 127.0.0.1 -U postgres -d "$DB_NAME" -c "$1"
}

wait_sql() {
    local query=$1 expected=$2 timeout=${3:-30} value=""
    local deadline=$((SECONDS + timeout))
    while (( SECONDS < deadline )); do
        value="$(sql "$query" 2>/dev/null | tail -n1 || true)"
        [[ "$value" == "$expected" ]] && return 0
        sleep 0.25
    done
    printf 'wait_sql timeout: expected=%q got=%q query=%s\n' "$expected" "$value" "$query" >&2
    return 1
}

wait_player_round() {
    local token=$1 round=$2 timeout=${3:-30} payload=""
    local deadline=$((SECONDS + timeout))
    while (( SECONDS < deadline )); do
        payload="$(api_ok GET "/api/events/$EVENT_ID/awd/status" "$token" 2>/dev/null || true)"
        if [[ -n "$payload" ]] \
          && [[ "$(json_get data.status <<<"$payload")" == "running" ]] \
          && [[ "$(json_get data.phase <<<"$payload")" == "attack" ]] \
          && [[ "$(json_get data.current_round <<<"$payload")" == "$round" ]]; then return 0; fi
        sleep 0.25
    done
    printf 'round %s did not become active; last=%s\n' "$round" "$payload" >&2
    return 1
}

score_of() {
    local token=$1 team_id=$2
    api_ok GET "/api/events/$EVENT_ID/awd/scores" "$token" | json_team_field "$team_id" total_score
}

log "preflight host readiness"
[[ "$(systemctl is-active floatctf-helper.service 2>/dev/null || true)" == "active" ]] || fail "floatctf-helper.service is not active"
[[ -S /run/floatctf/helper-control.sock ]] || fail "missing helper-control.sock"
[[ -S /run/floatctf/helper-docker.sock ]] || fail "missing helper-docker.sock"
for image in floatctf/awd-flagserver:latest floatctf/awd-judgeserver:latest; do
    docker image inspect "$image" >/dev/null 2>&1 || fail "missing required image $image"
done
if docker ps -aq --filter label=awd.event_id | grep -q .; then fail "existing AWD-labelled containers found"; fi
if docker network ls --format '{{.Name}}' | grep -q '^fctf-awd-'; then fail "existing AWD event network found"; fi
if ip -o link show 2>/dev/null | awk -F': ' '{print $2}' | grep -q '^fawg_'; then fail "existing AWD WireGuard interface found"; fi
current_awd_count="$(PGPASSWORD=postgres psql -X -q -A -t -h 127.0.0.1 -U postgres -d floatctf_db -c "SELECT count(*) FROM awd_events WHERE status::text NOT IN ('finished','archived')" 2>/dev/null || echo '?')"
[[ "$current_awd_count" == "0" ]] || fail "development DB has active AWD rows ($current_awd_count)"
pass "host has no competing AWD runtime"

log "build latest API"
if [[ "${FLOATCTF_E2E_SKIP_API_BUILD:-0}" != "1" ]]; then
  cargo build -p floatctf >/dev/null
fi

log "ensure reusable GameBox fixture image"
if ! docker image inspect floatctf/awd-business-gamebox:e2e >/dev/null 2>&1; then
    FIXTURE_CTX="$TMP/gamebox-image"; mkdir -p "$FIXTURE_CTX"
    cat > "$FIXTURE_CTX/Dockerfile" <<'DOCKERFILE'
FROM alpine:3.20
LABEL io.floatctf.managed="true"
CMD ["sleep", "infinity"]
DOCKERFILE
    docker build --network=none -t floatctf/awd-business-gamebox:e2e "$FIXTURE_CTX" >/dev/null
fi
GAMEBOX_IMAGE_ID="$(docker image inspect floatctf/awd-business-gamebox:e2e --format '{{.Id}}')"
docker run --rm --entrypoint sh floatctf/awd-business-gamebox:e2e -c 'command -v wget >/dev/null' || fail "fixture image needs wget"
pass "GameBox fixture ready"

log "create isolated PostgreSQL + Redis"
PGPASSWORD=postgres createdb -h 127.0.0.1 -U postgres "$DB_NAME"
docker run -d --name "$REDIS_NAME" -p 127.0.0.1::6379 redis:7-alpine >/dev/null
REDIS_PORT="$(docker port "$REDIS_NAME" 6379/tcp | sed -E 's/.*:([0-9]+)$/\1/' | head -n1)"
[[ "$REDIS_PORT" =~ ^[0-9]+$ ]] || fail "failed to discover Redis port"
for _ in $(seq 1 50); do
    docker exec "$REDIS_NAME" redis-cli ping 2>/dev/null | grep -q PONG && break
    sleep 0.1
done
docker exec "$REDIS_NAME" redis-cli ping | grep -q PONG || fail "isolated Redis did not become ready"

for p in $(seq 19090 19140); do
    if ! ss -ltnH | awk '{print $4}' | grep -Eq "(^|:)${p}$"; then API_PORT=$p; break; fi
done
[[ -n "$API_PORT" ]] || fail "no free E2E API port"

cp apps/api/config/development.toml "$CONFIG"
python3 - "$CONFIG" "$DB_NAME" "$REDIS_PORT" "$API_PORT" "$WORK_DIR" "$SUFFIX" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1]); db,rport,aport,work,suffix=sys.argv[2:]
s=p.read_text()
s=s.replace('main_url = "http://localhost:9090"', f'main_url = "http://127.0.0.1:{aport}"')
s=s.replace('listen_port = 9090', f'listen_port = {aport}', 1)
s=s.replace('work_dir = "../../app"', f'work_dir = "{work}"')
s=s.replace('url = "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db"', f'url = "postgres://postgres:postgres@127.0.0.1:5432/{db}"')
s=s.replace('platform_internal_url = "http://127.0.0.1:9090"', f'platform_internal_url = "http://127.0.0.1:{aport}"', 1)
s=s.replace('url = "redis://127.0.0.1:6379/"', f'url = "redis://127.0.0.1:{rport}/"')
s=s.replace('channel = "floatctf:realtime"', f'channel = "floatctf:realtime:e2e:{suffix}"')
p.write_text(s)
PY

log "apply migrations to isolated DB"
FLOATCTF_CONFIG="$CONFIG" apps/api/src/sql/migrate.sh apply >/dev/null
MIGRATION_COUNT="$(sql 'SELECT count(*) FROM schema_migrations')"
[[ "$MIGRATION_COUNT" -ge 44 ]] || fail "expected >=44 migrations, got $MIGRATION_COUNT"

# Only the reusable library fixture is seeded. Event/users/teams/runtime/scores all use real APIs.
GAMEBOX_ID="$(cat /proc/sys/kernel/random/uuid)"
PGPASSWORD=postgres psql -X -q -v ON_ERROR_STOP=1 -h 127.0.0.1 -U postgres -d "$DB_NAME" \
    -v gb="$GAMEBOX_ID" -v img="$GAMEBOX_IMAGE_ID" <<'SQL'
INSERT INTO gameboxes (
    id,name,safe_name,category,description,hidden,version,package_digest,image_ref,image_id,username,
    recommended_cpu_millis,recommended_memory_bytes,recommended_pids_limit,healthchecks_json,
    judge_script_name,judge_script_content,judge_args_json,judge_timeout_secs,judge_retry_interval_secs,build_status
) VALUES (
    :'gb'::uuid,'AWD Business E2E','awd-business-e2e','web','isolated acceptance fixture',false,
    '1.0.0','e2e-package','floatctf/awd-business-gamebox:e2e',:'img','root',
    250,134217728,64,'[]'::jsonb,
    'judge.sh',E'#!/bin/sh\ncase "$TARGET_IP" in\n  *.*.1.2) exit 1 ;;\n  *) exit 0 ;;\nesac\n',NULL,3,1,'ready'
);
SQL

log "start isolated latest API on 0.0.0.0:$API_PORT"
(cd apps/api && exec env FLOATCTF_CONFIG="$CONFIG" ../../target/debug/floatctf) >"$API_LOG" 2>&1 &
API_PID=$!
for _ in $(seq 1 160); do
    if ! kill -0 "$API_PID" 2>/dev/null; then tail -n 120 "$API_LOG" >&2; fail "isolated API exited during bootstrap"; fi
    code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 1 "http://127.0.0.1:$API_PORT/api/users/me" 2>/dev/null || true)"
    [[ "$code" == "401" ]] && break
    sleep 0.1
done
[[ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 2 "http://127.0.0.1:$API_PORT/api/users/me" 2>/dev/null || true)" == "401" ]] || { tail -n 160 "$API_LOG" >&2; fail "isolated API not ready"; }
pass "isolated API bootstrapped through DB + Redis + helper Docker + RustFS"

log "admin login"
ADMIN_PASSWORD="$(sed -n 's/^- 密码：`\(.*\)`/\1/p' docs/agents/TESTING.md | head -n1)"
[[ -n "$ADMIN_PASSWORD" ]] || fail "cannot read local test admin credential from TESTING.md"
ADMIN_BODY="$(json_string_obj username sysadmin password "$ADMIN_PASSWORD")"
ADMIN_JSON="$(api_ok POST /api/admin/session '' "$ADMIN_BODY")"
ADMIN_TOKEN="$(json_get data <<<"$ADMIN_JSON")"
unset ADMIN_PASSWORD ADMIN_BODY
[[ -n "$ADMIN_TOKEN" && "$ADMIN_TOKEN" != null ]] || fail "admin login returned no token"
pass "admin authenticated"

START_TIME="$(python3 - <<'PY'
from datetime import datetime, timezone, timedelta
print((datetime.now(timezone.utc)+timedelta(minutes=10)).isoformat(timespec='seconds'))
PY
)"
END_TIME="$(python3 - "$START_TIME" <<'PY'
from datetime import datetime, timedelta
import sys
print((datetime.fromisoformat(sys.argv[1])+timedelta(seconds=60)).isoformat(timespec='seconds'))
PY
)"

log "create generic AWD team event via admin HTTP"
EVENT_JSON="$(python3 - "$START_TIME" "$END_TIME" "$SUFFIX" <<'PYJSON'
import json,sys
start,end,suffix=sys.argv[1:]
print(json.dumps({"family":"awd","participant_mode":"team","title":f"AWD Business E2E {suffix}","description":"full business lifecycle acceptance","hidden":False,"allow_join":True,"rules":"E2E rules","flag_prefix":"flag","start_time":start,"end_time":end},separators=(",",":")))
PYJSON
)"
CREATE_EVENT="$(api_ok POST /api/admin/events "$ADMIN_TOKEN" "$EVENT_JSON")"
EVENT_ID="$(json_get data.id <<<"$CREATE_EVENT")"
[[ "$EVENT_ID" =~ ^[0-9a-f-]{36}$ ]] || fail "invalid event id"
pass "event created: $EVENT_ID"

log "configure 2 rounds x 30 seconds"
AWD_CONFIG="$(python3 - "$EVENT_ID" <<'PYJSON'
import json,sys
print(json.dumps({"event_id":sys.argv[1],"round_count":2,"round_duration_secs":30,"initial_score":1000,"free_reset_count":1,"extra_reset_penalty":50,"judge_max_concurrency":4,"judge_default_timeout_secs":3,"judge_retry_interval_secs":1,"judge_grace_period_secs":2,"archive_retention_hours":1},separators=(",",":")))
PYJSON
)"
api_ok POST /api/admin/events/awd "$ADMIN_TOKEN" "$AWD_CONFIG" >/dev/null
STATUS="$(api_ok GET "/api/admin/events/$EVENT_ID/awd" "$ADMIN_TOKEN")"
[[ "$(json_get data.status <<<"$STATUS")" == "configuring" ]] || fail "AWD did not enter configuring"
pass "AWD configured"

log "register + login four real users"
USER_PASS="E2e-${SUFFIX}-Aa1!"
declare -A TOKENS
for role in redcap redmember bluecap bluemember; do
    username="awd_${role}_${SUFFIX//-/_}"
    body="$(json_string_obj username "$username" nickname "$role" password "$USER_PASS" email "$username@example.invalid")"
    api_ok POST /api/users '' "$body" >/dev/null
    login_body="$(json_string_obj username "$username" password "$USER_PASS")"
    login="$(api_ok POST /api/users/session '' "$login_body")"
    TOKENS[$role]="$(json_get data <<<"$login")"
    [[ -n "${TOKENS[$role]}" && "${TOKENS[$role]}" != null ]] || fail "login failed for $role"
done
unset USER_PASS
pass "4 users registered and authenticated"

api_expect_fail POST "/api/events/$EVENT_ID/join" "${TOKENS[redcap]}" '' || fail "team AWD accepted generic /join"
[[ "$(sql "SELECT count(*) FROM event_users WHERE event_id='$EVENT_ID'")" == "0" ]] || fail "rejected generic team join left event_users residue"
pass "team AWD rejects generic solo /join without persistence"

log "captains create teams; members join through public team APIs"
RED_TEAM_JSON="$(api_ok POST "/api/events/$EVENT_ID/team" "${TOKENS[redcap]}" '{"name":"Red Team"}')"
RED_TEAM_ID="$(json_get data.id <<<"$RED_TEAM_JSON")"
sleep 0.2
BLUE_TEAM_JSON="$(api_ok POST "/api/events/$EVENT_ID/team" "${TOKENS[bluecap]}" '{"name":"Blue Team"}')"
BLUE_TEAM_ID="$(json_get data.id <<<"$BLUE_TEAM_JSON")"
api_ok POST "/api/events/$EVENT_ID/team/$RED_TEAM_ID/join" "${TOKENS[redmember]}" >/dev/null
api_ok POST "/api/events/$EVENT_ID/team/$BLUE_TEAM_ID/join" "${TOKENS[bluemember]}" >/dev/null
[[ "$(sql "SELECT count(*) FROM event_users WHERE event_id='$EVENT_ID'")" == "4" ]] || fail "expected 4 enrolled users"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID'")" == "4" ]] || fail "expected 4 team memberships"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND role::text='captain'")" == "2" ]] || fail "expected 2 captains"
pass "enrollment: 2 teams / 4 users / 2 captains"

log "attach ready GameBox fixture via admin HTTP"
ATTACH="$(python3 - "$GAMEBOX_ID" <<'PYJSON'
import json,sys
print(json.dumps({"gamebox_id":sys.argv[1],"host_offset":2,"hidden":False,"attack_score":100,"judge_down_penalty":40,"first_bonus":20},separators=(",",":")))
PYJSON
)"
api_ok POST "/api/admin/events/$EVENT_ID/awd/gameboxes" "$ADMIN_TOKEN" "$ATTACH" >/dev/null
pass "GameBox attached: attack=100 judge_down=40 first_bonus=20"

log "allocate real event network through helper"
api_ok PUT "/api/admin/events/$EVENT_ID/awd/network" "$ADMIN_TOKEN" '{}' >/dev/null
NET_JSON="$(api_ok GET "/api/admin/events/$EVENT_ID/awd/network" "$ADMIN_TOKEN")"
NETWORK_NAME="$(json_get data.docker_network_name <<<"$NET_JSON")"
WG_INTERFACE="$(json_get data.wireguard_interface_name <<<"$NET_JSON")"
FLAGSERVER_IP="$(json_get data.flagserver_ip <<<"$NET_JSON")"
[[ -n "$NETWORK_NAME" && "$NETWORK_NAME" != null ]] || fail "network allocation returned no Docker network"
pass "network allocated: $NETWORK_NAME / $WG_INTERFACE"

log "deploy real FlagServer/JudgeServer/GameBoxes"
api_ok POST "/api/admin/events/$EVENT_ID/awd/deploy" "$ADMIN_TOKEN" >/dev/null
[[ "$(sql "SELECT status::text FROM awd_events WHERE event_id='$EVENT_ID'")" == "deployed" ]] || fail "deploy did not reach deployed"
EVENT_CONTAINER_COUNT="$(docker ps -q --filter "label=awd.event_id=$EVENT_ID" | wc -l | tr -d ' ')"
[[ "$EVENT_CONTAINER_COUNT" == "4" ]] || { docker ps -a --filter "label=awd.event_id=$EVENT_ID"; fail "expected 4 event containers, got $EVENT_CONTAINER_COUNT"; }
docker network inspect "$NETWORK_NAME" >/dev/null
ip link show "$WG_INTERFACE" >/dev/null 2>&1 || fail "WireGuard interface missing after deploy"
pass "deploy created FlagServer + JudgeServer + 2 GameBoxes + Docker/WG runtime"

RED_SLOT="$(sql "SELECT subnet_index FROM awd_team_networks WHERE event_id='$EVENT_ID' AND team_id='$RED_TEAM_ID'")"
BLUE_SLOT="$(sql "SELECT subnet_index FROM awd_team_networks WHERE event_id='$EVENT_ID' AND team_id='$BLUE_TEAM_ID'")"
[[ "$RED_SLOT" == "1" && "$BLUE_SLOT" == "2" ]] || fail "expected Red slot1/Blue slot2, got $RED_SLOT/$BLUE_SLOT"
pass "stable team subnet allocation: Red=slot1 Blue=slot2"

RED_BOXES="$(api_ok GET "/api/events/$EVENT_ID/awd/gameboxes" "${TOKENS[redcap]}")"
BLUE_BOXES="$(api_ok GET "/api/events/$EVENT_ID/awd/gameboxes" "${TOKENS[bluecap]}")"
RED_CONTAINER="$(json_get data.0.container_name <<<"$RED_BOXES")"
BLUE_CONTAINER="$(json_get data.0.container_name <<<"$BLUE_BOXES")"
[[ -n "$RED_CONTAINER" && -n "$BLUE_CONTAINER" && "$RED_CONTAINER" != "$BLUE_CONTAINER" ]] || fail "player GameBox lookup failed"
pass "players resolve isolated per-team GameBoxes"

log "run real precheck"
api_ok POST "/api/admin/events/$EVENT_ID/awd/precheck" "$ADMIN_TOKEN" >/dev/null
wait_sql "SELECT status::text FROM awd_events WHERE event_id='$EVENT_ID'" "verified" 30
PRECHECK="$(sql "SELECT status::text FROM awd_precheck_runs WHERE event_id='$EVENT_ID' ORDER BY started_at DESC LIMIT 1")"
[[ "$PRECHECK" == "passed" ]] || fail "precheck row not passed: $PRECHECK"
pass "precheck passed; event verified"

log "manual start; 60s event duration minus 2x30s rounds means zero hardening"
api_ok POST "/api/admin/events/$EVENT_ID/awd/start" "$ADMIN_TOKEN" >/dev/null
wait_player_round "${TOKENS[redcap]}" 1 20
[[ "$(score_of "${TOKENS[redcap]}" "$RED_TEAM_ID")" == "1000" ]] || fail "Red initial score != 1000"
[[ "$(score_of "${TOKENS[bluecap]}" "$BLUE_TEAM_ID")" == "1000" ]] || fail "Blue initial score != 1000"
pass "Round 1 active; initial scores seeded exactly once"

get_flag_from_box() {
    local container=$1 flag
    flag="$(docker exec "$container" wget -qO- --timeout=5 "http://${FLAGSERVER_IP}:8080/flag")"
    [[ "$flag" == flag\{*\} ]] || { printf 'unexpected flag response from %s: %q\n' "$container" "$flag" >&2; return 1; }
    printf '%s' "$flag"
}

submit_flag() {
    local token=$1 flag=$2
    api_ok POST "/api/events/$EVENT_ID/awd/submissions" "$token" "$(json_string_obj flag "$flag")"
}

log "Round 1: obtain victim flags from real victim containers through FlagServer"
R1_RED_FLAG="$(get_flag_from_box "$RED_CONTAINER")"
R1_BLUE_FLAG="$(get_flag_from_box "$BLUE_CONTAINER")"
[[ "$R1_RED_FLAG" != "$R1_BLUE_FLAG" ]] || fail "different GameBoxes returned identical flag"

R1_RED_ATTACK="$(submit_flag "${TOKENS[redcap]}" "$R1_BLUE_FLAG")"
[[ "$(json_get data.attack_score <<<"$R1_RED_ATTACK")" == "100" ]] || fail "Round1 Red attack score != 100"
[[ "$(json_get data.first_bonus <<<"$R1_RED_ATTACK")" == "20" ]] || fail "Round1 first blood != 20"
[[ "$(json_get data.was_first_blood <<<"$R1_RED_ATTACK")" == "true" ]] || fail "Round1 Red not marked first blood"
api_expect_fail POST "/api/events/$EVENT_ID/awd/submissions" "${TOKENS[redcap]}" "$(json_string_obj flag "$R1_BLUE_FLAG")" || fail "duplicate attack unexpectedly accepted"
api_expect_fail POST "/api/events/$EVENT_ID/awd/submissions" "${TOKENS[redcap]}" "$(json_string_obj flag "$R1_RED_FLAG")" || fail "self attack unexpectedly accepted"
R1_BLUE_ATTACK="$(submit_flag "${TOKENS[bluecap]}" "$R1_RED_FLAG")"
[[ "$(json_get data.attack_score <<<"$R1_BLUE_ATTACK")" == "100" ]] || fail "Round1 Blue attack score != 100"
[[ "$(json_get data.first_bonus <<<"$R1_BLUE_ATTACK")" == "0" ]] || fail "Round1 second attack got first bonus"
pass "Round1 FlagServer attacks + first blood + duplicate/self guards passed"

log "wait Round 1 end, JudgeServer result, and Round 2 start"
wait_player_round "${TOKENS[redcap]}" 2 45
R1_ID="$(sql "SELECT id FROM awd_rounds WHERE event_id='$EVENT_ID' AND round_number=1")"
wait_sql "SELECT count(*) FROM awd_judge_tasks WHERE event_id='$EVENT_ID' AND round_id='$R1_ID' AND status::text NOT IN ('up','down','judge_error','skipped_resetting','skipped_banned')" "0" 20
R1_RED_SCORE="$(score_of "${TOKENS[redcap]}" "$RED_TEAM_ID")"
R1_BLUE_SCORE="$(score_of "${TOKENS[bluecap]}" "$BLUE_TEAM_ID")"
[[ "$R1_RED_SCORE" == "980" ]] || fail "Round1 Red expected 980, got $R1_RED_SCORE"
[[ "$R1_BLUE_SCORE" == "1000" ]] || fail "Round1 Blue expected 1000, got $R1_BLUE_SCORE"
R1_DOWN="$(sql "SELECT count(*) FROM awd_judge_tasks WHERE round_id='$R1_ID' AND status::text='down'")"
R1_UP="$(sql "SELECT count(*) FROM awd_judge_tasks WHERE round_id='$R1_ID' AND status::text='up'")"
[[ "$R1_DOWN" == "1" && "$R1_UP" == "1" ]] || fail "Round1 judge expected 1 down/1 up, got $R1_DOWN/$R1_UP"
pass "Round1 exact settlement: Red=980 Blue=1000, Judge 1 down/1 up"

log "Round 2: flags rotate; both teams attack again"
R2_RED_FLAG="$(get_flag_from_box "$RED_CONTAINER")"
R2_BLUE_FLAG="$(get_flag_from_box "$BLUE_CONTAINER")"
[[ "$R2_RED_FLAG" != "$R1_RED_FLAG" ]] || fail "Red flag did not rotate between rounds"
[[ "$R2_BLUE_FLAG" != "$R1_BLUE_FLAG" ]] || fail "Blue flag did not rotate between rounds"
R2_BLUE_ATTACK="$(submit_flag "${TOKENS[bluecap]}" "$R2_RED_FLAG")"
R2_RED_ATTACK="$(submit_flag "${TOKENS[redcap]}" "$R2_BLUE_FLAG")"
[[ "$(json_get data.first_bonus <<<"$R2_BLUE_ATTACK")" == "0" ]] || fail "Round2 re-awarded first bonus"
[[ "$(json_get data.first_bonus <<<"$R2_RED_ATTACK")" == "0" ]] || fail "Round2 re-awarded first bonus"
pass "Round2 flag rotation + attacks passed; first blood remains event-global"

log "wait final round judge settlement"
R2_ID=""
for _ in $(seq 1 160); do
    R2_ID="$(sql "SELECT id FROM awd_rounds WHERE event_id='$EVENT_ID' AND round_number=2" 2>/dev/null || true)"
    [[ -n "$R2_ID" ]] && break
    sleep 0.25
done
[[ -n "$R2_ID" ]] || fail "Round2 row missing"
wait_sql "SELECT status::text FROM awd_rounds WHERE id='$R2_ID'" "completed" 45
wait_sql "SELECT count(*) FROM awd_judge_tasks WHERE event_id='$EVENT_ID' AND round_id='$R2_ID' AND status::text NOT IN ('up','down','judge_error','skipped_resetting','skipped_banned')" "0" 20

FINAL_STATUS="$(sql "SELECT status::text FROM awd_events WHERE event_id='$EVENT_ID'")"
if [[ "$FINAL_STATUS" != "finished" ]]; then
    api_ok POST "/api/admin/events/$EVENT_ID/awd/finish" "$ADMIN_TOKEN" >/dev/null
    wait_sql "SELECT status::text FROM awd_events WHERE event_id='$EVENT_ID'" "finished" 20
fi

FINAL_SCORES="$(api_ok GET "/api/admin/events/$EVENT_ID/awd/scores" "$ADMIN_TOKEN")"
RED_FINAL="$(json_team_field "$RED_TEAM_ID" total_score <<<"$FINAL_SCORES")"
BLUE_FINAL="$(json_team_field "$BLUE_TEAM_ID" total_score <<<"$FINAL_SCORES")"
RED_RANK="$(json_team_field "$RED_TEAM_ID" rank <<<"$FINAL_SCORES")"
BLUE_RANK="$(json_team_field "$BLUE_TEAM_ID" rank <<<"$FINAL_SCORES")"
[[ "$RED_FINAL" == "940" ]] || fail "final Red expected 940, got $RED_FINAL"
[[ "$BLUE_FINAL" == "1000" ]] || fail "final Blue expected 1000, got $BLUE_FINAL"
[[ "$BLUE_RANK" == "1" && "$RED_RANK" == "2" ]] || fail "expected Blue#1 Red#2, got Blue#$BLUE_RANK Red#$RED_RANK"

SCORE_COUNTS="$(sql "SELECT event_type::text || '=' || count(*) FROM awd_score_events WHERE event_id='$EVENT_ID' GROUP BY event_type ORDER BY event_type")"
for expected in 'attack=4' 'victim_loss=4' 'first_bonus=1' 'initial_score=2' 'judge_down=2'; do
    grep -qx "$expected" <<<"$SCORE_COUNTS" || { printf '%s\n' "$SCORE_COUNTS" >&2; fail "score ledger missing $expected"; }
done
pass "final scoreboard exact: Blue=1000 rank#1, Red=940 rank#2; ledger verified"

log "archive through admin HTTP and assert zero event runtime residue"
api_ok POST "/api/admin/events/$EVENT_ID/awd/archive" "$ADMIN_TOKEN" >/dev/null
wait_sql "SELECT status::text FROM awd_events WHERE event_id='$EVENT_ID'" "archived" 20
sleep 0.5
REMAIN_CONTAINERS="$(docker ps -aq --filter "label=awd.event_id=$EVENT_ID" | wc -l | tr -d ' ')"
[[ "$REMAIN_CONTAINERS" == "0" ]] || fail "archive left $REMAIN_CONTAINERS event containers"
if docker network inspect "$NETWORK_NAME" >/dev/null 2>&1; then fail "archive left Docker network $NETWORK_NAME"; fi
if ip link show "$WG_INTERFACE" >/dev/null 2>&1; then fail "archive left WireGuard interface $WG_INTERFACE"; fi
# Runtime-resource rows are an observation/audit trail by design and have no lifecycle
# status column.  Archive must preserve those rows while removing the physical resources.
RUNTIME_ROWS="$(sql "SELECT count(*) FROM awd_runtime_resources WHERE event_id='$EVENT_ID'")"
[[ "$RUNTIME_ROWS" -ge 3 ]] || fail "archive unexpectedly lost runtime audit rows ($RUNTIME_ROWS)"
helper_nft_table_is_absent floatctf_awd || fail "archive left nft table floatctf_awd"
pass "archive removed event containers + Docker network + WireGuard + nftables; runtime audit rows preserved ($RUNTIME_ROWS)"

log "exercise admin team-management invariants after archive"
# Generic event_users mutations are invalid for team-mode events; they would create
# or destroy enrollment independently from membership.  Verify both are rejected.
BLUE_MEMBER_ID="$(sql "SELECT etm.user_id FROM event_team_members etm WHERE etm.event_id='$EVENT_ID' AND etm.team_id='$BLUE_TEAM_ID' AND etm.role::text='member' LIMIT 1")"
[[ -n "$BLUE_MEMBER_ID" ]] || fail "could not resolve Blue member id"
api_expect_fail POST "/api/admin/events/$EVENT_ID/users" "$ADMIN_TOKEN" "{\"user_id\":\"$BLUE_MEMBER_ID\"}" \
    || fail "team event accepted generic admin user add"
api_expect_fail DELETE "/api/admin/events/$EVENT_ID/users" "$ADMIN_TOKEN" "{\"id_list\":[\"$BLUE_MEMBER_ID\"]}" \
    || fail "team event accepted generic admin user remove"
[[ "$(sql "SELECT count(*) FROM event_users WHERE event_id='$EVENT_ID'")" == "4" ]] \
    || fail "rejected generic admin user mutation changed enrollment"

# Add a fifth user through the canonical admin team endpoint, reject a second-team
# membership, then remove them again and assert both membership + event_users disappear.
ADMIN_MEMBER_USER="awd_adminmember_${SUFFIX//-/_}"
ADMIN_MEMBER_PASS="AdminMember-${SUFFIX}-Aa1!"
ADMIN_MEMBER_BODY="$(json_string_obj username "$ADMIN_MEMBER_USER" nickname adminmember password "$ADMIN_MEMBER_PASS" email "$ADMIN_MEMBER_USER@example.invalid")"
api_ok POST /api/users '' "$ADMIN_MEMBER_BODY" >/dev/null
ADMIN_MEMBER_ID="$(sql "SELECT id FROM users WHERE username='$ADMIN_MEMBER_USER'")"
[[ -n "$ADMIN_MEMBER_ID" ]] || fail "admin-management user was not created"
api_ok POST "/api/admin/events/$EVENT_ID/teams/$BLUE_TEAM_ID/users" "$ADMIN_TOKEN" "{\"user_id\":\"$ADMIN_MEMBER_ID\"}" >/dev/null
[[ "$(sql "SELECT count(*) FROM event_users WHERE event_id='$EVENT_ID' AND user_id='$ADMIN_MEMBER_ID'")" == "1" ]] \
    || fail "admin team add did not create enrollment"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND team_id='$BLUE_TEAM_ID' AND user_id='$ADMIN_MEMBER_ID'")" == "1" ]] \
    || fail "admin team add did not create membership"
api_expect_fail POST "/api/admin/events/$EVENT_ID/teams/$RED_TEAM_ID/users" "$ADMIN_TOKEN" "{\"user_id\":\"$ADMIN_MEMBER_ID\"}" \
    || fail "admin team endpoint allowed one user in two teams"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND user_id='$ADMIN_MEMBER_ID'")" == "1" ]] \
    || fail "failed second-team admin add left membership residue"
api_ok DELETE "/api/admin/events/$EVENT_ID/teams/$BLUE_TEAM_ID/users" "$ADMIN_TOKEN" "{\"id_list\":[\"$ADMIN_MEMBER_ID\"]}" >/dev/null
[[ "$(sql "SELECT count(*) FROM event_users WHERE event_id='$EVENT_ID' AND user_id='$ADMIN_MEMBER_ID'")" == "0" ]] \
    || fail "admin team member removal left event_users residue"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND user_id='$ADMIN_MEMBER_ID'")" == "0" ]] \
    || fail "admin team member removal left membership residue"

# Captain cannot be removed as an isolated member because that would create a
# captainless team.  Team deletion is the canonical captain-removal workflow.
BLUE_CAPTAIN_ID="$(sql "SELECT etm.user_id FROM event_team_members etm WHERE etm.event_id='$EVENT_ID' AND etm.team_id='$BLUE_TEAM_ID' AND etm.role::text='captain' LIMIT 1")"
api_expect_fail DELETE "/api/admin/events/$EVENT_ID/teams/$BLUE_TEAM_ID/users" "$ADMIN_TOKEN" "{\"id_list\":[\"$BLUE_CAPTAIN_ID\"]}" \
    || fail "admin endpoint allowed isolated captain removal"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND team_id='$BLUE_TEAM_ID' AND user_id='$BLUE_CAPTAIN_ID' AND role::text='captain'")" == "1" ]] \
    || fail "rejected captain removal changed captain membership"

# Regression for the old catastrophic bug: deleting Red used to delete ALL
# event_users in the event.  Now only Red enrollments may disappear; Blue must survive.
RED_USER_COUNT="$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND team_id='$RED_TEAM_ID'")"
BLUE_USER_COUNT="$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND team_id='$BLUE_TEAM_ID'")"
[[ "$RED_USER_COUNT" == "2" && "$BLUE_USER_COUNT" == "2" ]] || fail "unexpected team membership counts before admin delete"
api_ok DELETE "/api/admin/events/$EVENT_ID/teams" "$ADMIN_TOKEN" "{\"id_list\":[\"$RED_TEAM_ID\"]}" >/dev/null
[[ "$(sql "SELECT count(*) FROM event_teams WHERE event_id='$EVENT_ID' AND id='$RED_TEAM_ID'")" == "0" ]] || fail "Red team was not deleted"
[[ "$(sql "SELECT count(*) FROM event_users eu WHERE eu.event_id='$EVENT_ID' AND EXISTS (SELECT 1 FROM users u WHERE u.id=eu.user_id AND u.nickname IN ('redcap','redmember'))")" == "0" ]] \
    || fail "Red enrollments remained after team deletion"
[[ "$(sql "SELECT count(*) FROM event_teams WHERE event_id='$EVENT_ID' AND id='$BLUE_TEAM_ID'")" == "1" ]] || fail "deleting Red also deleted Blue team"
[[ "$(sql "SELECT count(*) FROM event_team_members WHERE event_id='$EVENT_ID' AND team_id='$BLUE_TEAM_ID'")" == "2" ]] || fail "deleting Red changed Blue membership"
[[ "$(sql "SELECT count(*) FROM event_users eu WHERE eu.event_id='$EVENT_ID' AND EXISTS (SELECT 1 FROM event_team_members etm WHERE etm.event_id=eu.event_id AND etm.user_id=eu.user_id AND etm.team_id='$BLUE_TEAM_ID')")" == "2" ]] \
    || fail "deleting Red wiped Blue event_users"
pass "admin team add/remove/duplicate/captain/delete invariants preserved; unrelated team enrollment survived"

printf '\n=== AWD BUSINESS E2E SUMMARY ===\n'
printf 'event: %s\n' "$EVENT_ID"
printf 'users: 4, teams: 2, rounds: 2\n'
printf 'round1 totals: Red=%s Blue=%s\n' "$R1_RED_SCORE" "$R1_BLUE_SCORE"
printf 'final totals:  Red=%s (#%s) Blue=%s (#%s)\n' "$RED_FINAL" "$RED_RANK" "$BLUE_FINAL" "$BLUE_RANK"
printf 'score ledger:\n%s\n' "$SCORE_COUNTS"
printf 'host runtime after archive: containers=0 network=0 wireguard=0 nftables=0\n'
printf 'RESULT: PASS\n'
