#!/usr/bin/env bash
set -Eeuo pipefail

# Real Jeopardy three-mode lifecycle E2E.
#
# Covers Practice, Individual Competition, and Team Competition against:
#   isolated PostgreSQL + real floatctf-helper Docker proxy + real Docker Engine.
# The test DB and fixture image are disposable. It does not touch floatctf_db data.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() { printf '[jeopardy-e2e] %s\n' "$*" >&2; }
pass() { printf '[jeopardy-e2e] PASS: %s\n' "$*" >&2; }
fail() { printf '[jeopardy-e2e] FAIL: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "missing command: $1"; }
for cmd in cargo createdb dropdb docker psql python3 systemctl; do need "$cmd"; done

[[ "$(systemctl is-active floatctf-helper.service 2>/dev/null || true)" == "active" ]] \
    || fail "floatctf-helper.service is not active"
[[ -S /run/floatctf/helper-docker.sock ]] || fail "missing helper Docker socket"

if docker ps -a --format '{{.Names}}' | grep -Eq '^(JP|JS|JT)-'; then
    fail "existing Jeopardy E2E-style containers found; refusing broad cleanup"
fi

SUFFIX="$(date +%s)-$$"
DB_NAME="floatctf_jeopardy_modes_e2e_${SUFFIX//-/_}"
TMP="$(mktemp -d "/tmp/floatctf-jeopardy-modes-e2e-${SUFFIX}.XXXXXX")"
CONFIG="$TMP/test.toml"
IMAGE="floatctf/jeopardy-modes-e2e:$SUFFIX"
TEST_LOG="$TMP/test.log"
DB_URL="postgres://postgres:postgres@127.0.0.1:5432/$DB_NAME"
CHALLENGE_ID="$(python3 -c 'import uuid; print(uuid.uuid4())')"
USER_A="$(python3 -c 'import uuid; print(uuid.uuid4())')"
USER_B="$(python3 -c 'import uuid; print(uuid.uuid4())')"

cleanup() {
    local rc=$?
    set +e
    # Only E2E-style names are eligible, and preflight guaranteed none existed before this run.
    mapfile -t containers < <(docker ps -aq --filter 'name=JP-' --filter 'name=JS-' --filter 'name=JT-' 2>/dev/null)
    if ((${#containers[@]})); then docker rm -f "${containers[@]}" >/dev/null 2>&1 || true; fi
    docker image rm -f "$IMAGE" >/dev/null 2>&1 || true
    PGPASSWORD=postgres dropdb -h 127.0.0.1 -U postgres --if-exists --force "$DB_NAME" >/dev/null 2>&1 || true
    if (( rc == 0 )); then
        rm -rf "$TMP"
        pass "isolated DB/image/runtime cleanup completed"
    else
        log "failure artifacts kept at $TMP"
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM

log "create isolated database"
PGPASSWORD=postgres createdb -h 127.0.0.1 -U postgres "$DB_NAME"
cp apps/api/config/development.toml "$CONFIG"
python3 - "$CONFIG" "$DB_URL" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1]); url=sys.argv[2]
lines=p.read_text().splitlines()
in_database=False
changed=False
for i,line in enumerate(lines):
    stripped=line.strip()
    if stripped.startswith('[') and stripped.endswith(']'):
        in_database = stripped == '[database]'
        continue
    if in_database and stripped.startswith('url') and '=' in line:
        indent=line[:len(line)-len(line.lstrip())]
        lines[i]=f'{indent}url = "{url}"'
        changed=True
        break
if not changed:
    raise SystemExit('failed to rewrite database.url')
p.write_text('\n'.join(lines)+'\n')
PY
FLOATCTF_CONFIG="$CONFIG" apps/api/src/sql/migrate.sh apply >/dev/null
migration_count="$(PGPASSWORD=postgres psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$DB_URL" -c 'SELECT count(*) FROM schema_migrations')"
[[ "$migration_count" == "45" ]] || fail "expected 45 migrations, got $migration_count"
pass "45 migrations applied"

log "build disposable managed challenge image"
mkdir -p "$TMP/image"
cat > "$TMP/image/Dockerfile" <<'DOCKERFILE'
FROM alpine:3.20
LABEL io.floatctf.managed="true"
EXPOSE 8080
CMD ["sleep", "infinity"]
DOCKERFILE
docker build --network=none -t "$IMAGE" "$TMP/image" >/dev/null
IMAGE_ID="$(docker image inspect "$IMAGE" --format '{{.Id}}')"
[[ "$IMAGE_ID" == sha256:* ]] || fail "invalid image id: $IMAGE_ID"
pass "fixture image built and pinned by image ID"

log "seed settings, users, and one ready dynamic Docker challenge"
PGPASSWORD=postgres psql -X -q -v ON_ERROR_STOP=1 -d "$DB_URL" \
    -v challenge_id="$CHALLENGE_ID" -v user_a="$USER_A" -v user_b="$USER_B" -v image_id="$IMAGE_ID" <<'SQL'
INSERT INTO settings (key, value, type, description)
VALUES
  ('INSTANCE_DESTROY_DELAY', '60', 'integer', 'Jeopardy E2E destroy delay'),
  ('EVENT_SCORE_DECAY', '500', 'integer', 'Jeopardy E2E score decay'),
  ('EVENT_SCORE_MIN_PERCENT', '0.45', 'float', 'Jeopardy E2E score floor'),
  ('WORK_DIR', '/tmp/floatctf-jeopardy-e2e', 'string', 'Jeopardy E2E work dir'),
  ('CHALLENGES_DIR', '{{WORK_DIR}}/challenges', 'string', 'Jeopardy E2E challenge dir'),
  ('GAMEBOXES_DIR', '{{WORK_DIR}}/gameboxes', 'string', 'Jeopardy E2E gamebox dir'),
  ('HTTP_PREFIX', 'http://', 'string', 'Jeopardy E2E HTTP prefix'),
  ('NODE_IP', '127.0.0.1', 'string', 'Jeopardy E2E node IP'),
  ('FLAG_PREFIX', 'flag', 'string', 'Jeopardy E2E flag prefix'),
  ('MAIN_URL', 'http://127.0.0.1:9090', 'string', 'Jeopardy E2E main URL'),
  ('SMTP_URI', 'smtp.example.test:test@example.test:test', 'string', 'Jeopardy E2E SMTP placeholder')
ON CONFLICT (key) DO NOTHING;

INSERT INTO users (id, username, nickname, password, email)
VALUES
  (:'user_a'::uuid, 'jeopardy-e2e-a', 'Jeopardy E2E A', 'x', 'jeopardy-e2e-a@example.test'),
  (:'user_b'::uuid, 'jeopardy-e2e-b', 'Jeopardy E2E B', 'x', 'jeopardy-e2e-b@example.test');

INSERT INTO challenges (
  id, name, safe_name, category, description, hidden,
  version, flag_type, container_port, image_ref, image_id, build_status,
  recommended_cpu_millis, recommended_memory_bytes, recommended_pids_limit
) VALUES (
  :'challenge_id'::uuid,
  'Jeopardy Modes E2E',
  'jeopardy-modes-e2e',
  'web',
  'real Docker lifecycle fixture',
  false,
  '1.0.0',
  'dynamic',
  8080,
  'floatctf/jeopardy-modes-e2e:e2e',
  :'image_id',
  'ready',
  100,
  67108864,
  32
);
SQL
pass "fixture rows seeded"

log "run all three ignored real lifecycle tests serially"
set +e
DATABASE_URL="$DB_URL" \
FLOATCTF_CONFIG="$CONFIG" \
FLOATCTF_TEST_DOCKER_SOCKET="/run/floatctf/helper-docker.sock" \
FLOATCTF_TEST_CHALLENGE_ID="$CHALLENGE_ID" \
FLOATCTF_TEST_USER_A="$USER_A" \
FLOATCTF_TEST_USER_B="$USER_B" \
cargo test -p floatctf --test jeopardy_modes_live -- --ignored --test-threads=1 \
    >"$TEST_LOG" 2>&1
rc=$?
set -e
cat "$TEST_LOG"
(( rc == 0 )) || fail "Jeopardy modes test binary exited $rc"

grep -q 'test practice_full_lifecycle_and_retraining ... ok' "$TEST_LOG" \
    || fail "practice lifecycle did not pass"
grep -q 'test individual_competition_full_lifecycle ... ok' "$TEST_LOG" \
    || fail "individual competition lifecycle did not pass"
grep -q 'test team_competition_shared_instance_full_lifecycle ... ok' "$TEST_LOG" \
    || fail "team competition lifecycle did not pass"
grep -q 'test result: ok. 3 passed; 0 failed' "$TEST_LOG" \
    || fail "unexpected test summary"

remaining="$(PGPASSWORD=postgres psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$DB_URL" -c "SELECT count(*) FROM event_instances WHERE runtime_state='running'")"
[[ "$remaining" == "0" ]] || fail "$remaining running instance rows remain"
if docker ps -a --format '{{.Names}}' | grep -Eq '^(JP|JS|JT)-'; then
    fail "Jeopardy runtime container residue remains"
fi

pass "Practice → retrain lifecycle"
pass "Individual competition → launch/solve/score lifecycle"
pass "Team competition → shared launch/destroy/relaunch/teammate solve lifecycle"
printf '%s\n' 'RESULT: PASS'
